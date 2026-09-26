//! Operating-system integration: opening URLs and trusting the local CA.

use crate::process::hidden_command;
use anyhow::{bail, Context, Result};
use std::path::Path;

pub const REPOSITORY_URL: &str = "https://github.com/JustFlavio/werd";

/// URLs the apps may open: loopback services with an explicit port, and the repository.
pub fn is_openable_url(url: &str) -> bool {
    let loopback = ["http://127.0.0.1:", "http://localhost:", "https://localhost:"]
        .iter()
        .any(|prefix| {
            url.strip_prefix(prefix)
                .and_then(|rest| rest.split(['/', '?', '#']).next())
                .is_some_and(|port| port.parse::<u16>().is_ok())
        });
    loopback || url == REPOSITORY_URL
}

/// Opens an allowed URL in the default browser.
pub fn open_url(url: &str) -> Result<()> {
    if !is_openable_url(url) {
        bail!("URL not allowed: {url}");
    }
    let mut command = if cfg!(windows) {
        let mut command = hidden_command("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else if cfg!(target_os = "macos") {
        hidden_command("open")
    } else {
        hidden_command("xdg-open")
    };
    command.arg(url).spawn().context("Cannot open the browser")?;
    Ok(())
}

/// Tells running programs (Explorer, new terminals) that the user environment changed,
/// so a PATH update applies without signing out.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn broadcast_environment_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    let area: Vec<u16> = "Environment".encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `area` is a NUL-terminated UTF-16 string that outlives the call; the
    // result pointer may be null per the API contract. Hung windows are skipped.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            area.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            std::ptr::null_mut(),
        );
    }
}

/// Adds `certificate` to the current user's trusted roots.
pub fn trust_certificate(certificate: &Path) -> Result<String> {
    if cfg!(windows) {
        let output = hidden_command("certutil.exe")
            .args(["-user", "-addstore", "Root"])
            .arg(certificate)
            .output()
            .context("certutil is not available")?;
        if !output.status.success() {
            bail!(
                "Installing the certificate failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok("Werd's local CA was added to the current user's trusted roots".into())
    } else {
        bail!("Trusting the local CA is not available on this platform yet")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_urls_with_a_port_and_the_repository_are_openable() {
        for allowed in [
            "http://127.0.0.1:8025",
            "http://127.0.0.1:8025/view/latest",
            "https://localhost:52011",
            "http://localhost:9001?tab=1",
            REPOSITORY_URL,
        ] {
            assert!(is_openable_url(allowed), "{allowed}");
        }
        for denied in [
            "https://example.com",
            "http://127.0.0.1",
            "http://127.0.0.1:notaport",
            "http://127.0.0.1:8025@evil.com",
            "file:///C:/Windows/System32/calc.exe",
            "https://github.com/JustFlavio/werd/../other",
        ] {
            assert!(!is_openable_url(denied), "{denied}");
        }
    }
}
