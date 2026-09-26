//! `werd service …`: shared service instances (PostgreSQL, MySQL, Redis, …).

use crate::call;
use crate::runtimes::{parse_target, wait};
use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use werd_core::jobs::Job;

#[derive(Subcommand)]
pub enum ServiceAction {
    /// List service instances (default).
    List,
    /// List the services and versions you can add.
    Available,
    /// Add an instance, e.g. `werd service add postgresql@17 --with pgvector`.
    Add {
        /// <product>@<line>
        target: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Start it whenever Werd starts.
        #[arg(long)]
        autostart: bool,
        /// Extensions to enable (PostgreSQL: pgvector).
        #[arg(long = "with")]
        extensions: Vec<String>,
        /// Create it without starting it.
        #[arg(long)]
        no_start: bool,
    },
    Start {
        service: String,
    },
    Stop {
        service: String,
    },
    /// Rename an instance, e.g. `werd service rename "cloudino-ai PostgreSQL" "PostgreSQL 18"`.
    Rename {
        service: String,
        name: String,
    },
    /// Show credentials, .env values and the web UI address.
    Info {
        service: String,
    },
    Logs {
        service: String,
    },
    /// Create a database in a running instance.
    Db {
        service: String,
        name: String,
    },
    /// Delete an instance.
    #[command(visible_alias = "rm")]
    Remove {
        service: String,
        /// Keep the data folder on disk.
        #[arg(long)]
        keep_data: bool,
    },
}

fn instances() -> Result<Vec<Value>> {
    Ok(serde_json::from_value(call("services.list", json!({}))?)?)
}

/// Accepts an instance id or a unique name (case-insensitive).
fn resolve(service: &str) -> Result<String> {
    let list = instances()?;
    if let Some(found) = list.iter().find(|item| item["id"] == service) {
        return Ok(found["id"].as_str().unwrap_or_default().to_string());
    }
    let matches: Vec<&Value> = list
        .iter()
        .filter(|item| {
            item["name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(service))
        })
        .collect();
    match matches.as_slice() {
        [single] => Ok(single["id"].as_str().unwrap_or_default().to_string()),
        [] => bail!("No service named {service}. Run `werd service` to list them."),
        _ => bail!("Several services are named {service}; use the id."),
    }
}

fn print_instances(list: &[Value]) {
    if list.is_empty() {
        println!("No services yet. Add one with `werd service add postgresql@18`.");
        return;
    }
    let width = list
        .iter()
        .filter_map(|item| item["name"].as_str())
        .map(str::len)
        .max()
        .unwrap_or(4)
        .max(4);
    println!(
        "{:width$}  {:12} {:6} {:8}  {:6}  WEB UI",
        "NAME", "PRODUCT", "LINE", "STATUS", "PORT"
    );
    for item in list {
        println!(
            "{:width$}  {:12} {:6} {:8}  {:6}  {}",
            item["name"].as_str().unwrap_or_default(),
            item["product"].as_str().unwrap_or_default(),
            item["line"].as_str().unwrap_or_default(),
            item["status"].as_str().unwrap_or_default(),
            item["port"].as_u64().unwrap_or_default(),
            item["web_ui"].as_str().unwrap_or("-"),
        );
        if let Some(error) = item["error"].as_str() {
            println!("{:width$}  error: {error}", "");
        }
    }
}

pub fn run(action: Option<ServiceAction>, json_output: bool) -> Result<()> {
    let print = |value: &Value| -> Result<()> {
        println!("{}", serde_json::to_string_pretty(value)?);
        Ok(())
    };
    match action.unwrap_or(ServiceAction::List) {
        ServiceAction::List => {
            let list = instances()?;
            if json_output {
                return print(&json!(list));
            }
            print_instances(&list);
        }
        ServiceAction::Available => {
            let offerings = call("services.catalog", json!({}))?;
            if json_output {
                return print(&offerings);
            }
            for product in offerings.as_array().into_iter().flatten() {
                let lines: Vec<String> = product["lines"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|line| {
                        let installed = if line["installed"].is_null() { "" } else { "*" };
                        format!("{}{installed}", line["line"].as_str().unwrap_or_default())
                    })
                    .collect();
                println!(
                    "{:12} {:10} {}",
                    product["product"].as_str().unwrap_or_default(),
                    product["categories"][0].as_str().unwrap_or_default(),
                    lines.join(" ")
                );
            }
            println!("\n* installed");
        }
        ServiceAction::Add {
            target,
            name,
            port,
            autostart,
            extensions,
            no_start,
        } => {
            let (product, line) = parse_target(&target)?;
            let created = call(
                "services.create",
                json!({ "product": product, "line": line, "name": name, "port": port,
                        "autostart": autostart, "extensions": extensions, "start": !no_start }),
            )?;
            if let Some(job) = created.get("job").filter(|job| !job.is_null()) {
                wait(serde_json::from_value::<Job>(job.clone())?)?;
            }
            let id = created["instance"]["id"].as_str().unwrap_or_default();
            let details = call("services.details", json!({ "id": id }))?;
            if json_output {
                return print(&details);
            }
            let instance = &details["instance"];
            println!(
                "Added {} on port {} ({})",
                instance["name"].as_str().unwrap_or_default(),
                instance["port"],
                instance["status"].as_str().unwrap_or_default()
            );
            if let Some(error) = instance["error"].as_str() {
                bail!("{error}");
            }
        }
        ServiceAction::Start { service } => {
            let instance = call("services.start", json!({ "id": resolve(&service)? }))?;
            println!(
                "Started {} on port {}",
                instance["name"].as_str().unwrap_or_default(),
                instance["port"]
            );
        }
        ServiceAction::Stop { service } => {
            let instance = call("services.stop", json!({ "id": resolve(&service)? }))?;
            println!("Stopped {}", instance["name"].as_str().unwrap_or_default());
        }
        ServiceAction::Rename { service, name } => {
            let instance = call(
                "services.rename",
                json!({ "id": resolve(&service)?, "name": name }),
            )?;
            println!("Renamed to {}", instance["name"].as_str().unwrap_or_default());
        }
        ServiceAction::Info { service } => {
            let details = call("services.details", json!({ "id": resolve(&service)? }))?;
            if json_output {
                return print(&details);
            }
            let instance = &details["instance"];
            println!(
                "{} ({} {})",
                instance["name"].as_str().unwrap_or_default(),
                instance["product"].as_str().unwrap_or_default(),
                instance["line"].as_str().unwrap_or_default()
            );
            println!("status   {}", instance["status"].as_str().unwrap_or_default());
            println!("port     {}", instance["port"]);
            if let Some(url) = details["web_ui"].as_str() {
                println!("web UI   {url}");
            }
            if let Some(credentials) = details["credentials"].as_object() {
                println!(
                    "user     {}",
                    credentials["username"].as_str().unwrap_or_default()
                );
                println!(
                    "password {}",
                    credentials["password"].as_str().unwrap_or_default()
                );
            }
            println!("\n{}", details["env"].as_str().unwrap_or_default());
        }
        ServiceAction::Logs { service } => {
            for line in serde_json::from_value::<Vec<String>>(call(
                "services.logs",
                json!({ "id": resolve(&service)? }),
            )?)? {
                println!("{line}");
            }
        }
        ServiceAction::Db { service, name } => {
            let created = call(
                "services.database",
                json!({ "id": resolve(&service)?, "name": name }),
            )?;
            println!("Database {} is ready", created.as_str().unwrap_or_default());
        }
        ServiceAction::Remove { service, keep_data } => {
            call(
                "services.delete",
                json!({ "id": resolve(&service)?, "keep_data": keep_data }),
            )?;
            println!("Removed {service}{}", if keep_data { " (data kept)" } else { "" });
        }
    }
    Ok(())
}
