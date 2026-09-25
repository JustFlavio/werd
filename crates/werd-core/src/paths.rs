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

/// Path of a runtime executable, whether or not it is installed.
pub(crate) fn runtime_binary(root: &Path, name: &str) -> PathBuf {
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let family = match name {
        "php-cgi" => "php/8.5",
        "caddy" => "caddy/2.11.4",
        "postgres" | "initdb" | "psql" | "pg_ctl" => "postgres/18/bin",
        "redis-server" | "redis-cli" => "redis/7.2",
        "mailpit" => "mailpit/1.31.2",
        "rustfs" => "rustfs/1.0.0",
        _ => "unknown",
    };
    root.join("runtimes").join(family).join(executable)
}
