use super::{Service, ServiceContext};
use crate::model::Ports;
use crate::paths::runtime_binary;
use crate::ports;
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
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

fn pgvector_files(postgres: &Path) -> Result<(PathBuf, PathBuf)> {
    // bin/postgres -> <install root>
    let install = postgres
        .parent()
        .and_then(Path::parent)
        .context("Invalid PostgreSQL path")?;
    let library = if cfg!(windows) { "vector.dll" } else { "vector.so" };
    Ok((
        install.join("share/extension/vector.control"),
        install.join("lib").join(library),
    ))
}

impl Service for Postgres {
    fn start(
        &self,
        context: &ServiceContext,
        ports: &mut Ports,
        children: &mut Vec<ManagedChild>,
    ) -> Result<()> {
        let postgres = context.binary("postgres", "PostgreSQL 18")?;
        let initdb = context.binary("initdb", "PostgreSQL initdb")?;
        let psql_binary = context.binary("psql", "PostgreSQL psql")?;
        let directory = context.data_dir();
        let cluster = directory.join("postgres");
        let password = password(&directory)?;

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

        let (control, library) = pgvector_files(&postgres)?;
        if !control.is_file() || !library.is_file() {
            bail!("pgvector is not installed in the PostgreSQL 18 runtime");
        }

        let port = ports::assign(ports, "postgres")?;
        let mut command = hidden_command(postgres);
        command
            .arg("-D")
            .arg(&cluster)
            .args(["-h", "127.0.0.1", "-p", &port.to_string()]);
        children.push(spawn_ready(
            context.root,
            context.project_id,
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
        psql(
            &psql_binary,
            port,
            &password,
            DATABASE,
            &["-c", "CREATE EXTENSION IF NOT EXISTS vector"],
        )
        .context("Enabling pgvector failed")?;
        Ok(())
    }

    fn shutdown(&self, context: &ServiceContext, _ports: &Ports) {
        let _ = hidden_command(runtime_binary(context.root, "pg_ctl"))
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
