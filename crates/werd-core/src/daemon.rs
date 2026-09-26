//! The `werd-daemon` process: owns the state, supervises children, serves RPC.

use crate::catalog::Catalog;
use crate::jobs::Jobs;
use crate::model::{ProjectStatus, Snapshot};
use crate::paths::home;
use crate::process::append_log;
use crate::rpc::{self, PROTOCOL_VERSION};
use crate::runtimes::{self, Fetcher, HttpFetcher, Installed};
use crate::settings::Settings;
use crate::state::State;
use crate::{doctor, migrations, process, projects, shims, VERSION};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MONITOR_INTERVAL: Duration = Duration::from_secs(2);

/// Everything a request needs besides the locked project state.
#[derive(Clone)]
pub(crate) struct Daemon {
    pub root: PathBuf,
    pub jobs: Jobs,
    pub fetcher: Arc<dyn Fetcher>,
    /// Fixed catalog for tests; the embedded or cached catalog otherwise.
    pub catalog: Option<Arc<Catalog>>,
}

impl Daemon {
    fn catalog(&self) -> Catalog {
        self.catalog
            .as_deref()
            .cloned()
            .unwrap_or_else(|| Catalog::load(&self.root))
    }
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params[key].as_str().with_context(|| format!("Missing {key}"))
}

fn id(params: &Value) -> Result<&str> {
    text(params, "id")
}

/// Starts an install or update job after checking that nothing running uses the line.
fn start_install(daemon: &Daemon, state: &State, params: &Value, update: bool) -> Result<Value> {
    let product = text(params, "product")?.to_string();
    let line = text(params, "line")?.to_string();
    let catalog = daemon.catalog();
    catalog.build(&product, &line)?;
    let installed = Installed::load(&daemon.root)?.version(&product, &line).is_some();
    if update && !installed {
        bail!("{product} {line} is not installed");
    }
    if installed {
        let users = projects::using_runtime(state, &product, &line);
        if !users.is_empty() {
            bail!("Stop {} before updating {product} {line}", users.join(", "));
        }
    }
    let (root, fetcher) = (daemon.root.clone(), Arc::clone(&daemon.fetcher));
    let (job_product, job_line) = (product.clone(), line.clone());
    let job = daemon.jobs.start(
        &product,
        &line,
        if update { "update" } else { "install" },
        move |progress| {
            runtimes::install(
                &root,
                &catalog,
                fetcher.as_ref(),
                &job_product,
                &job_line,
                progress,
            )
            .map(|_| ())
        },
    )?;
    Ok(json!(job))
}

fn set_default(root: &Path, params: &Value) -> Result<Value> {
    let product = text(params, "product")?;
    let line = text(params, "line")?;
    if Installed::load(root)?.version(product, line).is_none() {
        bail!("Install {product} {line} before making it the default");
    }
    let mut settings = Settings::load(root)?;
    match product {
        "php" => settings.default_php = Some(line.into()),
        "node" => settings.default_node = Some(line.into()),
        other => bail!("{other} has no default version"),
    }
    settings.save(root)?;
    Ok(json!(settings))
}

fn update_settings(root: &Path, params: &Value) -> Result<Value> {
    let mut settings = Settings::load(root)?;
    if let Some(value) = params.get("upload_max_mb") {
        settings.upload_max_mb =
            serde_json::from_value(value.clone()).context("upload_max_mb must be a number")?;
    }
    if let Some(value) = params.get("memory_limit_mb") {
        settings.memory_limit_mb =
            serde_json::from_value(value.clone()).context("memory_limit_mb must be a number")?;
    }
    settings.validate()?;
    settings.save(root)?;
    runtimes::write_all_php_ini(root, &settings)?;
    Ok(json!(settings))
}

/// Executes one RPC method. `sites.*` names and the 0.1 names are both accepted.
fn dispatch(daemon: &Daemon, state: &mut State, method: &str, params: &Value) -> Result<Value> {
    let root = daemon.root.as_path();
    let method = method.strip_prefix("sites.").unwrap_or(method);
    Ok(match method {
        "ping" => json!({ "version": VERSION, "protocol": PROTOCOL_VERSION }),
        "list" => json!(Snapshot {
            projects: state.projects.clone(),
            daemon_version: VERSION.into()
        }),
        "add" => json!(projects::add(root, state, text(params, "path")?)?),
        "start" => {
            let id = id(params)?;
            match projects::start(root, state, id) {
                Ok(project) => json!(project),
                Err(error) => {
                    projects::mark_failed(root, state, id, &error);
                    return Err(error);
                }
            }
        }
        "stop" => json!(projects::stop(root, state, id(params)?)?),
        "reset-ports" => json!(projects::reset_ports(root, state, id(params)?)?),
        "open" => json!(projects::open(state, id(params)?)?),
        "logs" => {
            let id = id(params)?;
            state.project(id)?;
            json!(process::read_log(
                root,
                id,
                params["service"].as_str().unwrap_or("werd")
            )?)
        }
        "env" => json!(projects::env(root, state, id(params)?)?),
        "doctor" => json!(doctor::run(root, &daemon.catalog())),
        "trust-ca" => json!(projects::trust_local_ca(root)?),

        "system.info" => {
            let catalog = daemon.catalog();
            let settings = Settings::load(root)?;
            json!({
                "version": VERSION,
                "protocol": PROTOCOL_VERSION,
                "home": root,
                "bin": shims::bin_dir(root),
                "platform": crate::catalog::PLATFORM,
                "catalog_generated": catalog.generated,
                "catalog_refreshable": settings.catalog_url.is_some(),
                "path_enabled": settings.path_enabled,
            })
        }
        "path.enable" => {
            shims::enable(root)?;
            json!(Settings::load(root)?)
        }
        "path.disable" => {
            shims::disable(root)?;
            json!(Settings::load(root)?)
        }
        "catalog.get" => {
            let catalog = daemon.catalog();
            json!({ "generated": catalog.generated, "platform": crate::catalog::PLATFORM,
                    "refreshable": Settings::load(root)?.catalog_url.is_some() })
        }
        "catalog.refresh" => {
            let url = Settings::load(root)?
                .catalog_url
                .context("Online catalog updates are not available yet; update Werd to get newer runtimes")?;
            let catalog = Catalog::refresh(root, &url)?;
            json!({ "generated": catalog.generated })
        }
        "runtimes" | "runtimes.list" => {
            json!(runtimes::list(root, &daemon.catalog(), &Settings::load(root)?)?)
        }
        "runtimes.install" => start_install(daemon, state, params, false)?,
        "runtimes.update" => start_install(daemon, state, params, true)?,
        "runtimes.uninstall" => {
            let (product, line) = (text(params, "product")?, text(params, "line")?);
            let users = projects::using_runtime(state, product, line);
            if !users.is_empty() {
                bail!("Stop {} before removing {product} {line}", users.join(", "));
            }
            runtimes::uninstall(root, product, line)?;
            json!(null)
        }
        "runtimes.default" => set_default(root, params)?,
        "jobs.list" => json!(daemon.jobs.list()),
        "settings.get" => json!(Settings::load(root)?),
        "settings.set" => update_settings(root, params)?,
        other => bail!("Unknown method: {other}"),
    })
}

/// Marks projects whose processes died as failed and stops their other processes.
fn reap_exited(root: &Path, state: &mut State) {
    let mut failed = Vec::new();
    for (id, children) in &mut state.processes {
        if let Some((name, exit)) = children
            .iter_mut()
            .find_map(|child| child.has_exited().map(|exit| (child.name.clone(), exit)))
        {
            failed.push((id.clone(), name, exit));
        }
    }
    for (id, name, exit) in failed {
        projects::stop_processes(root, state, &id);
        if let Ok(index) = state.index(&id) {
            let project = &mut state.projects[index];
            project.status = ProjectStatus::Error;
            project.error = Some(format!("Process {name} exited: {exit}"));
            project.url = None;
        }
        let _ = append_log(root, &id, &format!("Error: {name} exited: {exit}"));
        let _ = state.save(root);
    }
}

pub fn run_daemon() -> Result<()> {
    let root: PathBuf = home()?;
    if rpc::rpc("ping", json!({})).is_ok() {
        bail!("A Werd daemon is already running");
    }
    migrations::run(&root).context("Upgrading the Werd data folder failed")?;
    shims::refresh(&root);
    let state = Arc::new(Mutex::new(State::load(&root)?));
    let daemon = Daemon {
        root: root.clone(),
        jobs: Jobs::default(),
        fetcher: Arc::new(HttpFetcher),
        catalog: None,
    };
    let (listener, endpoint) = rpc::listen(&root)?;

    let monitor = Arc::clone(&state);
    thread::spawn(move || loop {
        thread::sleep(MONITOR_INTERVAL);
        if let Ok(mut state) = monitor.lock() {
            reap_exited(&root, &mut state);
        }
    });

    for stream in listener.incoming().flatten() {
        let daemon = daemon.clone();
        let state = Arc::clone(&state);
        let token = endpoint.token.clone();
        thread::spawn(move || {
            let _ = rpc::serve_connection(stream, &token, |method, params| {
                let mut state = state.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
                dispatch(&daemon, &mut state, method, &params)
            });
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobState;
    use crate::runtimes::tests::{fixture, LocalFetcher};

    fn daemon(root: &Path, catalog: Option<(Catalog, LocalFetcher)>) -> Daemon {
        let (catalog, fetcher): (Option<Arc<Catalog>>, Arc<dyn Fetcher>) = match catalog {
            Some((catalog, fetcher)) => (Some(Arc::new(catalog)), Arc::new(fetcher)),
            None => (None, Arc::new(HttpFetcher)),
        };
        Daemon {
            root: root.to_path_buf(),
            jobs: Jobs::default(),
            fetcher,
            catalog,
        }
    }

    #[test]
    fn dispatch_answers_ping_list_and_rejects_unknown_methods() {
        let root = tempfile::tempdir().unwrap();
        let daemon = daemon(root.path(), None);
        let mut state = State::default();
        let ping = dispatch(&daemon, &mut state, "ping", &json!({})).unwrap();
        assert_eq!(ping["protocol"], PROTOCOL_VERSION);
        let list = dispatch(&daemon, &mut state, "sites.list", &json!({})).unwrap();
        assert_eq!(list["projects"], json!([]));
        assert!(dispatch(&daemon, &mut state, "rm -rf", &json!({})).is_err());
        assert!(dispatch(&daemon, &mut state, "stop", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("id"));
    }

    #[test]
    fn failed_start_is_recorded_on_the_project() {
        let root = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        std::fs::write(work.path().join("artisan"), "").unwrap();
        std::fs::write(work.path().join("composer.json"), "{}").unwrap();
        let daemon = daemon(root.path(), None);
        let mut state = State::default();
        let added = dispatch(&daemon, &mut state, "add", &json!({ "path": work.path() })).unwrap();
        let id = added["id"].as_str().unwrap().to_string();

        assert!(dispatch(&daemon, &mut state, "start", &json!({ "id": id })).is_err());
        let project = state.project(&id).unwrap();
        assert_eq!(project.status, ProjectStatus::Error);
        assert!(project.error.as_deref().unwrap().contains("not installed"));
    }

    #[test]
    fn runtimes_install_in_the_background_then_update_and_uninstall() {
        let root = tempfile::tempdir().unwrap();
        let daemon = daemon(root.path(), Some(fixture(root.path(), &[("1", "1.0.0")])));
        let mut state = State::default();
        let params = json!({ "product": "tool", "line": "1" });

        assert!(dispatch(&daemon, &mut state, "runtimes.update", &params)
            .unwrap_err()
            .to_string()
            .contains("not installed"));
        let job = dispatch(&daemon, &mut state, "runtimes.install", &params).unwrap();
        let finished = daemon.jobs.wait(job["id"].as_str().unwrap());
        assert_eq!(finished.state, JobState::Done, "{:?}", finished.error);

        let rows = dispatch(&daemon, &mut state, "runtimes.list", &json!({})).unwrap();
        assert_eq!(rows[0]["installed"], "1.0.0");
        assert!(dispatch(&daemon, &mut state, "runtimes.default", &params)
            .unwrap_err()
            .to_string()
            .contains("no default"));

        dispatch(&daemon, &mut state, "runtimes.uninstall", &params).unwrap();
        let rows = dispatch(&daemon, &mut state, "runtimes.list", &json!({})).unwrap();
        assert!(rows[0]["installed"].is_null());
    }

    #[test]
    fn settings_are_validated_and_saved() {
        let root = tempfile::tempdir().unwrap();
        let daemon = daemon(root.path(), None);
        let mut state = State::default();
        let saved = dispatch(
            &daemon,
            &mut state,
            "settings.set",
            &json!({ "upload_max_mb": 256 }),
        )
        .unwrap();
        assert_eq!(saved["upload_max_mb"], 256);
        assert!(dispatch(
            &daemon,
            &mut state,
            "settings.set",
            &json!({ "memory_limit_mb": 1 })
        )
        .is_err());
        assert_eq!(
            dispatch(&daemon, &mut state, "settings.get", &json!({})).unwrap()["upload_max_mb"],
            256
        );
        assert!(dispatch(&daemon, &mut state, "catalog.refresh", &json!({}))
            .unwrap_err()
            .to_string()
            .contains("not available"));
    }
}
