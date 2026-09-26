//! `werd.yml`: optional, versionable per-project configuration.
//!
//! ```yaml
//! version: 2
//! php: "8.4"
//! node: 22
//! services:
//!   database: { product: postgresql, version: "18", extensions: [pgvector] }
//!   cache: redis            # product, any installed version
//!   queue: redis@8.2        # product@line
//!   mail: true              # the default product of the category (Mailpit)
//!   storage: rustfs
//! ```
//!
//! Version 1 files (Werd 0.1) are still read. Werd never rewrites the file.

use crate::model::{Requirement, CATEGORIES};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub const FILE_NAME: &str = "werd.yml";

/// What a project asks for.
#[derive(Debug, Default, PartialEq)]
pub struct Manifest {
    pub php: Option<String>,
    pub node: Option<String>,
    pub requirements: Vec<Requirement>,
}

/// Numbers and strings are both accepted for versions (`php: 8.4`, `node: 22`).
#[derive(Deserialize)]
#[serde(untagged)]
enum Version {
    Text(String),
    Number(serde_yaml::Number),
}

impl Version {
    fn into_string(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Number(number) => number.to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ServiceSpec {
    Enabled(bool),
    Short(String),
    Full {
        #[serde(default)]
        product: Option<String>,
        #[serde(default)]
        version: Option<Version>,
        #[serde(default)]
        extensions: Vec<String>,
    },
}

#[derive(Deserialize)]
struct V2 {
    #[serde(default)]
    php: Option<Version>,
    #[serde(default)]
    node: Option<Version>,
    #[serde(default)]
    services: BTreeMap<String, ServiceSpec>,
}

#[derive(Deserialize)]
struct V1 {
    php: Version,
    services: V1Services,
}

#[derive(Deserialize)]
struct V1Services {
    postgres: Option<V1Postgres>,
    redis: Option<Version>,
    mailpit: Option<bool>,
    rustfs: Option<bool>,
}

#[derive(Deserialize)]
struct V1Postgres {
    major: u32,
    #[serde(default)]
    extensions: Vec<String>,
}

/// Default product when a category is enabled without naming one.
fn default_product(category: &str) -> Option<&'static str> {
    match category {
        "cache" | "queue" => Some("redis"),
        "mail" => Some("mailpit"),
        "storage" => Some("rustfs"),
        "search" => Some("meilisearch"),
        _ => None,
    }
}

fn is_line(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    (1..=2).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn check_line(field: &str, value: &str) -> Result<()> {
    if !is_line(value) {
        bail!("werd.yml: {field} must be a version line such as \"8.4\" or \"22\" (got \"{value}\")");
    }
    Ok(())
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self> {
        let value: serde_yaml::Value = serde_yaml::from_str(text).context("Invalid werd.yml")?;
        let version = value
            .get("version")
            .and_then(serde_yaml::Value::as_u64)
            .unwrap_or(2);
        let manifest = match version {
            1 => Self::from_v1(serde_yaml::from_value(value).context("Invalid werd.yml (version 1)")?),
            2 => Self::from_v2(serde_yaml::from_value(value).context("Invalid werd.yml")?)?,
            other => bail!("Unsupported werd.yml version: {other}"),
        };
        if let Some(php) = &manifest.php {
            check_line("php", php)?;
        }
        if let Some(node) = &manifest.node {
            check_line("node", node)?;
        }
        for requirement in &manifest.requirements {
            if let Some(line) = &requirement.line {
                check_line(&format!("services.{}", requirement.category), line)?;
            }
        }
        Ok(manifest)
    }

    /// Reads `werd.yml` from a project folder; a missing file means "no preferences".
    pub fn load(project: &Path) -> Result<Self> {
        let path = project.join(FILE_NAME);
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::parse(&fs::read_to_string(&path)?)
    }

    fn from_v1(v1: V1) -> Self {
        let requirement =
            |category: &str, product: &str, line: Option<String>, extensions: Vec<String>| Requirement {
                category: category.into(),
                product: product.into(),
                line,
                extensions,
            };
        let services = v1.services;
        let mut requirements = Vec::new();
        if let Some(postgres) = services.postgres {
            requirements.push(requirement(
                "database",
                "postgresql",
                Some(postgres.major.to_string()),
                postgres.extensions,
            ));
        }
        if let Some(redis) = services.redis {
            requirements.push(requirement("cache", "redis", Some(redis.into_string()), vec![]));
        }
        if services.mailpit == Some(true) {
            requirements.push(requirement("mail", "mailpit", None, vec![]));
        }
        if services.rustfs == Some(true) {
            requirements.push(requirement("storage", "rustfs", None, vec![]));
        }
        Self {
            php: Some(v1.php.into_string()),
            node: None,
            requirements,
        }
    }

    fn from_v2(v2: V2) -> Result<Self> {
        let mut requirements = Vec::new();
        for (category, spec) in v2.services {
            if !CATEGORIES.contains(&category.as_str()) {
                bail!(
                    "werd.yml: unknown service category \"{category}\" (use {})",
                    CATEGORIES.join(", ")
                );
            }
            let (product, line, extensions) = match spec {
                ServiceSpec::Enabled(false) => continue,
                ServiceSpec::Enabled(true) => (None, None, Vec::new()),
                ServiceSpec::Short(text) => match text.split_once('@') {
                    Some((product, line)) => (Some(product.to_string()), Some(line.to_string()), Vec::new()),
                    None => (Some(text), None, Vec::new()),
                },
                ServiceSpec::Full {
                    product,
                    version,
                    extensions,
                } => (product, version.map(Version::into_string), extensions),
            };
            let product = product
                .or_else(|| default_product(&category).map(str::to_string))
                .with_context(|| format!("werd.yml: services.{category} needs a product, e.g. postgresql"))?;
            if let Some(unknown) = extensions
                .iter()
                .find(|name| !(product == "postgresql" && *name == "pgvector"))
            {
                bail!("werd.yml: unknown extension \"{unknown}\" for {product}");
            }
            requirements.push(Requirement {
                category,
                product,
                line,
                extensions,
            });
        }
        Ok(Self {
            php: v2.php.map(Version::into_string),
            node: v2.node.map(Version::into_string),
            requirements,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(category: &str, product: &str, line: Option<&str>, extensions: &[&str]) -> Requirement {
        Requirement {
            category: category.into(),
            product: product.into(),
            line: line.map(str::to_string),
            extensions: extensions.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    #[test]
    fn parses_version_2_in_every_form() {
        let manifest = Manifest::parse(
            "version: 2\nphp: 8.4\nnode: 22\nservices:\n  database: { product: postgresql, version: 18, extensions: [pgvector] }\n  cache: redis\n  queue: redis@8.2\n  mail: true\n  search: false\n",
        )
        .unwrap();
        assert_eq!(manifest.php.as_deref(), Some("8.4"));
        assert_eq!(manifest.node.as_deref(), Some("22"));
        assert_eq!(
            manifest.requirements,
            [
                requirement("cache", "redis", None, &[]),
                requirement("database", "postgresql", Some("18"), &["pgvector"]),
                requirement("mail", "mailpit", None, &[]),
                requirement("queue", "redis", Some("8.2"), &[]),
            ]
        );
    }

    #[test]
    fn version_1_files_are_translated() {
        let manifest = Manifest::parse(
            "version: 1\nphp: '8.5'\nservices:\n  postgres: { major: 18, extensions: [pgvector] }\n  redis: '7.2'\n  mailpit: true\n  rustfs: false\n",
        )
        .unwrap();
        assert_eq!(manifest.php.as_deref(), Some("8.5"));
        assert_eq!(
            manifest.requirements,
            [
                requirement("database", "postgresql", Some("18"), &["pgvector"]),
                requirement("cache", "redis", Some("7.2"), &[]),
                requirement("mail", "mailpit", None, &[]),
            ]
        );
    }

    #[test]
    fn everything_is_optional_and_missing_files_are_fine() {
        assert_eq!(
            Manifest::parse("php: '8.3'\n").unwrap().php.as_deref(),
            Some("8.3")
        );
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(Manifest::load(folder.path()).unwrap(), Manifest::default());
        assert!(
            !folder.path().join(FILE_NAME).exists(),
            "Werd never writes werd.yml"
        );
    }

    #[test]
    fn invalid_files_explain_the_problem() {
        let error = |text: &str| Manifest::parse(text).unwrap_err().to_string();
        assert!(error("version: 3\n").contains("Unsupported"));
        assert!(error("php: latest\n").contains("php must be a version line"));
        assert!(error("services:\n  database: true\n").contains("needs a product"));
        assert!(error("services:\n  cron: redis\n").contains("unknown service category"));
        assert!(
            error("services:\n  database: { product: mysql, extensions: [pgvector] }\n").contains("pgvector")
        );
    }
}
