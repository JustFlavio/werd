//! Backing services a project can declare in `werd.yml`.
//!
//! Each service knows how to start its process, shut it down cleanly and
//! describe the `.env` values a Laravel app needs to reach it.

mod mailpit;
mod postgres;
mod redis;
mod rustfs;

use crate::model::{Ports, Project, ServiceName};
use crate::paths::project_dir;
use crate::process::ManagedChild;
use crate::runtimes;
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the service instances of one project live and which runtime lines they use.
pub(crate) struct ServiceContext<'a> {
    pub root: &'a Path,
    pub project_id: &'a str,
    /// Requested line per product (`postgresql` → `18`); newest installed otherwise.
    pub versions: &'a BTreeMap<String, String>,
    pub extensions: &'a [String],
}

impl<'a> ServiceContext<'a> {
    pub fn new(root: &'a Path, project: &'a Project) -> Self {
        Self {
            root,
            project_id: &project.id,
            versions: &project.versions,
            extensions: &project.extensions,
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        project_dir(self.root, self.project_id)
    }

    /// Install folder of the line this project uses for `product`.
    pub fn runtime_dir(&self, product: &str, label: &str) -> Result<PathBuf> {
        let preferred = self.versions.get(product).map(String::as_str);
        let line = runtimes::resolve_line(self.root, product, preferred, label)?;
        Ok(runtimes::line_dir(self.root, product, &line))
    }

    /// An installed executable (`relative` without `.exe`), or an error naming what to install.
    pub fn binary(&self, product: &str, relative: &str, label: &str) -> Result<PathBuf> {
        let path = self.runtime_dir(product, label)?.join(runtimes::exe(relative));
        if !path.is_file() {
            bail!("{label} is incomplete: {} is missing", path.display());
        }
        Ok(path)
    }
}

pub(crate) trait Service: Sync {
    /// Starts the service and pushes its process(es) onto `children`, even when
    /// a later setup step fails, so the caller can clean up.
    fn start(
        &self,
        context: &ServiceContext,
        ports: &mut Ports,
        children: &mut Vec<ManagedChild>,
    ) -> Result<()>;

    /// Asks the service to stop cleanly before its process is killed.
    fn shutdown(&self, _context: &ServiceContext, _ports: &Ports) {}

    /// `KEY=value` lines for the project's `.env`. Empty until ports are assigned.
    fn env(&self, context: &ServiceContext, ports: &Ports) -> Result<Vec<String>>;
}

pub(crate) fn get(name: ServiceName) -> &'static dyn Service {
    match name {
        ServiceName::Postgres => &postgres::Postgres,
        ServiceName::Redis => &redis::Redis,
        ServiceName::Mailpit => &mailpit::Mailpit,
        ServiceName::Rustfs => &rustfs::Rustfs,
    }
}

fn by_process_name(name: &str) -> Option<ServiceName> {
    ServiceName::ALL
        .into_iter()
        .find(|service| service.as_str() == name)
}

/// Starts `services` in order. On failure the processes already started stay
/// in `children` for [`stop_all`].
pub(crate) fn start_all(
    context: &ServiceContext,
    services: &[ServiceName],
    ports: &mut Ports,
    children: &mut Vec<ManagedChild>,
) -> Result<()> {
    for service in services {
        get(*service).start(context, ports, children)?;
    }
    Ok(())
}

/// Shuts services down cleanly, then kills every remaining process in reverse start order.
pub(crate) fn stop_all(context: &ServiceContext, children: &mut Vec<ManagedChild>, ports: &Ports) {
    for child in children.iter() {
        if let Some(service) = by_process_name(&child.name) {
            get(service).shutdown(context, ports);
        }
    }
    for child in children.iter_mut().rev() {
        child.kill();
    }
    children.clear();
}

/// The `.env` block for all services of a project, in declaration order.
pub(crate) fn env_lines(
    context: &ServiceContext,
    services: &[ServiceName],
    ports: &Ports,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for service in services {
        lines.extend(get(*service).env(context, ports)?);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_lines_follow_assigned_ports() {
        let root = tempfile::tempdir().unwrap();
        let versions = BTreeMap::new();
        let context = ServiceContext {
            root: root.path(),
            project_id: "p1",
            versions: &versions,
            extensions: &[],
        };
        std::fs::create_dir_all(context.data_dir()).unwrap();
        let ports = Ports::from([
            ("postgres".to_string(), 6001),
            ("redis".to_string(), 6002),
            ("mailpit_smtp".to_string(), 6003),
            ("rustfs_api".to_string(), 6005),
        ]);
        let lines = env_lines(&context, &ServiceName::ALL, &ports).unwrap();
        for expected in [
            "DB_CONNECTION=pgsql",
            "DB_PORT=6001",
            "REDIS_PORT=6002",
            "MAIL_PORT=6003",
            "AWS_ENDPOINT=http://127.0.0.1:6005",
            "AWS_USE_PATH_STYLE_ENDPOINT=true",
        ] {
            assert!(
                lines.iter().any(|line| line == expected),
                "missing {expected} in {lines:?}"
            );
        }
        assert!(lines
            .iter()
            .any(|line| line.starts_with("DB_PASSWORD=") && line.len() > 20));
    }

    #[test]
    fn env_lines_skip_services_without_ports() {
        let root = tempfile::tempdir().unwrap();
        let versions = BTreeMap::new();
        let context = ServiceContext {
            root: root.path(),
            project_id: "p1",
            versions: &versions,
            extensions: &[],
        };
        assert!(env_lines(&context, &ServiceName::ALL, &Ports::new())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn credentials_are_generated_once() {
        let root = tempfile::tempdir().unwrap();
        let versions = BTreeMap::new();
        let context = ServiceContext {
            root: root.path(),
            project_id: "p1",
            versions: &versions,
            extensions: &[],
        };
        std::fs::create_dir_all(context.data_dir()).unwrap();
        let ports = Ports::from([("postgres".to_string(), 1), ("rustfs_api".to_string(), 2)]);
        let first = env_lines(&context, &ServiceName::ALL, &ports).unwrap();
        assert_eq!(env_lines(&context, &ServiceName::ALL, &ports).unwrap(), first);
    }
}
