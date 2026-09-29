//! Optional Vite per site, with HTTPS and WebSocket forwarding through Caddy.

use crate::model::{Project, ProjectStatus};
use crate::paths::project_dir;
use crate::process::{append_log, hidden_command, spawn_logged, ManagedChild};
use crate::runtimes;
use crate::settings::Settings;
use crate::state::State;
use anyhow::{bail, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) struct RunningVite {
    pub process: ManagedChild,
    pub node: String,
}

pub(crate) fn available(folder: &Path) -> bool {
    let package: Value = fs::read(folder.join("package.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    ["vite", "laravel-vite-plugin"].iter().all(|name| {
        ["dependencies", "devDependencies"]
            .iter()
            .any(|section| package[section][name].is_string())
    })
}

fn record(root: &Path, project: &Project) -> PathBuf {
    project_dir(root, &project.id).join("vite-hot.json")
}

/// Only remove a hot file whose exact content still matches our recorded one.
/// Also called at startup after the previous daemon's children have stopped.
pub(crate) fn cleanup_hot(root: &Path, project: &Project) {
    let marker = record(root, project);
    let saved: Option<Value> = fs::read(&marker)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if let Some(expected) = saved.as_ref().and_then(|saved| saved["value"].as_str()) {
        let hot = Path::new(&project.path).join("public/hot");
        if fs::read_to_string(&hot).ok().as_deref() == Some(expected) {
            let _ = fs::remove_file(hot);
        }
    }
    let _ = fs::remove_file(marker);
}

/// Same project-file precedence as Werd's Node shim.
fn node_line(root: &Path, project: &Project) -> Result<String> {
    for folder in Path::new(&project.path).ancestors() {
        let manifest = crate::manifest::Manifest::load(folder)?;
        if let Some(line) = manifest.node {
            return runtimes::resolve_line(root, "node", Some(&line), "Node.js");
        }
        for file in [".nvmrc", ".node-version"] {
            if let Ok(value) = fs::read_to_string(folder.join(file)) {
                let major = value
                    .trim()
                    .trim_start_matches('v')
                    .split('.')
                    .next()
                    .unwrap_or_default();
                if !major.is_empty() && major.bytes().all(|byte| byte.is_ascii_digit()) {
                    return runtimes::resolve_line(root, "node", Some(major), "Node.js");
                }
            }
        }
    }
    let settings = Settings::load(root)?;
    runtimes::resolve_line(
        root,
        "node",
        project.node.as_deref().or(settings.default_node.as_deref()),
        "Node.js",
    )
}

fn launch(root: &Path, state: &mut State, id: &str) -> Result<()> {
    let index = state.index(id)?;
    if !state.processes.contains_key(id) {
        bail!("Start the site before starting Vite");
    }
    if state.vite_processes.contains_key(id) {
        return Ok(());
    }
    let project = state.projects[index].clone();
    let folder = Path::new(&project.path);
    if !available(folder) {
        bail!("This site does not declare Vite and laravel-vite-plugin in package.json");
    }
    if !folder.join("node_modules/vite/package.json").is_file() {
        bail!("Vite dependencies are missing. Run npm install in the site folder first");
    }
    if folder.join("public/hot").exists() {
        bail!("public/hot already exists. Stop your other Vite server; remove the file only if it is stale");
    }
    let line = node_line(root, &project)?;
    let node_dir = runtimes::line_dir(root, "node", &line);
    let node_bin = if cfg!(windows) {
        node_dir.clone()
    } else {
        node_dir.join("bin")
    };
    let mut ports = project.ports.clone().unwrap_or_default();
    let port = crate::ports::assign(&mut ports, "vite")?;
    let https = crate::ports::assign(&mut ports, "vite_https")?;
    crate::ports::ensure_available(&crate::model::Ports::from([
        ("vite".into(), port),
        ("vite_https".into(), https),
    ]))?;
    state.projects[index].ports = Some(ports);
    let directory = project_dir(root, id);
    fs::create_dir_all(&directory)?;
    let runner = directory.join("vite-runner.mjs");
    fs::write(&runner, include_str!("vite-runner.mjs"))?;
    let marker = record(root, &project);
    let _ = fs::remove_file(&marker);
    let mut origins = vec![format!(
        "https://localhost:{}",
        crate::router::port(&project, "site")?
    )];
    if let Some(url) = &project.url {
        origins.push(url.clone());
    }
    // Covers domain edits and the configured HTTPS port without restarting Vite.
    if let Some(domain) = &project.domain {
        let https_port = Settings::load(root)?.https_port;
        origins.push(if https_port == 443 {
            format!("https://{domain}")
        } else {
            format!("https://{domain}:{https_port}")
        });
    }
    let mut command = hidden_command(node_bin.join(runtimes::exe("node")));
    let paths = std::iter::once(node_bin)
        .chain(std::iter::once(runtimes::line_dir(root, "php", &project.php)))
        .chain(std::iter::once(crate::shims::bin_dir(root)))
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ))
        .collect::<Vec<_>>();
    command
        .arg(&runner)
        .arg(folder)
        .arg(port.to_string())
        .arg(https.to_string())
        .arg(serde_json::to_string(&origins)?)
        .arg(&marker)
        .current_dir(folder)
        .env("PATH", std::env::join_paths(paths)?)
        .env("WERD_HOME", root)
        .env("WERD_NODE", &line)
        .env("WERD_PHP", &project.php)
        .env("NO_COLOR", "1");
    let mut process = spawn_logged(&directory, "vite", command)?;
    let ready = (|| {
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline {
            if let Some(exit) = process.has_exited() {
                bail!("Vite exited during startup ({exit}); see the Vite log");
            }
            let ready: Option<Value> = fs::read(&marker)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            let origin = format!("https://localhost:{https}");
            if ready
                .as_ref()
                .and_then(|ready| ready["value"].as_str())
                .is_some_and(|value| value == origin || value.starts_with(&format!("{origin}/")))
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        bail!("Vite did not become ready; see the Vite log")
    })();
    if let Err(error) = ready {
        process.kill();
        cleanup_hot(root, &project);
        return Err(error);
    }
    state
        .vite_processes
        .insert(id.into(), RunningVite { process, node: line });
    if let Err(error) = state
        .sync_router(root)
        .and_then(|()| state.router.wait_for(https))
    {
        stop_process(root, state, id);
        let _ = state.sync_router(root);
        return Err(error);
    }
    let vite = &mut state.projects[index].vite;
    vite.available = true;
    vite.status = ProjectStatus::Running;
    vite.error = None;
    vite.url = Some(format!("https://localhost:{https}"));
    append_log(root, id, &format!("Vite started: https://localhost:{https}"))?;
    Ok(())
}

pub(crate) fn start(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    state.project(id)?;
    if let Err(error) = launch(root, state, id) {
        if state.vite_processes.contains_key(id) {
            stop_process(root, state, id);
            let _ = state.sync_router(root);
        }
        let index = state.index(id)?;
        state.projects[index].vite.status = ProjectStatus::Error;
        state.projects[index].vite.error = Some(format!("{error:#}"));
        state.projects[index].vite.url = None;
        let _ = append_log(root, id, &format!("Vite failed: {error:#}"));
        let _ = state.save(root);
        return Err(error);
    }
    state.save(root)?;
    Ok(state.project(id)?.clone())
}

pub(crate) fn stop_process(root: &Path, state: &mut State, id: &str) {
    if let Some(mut running) = state.vite_processes.remove(id) {
        running.process.kill();
        if let Ok(project) = state.project(id) {
            cleanup_hot(root, project);
        }
    }
    if let Ok(index) = state.index(id) {
        state.projects[index].vite.status = ProjectStatus::Stopped;
        state.projects[index].vite.url = None;
        state.projects[index].vite.error = None;
    }
}

pub(crate) fn stop(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    state.project(id)?;
    let https = state
        .vite_processes
        .contains_key(id)
        .then(|| crate::router::port(state.project(id)?, "vite_https"))
        .transpose()?;
    stop_process(root, state, id);
    state.sync_router(root)?;
    append_log(root, id, "Vite stopped")?;
    state.save(root)?;
    if let Some(port) = https {
        crate::ports::wait_until_available(port)?;
    }
    Ok(state.project(id)?.clone())
}

pub(crate) fn set_autostart(root: &Path, state: &mut State, id: &str, enabled: bool) -> Result<Project> {
    let index = state.index(id)?;
    state.projects[index].vite.autostart = enabled;
    state.save(root)?;
    Ok(state.projects[index].clone())
}

pub(crate) fn reap_exited(root: &Path, state: &mut State) {
    let failed: Vec<_> = state
        .vite_processes
        .iter_mut()
        .filter_map(|(id, running)| running.process.has_exited().map(|exit| (id.clone(), exit)))
        .collect();
    if failed.is_empty() {
        return;
    }
    let mut released = Vec::new();
    for (id, exit) in failed {
        if let Ok(port) = state
            .project(&id)
            .and_then(|project| crate::router::port(project, "vite_https"))
        {
            released.push(port);
        }
        stop_process(root, state, &id);
        if let Ok(index) = state.index(&id) {
            state.projects[index].vite.status = ProjectStatus::Error;
            state.projects[index].vite.error = Some(format!("Vite exited: {exit}; see the Vite log"));
        }
        let _ = append_log(root, &id, &format!("Vite exited: {exit}"));
    }
    let _ = state.sync_router(root);
    for port in released {
        let _ = crate::ports::wait_until_available(port);
    }
    let _ = state.save(root);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtimes::Installed;
    use serde_json::json;

    fn project(path: &Path) -> Project {
        serde_json::from_value(
            json!({ "id": "shop", "name": "shop", "path": path, "php": "8.4", "node": "20" }),
        )
        .unwrap()
    }

    #[test]
    fn hot_cleanup_only_removes_the_recorded_content() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let site = project(work.path());
        fs::create_dir_all(work.path().join("public")).unwrap();
        fs::create_dir_all(project_dir(root.path(), &site.id)).unwrap();
        let hot = work.path().join("public/hot");
        let saved = record(root.path(), &site);
        let owned = "https://localhost:8445";
        fs::write(&hot, owned).unwrap();
        fs::write(&saved, json!({ "value": owned }).to_string()).unwrap();
        cleanup_hot(root.path(), &site);
        assert!(!hot.exists());
        assert!(!saved.exists());
        fs::write(&hot, "http://localhost:5173").unwrap();
        fs::write(&saved, json!({ "value": owned }).to_string()).unwrap();
        cleanup_hot(root.path(), &site);
        assert_eq!(fs::read_to_string(&hot).unwrap(), "http://localhost:5173");
    }

    #[test]
    fn node_respects_project_files_then_site_then_default() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut site = project(work.path());
        let mut installed = Installed::default();
        for line in ["20", "22", "24"] {
            installed.set("node", line, &format!("{line}.0.0"));
        }
        installed.save(root.path()).unwrap();
        let settings = Settings {
            default_node: Some("24".into()),
            ..Default::default()
        };
        settings.save(root.path()).unwrap();
        assert_eq!(node_line(root.path(), &site).unwrap(), "20");
        site.node = None;
        assert_eq!(node_line(root.path(), &site).unwrap(), "24");
        fs::write(work.path().join(".nvmrc"), "v22.1.0\n").unwrap();
        assert_eq!(node_line(root.path(), &site).unwrap(), "22");
        fs::write(work.path().join("werd.yml"), "node: 20\n").unwrap();
        assert_eq!(node_line(root.path(), &site).unwrap(), "20");
        fs::write(work.path().join("werd.yml"), "node: 18\n").unwrap();
        assert!(node_line(root.path(), &site)
            .unwrap_err()
            .to_string()
            .contains("18 is not installed"));
    }

    #[test]
    fn failed_frontend_start_leaves_php_running_and_persists_autostart() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut site = project(work.path());
        site.status = ProjectStatus::Running;
        let mut state = State {
            projects: vec![site],
            ..Default::default()
        };
        state.processes.insert("shop".into(), vec![]);
        set_autostart(root.path(), &mut state, "shop", true).unwrap();
        assert!(start(root.path(), &mut state, "shop").is_err());
        assert_eq!(state.projects[0].status, ProjectStatus::Running);
        assert_eq!(state.projects[0].vite.status, ProjectStatus::Error);
        assert!(state.processes.contains_key("shop"));
        let loaded = State::load(root.path()).unwrap();
        assert!(loaded.projects[0].vite.autostart);
        assert_eq!(loaded.projects[0].vite.status, ProjectStatus::Stopped);
        assert!(loaded.projects[0].vite.error.is_none());
    }
}
