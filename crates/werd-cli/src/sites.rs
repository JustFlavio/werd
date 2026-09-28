//! Site configuration commands: info, set, link, resolve.

use crate::call;
use crate::runtimes::wait;
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::{json, Value};
use std::io::Write;
use std::time::Duration;
use werd_core::jobs::Job;
use werd_core::jobs::JobState;

fn service_name(instances: &[Value], id: &str) -> String {
    instances
        .iter()
        .find(|instance| instance["id"] == id)
        .and_then(|instance| instance["name"].as_str())
        .unwrap_or(id)
        .to_string()
}

pub fn info(id: &str, json_output: bool) -> Result<()> {
    let snapshot = call("sites.list", json!({}))?;
    let site = snapshot["projects"]
        .as_array()
        .and_then(|projects| projects.iter().find(|project| project["id"] == id))
        .cloned()
        .unwrap_or_default();
    if json_output {
        println!("{}", serde_json::to_string_pretty(&site)?);
        return Ok(());
    }
    let instances: Vec<Value> = serde_json::from_value(call("services.list", json!({}))?)?;
    println!(
        "{}  {}",
        site["name"].as_str().unwrap_or_default(),
        site["path"].as_str().unwrap_or_default()
    );
    println!("status   {}", site["status"].as_str().unwrap_or_default());
    println!("domain   {}", site["domain"].as_str().unwrap_or("-"));
    if let Some(folder) = site["parked"].as_str() {
        println!("parked   {folder}");
    }
    println!("php      {}", site["php"].as_str().unwrap_or("-"));
    println!("node     {}", site["node"].as_str().unwrap_or("default"));
    if let Some(url) = site["url"].as_str() {
        println!("url      {url}");
    }
    if let Ok(details) = call("sites.info", json!({ "id": id })) {
        if let Some(constraint) = details["php_constraint"].as_str() {
            println!("requires php {constraint}");
        }
        let packages: Vec<String> = ["php_packages", "js_packages"]
            .iter()
            .flat_map(|key| details[*key].as_array().cloned().unwrap_or_default())
            .map(|package| {
                format!(
                    "{} {}",
                    package["label"].as_str().unwrap_or_default(),
                    package["version"].as_str().unwrap_or_default()
                )
            })
            .collect();
        if !packages.is_empty() {
            println!("stack    {}", packages.join(", "));
        }
    }
    if let Some(links) = site["links"].as_object().filter(|links| !links.is_empty()) {
        println!("\nservices");
        for (category, link) in links {
            let database = link["database"]
                .as_str()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default();
            println!(
                "  {category:9} {}{database}",
                service_name(&instances, link["instance"].as_str().unwrap_or_default())
            );
        }
    }
    if let Some(pending) = site["requirements"]
        .as_array()
        .filter(|pending| !pending.is_empty())
    {
        println!(
            "\nstill needed (run `werd resolve {}`)",
            site["name"].as_str().unwrap_or_default()
        );
        for requirement in pending {
            let line = requirement["line"]
                .as_str()
                .map(|line| format!(" {line}"))
                .unwrap_or_default();
            println!(
                "  {:9} {}{line}",
                requirement["category"].as_str().unwrap_or_default(),
                requirement["product"].as_str().unwrap_or_default()
            );
        }
    }
    Ok(())
}

pub fn set(
    id: &str,
    php: Option<&str>,
    node: Option<&str>,
    domain: Option<&str>,
    autostart: Option<bool>,
) -> Result<()> {
    if php.is_none() && node.is_none() && domain.is_none() && autostart.is_none() {
        bail!("Pass --php <version>, --node <version>, --domain <name> or --autostart true|false");
    }
    if let Some(enabled) = autostart {
        call("sites.autostart", json!({ "id": id, "autostart": enabled }))?;
        println!(
            "{}",
            if enabled {
                "The site starts whenever Werd starts"
            } else {
                "The site no longer starts with Werd"
            }
        );
    }
    if let Some(domain) = domain {
        let site = call("sites.domain", json!({ "id": id, "domain": domain }))?;
        println!("Domain set to {}", site["domain"].as_str().unwrap_or(domain));
        sync_hosts_if_needed()?;
    }
    if let Some(line) = php {
        call("sites.php", json!({ "id": id, "line": line }))?;
        println!("PHP {line} will be used from the next start");
    }
    if let Some(line) = node {
        call("sites.node", json!({ "id": id, "line": line }))?;
        println!("Node.js {line} is used inside this site");
    }
    Ok(())
}

pub fn link(id: &str, category: &str, service: &str, database: Option<&str>) -> Result<()> {
    let instances: Vec<Value> = serde_json::from_value(call("services.list", json!({}))?)?;
    let instance = instances
        .iter()
        .find(|instance| {
            instance["id"] == service
                || instance["name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(service))
        })
        .and_then(|instance| instance["id"].as_str())
        .map(str::to_string);
    let Some(instance) = instance else {
        bail!("No service named {service}. Run `werd service` to list them.")
    };
    call(
        "sites.link",
        json!({ "id": id, "category": category, "instance": instance, "database": database }),
    )?;
    println!("Linked {category} to {service}. Run `werd env` for the new .env values.");
    Ok(())
}

pub fn resolve_services(id: &str) -> Result<()> {
    let result = call("sites.resolve", json!({ "id": id }))?;
    for job in result["jobs"].as_array().into_iter().flatten() {
        wait(serde_json::from_value::<Job>(job.clone())?)?;
    }
    let links = result["project"]["links"]
        .as_object()
        .map_or(0, serde_json::Map::len);
    println!("{links} service(s) linked. Start the site with `werd up`.");
    Ok(())
}

#[derive(Subcommand)]
pub enum DomainsAction {
    /// Show every site domain and whether the hosts file has it (default).
    Status,
    /// Add the site domains to the hosts file (asks for administrator approval).
    Sync,
    /// Serve sites on https://<name>.test.
    Enable,
    /// Use only https://localhost:<port> addresses.
    Disable,
    /// Serve .test domains on another HTTPS port, e.g. when 443 is taken.
    Port { port: u16 },
}

/// Updates the hosts file when a site domain is missing from it.
pub fn sync_hosts_if_needed() -> Result<()> {
    let status = call("domains.status", json!({}))?;
    let missing: Vec<String> = serde_json::from_value(status["missing"].clone())?;
    if missing.is_empty() {
        return Ok(());
    }
    println!(
        "Adding {} to the hosts file; your system will ask for administrator approval.",
        missing.join(", ")
    );
    let domains: Vec<String> = serde_json::from_value(status["domains"].clone())?;
    werd_core::platform::sync_hosts(&domains)
}

pub fn domains(action: Option<&DomainsAction>, json_output: bool) -> Result<()> {
    match action {
        None | Some(DomainsAction::Status) => {}
        Some(DomainsAction::Sync) => {
            let status = call("domains.status", json!({}))?;
            let domains: Vec<String> = serde_json::from_value(status["domains"].clone())?;
            werd_core::platform::sync_hosts(&domains)?;
            println!(
                "The hosts file now maps {} domain(s) to this computer.",
                domains.len()
            );
            return Ok(());
        }
        Some(DomainsAction::Enable | DomainsAction::Disable) => {
            let enabled = matches!(action, Some(DomainsAction::Enable));
            call("settings.set", json!({ "domains": enabled }))?;
            if enabled {
                sync_hosts_if_needed()?;
            }
        }
        Some(DomainsAction::Port { port }) => {
            call("settings.set", json!({ "https_port": port }))?;
        }
    }
    let status = call("domains.status", json!({}))?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }
    let enabled = status["enabled"]
        .as_bool()
        .context("Invalid answer from the daemon")?;
    if !enabled {
        println!(".test domains are off; sites use https://localhost:<port>. Turn them on with `werd domains enable`.");
        return Ok(());
    }
    println!(".test domains are on (HTTPS port {})", status["https_port"]);
    if let Some(warning) = status["warning"].as_str() {
        println!("warning: {warning}");
    }
    let missing: Vec<String> = serde_json::from_value(status["missing"].clone())?;
    for domain in status["domains"].as_array().into_iter().flatten() {
        let domain = domain.as_str().unwrap_or_default();
        let note = if missing.iter().any(|name| name == domain) {
            "  (not in the hosts file)"
        } else {
            ""
        };
        println!("  {domain}{note}");
    }
    if !missing.is_empty() {
        println!("Run `werd domains sync` to add the missing domains.");
    }
    Ok(())
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Kit {
    React,
    Vue,
    Svelte,
    Livewire,
}

#[derive(Args)]
pub struct NewArgs {
    /// Project folder name, also the site name.
    name: String,
    /// Folder to create the project in.
    #[arg(long, default_value = ".")]
    dir: String,
    /// Starter kit; none by default.
    #[arg(long, value_enum)]
    kit: Option<Kit>,
    /// A community starter kit (Composer package) instead of --kit.
    #[arg(long, conflicts_with = "kit")]
    using: Option<String>,
    /// Use WorkOS for authentication (starter kits).
    #[arg(long, conflicts_with = "no_auth")]
    workos: bool,
    /// Skip authentication scaffolding (starter kits).
    #[arg(long)]
    no_auth: bool,
    /// Add team support (starter kits).
    #[arg(long)]
    teams: bool,
    /// Use PHPUnit instead of Pest.
    #[arg(long)]
    phpunit: bool,
    /// Install Laravel Boost.
    #[arg(long)]
    boost: bool,
    /// Initialize a Git repository.
    #[arg(long)]
    git: bool,
    /// Install and build npm dependencies.
    #[arg(long)]
    npm: bool,
    /// PHP line (default: your default PHP).
    #[arg(long)]
    php: Option<String>,
}

pub fn new_project(arguments: &NewArgs) -> Result<()> {
    let php = match &arguments.php {
        Some(line) => line.clone(),
        None => call("settings.get", json!({}))?["default_php"]
            .as_str()
            .map(str::to_string)
            .context("Choose a PHP version with --php, or set a default with `werd php use <version>`")?,
    };
    let directory = std::fs::canonicalize(&arguments.dir)
        .with_context(|| format!("Folder not found: {}", arguments.dir))?;
    let kit = match (&arguments.using, arguments.kit) {
        (Some(_), _) => Some("custom".to_string()),
        (None, Some(kit)) => kit.to_possible_value().map(|value| value.get_name().to_string()),
        (None, None) => None,
    };
    let auth = if arguments.workos {
        "workos"
    } else if arguments.no_auth {
        "none"
    } else {
        "laravel"
    };
    let job: Job = serde_json::from_value(call(
        "sites.create",
        json!({
            "name": arguments.name,
            "directory": directory.to_string_lossy().trim_start_matches(r"\\?\"),
            "kit": kit,
            "using": arguments.using,
            "auth": auth,
            "teams": arguments.teams,
            "testing": if arguments.phpunit { "phpunit" } else { "pest" },
            "boost": arguments.boost,
            "git": arguments.git,
            "npm": arguments.npm,
            "php": php,
        }),
    )?)?;
    let finished = follow(&job)?;
    let snapshot = call("sites.list", json!({}))?;
    let site = snapshot["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|project| finished.result.as_deref().is_some_and(|id| project["id"] == id));
    let name = site
        .and_then(|site| site["name"].as_str())
        .unwrap_or(&arguments.name);
    println!("Created {name}. Start it with `werd up {name}`.");
    if let Err(error) = sync_hosts_if_needed() {
        println!("The hosts file was not updated: {error:#}");
    }
    Ok(())
}

/// Prints a job's output as it arrives; fails when the job fails.
fn follow(job: &Job) -> Result<Job> {
    let mut printed: u64 = 0;
    let mut step = String::new();
    let mut stdout = std::io::stdout();
    loop {
        let jobs: Vec<Job> = serde_json::from_value(call("jobs.list", json!({}))?)?;
        let current = jobs
            .into_iter()
            .find(|candidate| candidate.id == job.id)
            .context("The job disappeared")?;
        if current.step != step {
            step.clone_from(&current.step);
            let _ = writeln!(stdout, "==> {step}");
        }
        let start = printed.saturating_sub(current.log_dropped) as usize;
        for line in current.log.iter().skip(start) {
            let _ = writeln!(stdout, "{line}");
        }
        printed = current.log_dropped + current.log.len() as u64;
        match current.state {
            JobState::Running => std::thread::sleep(Duration::from_millis(300)),
            JobState::Done => return Ok(current),
            JobState::Failed => bail!("{}", current.error.unwrap_or_else(|| "Failed".into())),
        }
    }
}
