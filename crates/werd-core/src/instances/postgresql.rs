use super::{Credentials, Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use crate::runtimes;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

pub(super) struct Postgresql;

const USER: &str = "werd";

fn psql(
    context: &InstanceContext,
    credentials: &Credentials,
    database: &str,
    args: &[&str],
) -> Result<String> {
    let output = hidden_command(context.binary("bin/psql")?)
        .args([
            "-h",
            "127.0.0.1",
            "-p",
            &context.instance.port.to_string(),
            "-U",
            USER,
            "-d",
            database,
        ])
        .args(args)
        .env("PGPASSWORD", &credentials.password)
        .output()
        .context("psql did not start")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn uses_pgvector(context: &InstanceContext) -> bool {
    context.instance.extensions.iter().any(|name| name == "pgvector")
}

fn validate_data(data: &Path, line: &str) -> Result<()> {
    let marker = data.join("PG_VERSION");
    let existing = fs::read_to_string(&marker).with_context(|| {
        format!(
            "Cannot read {}; existing PostgreSQL data was left untouched. Restore PG_VERSION from a backup or restore the cluster before starting it",
            marker.display()
        )
    })?;
    if existing.trim() != line {
        bail!(
            "This data was created with PostgreSQL {}; it cannot run on PostgreSQL {}",
            existing.trim(),
            line
        );
    }
    Ok(())
}

/// Initialize only a new staging directory. Existing data is never a cleanup target.
fn ensure_data(data: &Path, line: &str, initialize: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    match fs::symlink_metadata(data) {
        Ok(_) => return validate_data(data, line),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("Cannot inspect {}", data.display())),
    }
    let parent = data.parent().context("Invalid PostgreSQL data directory")?;
    let staging = parent.join(format!(".initdb-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let result = initialize(&staging)
        .and_then(|()| validate_data(&staging, line))
        .and_then(|()| {
            // Never replace a directory created while initialization was running.
            match fs::symlink_metadata(data) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => bail!(
                    "{} appeared during initialization; it was left untouched",
                    data.display()
                ),
                Err(error) => return Err(error.into()),
            }
            fs::rename(&staging, data)
                .with_context(|| format!("Cannot publish PostgreSQL data at {}", data.display()))
        });
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

impl Driver for Postgresql {
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let install = context.install_dir()?;
        let data = context.data();
        let credentials = context.credentials(USER)?;

        ensure_data(&data, &context.instance.line, |staging| {
            let initdb = context.binary("bin/initdb")?;
            let password_file = context.dir().join("initdb-password.tmp");
            fs::write(&password_file, &credentials.password)?;
            let output = hidden_command(initdb)
                .arg("-D")
                .arg(staging)
                .args([
                    "-U",
                    USER,
                    "-E",
                    "UTF8",
                    "--auth-host=scram-sha-256",
                    "--auth-local=trust",
                    "--pwfile",
                ])
                .arg(&password_file)
                .output()
                .context("initdb did not start");
            let _ = fs::remove_file(&password_file);
            let output = output?;
            if !output.status.success() {
                bail!("initdb failed: {}", String::from_utf8_lossy(&output.stderr));
            }
            Ok(())
        })?;
        if uses_pgvector(context) && !runtimes::has_pgvector(&install) {
            bail!(
                "pgvector is not installed for PostgreSQL {}; install pgvector first",
                context.instance.line
            );
        }

        let port = context.instance.port;
        let mut command = hidden_command(context.binary("bin/postgres")?);
        command
            .arg("-D")
            .arg(&data)
            .args(["-h", "127.0.0.1", "-p", &port.to_string()]);
        children.push(spawn_ready(
            &context.dir(),
            "postgresql",
            command,
            port,
            &context.label(),
        )?);
        Ok(())
    }

    fn shutdown(&self, context: &InstanceContext) {
        if let Ok(pg_ctl) = context.binary("bin/pg_ctl") {
            let _ = hidden_command(pg_ctl)
                .arg("-D")
                .arg(context.data())
                .args(["stop", "-m", "fast", "-w"])
                .status();
        }
    }

    fn create_database(&self, context: &InstanceContext, name: &str) -> Result<()> {
        let credentials = context.credentials(USER)?;
        let exists = psql(
            context,
            &credentials,
            "postgres",
            &[
                "-tAc",
                &format!("SELECT 1 FROM pg_database WHERE datname='{name}'"),
            ],
        )?;
        if exists.trim() != "1" {
            psql(
                context,
                &credentials,
                "postgres",
                &["-c", &format!("CREATE DATABASE \"{name}\"")],
            )
            .with_context(|| format!("Creating database {name} failed"))?;
        }
        if uses_pgvector(context) {
            psql(
                context,
                &credentials,
                name,
                &["-c", "CREATE EXTENSION IF NOT EXISTS vector"],
            )
            .context("Enabling pgvector failed")?;
        }
        Ok(())
    }

    fn env(&self, context: &InstanceContext, database: Option<&str>) -> Result<Vec<String>> {
        let credentials = context.credentials(USER)?;
        Ok(vec![
            "DB_CONNECTION=pgsql".into(),
            "DB_HOST=127.0.0.1".into(),
            format!("DB_PORT={}", context.instance.port),
            format!("DB_DATABASE={}", database.unwrap_or("postgres")),
            format!("DB_USERNAME={}", credentials.username),
            format!("DB_PASSWORD={}", credentials.password),
        ])
    }

    fn credentials(&self, context: &InstanceContext) -> Result<Option<Credentials>> {
        context.credentials(USER).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_data_with_missing_or_unreadable_version_is_never_initialized() {
        for marker in [None, Some(vec![0xff]), Some(b"16\n".to_vec())] {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("data");
            fs::create_dir(&data).unwrap();
            fs::write(data.join("important.db"), "keep my data").unwrap();
            if let Some(bytes) = marker {
                fs::write(data.join("PG_VERSION"), bytes).unwrap();
            }
            let error =
                ensure_data(&data, "17", |_| panic!("existing data must not run initdb")).unwrap_err();
            assert!(
                error.to_string().contains("left untouched")
                    || error.to_string().contains("cannot run on PostgreSQL 17"),
                "{error:#}"
            );
            assert_eq!(
                fs::read_to_string(data.join("important.db")).unwrap(),
                "keep my data"
            );
        }
    }

    #[test]
    fn existing_empty_directory_and_directory_marker_are_left_untouched() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        fs::create_dir(&data).unwrap();
        assert!(ensure_data(&data, "17", |_| panic!(
            "must not initialize an existing directory"
        ))
        .is_err());
        fs::create_dir(data.join("PG_VERSION")).unwrap();
        assert!(ensure_data(&data, "17", |_| panic!(
            "must not initialize an unreadable marker"
        ))
        .is_err());
        assert!(data.join("PG_VERSION").is_dir());
    }

    #[test]
    fn existing_matching_cluster_is_reused_without_initialization() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("PG_VERSION"), "17\n").unwrap();
        ensure_data(&data, "17", |_| panic!("a valid cluster must not run initdb")).unwrap();
    }

    #[test]
    fn new_cluster_is_published_only_after_successful_initialization() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        ensure_data(&data, "17", |staging| {
            assert_ne!(staging, data);
            assert!(!data.exists());
            fs::write(staging.join("PG_VERSION"), "17\n")?;
            fs::write(staging.join("important.db"), "new data")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read_to_string(data.join("important.db")).unwrap(), "new data");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_initialization_cleans_only_its_staging_directory_and_can_retry() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let error = ensure_data(&data, "17", |staging| {
            fs::write(staging.join("partial"), "incomplete")?;
            bail!("fake initdb failed")
        })
        .unwrap_err();
        assert!(error.to_string().contains("fake initdb failed"));
        assert!(!data.exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
        ensure_data(&data, "17", |staging| {
            fs::write(staging.join("PG_VERSION"), "17")?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_directory_created_during_initialization_is_not_replaced() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let error = ensure_data(&data, "17", |staging| {
            fs::write(staging.join("PG_VERSION"), "17")?;
            fs::create_dir(&data)?;
            fs::write(data.join("important.db"), "someone else's data")?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("left untouched"));
        assert_eq!(
            fs::read_to_string(data.join("important.db")).unwrap(),
            "someone else's data"
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
