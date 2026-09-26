#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;
use werd_core::{daemon_executable, ensure_daemon, rpc as daemon_rpc};

/// Forwards one call to the daemon, starting it if needed. The daemon validates
/// every method and parameter, so the webview gets no extra privileges here.
/// Runs off the main thread so a slow daemon never freezes the window.
#[tauri::command]
async fn rpc(method: String, params: Option<Value>) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_daemon(&daemon_executable()?)?;
        daemon_rpc(&method, params.unwrap_or(Value::Null))
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("{error:#}"))
}

/// Maps every site's `.test` domain in the hosts file. Runs here rather than in
/// the daemon so the UAC prompt belongs to the visible window.
#[tauri::command]
async fn sync_hosts() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        ensure_daemon(&daemon_executable()?)?;
        let status = daemon_rpc("domains.status", Value::Null)?;
        let domains: Vec<String> = serde_json::from_value(status["domains"].clone())?;
        werd_core::platform::sync_hosts(&domains)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("{error:#}"))
}

/// Opens local service UIs (Mailpit, RustFS console) and the project repository.
/// The core rejects any other URL, so the webview cannot launch arbitrary programs.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    werd_core::platform::open_url(&url).map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![rpc, open_url, sync_hosts])
        .run(tauri::generate_context!())
        .expect("Cannot start Werd");
}
