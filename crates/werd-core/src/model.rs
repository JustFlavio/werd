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

    /// Catalog product that provides this service.
    pub fn product(self) -> &'static str {
        match self {
            Self::Postgres => "postgresql",
            Self::Redis => "redis",
            Self::Mailpit => "mailpit",
            Self::Rustfs => "rustfs",
        }
    }

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

/// Categories a site can link a service instance to.
pub const CATEGORIES: [&str; 6] = ["database", "cache", "queue", "mail", "storage", "search"];

/// A site's use of a service instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// Service instance id.
    pub instance: String,
    /// Database (or bucket) of the site inside the instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

/// A service a site asks for (from `werd.yml`) that is not linked yet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    pub category: String,
    pub product: String,
    /// Wanted line; any installed line matches when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    /// PHP line the site runs on.
    pub php: String,
    /// `.test` domain of the site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Parked folder the site was found in; such sites follow their folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parked: Option<String>,
    /// Node.js major used by the shims inside the site folder, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// Service instance per category (`database`, `cache`, …).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, Link>,
    /// Services from `werd.yml` still waiting to be linked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub status: ProjectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Ports of the site itself (`site`, `fastcgi`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<Ports>,

    // Fields of 0.1 and 0.2 per-site services, read only by the data migration.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ServiceName>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub versions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
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
