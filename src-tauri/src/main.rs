#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_updater::{Update, UpdaterExt};
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

/// Lets the user choose a folder with the system dialog; None when cancelled.
#[tauri::command]
async fn pick_folder(app: AppHandle, title: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = app.dialog().file().set_title(title).blocking_pick_folder() else {
            return Ok(None);
        };
        folder
            .into_path()
            .map(|path| Some(path.to_string_lossy().into_owned()))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Code editors found on this computer.
#[tauri::command]
fn editors() -> Vec<werd_core::actions::Editor> {
    werd_core::actions::editors()
}

/// Opens a site in the file manager, a terminal, Tinker or an editor. The
/// webview passes a site id; the folder comes from the daemon.
#[tauri::command]
async fn site_action(id: String, action: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_daemon(&daemon_executable()?)?;
        let snapshot = daemon_rpc("sites.list", json!({}))?;
        let path = snapshot["projects"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|project| project["id"] == id.as_str())
            .and_then(|project| project["path"].as_str())
            .map(std::path::PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Site not found"))?;
        werd_core::actions::run(&werd_core::home()?, &path, &action)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("{error:#}"))
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

/// An update found by `check_update`, and its package once downloaded.
#[derive(Default)]
struct PendingUpdate(Mutex<Option<(Update, Option<Vec<u8>>)>>);

#[derive(serde::Serialize)]
struct UpdateInfo {
    version: String,
    /// Release notes (markdown) from the release.
    notes: Option<String>,
    date: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct DownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

/// Asks werd-releases whether a newer version exists; remembers it for download.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let Some(update) = update else {
        return Ok(None);
    };
    let info = UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone(),
        date: update.date.map(|date| date.to_string()),
    };
    if let Ok(mut pending) = app.state::<PendingUpdate>().0.lock() {
        *pending = Some((update, None));
    }
    Ok(Some(info))
}

/// Downloads the pending update, reporting progress; installing waits for Restart.
#[tauri::command]
async fn download_update(app: AppHandle, on_progress: Channel<DownloadProgress>) -> Result<(), String> {
    let update = app
        .state::<PendingUpdate>()
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(|(update, _)| update.clone())
        .ok_or("No update to download")?;
    let mut downloaded: u64 = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(DownloadProgress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())?;
    if let Ok(mut pending) = app.state::<PendingUpdate>().0.lock() {
        if let Some((_, package)) = pending.as_mut() {
            *package = Some(bytes);
        }
    }
    Ok(())
}

/// Installs the downloaded update and restarts Werd. On Windows the installer
/// closes the app, updates it in passive mode and starts it again.
#[tauri::command]
fn install_update(app: AppHandle) -> Result<(), String> {
    let (update, bytes) = app
        .state::<PendingUpdate>()
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .take()
        .ok_or("No update to install")?;
    let bytes = bytes.ok_or("Download the update first")?;
    update.install(bytes).map_err(|error| error.to_string())?;
    app.restart();
}

/// Tray menu labels in the interface language.
#[derive(Clone, serde::Deserialize)]
struct TrayLabels {
    open: String,
    stop_all: String,
    quit: String,
    /// Prefix of the PHP entries, e.g. "Use PHP".
    use_php: String,
    check_updates: String,
}

impl Default for TrayLabels {
    fn default() -> Self {
        Self {
            open: "Open Werd".into(),
            stop_all: "Stop all sites and services".into(),
            quit: "Quit".into(),
            use_php: "Use PHP".into(),
            check_updates: "Check for updates".into(),
        }
    }
}

/// The labels last sent by the window, reused when the menu is rebuilt.
struct TrayState(Mutex<TrayLabels>);

/// Installed PHP lines, newest first, and whether each is the default.
fn php_lines() -> Vec<(String, bool)> {
    let Ok(rows) = daemon_rpc("runtimes.list", json!({})) else {
        return Vec::new();
    };
    rows.as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["product"] == "php" && !row["installed"].is_null())
        .filter_map(|row| {
            Some((
                row["line"].as_str()?.to_string(),
                row["is_default"].as_bool().unwrap_or(false),
            ))
        })
        .collect()
}

fn tray_menu(
    app: &AppHandle,
    labels: &TrayLabels,
    php: &[(String, bool)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let mut items: Vec<Box<dyn IsMenuItem<tauri::Wry>>> = vec![
        Box::new(MenuItem::with_id(app, "open", &labels.open, true, None::<&str>)?),
        Box::new(PredefinedMenuItem::separator(app)?),
    ];
    // The global PHP version, used by `php` and `composer` outside Werd sites.
    for (line, default) in php {
        items.push(Box::new(CheckMenuItem::with_id(
            app,
            format!("php:{line}"),
            format!("{} {line}", labels.use_php),
            true,
            *default,
            None::<&str>,
        )?));
    }
    if !php.is_empty() {
        items.push(Box::new(PredefinedMenuItem::separator(app)?));
    }
    items.push(Box::new(MenuItem::with_id(
        app,
        "stop-all",
        &labels.stop_all,
        true,
        None::<&str>,
    )?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "check-updates",
        &labels.check_updates,
        true,
        None::<&str>,
    )?));
    items.push(Box::new(MenuItem::with_id(
        app,
        "quit",
        &labels.quit,
        true,
        None::<&str>,
    )?));
    let references: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|item| item.as_ref()).collect();
    Menu::with_items(app, &references)
}

/// Rebuilds the tray menu with the current labels and PHP versions.
/// Calls the daemon, so it runs off the main thread.
fn refresh_tray(app: &AppHandle) {
    let labels = app
        .state::<TrayState>()
        .0
        .lock()
        .map(|labels| labels.clone())
        .unwrap_or_default();
    let php = php_lines();
    if let (Ok(menu), Some(tray)) = (tray_menu(app, &labels, &php), app.tray_by_id(TRAY_ID)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Called by the window when the language or the installed PHP versions change.
#[tauri::command]
async fn set_tray_labels(app: AppHandle, labels: TrayLabels) -> Result<(), String> {
    if let Ok(mut current) = app.state::<TrayState>().0.lock() {
        *current = labels;
    }
    tauri::async_runtime::spawn_blocking(move || refresh_tray(&app))
        .await
        .map_err(|error| error.to_string())
}

/// Makes `line` the global PHP version from the tray.
fn use_php(app: AppHandle, line: String) {
    std::thread::spawn(move || {
        let _ = daemon_rpc("runtimes.default", json!({ "product": "php", "line": line }));
        refresh_tray(&app);
        // Lets the window refresh its PHP pages.
        let _ = app.emit("werd://runtimes-changed", ());
    });
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
        // Registered first, as the plugin requires.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PendingUpdate::default())
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
            pick_folder,
            editors,
            site_action,
            launch_at_login,
            set_launch_at_login,
            set_tray_labels,
            check_update,
            download_update,
            install_update
        ])
        .setup(|app| {
            app.manage(TrayState(Mutex::new(TrayLabels::default())));
            let menu = tray_menu(app.handle(), &TrayLabels::default(), &[])?;
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
                    "check-updates" => {
                        show_window(app);
                        let _ = app.emit("werd://check-update", ());
                    }
                    id => {
                        if let Some(line) = id.strip_prefix("php:") {
                            use_php(app.clone(), line.to_string());
                        }
                    }
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

            // The login item stores the program path; refresh it so it follows Werd
            // when an update moves the program (0.3 moved it out of the data folder).
            let launcher = app.autolaunch();
            if launcher.is_enabled().unwrap_or(false) {
                let _ = launcher.enable();
            }

            // Start the daemon right away, so autostart services come up at login
            // even while the window stays hidden.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                if let Ok(executable) = daemon_executable() {
                    if ensure_daemon(&executable).is_ok() {
                        refresh_tray(&handle);
                    }
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
