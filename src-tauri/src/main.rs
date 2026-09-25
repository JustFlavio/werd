#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use werd_core::{daemon_executable, ensure_daemon, rpc, DoctorResult, Project, Snapshot};

fn call(method: &str, params: Value) -> Result<Value, String> {
    let executable = daemon_executable().map_err(|error| error.to_string())?;
    ensure_daemon(&executable).map_err(|error| error.to_string())?;
    rpc(method, params).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_projects() -> Result<Snapshot, String> {
    serde_json::from_value(call("list", json!({}))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn add_project(path: String) -> Result<Project, String> {
    serde_json::from_value(call("add", json!({ "path": path }))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn start_project(id: String) -> Result<Project, String> {
    serde_json::from_value(call("start", json!({ "id": id }))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn stop_project(id: String) -> Result<Project, String> {
    serde_json::from_value(call("stop", json!({ "id": id }))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_ports(id: String) -> Result<Project, String> {
    serde_json::from_value(call("reset-ports", json!({ "id": id }))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn project_logs(id: String, service: Option<String>) -> Result<Vec<String>, String> {
    serde_json::from_value(call(
        "logs",
        json!({ "id": id, "service": service.unwrap_or_else(|| "werd".into()) }),
    )?)
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn doctor() -> Result<Vec<DoctorResult>, String> {
    serde_json::from_value(call("doctor", json!({}))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn runtimes() -> Result<Value, String> {
    call("runtimes", json!({}))
}

#[tauri::command]
fn install_runtime(id: String) -> Result<Value, String> {
    call("install", json!({ "id": id }))
}

#[tauri::command]
fn project_env(id: String) -> Result<String, String> {
    serde_json::from_value(call("env", json!({ "id": id }))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn trust_ca() -> Result<String, String> {
    serde_json::from_value(call("trust-ca", json!({}))?).map_err(|error| error.to_string())
}

#[tauri::command]
fn open_site(id: String) -> Result<String, String> {
    serde_json::from_value(call("open", json!({ "id": id }))?).map_err(|error| error.to_string())
}

/// Opens local service UIs (Mailpit, RustFS console) and the project repository.
/// The core rejects any other URL, so the webview cannot launch arbitrary programs.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    werd_core::platform::open_url(&url).map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            list_projects,
            add_project,
            start_project,
            stop_project,
            reset_ports,
            open_site,
            open_url,
            project_logs,
            doctor,
            runtimes,
            install_runtime,
            project_env,
            trust_ca
        ])
        .run(tauri::generate_context!())
        .expect("Cannot start Werd");
}
