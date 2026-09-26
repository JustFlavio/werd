#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use werd_core::{daemon_executable, ensure_daemon, rpc as daemon_rpc};

/// Passed by the login item so Werd starts in the tray without a window.
const HIDDEN_ARG: &str = "--hidden";
const TRAY_ID: &str = "werd";

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

#[tauri::command]
fn launch_at_login(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|error| error.to_string())?;
    launcher.is_enabled().map_err(|error| error.to_string())
}

/// Tray menu labels in the interface language.
#[derive(serde::Deserialize)]
struct TrayLabels {
    open: String,
    stop_all: String,
    quit: String,
}

#[tauri::command]
fn set_tray_labels(app: AppHandle, labels: TrayLabels) -> Result<(), String> {
    let menu = tray_menu(&app, &labels).map_err(|error| error.to_string())?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn tray_menu(app: &AppHandle, labels: &TrayLabels) -> tauri::Result<Menu<tauri::Wry>> {
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", &labels.open, true, None::<&str>)?,
            &MenuItem::with_id(app, "stop-all", &labels.stop_all, true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", &labels.quit, true, None::<&str>)?,
        ],
    )
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Stops every running site and service instance.
fn stop_all() -> anyhow::Result<()> {
    let snapshot = daemon_rpc("sites.list", json!({}))?;
    for project in snapshot["projects"].as_array().into_iter().flatten() {
        if project["status"] == "running" {
            daemon_rpc("sites.stop", json!({ "id": project["id"] }))?;
        }
    }
    let services = daemon_rpc("services.list", json!({}))?;
    for service in services.as_array().into_iter().flatten() {
        if service["status"] == "running" {
            daemon_rpc("services.stop", json!({ "id": service["id"] }))?;
        }
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        // Opening Werd again (shortcut, login item) focuses the running window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app)
        }))
        // The name is fixed because the Windows uninstaller removes this login item by name.
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Werd")
                .args([HIDDEN_ARG])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            rpc,
            open_url,
            sync_hosts,
            launch_at_login,
            set_launch_at_login,
            set_tray_labels
        ])
        .setup(|app| {
            let labels = TrayLabels {
                open: "Open Werd".into(),
                stop_all: "Stop all".into(),
                quit: "Quit".into(),
            };
            let menu = tray_menu(app.handle(), &labels)?;
            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .tooltip("Werd")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => show_window(app),
                    "stop-all" => {
                        std::thread::spawn(|| {
                            let _ = stop_all();
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Start the daemon right away, so autostart services come up at login
            // even while the window stays hidden.
            std::thread::spawn(|| {
                if let Ok(executable) = daemon_executable() {
                    let _ = ensure_daemon(&executable);
                }
            });
            if !std::env::args().any(|arg| arg == HIDDEN_ARG) {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Werd in the tray; sites keep running either way.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("Cannot start Werd");
}
