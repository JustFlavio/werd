//! Types shared by the daemon, the CLI and the desktop app.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Ports assigned to a project, keyed by role (`site`, `postgres`, `mailpit_ui`, ...).
pub type Ports = BTreeMap<String, u16>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    #[default]
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceName {
    Postgres,
    Redis,
    Mailpit,
    Rustfs,
}

impl ServiceName {
    pub const ALL: [ServiceName; 4] = [Self::Postgres, Self::Redis, Self::Mailpit, Self::Rustfs];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Redis => "redis",
            Self::Mailpit => "mailpit",
            Self::Rustfs => "rustfs",
        }
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub php: String,
    pub services: Vec<ServiceName>,
    #[serde(default)]
    pub status: ProjectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<Ports>,
}

impl Project {
    pub fn uses(&self, service: ServiceName) -> bool {
        self.services.contains(&service)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub daemon_version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DoctorResult {
    pub label: String,
    pub ok: bool,
    pub detail: String,
}
