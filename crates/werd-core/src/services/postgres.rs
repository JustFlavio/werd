use super::{Service, ServiceContext};
use crate::model::Ports;
use crate::ports;
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use crate::runtimes;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub(super) struct Postgres;

const USER: &str = "werd";
const DATABASE: &str = "app";

/// Password of the `werd` superuser, generated on first use.
fn password(directory: &Path) -> Result<String> {
    let path = directory.join("postgres-password.txt");
    if path.exists() {
        return Ok(fs::read_to_string(path)?.trim().into());
    }
    fs::create_dir_all(directory)?;
    let password = Uuid::new_v4().simple().to_string();
    fs::write(path, format!("{password}\n"))?;
    Ok(password)
}

/// Runs one SQL statement with `psql`, returning its stdout.
fn psql(binary: &Path, port: u16, password: &str, database: &str, args: &[&str]) -> Result<String> {
    let output = hidden_command(binary)
        .args([
            "-h",
            "127.0.0.1",
            "-p",
            &port.to_string(),
            "-U",
            USER,
            "-d",
            database,
        ])
        .args(args)
        .env("PGPASSWORD", password)
        .output()
        .context("psql did not start")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

impl Service for Postgres {
    fn start(
        &self,
        context: &ServiceContext,
        ports: &mut Ports,
        children: &mut Vec<ManagedChild>,
    ) -> Result<()> {
        let install = context.runtime_dir("postgresql", "PostgreSQL")?;
        let postgres = context.binary("postgresql", "bin/postgres", "PostgreSQL")?;
        let initdb = context.binary("postgresql", "bin/initdb", "PostgreSQL")?;
        let psql_binary = context.binary("postgresql", "bin/psql", "PostgreSQL")?;
        let directory = context.data_dir();
        let cluster = directory.join("postgres");
        let password = password(&directory)?;

        // A data directory only works with the major version that created it.
        let line = install
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Ok(existing) = fs::read_to_string(cluster.join("PG_VERSION")) {
            if existing.trim() != line {
                bail!(
                    "This project's database was created with PostgreSQL {}; install that version or reset the database",
                    existing.trim()
                );
            }
        }
        let vector = context.extensions.iter().any(|extension| extension == "pgvector");

        if !cluster.join("PG_VERSION").exists() {
            let output = hidden_command(initdb)
                .arg("-D")
                .arg(&cluster)
                .args([
                    "-U",
                    USER,
                    "--auth-host=scram-sha-256",
                    "--auth-local=trust",
                    "--pwfile",
                ])
                .arg(directory.join("postgres-password.txt"))
                .output()
                .context("initdb did not start")?;
            if !output.status.success() {
                bail!("initdb failed: {}", String::from_utf8_lossy(&output.stderr));
            }
        }

        if vector && !runtimes::has_pgvector(&install) {
            bail!("pgvector is not installed for PostgreSQL {line}; install pgvector first");
        }

        let port = ports::assign(ports, "postgres")?;
        let mut command = hidden_command(postgres);
        command
            .arg("-D")
            .arg(&cluster)
            .args(["-h", "127.0.0.1", "-p", &port.to_string()]);
        children.push(spawn_ready(
            &context.data_dir(),
            "postgres",
            command,
            port,
            "PostgreSQL",
        )?);

        let exists = psql(
            &psql_binary,
            port,
            &password,
            "postgres",
            &[
                "-tc",
                &format!("SELECT 1 FROM pg_database WHERE datname='{DATABASE}'"),
            ],
        )
        .context("Checking the PostgreSQL database failed")?;
        if exists.trim() != "1" {
            psql(
                &psql_binary,
                port,
                &password,
                "postgres",
                &["-c", &format!("CREATE DATABASE {DATABASE}")],
            )
            .context("Creating the app database failed")?;
        }
        if vector {
            psql(
                &psql_binary,
                port,
                &password,
                DATABASE,
                &["-c", "CREATE EXTENSION IF NOT EXISTS vector"],
            )
            .context("Enabling pgvector failed")?;
        }
        Ok(())
    }

    fn shutdown(&self, context: &ServiceContext, _ports: &Ports) {
        let Ok(pg_ctl) = context.binary("postgresql", "bin/pg_ctl", "PostgreSQL") else {
            return;
        };
        let _ = hidden_command(pg_ctl)
            .arg("-D")
            .arg(context.data_dir().join("postgres"))
            .args(["stop", "-m", "fast", "-w"])
            .status();
    }

    fn env(&self, context: &ServiceContext, ports: &Ports) -> Result<Vec<String>> {
        let Some(port) = ports.get("postgres") else {
            return Ok(Vec::new());
        };
        Ok(vec![
            "DB_CONNECTION=pgsql".into(),
            "DB_HOST=127.0.0.1".into(),
            format!("DB_PORT={port}"),
            format!("DB_DATABASE={DATABASE}"),
            format!("DB_USERNAME={USER}"),
            format!("DB_PASSWORD={}", password(&context.data_dir())?),
        ])
    }
}
