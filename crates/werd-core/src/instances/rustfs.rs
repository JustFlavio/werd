use super::{Credentials, Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;
use std::fs;

pub(super) struct Rustfs;

/// RustFS uses the credentials as its access key and secret key.
fn keys(context: &InstanceContext) -> Result<Credentials> {
    context.credentials("werd-access")
}

impl Driver for Rustfs {
    fn extra_ports(&self) -> &'static [(&'static str, u16)] {
        &[("console", 9001)]
    }

    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let data = context.data();
        fs::create_dir_all(&data)?;
        let keys = keys(context)?;
        let api = context.instance.port;
        let mut command = hidden_command(context.binary("rustfs")?);
        command
            .arg("server")
            .arg("--address")
            .arg(format!("127.0.0.1:{api}"))
            .arg("--console-enable")
            .arg("--console-address")
            .arg(format!("127.0.0.1:{}", context.extra_port("console")?))
            .arg(&data)
            .env("RUSTFS_ACCESS_KEY", &keys.username)
            .env("RUSTFS_SECRET_KEY", &keys.password);
        children.push(spawn_ready(
            &context.dir(),
            "rustfs",
            command,
            api,
            &context.label(),
        )?);
        Ok(())
    }

    fn env(&self, context: &InstanceContext, database: Option<&str>) -> Result<Vec<String>> {
        let keys = keys(context)?;
        Ok(vec![
            "FILESYSTEM_DISK=s3".into(),
            format!("AWS_ACCESS_KEY_ID={}", keys.username),
            format!("AWS_SECRET_ACCESS_KEY={}", keys.password),
            "AWS_DEFAULT_REGION=us-east-1".into(),
            format!("AWS_BUCKET={}", database.unwrap_or("laravel")),
            format!("AWS_ENDPOINT=http://127.0.0.1:{}", context.instance.port),
            "AWS_USE_PATH_STYLE_ENDPOINT=true".into(),
        ])
    }

    fn credentials(&self, context: &InstanceContext) -> Result<Option<Credentials>> {
        keys(context).map(Some)
    }

    fn web_ui(&self, context: &InstanceContext) -> Option<String> {
        context
            .extra_port("console")
            .ok()
            .map(|port| format!("http://127.0.0.1:{port}"))
    }
}
