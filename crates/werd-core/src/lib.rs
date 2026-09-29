//! Werd core: the daemon that manages runtimes, services and Laravel projects,
//! plus the RPC client used by the CLI and the desktop app.
//!
//! Module map:
//! - [`model`]: types exchanged with clients
//! - [`catalog`]: what can be installed (generated data, see `scripts/catalog`)
//! - [`runtimes`]: installed lines, install/update/uninstall, `php.ini`
//! - [`settings`], `migrations`: user settings and data-folder upgrades
//! - `jobs`: background downloads with progress
//! - `shims`: php/composer/node/npm/npx launchers and PATH integration
//! - `manifest`: `werd.yml`
//! - `state`, `projects`: linked projects and their lifecycle
//! - `instances`: shared service instances (PostgreSQL, MySQL, Redis, …)
//! - `proxy`, `router`: PHP FastCGI per site behind one shared Caddy
//! - [`domains`]: `.test` names and the hosts file
//! - `parks`: parked folders
//! - [`platform`]: OS integration (browser, certificates)
//! - `rpc`, `daemon`: local API and the daemon process

pub mod actions;
mod artisan;
pub mod catalog;
mod create;
mod daemon;
mod doctor;
pub mod domains;
mod inspect;
mod instances;
pub mod jobs;
pub mod manifest;
mod migrations;
pub mod model;
mod parks;
mod paths;
pub mod platform;
mod ports;
mod process;
mod projects;
mod proxy;
mod router;
mod rpc;
pub mod runtimes;
pub mod settings;
mod shims;
mod state;
mod vite;

pub use daemon::run_daemon;
pub use model::{DoctorResult, Ports, Project, ProjectStatus, ServiceName, Snapshot};
pub use paths::home;
pub use process::LOG_SOURCES;
pub use rpc::{daemon_executable, ensure_daemon, rpc, PROTOCOL_VERSION};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
