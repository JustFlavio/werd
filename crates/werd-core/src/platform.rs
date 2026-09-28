//! Operating-system integration: opening URLs, trusting the local CA and
//! updating the hosts file through the elevated helper.

use crate::process::hidden_command;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const REPOSITORY_URL: &str = "https://github.com/JustFlavio/werd";

/// URLs the apps may open: loopback services with an explicit port, `.test` sites
/// and the repository.
pub fn is_openable_url(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let (host, port) = match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        if crate::domains::is_valid(host) && port.is_none_or(|port| port.parse::<u16>().is_ok()) {
            return true;
        }
    }
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

/// Finds `werd-helper`: `WERD_HELPER_BIN`, next to the current executable, or the dev build.
pub fn helper_executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("WERD_HELPER_BIN") {
        return Ok(PathBuf::from(path));
    }
    let name = if cfg!(windows) {
        "werd-helper.exe"
    } else {
        "werd-helper"
    };
    let current = std::env::current_exe()?;
    let adjacent = current.parent().context("Invalid executable path")?.join(name);
    if adjacent.exists() {
        return Ok(adjacent);
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/debug")
        .join(name);
    if dev.exists() {
        return Ok(dev);
    }
    bail!("werd-helper not found next to {}", current.display())
}

/// Maps `domains` to 127.0.0.1 in the hosts file. Windows asks for administrator
/// approval (UAC), macOS for an administrator password; nothing changes if the
/// user declines.
pub fn sync_hosts(domains: &[String]) -> Result<()> {
    for domain in domains {
        if !crate::domains::is_valid(domain) {
            bail!("{domain} is not a .test domain");
        }
    }
    let helper = helper_executable()?;
    let mut arguments = vec!["hosts".to_string()];
    arguments.extend(domains.iter().cloned());
    let code = run_elevated(&helper, &arguments)?;
    if code != 0 {
        bail!("Updating the hosts file failed (exit code {code})");
    }
    Ok(())
}

/// Runs `program` as administrator and waits for it; returns its exit code.
#[cfg(windows)]
#[allow(unsafe_code)]
fn run_elevated(program: &Path, arguments: &[String]) -> Result<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    // Arguments are validated domain names, so plain spaces are enough as separators.
    let verb = wide("runas");
    let file = wide(&program.to_string_lossy());
    let parameters = wide(&arguments.join(" "));
    // SAFETY: every pointer refers to a NUL-terminated buffer that outlives the
    // call; `info` is zero-initialised with its size set as the API requires. The
    // process handle is used only after a successful call and closed afterwards.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 {
            if GetLastError() == ERROR_CANCELLED {
                bail!("Administrator approval was declined; the hosts file was not changed");
            }
            bail!(
                "Cannot start the Werd helper: {}",
                std::io::Error::last_os_error()
            );
        }
        if info.hProcess.is_null() {
            bail!("The Werd helper did not start");
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}

/// AppleScript that runs its arguments as one shell command, each argument quoted.
#[cfg(target_os = "macos")]
const ELEVATED_SCRIPT: &str = r#"on run argv
    set command to quoted form of (item 1 of argv)
    repeat with argument in rest of argv
        set command to command & " " & quoted form of (argument as text)
    end repeat
    do shell script command with prompt "Werd needs your password to add .test domains to /etc/hosts." with administrator privileges
end run"#;

/// Runs `program` as root after macOS asks for an administrator password;
/// returns its exit code.
#[cfg(target_os = "macos")]
fn run_elevated(program: &Path, arguments: &[String]) -> Result<u32> {
    let output = hidden_command("/usr/bin/osascript")
        .arg("-e")
        .arg(ELEVATED_SCRIPT)
        .arg(program)
        .args(arguments)
        .output()
        .context("Cannot ask for administrator approval")?;
    if output.status.success() {
        return Ok(0);
    }
    elevated_exit_code(&String::from_utf8_lossy(&output.stderr))
}

/// Reads `do shell script`'s error: -128 is a cancelled password prompt, any
/// other number is the command's exit code.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn elevated_exit_code(stderr: &str) -> Result<u32> {
    let number = stderr
        .trim()
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once('('))
        .and_then(|(_, number)| number.parse::<i64>().ok());
    match number {
        Some(-128) => bail!("Administrator approval was declined; the hosts file was not changed"),
        Some(code) if code > 0 => Ok(u32::try_from(code).unwrap_or(1)),
        _ => bail!("Cannot run the Werd helper as administrator: {}", stderr.trim()),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn run_elevated(_program: &Path, _arguments: &[String]) -> Result<u32> {
    bail!("Updating the hosts file is not available on this platform yet")
}

/// SHA-1 thumbprint of a PEM certificate, as Windows shows it (uppercase hex).
pub(crate) fn thumbprint(pem: &str) -> Option<String> {
    use base64::Engine;
    use sha1::{Digest, Sha1};
    let body: String = pem
        .lines()
        .skip_while(|line| !line.starts_with("-----BEGIN CERTIFICATE-----"))
        .skip(1)
        .take_while(|line| !line.starts_with("-----END CERTIFICATE-----"))
        .collect();
    if body.trim().is_empty() {
        return None;
    }
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .ok()?;
    Some(
        Sha1::digest(der)
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect(),
    )
}

/// Whether `certificate` is among the trusted roots of the current user or the
/// computer. Reads the certificate stores in the registry; nothing is changed.
pub fn is_certificate_trusted(certificate: &Path) -> bool {
    let Some(thumbprint) = std::fs::read_to_string(certificate)
        .ok()
        .as_deref()
        .and_then(thumbprint)
    else {
        return false;
    };
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        let key = format!(r"SOFTWARE\Microsoft\SystemCertificates\Root\Certificates\{thumbprint}");
        [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
            .into_iter()
            .any(|hive| RegKey::predef(hive).open_subkey(&key).is_ok())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = thumbprint;
        // Succeeds only when the keychain trust settings make the CA a trusted root.
        hidden_command("/usr/bin/security")
            .args(["verify-cert", "-p", "basic", "-q", "-c"])
            .arg(certificate)
            .output()
            .is_ok_and(|output| output.status.success())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = thumbprint;
        false
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
    } else if cfg!(target_os = "macos") {
        // macOS asks for the user's password before changing trust settings.
        let keychain = dirs::home_dir()
            .context("Home folder not found")?
            .join("Library/Keychains/login.keychain-db");
        let output = hidden_command("/usr/bin/security")
            .args(["add-trusted-cert", "-r", "trustRoot", "-k"])
            .arg(keychain)
            .arg(certificate)
            .output()
            .context("The security tool is not available")?;
        if !output.status.success() {
            bail!(
                "Trusting the certificate failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok("Werd's local CA was added to your login keychain as a trusted root".into())
    } else {
        bail!("Trusting the local CA is not available on this platform yet")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbprints_match_what_windows_shows() {
        // Any PEM body works: the thumbprint is the SHA-1 of the decoded bytes.
        let pem = "-----BEGIN CERTIFICATE-----\nAAECAwQF\n-----END CERTIFICATE-----\n";
        assert_eq!(
            thumbprint(pem).as_deref(),
            Some("868460D98D09D8BBB93D7B6CDD15CC7FBEC676B9")
        );
        assert!(thumbprint("not a certificate").is_none());
        assert!(!is_certificate_trusted(Path::new("missing.crt")));
    }

    #[test]
    fn elevated_errors_are_read_from_osascript() {
        let declined = "0:198: execution error: User canceled. (-128)\n";
        assert!(elevated_exit_code(declined)
            .unwrap_err()
            .to_string()
            .contains("declined"));
        assert_eq!(elevated_exit_code("0:198: execution error: (3)\n").unwrap(), 3);
        assert!(elevated_exit_code("something else").is_err());
    }

    #[test]
    fn only_loopback_urls_with_a_port_and_the_repository_are_openable() {
        for allowed in [
            "http://127.0.0.1:8025",
            "http://127.0.0.1:8025/view/latest",
            "https://localhost:52011",
            "http://localhost:9001?tab=1",
            REPOSITORY_URL,
            "https://my-shop.test",
            "https://my-shop.test:8443/login?next=1",
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
            "https://shop.test:x",
            "https://shop.test.evil.com",
            "http://shop.test",
        ] {
            assert!(!is_openable_url(denied), "{denied}");
        }
    }
}
