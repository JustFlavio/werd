//! Long-running operations (downloads, builds) run as background jobs so the
//! daemon keeps answering while they progress. Clients poll `jobs.list`.

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_FINISHED: usize = 20;
/// Output lines kept per job.
pub const LOG_LINES: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    /// What the job acts on, e.g. `php` / `8.4`.
    pub product: String,
    pub line: String,
    /// `install`, `update`, …
    pub action: String,
    pub state: JobState,
    /// Bytes downloaded so far, and the total when the server sent it.
    pub downloaded: u64,
    pub total: Option<u64>,
    /// Current step, e.g. "Downloading", "Extracting".
    pub step: String,
    pub error: Option<String>,
    pub started_at: u64,
    /// Output of the commands the job runs (last lines only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
    /// Lines dropped from the start of `log`, so clients can print only new lines.
    #[serde(default)]
    pub log_dropped: u64,
    /// What the job produced, e.g. the id of a created site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

/// Lets a running job report progress.
#[derive(Clone)]
pub struct Progress {
    jobs: Jobs,
    id: String,
}

impl Progress {
    pub fn bytes(&self, downloaded: u64, total: Option<u64>) {
        self.jobs.update(&self.id, |job| {
            job.downloaded = downloaded;
            job.total = total;
        });
    }

    pub fn step(&self, step: &str) {
        self.jobs.update(&self.id, |job| job.step = step.into());
    }

    /// Appends one output line, keeping the last [`LOG_LINES`].
    pub fn log(&self, line: &str) {
        self.jobs.update(&self.id, |job| {
            job.log.push(line.trim_end().to_string());
            let excess = job.log.len().saturating_sub(LOG_LINES);
            job.log.drain(..excess);
            job.log_dropped += excess as u64;
        });
    }

    pub fn result(&self, value: &str) {
        self.jobs.update(&self.id, |job| job.result = Some(value.into()));
    }

    /// A progress sink that goes nowhere, for synchronous callers and tests.
    pub fn detached() -> Self {
        Self {
            jobs: Jobs::default(),
            id: String::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Jobs(Arc<Mutex<Vec<Job>>>);

impl Jobs {
    pub fn list(&self) -> Vec<Job> {
        self.0.lock().map(|jobs| jobs.clone()).unwrap_or_default()
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) {
        if let Ok(mut jobs) = self.0.lock() {
            if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
                change(job);
            }
        }
    }

    /// Runs `work` on a new thread. Only one job may run per product line at a time.
    pub fn start(
        &self,
        product: &str,
        line: &str,
        action: &str,
        work: impl FnOnce(&Progress) -> Result<()> + Send + 'static,
    ) -> Result<Job> {
        let job = {
            let mut jobs = self.0.lock().map_err(|_| anyhow!("Job table unavailable"))?;
            if jobs
                .iter()
                .any(|job| job.state == JobState::Running && job.product == product && job.line == line)
            {
                bail!("{product} {line} is already being installed or updated");
            }
            let finished: Vec<usize> = jobs
                .iter()
                .enumerate()
                .filter(|(_, job)| job.state != JobState::Running)
                .map(|(i, _)| i)
                .collect();
            if finished.len() >= KEEP_FINISHED {
                jobs.remove(finished[0]);
            }
            let job = Job {
                id: uuid::Uuid::new_v4().to_string(),
                product: product.into(),
                line: line.into(),
                action: action.into(),
                state: JobState::Running,
                downloaded: 0,
                total: None,
                step: "Starting".into(),
                error: None,
                started_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                log: Vec::new(),
                log_dropped: 0,
                result: None,
            };
            jobs.push(job.clone());
            job
        };
        let progress = Progress {
            jobs: self.clone(),
            id: job.id.clone(),
        };
        thread::spawn(move || {
            let result = work(&progress);
            progress.jobs.update(&progress.id, |job| match result {
                Ok(()) => {
                    job.state = JobState::Done;
                    job.step = "Done".into();
                }
                Err(error) => {
                    job.state = JobState::Failed;
                    job.error = Some(format!("{error:#}"));
                }
            });
        });
        Ok(job)
    }

    /// Waits until the job finishes; used by tests and synchronous callers.
    pub fn wait(&self, id: &str) -> Job {
        loop {
            if let Some(job) = self
                .list()
                .into_iter()
                .find(|job| job.id == id && job.state != JobState::Running)
            {
                return job;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_report_progress_and_outcome() {
        let jobs = Jobs::default();
        let done = jobs
            .start("php", "8.4", "install", |progress| {
                progress.step("Downloading");
                progress.bytes(50, Some(100));
                for index in 0..LOG_LINES + 5 {
                    progress.log(&format!("line {index}\n"));
                }
                progress.result("site-1");
                Ok(())
            })
            .unwrap();
        let finished = jobs.wait(&done.id);
        assert_eq!(finished.state, JobState::Done);
        assert_eq!((finished.downloaded, finished.total), (50, Some(100)));
        assert_eq!(finished.log.len(), LOG_LINES);
        assert_eq!(finished.log[0], "line 5");
        assert_eq!(finished.log_dropped, 5);
        assert_eq!(finished.result.as_deref(), Some("site-1"));

        let failed = jobs
            .start("node", "22", "install", |_| bail!("network down"))
            .unwrap();
        let finished = jobs.wait(&failed.id);
        assert_eq!(finished.state, JobState::Failed);
        assert_eq!(finished.error.as_deref(), Some("network down"));
    }

    #[test]
    fn one_job_per_line_at_a_time() {
        let jobs = Jobs::default();
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let first = jobs
            .start("php", "8.4", "install", move |_| {
                let _ = wait.recv();
                Ok(())
            })
            .unwrap();
        assert!(jobs.start("php", "8.4", "update", |_| Ok(())).is_err());
        assert!(jobs.start("php", "8.5", "install", |_| Ok(())).is_ok());
        release.send(()).unwrap();
        jobs.wait(&first.id);
        assert!(jobs.start("php", "8.4", "update", |_| Ok(())).is_ok());
    }
}
