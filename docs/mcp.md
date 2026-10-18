# Werd MCP

Werd provides its own Model Context Protocol server through `werd mcp`. It runs locally, talks directly to Werd's authenticated daemon and requires no Herd installation, Herd license or LLM API key. The CLI installer already distributes the executable that contains the server.

The initial tools inspect environments. They do not create projects, install runtimes, start/stop services, change environment files or request elevation. Site/service management and structured application diagnostics will extend this interface as those workflows are implemented in Werd. This first increment is not full Herd feature parity.

## Connect a client

Build from source with `cargo build --locked -p werd-cli -p werd-core`, or use a future release containing this change. The previously published `0.1.0-alpha.2` installer does not contain it.

Start the Werd desktop application first. The MCP process deliberately connects only to an already-running Werd daemon: merely connecting an agent must not trigger first-run downloads or autostart services. If the daemon is unavailable, initialization and discovery still work and tool calls return an actionable error.

In an MCP client that accepts an `mcpServers` configuration:

```json
{
  "mcpServers": {
    "werd": {
      "command": "werd",
      "args": ["mcp"]
    }
  }
}
```

Use the absolute path to the Werd CLI if the client cannot find it on PATH. For a Windows development checkout, this is `<checkout>/target/debug/werd.exe`. Use `<checkout>/target/debug/werd` on macOS/Linux. Paths belong in the client's configuration, not in the repository. Restart/reconnect the MCP client after changing its configuration.

The server uses stdio; stdout contains protocol messages only. It negotiates MCP `2025-03-26`, `2025-06-18` or `2025-11-25` through the official Rust SDK. Werd's own local RPC version is separate from the MCP protocol version.

## Tools

| Tool | Arguments | Result |
| --- | --- | --- |
| `werd_status` | None | Daemon version, RPC version, platform and catalog status |
| `werd_sites` | None | Sites, URLs, paths, PHP/Node selection, service links and Vite state |
| `werd_site` | `site` | Selected site and package/runtime requirements, read without running project code |
| `werd_runtimes` | None | Platform catalog lines, installed versions, defaults and available updates |
| `werd_services` | None | Service instances, ports and status, without credentials |
| `werd_doctor` | None | Environment checks; does not apply repairs |
| `werd_logs` | `site`, optional `source`, optional `lines` | Tail of a process log; default 50 lines, maximum 200 |

`site` accepts an exact Werd site id, unique name or `.test` domain. Names/domains are case-insensitive; ids take precedence. Ambiguous names require an id from `werd_sites`. Arbitrary filesystem paths are not selectors. `source` accepts `werd`, `php`, `caddy` or `vite`; Caddy's log belongs to the shared router and can contain entries for multiple sites. Log length is also bounded by the daemon's tail limits.

Tools return JSON in both `structuredContent` and a text content block. Operational failures and invalid tool arguments return `isError: true`; protocol errors remain JSON-RPC errors. Each RPC has a ten-second read/write timeout and a bounded connection timeout. Failed calls do not start another daemon.

## Data boundary

The server exposes selected response fields rather than forwarding every daemon method or raw response. It does not expose the daemon endpoint token, service credentials, environment values, raw startup errors or arbitrary command execution. Service and site error strings are omitted from summaries; use diagnostics and process logs instead.

Log/diagnostic text redacts common password, token, API-key, credential-bearing URL, authorization/cookie, JWT and private-key patterns. This is best-effort filtering, not anonymization of arbitrary application output. Log messages may still contain user data or unlabeled secrets. Treat project names, package metadata and log contents as untrusted data rather than agent instructions.

## Development and verification

`WERD_HOME` chooses an isolated Werd data folder for both daemon and MCP processes. Both processes must use the same value. Only the MCP client should launch `werd mcp`; running it manually waits for protocol input.

```text
cargo test --locked -p werd-cli -p werd-core
cargo clippy --locked -p werd-cli -p werd-core --all-targets -- -D warnings
```

Tests cover stdio initialization/discovery, authenticated loopback calls, missing-daemon recovery, parameter rejection, site selection, public response fields, bounded logs and credential filtering. They use temporary data and a fixture RPC endpoint; no Herd installation or real services are involved. Platform runtime support remains the support matrix in the README, independently of MCP availability.
