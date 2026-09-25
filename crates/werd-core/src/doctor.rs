//! Environment checks shown in the Diagnostics page and by `werd doctor`.

use crate::model::DoctorResult;
use crate::paths::runtime_binary;
use std::net::TcpListener;
use std::path::Path;

const RUNTIMES: [(&str, &str); 6] = [
    ("php-cgi", "PHP 8.5"),
    ("caddy", "Caddy"),
    ("postgres", "PostgreSQL 18"),
    ("redis-server", "Redis 7.2"),
    ("mailpit", "Mailpit"),
    ("rustfs", "RustFS"),
];

pub fn run(root: &Path) -> Vec<DoctorResult> {
    let mut checks = vec![DoctorResult {
        label: "Werd daemon".into(),
        ok: true,
        detail: "The local daemon is responding".into(),
    }];

    for port in [80u16, 443] {
        let free = TcpListener::bind(("127.0.0.1", port)).is_ok();
        checks.push(DoctorResult {
            label: format!("Web port {port}"),
            ok: true,
            detail: if free {
                "Free; Werd still uses a dedicated HTTPS port per site".into()
            } else {
                "In use by another process; Werd uses a dedicated HTTPS port per site and leaves it alone"
                    .into()
            },
        });
    }

    for (binary, label) in RUNTIMES {
        let path = runtime_binary(root, binary);
        let ok = path.is_file();
        checks.push(DoctorResult {
            label: label.into(),
            ok,
            detail: format!(
                "{}: {}",
                if ok { "Installed" } else { "Not installed" },
                path.display()
            ),
        });
    }

    let postgres = root.join("runtimes/postgres/18");
    let library = if cfg!(windows) {
        "lib/vector.dll"
    } else {
        "lib/vector.so"
    };
    let vector =
        postgres.join("share/extension/vector.control").is_file() && postgres.join(library).is_file();
    checks.push(DoctorResult {
        label: "pgvector".into(),
        ok: vector,
        detail: if vector {
            "Extension available for PostgreSQL 18".into()
        } else {
            "Install pgvector after PostgreSQL 18".into()
        },
    });
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_runtimes_in_an_empty_home() {
        let root = tempfile::tempdir().unwrap();
        let checks = run(root.path());
        let php = checks.iter().find(|check| check.label == "PHP 8.5").unwrap();
        assert!(!php.ok);
        assert!(checks.iter().any(|check| check.label == "pgvector" && !check.ok));
        assert!(checks[0].ok, "the daemon check is always first and ok");
    }
}
