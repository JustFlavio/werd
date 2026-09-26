//! The web stack of a site: PHP FastCGI behind Caddy with a locally trusted certificate.

use crate::model::Ports;
use crate::paths::{caddy_data, project_dir};
use crate::ports;
use crate::process::{hidden_command, spawn_logged, spawn_ready, ManagedChild};
use crate::runtimes;
use anyhow::{bail, Result};
use std::fs;
use std::path::Path;

/// Caddy configuration serving `public_dir` on `https://localhost:<site_port>`.
pub(crate) fn caddyfile(public_dir: &Path, site_port: u16, fastcgi_port: u16) -> String {
    let root = public_dir
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        .replace('"', "\\\"");
    format!(
        "{{\n  admin off\n  auto_https disable_redirects\n  skip_install_trust\n}}\n\
         https://localhost:{site_port} {{\n  bind 127.0.0.1\n  root * \"{root}\"\n  \
         php_fastcgi 127.0.0.1:{fastcgi_port}\n  file_server\n  tls internal\n}}\n"
    )
}

/// Starts php-cgi (from the project's PHP line) and Caddy, returning the site URL.
pub(crate) fn start_site(
    root: &Path,
    project_id: &str,
    project_path: &Path,
    php_line: &str,
    ports: &mut Ports,
    children: &mut Vec<ManagedChild>,
) -> Result<String> {
    let php_dir = runtimes::line_dir(
        root,
        "php",
        &runtimes::resolve_line(root, "php", Some(php_line), "PHP")?,
    );
    let php = php_dir.join(runtimes::exe("php-cgi"));
    let caddy_dir = runtimes::line_dir(
        root,
        "caddy",
        &runtimes::resolve_line(root, "caddy", None, "Caddy")?,
    );
    let caddy = caddy_dir.join(runtimes::exe("caddy"));
    let public_dir = project_path.join("public");
    if !public_dir.join("index.php").is_file() {
        bail!("public/index.php not found in {}", project_path.display());
    }
    let site_port = ports::assign(ports, "site")?;
    let fastcgi_port = ports::assign(ports, "fastcgi")?;
    let directory = project_dir(root, project_id);
    fs::create_dir_all(&directory)?;

    let caddyfile_path = directory.join("Caddyfile");
    fs::write(&caddyfile_path, caddyfile(&public_dir, site_port, fastcgi_port))?;

    let mut php_command = hidden_command(php);
    php_command
        .arg("-c")
        .arg(php_dir.join("php.ini"))
        .arg("-b")
        .arg(format!("127.0.0.1:{fastcgi_port}"))
        .current_dir(project_path);
    let mut php_process = spawn_logged(&directory, "php", php_command)?;
    let ready = ports::wait_until_listening(fastcgi_port, &mut php_process.child, "PHP FastCGI");
    children.push(php_process);
    ready?;

    let mut caddy_command = hidden_command(caddy);
    caddy_command
        .arg("run")
        .arg("--config")
        .arg(&caddyfile_path)
        .args(["--adapter", "caddyfile"])
        .current_dir(&directory)
        .env("XDG_DATA_HOME", caddy_data(root));
    children.push(spawn_ready(
        &directory,
        "caddy",
        caddy_command,
        site_port,
        "Caddy HTTPS",
    )?);

    Ok(format!("https://localhost:{site_port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caddyfile_serves_public_dir_over_local_tls() {
        let config = caddyfile(Path::new(r"\\?\C:\work\my shop\public"), 8443, 9000);
        assert!(config.contains("https://localhost:8443 {"));
        assert!(config.contains(r#"root * "C:/work/my shop/public""#));
        assert!(config.contains("php_fastcgi 127.0.0.1:9000"));
        assert!(config.contains("bind 127.0.0.1"));
        assert!(config.contains("tls internal"));
        assert!(config.contains("admin off"));
    }

    #[test]
    fn caddyfile_escapes_quotes_in_paths() {
        let config = caddyfile(Path::new(r#"/srv/a"b/public"#), 1, 2);
        assert!(config.contains(r#"root * "/srv/a\"b/public""#));
    }
}
