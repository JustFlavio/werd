//! `werd-shim`: copied into `<werd home>/bin` as `php`, `composer`, `node`, `npm`
//! and `npx`. It picks the runtime version for the current folder and runs it.
//!
//! Version resolution, first match wins:
//! 1. `werd.yml` (`php:` / `node:`) in the current folder or a parent;
//! 2. for Node, `.nvmrc` or `.node-version` in the current folder or a parent;
//! 3. the default chosen in Werd (`config.json`);
//! 4. the newest installed line.
//!
//! It reads Werd's files directly instead of asking the daemon, so it stays fast
//! and works while the daemon is stopped.

use std::cmp::Ordering;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Tool {
    Php,
    Composer,
    Node,
    Npm,
    Npx,
}

impl Tool {
    fn from_program(path: &Path) -> Option<Self> {
        let name = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
        Some(match name.as_str() {
            "php" => Self::Php,
            "composer" => Self::Composer,
            "node" => Self::Node,
            "npm" => Self::Npm,
            "npx" => Self::Npx,
            _ => return None,
        })
    }

    fn runtime(self) -> &'static str {
        match self {
            Self::Php | Self::Composer => "php",
            Self::Node | Self::Npm | Self::Npx => "node",
        }
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    }
}

fn compare_versions(a: &str, b: &str) -> Ordering {
    let parse =
        |value: &str| -> Vec<u64> { value.split('.').map(|part| part.parse().unwrap_or(0)).collect() };
    parse(a).cmp(&parse(b))
}

fn read_json(path: &Path) -> serde_json::Value {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Lines of `product` recorded in `runtimes/installed.json`.
fn installed_lines(home: &Path, product: &str) -> Vec<String> {
    let installed = read_json(&home.join("runtimes").join("installed.json"));
    let mut lines: Vec<String> = installed[product]
        .as_object()
        .map(|lines| lines.keys().cloned().collect())
        .unwrap_or_default();
    lines.sort_by(|a, b| compare_versions(b, a));
    lines
}

/// `php: "8.4"` or `node: 22` from a werd.yml, without a YAML parser.
fn manifest_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let value = line.strip_prefix(key)?.strip_prefix(':')?.trim();
        let value = value
            .split('#')
            .next()?
            .trim()
            .trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Major version from an `.nvmrc` value (`22`, `v22.3.0`, `22.3`); aliases such as `lts/*` give `None`.
fn nvmrc_major(text: &str) -> Option<String> {
    let value = text.trim().trim_start_matches('v');
    let major = value.split('.').next()?;
    (!major.is_empty() && major.bytes().all(|b| b.is_ascii_digit())).then(|| major.to_string())
}

/// The version requested by the nearest project file, if any.
fn requested(start: &Path, tool: Tool) -> Option<String> {
    for folder in start.ancestors() {
        if let Ok(text) = fs::read_to_string(folder.join("werd.yml")) {
            if let Some(value) = manifest_value(&text, tool.runtime()) {
                return Some(value);
            }
        }
        if tool.runtime() == "node" {
            for file in [".nvmrc", ".node-version"] {
                if let Some(major) = fs::read_to_string(folder.join(file))
                    .ok()
                    .and_then(|text| nvmrc_major(&text))
                {
                    return Some(major);
                }
            }
        }
    }
    None
}

/// Picks the line to run: requested if installed, then the default, then the newest.
fn resolve(home: &Path, tool: Tool, cwd: &Path) -> Result<String, String> {
    let product = tool.runtime();
    let installed = installed_lines(home, product);
    let label = if product == "php" { "PHP" } else { "Node.js" };
    if installed.is_empty() {
        return Err(format!("werd: no {label} version is installed. Install one from Werd or with `werd {product} install <version>`."));
    }
    if let Some(wanted) = requested(cwd, tool) {
        if installed.contains(&wanted) {
            return Ok(wanted);
        }
        return Err(format!("werd: this project asks for {label} {wanted}, which is not installed. Run `werd {product} install {wanted}`."));
    }
    let settings = read_json(&home.join("config.json"));
    let default = settings[format!("default_{product}")]
        .as_str()
        .map(str::to_string);
    Ok(default
        .filter(|line| installed.contains(line))
        .unwrap_or_else(|| installed[0].clone()))
}

/// The program and leading arguments to run for `tool`.
fn target(home: &Path, tool: Tool, line: &str) -> Result<(PathBuf, Vec<OsString>), String> {
    let runtimes = home.join("runtimes");
    let php = runtimes.join("php").join(line).join(exe("php"));
    let node_dir = runtimes.join("node").join(line);
    let npm_script = |name: &str| node_dir.join("node_modules/npm/bin").join(name).into_os_string();
    Ok(match tool {
        Tool::Php => (php, Vec::new()),
        Tool::Composer => {
            let phar = installed_lines(home, "composer")
                .first()
                .map(|composer| runtimes.join("composer").join(composer).join("composer.phar"))
                .ok_or("werd: Composer is not installed. Install it from Werd or with `werd install composer@2`.")?;
            (php, vec![phar.into_os_string()])
        }
        Tool::Node => (node_dir.join(exe("node")), Vec::new()),
        Tool::Npm => (node_dir.join(exe("node")), vec![npm_script("npm-cli.js")]),
        Tool::Npx => (node_dir.join(exe("node")), vec![npm_script("npx-cli.js")]),
    })
}

fn run() -> Result<i32, String> {
    let mut args = env::args_os();
    let program = PathBuf::from(args.next().unwrap_or_default());
    let tool = Tool::from_program(&program)
        .ok_or("werd-shim must be copied as php, composer, node, npm or npx; Werd does this for you")?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    // <home>/bin/<tool>.exe
    let home = executable
        .parent()
        .and_then(Path::parent)
        .ok_or("werd: cannot locate the Werd data folder")?;
    let cwd = env::current_dir().map_err(|error| error.to_string())?;
    let line = resolve(home, tool, &cwd)?;
    let (binary, leading) = target(home, tool, &line)?;
    if !binary.is_file() {
        return Err(format!(
            "werd: {} is missing; reinstall it from Werd",
            binary.display()
        ));
    }
    let status = Command::new(&binary)
        .args(leading)
        .args(args)
        .status()
        .map_err(|error| format!("werd: cannot run {}: {error}", binary.display()))?;
    Ok(status.code().unwrap_or(1))
}

fn main() {
    match run() {
        // `exit` keeps full Windows exit codes (e.g. 0xC000013A after Ctrl+C), which ExitCode cannot.
        Ok(code) => std::process::exit(code),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_with(installed: &str, config: &str) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("runtimes")).unwrap();
        fs::write(home.path().join("runtimes/installed.json"), installed).unwrap();
        fs::write(home.path().join("config.json"), config).unwrap();
        home
    }

    const INSTALLED: &str = r#"{"php":{"8.4":{"version":"8.4.26"},"8.5":{"version":"8.5.11"}},
        "node":{"20":{"version":"20.1.0"},"22":{"version":"22.23.3"}}}"#;

    #[test]
    fn tools_come_from_the_program_name() {
        assert_eq!(
            Tool::from_program(Path::new(r"C:\x\bin\php.exe")),
            Some(Tool::Php)
        );
        assert_eq!(Tool::from_program(Path::new("/home/x/bin/npx")), Some(Tool::Npx));
        assert_eq!(Tool::from_program(Path::new("werd-shim")), None);
    }

    #[test]
    fn default_then_newest_line() {
        let project = tempfile::tempdir().unwrap();
        let home = home_with(INSTALLED, r#"{"default_php":"8.4"}"#);
        assert_eq!(resolve(home.path(), Tool::Php, project.path()).unwrap(), "8.4");
        assert_eq!(resolve(home.path(), Tool::Node, project.path()).unwrap(), "22");
    }

    #[test]
    fn project_files_win_over_the_default() {
        let project = tempfile::tempdir().unwrap();
        let nested = project.path().join("app/Http");
        fs::create_dir_all(&nested).unwrap();
        fs::write(
            project.path().join("werd.yml"),
            "version: 1\nphp: '8.5'  # pinned\n",
        )
        .unwrap();
        fs::write(project.path().join(".nvmrc"), "v20.11.1\n").unwrap();
        let home = home_with(INSTALLED, r#"{"default_php":"8.4","default_node":"22"}"#);
        assert_eq!(resolve(home.path(), Tool::Composer, &nested).unwrap(), "8.5");
        assert_eq!(resolve(home.path(), Tool::Npm, &nested).unwrap(), "20");
    }

    #[test]
    fn missing_versions_explain_what_to_do() {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join(".nvmrc"), "18").unwrap();
        let home = home_with(INSTALLED, "{}");
        assert!(resolve(home.path(), Tool::Node, project.path())
            .unwrap_err()
            .contains("werd node install 18"));
        let empty = home_with("{}", "{}");
        assert!(resolve(empty.path(), Tool::Php, project.path())
            .unwrap_err()
            .contains("no PHP version"));
    }

    #[test]
    fn nvmrc_aliases_are_ignored() {
        assert_eq!(nvmrc_major("lts/*"), None);
        assert_eq!(nvmrc_major("v22.3.0"), Some("22".into()));
        assert_eq!(manifest_value("node: 22\n", "node"), Some("22".into()));
        assert_eq!(manifest_value("phpstan: 1\n", "php"), None);
    }
}
