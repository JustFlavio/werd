//! Locations inside the Werd data directory.
//!
//! ```text
//! <home>/                     WERD_HOME, or the OS local data dir + "Werd"
//!   state.json                linked projects
//!   daemon.json               RPC endpoint of the running daemon
//!   runtimes/<family>/...     downloaded runtimes
//!   downloads/                verified archives
//!   projects/<id>/            per-project data and logs
//!   caddy-data/               Caddy PKI (local CA)
//! ```

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn home() -> Result<PathBuf> {
    let root = match std::env::var_os("WERD_HOME") {
        Some(path) => PathBuf::from(path),
        None => dirs::data_local_dir()
            .context("User data directory not found")?
            .join("Werd"),
    };
    fs::create_dir_all(&root).with_context(|| format!("Cannot create {}", root.display()))?;
    Ok(root)
}

pub(crate) fn state_file(root: &Path) -> PathBuf {
    root.join("state.json")
}

pub(crate) fn endpoint_file(root: &Path) -> PathBuf {
    root.join("daemon.json")
}

pub(crate) fn project_dir(root: &Path, id: &str) -> PathBuf {
    root.join("projects").join(id)
}

pub(crate) fn caddy_data(root: &Path) -> PathBuf {
    root.join("caddy-data")
}
