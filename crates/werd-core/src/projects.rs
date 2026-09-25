//! Project lifecycle: link, start, stop and inspect.

use crate::manifest::Manifest;
use crate::model::{Project, ProjectStatus};
use crate::paths::caddy_data;
use crate::platform;
use crate::ports;
use crate::process::append_log;
use crate::proxy;
use crate::services::{self, ServiceContext};
use crate::state::State;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// Links a Laravel project folder, creating its `werd.yml` if needed.
pub(crate) fn add(root: &Path, state: &mut State, path: &str) -> Result<Project> {
    let canonical = fs::canonicalize(path).with_context(|| format!("Folder not found: {path}"))?;
    if !canonical.is_dir() {
        bail!("{path} is not a folder");
    }
    if !canonical.join("artisan").is_file() || !canonical.join("composer.json").is_file() {
        bail!("This folder does not look like a Laravel project (artisan and composer.json are missing)");
    }
    let display_path = canonical
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    if state.projects.iter().any(|project| project.path == display_path) {
        bail!("This project is already linked to Werd");
    }
    let manifest = Manifest::load_or_create(&canonical)?;
    let project = Project {
        id: Uuid::new_v4().to_string(),
        name: canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: display_path,
        php: manifest.php.clone(),
        services: manifest.services(),
        status: ProjectStatus::Stopped,
        url: None,
        error: None,
        ports: None,
    };
    state.projects.push(project.clone());
    state.save(root)?;
    Ok(project)
}

/// Starts the project's services, then PHP and Caddy. On any failure every
/// process started so far is stopped again.
pub(crate) fn start(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.index(id)?;
    if state.projects[index].status == ProjectStatus::Running {
        return Ok(state.projects[index].clone());
    }
    let project = state.projects[index].clone();
    let mut ports = project.ports.clone().unwrap_or_default();
    ports::ensure_available(&ports)?;

    let context = ServiceContext { root, project_id: id };
    let mut children = Vec::new();
    let started = services::start_all(&context, &project.services, &mut ports, &mut children)
        .and_then(|()| proxy::start_site(&context, Path::new(&project.path), &mut ports, &mut children));
    let url = match started {
        Ok(url) => url,
        Err(error) => {
            services::stop_all(&context, &mut children, &ports);
            return Err(error);
        }
    };

    state.processes.insert(id.into(), children);
    let project = &mut state.projects[index];
    project.status = ProjectStatus::Running;
    project.error = None;
    project.url = Some(url.clone());
    project.ports = Some(ports);
    let updated = project.clone();
    append_log(root, id, &format!("Site started: {url}"))?;
    state.save(root)?;
    Ok(updated)
}

/// Records a failed start so the UI can show it.
pub(crate) fn mark_failed(root: &Path, state: &mut State, id: &str, error: &anyhow::Error) {
    if let Ok(index) = state.index(id) {
        state.projects[index].status = ProjectStatus::Error;
        state.projects[index].error = Some(format!("{error:#}"));
    }
    let _ = append_log(root, id, &format!("Start failed: {error:#}"));
    let _ = state.save(root);
}

/// Stops the processes of a project without touching its data.
pub(crate) fn stop_processes(root: &Path, state: &mut State, id: &str) {
    if let Some(mut children) = state.processes.remove(id) {
        let ports = state
            .project(id)
            .ok()
            .and_then(|project| project.ports.clone())
            .unwrap_or_default();
        services::stop_all(&ServiceContext { root, project_id: id }, &mut children, &ports);
    }
}

pub(crate) fn stop(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.index(id)?;
    stop_processes(root, state, id);
    let project = &mut state.projects[index];
    project.status = ProjectStatus::Stopped;
    project.url = None;
    let updated = project.clone();
    append_log(root, id, "Project stopped")?;
    state.save(root)?;
    Ok(updated)
}

/// Forgets assigned ports so new ones are picked on the next start.
pub(crate) fn reset_ports(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.index(id)?;
    let project = &mut state.projects[index];
    if project.status == ProjectStatus::Running {
        bail!("Stop the project first");
    }
    project.ports = None;
    project.error = None;
    project.status = ProjectStatus::Stopped;
    let updated = project.clone();
    append_log(
        root,
        id,
        "Ports will be reassigned on next start; update the .env file",
    )?;
    state.save(root)?;
    Ok(updated)
}

pub(crate) fn open(state: &State, id: &str) -> Result<String> {
    let url = state
        .project(id)?
        .url
        .clone()
        .context("Start the project first")?;
    platform::open_url(&url)?;
    Ok(url)
}

pub(crate) fn env(root: &Path, state: &State, id: &str) -> Result<String> {
    let project = state.project(id)?;
    let ports = project
        .ports
        .as_ref()
        .context("Start the project first to know its ports")?;
    let lines = services::env_lines(&ServiceContext { root, project_id: id }, &project.services, ports)?;
    Ok(lines.join("\n"))
}

pub(crate) fn trust_local_ca(root: &Path) -> Result<String> {
    let certificate = caddy_data(root).join("caddy/pki/authorities/local/root.crt");
    if !certificate.is_file() {
        bail!("Start a site once before trusting the local certificate");
    }
    platform::trust_certificate(&certificate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ServiceName;

    fn laravel_folder(parent: &Path, name: &str) -> String {
        let folder = parent.join(name);
        fs::create_dir_all(folder.join("public")).unwrap();
        fs::write(folder.join("artisan"), "").unwrap();
        fs::write(folder.join("composer.json"), "{}").unwrap();
        folder.to_string_lossy().into_owned()
    }

    #[test]
    fn add_links_a_laravel_folder_once() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let folder = laravel_folder(work.path(), "shop");

        let project = add(root.path(), &mut state, &folder).unwrap();
        assert_eq!(project.name, "shop");
        assert_eq!(project.services, Manifest::default().services());
        assert!(Path::new(&folder).join("werd.yml").is_file());
        assert_eq!(State::load(root.path()).unwrap().projects.len(), 1);

        let error = add(root.path(), &mut state, &folder).unwrap_err().to_string();
        assert!(error.contains("already linked"));
    }

    #[test]
    fn add_rejects_non_laravel_folders() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let error = add(root.path(), &mut state, &work.path().to_string_lossy())
            .unwrap_err()
            .to_string();
        assert!(error.contains("Laravel"));
        assert!(add(root.path(), &mut state, "/definitely/missing/folder").is_err());
    }

    #[test]
    fn start_fails_cleanly_without_runtimes() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let folder = laravel_folder(work.path(), "blog");
        fs::write(
            Path::new(&folder).join("werd.yml"),
            "version: 1\nphp: '8.5'\nservices:\n  postgres: null\n  redis: null\n  mailpit: false\n  rustfs: false\n",
        )
        .unwrap();
        let project = add(root.path(), &mut state, &folder).unwrap();
        assert!(project.services.is_empty());

        let error = start(root.path(), &mut state, &project.id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("PHP 8.5 runtime is not installed"), "{error}");
        assert!(state.processes.is_empty());
        assert_eq!(state.project(&project.id).unwrap().status, ProjectStatus::Stopped);
    }

    #[test]
    fn reset_ports_and_env_require_the_right_state() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let project = add(root.path(), &mut state, &laravel_folder(work.path(), "api")).unwrap();

        assert!(env(root.path(), &state, &project.id)
            .unwrap_err()
            .to_string()
            .contains("Start the project"));

        state.projects[0].ports = Some(crate::model::Ports::from([("redis".to_string(), 7000)]));
        state.projects[0].services = vec![ServiceName::Redis];
        assert_eq!(
            env(root.path(), &state, &project.id).unwrap(),
            "REDIS_HOST=127.0.0.1\nREDIS_PORT=7000\nREDIS_PASSWORD=null"
        );

        state.projects[0].status = ProjectStatus::Running;
        assert!(reset_ports(root.path(), &mut state, &project.id).is_err());
        state.projects[0].status = ProjectStatus::Error;
        let reset = reset_ports(root.path(), &mut state, &project.id).unwrap();
        assert_eq!(reset.ports, None);
        assert_eq!(reset.status, ProjectStatus::Stopped);
    }
}
