//! What Werd can tell about a project folder before and after linking it:
//! whether it is Laravel, which PHP it asks for, the framework version and
//! the notable packages it uses. Everything is read from the project's own
//! files; nothing runs.

use crate::runtimes::Installed;
use crate::settings::Settings;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

/// Packages worth showing on the site's Information tab, with display names.
const NOTABLE_PHP: [(&str, &str); 16] = [
    ("laravel/framework", "Laravel"),
    ("filament/filament", "Filament"),
    ("livewire/livewire", "Livewire"),
    ("livewire/flux", "Flux"),
    ("inertiajs/inertia-laravel", "Inertia"),
    ("laravel/horizon", "Horizon"),
    ("laravel/octane", "Octane"),
    ("laravel/reverb", "Reverb"),
    ("laravel/sanctum", "Sanctum"),
    ("laravel/passport", "Passport"),
    ("laravel/fortify", "Fortify"),
    ("laravel/scout", "Scout"),
    ("laravel/cashier", "Cashier"),
    ("laravel/boost", "Boost"),
    ("pestphp/pest", "Pest"),
    ("phpunit/phpunit", "PHPUnit"),
];
const NOTABLE_JS: [(&str, &str); 7] = [
    ("react", "React"),
    ("vue", "Vue"),
    ("svelte", "Svelte"),
    ("@inertiajs/react", "Inertia React"),
    ("@inertiajs/vue3", "Inertia Vue"),
    ("tailwindcss", "Tailwind CSS"),
    ("vite", "Vite"),
];

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Package {
    pub name: String,
    pub label: String,
    /// Installed version from the lock file, else the requested constraint.
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProjectInfo {
    pub path: String,
    /// Folder name, the suggested site name.
    pub name: String,
    pub laravel: bool,
    /// `require.php` of composer.json, e.g. `^8.2`.
    pub php_constraint: Option<String>,
    /// Best PHP line: installed and allowed, else the newest allowed line.
    pub suggested_php: Option<String>,
    /// Whether `suggested_php` still needs to be installed.
    pub suggested_php_installed: bool,
    pub php_packages: Vec<Package>,
    pub js_packages: Vec<Package>,
    /// From `.nvmrc`, `.node-version` or `engines.node`.
    pub node: Option<String>,
    pub werd_yml: bool,
    pub env_file: bool,
}

fn read_json(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(Value::Null)
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = text.split(['.', '-', '+']).map(|part| part.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().and_then(Result::ok).unwrap_or(0);
    let patch = parts.next().and_then(Result::ok).unwrap_or(0);
    Some((major, minor, patch))
}

/// Whether a PHP line (`8.4`) satisfies one constraint atom such as `^8.2`,
/// `~8.3.0`, `>=8.1`, `<8.5`, `8.4.*` or `8.4`. The line stands for all of its
/// patches, so an atom matches when some patch of the line would.
fn atom_allows(atom: &str, line: (u64, u64)) -> bool {
    let atom = atom.trim();
    if atom.is_empty() || atom == "*" {
        return true;
    }
    let (low, high) = ((line.0, line.1, 0), (line.0, line.1, u64::MAX));
    let version_of = |text: &str| parse_version(text.trim_end_matches(".*"));
    if let Some(rest) = atom.strip_prefix('^') {
        let Some(min) = version_of(rest) else { return false };
        return high >= min && low < (min.0 + 1, 0, 0);
    }
    if let Some(rest) = atom.strip_prefix('~') {
        let Some(min) = version_of(rest) else { return false };
        let dots = rest.matches('.').count();
        let max = if dots >= 2 {
            (min.0, min.1 + 1, 0)
        } else {
            (min.0 + 1, 0, 0)
        };
        return high >= min && low < max;
    }
    for (operator, compare) in [
        (
            ">=",
            (|v, b| v >= b) as fn((u64, u64, u64), (u64, u64, u64)) -> bool,
        ),
        ("<=", |v, b| v <= b),
        (">", |v, b| v > b),
        ("<", |v, b| v < b),
        ("!=", |_, _| true),
    ] {
        if let Some(rest) = atom.strip_prefix(operator) {
            let Some(bound) = version_of(rest) else {
                return false;
            };
            // Test the patch of the line closest to satisfying the bound.
            let probe = if operator.starts_with('>') { high } else { low };
            return compare(probe, bound);
        }
    }
    let exact = atom.trim_start_matches('=');
    let Some(version) = version_of(exact) else {
        return false;
    };
    let parts = exact.trim_end_matches(".*").matches('.').count();
    match parts {
        0 => version.0 == line.0,
        _ => (version.0, version.1) == line,
    }
}

/// Whether a PHP line satisfies a Composer constraint (`||` alternatives of
/// space or comma separated atoms).
pub fn allows(constraint: &str, line: &str) -> bool {
    let Some((major, minor, _)) = parse_version(line) else {
        return false;
    };
    constraint
        .split('|')
        .filter(|part| !part.trim().is_empty())
        .any(|alternative| {
            alternative
                .split([' ', ','])
                .filter(|atom| !atom.is_empty())
                .all(|atom| atom_allows(atom, (major, minor)))
        })
}

/// Picks the PHP line for a project: installed lines first, newest first.
fn suggest_php(root: &Path, catalog_lines: &[String], constraint: Option<&str>) -> (Option<String>, bool) {
    let installed = Installed::load(root)
        .map(|installed| installed.lines("php"))
        .unwrap_or_default();
    let fits = |line: &String| constraint.is_none_or(|constraint| allows(constraint, line));
    if constraint.is_none() {
        if let Some(default) = Settings::load(root)
            .ok()
            .and_then(|settings| settings.default_php)
        {
            if installed.contains(&default) {
                return (Some(default), true);
            }
        }
    }
    if let Some(line) = installed.iter().find(|line| fits(line)) {
        return (Some(line.clone()), true);
    }
    (catalog_lines.iter().find(|line| fits(line)).cloned(), false)
}

fn packages(
    manifest: &Value,
    sections: &[&str],
    lock: &Value,
    lock_sections: &[&str],
    notable: &[(&str, &str)],
) -> Vec<Package> {
    let locked = |name: &str| {
        lock_sections.iter().find_map(|section| {
            lock[*section].as_array()?.iter().find_map(|package| {
                (package["name"] == name).then(|| package["version"].as_str().map(str::to_string))?
            })
        })
    };
    notable
        .iter()
        .filter_map(|(name, label)| {
            let requested = sections
                .iter()
                .find_map(|section| manifest[*section][*name].as_str())?;
            Some(Package {
                name: (*name).into(),
                label: (*label).into(),
                version: locked(name).or_else(|| Some(requested.to_string())),
            })
        })
        .collect()
}

/// Versions of npm packages from package-lock.json (`packages["node_modules/x"]`).
fn npm_locked(lock: &Value, name: &str) -> Option<String> {
    lock["packages"][format!("node_modules/{name}")]["version"]
        .as_str()
        .map(str::to_string)
}

/// Reads a project folder. `catalog_lines` are the PHP lines Werd can install, newest first.
pub fn inspect(root: &Path, folder: &Path, catalog_lines: &[String]) -> ProjectInfo {
    let composer = read_json(&folder.join("composer.json"));
    let composer_lock = read_json(&folder.join("composer.lock"));
    let package = read_json(&folder.join("package.json"));
    let package_lock = read_json(&folder.join("package-lock.json"));
    let php_constraint = composer["require"]["php"].as_str().map(str::to_string);
    let (suggested_php, suggested_php_installed) =
        suggest_php(root, catalog_lines, php_constraint.as_deref());

    let mut js_packages = packages(
        &package,
        &["dependencies", "devDependencies"],
        &Value::Null,
        &[],
        &NOTABLE_JS,
    );
    for js in &mut js_packages {
        if let Some(version) = npm_locked(&package_lock, &js.name) {
            js.version = Some(version);
        }
    }
    let node = [".nvmrc", ".node-version"]
        .iter()
        .find_map(|file| fs::read_to_string(folder.join(file)).ok())
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .or_else(|| package["engines"]["node"].as_str().map(str::to_string));

    ProjectInfo {
        path: folder.to_string_lossy().trim_start_matches(r"\\?\").to_string(),
        name: folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        laravel: crate::parks::is_laravel(folder),
        php_constraint,
        suggested_php,
        suggested_php_installed,
        php_packages: packages(
            &composer,
            &["require", "require-dev"],
            &composer_lock,
            &["packages", "packages-dev"],
            &NOTABLE_PHP,
        ),
        js_packages,
        node,
        werd_yml: folder.join("werd.yml").is_file(),
        env_file: folder.join(".env").is_file(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_constraints_match_php_lines() {
        let cases = [
            ("^8.2", "8.1", false),
            ("^8.2", "8.2", true),
            ("^8.2", "8.5", true),
            ("^8.2", "9.0", false),
            ("^7.4|^8.0", "7.4", true),
            ("^7.4 || ^8.0", "8.3", true),
            (">=8.1 <8.4", "8.3", true),
            (">=8.1 <8.4", "8.4", false),
            (">=8.1,<8.4", "8.0", false),
            ("~8.3.0", "8.3", true),
            ("~8.3.0", "8.4", false),
            ("~8.3", "8.5", true),
            ("8.4.*", "8.4", true),
            ("8.4.*", "8.5", false),
            ("8.4", "8.4", true),
            (">8.3", "8.3", true),
            ("*", "7.4", true),
        ];
        for (constraint, line, expected) in cases {
            assert_eq!(allows(constraint, line), expected, "{constraint} vs {line}");
        }
    }

    #[test]
    fn a_laravel_project_is_described_from_its_files() {
        let root = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        let project = folder.path().join("fleet-desk");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("artisan"), "").unwrap();
        fs::write(
            project.join("composer.json"),
            r#"{"require":{"php":"^8.2","laravel/framework":"^12.0","filament/filament":"^4.0"},"require-dev":{"pestphp/pest":"^4.0"}}"#,
        )
        .unwrap();
        fs::write(
            project.join("composer.lock"),
            r#"{"packages":[{"name":"laravel/framework","version":"v12.30.1"}],"packages-dev":[]}"#,
        )
        .unwrap();
        fs::write(
            project.join("package.json"),
            r#"{"devDependencies":{"vite":"^7.0.0","tailwindcss":"^4.1.0"}}"#,
        )
        .unwrap();
        fs::write(project.join(".nvmrc"), "22\n").unwrap();
        let mut installed = Installed::default();
        installed.set("php", "8.1", "8.1.34");
        installed.set("php", "8.4", "8.4.26");
        installed.save(root.path()).unwrap();

        let catalog = vec!["8.5".to_string(), "8.4".into(), "8.1".into()];
        let info = inspect(root.path(), &project, &catalog);
        assert!(info.laravel);
        assert_eq!(info.name, "fleet-desk");
        assert_eq!(info.php_constraint.as_deref(), Some("^8.2"));
        assert_eq!(info.suggested_php.as_deref(), Some("8.4"));
        assert!(info.suggested_php_installed);
        assert_eq!(
            info.php_packages[0],
            Package {
                name: "laravel/framework".into(),
                label: "Laravel".into(),
                version: Some("v12.30.1".into())
            }
        );
        assert_eq!(info.php_packages[1].version.as_deref(), Some("^4.0"));
        assert!(info.php_packages.iter().any(|package| package.label == "Pest"));
        assert_eq!(info.js_packages.len(), 2);
        assert_eq!(info.node.as_deref(), Some("22"));
    }

    #[test]
    fn a_missing_php_line_is_suggested_from_the_catalog() {
        let root = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        fs::write(
            project.path().join("composer.json"),
            r#"{"require":{"php":"^8.5"}}"#,
        )
        .unwrap();
        let catalog = vec!["8.5".to_string(), "8.4".into()];
        let info = inspect(root.path(), project.path(), &catalog);
        assert!(!info.laravel);
        assert_eq!(info.suggested_php.as_deref(), Some("8.5"));
        assert!(!info.suggested_php_installed);
    }
}
