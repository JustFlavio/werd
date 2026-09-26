//! `werd`: command line client for the Werd daemon.

// Printing to stdout is this binary's job.
#![allow(clippy::print_stdout)]

mod runtimes;
mod services;
mod sites;

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use serde_json::{json, Value};
use werd_core::{daemon_executable, ensure_daemon, rpc, DoctorResult, Project, Snapshot, LOG_SOURCES};

#[derive(Parser)]
#[command(
    name = "werd",
    version,
    about = "Local Laravel environments without Docker",
    propagate_version = true
)]
struct Cli {
    /// Print raw JSON instead of human-readable output.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List linked projects and their status.
    #[command(visible_alias = "status")]
    List,
    /// Link a Laravel project folder.
    Add {
        /// Project folder (defaults to the current directory).
        #[arg(default_value = ".")]
        path: String,
    },
    /// Start a project and its services.
    #[command(visible_alias = "start")]
    Up { project: String },
    /// Stop a project and its services.
    #[command(visible_alias = "stop")]
    Down { project: String },
    /// Open a running project in the browser.
    Open { project: String },
    /// Print the .env values for a started project.
    Env { project: String },
    /// Show the last lines of a project log.
    Logs {
        project: String,
        /// Log source.
        #[arg(default_value = "werd", value_parser = clap::builder::PossibleValuesParser::new(LOG_SOURCES))]
        source: String,
    },
    /// Pick new ports for a stopped project on its next start.
    ResetPorts { project: String },
    /// Show a site: PHP and Node versions, linked services and what is still missing.
    Info { project: String },
    /// Choose the PHP or Node.js version, or the .test domain, of a site.
    Set {
        project: String,
        #[arg(long)]
        php: Option<String>,
        #[arg(long)]
        node: Option<String>,
        /// e.g. `shop` or `shop.test`
        #[arg(long)]
        domain: Option<String>,
    },
    /// Link a site category (database, cache, queue, mail, storage, search) to a service.
    Link {
        project: String,
        category: String,
        /// Service name or id, as shown by `werd service`.
        service: String,
        /// Database or bucket name (defaults to the site name).
        #[arg(long)]
        database: Option<String>,
    },
    /// Remove the service linked to a site category.
    Unlink { project: String, category: String },
    /// Create and link the services a site asks for in werd.yml.
    Resolve { project: String },
    /// Forget a site. Its folder and services are left untouched.
    #[command(visible_alias = "rm")]
    Remove { project: String },
    /// Manage PHP versions (list, install, update, use, limits).
    Php {
        #[command(subcommand)]
        action: Option<runtimes::LineAction>,
    },
    /// Manage Node.js versions (list, install, update, use).
    Node {
        #[command(subcommand)]
        action: Option<runtimes::LineAction>,
    },
    /// List everything Werd can install on this platform.
    Runtimes,
    /// Install a version line, e.g. `werd install postgresql@17`.
    Install {
        /// <product>@<line>, as shown by `werd runtimes`.
        target: String,
    },
    /// Update one line (`php@8.4`), or everything that has an update.
    Update { target: Option<String> },
    /// Remove an installed line, e.g. `werd uninstall node@18`.
    Uninstall { target: String },
    /// Manage shared services: PostgreSQL, MySQL, MariaDB, Redis, Mailpit, …
    #[command(visible_alias = "services")]
    Service {
        #[command(subcommand)]
        action: Option<services::ServiceAction>,
    },
    /// Add or remove the php, composer, node, npm and npx commands from your PATH.
    Path {
        #[command(subcommand)]
        action: PathAction,
    },
    /// Show the runtime catalog, or refresh it with `werd catalog refresh`.
    Catalog {
        #[command(subcommand)]
        action: Option<CatalogAction>,
    },
    /// Show .test domains, update the hosts file or change the HTTPS port.
    Domains {
        #[command(subcommand)]
        action: Option<sites::DomainsAction>,
    },
    /// Trust Werd's local HTTPS certificate authority.
    TrustCa,
    /// Check the environment.
    Doctor,
    /// Print a shell completion script.
    Completions { shell: Shell },
}

#[derive(Subcommand)]
enum PathAction {
    /// Install the shims and add Werd's bin folder to your user PATH.
    Enable,
    /// Remove Werd's bin folder from your user PATH and delete the shims.
    Disable,
}

#[derive(Subcommand)]
enum CatalogAction {
    /// Download the latest catalog.
    Refresh,
}

pub(crate) fn call(method: &str, params: Value) -> Result<Value> {
    ensure_daemon(&daemon_executable()?)?;
    rpc(method, params)
}

/// Accepts a project id, or a unique project name.
fn resolve(project: &str) -> Result<String> {
    let snapshot: Snapshot = serde_json::from_value(call("list", json!({}))?)?;
    if let Some(found) = snapshot.projects.iter().find(|candidate| candidate.id == project) {
        return Ok(found.id.clone());
    }
    let matches: Vec<&Project> = snapshot
        .projects
        .iter()
        .filter(|candidate| {
            candidate.name.eq_ignore_ascii_case(project)
                || candidate
                    .domain
                    .as_deref()
                    .is_some_and(|domain| domain.eq_ignore_ascii_case(project))
        })
        .collect();
    match matches.as_slice() {
        [single] => Ok(single.id.clone()),
        [] => bail!("No project named {project}. Run `werd list` to see linked projects."),
        _ => bail!("Several projects are named {project}; use the id from `werd list`."),
    }
}

fn status_label(project: &Project) -> String {
    serde_json::to_value(project.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn print_projects(snapshot: &Snapshot) {
    if snapshot.projects.is_empty() {
        println!("No projects yet. Link one with `werd add <folder>`.");
        return;
    }
    let width = snapshot
        .projects
        .iter()
        .map(|project| project.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    println!("{:width$}  {:8}  {:32}  PATH", "NAME", "STATUS", "URL");
    for project in &snapshot.projects {
        println!(
            "{:width$}  {:8}  {:32}  {}",
            project.name,
            status_label(project),
            project.url.as_deref().unwrap_or("-"),
            project.path
        );
        if let Some(error) = &project.error {
            println!("{:width$}  error: {error}", "");
        }
    }
}

fn print_project(verb: &str, project: &Project) {
    match &project.url {
        Some(url) => println!("{verb} {} at {url}", project.name),
        None => println!("{verb} {}", project.name),
    }
}

fn run(cli: Cli) -> Result<()> {
    let (method, params) = match &cli.command {
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "werd", &mut std::io::stdout());
            return Ok(());
        }
        Command::Php { .. } | Command::Node { .. } => {
            let (product, action) = match cli.command {
                Command::Php { action } => ("php", action),
                Command::Node { action } => ("node", action),
                _ => unreachable!(),
            };
            return runtimes::run_line_action(product, action, cli.json);
        }
        Command::Runtimes => return runtimes::list_all(cli.json),
        Command::Service { .. } => {
            let Command::Service { action } = cli.command else {
                unreachable!()
            };
            return services::run(action, cli.json);
        }
        Command::Install { target } => {
            let (product, line) = runtimes::parse_target(target)?;
            runtimes::install(product, line, false)?;
            println!("Installed {product} {line}");
            return Ok(());
        }
        Command::Update { target } => {
            return match target.as_deref().map(runtimes::parse_target).transpose()? {
                Some((product, line)) => {
                    runtimes::install(product, line, true)?;
                    println!("Updated {product} {line}");
                    Ok(())
                }
                None => runtimes::update_all(None),
            };
        }
        Command::Uninstall { target } => {
            let (product, line) = runtimes::parse_target(target)?;
            call("runtimes.uninstall", json!({ "product": product, "line": line }))?;
            println!("Removed {product} {line}");
            return Ok(());
        }
        Command::Path { action } => {
            let enable = matches!(action, PathAction::Enable);
            call(if enable { "path.enable" } else { "path.disable" }, json!({}))?;
            if enable {
                let info = call("system.info", json!({}))?;
                let bin = info["bin"].as_str().unwrap_or("the Werd bin folder");
                println!("Added {bin} to your PATH. Open a new terminal to use php, composer and node.");
            } else {
                println!("Removed Werd from your PATH.");
            }
            return Ok(());
        }
        Command::Catalog {
            action: Some(CatalogAction::Refresh),
        } => ("catalog.refresh", json!({})),
        Command::Catalog { action: None } => ("catalog.get", json!({})),
        Command::List => ("list", json!({})),
        Command::Add { path } => {
            let absolute =
                std::fs::canonicalize(path).with_context(|| format!("Folder not found: {path}"))?;
            ("add", json!({ "path": absolute }))
        }
        Command::Up { project } => ("start", json!({ "id": resolve(project)? })),
        Command::Down { project } => ("stop", json!({ "id": resolve(project)? })),
        Command::Open { project } => ("open", json!({ "id": resolve(project)? })),
        Command::Env { project } => ("env", json!({ "id": resolve(project)? })),
        Command::Logs { project, source } => ("logs", json!({ "id": resolve(project)?, "service": source })),
        Command::ResetPorts { project } => ("reset-ports", json!({ "id": resolve(project)? })),
        Command::Info { project } => return sites::info(&resolve(project)?, cli.json),
        Command::Set {
            project,
            php,
            node,
            domain,
        } => {
            return sites::set(
                &resolve(project)?,
                php.as_deref(),
                node.as_deref(),
                domain.as_deref(),
            )
        }
        Command::Domains { action } => return sites::domains(action.as_ref(), cli.json),
        Command::Link {
            project,
            category,
            service,
            database,
        } => {
            return sites::link(&resolve(project)?, category, service, database.as_deref());
        }
        Command::Unlink { project, category } => {
            call(
                "sites.unlink",
                json!({ "id": resolve(project)?, "category": category }),
            )?;
            println!("Unlinked {category}");
            return Ok(());
        }
        Command::Resolve { project } => return sites::resolve_services(&resolve(project)?),
        Command::Remove { project } => {
            call("sites.remove", json!({ "id": resolve(project)? }))?;
            println!("Removed {project} from Werd; its folder and services are untouched.");
            return Ok(());
        }

        Command::TrustCa => ("trust-ca", json!({})),
        Command::Doctor => ("doctor", json!({})),
    };

    let result = call(method, params)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    match cli.command {
        Command::List => print_projects(&serde_json::from_value(result)?),
        Command::Add { .. } => {
            let project: Project = serde_json::from_value(result)?;
            println!("Linked {} ({})", project.name, project.path);
            if let Err(error) = sites::sync_hosts_if_needed() {
                println!("The hosts file was not updated: {error:#}");
                println!("Run `werd domains sync` to retry.");
            }
            println!("Start it with `werd up {}`", project.name);
        }
        Command::Up { .. } => print_project("Started", &serde_json::from_value(result)?),
        Command::Down { .. } => print_project("Stopped", &serde_json::from_value(result)?),
        Command::ResetPorts { .. } => {
            print_project("Ports reset for", &serde_json::from_value(result)?);
            println!("New ports are assigned on the next start; update your .env afterwards.");
        }
        Command::Open { .. } | Command::Env { .. } | Command::TrustCa => {
            println!("{}", result.as_str().unwrap_or_default());
        }
        Command::Logs { .. } => {
            for line in serde_json::from_value::<Vec<String>>(result)? {
                println!("{line}");
            }
        }
        Command::Catalog { .. } => {
            println!(
                "Catalog generated {}",
                result["generated"].as_str().unwrap_or("?")
            );
            if let Some(platform) = result["platform"].as_str() {
                println!("Platform {platform}");
            }
        }
        Command::Doctor => {
            for check in serde_json::from_value::<Vec<DoctorResult>>(result)? {
                println!(
                    "[{}] {}: {}",
                    if check.ok { "ok" } else { "!!" },
                    check.label,
                    check.detail
                );
            }
        }
        Command::Completions { .. }
        | Command::Php { .. }
        | Command::Node { .. }
        | Command::Runtimes
        | Command::Install { .. }
        | Command::Update { .. }
        | Command::Uninstall { .. }
        | Command::Service { .. }
        | Command::Info { .. }
        | Command::Set { .. }
        | Command::Link { .. }
        | Command::Unlink { .. }
        | Command::Resolve { .. }
        | Command::Remove { .. }
        | Command::Path { .. }
        | Command::Domains { .. } => {}
    }
    Ok(())
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_aliases_and_defaults() {
        let cli = Cli::try_parse_from(["werd", "start", "shop"]).unwrap();
        assert!(matches!(cli.command, Command::Up { project } if project == "shop"));
        let cli = Cli::try_parse_from(["werd", "logs", "shop", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(cli.command, Command::Logs { source, .. } if source == "werd"));
        assert!(Cli::try_parse_from(["werd", "logs", "shop", "nginx"]).is_err());
    }

    #[test]
    fn parses_runtime_commands() {
        let cli = Cli::try_parse_from(["werd", "php", "install", "8.4"]).unwrap();
        assert!(
            matches!(cli.command, Command::Php { action: Some(runtimes::LineAction::Install { line }) } if line == "8.4")
        );
        let cli = Cli::try_parse_from(["werd", "php", "limits", "--memory-limit-mb", "-1"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Php {
                action: Some(runtimes::LineAction::Limits {
                    memory_limit_mb: Some(-1),
                    ..
                })
            }
        ));
        assert!(matches!(
            Cli::try_parse_from(["werd", "node"]).unwrap().command,
            Command::Node { action: None }
        ));
        assert!(matches!(
            Cli::try_parse_from(["werd", "update"]).unwrap().command,
            Command::Update { target: None }
        ));
    }
}
