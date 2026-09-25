//! `werd.yml`: the versionable per-project configuration.

use crate::model::ServiceName;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const FILE_NAME: &str = "werd.yml";

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub php: String,
    pub services: ManifestServices,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ManifestServices {
    pub postgres: Option<PostgresManifest>,
    pub redis: Option<String>,
    pub mailpit: Option<bool>,
    pub rustfs: Option<bool>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PostgresManifest {
    pub major: u32,
    pub extensions: Vec<String>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            version: 1,
            php: "8.5".into(),
            services: ManifestServices {
                postgres: Some(PostgresManifest {
                    major: 18,
                    extensions: vec!["pgvector".into()],
                }),
                redis: Some("7.2".into()),
                mailpit: Some(true),
                rustfs: Some(true),
            },
        }
    }
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: Self = serde_yaml::from_str(text).context("Invalid werd.yml")?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Reads `werd.yml` from `project`, writing the default one first if it is missing.
    pub fn load_or_create(project: &Path) -> Result<Self> {
        let path = project.join(FILE_NAME);
        if path.exists() {
            return Self::parse(&fs::read_to_string(&path)?);
        }
        let manifest = Self::default();
        fs::write(&path, serde_yaml::to_string(&manifest)?)
            .with_context(|| format!("Cannot write {}", path.display()))?;
        Ok(manifest)
    }

    /// Rejects configurations the current runtime catalog cannot serve.
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("Unsupported werd.yml version: {}", self.version);
        }
        if self.php != "8.5" {
            bail!(
                "Only PHP 8.5 is supported for now (werd.yml asks for {})",
                self.php
            );
        }
        if let Some(postgres) = &self.services.postgres {
            if postgres.major != 18 || postgres.extensions != ["pgvector"] {
                bail!("Only PostgreSQL 18 with pgvector is supported for now");
            }
        }
        if let Some(redis) = &self.services.redis {
            if redis != "7.2" {
                bail!("Only Redis 7.2 is supported for now (werd.yml asks for {redis})");
            }
        }
        Ok(())
    }

    pub fn services(&self) -> Vec<ServiceName> {
        let services = &self.services;
        [
            (services.postgres.is_some(), ServiceName::Postgres),
            (services.redis.is_some(), ServiceName::Redis),
            (services.mailpit == Some(true), ServiceName::Mailpit),
            (services.rustfs == Some(true), ServiceName::Rustfs),
        ]
        .into_iter()
        .filter_map(|(enabled, name)| enabled.then_some(name))
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manifest_enables_the_first_release_stack() {
        let manifest = Manifest::default();
        manifest.validate().unwrap();
        assert_eq!(
            manifest.services(),
            [
                ServiceName::Postgres,
                ServiceName::Redis,
                ServiceName::Mailpit,
                ServiceName::Rustfs
            ]
        );
    }

    #[test]
    fn default_manifest_round_trips_through_yaml() {
        let text = serde_yaml::to_string(&Manifest::default()).unwrap();
        assert_eq!(Manifest::parse(&text).unwrap(), Manifest::default());
    }

    #[test]
    fn services_can_be_disabled() {
        let manifest = Manifest::parse(
            "version: 1\nphp: '8.5'\nservices:\n  postgres: null\n  redis: null\n  mailpit: false\n  rustfs: true\n",
        )
        .unwrap();
        assert_eq!(manifest.services(), [ServiceName::Rustfs]);
    }

    #[test]
    fn unsupported_versions_are_rejected() {
        let manifest = Manifest {
            php: "8.3".into(),
            ..Manifest::default()
        };
        assert!(manifest.validate().unwrap_err().to_string().contains("PHP 8.5"));

        let manifest = Manifest {
            version: 2,
            ..Manifest::default()
        };
        assert!(manifest.validate().is_err());

        let mut manifest = Manifest::default();
        manifest.services.redis = Some("6".into());
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn load_or_create_writes_the_default_file_once() {
        let directory = tempfile::tempdir().unwrap();
        let created = Manifest::load_or_create(directory.path()).unwrap();
        assert!(directory.path().join(FILE_NAME).is_file());
        assert_eq!(Manifest::load_or_create(directory.path()).unwrap(), created);
    }
}
