use super::{Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;
use std::fs;

pub(super) struct Mailpit;

impl Driver for Mailpit {
    fn extra_ports(&self) -> &'static [(&'static str, u16)] {
        &[("ui", 8025)]
    }

    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        fs::create_dir_all(context.data())?;
        let smtp = context.instance.port;
        let mut command = hidden_command(context.binary("mailpit")?);
        command
            .arg("--smtp")
            .arg(format!("127.0.0.1:{smtp}"))
            .arg("--listen")
            .arg(format!("127.0.0.1:{}", context.extra_port("ui")?))
            .arg("--database")
            .arg(context.data().join("mailpit.db"))
            .arg("--disable-version-check");
        children.push(spawn_ready(
            &context.dir(),
            "mailpit",
            command,
            smtp,
            &context.label(),
        )?);
        Ok(())
    }

    fn env(&self, context: &InstanceContext, _database: Option<&str>) -> Result<Vec<String>> {
        Ok(vec![
            "MAIL_MAILER=smtp".into(),
            "MAIL_HOST=127.0.0.1".into(),
            format!("MAIL_PORT={}", context.instance.port),
            "MAIL_USERNAME=null".into(),
            "MAIL_PASSWORD=null".into(),
            "MAIL_ENCRYPTION=null".into(),
        ])
    }

    fn web_ui(&self, context: &InstanceContext) -> Option<String> {
        context
            .extra_port("ui")
            .ok()
            .map(|port| format!("http://127.0.0.1:{port}"))
    }
}
