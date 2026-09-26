//! Command line shims (`php`, `composer`, `node`, `npm`, `npx`) in `<home>/bin`,
//! and the opt-in PATH integration that makes them available in every terminal.

use crate::settings::Settings;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const TOOLS: [&str; 5] = ["php", "composer", "node", "npm", "npx"];

pub fn bin_dir(root: &Path) -> PathBuf {
    root.join("bin")
}

/// `werd-shim` next to the running daemon (installed app) or in the dev build folder.
fn shim_source() -> Result<PathBuf> {
    let name = if cfg!(windows) {
        "werd-shim.exe"
    } else {
        "werd-shim"
    };
    let current = std::env::current_exe()?;
    let candidate = current.parent().context("Invalid executable path")?.join(name);
    if candidate.is_file() {
        return Ok(candidate);
    }
    bail!("{name} not found next to {}; reinstall Werd", current.display())
}

/// Copies the shim once per tool. Copies (not links) let Werd update the shim in place.
pub fn install(root: &Path, source: &Path) -> Result<()> {
    let bin = bin_dir(root);
    fs::create_dir_all(&bin)?;
    for tool in TOOLS {
        let target = bin.join(if cfg!(windows) {
            format!("{tool}.exe")
        } else {
            tool.into()
        });
        fs::copy(source, &target).with_context(|| format!("Cannot write {}", target.display()))?;
    }
    Ok(())
}

/// Makes sure the shims exist in `<home>/bin`, even without PATH integration,
/// for commands Werd runs itself.
pub fn ensure_installed(root: &Path) -> Result<()> {
    let marker = bin_dir(root).join(if cfg!(windows) { "composer.exe" } else { "composer" });
    if marker.is_file() {
        return Ok(());
    }
    install(root, &shim_source()?)
}

/// Puts `entry` first in a `;`-separated PATH value, so Werd's `php` and
/// `composer` win over other tools (Herd, Composer-Setup). Returns `None` when
/// it already comes first.
// Used by the Windows PATH integration; tested on every platform.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn with_entry(path: &str, entry: &str) -> Option<String> {
    let normalize = |value: &str| value.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let mut parts = path.split(';').filter(|part| !part.trim().is_empty());
    if parts
        .next()
        .is_some_and(|first| normalize(first) == normalize(entry))
    {
        return None;
    }
    let others: Vec<&str> = path
        .split(';')
        .filter(|part| !part.trim().is_empty() && normalize(part) != normalize(entry))
        .collect();
    Some(std::iter::once(entry).chain(others).collect::<Vec<_>>().join(";"))
}

/// Removes `entry` from a `;`-separated PATH value, if present.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn without_entry(path: &str, entry: &str) -> Option<String> {
    let normalize = |value: &str| value.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let kept: Vec<&str> = path
        .split(';')
        .filter(|existing| normalize(existing) != normalize(entry))
        .collect();
    (kept.len() != path.split(';').count()).then(|| kept.join(";"))
}

#[cfg(windows)]
mod user_path {
    use anyhow::{Context, Result};
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::{RegKey, RegValue};

    fn key() -> Result<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
            .context("Cannot open the user environment in the registry")
    }

    /// The raw user PATH, with `%VARIABLES%` left unexpanded.
    pub fn read() -> Result<String> {
        match key()?.get_raw_value("Path") {
            Ok(value) => {
                let units: Vec<u16> = value
                    .bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect();
                Ok(String::from_utf16_lossy(&units)
                    .trim_end_matches('\0')
                    .to_string())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error.into()),
        }
    }

    /// Writes the user PATH as REG_EXPAND_SZ so `%USERPROFILE%`-style entries keep working.
    pub fn write(value: &str) -> Result<()> {
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        key()?.set_raw_value(
            "Path",
            &RegValue {
                bytes,
                vtype: winreg::enums::REG_EXPAND_SZ,
            },
        )?;
        crate::platform::broadcast_environment_change();
        Ok(())
    }
}

/// Installs the shims and adds `<home>/bin` to the user's PATH.
pub fn enable(root: &Path) -> Result<()> {
    install(root, &shim_source()?)?;
    let bin = bin_dir(root).to_string_lossy().into_owned();
    #[cfg(windows)]
    if let Some(updated) = with_entry(&user_path::read()?, &bin) {
        user_path::write(&updated)?;
    }
    #[cfg(not(windows))]
    bail!("Adding Werd to PATH is not supported on this platform yet; add {bin} to your shell profile");
    #[allow(unreachable_code)]
    {
        let mut settings = Settings::load(root)?;
        settings.path_enabled = true;
        settings.save(root)
    }
}

/// Removes `<home>/bin` from the user's PATH and deletes the shims.
pub fn disable(root: &Path) -> Result<()> {
    let bin = bin_dir(root);
    #[cfg(windows)]
    if let Some(updated) = without_entry(&user_path::read()?, &bin.to_string_lossy()) {
        user_path::write(&updated)?;
    }
    if bin.exists() {
        fs::remove_dir_all(&bin).with_context(|| format!("Cannot remove {}", bin.display()))?;
    }
    let mut settings = Settings::load(root)?;
    settings.path_enabled = false;
    settings.save(root)
}

/// Refreshes the shim copies after a Werd update, when PATH integration is on,
/// and moves Werd back to the front of PATH if another installer pushed it down.
pub fn refresh(root: &Path) {
    if Settings::load(root).is_ok_and(|settings| settings.path_enabled) {
        if let Ok(source) = shim_source() {
            let _ = install(root, &source);
        }
        #[cfg(windows)]
        if let Ok(current) = user_path::read() {
            if let Some(updated) = with_entry(&current, &bin_dir(root).to_string_lossy()) {
                let _ = user_path::write(&updated);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entries_are_added_once_and_removed_cleanly() {
        let bin = r"C:\Users\me\AppData\Local\Werd\bin";
        let original = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\tools;";
        let added = with_entry(original, bin).unwrap();
        assert_eq!(
            added,
            format!(r"{bin};%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\tools")
        );
        let herd_first = format!(r"C:\Users\me\.config\herd\bin;{bin};C:\tools");
        assert_eq!(
            with_entry(&herd_first, bin).unwrap(),
            format!(r"{bin};C:\Users\me\.config\herd\bin;C:\tools"),
            "moved ahead of Herd, not duplicated"
        );
        assert_eq!(
            with_entry(&added, &format!("{bin}\\").to_uppercase()),
            None,
            "case and trailing slash ignored"
        );
        assert_eq!(
            without_entry(&added, bin).unwrap(),
            r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps;C:\tools"
        );
        assert_eq!(without_entry(original, bin), None);
        assert_eq!(with_entry("", bin).unwrap(), bin);
    }

    #[test]
    fn install_copies_one_shim_per_tool() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("werd-shim.bin");
        fs::write(&source, "shim").unwrap();
        install(root.path(), &source).unwrap();
        for tool in TOOLS {
            let name = if cfg!(windows) {
                format!("{tool}.exe")
            } else {
                tool.to_string()
            };
            assert_eq!(
                fs::read_to_string(bin_dir(root.path()).join(name)).unwrap(),
                "shim"
            );
        }
    }
}
