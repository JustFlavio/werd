use super::{Service, ServiceContext};
use crate::model::Ports;
use crate::ports;
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;
use std::fs;

pub(super) struct Redis;

impl Service for Redis {
    fn start(
        &self,
        context: &ServiceContext,
        ports: &mut Ports,
        children: &mut Vec<ManagedChild>,
    ) -> Result<()> {
        let binary = context.binary("redis", "redis-server", "Redis")?;
        let data = context.data_dir().join("redis");
        fs::create_dir_all(&data)?;
        let port = ports::assign(ports, "redis")?;
        let mut command = hidden_command(binary);
        command
            .args(["--bind", "127.0.0.1", "--port", &port.to_string(), "--dir"])
            .arg(&data)
            .args(["--appendonly", "yes"]);
        children.push(spawn_ready(&context.data_dir(), "redis", command, port, "Redis")?);
        Ok(())
    }

    fn shutdown(&self, context: &ServiceContext, ports: &Ports) {
        if let (Some(port), Ok(cli)) = (ports.get("redis"), context.binary("redis", "redis-cli", "Redis")) {
            let _ = hidden_command(cli)
                .args(["-h", "127.0.0.1", "-p", &port.to_string(), "SHUTDOWN"])
                .status();
        }
    }

    fn env(&self, _context: &ServiceContext, ports: &Ports) -> Result<Vec<String>> {
        let Some(port) = ports.get("redis") else {
            return Ok(Vec::new());
        };
        Ok(vec![
            "REDIS_HOST=127.0.0.1".into(),
            format!("REDIS_PORT={port}"),
            "REDIS_PASSWORD=null".into(),
        ])
    }
}
