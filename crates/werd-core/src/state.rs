//! In-memory daemon state and its persistence in `state.json`.

use crate::instances::Instances;
use crate::model::{Project, ProjectStatus};
use crate::paths::state_file;
use crate::process::ManagedChild;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
pub(crate) struct State {
    pub projects: Vec<Project>,
    /// Running processes per project id.
    pub processes: HashMap<String, Vec<ManagedChild>>,
    /// Shared service instances (PostgreSQL, Redis, …).
    pub instances: Instances,
}

impl State {
    /// Loads linked projects. Nothing survives a daemon restart, so every
    /// project starts as stopped.
    pub fn load(root: &Path) -> Result<Self> {
        let path = state_file(root);
        let mut projects: Vec<Project> = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("Corrupted {}", path.display()))?
        } else {
            Vec::new()
        };
        for project in &mut projects {
            project.status = ProjectStatus::Stopped;
            project.url = None;
            crate::migrations::upgrade_project(project);
        }
        Ok(Self {
            projects,
            processes: HashMap::new(),
            instances: Instances::load(root)?,
        })
    }

    /// Writes `state.json` through a temporary file so a crash never leaves it half-written.
    pub fn save(&self, root: &Path) -> Result<()> {
        let path = state_file(root);
        let temporary = root.join("state.json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.projects)?)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn index(&self, id: &str) -> Result<usize> {
        self.projects
            .iter()
            .position(|project| project.id == id)
            .context("Project not found")
    }

    pub fn project(&self, id: &str) -> Result<&Project> {
        Ok(&self.projects[self.index(id)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Ports, ServiceName};

    fn sample() -> Project {
        Project {
            id: "p1".into(),
            name: "shop".into(),
            path: "/work/shop".into(),
            php: "8.5".into(),
            node: None,
            links: Default::default(),
            requirements: Vec::new(),
            services: vec![ServiceName::Postgres, ServiceName::Mailpit],
            status: ProjectStatus::Running,
            url: Some("https://localhost:1234".into()),
            error: None,
            ports: Some(Ports::from([("site".to_string(), 1234)])),
            versions: [("postgresql".to_string(), "17".to_string())].into(),
            extensions: vec![],
        }
    }

    #[test]
    fn save_and_load_round_trip_resets_runtime_fields() {
        let root = tempfile::tempdir().unwrap();
        let state = State {
            projects: vec![sample()],
            ..State::default()
        };
        state.save(root.path()).unwrap();

        let loaded = State::load(root.path()).unwrap();
        let project = &loaded.projects[0];
        assert_eq!(project.status, ProjectStatus::Stopped);
        assert_eq!(project.url, None);
        assert_eq!(
            project.ports,
            sample().ports,
            "ports persist so .env values stay valid"
        );
        assert_eq!(project.services, sample().services);
    }

    #[test]
    fn reads_state_written_by_version_0_1() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            state_file(root.path()),
            r#"[{"id":"a","name":"a","path":"/a","php":"8.5","services":["postgres","redis"],"status":"error","error":"boom"}]"#,
        )
        .unwrap();
        let loaded = State::load(root.path()).unwrap();
        assert_eq!(
            loaded.projects[0].services,
            [ServiceName::Postgres, ServiceName::Redis]
        );
        assert_eq!(loaded.projects[0].status, ProjectStatus::Stopped);
        // 0.1 always meant PostgreSQL 18 with pgvector and Redis 7.2.
        assert_eq!(loaded.projects[0].versions["postgresql"], "18");
        assert_eq!(loaded.projects[0].versions["redis"], "7.2");
        assert_eq!(loaded.projects[0].extensions, ["pgvector"]);
    }

    #[test]
    fn missing_state_file_means_no_projects() {
        let root = tempfile::tempdir().unwrap();
        assert!(State::load(root.path()).unwrap().projects.is_empty());
    }
}
