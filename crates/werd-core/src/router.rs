//! The shared Caddy that serves every running site.
//!
//! Each site keeps its own `https://localhost:<port>` address, so `.env` files
//! keep working, and gets `https://<name>.test` when `.test` domains are on and
//! the HTTPS port is free. Caddy runs with `--watch`: rewriting the Caddyfile
//! adds or removes sites without restarting it.

use crate::model::Project;
use crate::paths::caddy_data;
use crate::process::{hidden_command, spawn_logged, ManagedChild};
use crate::runtimes;
use crate::settings::Settings;
use anyhow::{bail, Context, Result};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const READY_TIMEOUT: Duration = Duration::from_secs(12);

/// A running site as the router sees it.
pub(crate) struct Route {
    pub site_port: u16,
    pub fastcgi_port: u16,
    pub public_dir: PathBuf,
    pub domain: Option<String>,
    /// Loopback Vite upstream and its HTTPS endpoint, when managed by Werd.
    pub vite_ports: Option<(u16, u16)>,
}

#[derive(Default)]
pub(crate) struct Router {
    child: Option<ManagedChild>,
    /// HTTPS port of `.test` domains while the router serves them.
    domains_port: Option<u16>,
    /// Why `.test` domains are not served, when they are enabled.
    pub warning: Option<String>,
}

pub(crate) fn dir(root: &Path) -> PathBuf {
    root.join("router")
}

fn caddy_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        .replace('"', "\\\"")
}

/// Caddy configuration for `routes`; `.test` addresses only with a domains port.
pub(crate) fn caddyfile(routes: &[Route], domains_port: Option<u16>) -> String {
    let mut config = String::from(
        "{\n  admin off\n  auto_https disable_redirects\n  skip_install_trust\n  grace_period 2s\n}\n",
    );
    for route in routes {
        let mut addresses = vec![format!("https://localhost:{}", route.site_port)];
        if let (Some(port), Some(domain)) = (domains_port, &route.domain) {
            addresses.push(format!("https://{domain}:{port}"));
        }
        config.push_str(&format!(
            "{} {{\n  bind 127.0.0.1\n  root * \"{}\"\n  php_fastcgi 127.0.0.1:{}\n  file_server\n  tls internal\n}}\n",
            addresses.join(", "),
            caddy_path(&route.public_dir),
            route.fastcgi_port,
        ));
        if let Some((upstream, https)) = route.vite_ports {
            config.push_str(&format!(
                "https://localhost:{https} {{\n  bind 127.0.0.1\n  reverse_proxy 127.0.0.1:{upstream}\n  tls internal\n}}\n"
            ));
        }
    }
    config
}

impl Router {
    /// The address a running site answers on.
    pub fn url(&self, project: &Project, site_port: u16) -> String {
        match (self.domains_port, &project.domain) {
            (Some(443), Some(domain)) => format!("https://{domain}"),
            (Some(port), Some(domain)) => format!("https://{domain}:{port}"),
            _ => format!("https://localhost:{site_port}"),
        }
    }

    pub fn domains_active(&self) -> bool {
        self.domains_port.is_some()
    }

    pub fn has_exited(&mut self) -> Option<String> {
        let exit = self.child.as_mut()?.has_exited()?;
        self.child = None;
        Some(exit)
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            child.kill();
        }
    }

    /// Serves exactly `routes`: writes the configuration and starts Caddy if
    /// needed, stopping it when no site runs.
    pub fn sync(&mut self, root: &Path, routes: &[Route]) -> Result<()> {
        if routes.is_empty() {
            self.stop();
            return Ok(());
        }
        if self.has_exited().is_some() {
            self.child = None;
        }
        let starting = self.child.is_none();
        if starting {
            self.choose_domains_port(root);
        }
        let directory = dir(root);
        fs::create_dir_all(&directory)?;
        let config = directory.join("Caddyfile");
        fs::write(&config, caddyfile(routes, self.domains_port))?;
        if starting {
            let caddy_dir = runtimes::line_dir(
                root,
                "caddy",
                &runtimes::resolve_line(root, "caddy", None, "Caddy")?,
            );
            let mut command = hidden_command(caddy_dir.join(runtimes::exe("caddy")));
            command
                .arg("run")
                .arg("--config")
                .arg(&config)
                .args(["--adapter", "caddyfile", "--watch"])
                .current_dir(&directory)
                .env("XDG_DATA_HOME", caddy_data(root))
                .env("XDG_CONFIG_HOME", &directory);
            self.child = Some(spawn_logged(&directory, "caddy", command)?);
        }
        Ok(())
    }

    /// Restarts Caddy so settings such as the HTTPS port apply.
    pub fn restart(&mut self, root: &Path, routes: &[Route]) -> Result<()> {
        self.stop();
        self.sync(root, routes)
    }

    /// Waits until the router answers on `port` (a site's localhost port).
    pub fn wait_for(&mut self, port: u16) -> Result<()> {
        let deadline = Instant::now() + READY_TIMEOUT;
        while Instant::now() < deadline {
            if TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return Ok(());
            }
            if let Some(exit) = self.has_exited() {
                bail!("Caddy exited ({exit}); see the Caddy log");
            }
            thread::sleep(Duration::from_millis(150));
        }
        bail!("Caddy is not answering on port {port}; see the Caddy log")
    }

    fn choose_domains_port(&mut self, root: &Path) {
        self.domains_port = None;
        self.warning = None;
        let settings = Settings::load(root).unwrap_or_default();
        if !settings.domains {
            return;
        }
        let port = settings.https_port;
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(_) => self.domains_port = Some(port),
            Err(_) => {
                self.warning = Some(format!(
                    "Port {port} is used by another program, so sites use their localhost address. Stop that program or choose another HTTPS port."
                ))
            }
        }
    }
}

/// The router's Caddy log.
pub(crate) fn log_file(root: &Path) -> PathBuf {
    dir(root).join("caddy.log")
}

/// Checks the site folder can be served.
pub(crate) fn public_dir(project_path: &Path) -> Result<PathBuf> {
    let public_dir = project_path.join("public");
    if !public_dir.join("index.php").is_file() {
        bail!("public/index.php not found in {}", project_path.display());
    }
    Ok(public_dir)
}

/// Port of `role` in a started site.
pub(crate) fn port(project: &Project, role: &str) -> Result<u16> {
    project
        .ports
        .as_ref()
        .and_then(|ports| ports.get(role).copied())
        .with_context(|| format!("{} has no {role} port", project.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(domain: Option<&str>) -> Route {
        Route {
            site_port: 8443,
            fastcgi_port: 9000,
            public_dir: PathBuf::from(r"\\?\C:\work\my shop\public"),
            domain: domain.map(str::to_string),
            vite_ports: None,
        }
    }

    #[test]
    fn caddyfile_serves_localhost_and_the_domain() {
        let config = caddyfile(&[route(Some("my-shop.test"))], Some(443));
        assert!(config.contains("https://localhost:8443, https://my-shop.test:443 {"));
        assert!(config.contains(r#"root * "C:/work/my shop/public""#));
        assert!(config.contains("php_fastcgi 127.0.0.1:9000"));
        assert!(config.contains("bind 127.0.0.1"));
        assert!(config.contains("tls internal"));
        assert!(config.contains("admin off"));
    }

    #[test]
    fn without_a_domains_port_only_localhost_is_served() {
        let config = caddyfile(&[route(Some("my-shop.test")), route(None)], None);
        assert!(!config.contains(".test"));
        assert_eq!(config.matches("https://localhost:8443 {").count(), 2);
    }

    #[test]
    fn paths_with_quotes_are_escaped() {
        let mut quoted = route(None);
        quoted.public_dir = PathBuf::from(r#"/srv/a"b/public"#);
        assert!(caddyfile(&[quoted], None).contains(r#"root * "/srv/a\"b/public""#));
    }

    #[test]
    fn vite_uses_a_separate_https_listener_and_loopback_upstream() {
        let mut site = route(Some("shop.test"));
        site.vite_ports = Some((5173, 8445));
        let config = caddyfile(&[site], Some(443));
        assert!(config.contains(
            "https://localhost:8445 {\n  bind 127.0.0.1\n  reverse_proxy 127.0.0.1:5173\n  tls internal"
        ));
        assert!(config.contains("php_fastcgi 127.0.0.1:9000"));
    }

    #[test]
    fn urls_prefer_the_domain_when_served() {
        let mut project: Project = serde_json::from_value(serde_json::json!({
            "id": "p", "name": "shop", "path": "/w", "php": "8.4", "domain": "shop.test"
        }))
        .unwrap();
        let mut router = Router::default();
        assert_eq!(router.url(&project, 8443), "https://localhost:8443");
        router.domains_port = Some(443);
        assert_eq!(router.url(&project, 8443), "https://shop.test");
        router.domains_port = Some(8444);
        assert_eq!(router.url(&project, 8443), "https://shop.test:8444");
        project.domain = None;
        assert_eq!(router.url(&project, 8443), "https://localhost:8443");
    }
}
