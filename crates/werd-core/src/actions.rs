//! Opening a site folder in the tools around it: the file manager, a terminal
//! (optionally running Tinker) and code editors found on this computer.
//!
//! Terminals get Werd's shims first on PATH, so `php`, `composer` and `npm`
//! use the site's versions even without the PATH integration.

use crate::process::hidden_command;
use crate::shims;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Serialize)]
pub struct Editor {
    pub id: String,
    pub label: String,
    #[serde(skip)]
    pub program: PathBuf,
}

fn env_path(variable: &str) -> Option<PathBuf> {
    std::env::var_os(variable).map(PathBuf::from)
}

/// The newest `PhpStorm <version>` folder under `JetBrains`.
fn newest_phpstorm(jetbrains: &Path) -> Option<PathBuf> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(jetbrains)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("PhpStorm"))
        })
        .collect();
    folders.sort();
    folders
        .into_iter()
        .rev()
        .map(|folder| folder.join("bin").join("phpstorm64.exe"))
        .find(|program| program.is_file())
}

/// Code editors installed on this computer, in a stable order.
pub fn editors() -> Vec<Editor> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let local = env_path("LOCALAPPDATA").map(|path| path.join("Programs"));
    let program_files = env_path("ProgramFiles");
    let under = |base: &Option<PathBuf>, relative: &str| base.as_ref().map(|base| base.join(relative));
    let candidates: [(&str, &str, Vec<Option<PathBuf>>); 5] = [
        (
            "cursor",
            "Cursor",
            vec![
                under(&local, r"cursor\Cursor.exe"),
                under(&program_files, r"cursor\Cursor.exe"),
            ],
        ),
        (
            "vscode",
            "VS Code",
            vec![
                under(&local, r"Microsoft VS Code\Code.exe"),
                under(&program_files, r"Microsoft VS Code\Code.exe"),
            ],
        ),
        (
            "windsurf",
            "Windsurf",
            vec![under(&local, r"Windsurf\Windsurf.exe")],
        ),
        ("zed", "Zed", vec![under(&local, r"Zed\zed.exe")]),
        (
            "phpstorm",
            "PhpStorm",
            vec![
                program_files
                    .as_ref()
                    .and_then(|base| newest_phpstorm(&base.join("JetBrains"))),
                under(&local, r"PhpStorm\bin\phpstorm64.exe"),
            ],
        ),
    ];
    candidates
        .into_iter()
        .filter_map(|(id, label, paths)| {
            let program = paths.into_iter().flatten().find(|path| path.is_file())?;
            Some(Editor {
                id: id.into(),
                label: label.into(),
                program,
            })
        })
        .collect()
}

/// A console window in `folder`, optionally running `command` (e.g. Tinker).
fn open_terminal(root: &Path, folder: &Path, command: Option<&str>) -> Result<()> {
    if !cfg!(windows) {
        bail!("Opening a terminal is not available on this platform yet");
    }
    shims::ensure_installed(root)?;
    let path = std::env::join_paths(
        std::iter::once(shims::bin_dir(root))
            .chain(std::env::var_os("PATH").iter().flat_map(std::env::split_paths)),
    )?;
    let mut terminal = Command::new("powershell.exe");
    terminal.arg("-NoLogo").arg("-NoExit");
    if let Some(command) = command {
        terminal.arg("-Command").arg(command);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // A new console; Windows Terminal takes it over when it is the default.
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        terminal.creation_flags(CREATE_NEW_CONSOLE);
    }
    terminal
        .current_dir(folder)
        .env("PATH", path)
        .spawn()
        .context("Cannot open a terminal")?;
    Ok(())
}

/// Runs `action` for the site folder `folder`: `folder`, `terminal`, `tinker`
/// or `editor:<id>` with an id from [`editors`].
pub fn run(root: &Path, folder: &Path, action: &str) -> Result<()> {
    if !folder.is_dir() {
        bail!("{} no longer exists", folder.display());
    }
    match action {
        "folder" => {
            let explorer = if cfg!(windows) { "explorer.exe" } else { "xdg-open" };
            hidden_command(explorer)
                .arg(folder)
                .spawn()
                .context("Cannot open the folder")?;
        }
        "terminal" => open_terminal(root, folder, None)?,
        "tinker" => open_terminal(root, folder, Some("php artisan tinker"))?,
        other => {
            let id = other
                .strip_prefix("editor:")
                .with_context(|| format!("Unknown action {other}"))?;
            let editor = editors()
                .into_iter()
                .find(|editor| editor.id == id)
                .with_context(|| format!("{id} is not installed"))?;
            hidden_command(&editor.program)
                .arg(folder)
                .spawn()
                .with_context(|| format!("Cannot start {}", editor.label))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_actions_and_missing_folders_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        assert!(run(root.path(), folder.path(), "rm").is_err());
        assert!(run(root.path(), folder.path(), "editor:notepad-plus-plus").is_err());
        assert!(run(root.path(), &folder.path().join("gone"), "folder").is_err());
    }

    #[test]
    fn the_newest_phpstorm_wins() {
        let jetbrains = tempfile::tempdir().unwrap();
        for version in ["PhpStorm 2024.3", "PhpStorm 2025.3.2", "RustRover 2026.2"] {
            let bin = jetbrains.path().join(version).join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("phpstorm64.exe"), "").unwrap();
        }
        let found = newest_phpstorm(jetbrains.path()).unwrap();
        assert!(found.to_string_lossy().contains("2025.3.2"));
    }
}
