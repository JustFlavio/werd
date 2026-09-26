//! Local RPC between the daemon and its clients (GUI, CLI).
//!
//! Transport: one TCP connection per call on 127.0.0.1, carrying one JSON line
//! each way. The daemon writes its port and a random token to `daemon.json`
//! in the user's data directory; requests without that token are refused.

use crate::paths::{endpoint_file, home};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// Bumped when requests or responses change incompatibly.
pub const PROTOCOL_VERSION: u32 = 1;

/// Long enough for runtime downloads, which run inside a single call.
const CALL_TIMEOUT: Duration = Duration::from_secs(650);
const MAX_REQUEST_BYTES: usize = 1_000_000;
const DAEMON_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// Serializes daemon launches from threads of the same process.
static DAEMON_START: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Endpoint {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    token: String,
    method: String,
    params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    ok: bool,
    data: Value,
    error: Option<String>,
}

// ---- Client ---------------------------------------------------------------

/// Calls `method` on the daemon registered in `root`.
pub(crate) fn call_at(root: &Path, method: &str, params: Value) -> Result<Value> {
    let endpoint: Endpoint =
        serde_json::from_slice(&fs::read(endpoint_file(root)).context("The Werd daemon is not running")?)?;
    let address = format!("127.0.0.1:{}", endpoint.port).parse()?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(CALL_TIMEOUT))?;
    let request = Request {
        token: endpoint.token,
        method: method.into(),
        params,
    };
    serde_json::to_writer(&mut stream, &request)?;
    writeln!(stream)?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line)?;
    let response: Response = serde_json::from_str(&line).context("Invalid response from the daemon")?;
    if response.ok {
        Ok(response.data)
    } else {
        bail!("{}", response.error.unwrap_or_else(|| "Unknown error".into()))
    }
}

/// Calls `method` on the daemon of the current user.
pub fn rpc(method: &str, params: Value) -> Result<Value> {
    call_at(&home()?, method, params)
}

/// Starts `executable` as the daemon unless one already answers.
pub fn ensure_daemon(executable: &Path) -> Result<()> {
    if rpc("ping", json!({})).is_ok() {
        return Ok(());
    }
    let _guard = DAEMON_START
        .lock()
        .map_err(|_| anyhow!("Daemon startup lock poisoned"))?;
    if rpc("ping", json!({})).is_ok() {
        return Ok(());
    }
    let startup_log = home()?.join("daemon-startup.log");
    crate::process::stop_inheriting_std_handles();
    let mut child = crate::process::hidden_command(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(File::create(&startup_log)?))
        .spawn()
        .with_context(|| format!("Cannot start {}", executable.display()))?;
    let deadline = Instant::now() + DAEMON_STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if rpc("ping", json!({})).is_ok() {
            return Ok(());
        }
        if let Some(exit) = child.try_wait()? {
            let details = fs::read_to_string(&startup_log).unwrap_or_default();
            bail!("The Werd daemon exited during startup ({exit}): {details}");
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("The Werd daemon is not answering after 30 seconds; see daemon-startup.log in the Werd data folder")
}

/// Finds `werd-daemon`: `WERD_DAEMON_BIN`, next to the current executable, or the dev build.
pub fn daemon_executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("WERD_DAEMON_BIN") {
        return Ok(PathBuf::from(path));
    }
    let name = if cfg!(windows) {
        "werd-daemon.exe"
    } else {
        "werd-daemon"
    };
    let current = std::env::current_exe()?;
    let adjacent = current.parent().context("Invalid executable path")?.join(name);
    if adjacent.exists() {
        return Ok(adjacent);
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(name);
    if dev.exists() {
        return Ok(dev);
    }
    bail!("werd-daemon not found; build it or set WERD_DAEMON_BIN")
}

// ---- Server ---------------------------------------------------------------

/// Binds a loopback listener and publishes its endpoint in `root`.
pub(crate) fn listen(root: &Path) -> Result<(TcpListener, Endpoint)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = Endpoint {
        port: listener.local_addr()?.port(),
        token: uuid::Uuid::new_v4().to_string(),
        pid: std::process::id(),
    };
    fs::write(endpoint_file(root), serde_json::to_vec(&endpoint)?)?;
    Ok((listener, endpoint))
}

/// Handles one connection: reads a request, checks the token, writes the response.
pub(crate) fn serve_connection(
    mut stream: TcpStream,
    token: &str,
    handler: impl FnOnce(&str, Value) -> Result<Value>,
) -> Result<()> {
    stream.set_read_timeout(Some(CALL_TIMEOUT))?;
    stream.set_write_timeout(Some(CALL_TIMEOUT))?;
    // Cap the read so a client cannot make the daemon buffer unbounded input.
    let mut line = String::new();
    BufReader::new(stream.try_clone()?.take(MAX_REQUEST_BYTES as u64 + 1)).read_line(&mut line)?;
    if line.len() > MAX_REQUEST_BYTES {
        bail!("Request too large");
    }
    let response = match serde_json::from_str::<Request>(&line) {
        Ok(request) if request.token != token => failure("Invalid local token".into()),
        Ok(request) => match handler(&request.method, request.params) {
            Ok(data) => Response {
                ok: true,
                data,
                error: None,
            },
            Err(error) => failure(format!("{error:#}")),
        },
        Err(error) => failure(format!("Invalid request: {error}")),
    };
    serde_json::to_writer(&mut stream, &response)?;
    writeln!(stream)?;
    Ok(())
}

fn failure(message: String) -> Response {
    Response {
        ok: false,
        data: Value::Null,
        error: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serves `connections` requests with an echo handler on a temporary home.
    fn echo_server(root: &Path, connections: usize) -> thread::JoinHandle<()> {
        let (listener, endpoint) = listen(root).unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().take(connections) {
                serve_connection(stream.unwrap(), &endpoint.token, |method, params| match method {
                    "echo" => Ok(params),
                    other => bail!("Unknown method: {other}"),
                })
                .unwrap();
            }
        })
    }

    #[test]
    fn round_trip_returns_data_and_errors() {
        let root = tempfile::tempdir().unwrap();
        let server = echo_server(root.path(), 2);
        assert_eq!(
            call_at(root.path(), "echo", json!({ "a": 1 })).unwrap(),
            json!({ "a": 1 })
        );
        let error = call_at(root.path(), "nope", json!({})).unwrap_err().to_string();
        assert_eq!(error, "Unknown method: nope");
        server.join().unwrap();
    }

    #[test]
    fn requests_with_a_wrong_token_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let server = echo_server(root.path(), 1);
        let mut endpoint: Endpoint =
            serde_json::from_slice(&fs::read(endpoint_file(root.path())).unwrap()).unwrap();
        endpoint.token = "forged".into();
        fs::write(endpoint_file(root.path()), serde_json::to_vec(&endpoint).unwrap()).unwrap();
        let error = call_at(root.path(), "echo", json!({})).unwrap_err().to_string();
        assert_eq!(error, "Invalid local token");
        server.join().unwrap();
    }

    #[test]
    fn missing_endpoint_means_daemon_not_running() {
        let root = tempfile::tempdir().unwrap();
        let error = call_at(root.path(), "ping", json!({})).unwrap_err().to_string();
        assert!(error.contains("not running"));
    }
}
