//! MySQL and MariaDB share one driver; only binary names and initialization differ.

use super::{Credentials, Driver, InstanceContext};
use crate::process::{hidden_command, spawn_ready, ManagedChild};
use anyhow::{bail, Context, Result};
use std::fs;

pub(super) struct Mysql {
    product: &'static str,
    server: &'static str,
    client: &'static str,
    admin: &'static str,
}

pub(super) const MYSQL: Mysql = Mysql {
    product: "mysql",
    server: "bin/mysqld",
    client: "bin/mysql",
    admin: "bin/mysqladmin",
};
pub(super) const MARIADB: Mysql = Mysql {
    product: "mariadb",
    server: "bin/mariadbd",
    client: "bin/mariadb",
    admin: "bin/mariadb-admin",
};

const USER: &str = "werd";
/// Written after the `werd` account exists, so setup runs once.
const READY_MARKER: &str = "werd-account-ready";

impl Mysql {
    /// Runs SQL as `user` over TCP. `password` is `None` for MySQL's passwordless root after initialization.
    fn sql(
        &self,
        context: &InstanceContext,
        user: &str,
        password: Option<&str>,
        sql: &str,
    ) -> Result<String> {
        let mut command = hidden_command(context.binary(self.client)?);
        command.args([
            "--no-defaults",
            "-h",
            "127.0.0.1",
            "-P",
            &context.instance.port.to_string(),
            "-u",
            user,
        ]);
        command.args(["--batch", "--skip-column-names", "-e", sql]);
        if let Some(password) = password {
            // Passed through the environment, not the command line, so it does not show in process lists.
            command.env("MYSQL_PWD", password);
        }
        let output = command.output().context("The database client did not start")?;
        if !output.status.success() {
            bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn initialize(&self, context: &InstanceContext, credentials: &Credentials) -> Result<()> {
        let data = context.data();
        let install = context.install_dir()?;
        let output = if self.product == "mariadb" {
            // mariadb-install-db sets the root password directly.
            hidden_command(context.binary("bin/mariadb-install-db")?)
                .arg(format!("--datadir={}", data.display()))
                .arg(format!("--password={}", credentials.password))
                .output()
        } else {
            hidden_command(context.binary(self.server)?)
                .args(["--no-defaults", "--initialize-insecure", "--console"])
                .arg(format!("--basedir={}", install.display()))
                .arg(format!("--datadir={}", data.display()))
                .output()
        }
        .context("Database initialization did not start")?;
        if !output.status.success() {
            let _ = fs::remove_dir_all(&data);
            bail!(
                "Initializing the database failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Creates the `werd` account with full privileges from localhost.
    fn create_account(&self, context: &InstanceContext, credentials: &Credentials) -> Result<()> {
        let root_password = (self.product == "mariadb").then_some(credentials.password.as_str());
        let password = credentials.password.replace('\'', "''");
        let sql = format!(
            "CREATE USER IF NOT EXISTS '{USER}'@'localhost' IDENTIFIED BY '{password}';\
             CREATE USER IF NOT EXISTS '{USER}'@'127.0.0.1' IDENTIFIED BY '{password}';\
             GRANT ALL PRIVILEGES ON *.* TO '{USER}'@'localhost' WITH GRANT OPTION;\
             GRANT ALL PRIVILEGES ON *.* TO '{USER}'@'127.0.0.1' WITH GRANT OPTION;\
             FLUSH PRIVILEGES;"
        );
        self.sql(context, "root", root_password, &sql)
            .context("Creating the werd database account failed")?;
        fs::write(context.dir().join(READY_MARKER), "")?;
        Ok(())
    }
}

impl Driver for Mysql {
    fn start(&self, context: &InstanceContext, children: &mut Vec<ManagedChild>) -> Result<()> {
        let credentials = context.credentials(USER)?;
        let data = context.data();
        if !data.exists() {
            self.initialize(context, &credentials)?;
        }
        let port = context.instance.port;
        let mut command = hidden_command(context.binary(self.server)?);
        command
            .arg("--no-defaults")
            .arg(format!("--basedir={}", context.install_dir()?.display()))
            .arg(format!("--datadir={}", data.display()))
            .arg(format!("--port={port}"))
            .args(["--bind-address=127.0.0.1", "--console"]);
        if self.product == "mysql" {
            // The X Protocol listens on 33060 by default and would clash between instances.
            command.arg("--mysqlx=OFF");
        }
        children.push(spawn_ready(
            &context.dir(),
            self.product,
            command,
            port,
            &context.label(),
        )?);
        if !context.dir().join(READY_MARKER).exists() {
            self.create_account(context, &credentials)?;
        }
        Ok(())
    }

    fn shutdown(&self, context: &InstanceContext) {
        let (Ok(admin), Ok(credentials)) = (context.binary(self.admin), context.credentials(USER)) else {
            return;
        };
        let _ = hidden_command(admin)
            .args([
                "--no-defaults",
                "-h",
                "127.0.0.1",
                "-P",
                &context.instance.port.to_string(),
                "-u",
                USER,
                "shutdown",
            ])
            .env("MYSQL_PWD", &credentials.password)
            .status();
    }

    fn create_database(&self, context: &InstanceContext, name: &str) -> Result<()> {
        let credentials = context.credentials(USER)?;
        self.sql(
            context,
            USER,
            Some(&credentials.password),
            &format!("CREATE DATABASE IF NOT EXISTS `{name}`"),
        )
        .with_context(|| format!("Creating database {name} failed"))?;
        Ok(())
    }

    fn env(&self, context: &InstanceContext, database: Option<&str>) -> Result<Vec<String>> {
        let credentials = context.credentials(USER)?;
        Ok(vec![
            format!(
                "DB_CONNECTION={}",
                if self.product == "mariadb" {
                    "mariadb"
                } else {
                    "mysql"
                }
            ),
            "DB_HOST=127.0.0.1".into(),
            format!("DB_PORT={}", context.instance.port),
            format!("DB_DATABASE={}", database.unwrap_or("laravel")),
            format!("DB_USERNAME={}", credentials.username),
            format!("DB_PASSWORD={}", credentials.password),
        ])
    }

    fn credentials(&self, context: &InstanceContext) -> Result<Option<Credentials>> {
        context.credentials(USER).map(Some)
    }
}
