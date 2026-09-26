//! `werd-helper hosts <domain>...`: replaces the Werd block of the hosts file.
//! `werd-helper check`: exits 1 when the hosts file has a Werd block (no rights needed).
//!
//! Werd starts `hosts` elevated (UAC) because the hosts file needs administrator
//! rights. It does nothing else: every domain must be a `.test` name and is
//! always mapped to 127.0.0.1, so a tampered call cannot redirect real sites.
//! `hosts` without domains removes the block; the uninstaller uses it.
//!
//! Exit codes: 0 done, 1 block present (`check`), 2 invalid arguments,
//! 3 the hosts file could not be read or updated.
#![windows_subsystem = "windows"]

use std::process::ExitCode;
use werd_core::domains;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("hosts") => {
            let wanted: Vec<String> = arguments.collect();
            if !wanted.iter().all(|domain| domains::is_valid(domain)) {
                return ExitCode::from(2);
            }
            match domains::apply_hosts(&wanted) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::from(3),
            }
        }
        Some("check") => match std::fs::read_to_string(domains::hosts_path()) {
            Ok(contents) if domains::has_block(&contents) => ExitCode::from(1),
            Ok(_) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(3),
        },
        _ => ExitCode::from(2),
    }
}
