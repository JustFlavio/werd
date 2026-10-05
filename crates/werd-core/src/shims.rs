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
                bytes: bytes.into(),
                vtype: winreg::enums::REG_EXPAND_SZ,
            },
        )?;
        crate::platform::broadcast_environment_change();
        Ok(())
    }
}

const PROFILE_START: &str = "# >>> werd >>>";
const PROFILE_END: &str = "# <<< werd <<<";

/// `profile` with Werd's PATH block at the end, or `None` when it is already there.
/// Appending keeps Werd ahead of entries the file adds earlier (Homebrew, Herd);
/// a block already present is left where it is.
#[cfg_attr(windows, allow(dead_code))]
pub fn profile_with_block(profile: &str, bin: &str) -> Option<String> {
    let block = format!(
        "{PROFILE_START}\n# Added by Werd: php, composer, node, npm and npx.\nexport PATH=\"{}:$PATH\"\n{PROFILE_END}\n",
        bin.replace('\\', "\\\\").replace('"', "\\\"").replace('$', "\\$")
    );
    if profile.contains(&block) {
        return None;
    }
    let mut updated = profile_without_block(profile).unwrap_or_else(|| profile.to_string());
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&block);
    Some(updated)
}

/// `profile` without Werd's PATH block, or `None` when it has none.
#[cfg_attr(windows, allow(dead_code))]
pub fn profile_without_block(profile: &str) -> Option<String> {
    let start = profile.find(PROFILE_START)?;
    let end = profile[start..].find(PROFILE_END)? + start + PROFILE_END.len();
    let end = if profile[end..].starts_with('\n') {
        end + 1
    } else {
        end
    };
    Some(format!("{}{}", &profile[..start], &profile[end..]))
}

/// Shell start-up files that get the PATH block: the default shell's file is
/// created when missing (zsh on macOS, bash on Linux), the others only if present.
#[cfg(not(windows))]
fn shell_profiles() -> Result<Vec<(PathBuf, bool)>> {
    let home = dirs::home_dir().context("Home folder not found")?;
    let default = if cfg!(target_os = "macos") {
        ".zshrc"
    } else {
        ".bashrc"
    };
    Ok([".zshrc", ".bashrc", ".bash_profile"]
        .into_iter()
        .map(|name| (home.join(name), name == default))
        .collect())
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
    for (profile, create) in shell_profiles()? {
        if !create && !profile.exists() {
            continue;
        }
        let current = fs::read_to_string(&profile).unwrap_or_default();
        if let Some(updated) = profile_with_block(&current, &bin) {
            fs::write(&profile, updated).with_context(|| format!("Cannot update {}", profile.display()))?;
        }
    }
    let mut settings = Settings::load(root)?;
    settings.path_enabled = true;
    settings.save(root)
}

/// Removes `<home>/bin` from the user's PATH and deletes the shims.
pub fn disable(root: &Path) -> Result<()> {
    let bin = bin_dir(root);
    #[cfg(windows)]
    if let Some(updated) = without_entry(&user_path::read()?, &bin.to_string_lossy()) {
        user_path::write(&updated)?;
    }
    #[cfg(not(windows))]
    for (profile, _) in shell_profiles()? {
        if let Some(updated) = fs::read_to_string(&profile)
            .ok()
            .as_deref()
            .and_then(profile_without_block)
        {
            fs::write(&profile, updated).with_context(|| format!("Cannot update {}", profile.display()))?;
        }
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
    fn shell_profile_block_is_added_once_and_removed_cleanly() {
        let bin = "/Users/me/Library/Application Support/Werd/bin";
        let original = "export PATH=\"/opt/homebrew/bin:$PATH\"";
        let added = profile_with_block(original, bin).unwrap();
        assert!(added.starts_with("export PATH=\"/opt/homebrew/bin:$PATH\"\n# >>> werd >>>\n"));
        assert!(added.contains(&format!("export PATH=\"{bin}:$PATH\"\n")));
        assert_eq!(profile_with_block(&added, bin), None, "not duplicated");
        assert_eq!(
            profile_with_block(&format!("{added}alias ll='ls -l'\n"), bin),
            None,
            "an existing block is left where the user keeps it"
        );
        let other_bin = profile_with_block(&added, "/elsewhere/bin").unwrap();
        assert_eq!(
            other_bin.matches("# >>> werd >>>").count(),
            1,
            "a stale block is replaced"
        );
        assert_eq!(profile_without_block(&added).unwrap(), format!("{original}\n"));
        assert_eq!(profile_without_block(original), None);
        assert!(profile_with_block("", "/a$b\"c")
            .unwrap()
            .contains(r#"export PATH="/a\$b\"c:$PATH""#));
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
