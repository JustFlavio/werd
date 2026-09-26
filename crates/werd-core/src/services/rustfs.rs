use super::{Service, ServiceContext};
use crate::model::Ports;
use crate::ports;
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub(super) struct Rustfs;

#[derive(Serialize, Deserialize)]
struct Credentials {
    access_key: String,
    secret_key: String,
}

/// S3 credentials of the project, generated on first use.
fn credentials(directory: &Path) -> Result<Credentials> {
    let path = directory.join("rustfs-credentials.json");
    if path.exists() {
        return serde_json::from_slice(&fs::read(&path)?).context("Invalid RustFS credentials file");
    }
    fs::create_dir_all(directory)?;
    let credentials = Credentials {
        access_key: format!("werd{}", Uuid::new_v4().simple()),
        secret_key: format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple()),
    };
    fs::write(path, serde_json::to_vec(&credentials)?)?;
    Ok(credentials)
}

impl Service for Rustfs {
    fn start(
        &self,
        context: &ServiceContext,
        ports: &mut Ports,
        children: &mut Vec<ManagedChild>,
    ) -> Result<()> {
        let binary = context.binary("rustfs", "rustfs", "RustFS")?;
        let data = context.data_dir().join("rustfs");
        fs::create_dir_all(&data)?;
        let credentials = credentials(&context.data_dir())?;
        let api = ports::assign(ports, "rustfs_api")?;
        let console = ports::assign(ports, "rustfs_console")?;
        let mut command = hidden_command(binary);
        command
            .arg("server")
            .arg("--address")
            .arg(format!("127.0.0.1:{api}"))
            .arg("--console-enable")
            .arg("--console-address")
            .arg(format!("127.0.0.1:{console}"))
            .arg(&data)
            .env("RUSTFS_ACCESS_KEY", credentials.access_key)
            .env("RUSTFS_SECRET_KEY", credentials.secret_key);
        children.push(spawn_ready(
            &context.data_dir(),
            "rustfs",
            command,
            api,
            "RustFS",
        )?);
        Ok(())
    }

    fn env(&self, context: &ServiceContext, ports: &Ports) -> Result<Vec<String>> {
        let Some(port) = ports.get("rustfs_api") else {
            return Ok(Vec::new());
        };
        let credentials = credentials(&context.data_dir())?;
        Ok(vec![
            "FILESYSTEM_DISK=s3".into(),
            format!("AWS_ACCESS_KEY_ID={}", credentials.access_key),
            format!("AWS_SECRET_ACCESS_KEY={}", credentials.secret_key),
            "AWS_DEFAULT_REGION=us-east-1".into(),
            "AWS_BUCKET=werd".into(),
            format!("AWS_ENDPOINT=http://127.0.0.1:{port}"),
            "AWS_USE_PATH_STYLE_ENDPOINT=true".into(),
        ])
    }
}
