//! One-way upgrades of the data folder, run when the daemon starts.

use crate::model::{Project, ServiceName};
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
