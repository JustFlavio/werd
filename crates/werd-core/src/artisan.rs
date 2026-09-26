//! Running `php artisan` in a site with the site's PHP line: `about` for the
//! Information tab and `boost:update` when linking a project that uses Boost.

use crate::model::Project;
use crate::process::hidden_command;
use crate::{runtimes, shims};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `php artisan <arguments>` in the site folder, killed after `timeout`.
pub(crate) fn run(root: &Path, project: &Project, arguments: &[&str], timeout: Duration) -> Result<Output> {
    let folder = Path::new(&project.path);
    if !folder.join("artisan").is_file() {
        bail!("{} has no artisan file", project.path);
    }
    let line = runtimes::resolve_line(root, "php", Some(&project.php), "PHP")?;
    let php_dir = runtimes::line_dir(root, "php", &line);
    let _ = shims::ensure_installed(root);
    let path = std::env::join_paths(
        std::iter::once(shims::bin_dir(root))
            .chain(std::env::var_os("PATH").iter().flat_map(std::env::split_paths)),
    )?;
    let mut child = hidden_command(php_dir.join(runtimes::exe("php")))
        .arg("-c")
        .arg(php_dir.join("php.ini"))
        .arg("artisan")
        .args(arguments)
        .current_dir(folder)
        .env("PATH", path)
        .env("WERD_PHP", &line)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Cannot run php artisan")?;
    let read = |stream: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut stream) = stream {
                let mut bytes = Vec::new();
                let _ = stream.read_to_end(&mut bytes);
                text = String::from_utf8_lossy(&bytes).into_owned();
            }
            text
        })
    };
    let stdout = read(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let stderr = read(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "php artisan {} did not finish within {} seconds",
                arguments.join(" "),
                timeout.as_secs()
            );
        }
        thread::sleep(Duration::from_millis(50));
    };
    Ok(Output {
        success: status.success(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// `php artisan about --json`: environment, caches, drivers, storage links and
/// sections added by packages (Livewire, Filament, …).
pub(crate) fn about(root: &Path, project: &Project) -> Result<Value> {
    let output = run(root, project, &["about", "--json"], Duration::from_secs(30))?;
    // Deprecation notices may precede the JSON; it is the last line that parses.
    let parsed = output.stdout.lines().rev().find_map(|line| {
        serde_json::from_str::<Value>(line.trim())
            .ok()
            .filter(Value::is_object)
    });
    match parsed {
        Some(value) => Ok(value),
        None => {
            let detail = if output.stderr.trim().is_empty() {
                output.stdout
            } else {
                output.stderr
            };
            let tail: Vec<&str> = detail.lines().rev().take(6).collect();
            bail!(
                "php artisan about failed: {}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            )
        }
    }
}

/// Whether the project has Laravel Boost installed.
pub(crate) fn has_boost(project: &Project) -> bool {
    Path::new(&project.path).join("vendor/laravel/boost").is_dir()
}

/// `php artisan boost:update`: regenerates Boost's AI guidelines and skills.
pub(crate) fn boost_update(root: &Path, project: &Project) -> Result<String> {
    let output = run(
        root,
        project,
        &["boost:update", "--no-interaction", "--no-ansi"],
        Duration::from_secs(120),
    )?;
    let text = format!("{}{}", output.stdout, output.stderr).trim().to_string();
    if !output.success {
        bail!("php artisan boost:update failed: {text}");
    }
    Ok(text)
}
