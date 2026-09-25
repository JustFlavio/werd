use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::env;
use werd_core::{daemon_executable, ensure_daemon, rpc};

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (method, params) = match args.first().map(String::as_str) {
        Some("list") | Some("status") => ("list", json!({})),
        Some("add") => ("add", json!({ "path": args.get(1).context("Uso: werd add <cartella>")? })),
        Some("up") | Some("start") => ("start", json!({ "id": args.get(1).context("Uso: werd up <id>")? })),
        Some("down") | Some("stop") => ("stop", json!({ "id": args.get(1).context("Uso: werd down <id>")? })),
        Some("reset-ports") => ("reset-ports", json!({ "id": args.get(1).context("Uso: werd reset-ports <id>")? })),
        Some("open") => ("open", json!({ "id": args.get(1).context("Uso: werd open <id>")? })),
        Some("logs") => ("logs", json!({ "id": args.get(1).context("Uso: werd logs <id> [servizio]")?, "service": args.get(2).map(String::as_str).unwrap_or("werd") })),
        Some("doctor") => ("doctor", json!({})),
        Some("trust-ca") => ("trust-ca", json!({})),
        Some("runtimes") => ("runtimes", json!({})),
        Some("install") => ("install", json!({ "id": args.get(1).context("Uso: werd install <php|caddy|postgres|pgvector|redis|mailpit|rustfs>")? })),
        Some("env") => ("env", json!({ "id": args.get(1).context("Uso: werd env <id>")? })),
        Some("help") | None => {
            println!("Werd CLI\n  werd list\n  werd add <cartella>\n  werd up <id>\n  werd down <id>\n  werd open <id>\n  werd reset-ports <id>\n  werd logs <id> [servizio]\n  werd env <id>\n  werd runtimes\n  werd install <runtime>\n  werd trust-ca\n  werd doctor");
            return Ok(());
        }
        Some(other) => bail!("Comando sconosciuto: {other}. Usa 'werd help'."),
    };
    ensure_daemon(&daemon_executable()?)?;
    let result: Value = rpc(method, params)?;
    if method == "env" { println!("{}", result.as_str().unwrap_or_default()); }
    else { println!("{}", serde_json::to_string_pretty(&result)?); }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Errore: {error:#}");
        std::process::exit(1);
    }
}
