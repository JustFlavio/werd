//! `werd php`, `werd node`, `werd install|update|uninstall` and `werd runtimes`.

use crate::call;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::io::Write;
use std::thread;
use std::time::Duration;
use werd_core::jobs::{Job, JobState};
use werd_core::runtimes::RuntimeLine;

#[derive(Subcommand)]
pub enum LineAction {
    /// List available and installed versions (default).
    List {
        /// Include every version, not only installed and supported ones.
        #[arg(long)]
        all: bool,
    },
    /// Install a version line, e.g. `8.4` or `22`.
    Install { line: String },
    /// Update one line, or every installed line with an update.
    Update { line: Option<String> },
    /// Remove an installed line.
    Uninstall { line: String },
    /// Make a line the default for new sites and the command line.
    Use { line: String },
    /// Show or change PHP limits (PHP only).
    Limits {
        /// Maximum upload size in MB.
        #[arg(long)]
        upload_max_mb: Option<u32>,
        /// Memory limit in MB, -1 for unlimited.
        #[arg(long, allow_hyphen_values = true)]
        memory_limit_mb: Option<i64>,
    },
}

fn lines(product: Option<&str>) -> Result<Vec<RuntimeLine>> {
    let rows: Vec<RuntimeLine> = serde_json::from_value(call("runtimes.list", json!({}))?)?;
    Ok(rows
        .into_iter()
        .filter(|row| product.is_none_or(|product| row.product == product))
        .collect())
}

fn print_lines(rows: &[RuntimeLine], show_product: bool) {
    if rows.is_empty() {
        println!("Nothing to show for this platform yet.");
        return;
    }
    for row in rows {
        let mut notes = Vec::new();
        if row.is_default {
            notes.push("default".to_string());
        }
        if row.update_available {
            notes.push(format!("update to {}", row.latest.as_deref().unwrap_or("?")));
        }
        if row.lts {
            notes.push("LTS".into());
        }
        let installed = row
            .installed
            .as_deref()
            .map_or("-".to_string(), |version| format!("installed {version}"));
        let product = if show_product {
            format!("{:12} ", row.product)
        } else {
            String::new()
        };
        println!(
            "{product}{:6} {:18} latest {:10} {}",
            row.line,
            installed,
            row.latest.as_deref().unwrap_or("-"),
            notes.join(", ")
        );
    }
}

/// Polls a background job until it ends, drawing progress on stderr.
pub fn wait(job: Job) -> Result<Job> {
    let mut stderr = std::io::stderr();
    loop {
        let jobs: Vec<Job> = serde_json::from_value(call("jobs.list", json!({}))?)?;
        let current = jobs
            .into_iter()
            .find(|candidate| candidate.id == job.id)
            .context("The job disappeared")?;
        let progress = match current.total {
            Some(total) if total > 0 => format!(
                "{:3}% ({:.1}/{:.1} MiB)",
                current.downloaded * 100 / total,
                current.downloaded as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0
            ),
            _ if current.downloaded > 0 => format!("{:.1} MiB", current.downloaded as f64 / 1_048_576.0),
            _ => String::new(),
        };
        let _ = write!(
            stderr,
            "\r{} {} {}: {} {progress:<30}",
            current.action, current.product, current.line, current.step
        );
        let _ = stderr.flush();
        match current.state {
            JobState::Running => thread::sleep(Duration::from_millis(250)),
            JobState::Done => {
                let _ = writeln!(stderr);
                return Ok(current);
            }
            JobState::Failed => {
                let _ = writeln!(stderr);
                bail!("{}", current.error.unwrap_or_else(|| "The job failed".into()));
            }
        }
    }
}

pub fn install(product: &str, line: &str, update: bool) -> Result<Job> {
    let method = if update {
        "runtimes.update"
    } else {
        "runtimes.install"
    };
    let job: Job = serde_json::from_value(call(method, json!({ "product": product, "line": line }))?)?;
    wait(job)
}

/// Splits `postgresql@17` into its product and line.
pub fn parse_target(target: &str) -> Result<(&str, &str)> {
    target
        .split_once('@')
        .filter(|(product, line)| !product.is_empty() && !line.is_empty())
        .with_context(|| {
            format!("Expected <product>@<line>, e.g. php@8.4 or postgresql@17 (got \"{target}\")")
        })
}

pub fn list_all(json_output: bool) -> Result<()> {
    let rows = lines(None)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        print_lines(&rows, true);
    }
    Ok(())
}

/// Updates every installed line that has a newer patch, or only `product` when given.
pub fn update_all(product: Option<&str>) -> Result<()> {
    let pending: Vec<RuntimeLine> = lines(product)?
        .into_iter()
        .filter(|row| row.update_available)
        .collect();
    if pending.is_empty() {
        println!("Everything is up to date.");
    }
    for row in pending {
        let job = install(&row.product, &row.line, true)?;
        println!(
            "Updated {} {} to {}",
            row.label,
            row.line,
            row.latest.as_deref().unwrap_or(&job.line)
        );
    }
    Ok(())
}

pub fn run_line_action(product: &str, action: Option<LineAction>, json_output: bool) -> Result<()> {
    let label = if product == "php" { "PHP" } else { "Node.js" };
    match action.unwrap_or(LineAction::List { all: false }) {
        LineAction::List { all } => {
            let rows: Vec<RuntimeLine> = lines(Some(product))?
                .into_iter()
                .filter(|row| {
                    all || row.installed.is_some() || row.eol.is_none() && (product != "node" || row.lts)
                })
                .collect();
            if json_output {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else {
                print_lines(&rows, false);
            }
        }
        LineAction::Install { line } => {
            install(product, &line, false)?;
            println!("Installed {label} {line}");
        }
        LineAction::Update { line: Some(line) } => {
            install(product, &line, true)?;
            println!("Updated {label} {line}");
        }
        LineAction::Update { line: None } => update_all(Some(product))?,
        LineAction::Uninstall { line } => {
            call("runtimes.uninstall", json!({ "product": product, "line": line }))?;
            println!("Removed {label} {line}");
        }
        LineAction::Use { line } => {
            call("runtimes.default", json!({ "product": product, "line": line }))?;
            println!("{label} {line} is now the default");
        }
        LineAction::Limits {
            upload_max_mb,
            memory_limit_mb,
        } => {
            if product != "php" {
                bail!("Limits only apply to PHP");
            }
            let mut changes = serde_json::Map::new();
            if let Some(value) = upload_max_mb {
                changes.insert("upload_max_mb".into(), json!(value));
            }
            if let Some(value) = memory_limit_mb {
                changes.insert("memory_limit_mb".into(), json!(value));
            }
            let method = if changes.is_empty() {
                "settings.get"
            } else {
                "settings.set"
            };
            let settings: Value = call(method, Value::Object(changes))?;
            println!("upload_max_filesize = {} MB", settings["upload_max_mb"]);
            println!("memory_limit        = {} MB", settings["memory_limit_mb"]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_need_a_product_and_a_line() {
        assert_eq!(parse_target("postgresql@17").unwrap(), ("postgresql", "17"));
        assert_eq!(parse_target("php@8.4").unwrap(), ("php", "8.4"));
        assert!(parse_target("php").is_err());
        assert!(parse_target("@8.4").is_err());
    }
}
