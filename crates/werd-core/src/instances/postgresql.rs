use super::{Credentials, Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use crate::runtimes;
use anyhow::{bail, Context, Result};
use std::fs;

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

impl Driver for Postgresql {
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let install = context.install_dir()?;
        let data = context.data();
        let credentials = context.credentials(USER)?;

        if let Ok(existing) = fs::read_to_string(data.join("PG_VERSION")) {
            if existing.trim() != context.instance.line {
                bail!(
                    "This data was created with PostgreSQL {}; it cannot run on PostgreSQL {}",
                    existing.trim(),
                    context.instance.line
                );
            }
        } else {
            let password_file = context.dir().join("initdb-password.tmp");
            fs::write(&password_file, &credentials.password)?;
            let output = hidden_command(context.binary("bin/initdb")?)
                .arg("-D")
                .arg(&data)
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
                let _ = fs::remove_dir_all(&data);
                bail!("initdb failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
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
