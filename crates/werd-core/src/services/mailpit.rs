use super::{Service, ServiceContext};
use crate::model::Ports;
use crate::ports;
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::Result;

pub(super) struct Mailpit;

impl Service for Mailpit {
    fn start(
        &self,
        context: &ServiceContext,
        ports: &mut Ports,
        children: &mut Vec<ManagedChild>,
    ) -> Result<()> {
        let binary = context.binary("mailpit", "Mailpit")?;
        let smtp = ports::assign(ports, "mailpit_smtp")?;
        let ui = ports::assign(ports, "mailpit_ui")?;
        let mut command = hidden_command(binary);
        command
            .arg("--smtp")
            .arg(format!("127.0.0.1:{smtp}"))
            .arg("--listen")
            .arg(format!("127.0.0.1:{ui}"))
            .arg("--database")
            .arg(context.data_dir().join("mailpit.db"))
            .arg("--disable-version-check");
        children.push(spawn_ready(
            context.root,
            context.project_id,
            "mailpit",
            command,
            smtp,
            "Mailpit SMTP",
        )?);
        Ok(())
    }

    fn env(&self, _context: &ServiceContext, ports: &Ports) -> Result<Vec<String>> {
        let Some(port) = ports.get("mailpit_smtp") else {
            return Ok(Vec::new());
        };
        Ok(vec![
            "MAIL_MAILER=smtp".into(),
            "MAIL_HOST=127.0.0.1".into(),
            format!("MAIL_PORT={port}"),
            "MAIL_USERNAME=null".into(),
            "MAIL_PASSWORD=null".into(),
            "MAIL_ENCRYPTION=null".into(),
        ])
    }
}
