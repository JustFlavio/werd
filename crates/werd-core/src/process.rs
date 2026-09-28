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

/// Puts the daemon in a Windows job object that kills every process in it when the
/// daemon exits, even when it crashes or is killed. Children and their own children
/// (php-cgi, Caddy, PostgreSQL, …) join the job automatically, so no orphan keeps a
/// port busy after a crash.
///
/// The job handle is intentionally never closed: Windows closes it when the daemon
/// exits, and that is what triggers the cleanup.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn kill_children_on_exit() -> Result<()> {
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // SAFETY: CreateJobObjectW accepts null attributes and name. `limits` is a
    // zero-initialised plain struct passed with its exact size. GetCurrentProcess
    // returns a pseudo-handle that needs no closing.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            bail!(
                "Cannot create the process job: {}",
                std::io::Error::last_os_error()
            );
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            bail!(
                "Cannot configure the process job: {}",
                std::io::Error::last_os_error()
            );
        }
        if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            bail!("Cannot join the process job: {}", std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn kill_children_on_exit() -> Result<()> {
    Ok(())
}

// ---- Leftovers of a crashed daemon (macOS and Linux) ------------------------
//
// Without a job object, children outlive a daemon that crashes or is killed.
// Each one is recorded in `<home>/children.json` with its start time, and the
// next daemon stops the process groups still running. The start time guards
// against a PID the system has since given to an unrelated program.

/// `children.json` of the running daemon, once [`stop_leftover_children`] ran.
#[cfg(unix)]
static REGISTRY: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
#[cfg(unix)]
static REGISTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
struct Recorded {
    pid: u32,
    started: String,
}

/// When `pid` started, as `ps` prints it; `None` once it has exited.
#[cfg(unix)]
fn start_time(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !started.is_empty()).then_some(started)
}

#[cfg(unix)]
fn update_registry(change: impl FnOnce(&mut Vec<Recorded>)) {
    let Some(path) = REGISTRY.get() else { return };
    let _guard = REGISTRY_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut records: Vec<Recorded> = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    change(&mut records);
    let _ = fs::write(path, serde_json::to_vec(&records).unwrap_or_default());
}

/// Stops the process groups recorded in `path` that are still the same processes.
/// Returns how many were stopped.
#[cfg(unix)]
fn stop_recorded(path: &Path) -> usize {
    use rustix::process::{kill_process_group, Pid, Signal};
    let records: Vec<Recorded> = fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut stopped = 0;
    for record in records {
        if start_time(record.pid).as_deref() != Some(record.started.as_str()) {
            continue;
        }
        let Some(group) = i32::try_from(record.pid).ok().and_then(Pid::from_raw) else {
            continue;
        };
        let _ = kill_process_group(group, Signal::TERM);
        let deadline = std::time::Instant::now() + STOP_GRACE;
        while std::time::Instant::now() < deadline && start_time(record.pid).is_some() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = kill_process_group(group, Signal::KILL);
        stopped += 1;
    }
    stopped
}

/// Stops what a crashed daemon left running, then records this daemon's children.
pub(crate) fn stop_leftover_children(root: &Path) {
    #[cfg(unix)]
    {
        let path = root.join("children.json");
        let stopped = stop_recorded(&path);
        if stopped > 0 {
            eprintln!("Werd daemon: stopped {stopped} process(es) left by a previous daemon");
        }
        let _ = fs::write(&path, "[]");
        let _ = REGISTRY.set(path);
    }
    #[cfg(not(unix))]
    let _ = root;
}

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

    /// Stops the child. On macOS and Linux it runs in its own process group
    /// (see [`spawn_logged`]): the whole group gets SIGTERM, so servers such as
    /// php-fpm stop their workers, then SIGKILL for anything still running.
    /// Killing only php-fpm's master leaves a worker holding the site's port.
    pub fn kill(&mut self) {
        #[cfg(unix)]
        {
            use rustix::process::{kill_process_group, Pid, Signal};
            let group = Pid::from_child(&self.child);
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = kill_process_group(group, Signal::TERM);
                let deadline = std::time::Instant::now() + STOP_GRACE;
                while std::time::Instant::now() < deadline && self.child.try_wait().ok().flatten().is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            // Fails with ESRCH once every member has exited.
            let _ = kill_process_group(group, Signal::KILL);
            let pid = self.child.id();
            update_registry(|records| records.retain(|record| record.pid != pid));
        }
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

/// How long a stopping child may take before it is killed.
#[cfg(unix)]
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Starts `command` with stderr going to `<log_dir>/<name>.log`. On macOS and
/// Linux the child leads a new process group, so stopping it reaches its workers.
pub(crate) fn spawn_logged(log_dir: &Path, name: &str, mut command: Command) -> Result<ManagedChild> {
    fs::create_dir_all(log_dir)?;
    let log = File::create(log_dir.join(format!("{name}.log")))?;
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .with_context(|| format!("Cannot start {name}"))?;
    #[cfg(unix)]
    if REGISTRY.get().is_some() {
        if let Some(started) = start_time(child.id()) {
            update_registry(|records| {
                records.push(Recorded {
                    pid: child.id(),
                    started,
                })
            });
        }
    }
    Ok(ManagedChild {
        name: name.into(),
        child,
    })
}

/// Like [`spawn_logged`], then waits for `port` to accept connections.
pub(crate) fn spawn_ready(
    log_dir: &Path,
    name: &str,
    command: Command,
    port: u16,
    label: &str,
) -> Result<ManagedChild> {
    let mut process = spawn_logged(log_dir, name, command)?;
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
    if source == "caddy" {
        // One Caddy serves every site.
        return tail_file(&crate::router::log_file(root));
    }
    tail_file(&project_dir(root, id).join(format!("{source}.log")))
}

/// The last lines of a log file; empty when it does not exist yet.
pub(crate) fn tail_file(path: &Path) -> Result<Vec<String>> {
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
    #[cfg(unix)]
    #[test]
    fn stopping_a_child_also_stops_the_processes_it_started() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("grandchild.pid");
        // A parent that starts a long-running child and ignores SIGTERM for itself,
        // like a server master whose worker would otherwise be orphaned.
        let mut command = std::process::Command::new("/bin/sh");
        command.arg("-c").arg(format!(
            "sleep 300 & echo $! > '{}'; trap '' TERM; wait",
            pid_file.display()
        ));
        let mut child = super::spawn_logged(directory.path(), "test", command).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !pid_file.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let grandchild: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        child.kill();
        let alive = std::process::Command::new("kill")
            .args(["-0", &grandchild.to_string()])
            .status()
            .unwrap()
            .success();
        assert!(!alive, "the grandchild {grandchild} survived");
    }

    #[cfg(unix)]
    #[test]
    fn leftovers_of_a_crashed_daemon_are_stopped_but_reused_pids_are_not() {
        use super::{start_time, stop_recorded, Recorded};
        use std::os::unix::process::CommandExt;
        let directory = tempfile::tempdir().unwrap();
        let spawn = || {
            std::process::Command::new("/bin/sleep")
                .arg("300")
                .process_group(0)
                .spawn()
                .unwrap()
        };
        let (mut leftover, mut unrelated) = (spawn(), spawn());
        let records = vec![
            Recorded {
                pid: leftover.id(),
                started: start_time(leftover.id()).unwrap(),
            },
            // Same PID, different start time: another program now owns it.
            Recorded {
                pid: unrelated.id(),
                started: "Thu Jan  1 00:00:00 1970".into(),
            },
        ];
        let path = directory.path().join("children.json");
        std::fs::write(&path, serde_json::to_vec(&records).unwrap()).unwrap();

        assert_eq!(stop_recorded(&path), 1);
        // Stopped by now; the test process still has to reap it.
        assert!(
            leftover.try_wait().unwrap().is_some(),
            "the leftover is still running"
        );
        assert!(
            unrelated.try_wait().unwrap().is_none(),
            "an unrelated process was killed"
        );
        let _ = unrelated.kill();
        let _ = unrelated.wait();
    }

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
