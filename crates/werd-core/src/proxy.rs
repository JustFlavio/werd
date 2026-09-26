//! PHP FastCGI of a site. The shared Caddy in `router` serves it over HTTPS.

use crate::model::Ports;
use crate::paths::project_dir;
use crate::ports;
use crate::process::{hidden_command, spawn_logged, ManagedChild};
use crate::runtimes;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Starts php-cgi from the site's PHP line and assigns the site's ports.
pub(crate) fn start_php(
    root: &Path,
    project_id: &str,
    project_path: &Path,
    php_line: &str,
    ports: &mut Ports,
    children: &mut Vec<ManagedChild>,
) -> Result<()> {
    let php_dir = runtimes::line_dir(
        root,
        "php",
        &runtimes::resolve_line(root, "php", Some(php_line), "PHP")?,
    );
    // Fail before starting anything when Caddy is missing.
    runtimes::resolve_line(root, "caddy", None, "Caddy")?;
    crate::router::public_dir(project_path)?;
    ports::assign(ports, "site")?;
    let fastcgi_port = ports::assign(ports, "fastcgi")?;
    let directory = project_dir(root, project_id);
    fs::create_dir_all(&directory)?;

    let mut php_command = hidden_command(php_dir.join(runtimes::exe("php-cgi")));
    php_command
        .arg("-c")
        .arg(php_dir.join("php.ini"))
        .arg("-b")
        .arg(format!("127.0.0.1:{fastcgi_port}"))
        .current_dir(project_path);
    let mut php_process = spawn_logged(&directory, "php", php_command)?;
    let ready = ports::wait_until_listening(fastcgi_port, &mut php_process.child, "PHP FastCGI");
    children.push(php_process);
    ready
}
