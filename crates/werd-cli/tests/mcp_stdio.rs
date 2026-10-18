//! Exercise the shipped CLI over actual stdio and authenticated loopback RPC.
//! The fixture cannot start services and does not need Herd or installed runtimes.

#![allow(clippy::unwrap_used)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: Receiver<Value>,
}

impl Client {
    fn new(root: &std::path::Path) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_werd"));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .args(["mcp"])
            .env("WERD_HOME", root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in output.lines() {
                let value = serde_json::from_str(&line.unwrap()).expect("stdout must contain only MCP JSON");
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            input: child.stdin.take(),
            output: receiver,
            child,
        }
    }

    fn send(&mut self, message: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }

    fn request(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}));
        let response = self
            .output
            .recv_timeout(Duration::from_secs(10))
            .expect("MCP did not reply");
        assert_eq!(response["id"], id);
        response
    }

    fn initialize(&mut self) {
        let result = self.request(1, "initialize", json!({
            "protocolVersion":"2025-06-18", "capabilities":{}, "clientInfo":{"name":"werd-test", "version":"1"}
        }));
        assert_eq!(result["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(result["result"]["serverInfo"]["name"], "werd");
        self.send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}));
    }

    fn close(&mut self) {
        drop(self.input.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "MCP did not exit after stdin closed");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn initialization_discovery_and_real_rpc_round_trip() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    std::fs::write(root.path().join("daemon.json"), json!({
        "port":listener.local_addr().unwrap().port(), "token":"fixture-private-token", "pid":std::process::id()
    }).to_string()).unwrap();
    let rpc = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "No RPC request received");
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("RPC accept failed: {error}"),
            }
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(request["token"], "fixture-private-token");
        assert_eq!(request["method"], "system.info");
        writeln!(stream, "{}", json!({"ok":true, "data":{
            "version":"fixture", "protocol":2, "platform":"test", "token":"fixture-private-token", "home":"private"
        }, "error":null})).unwrap();
    });
    let mut client = Client::new(root.path());
    client.initialize();
    let list = client.request(2, "tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 7);
    assert!(tools
        .iter()
        .all(|tool| tool["annotations"]["readOnlyHint"] == true));
    assert!(tools
        .iter()
        .all(|tool| tool["inputSchema"]["additionalProperties"] == false));
    let response = client.request(3, "tools/call", json!({"name":"werd_status", "arguments":{}}));
    assert_eq!(response["result"]["isError"], false);
    assert_eq!(response["result"]["structuredContent"]["version"], "fixture");
    assert!(!response.to_string().contains("fixture-private-token"));
    assert!(!response.to_string().contains("private"));
    rpc.join().unwrap();
    // Initialization and tool discovery made no RPC requests; only the call above did.
    let invalid = client.request(
        4,
        "tools/call",
        json!({"name":"werd_status", "arguments":{"password":"secret"}}),
    );
    assert_eq!(invalid["result"]["isError"], true);
    assert!(!invalid.to_string().contains("secret"));
    client.close();
}

#[test]
fn missing_daemon_is_a_tool_error_and_does_not_launch_one() {
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::new(root.path());
    client.initialize();
    let response = client.request(2, "tools/call", json!({"name":"werd_status"}));
    assert_eq!(response["result"]["isError"], true);
    assert!(response.to_string().contains("Open Werd"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let list = client.request(3, "tools/list", json!({}));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 7);
    client.close();
}
