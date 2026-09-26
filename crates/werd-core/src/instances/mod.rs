//! Service instances: user-created, shared PostgreSQL, MySQL, Redis, … servers.
//!
//! An instance is one product line (e.g. PostgreSQL 17) with a name, a port,
//! an autostart flag and its own data in `<home>/services/<id>/`. Sites link to
//! instances and get their own database inside them.

mod mailpit;
mod meilisearch;
mod mongodb;
mod mysql;
mod postgresql;
mod redis;
mod rustfs;

use crate::catalog::{Catalog, Kind};
use crate::model::{Ports, ProjectStatus};
use crate::ports;
use crate::process::{tail_file, ManagedChild};
use crate::runtimes::{self, Installed};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub id: String,
    pub name: String,
    /// Catalog product, e.g. `postgresql`.
    pub product: String,
    /// Catalog line, e.g. `17`.
    pub line: String,
    pub port: u16,
    /// Secondary ports such as a web UI (`ui`), keyed by role.
    #[serde(default, skip_serializing_if = "Ports::is_empty")]
    pub extra_ports: Ports,
    #[serde(default)]
    pub autostart: bool,
    /// Optional extensions, e.g. `pgvector`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub status: ProjectStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Login details shown to the user and written into `.env`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// Where one instance lives on disk.
pub(crate) struct InstanceContext<'a> {
    pub root: &'a Path,
    pub instance: &'a ServiceInstance,
}

impl InstanceContext<'_> {
    pub fn dir(&self) -> PathBuf {
        instance_dir(self.root, &self.instance.id)
    }

    /// The server's data directory.
    pub fn data(&self) -> PathBuf {
        self.dir().join("data")
    }

    pub fn install_dir(&self) -> Result<PathBuf> {
        let label = self.label();
        let line = runtimes::resolve_line(
            self.root,
            &self.instance.product,
            Some(&self.instance.line),
            &label,
        )?;
        Ok(runtimes::line_dir(self.root, &self.instance.product, &line))
    }

    /// An executable of the installed line (`relative` without `.exe`).
    pub fn binary(&self, relative: &str) -> Result<PathBuf> {
        let path = self.install_dir()?.join(runtimes::exe(relative));
        if !path.is_file() {
            bail!("{} is incomplete: {} is missing", self.label(), path.display());
        }
        Ok(path)
    }

    pub fn label(&self) -> String {
        format!("{} {}", driver_label(&self.instance.product), self.instance.line)
    }

    pub fn extra_port(&self, role: &str) -> Result<u16> {
        self.instance
            .extra_ports
            .get(role)
            .copied()
            .with_context(|| format!("No {role} port assigned"))
    }

    /// Stored credentials, created on first use.
    pub fn credentials(&self, username: &str) -> Result<Credentials> {
        let path = self.dir().join("credentials.json");
        if let Ok(bytes) = fs::read(&path) {
            return Ok(serde_json::from_slice(&bytes)?);
        }
        fs::create_dir_all(self.dir())?;
        let credentials = Credentials {
            username: username.into(),
            password: Uuid::new_v4().simple().to_string(),
        };
        fs::write(&path, serde_json::to_vec_pretty(&credentials)?)?;
        Ok(credentials)
    }
}

pub(crate) trait Driver: Sync {
    /// Secondary ports with their preferred defaults, e.g. `[("ui", 8025)]`.
    fn extra_ports(&self) -> &'static [(&'static str, u16)] {
        &[]
    }

    /// Initializes data on first start, then starts the server and pushes its
    /// process onto `children`, even when a later setup step fails.
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()>;

    /// Asks the server to stop cleanly before its process is killed.
    fn shutdown(&self, _context: &InstanceContext) {}

    /// Creates a database for a site; only database servers support it.
    fn create_database(&self, context: &InstanceContext, _name: &str) -> Result<()> {
        bail!("{} has no databases", context.label())
    }

    /// `KEY=value` lines for a Laravel `.env`.
    fn env(&self, context: &InstanceContext, database: Option<&str>) -> Result<Vec<String>>;

    /// Login details to show in the app, if the server has any.
    fn credentials(&self, _context: &InstanceContext) -> Result<Option<Credentials>> {
        Ok(None)
    }

    /// Web UI of the server, if it has one.
    fn web_ui(&self, _context: &InstanceContext) -> Option<String> {
        None
    }
}

pub(crate) fn driver(product: &str) -> Result<&'static dyn Driver> {
    Ok(match product {
        "postgresql" => &postgresql::Postgresql,
        "mysql" => &mysql::MYSQL,
        "mariadb" => &mysql::MARIADB,
        "redis" => &redis::Redis,
        "mailpit" => &mailpit::Mailpit,
        "rustfs" => &rustfs::Rustfs,
        "meilisearch" => &meilisearch::Meilisearch,
        "mongodb" => &mongodb::Mongodb,
        other => bail!("Werd cannot run {other} as a service yet"),
    })
}

fn driver_label(product: &str) -> &str {
    match product {
        "postgresql" => "PostgreSQL",
        "mysql" => "MySQL",
        "mariadb" => "MariaDB",
        "redis" => "Redis",
        "mailpit" => "Mailpit",
        "rustfs" => "RustFS",
        "meilisearch" => "Meilisearch",
        "mongodb" => "MongoDB",
        other => other,
    }
}

pub(crate) fn instance_dir(root: &Path, id: &str) -> PathBuf {
    root.join("services").join(id)
}

/// Database names Werd creates: lowercase letters, digits and underscores.
pub(crate) fn database_name(value: &str) -> Result<String> {
    let name: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if name.is_empty() || name.len() > 63 {
        bail!("Invalid database name: {value}");
    }
    Ok(if name.as_bytes()[0].is_ascii_digit() {
        format!("db_{name}")
    } else {
        name
    })
}

// ---- Store -------------------------------------------------------------------

/// Instances and their running processes.
#[derive(Default)]
pub(crate) struct Instances {
    pub list: Vec<ServiceInstance>,
    pub processes: HashMap<String, Vec<ManagedChild>>,
}

impl Instances {
    fn file(root: &Path) -> PathBuf {
        root.join("services.json")
    }

    /// Loads instances; nothing runs after a daemon restart.
    pub fn load(root: &Path) -> Result<Self> {
        let path = Self::file(root);
        let mut list: Vec<ServiceInstance> = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("Corrupted {}", path.display()))?
        } else {
            Vec::new()
        };
        for instance in &mut list {
            instance.status = ProjectStatus::Stopped;
        }
        Ok(Self {
            list,
            processes: HashMap::new(),
        })
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let temporary = root.join("services.json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.list)?)?;
        fs::rename(temporary, Self::file(root))?;
        Ok(())
    }

    fn index(&self, id: &str) -> Result<usize> {
        self.list
            .iter()
            .position(|instance| instance.id == id)
            .context("Service not found")
    }

    pub fn get(&self, id: &str) -> Result<&ServiceInstance> {
        Ok(&self.list[self.index(id)?])
    }

    fn ports_in_use(&self) -> Vec<u16> {
        self.list
            .iter()
            .flat_map(|instance| std::iter::once(instance.port).chain(instance.extra_ports.values().copied()))
            .collect()
    }

    /// `preferred` when free, otherwise the next free port above it.
    pub fn suggest_port(&self, preferred: u16) -> u16 {
        let taken = self.ports_in_use();
        (preferred..=u16::MAX)
            .find(|port| !taken.contains(port) && TcpListener::bind(("127.0.0.1", *port)).is_ok())
            .unwrap_or(preferred)
    }
}

// ---- Lifecycle -----------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct CreateRequest {
    pub product: String,
    pub line: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub extensions: Vec<String>,
}

pub(crate) fn create(
    root: &Path,
    instances: &mut Instances,
    catalog: &Catalog,
    request: CreateRequest,
) -> Result<ServiceInstance> {
    let product = catalog.product(&request.product)?;
    if product.kind != Kind::Service {
        bail!("{} is not a service", product.label);
    }
    catalog.build(&request.product, &request.line)?;
    let driver = driver(&request.product)?;
    if let Some(unknown) = request
        .extensions
        .iter()
        .find(|name| !(request.product == "postgresql" && *name == "pgvector"))
    {
        bail!("Unknown extension for {}: {unknown}", product.label);
    }

    let default_port = product.default_port.unwrap_or(10_000);
    let port = request
        .port
        .unwrap_or_else(|| instances.suggest_port(default_port));
    if instances.ports_in_use().contains(&port) {
        bail!("Port {port} is already used by another Werd service");
    }
    let mut extra_ports = Ports::new();
    for (role, preferred) in driver.extra_ports() {
        let mut candidate = instances.suggest_port(*preferred);
        while candidate == port || extra_ports.values().any(|taken| *taken == candidate) {
            candidate = instances.suggest_port(candidate + 1);
        }
        extra_ports.insert((*role).into(), candidate);
    }

    let name = request
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let instance = ServiceInstance {
        id: Uuid::new_v4().to_string(),
        name: name.unwrap_or_else(|| format!("{} {}", product.label, request.line)),
        product: request.product,
        line: request.line,
        port,
        extra_ports,
        autostart: request.autostart,
        extensions: request.extensions,
        status: ProjectStatus::Stopped,
        error: None,
    };
    fs::create_dir_all(instance_dir(root, &instance.id))?;
    instances.list.push(instance.clone());
    instances.save(root)?;
    Ok(instance)
}

pub(crate) fn start(root: &Path, instances: &mut Instances, id: &str) -> Result<ServiceInstance> {
    let index = instances.index(id)?;
    if instances.list[index].status == ProjectStatus::Running {
        return Ok(instances.list[index].clone());
    }
    let instance = instances.list[index].clone();
    let context = InstanceContext {
        root,
        instance: &instance,
    };
    let result = (|| {
        if Installed::load(root)?
            .version(&instance.product, &instance.line)
            .is_none()
        {
            bail!("{} is not installed. Install it first.", context.label());
        }
        let mut wanted: Ports = instance.extra_ports.clone();
        wanted.insert("port".into(), instance.port);
        ports::ensure_available(&wanted)?;
        let mut children = Vec::new();
        let driver = driver(&instance.product)?;
        if let Err(error) = driver.start(&context, &mut children) {
            stop_children(&context, driver, &mut children);
            return Err(error);
        }
        Ok(children)
    })();
    let instance = &mut instances.list[index];
    match result {
        Ok(children) => {
            instances.processes.insert(id.into(), children);
            instance.status = ProjectStatus::Running;
            instance.error = None;
        }
        Err(error) => {
            instance.status = ProjectStatus::Error;
            instance.error = Some(format!("{error:#}"));
            instances.save(root)?;
            return Err(error);
        }
    }
    let updated = instances.list[index].clone();
    instances.save(root)?;
    Ok(updated)
}

fn stop_children(context: &InstanceContext, driver: &dyn Driver, children: &mut Vec<ManagedChild>) {
    if !children.is_empty() {
        driver.shutdown(context);
    }
    for child in children.iter_mut().rev() {
        child.kill();
    }
    children.clear();
}

/// Stops the processes of an instance, keeping its data.
pub(crate) fn stop_processes(root: &Path, instances: &mut Instances, id: &str) {
    if let (Some(mut children), Ok(instance)) = (instances.processes.remove(id), instances.get(id).cloned()) {
        if let Ok(driver) = driver(&instance.product) {
            stop_children(
                &InstanceContext {
                    root,
                    instance: &instance,
                },
                driver,
                &mut children,
            );
        }
    }
}

pub(crate) fn stop(root: &Path, instances: &mut Instances, id: &str) -> Result<ServiceInstance> {
    let index = instances.index(id)?;
    stop_processes(root, instances, id);
    instances.list[index].status = ProjectStatus::Stopped;
    instances.save(root)?;
    Ok(instances.list[index].clone())
}

/// Removes an instance. Its data folder is deleted unless `keep_data` is set.
pub(crate) fn delete(root: &Path, instances: &mut Instances, id: &str, keep_data: bool) -> Result<()> {
    let index = instances.index(id)?;
    stop_processes(root, instances, id);
    instances.list.remove(index);
    instances.save(root)?;
    if !keep_data {
        let dir = instance_dir(root, id);
        if dir.exists() {
            fs::remove_dir_all(&dir).with_context(|| format!("Cannot remove {}", dir.display()))?;
        }
    }
    Ok(())
}

pub(crate) fn set_autostart(
    root: &Path,
    instances: &mut Instances,
    id: &str,
    autostart: bool,
) -> Result<ServiceInstance> {
    let index = instances.index(id)?;
    instances.list[index].autostart = autostart;
    instances.save(root)?;
    Ok(instances.list[index].clone())
}

/// Renames an instance. Names stay unique so the CLI can find services by name.
pub(crate) fn rename(
    root: &Path,
    instances: &mut Instances,
    id: &str,
    name: &str,
) -> Result<ServiceInstance> {
    let index = instances.index(id)?;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        bail!("A service name needs 1 to 80 characters");
    }
    if instances
        .list
        .iter()
        .any(|other| other.id != id && other.name.eq_ignore_ascii_case(name))
    {
        bail!("Another service is already called {name}");
    }
    instances.list[index].name = name.to_string();
    instances.save(root)?;
    Ok(instances.list[index].clone())
}

/// Details for the app: credentials, web UI and `.env` lines.
pub(crate) fn details(root: &Path, instances: &Instances, id: &str) -> Result<serde_json::Value> {
    let instance = instances.get(id)?;
    let context = InstanceContext { root, instance };
    let driver = driver(&instance.product)?;
    Ok(serde_json::json!({
        "instance": instance,
        "credentials": driver.credentials(&context)?,
        "web_ui": driver.web_ui(&context),
        "env": driver.env(&context, None)?.join("\n"),
    }))
}

/// Creates a database (or does nothing if it exists) in a running instance.
pub(crate) fn create_database(root: &Path, instances: &Instances, id: &str, name: &str) -> Result<String> {
    let instance = instances.get(id)?;
    if !instances.processes.contains_key(id) {
        bail!("Start {} before creating a database", instance.name);
    }
    let name = database_name(name)?;
    driver(&instance.product)?.create_database(&InstanceContext { root, instance }, &name)?;
    Ok(name)
}

/// `.env` lines to reach an instance, using `database` as the site database or bucket.
pub(crate) fn env_for(
    root: &Path,
    instance: &ServiceInstance,
    database: Option<&str>,
) -> Result<Vec<String>> {
    driver(&instance.product)?.env(&InstanceContext { root, instance }, database)
}

/// Instances as shown in lists, each with its web UI address when it has one.
pub(crate) fn summaries(root: &Path, instances: &Instances) -> Vec<serde_json::Value> {
    instances
        .list
        .iter()
        .map(|instance| {
            let web_ui = driver(&instance.product)
                .ok()
                .and_then(|driver| driver.web_ui(&InstanceContext { root, instance }));
            let mut value = serde_json::json!(instance);
            value["web_ui"] = serde_json::json!(web_ui);
            value
        })
        .collect()
}

/// Service products available on this platform, grouped for the "Add service" dialog.
pub(crate) fn offerings(root: &Path, catalog: &Catalog) -> Result<Vec<serde_json::Value>> {
    let installed = Installed::load(root)?;
    let mut products = Vec::new();
    for (id, product) in &catalog.products {
        if product.kind != Kind::Service || driver(id).is_err() {
            continue;
        }
        let lines: Vec<serde_json::Value> = product
            .available_lines()
            .into_iter()
            .map(|(line, entry)| {
                serde_json::json!({
                    "line": line,
                    "latest": entry.latest,
                    "lts": entry.lts,
                    "eol": entry.eol,
                    "installed": installed.version(id, line),
                })
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        products.push(serde_json::json!({
            "product": id,
            "label": product.label,
            "categories": product.categories,
            "default_port": product.default_port,
            "extensions": if id == "postgresql" && catalog.products.contains_key("pgvector") { vec!["pgvector"] } else { vec![] },
            "lines": lines,
        }));
    }
    Ok(products)
}

pub(crate) fn logs(root: &Path, instances: &Instances, id: &str) -> Result<Vec<String>> {
    let instance = instances.get(id)?;
    tail_file(&instance_dir(root, id).join(format!("{}.log", instance.product)))
}

/// Instances whose runtime line is `product`/`line` and that are running.
pub(crate) fn using_runtime(instances: &Instances, product: &str, line: &str) -> Vec<String> {
    instances
        .list
        .iter()
        .filter(|instance| instances.processes.contains_key(&instance.id))
        .filter(|instance| {
            (instance.product == product && instance.line == line)
                || (product == "pgvector" && instance.extensions.iter().any(|name| name == "pgvector"))
        })
        .map(|instance| instance.name.clone())
        .collect()
}

/// Marks instances whose server died and cleans up their processes.
pub(crate) fn reap_exited(root: &Path, instances: &mut Instances) {
    let mut failed = Vec::new();
    for (id, children) in &mut instances.processes {
        if let Some(exit) = children.iter_mut().find_map(ManagedChild::has_exited) {
            failed.push((id.clone(), exit));
        }
    }
    for (id, exit) in failed {
        stop_processes(root, instances, &id);
        if let Ok(index) = instances.index(&id) {
            instances.list[index].status = ProjectStatus::Error;
            instances.list[index].error = Some(format!("The server exited: {exit}"));
        }
        let _ = instances.save(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let zero = "0".repeat(64);
        let build = |marker: &str| {
            serde_json::json!({ crate::catalog::PLATFORM: {
            "url": "https://example.test/x.zip", "sha256": zero, "format": "zip", "marker": marker } })
        };
        Catalog::parse(
            &serde_json::json!({ "schema": 1, "products": {
                "postgresql": { "label": "PostgreSQL", "kind": "service", "categories": ["database"], "default_port": 55432,
                    "lines": { "17": { "latest": "17.5", "builds": build("bin/postgres.exe") } } },
                "mailpit": { "label": "Mailpit", "kind": "service", "categories": ["mail"], "default_port": 51025,
                    "lines": { "1": { "latest": "1.31.2", "builds": build("mailpit.exe") } } },
                "php": { "label": "PHP", "kind": "runtime", "lines": { "8.4": { "latest": "8.4.26", "builds": build("php-cgi.exe") } } }
            }})
            .to_string(),
        )
        .unwrap()
    }

    fn request(product: &str, line: &str) -> CreateRequest {
        CreateRequest {
            product: product.into(),
            line: line.into(),
            name: None,
            port: None,
            autostart: false,
            extensions: vec![],
        }
    }

    #[test]
    fn create_assigns_names_ports_and_persists() {
        let root = tempfile::tempdir().unwrap();
        let mut instances = Instances::default();
        let first = create(
            root.path(),
            &mut instances,
            &catalog(),
            request("postgresql", "17"),
        )
        .unwrap();
        assert_eq!(first.name, "PostgreSQL 17");
        let second = create(
            root.path(),
            &mut instances,
            &catalog(),
            request("postgresql", "17"),
        )
        .unwrap();
        assert_ne!(
            first.port, second.port,
            "a second instance gets the next free port"
        );
        let mailpit = create(root.path(), &mut instances, &catalog(), request("mailpit", "1")).unwrap();
        assert!(mailpit.extra_ports.contains_key("ui"));

        let loaded = Instances::load(root.path()).unwrap();
        assert_eq!(loaded.list.len(), 3);
        assert!(root.path().join("services").join(&first.id).is_dir());
    }

    #[test]
    fn instances_can_be_renamed_to_unique_names() {
        let root = tempfile::tempdir().unwrap();
        let mut instances = Instances::default();
        let postgres = create(
            root.path(),
            &mut instances,
            &catalog(),
            request("postgresql", "17"),
        )
        .unwrap();
        let mailpit = create(root.path(), &mut instances, &catalog(), request("mailpit", "1")).unwrap();
        let renamed = rename(root.path(), &mut instances, &postgres.id, "  Main database ").unwrap();
        assert_eq!(renamed.name, "Main database");
        assert!(rename(root.path(), &mut instances, &mailpit.id, "main DATABASE").is_err());
        assert!(rename(root.path(), &mut instances, &mailpit.id, "   ").is_err());
        assert_eq!(
            Instances::load(root.path()).unwrap().list[0].name,
            "Main database"
        );
    }

    #[test]
    fn create_rejects_bad_requests() {
        let root = tempfile::tempdir().unwrap();
        let mut instances = Instances::default();
        assert!(
            create(root.path(), &mut instances, &catalog(), request("php", "8.4")).is_err(),
            "runtimes are not services"
        );
        assert!(create(
            root.path(),
            &mut instances,
            &catalog(),
            request("postgresql", "9")
        )
        .is_err());
        let mut with_extension = request("mailpit", "1");
        with_extension.extensions = vec!["pgvector".into()];
        assert!(create(root.path(), &mut instances, &catalog(), with_extension).is_err());
        let first = create(
            root.path(),
            &mut instances,
            &catalog(),
            request("postgresql", "17"),
        )
        .unwrap();
        let mut clash = request("postgresql", "17");
        clash.port = Some(first.port);
        assert!(create(root.path(), &mut instances, &catalog(), clash)
            .unwrap_err()
            .to_string()
            .contains("already used"));
    }

    #[test]
    fn start_requires_the_runtime_and_delete_can_keep_data() {
        let root = tempfile::tempdir().unwrap();
        let mut instances = Instances::default();
        let instance = create(
            root.path(),
            &mut instances,
            &catalog(),
            request("postgresql", "17"),
        )
        .unwrap();
        let error = start(root.path(), &mut instances, &instance.id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("PostgreSQL 17 is not installed"), "{error}");
        assert_eq!(instances.get(&instance.id).unwrap().status, ProjectStatus::Error);

        fs::write(instance_dir(root.path(), &instance.id).join("keep.txt"), "data").unwrap();
        delete(root.path(), &mut instances, &instance.id, true).unwrap();
        assert!(instance_dir(root.path(), &instance.id).join("keep.txt").is_file());
        assert!(instances.list.is_empty());
    }

    #[test]
    fn database_names_are_sanitized() {
        assert_eq!(database_name("My Shop-2").unwrap(), "my_shop_2");
        assert_eq!(database_name("2fa").unwrap(), "db_2fa");
        assert!(database_name("---").is_err());
    }
}
