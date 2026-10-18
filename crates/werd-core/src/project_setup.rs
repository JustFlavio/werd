//! Dependency installation requested from the project's setup screen.
use crate::{
    jobs::Progress,
    model::Project,
    process::{hidden_command, spawn_managed},
    runtimes, shims,
};
use anyhow::{bail, Context, Result};
use std::{
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) fn command(root: &Path, project: &Project, kind: &str) -> Result<Command> {
    let mut command = match kind {
        "key" | "config" => {
            let php = runtimes::resolve_line(root, "php", Some(&project.php), "PHP")?;
            let directory = runtimes::line_dir(root, "php", &php);
            let mut command = hidden_command(directory.join(runtimes::exe("php")));
            command
                .arg("-c")
                .arg(directory.join("php.ini"))
                .arg("artisan")
                .arg(if kind == "key" {
                    "key:generate"
                } else {
                    "config:clear"
                })
                .arg("--no-interaction");
            command
        }
        "composer" => {
            let php = runtimes::resolve_line(root, "php", Some(&project.php), "PHP")?;
            let composer = runtimes::resolve_line(root, "composer", None, "Composer")?;
            let directory = runtimes::line_dir(root, "php", &php);
            let mut command = hidden_command(directory.join(runtimes::exe("php")));
            command
                .arg("-c")
                .arg(directory.join("php.ini"))
                .arg(runtimes::line_dir(root, "composer", &composer).join("composer.phar"))
                .args(["install", "--no-interaction"]);
            command
        }
        "npm" => {
            let line = crate::vite::node_line(root, project)?;
            let directory = runtimes::line_dir(root, "node", &line);
            let binary = directory.join(if cfg!(windows) { "node.exe" } else { "bin/node" });
            let cli = directory.join(if cfg!(windows) {
                "node_modules/npm/bin/npm-cli.js"
            } else {
                "lib/node_modules/npm/bin/npm-cli.js"
            });
            if !cli.is_file() {
                bail!("The installed Node.js version is missing npm. Reinstall it from runtime settings.");
            }
            let mut command = hidden_command(binary);
            command.env("WERD_NODE", &line);
            command.arg(cli).arg("install");
            command
        }
        _ => bail!("Unknown dependency installer"),
    };
    shims::ensure_installed(root)?;
    command
        .current_dir(&project.path)
        .env("WERD_HOME", root)
        .env("WERD_PHP", &project.php)
        .env(
            "PATH",
            std::env::join_paths(
                std::iter::once(shims::bin_dir(root))
                    .chain(std::env::var_os("PATH").iter().flat_map(std::env::split_paths)),
            )?,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

pub(crate) fn run(command: Command, progress: &Progress) -> Result<()> {
    let mut child = spawn_managed("project-dependencies", command)?;
    let forward = |stream: Box<dyn Read + Send>, progress: Progress| {
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                progress.log(&line);
            }
        })
    };
    let stdout = child
        .child
        .stdout
        .take()
        .map(|stream| forward(Box::new(stream), progress.clone()));
    let stderr = child
        .child
        .stderr
        .take()
        .map(|stream| forward(Box::new(stream), progress.clone()));
    let result = (|| {
        let deadline = Instant::now() + Duration::from_secs(1800);
        loop {
            if let Some(status) = child.child.try_wait()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                bail!("Dependency installation timed out. Retry from the project setup screen.");
            }
            thread::sleep(Duration::from_millis(100));
        }
    })();
    child.kill(); // Finalize the process registry even on normal completion.
    for reader in [stdout, stderr].into_iter().flatten() {
        let _ = reader.join();
    }
    if !result.context("Cannot install project dependencies")?.success() {
        bail!("Dependency installation failed. Expand the output for details, then retry.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_dependency_jobs_capture_output_and_exit() {
        let jobs = crate::jobs::Jobs::default();
        let mut command = if cfg!(windows) {
            let mut command = hidden_command("cmd.exe");
            command.args(["/C", "echo dependency-output"]);
            command
        } else {
            let mut command = hidden_command("printf");
            command.arg("dependency-output\\n");
            command
        };
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let job = jobs
            .start("project-setup", "test", "composer", move |progress| {
                run(command, progress)
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let current = jobs.list().into_iter().find(|entry| entry.id == job.id).unwrap();
            if current.state != crate::jobs::JobState::Running {
                assert_eq!(current.state, crate::jobs::JobState::Done, "{:?}", current.error);
                assert!(current.log.iter().any(|line| line.contains("dependency-output")));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
    }
}
