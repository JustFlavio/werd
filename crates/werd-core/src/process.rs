//! Child processes and their log files.

use crate::paths::project_dir;
use crate::ports::wait_until_listening;
use anyhow::{bail, Context, Result};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// Log files a project can have. `werd` holds Werd's own events.
pub const LOG_SOURCES: [&str; 7] = ["werd", "postgres", "redis", "mailpit", "rustfs", "php", "caddy"];

const LOG_TAIL_BYTES: u64 = 256 * 1024;
const LOG_TAIL_LINES: usize = 100;

/// A `Command` that never opens a console window on Windows.
pub(crate) fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Stops this process's standard handles from leaking into children that outlive it.
///
/// Windows children inherit every inheritable handle of the parent. When the CLI
/// starts the daemon while its output is piped (`werd list | findstr x`), the daemon
/// would keep the pipe open and the shell would wait for the daemon to exit.
/// Rust duplicates handles it passes explicitly, so `Stdio::inherit` keeps working.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn stop_inheriting_std_handles() {
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };
    for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle has no preconditions. SetHandleInformation only clears
        // the inherit flag of a handle owned by this process, and is skipped for
        // null or invalid handles (no console, redirected to NUL).
        unsafe {
            let handle = GetStdHandle(id);
            if !handle.is_null() && handle != INVALID_HANDLE_VALUE {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn stop_inheriting_std_handles() {}

/// A supervised child, named after its log file (`postgres`, `php`, ...).
pub(crate) struct ManagedChild {
    pub name: String,
    pub child: Child,
}

impl ManagedChild {
    pub fn has_exited(&mut self) -> Option<String> {
        self.child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.to_string())
    }

    pub fn kill(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// Starts `command` with stderr going to `projects/<id>/<name>.log`.
pub(crate) fn spawn_logged(root: &Path, id: &str, name: &str, mut command: Command) -> Result<ManagedChild> {
    let directory = project_dir(root, id);
    fs::create_dir_all(&directory)?;
    let log = File::create(directory.join(format!("{name}.log")))?;
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .with_context(|| format!("Cannot start {name}"))?;
    Ok(ManagedChild {
        name: name.into(),
        child,
    })
}

/// Like [`spawn_logged`], then waits for `port` to accept connections.
pub(crate) fn spawn_ready(
    root: &Path,
    id: &str,
    name: &str,
    command: Command,
    port: u16,
    label: &str,
) -> Result<ManagedChild> {
    let mut process = spawn_logged(root, id, name, command)?;
    if let Err(error) = wait_until_listening(port, &mut process.child, label) {
        process.kill();
        return Err(error);
    }
    Ok(process)
}

/// Appends one line to the project's `werd.log`.
pub(crate) fn append_log(root: &Path, id: &str, message: &str) -> Result<()> {
    let directory = project_dir(root, id);
    fs::create_dir_all(&directory)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("werd.log"))?;
    writeln!(file, "{message}")?;
    Ok(())
}

/// The last lines of a project log. Non-UTF-8 output (common for Windows
/// console tools) is decoded as Windows-1252.
pub fn read_log(root: &Path, id: &str, source: &str) -> Result<Vec<String>> {
    if !LOG_SOURCES.contains(&source) {
        bail!("Unknown log: {source}");
    }
    let path = project_dir(root, id).join(format!("{source}.log"));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    let truncated = size > LOG_TAIL_BYTES;
    if truncated {
        file.seek(SeekFrom::Start(size - LOG_TAIL_BYTES))?;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if truncated {
        // Drop the partial first line.
        if let Some(end) = bytes.iter().position(|byte| *byte == b'\n') {
            bytes.drain(..=end);
        }
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(error) if cfg!(windows) => encoding_rs::WINDOWS_1252.decode(error.as_bytes()).0.into_owned(),
        Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(LOG_TAIL_LINES);
    Ok(lines[start..].iter().map(|line| (*line).to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_log_returns_the_last_lines() {
        let root = tempfile::tempdir().unwrap();
        for index in 0..150 {
            append_log(root.path(), "p1", &format!("line {index}")).unwrap();
        }
        let lines = read_log(root.path(), "p1", "werd").unwrap();
        assert_eq!(lines.len(), LOG_TAIL_LINES);
        assert_eq!(lines.first().unwrap(), "line 50");
        assert_eq!(lines.last().unwrap(), "line 149");
    }

    #[test]
    fn read_log_handles_missing_and_unknown_logs() {
        let root = tempfile::tempdir().unwrap();
        assert!(read_log(root.path(), "p1", "php").unwrap().is_empty());
        assert!(read_log(root.path(), "p1", "../secrets").is_err());
    }
}
