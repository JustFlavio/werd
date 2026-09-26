//! User settings in `<home>/config.json`.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Current on-disk layout; see `migrations.rs`.
pub const LAYOUT_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Data layout version; 0 means a pre-0.2 data folder.
    pub layout_version: u32,
    /// PHP line used by sites and the `php` shim when nothing else is specified.
    pub default_php: Option<String>,
    /// Node major used by the `node` shim when no `.nvmrc` applies.
    pub default_node: Option<String>,
    pub upload_max_mb: u32,
    /// `-1` means unlimited.
    pub memory_limit_mb: i64,
    /// Where `catalog refresh` downloads the catalog from. Unset until the repository is public.
    pub catalog_url: Option<String>,
    /// Whether `<home>/bin` has been added to the user's PATH.
    pub path_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            layout_version: 0,
            default_php: None,
            default_node: None,
            upload_max_mb: 100,
            memory_limit_mb: 512,
            catalog_url: None,
            path_enabled: false,
        }
    }
}

impl Settings {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("config.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let temporary = root.join("config.json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, root.join("config.json"))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if !(1..=4096).contains(&self.upload_max_mb) {
            bail!("Max upload size must be between 1 and 4096 MB");
        }
        if self.memory_limit_mb != -1 && !(16..=65536).contains(&self.memory_limit_mb) {
            bail!("Memory limit must be -1 (unlimited) or between 16 and 65536 MB");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults_and_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let mut settings = Settings::load(root.path()).unwrap();
        assert_eq!(settings, Settings::default());
        settings.default_php = Some("8.4".into());
        settings.save(root.path()).unwrap();
        assert_eq!(Settings::load(root.path()).unwrap(), settings);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("config.json"),
            r#"{"upload_max_mb": 64, "future_option": true}"#,
        )
        .unwrap();
        let settings = Settings::load(root.path()).unwrap();
        assert_eq!(settings.upload_max_mb, 64);
        assert_eq!(settings.memory_limit_mb, 512);
    }

    #[test]
    fn limits_are_validated() {
        assert!(Settings::default().validate().is_ok());
        assert!(Settings {
            upload_max_mb: 0,
            ..Settings::default()
        }
        .validate()
        .is_err());
        assert!(Settings {
            memory_limit_mb: -1,
            ..Settings::default()
        }
        .validate()
        .is_ok());
        assert!(Settings {
            memory_limit_mb: 8,
            ..Settings::default()
        }
        .validate()
        .is_err());
    }
}
