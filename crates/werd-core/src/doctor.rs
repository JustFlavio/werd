//! Environment checks for `werd doctor`. They cover what the user actually
//! installed and configured, not a fixed stack.

use crate::catalog::Catalog;
use crate::model::DoctorResult;
use crate::runtimes::{line_dir, Installed};
use crate::settings::Settings;
use std::path::Path;

fn check(label: impl Into<String>, ok: bool, detail: impl Into<String>) -> DoctorResult {
    DoctorResult {
        label: label.into(),
        ok,
        detail: detail.into(),
    }
}

pub fn run(root: &Path, catalog: &Catalog) -> Vec<DoctorResult> {
    let mut checks = vec![check(
        "Werd daemon",
        true,
        format!("Running, data folder {}", root.display()),
    )];

    checks.push(check(
        "Runtime catalog",
        true,
        format!(
            "{} ({})",
            catalog.generated.as_deref().unwrap_or("unknown date"),
            crate::catalog::PLATFORM
        ),
    ));

    match Installed::load(root) {
        Ok(installed) => {
            if installed.0.is_empty() {
                checks.push(check("Runtimes", true, "Nothing installed yet"));
            }
            for (product, lines) in &installed.0 {
                for (line, entry) in lines {
                    let label = catalog
                        .products
                        .get(product)
                        .map_or(product.as_str(), |p| p.label.as_str());
                    let marker = catalog
                        .products
                        .get(product)
                        .and_then(|p| p.lines.get(line))
                        .and_then(|l| l.builds.values().next())
                        .map(|build| build.marker.clone());
                    let present =
                        marker.is_none_or(|marker| line_dir(root, product, line).join(marker).is_file());
                    checks.push(check(
                        format!("{label} {line}"),
                        present,
                        if present {
                            format!("{} installed", entry.version)
                        } else {
                            format!("{} recorded but files are missing; reinstall it", entry.version)
                        },
                    ));
                }
            }
        }
        Err(error) => checks.push(check("Runtimes", false, format!("{error:#}"))),
    }

    if let Ok(settings) = Settings::load(root) {
        checks.push(check(
            "Command line tools",
            true,
            if settings.path_enabled {
                "php, composer and node shims are on your PATH"
            } else {
                "Not on PATH; enable them from General"
            },
        ));
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_installed_lines_and_missing_files() {
        let root = tempfile::tempdir().unwrap();
        let mut installed = Installed::default();
        installed.set("php", "8.5", "8.5.11");
        installed.save(root.path()).unwrap();
        let checks = run(root.path(), &Catalog::embedded());
        let php = checks.iter().find(|check| check.label == "PHP 8.5").unwrap();
        assert!(!php.ok, "php-cgi.exe is missing on disk");
        assert!(checks[0].ok, "the daemon check is always first and ok");
    }
}
