//! One-way upgrades of the data folder, run when the daemon starts.

use crate::instances::{instance_dir, Credentials, Instances, ServiceInstance};
use crate::model::{Link, Ports, Project, ProjectStatus, ServiceName};
use crate::paths::{project_dir, state_file};
use crate::runtimes::{self, Installed};
use crate::settings::{Settings, LAYOUT_VERSION};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub fn run(root: &Path) -> Result<()> {
    let mut settings = Settings::load(root)?;
    if settings.layout_version >= LAYOUT_VERSION {
        return Ok(());
    }
    if settings.layout_version < 2 {
        layout_v2(root, &mut settings)?;
    }
    if settings.layout_version < 3 {
        layout_v3(root)?;
    }
    settings.layout_version = LAYOUT_VERSION;
    settings.save(root)
}

/// 0.1 kept one pinned version per runtime under version-named folders
/// (`caddy/2.11.4`, `postgres/18`, …). 0.2 uses `<product>/<line>` plus `installed.json`.
fn layout_v2(root: &Path, settings: &mut Settings) -> Result<()> {
    let runtimes_dir = root.join("runtimes");
    // (old folder, product, line, version pinned by 0.1, marker)
    let moves = [
        ("php/8.5", "php", "8.5", "8.5.11", "php-cgi.exe"),
        ("caddy/2.11.4", "caddy", "2", "2.11.4", "caddy.exe"),
        ("postgres/18", "postgresql", "18", "18.6", "bin/postgres.exe"),
        ("redis/7.2", "redis", "7.2", "7.2.8", "redis-server.exe"),
        ("mailpit/1.31.2", "mailpit", "1", "1.31.2", "mailpit.exe"),
        ("rustfs/1.0.0", "rustfs", "1", "1.0.0", "rustfs.exe"),
    ];
    let mut installed = Installed::load(root)?;
    for (old, product, line, version, marker) in moves {
        let from = runtimes_dir.join(old);
        let to = runtimes::line_dir(root, product, line);
        if !from.join(marker).is_file() {
            continue;
        }
        if from != to {
            fs::create_dir_all(to.parent().context("Invalid runtime path")?)?;
            fs::rename(&from, &to)
                .with_context(|| format!("Cannot move {} to {}", from.display(), to.display()))?;
        }
        installed.set(product, line, version);
    }
    // 0.1 stored the pgvector source under the runtimes folder.
    let old_vector = runtimes_dir.join("sources/pgvector/0.8.6");
    if old_vector.join("Makefile.win").is_file() {
        let to = runtimes::line_dir(root, "pgvector", "0.8");
        fs::create_dir_all(to.parent().context("Invalid runtime path")?)?;
        fs::rename(&old_vector, &to)?;
        installed.set("pgvector", "0.8", "0.8.6");
        let _ = fs::remove_dir_all(runtimes_dir.join("sources"));
    }
    // Remove empty version-named parents left behind (`runtimes/postgres`, …).
    for family in ["postgres", "caddy/2.11.4", "mailpit/1.31.2", "rustfs/1.0.0"] {
        let _ = fs::remove_dir(runtimes_dir.join(family));
    }
    installed.save(root)?;

    // Old downloads were named `<id>-<version>.zip`; 0.2 caches by checksum.
    if let Ok(entries) = fs::read_dir(root.join("downloads")) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().ends_with(".zip") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    if installed.version("php", "8.5").is_some() && settings.default_php.is_none() {
        settings.default_php = Some("8.5".into());
    }
    runtimes::write_all_php_ini(root, settings)?;
    Ok(())
}

/// 0.1 and 0.2 started private copies of PostgreSQL, Redis, Mailpit and RustFS for
/// every site. 0.3 uses shared instances: each old copy becomes an instance named
/// after its site, keeping its data, credentials and ports, and the site links to it.
fn layout_v3(root: &Path) -> Result<()> {
    let path = state_file(root);
    if !path.exists() {
        return Ok(());
    }
    let mut projects: Vec<Project> =
        serde_json::from_slice(&fs::read(&path)?).with_context(|| format!("Corrupted {}", path.display()))?;
    let mut instances = Instances::load(root)?;
    let installed = Installed::load(root)?;

    for project in &mut projects {
        upgrade_project(project);
        let old_ports = project.ports.clone().unwrap_or_default();
        let old_dir = project_dir(root, &project.id);
        for service in std::mem::take(&mut project.services) {
            let product = service.product();
            let line = project
                .versions
                .get(product)
                .cloned()
                .or_else(|| installed.lines(product).into_iter().next())
                .unwrap_or_else(|| {
                    if product == "redis" {
                        "7.2".into()
                    } else {
                        "1".into()
                    }
                });
            let id = uuid::Uuid::new_v4().to_string();
            let dir = instance_dir(root, &id);
            let data = dir.join("data");
            fs::create_dir_all(&dir)?;
            let port_of = |key: &str, fallback: u16, instances: &Instances| {
                old_ports
                    .get(key)
                    .copied()
                    .unwrap_or_else(|| instances.suggest_port(fallback))
            };
            let move_path = |from: &Path, to: &Path| -> Result<()> {
                if from.exists() {
                    if let Some(parent) = to.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(from, to).with_context(|| format!("Cannot move {}", from.display()))?;
                }
                Ok(())
            };
            let write_credentials = |username: &str, password: &str| -> Result<()> {
                let credentials = Credentials {
                    username: username.into(),
                    password: password.into(),
                };
                fs::write(
                    dir.join("credentials.json"),
                    serde_json::to_vec_pretty(&credentials)?,
                )?;
                Ok(())
            };

            let mut extra_ports = Ports::new();
            let (category, port, database, extensions) = match service {
                ServiceName::Postgres => {
                    move_path(&old_dir.join("postgres"), &data)?;
                    if let Ok(password) = fs::read_to_string(old_dir.join("postgres-password.txt")) {
                        write_credentials("werd", password.trim())?;
                    }
                    // 0.1 created a database called "app"; keep using it.
                    (
                        "database",
                        port_of("postgres", 5432, &instances),
                        Some("app".to_string()),
                        project.extensions.clone(),
                    )
                }
                ServiceName::Redis => {
                    move_path(&old_dir.join("redis"), &data)?;
                    ("cache", port_of("redis", 6379, &instances), None, Vec::new())
                }
                ServiceName::Mailpit => {
                    move_path(&old_dir.join("mailpit.db"), &data.join("mailpit.db"))?;
                    extra_ports.insert("ui".into(), port_of("mailpit_ui", 8025, &instances));
                    (
                        "mail",
                        port_of("mailpit_smtp", 1025, &instances),
                        None,
                        Vec::new(),
                    )
                }
                ServiceName::Rustfs => {
                    move_path(&old_dir.join("rustfs"), &data)?;
                    if let Ok(bytes) = fs::read(old_dir.join("rustfs-credentials.json")) {
                        let keys: serde_json::Value = serde_json::from_slice(&bytes)?;
                        write_credentials(
                            keys["access_key"].as_str().unwrap_or_default(),
                            keys["secret_key"].as_str().unwrap_or_default(),
                        )?;
                    }
                    extra_ports.insert("console".into(), port_of("rustfs_console", 9001, &instances));
                    // 0.1 told Laravel to use the "werd" bucket.
                    (
                        "storage",
                        port_of("rustfs_api", 9000, &instances),
                        Some("werd".to_string()),
                        Vec::new(),
                    )
                }
            };
            let label = match service {
                ServiceName::Postgres => "PostgreSQL",
                ServiceName::Redis => "Redis",
                ServiceName::Mailpit => "Mailpit",
                ServiceName::Rustfs => "RustFS",
            };
            instances.list.push(ServiceInstance {
                id: id.clone(),
                name: format!("{} {label}", project.name),
                product: product.into(),
                line,
                port,
                extra_ports,
                autostart: false,
                extensions,
                status: ProjectStatus::Stopped,
                error: None,
            });
            project.links.insert(
                category.into(),
                Link {
                    instance: id,
                    database,
                },
            );
        }
        project.versions.clear();
        project.extensions.clear();
        if let Some(ports) = project.ports.as_mut() {
            ports.retain(|role, _| role == "site" || role == "fastcgi");
        }
    }

    instances.save(root)?;
    let temporary = root.join("state.json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&projects)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

/// Projects saved by 0.1 had no per-product versions: they always used
/// PostgreSQL 18 with pgvector and Redis 7.2.
pub(crate) fn upgrade_project(project: &mut Project) {
    if !project.versions.is_empty() || !project.extensions.is_empty() {
        return;
    }
    if project.uses(ServiceName::Postgres) {
        project.versions.insert("postgresql".into(), "18".into());
        project.extensions.push("pgvector".into());
    }
    if project.uses(ServiceName::Redis) {
        project.versions.insert("redis".into(), "7.2".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    #[test]
    fn moves_0_1_runtimes_into_product_lines() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        touch(&home.join("runtimes/php/8.5/php-cgi.exe"));
        touch(&home.join("runtimes/caddy/2.11.4/caddy.exe"));
        touch(&home.join("runtimes/postgres/18/bin/postgres.exe"));
        touch(&home.join("runtimes/postgres/18/lib/vector.dll"));
        touch(&home.join("runtimes/mailpit/1.31.2/mailpit.exe"));
        touch(&home.join("runtimes/sources/pgvector/0.8.6/Makefile.win"));
        touch(&home.join("downloads/php-8.5.11.zip"));

        run(home).unwrap();

        let installed = Installed::load(home).unwrap();
        assert_eq!(installed.version("php", "8.5"), Some("8.5.11"));
        assert_eq!(installed.version("caddy", "2"), Some("2.11.4"));
        assert_eq!(installed.version("postgresql", "18"), Some("18.6"));
        assert_eq!(installed.version("mailpit", "1"), Some("1.31.2"));
        assert_eq!(installed.version("pgvector", "0.8"), Some("0.8.6"));
        assert_eq!(
            installed.version("redis", "7.2"),
            None,
            "missing runtimes are skipped"
        );
        assert!(home.join("runtimes/postgresql/18/lib/vector.dll").is_file());
        assert!(home.join("runtimes/caddy/2/caddy.exe").is_file());
        assert!(!home.join("runtimes/postgres").exists());
        assert!(!home.join("runtimes/sources").exists());
        assert!(!home.join("downloads/php-8.5.11.zip").exists());
        assert!(home.join("runtimes/php/8.5/php.ini").is_file());

        let settings = Settings::load(home).unwrap();
        assert_eq!(settings.layout_version, LAYOUT_VERSION);
        assert_eq!(settings.default_php.as_deref(), Some("8.5"));
        run(home).unwrap(); // idempotent
    }

    #[test]
    fn per_site_services_become_linked_instances_with_their_data() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        Settings {
            layout_version: 2,
            ..Settings::default()
        }
        .save(home)
        .unwrap();
        fs::write(
            state_file(home),
            r#"[{"id":"p1","name":"shop","path":"/work/shop","php":"8.5",
                "services":["postgres","mailpit"],"versions":{"postgresql":"18"},"extensions":["pgvector"],
                "ports":{"site":4430,"fastcgi":4431,"postgres":6543,"mailpit_smtp":2025,"mailpit_ui":2026}}]"#,
        )
        .unwrap();
        touch(&home.join("projects/p1/postgres/PG_VERSION"));
        fs::write(home.join("projects/p1/postgres-password.txt"), "secret\n").unwrap();
        touch(&home.join("projects/p1/mailpit.db"));

        run(home).unwrap();

        let instances = Instances::load(home).unwrap();
        assert_eq!(instances.list.len(), 2);
        let postgres = instances.list.iter().find(|i| i.product == "postgresql").unwrap();
        assert_eq!(
            (postgres.name.as_str(), postgres.line.as_str(), postgres.port),
            ("shop PostgreSQL", "18", 6543)
        );
        assert_eq!(postgres.extensions, ["pgvector"]);
        assert!(
            instance_dir(home, &postgres.id).join("data/PG_VERSION").is_file(),
            "cluster moved"
        );
        let credentials: Credentials = serde_json::from_slice(
            &fs::read(instance_dir(home, &postgres.id).join("credentials.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            credentials,
            Credentials {
                username: "werd".into(),
                password: "secret".into()
            }
        );
        let mailpit = instances.list.iter().find(|i| i.product == "mailpit").unwrap();
        assert_eq!((mailpit.port, mailpit.extra_ports["ui"]), (2025, 2026));
        assert!(instance_dir(home, &mailpit.id).join("data/mailpit.db").is_file());

        let projects: Vec<Project> = serde_json::from_slice(&fs::read(state_file(home)).unwrap()).unwrap();
        let project = &projects[0];
        assert!(project.services.is_empty() && project.versions.is_empty() && project.extensions.is_empty());
        assert_eq!(
            project.links["database"],
            Link {
                instance: postgres.id.clone(),
                database: Some("app".into())
            }
        );
        assert_eq!(project.links["mail"].instance, mailpit.id);
        assert_eq!(
            project.ports.as_ref().unwrap().len(),
            2,
            "only the site ports stay on the site"
        );
        assert_eq!(Settings::load(home).unwrap().layout_version, LAYOUT_VERSION);
    }

    #[test]
    fn fresh_homes_only_record_the_layout_version() {
        let root = tempfile::tempdir().unwrap();
        run(root.path()).unwrap();
        assert_eq!(
            Settings::load(root.path()).unwrap().layout_version,
            LAYOUT_VERSION
        );
        assert!(Installed::load(root.path()).unwrap().0.is_empty());
    }
}
