//! New Laravel projects, created with the official Laravel installer.
//!
//! Werd keeps its own copy of `laravel/installer` in `<home>/tools` and runs it
//! with the PHP line the user picked. The installer calls `composer`, `php` and
//! `npm` itself; Werd's shims come first on its PATH, and `WERD_PHP` makes them
//! use the chosen PHP line. The new folder is then linked as a site.

use crate::catalog::Catalog;
use crate::jobs::Progress;
use crate::process::hidden_command;
use crate::runtimes::{self, Fetcher, Installed};
use crate::shims;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;

const STARTER_KITS: [&str; 4] = ["react", "vue", "svelte", "livewire"];

#[derive(Clone, Debug, Deserialize)]
pub struct NewProject {
    /// Folder name of the project, also the site name.
    pub name: String,
    /// Folder the project is created in.
    pub directory: String,
    /// `react`, `vue`, `svelte`, `livewire`, `custom`, or none.
    #[serde(default)]
    pub kit: Option<String>,
    /// Composer package of a custom starter kit.
    #[serde(default)]
    pub using: Option<String>,
    /// `laravel` (built in), `workos` or `none`; starter kits only.
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub teams: bool,
    /// `pest` or `phpunit`.
    #[serde(default = "default_testing")]
    pub testing: String,
    #[serde(default)]
    pub boost: bool,
    #[serde(default)]
    pub git: bool,
    /// Install and build npm dependencies.
    #[serde(default)]
    pub npm: bool,
    pub php: String,
}

fn default_auth() -> String {
    "laravel".into()
}

fn default_testing() -> String {
    "pest".into()
}

impl NewProject {
    pub fn target(&self) -> PathBuf {
        Path::new(&self.directory).join(&self.name)
    }

    /// Checks everything that can be checked before the long download starts.
    pub fn validate(&self, root: &Path) -> Result<()> {
        let valid_name = !self.name.is_empty()
            && self.name.len() <= 100
            && !self.name.starts_with(['.', '-'])
            && self
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !valid_name {
            bail!("Use letters, digits, dashes, dots or underscores for the project name");
        }
        if !Path::new(&self.directory).is_dir() {
            bail!("The folder {} does not exist", self.directory);
        }
        if self.target().exists() {
            bail!("{} already exists", self.target().display());
        }
        if let Some(kit) = &self.kit {
            if kit == "custom" {
                let package = self.using.as_deref().unwrap_or_default();
                let valid = package.split_once('/').is_some_and(|(vendor, name)| {
                    !vendor.is_empty()
                        && !name.is_empty()
                        && package.chars().all(|c| {
                            c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '^' | '~')
                        })
                });
                if !valid {
                    bail!("Enter the Composer package of the starter kit, e.g. vendor/starter-kit");
                }
            } else if !STARTER_KITS.contains(&kit.as_str()) {
                bail!("Unknown starter kit {kit}");
            }
        }
        if !matches!(self.auth.as_str(), "laravel" | "workos" | "none") {
            bail!("Unknown authentication {}", self.auth);
        }
        if !matches!(self.testing.as_str(), "pest" | "phpunit") {
            bail!("Unknown testing framework {}", self.testing);
        }
        let installed = Installed::load(root)?;
        if installed.version("php", &self.php).is_none() {
            bail!("Install PHP {} before creating a project with it", self.php);
        }
        if self.npm && installed.lines("node").is_empty() {
            bail!("Install Node.js to build the frontend, or turn off npm install");
        }
        Ok(())
    }

    /// Options for `laravel new`.
    pub fn installer_arguments(&self) -> Vec<String> {
        let mut arguments = vec![
            "new".to_string(),
            self.name.clone(),
            "--no-interaction".into(),
            "--no-ansi".into(),
            "--database=sqlite".into(),
            format!("--{}", self.testing),
            if self.boost { "--boost" } else { "--no-boost" }.into(),
            if self.npm { "--npm" } else { "--no-node" }.into(),
        ];
        match self.kit.as_deref() {
            Some("custom") => {
                arguments.push(format!("--using={}", self.using.as_deref().unwrap_or_default()))
            }
            Some(kit) => {
                arguments.push(format!("--{kit}"));
                match self.auth.as_str() {
                    "workos" => arguments.push("--workos".into()),
                    "none" => arguments.push("--no-authentication".into()),
                    _ => {}
                }
                if self.teams {
                    arguments.push("--teams".into());
                }
            }
            None => {}
        }
        if self.git {
            arguments.push("--git".into());
        }
        arguments
    }
}

fn installer_dir(root: &Path) -> PathBuf {
    root.join("tools").join("laravel-installer")
}

/// Runs a command, sending each output line to the job log.
fn run_logged(mut command: std::process::Command, progress: &Progress, what: &str) -> Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Cannot start {what}"))?;
    let forward = |stream: Box<dyn Read + Send>, progress: Progress| {
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                progress.log(&line);
            }
        })
    };
    let stdout = child
        .stdout
        .take()
        .map(|s| forward(Box::new(s), progress.clone()));
    let stderr = child
        .stderr
        .take()
        .map(|s| forward(Box::new(s), progress.clone()));
    let status = child.wait()?;
    for reader in [stdout, stderr].into_iter().flatten() {
        let _ = reader.join();
    }
    if !status.success() {
        bail!("{what} failed ({status}); see the output above");
    }
    Ok(())
}

/// Creates the project folder. Returns its path; the caller links it as a site.
pub fn run(
    root: &Path,
    catalog: &Catalog,
    fetcher: &dyn Fetcher,
    project: &NewProject,
    progress: &Progress,
) -> Result<PathBuf> {
    let php_dir = runtimes::line_dir(root, "php", &project.php);
    let php = php_dir.join(runtimes::exe("php"));
    let ini = php_dir.join("php.ini");

    let composer_line = match Installed::load(root)?.lines("composer").into_iter().next() {
        Some(line) => line,
        None => {
            progress.step("Installing Composer");
            let line = catalog
                .product("composer")?
                .available_lines()
                .first()
                .map(|(line, _)| (*line).clone())
                .context("Composer is not available for this platform")?;
            runtimes::install(root, catalog, fetcher, "composer", &line, progress)?;
            line
        }
    };
    let composer = runtimes::line_dir(root, "composer", &composer_line).join("composer.phar");
    shims::ensure_installed(root)?;

    let installer = installer_dir(root);
    let laravel = installer.join("bin").join("laravel");
    if !laravel.is_file() {
        progress.step("Downloading the Laravel installer");
        let _ = fs::remove_dir_all(&installer);
        fs::create_dir_all(installer.parent().context("Invalid tools folder")?)?;
        let mut command = hidden_command(&php);
        command
            .arg("-c")
            .arg(&ini)
            .arg(&composer)
            .args(["create-project", "laravel/installer"])
            .arg(&installer)
            .args(["--no-dev", "--no-interaction", "--no-ansi", "--prefer-dist"]);
        run_logged(command, progress, "Downloading the Laravel installer")?;
    }

    progress.step("Creating the project");
    let path_variable = std::env::join_paths(
        std::iter::once(shims::bin_dir(root))
            .chain(std::env::var_os("PATH").iter().flat_map(std::env::split_paths)),
    )?;
    let mut command = hidden_command(&php);
    command
        .arg("-c")
        .arg(&ini)
        .arg(&laravel)
        .args(project.installer_arguments())
        .current_dir(&project.directory)
        .env("PATH", path_variable)
        .env("WERD_PHP", &project.php)
        .env("WERD_HOME", root);
    run_logged(command, progress, "laravel new")?;
    let target = project.target();
    if !crate::parks::is_laravel(&target) {
        bail!(
            "The installer finished but {} is not a Laravel project",
            target.display()
        );
    }
    Ok(target)
}

/// Points APP_URL of a project Werd just created at its site address.
pub fn set_app_url(project: &Path, url: &str) -> Result<()> {
    let env = project.join(".env");
    let Ok(contents) = fs::read_to_string(&env) else {
        return Ok(());
    };
    let updated: Vec<String> = contents
        .lines()
        .map(|line| {
            if line.starts_with("APP_URL=") {
                format!("APP_URL={url}")
            } else {
                line.to_string()
            }
        })
        .collect();
    let newline = if contents.contains("\r\n") { "\r\n" } else { "\n" };
    fs::write(&env, updated.join(newline) + newline)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(directory: &Path) -> NewProject {
        NewProject {
            name: "shop".into(),
            directory: directory.to_string_lossy().into_owned(),
            kit: Some("react".into()),
            using: None,
            auth: "workos".into(),
            teams: true,
            testing: "pest".into(),
            boost: true,
            git: true,
            npm: false,
            php: "8.4".into(),
        }
    }

    #[test]
    fn installer_arguments_follow_the_choices() {
        let directory = tempfile::tempdir().unwrap();
        let mut request = project(directory.path());
        let arguments = request.installer_arguments().join(" ");
        assert_eq!(
            arguments,
            "new shop --no-interaction --no-ansi --database=sqlite --pest --boost --no-node --react --workos --teams --git"
        );
        request.kit = None;
        request.git = false;
        request.boost = false;
        let arguments = request.installer_arguments().join(" ");
        assert!(!arguments.contains("--workos") && !arguments.contains("--teams"));
        assert!(arguments.ends_with("--no-boost --no-node"));
        request.kit = Some("custom".into());
        request.using = Some("acme/kit".into());
        assert!(request
            .installer_arguments()
            .contains(&"--using=acme/kit".to_string()));
    }

    #[test]
    fn requests_are_validated_before_anything_runs() {
        let root = tempfile::tempdir().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut request = project(directory.path());
        assert!(request
            .validate(root.path())
            .unwrap_err()
            .to_string()
            .contains("Install PHP 8.4"));
        let mut installed = Installed::default();
        installed.set("php", "8.4", "8.4.26");
        installed.save(root.path()).unwrap();
        request.validate(root.path()).unwrap();

        request.name = "my shop".into();
        assert!(request.validate(root.path()).is_err());
        request.name = "shop".into();
        fs::create_dir(directory.path().join("shop")).unwrap();
        assert!(request
            .validate(root.path())
            .unwrap_err()
            .to_string()
            .contains("already exists"));
        request.name = "other".into();
        request.kit = Some("custom".into());
        request.using = Some("not a package; rm -rf".into());
        assert!(request.validate(root.path()).is_err());
        request.using = Some("acme/kit".into());
        request.npm = true;
        assert!(request
            .validate(root.path())
            .unwrap_err()
            .to_string()
            .contains("Node.js"));
    }

    #[test]
    fn app_url_is_rewritten_in_place() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(
            folder.path().join(".env"),
            "APP_NAME=Shop\r\nAPP_URL=http://localhost\r\nDB=x\r\n",
        )
        .unwrap();
        set_app_url(folder.path(), "https://shop.test").unwrap();
        assert_eq!(
            fs::read_to_string(folder.path().join(".env")).unwrap(),
            "APP_NAME=Shop\r\nAPP_URL=https://shop.test\r\nDB=x\r\n"
        );
    }
}
