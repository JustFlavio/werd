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

    /// Checks the shape of the file. Whether a version is installed is checked when the site starts.
    pub fn validate(&self) -> Result<()> {
        let is_line = |value: &str| {
            let parts: Vec<&str> = value.split('.').collect();
            (1..=2).contains(&parts.len())
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        };
        if self.version != 1 {
            bail!("Unsupported werd.yml version: {}", self.version);
        }
        if !is_line(&self.php) {
            bail!(
                "werd.yml: php must be a version line such as \"8.4\" (got \"{}\")",
                self.php
            );
        }
        if let Some(postgres) = &self.services.postgres {
            if let Some(unknown) = postgres
                .extensions
                .iter()
                .find(|name| name.as_str() != "pgvector")
            {
                bail!("werd.yml: unknown PostgreSQL extension \"{unknown}\" (supported: pgvector)");
            }
        }
        if let Some(redis) = &self.services.redis {
            if !is_line(redis) {
                bail!("werd.yml: redis must be a version line such as \"7.2\" (got \"{redis}\")");
            }
        }
        Ok(())
    }

    /// Requested line per catalog product.
    pub fn versions(&self) -> std::collections::BTreeMap<String, String> {
        let mut versions = std::collections::BTreeMap::new();
        if let Some(postgres) = &self.services.postgres {
            versions.insert("postgresql".into(), postgres.major.to_string());
        }
        if let Some(redis) = &self.services.redis {
            versions.insert("redis".into(), redis.clone());
        }
        versions
    }

    pub fn extensions(&self) -> Vec<String> {
        self.services
            .postgres
            .as_ref()
            .map(|postgres| postgres.extensions.clone())
            .unwrap_or_default()
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
    fn any_version_line_is_accepted_but_malformed_values_are_not() {
        let manifest = Manifest::parse(
            "version: 1\nphp: '8.3'\nservices:\n  postgres: { major: 16, extensions: [] }\n  redis: '8.2'\n  mailpit: false\n  rustfs: false\n",
        )
        .unwrap();
        assert_eq!(manifest.php, "8.3");
        assert_eq!(manifest.versions()["postgresql"], "16");
        assert_eq!(manifest.versions()["redis"], "8.2");
        assert!(manifest.extensions().is_empty(), "pgvector is optional");

        assert!(Manifest {
            php: "latest".into(),
            ..Manifest::default()
        }
        .validate()
        .is_err());
        assert!(Manifest {
            version: 2,
            ..Manifest::default()
        }
        .validate()
        .is_err());
        let mut manifest = Manifest::default();
        manifest.services.redis = Some("7.x".into());
        assert!(manifest.validate().is_err());
        let mut manifest = Manifest::default();
        manifest.services.postgres.as_mut().unwrap().extensions = vec!["postgis".into()];
        assert!(manifest.validate().unwrap_err().to_string().contains("postgis"));
    }

    #[test]
    fn load_or_create_writes_the_default_file_once() {
        let directory = tempfile::tempdir().unwrap();
        let created = Manifest::load_or_create(directory.path()).unwrap();
        assert!(directory.path().join(FILE_NAME).is_file());
        assert_eq!(Manifest::load_or_create(directory.path()).unwrap(), created);
    }
}
