use super::{Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;
use std::fs;

pub(super) struct Redis;

impl Driver for Redis {
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let data = context.data();
        fs::create_dir_all(&data)?;
        let port = context.instance.port;
        let mut command = hidden_command(context.binary("redis-server")?);
        command
            .args(["--bind", "127.0.0.1", "--port", &port.to_string(), "--dir"])
            .arg(&data)
            .args(["--appendonly", "yes"]);
        children.push(spawn_ready(
            &context.dir(),
            "redis",
            command,
            port,
            &context.label(),
        )?);
        Ok(())
    }

    fn shutdown(&self, context: &InstanceContext) {
        if let Ok(cli) = context.binary("redis-cli") {
            let _ = hidden_command(cli)
                .args([
                    "-h",
                    "127.0.0.1",
                    "-p",
                    &context.instance.port.to_string(),
                    "SHUTDOWN",
                ])
                .status();
        }
    }

    fn env(&self, context: &InstanceContext, _database: Option<&str>) -> Result<Vec<String>> {
        Ok(vec![
            "REDIS_HOST=127.0.0.1".into(),
            format!("REDIS_PORT={}", context.instance.port),
            "REDIS_PASSWORD=null".into(),
        ])
    }
}
