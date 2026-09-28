//! Environment checks for `werd doctor`. They cover what the user actually
//! installed and configured, not a fixed stack.

use crate::catalog::Catalog;
use crate::model::DoctorResult;
use crate::router::Router;
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

/// Whether `.test` domains work: enabled, served on the HTTPS port, in the hosts file.
fn domains(settings: &Settings, router: &Router, domains: &[String]) -> DoctorResult {
    if !settings.domains {
        return check(".test domains", true, "Off; sites use https://localhost:<port>");
    }
    if let Some(warning) = &router.warning {
        return check(".test domains", false, warning.clone());
    }
    let missing = crate::domains::missing_from_hosts(domains);
    if missing.is_empty() {
        check(
            ".test domains",
            true,
            format!("{} site domain(s) in the hosts file", domains.len()),
        )
    } else {
        check(
            ".test domains",
            false,
            format!(
                "Not in the hosts file: {}. Run `werd domains sync`",
                missing.join(", ")
            ),
        )
    }
}

pub fn run(root: &Path, catalog: &Catalog, router: &Router, site_domains: &[String]) -> Vec<DoctorResult> {
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
                        .build(product, line)
                        .ok()
                        .map(|(_, build)| build.marker.clone());
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
        checks.push(domains(&settings, router, site_domains));
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
        // A build for whatever platform the tests run on, so its marker is checked.
        let catalog = Catalog::parse(
            &serde_json::json!({ "schema": 1, "products": { "php": { "label": "PHP", "kind": "runtime",
                "lines": { "8.5": { "latest": "8.5.11", "builds": { crate::catalog::PLATFORM: {
                    "url": "https://example.test/php.zip", "sha256": "0".repeat(64),
                    "format": "zip", "marker": "php-binary"
                }}}}
            }}})
            .to_string(),
        )
        .unwrap();
        let checks = run(root.path(), &catalog, &Router::default(), &[]);
        let php = checks.iter().find(|check| check.label == "PHP 8.5").unwrap();
        assert!(!php.ok, "the PHP binary is missing on disk");
        assert!(checks[0].ok, "the daemon check is always first and ok");

        let binary = crate::runtimes::line_dir(root.path(), "php", "8.5");
        std::fs::create_dir_all(&binary).unwrap();
        std::fs::write(binary.join("php-binary"), "").unwrap();
        let checks = run(root.path(), &catalog, &Router::default(), &[]);
        assert!(checks.iter().find(|check| check.label == "PHP 8.5").unwrap().ok);
    }
}
