//! `werd-helper hosts <domain>...`: replaces the Werd block of the hosts file.
//!
//! Werd starts it elevated (UAC) because the hosts file needs administrator
//! rights. It does nothing else: every domain must be a `.test` name and is
//! always mapped to 127.0.0.1, so a tampered call cannot redirect real sites.
//!
//! Exit codes: 0 done, 2 invalid arguments, 3 the hosts file could not be updated.
#![windows_subsystem = "windows"]

use std::process::ExitCode;
use werd_core::domains;

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some("hosts") {
        return ExitCode::from(2);
    }
    let wanted: Vec<String> = arguments.collect();
    if !wanted.iter().all(|domain| domains::is_valid(domain)) {
        return ExitCode::from(2);
    }
    match domains::apply_hosts(&wanted) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(3),
    }
}
