//! Loopback port allocation. Every service binds to 127.0.0.1 only.

use crate::model::Ports;
use anyhow::{bail, Context, Result};
use std::net::{TcpListener, TcpStream};
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(12);

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// Returns the port stored under `key`, assigning a free one the first time.
/// Assignments are persisted with the project so `.env` values stay stable.
pub fn assign(ports: &mut Ports, key: &str) -> Result<u16> {
    if let Some(port) = ports.get(key) {
        return Ok(*port);
    }
    for _ in 0..20 {
        let port = free_port()?;
        if !ports.values().any(|taken| *taken == port) {
            ports.insert(key.into(), port);
            return Ok(port);
        }
    }
    bail!("Could not find a free port for {key}")
}

/// Fails if another process took one of the project's persisted ports.
pub fn ensure_available(ports: &Ports) -> Result<()> {
    for (role, port) in ports {
        TcpListener::bind(("127.0.0.1", *port)).with_context(|| {
            format!(
                "Port {port} ({role}) is in use by another process. Stop it or reassign the project's ports"
            )
        })?;
    }
    Ok(())
}

/// Waits for a removed Caddy listener to close after its watched config reloads.
pub(crate) fn wait_until_available(port: u16) -> Result<()> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("Port {port} is still in use after stopping Vite; see the Caddy log")
}

/// Waits until `port` accepts connections, failing early if `child` exits.
pub fn wait_until_listening(port: u16, child: &mut Child, label: &str) -> Result<()> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        if let Some(exit) = child.try_wait()? {
            bail!("{label} exited during startup ({exit})");
        }
        thread::sleep(Duration::from_millis(150));
    }
    bail!("{label} is not answering on port {port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assign_is_stable_and_unique() {
        let mut ports = Ports::new();
        let site = assign(&mut ports, "site").unwrap();
        let fastcgi = assign(&mut ports, "fastcgi").unwrap();
        assert_ne!(site, fastcgi);
        assert_eq!(assign(&mut ports, "site").unwrap(), site);
        assert_eq!(ports.len(), 2);
    }

    #[test]
    fn ensure_available_reports_the_busy_port() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let ports = Ports::from([("redis".to_string(), port)]);
        let error = ensure_available(&ports).unwrap_err().to_string();
        assert!(error.contains(&port.to_string()) && error.contains("redis"));
        drop(listener);
        ensure_available(&ports).unwrap();
    }
}
