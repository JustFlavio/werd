//! Werd core: the daemon that manages runtimes, services and Laravel projects,
//! plus the RPC client used by the CLI and the desktop app.
//!
//! Module map:
//! - [`model`]: types exchanged with clients
//! - [`catalog`]: what can be installed (generated data, see `scripts/catalog`)
//! - [`runtimes`]: installed lines, install/update/uninstall, `php.ini`
//! - [`settings`], `migrations`: user settings and data-folder upgrades
//! - `jobs`: background downloads with progress
//! - `manifest`: `werd.yml`
//! - `state`, `projects`: linked projects and their lifecycle
//! - `services`: PostgreSQL, Redis, Mailpit, RustFS
//! - `proxy`: PHP FastCGI + Caddy per site
//! - [`platform`]: OS integration (browser, certificates)
//! - `rpc`, `daemon`: local API and the daemon process

pub mod catalog;
mod daemon;
mod doctor;
pub mod jobs;
pub mod manifest;
mod migrations;
pub mod model;
mod paths;
pub mod platform;
mod ports;
mod process;
mod projects;
mod proxy;
mod rpc;
pub mod runtimes;
mod services;
pub mod settings;
mod state;

pub use daemon::run_daemon;
pub use model::{DoctorResult, Ports, Project, ProjectStatus, ServiceName, Snapshot};
pub use paths::home;
pub use process::LOG_SOURCES;
pub use rpc::{daemon_executable, ensure_daemon, rpc, PROTOCOL_VERSION};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
