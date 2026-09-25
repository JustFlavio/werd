//! The `werd-daemon` process: owns the state, supervises children, serves RPC.

use crate::model::{ProjectStatus, Snapshot};
use crate::paths::home;
use crate::process::append_log;
use crate::rpc::{self, PROTOCOL_VERSION};
use crate::state::State;
use crate::{doctor, process, projects, runtimes, VERSION};
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MONITOR_INTERVAL: Duration = Duration::from_secs(2);

fn id(params: &Value) -> Result<&str> {
    params["id"].as_str().context("Missing project id")
}

/// Executes one RPC method against the daemon state.
fn dispatch(root: &Path, state: &mut State, method: &str, params: &Value) -> Result<Value> {
    Ok(match method {
        "ping" => json!({ "version": VERSION, "protocol": PROTOCOL_VERSION }),
        "list" => json!(Snapshot {
            projects: state.projects.clone(),
            daemon_version: VERSION.into()
        }),
        "add" => json!(projects::add(
            root,
            state,
            params["path"].as_str().context("Missing path")?
        )?),
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
        "doctor" => json!(doctor::run(root)),
        "trust-ca" => json!(projects::trust_local_ca(root)?),
        "runtimes" => json!(runtimes::list(root)),
        "install" => json!(runtimes::install(
            root,
            params["id"].as_str().context("Missing runtime id")?
        )?),
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
    let state = Arc::new(Mutex::new(State::load(&root)?));
    let (listener, endpoint) = rpc::listen(&root)?;

    let monitor = Arc::clone(&state);
    let monitor_root = root.clone();
    thread::spawn(move || loop {
        thread::sleep(MONITOR_INTERVAL);
        if let Ok(mut state) = monitor.lock() {
            reap_exited(&monitor_root, &mut state);
        }
    });

    for stream in listener.incoming().flatten() {
        let root = root.clone();
        let state = Arc::clone(&state);
        let token = endpoint.token.clone();
        thread::spawn(move || {
            let _ = rpc::serve_connection(stream, &token, |method, params| {
                let mut state = state.lock().map_err(|_| anyhow!("Daemon state unavailable"))?;
                dispatch(&root, &mut state, method, &params)
            });
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_answers_ping_list_and_rejects_unknown_methods() {
        let root = tempfile::tempdir().unwrap();
        let mut state = State::default();
        let ping = dispatch(root.path(), &mut state, "ping", &json!({})).unwrap();
        assert_eq!(ping["protocol"], PROTOCOL_VERSION);
        let list = dispatch(root.path(), &mut state, "list", &json!({})).unwrap();
        assert_eq!(list["projects"], json!([]));
        assert!(dispatch(root.path(), &mut state, "rm -rf", &json!({})).is_err());
        assert!(dispatch(root.path(), &mut state, "stop", &json!({}))
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
        let mut state = State::default();
        let added = dispatch(root.path(), &mut state, "add", &json!({ "path": work.path() })).unwrap();
        let id = added["id"].as_str().unwrap().to_string();

        assert!(dispatch(root.path(), &mut state, "start", &json!({ "id": id })).is_err());
        let project = state.project(&id).unwrap();
        assert_eq!(project.status, ProjectStatus::Error);
        assert!(project.error.as_deref().unwrap().contains("not installed"));
    }
}
