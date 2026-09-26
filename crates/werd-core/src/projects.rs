//! Site lifecycle: link, configure, start, stop and inspect.
//!
//! A site runs its own PHP FastCGI behind the shared Caddy of `router`.
//! Databases, caches, mail and storage come from shared service instances
//! the site is linked to.

use crate::instances::{self, database_name};
use crate::manifest::Manifest;
use crate::model::{Link, Project, ProjectStatus, Requirement, CATEGORIES};
use crate::paths::{caddy_data, project_dir};
use crate::platform;
use crate::ports;
use crate::process::append_log;
use crate::proxy;
use crate::router;
use crate::runtimes::Installed;
use crate::settings::Settings;
use crate::state::State;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// Database (or bucket) name for a site in categories that have one.
fn default_database(category: &str, site: &str) -> Option<String> {
    matches!(category, "database" | "storage")
        .then(|| database_name(site).ok())
        .flatten()
}

/// PHP line for a new site: `werd.yml`, then the default, then the newest installed, then 8.5.
fn initial_php(root: &Path, requested: Option<String>) -> String {
    requested
        .or_else(|| {
            Settings::load(root)
                .ok()
                .and_then(|settings| settings.default_php)
        })
        .or_else(|| {
            Installed::load(root)
                .ok()
                .and_then(|installed| installed.lines("php").into_iter().next())
        })
        .unwrap_or_else(|| "8.5".into())
}

/// Links pending requirements to existing instances that satisfy them.
pub(crate) fn auto_link(state: &mut State, index: usize) {
    let instances = &state.instances.list;
    let project = &mut state.projects[index];
    let mut pending = Vec::new();
    for requirement in std::mem::take(&mut project.requirements) {
        let found = instances.iter().find(|instance| {
            instance.product == requirement.product
                && requirement
                    .line
                    .as_ref()
                    .is_none_or(|line| *line == instance.line)
                && requirement
                    .extensions
                    .iter()
                    .all(|extension| instance.extensions.contains(extension))
        });
        match found {
            Some(instance) if !project.links.contains_key(&requirement.category) => {
                project.links.insert(
                    requirement.category.clone(),
                    Link {
                        instance: instance.id.clone(),
                        database: default_database(&requirement.category, &project.name),
                    },
                );
            }
            Some(_) => {}
            None => pending.push(requirement),
        }
    }
    project.requirements = pending;
}

/// Links a Laravel project folder. `werd.yml` is read when present, never written.
pub(crate) fn add(root: &Path, state: &mut State, path: &str) -> Result<Project> {
    add_with(root, state, path, None, None)
}

/// Like [`add`], with the site name (and so its domain) and PHP line chosen
/// by the user instead of the folder name and `werd.yml`.
pub(crate) fn add_with(
    root: &Path,
    state: &mut State,
    path: &str,
    name: Option<&str>,
    php: Option<&str>,
) -> Result<Project> {
    let display_path = crate::parks::display_path(Path::new(path))?;
    let canonical = Path::new(&display_path);
    if !canonical.is_dir() {
        bail!("{path} is not a folder");
    }
    if !crate::parks::is_laravel(canonical) {
        bail!("This folder does not look like a Laravel project (artisan and composer.json are missing)");
    }
    if state.projects.iter().any(|project| project.path == display_path) {
        bail!("This project is already linked to Werd");
    }
    let manifest = Manifest::load(canonical)?;
    let name = match name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => canonical
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    };
    let domain = state.unique_domain(&name, None);
    // Ports are picked now so the site address is known before the first start.
    let mut site_ports = crate::model::Ports::new();
    ports::assign(&mut site_ports, "site")?;
    ports::assign(&mut site_ports, "fastcgi")?;
    state.projects.push(Project {
        id: Uuid::new_v4().to_string(),
        name,
        domain: Some(domain),
        parked: None,
        path: display_path,
        php: initial_php(root, php.map(str::to_string).or(manifest.php)),
        node: manifest.node,
        links: Default::default(),
        requirements: manifest.requirements,
        status: ProjectStatus::Stopped,
        url: None,
        error: None,
        ports: Some(site_ports),
        services: Vec::new(),
        versions: Default::default(),
        extensions: Vec::new(),
    });
    let index = state.projects.len() - 1;
    auto_link(state, index);
    state.save(root)?;
    Ok(state.projects[index].clone())
}

/// Stops a site and forgets it. The project folder and linked services are untouched.
pub(crate) fn remove(root: &Path, state: &mut State, id: &str) -> Result<()> {
    if let Some(folder) = &state.project(id)?.parked {
        bail!("This site comes from the parked folder {folder}; unpark the folder or move the project out of it");
    }
    remove_parked(root, state, id)
}

/// Forgets a site without the parked-folder check; used by the parked-folder scan.
pub(crate) fn remove_parked(root: &Path, state: &mut State, id: &str) -> Result<()> {
    let index = state.index(id)?;
    let was_running = stop_processes(state, id);
    state.projects.remove(index);
    if was_running {
        state.sync_router(root)?;
    }
    state.save(root)?;
    let _ = fs::remove_dir_all(project_dir(root, id));
    Ok(())
}

/// Starts linked services (and the site database in them), then PHP, then
/// adds the site to the router.
pub(crate) fn start(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.index(id)?;
    if state.projects[index].status == ProjectStatus::Running {
        return Ok(state.projects[index].clone());
    }
    let project = state.projects[index].clone();
    if !project.requirements.is_empty() {
        let missing: Vec<String> = project
            .requirements
            .iter()
            .map(|requirement| format!("{} ({})", requirement.category, requirement.product))
            .collect();
        bail!(
            "{} needs services that are not set up yet: {}",
            project.name,
            missing.join(", ")
        );
    }
    for (category, link) in &project.links {
        if !state.instances.processes.contains_key(&link.instance) {
            instances::start(root, &mut state.instances, &link.instance)
                .with_context(|| format!("Starting the {category} service failed"))?;
        }
        if let (Some(database), true) = (&link.database, category == "database") {
            instances::create_database(root, &state.instances, &link.instance, database)?;
        }
    }

    let mut ports = project.ports.clone().unwrap_or_default();
    ports.retain(|role, _| role == "site" || role == "fastcgi");
    ports::ensure_available(&ports)?;
    let mut children = Vec::new();
    if let Err(error) = proxy::start_php(
        root,
        id,
        Path::new(&project.path),
        &project.php,
        &mut ports,
        &mut children,
    ) {
        for child in children.iter_mut().rev() {
            child.kill();
        }
        return Err(error);
    }
    let site_port = ports["site"];
    state.processes.insert(id.into(), children);
    state.projects[index].ports = Some(ports);
    let served = state
        .sync_router(root)
        .and_then(|()| state.router.wait_for(site_port));
    if let Err(error) = served {
        stop_processes(state, id);
        let _ = state.sync_router(root);
        return Err(error);
    }

    let project = &mut state.projects[index];
    project.status = ProjectStatus::Running;
    project.error = None;
    let url = project.url.clone().unwrap_or_default();
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

/// Stops PHP of a site; returns whether it was running. Callers then sync the
/// router. Shared services keep running.
pub(crate) fn stop_processes(state: &mut State, id: &str) -> bool {
    let Some(mut children) = state.processes.remove(id) else {
        return false;
    };
    for child in children.iter_mut().rev() {
        child.kill();
    }
    true
}

pub(crate) fn stop(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.index(id)?;
    if stop_processes(state, id) {
        state.sync_router(root)?;
    }
    let project = &mut state.projects[index];
    project.status = ProjectStatus::Stopped;
    project.url = None;
    let updated = project.clone();
    append_log(root, id, "Site stopped")?;
    state.save(root)?;
    Ok(updated)
}

/// Forgets the site's ports so new ones are picked on the next start.
pub(crate) fn reset_ports(root: &Path, state: &mut State, id: &str) -> Result<Project> {
    let index = state.index(id)?;
    let project = &mut state.projects[index];
    if project.status == ProjectStatus::Running {
        bail!("Stop the site first");
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

/// Changes the PHP line (applied on the next start) or the Node major of a site.
pub(crate) fn set_runtime(
    root: &Path,
    state: &mut State,
    id: &str,
    product: &str,
    line: Option<&str>,
) -> Result<Project> {
    let index = state.index(id)?;
    if let Some(line) = line {
        if Installed::load(root)?.version(product, line).is_none() {
            bail!("Install {product} {line} first");
        }
    }
    let project = &mut state.projects[index];
    match product {
        "php" => project.php = line.context("A site always needs a PHP version")?.into(),
        "node" => project.node = line.map(str::to_string),
        other => bail!("Sites cannot choose a {other} version"),
    }
    let updated = project.clone();
    state.save(root)?;
    Ok(updated)
}

/// Changes the `.test` domain of a site; a running site is served on it at once.
pub(crate) fn set_domain(root: &Path, state: &mut State, id: &str, input: &str) -> Result<Project> {
    let index = state.index(id)?;
    let domain = crate::domains::normalize(input)?;
    if state
        .projects
        .iter()
        .any(|project| project.id != id && project.domain.as_deref() == Some(domain.as_str()))
    {
        bail!("{domain} is already used by another site");
    }
    state.projects[index].domain = Some(domain);
    if state.processes.contains_key(id) {
        state.sync_router(root)?;
    }
    state.save(root)?;
    Ok(state.projects[index].clone())
}

/// Every site domain, sorted; these belong in the hosts file.
pub(crate) fn domains(state: &State) -> Vec<String> {
    let mut domains: Vec<String> = state
        .projects
        .iter()
        .filter_map(|project| project.domain.clone())
        .collect();
    domains.sort();
    domains
}

/// The address a site has (or will have once started).
pub(crate) fn site_url(root: &Path, project: &Project) -> Option<String> {
    if let Some(url) = &project.url {
        return Some(url.clone());
    }
    let settings = Settings::load(root).unwrap_or_default();
    match (&project.domain, settings.domains) {
        (Some(domain), true) if settings.https_port == 443 => Some(format!("https://{domain}")),
        (Some(domain), true) => Some(format!("https://{domain}:{}", settings.https_port)),
        _ => router::port(project, "site")
            .ok()
            .map(|port| format!("https://localhost:{port}")),
    }
}

/// Links (or relinks) a category of a site to a service instance.
pub(crate) fn link(
    root: &Path,
    state: &mut State,
    id: &str,
    category: &str,
    instance: &str,
    database: Option<&str>,
) -> Result<Project> {
    if !CATEGORIES.contains(&category) {
        bail!("Unknown category {category}");
    }
    let service = state.instances.get(instance)?.id.clone();
    let index = state.index(id)?;
    let project = &mut state.projects[index];
    let database = match database {
        Some(name) => Some(database_name(name)?),
        None => default_database(category, &project.name),
    };
    project.links.insert(
        category.into(),
        Link {
            instance: service,
            database,
        },
    );
    project
        .requirements
        .retain(|requirement| requirement.category != category);
    let updated = project.clone();
    state.save(root)?;
    Ok(updated)
}

pub(crate) fn unlink(root: &Path, state: &mut State, id: &str, category: &str) -> Result<Project> {
    let index = state.index(id)?;
    state.projects[index].links.remove(category);
    let updated = state.projects[index].clone();
    state.save(root)?;
    Ok(updated)
}

pub(crate) fn open(state: &State, id: &str) -> Result<String> {
    let url = state.project(id)?.url.clone().context("Start the site first")?;
    platform::open_url(&url)?;
    Ok(url)
}

/// The `.env` block for a site: its URL plus every linked service.
pub(crate) fn env(root: &Path, state: &State, id: &str) -> Result<String> {
    let project = state.project(id)?;
    let mut lines: Vec<String> = site_url(root, project)
        .iter()
        .map(|url| format!("APP_URL={url}"))
        .collect();
    for category in CATEGORIES {
        let Some(link) = project.links.get(category) else {
            continue;
        };
        let Ok(instance) = state.instances.get(&link.instance) else {
            continue;
        };
        for line in instances::env_for(root, instance, link.database.as_deref())? {
            let key = line.split('=').next().unwrap_or_default();
            if !lines
                .iter()
                .any(|existing| existing.split('=').next() == Some(key))
            {
                lines.push(line);
            }
        }
        match (category, instance.product.as_str()) {
            ("cache", "redis") => lines.push("CACHE_STORE=redis".into()),
            ("queue", "redis") => lines.push("QUEUE_CONNECTION=redis".into()),
            _ => {}
        }
    }
    Ok(lines.join("\n"))
}

/// Names of running sites that use a runtime line; such lines cannot be updated or removed.
pub(crate) fn using_runtime(state: &State, product: &str, line: &str) -> Vec<String> {
    state
        .projects
        .iter()
        .filter(|project| state.processes.contains_key(&project.id))
        .filter(|project| match product {
            "php" => project.php == line,
            // One Caddy and one CA bundle serve every running site.
            "caddy" | "cacert" => true,
            _ => false,
        })
        .map(|project| project.name.clone())
        .collect()
}

pub(crate) fn trust_local_ca(root: &Path) -> Result<String> {
    let certificate = caddy_data(root).join("caddy/pki/authorities/local/root.crt");
    if !certificate.is_file() {
        bail!("Start a site once before trusting the local certificate");
    }
    platform::trust_certificate(&certificate)
}

/// Services a site asks for that are not linked yet.
pub(crate) fn pending(state: &State, id: &str) -> Result<Vec<Requirement>> {
    Ok(state.project(id)?.requirements.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::ServiceInstance;
    use crate::model::Ports;

    fn laravel_folder(parent: &Path, name: &str, manifest: Option<&str>) -> String {
        let folder = parent.join(name);
        fs::create_dir_all(folder.join("public")).unwrap();
        fs::write(folder.join("artisan"), "").unwrap();
        fs::write(folder.join("composer.json"), "{}").unwrap();
        if let Some(manifest) = manifest {
            fs::write(folder.join("werd.yml"), manifest).unwrap();
        }
        folder.to_string_lossy().into_owned()
    }

    fn instance(id: &str, product: &str, line: &str, extensions: &[&str]) -> ServiceInstance {
        ServiceInstance {
            id: id.into(),
            name: format!("{product} {line}"),
            product: product.into(),
            line: line.into(),
            port: 1,
            extra_ports: Ports::new(),
            autostart: false,
            extensions: extensions.iter().map(|name| (*name).to_string()).collect(),
            status: ProjectStatus::Stopped,
            error: None,
        }
    }

    #[test]
    fn add_links_matching_instances_and_keeps_the_rest_pending() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        state
            .instances
            .list
            .push(instance("pg16", "postgresql", "16", &[]));
        state
            .instances
            .list
            .push(instance("pg18", "postgresql", "18", &["pgvector"]));
        state.instances.list.push(instance("redis", "redis", "8.2", &[]));
        let folder = laravel_folder(
            work.path(),
            "My Shop",
            Some("version: 2\nphp: '8.3'\nservices:\n  database: { product: postgresql, version: '18', extensions: [pgvector] }\n  cache: redis\n  mail: true\n"),
        );

        let project = add(root.path(), &mut state, &folder).unwrap();
        assert_eq!(project.php, "8.3");
        assert_eq!(
            project.links["database"],
            Link {
                instance: "pg18".into(),
                database: Some("my_shop".into())
            }
        );
        assert_eq!(project.links["cache"].instance, "redis");
        assert_eq!(project.requirements.len(), 1, "no Mailpit instance exists yet");
        assert_eq!(project.requirements[0].product, "mailpit");

        assert!(add(root.path(), &mut state, &folder)
            .unwrap_err()
            .to_string()
            .contains("already linked"));
    }

    #[test]
    fn sites_get_unique_test_domains() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        fs::create_dir_all(work.path().join("a")).unwrap();
        let first = add(
            root.path(),
            &mut state,
            &laravel_folder(work.path(), "My Shop", None),
        )
        .unwrap();
        let second = add(
            root.path(),
            &mut state,
            &laravel_folder(&work.path().join("a"), "My Shop", None),
        )
        .unwrap();
        assert_eq!(first.domain.as_deref(), Some("my-shop.test"));
        assert_eq!(second.domain.as_deref(), Some("my-shop-2.test"));

        let error = set_domain(root.path(), &mut state, &second.id, "my-shop").unwrap_err();
        assert!(error.to_string().contains("already used"), "{error}");
        assert!(set_domain(root.path(), &mut state, &second.id, "my shop").is_err());
        let renamed = set_domain(root.path(), &mut state, &second.id, "Store").unwrap();
        assert_eq!(renamed.domain.as_deref(), Some("store.test"));
        assert_eq!(domains(&state), ["my-shop.test", "store.test"]);
        assert!(env(root.path(), &state, &first.id)
            .unwrap()
            .contains("APP_URL=https://my-shop.test"));
    }

    #[test]
    fn sites_without_werd_yml_use_the_default_php_and_no_services() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        Settings {
            default_php: Some("8.4".into()),
            ..Settings::default()
        }
        .save(root.path())
        .unwrap();
        let mut state = State::default();
        let folder = laravel_folder(work.path(), "blog", None);
        let project = add(root.path(), &mut state, &folder).unwrap();
        assert_eq!(project.php, "8.4");
        assert!(project.links.is_empty() && project.requirements.is_empty());
        assert!(
            !Path::new(&folder).join("werd.yml").exists(),
            "werd.yml is never created"
        );
    }

    #[test]
    fn start_explains_missing_services_and_runtimes() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let folder = laravel_folder(work.path(), "api", Some("services:\n  mail: true\n"));
        let project = add(root.path(), &mut state, &folder).unwrap();
        let error = start(root.path(), &mut state, &project.id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not set up yet: mail (mailpit)"), "{error}");

        state.projects[0].requirements.clear();
        let error = start(root.path(), &mut state, &project.id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("PHP 8.5 is not installed"), "{error}");
        assert!(state.processes.is_empty());
    }

    #[test]
    fn link_env_runtime_changes_and_removal() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let mut redis = instance("r1", "redis", "7.2", &[]);
        redis.port = 6390;
        state.instances.list.push(redis);
        let project = add(root.path(), &mut state, &laravel_folder(work.path(), "api", None)).unwrap();

        link(root.path(), &mut state, &project.id, "cache", "r1", None).unwrap();
        link(root.path(), &mut state, &project.id, "queue", "r1", None).unwrap();
        let env = env(root.path(), &state, &project.id).unwrap();
        assert_eq!(
            env.matches("REDIS_PORT=6390").count(),
            1,
            "shared keys appear once:\n{env}"
        );
        assert!(env.contains("CACHE_STORE=redis") && env.contains("QUEUE_CONNECTION=redis"));

        assert!(link(root.path(), &mut state, &project.id, "cron", "r1", None).is_err());
        assert!(!unlink(root.path(), &mut state, &project.id, "queue")
            .unwrap()
            .links
            .contains_key("queue"));

        let error = set_runtime(root.path(), &mut state, &project.id, "php", Some("8.1")).unwrap_err();
        assert!(error.to_string().contains("Install php 8.1"));
        let mut installed = Installed::default();
        installed.set("node", "22", "22.1.0");
        installed.save(root.path()).unwrap();
        let updated = set_runtime(root.path(), &mut state, &project.id, "node", Some("22")).unwrap();
        assert_eq!(updated.node.as_deref(), Some("22"));

        remove(root.path(), &mut state, &project.id).unwrap();
        assert!(state.projects.is_empty());
    }
}
