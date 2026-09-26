use super::{Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;
use std::fs;

pub(super) struct Mongodb;

impl Driver for Mongodb {
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let data = context.data();
        fs::create_dir_all(&data)?;
        let port = context.instance.port;
        let mut command = hidden_command(context.binary("bin/mongod")?);
        command
            .arg("--dbpath")
            .arg(&data)
            .args(["--port", &port.to_string(), "--bind_ip", "127.0.0.1"]);
        children.push(spawn_ready(
            &context.dir(),
            "mongodb",
            command,
            port,
            &context.label(),
        )?);
        Ok(())
    }

    fn create_database(&self, _context: &InstanceContext, _name: &str) -> anyhow::Result<()> {
        // MongoDB creates a database on the first write.
        Ok(())
    }

    fn env(&self, context: &InstanceContext, database: Option<&str>) -> Result<Vec<String>> {
        let port = context.instance.port;
        Ok(vec![
            format!("MONGODB_URI=mongodb://127.0.0.1:{port}"),
            format!("MONGODB_DATABASE={}", database.unwrap_or("laravel")),
        ])
    }
}
