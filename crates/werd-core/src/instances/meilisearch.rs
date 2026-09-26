use super::{Credentials, Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;
use std::fs;

pub(super) struct Meilisearch;

/// The generated password is the Meilisearch master key.
fn master_key(context: &InstanceContext) -> Result<String> {
    Ok(context.credentials("master key")?.password)
}

impl Driver for Meilisearch {
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let data = context.data();
        fs::create_dir_all(&data)?;
        let port = context.instance.port;
        let mut command = hidden_command(context.binary("meilisearch")?);
        command
            .arg("--db-path")
            .arg(data.join("data.ms"))
            .arg("--dump-dir")
            .arg(data.join("dumps"))
            .arg("--http-addr")
            .arg(format!("127.0.0.1:{port}"))
            .args(["--env", "development", "--no-analytics"])
            .env("MEILI_MASTER_KEY", master_key(context)?)
            .current_dir(&data);
        children.push(spawn_ready(
            &context.dir(),
            "meilisearch",
            command,
            port,
            &context.label(),
        )?);
        Ok(())
    }

    fn env(&self, context: &InstanceContext, _database: Option<&str>) -> Result<Vec<String>> {
        Ok(vec![
            "SCOUT_DRIVER=meilisearch".into(),
            format!("MEILISEARCH_HOST=http://127.0.0.1:{}", context.instance.port),
            format!("MEILISEARCH_KEY={}", master_key(context)?),
        ])
    }

    fn credentials(&self, context: &InstanceContext) -> Result<Option<Credentials>> {
        context.credentials("master key").map(Some)
    }

    fn web_ui(&self, context: &InstanceContext) -> Option<String> {
        Some(format!("http://127.0.0.1:{}", context.instance.port))
    }
}
