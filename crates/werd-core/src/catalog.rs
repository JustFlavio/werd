//! Runtime catalog: what can be installed, per product, line and platform.
//!
//! The catalog is data, not code. `catalog/catalog.json` is generated from
//! official sources by `scripts/catalog/generate.mjs` (weekly in CI) and embedded
//! in the daemon. A newer copy can be cached in `<home>/catalog.json`.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

const EMBEDDED: &str = include_str!("../../../catalog/catalog.json");
const SCHEMA: u32 = 1;

/// Platform key used in catalog builds, e.g. `windows-x64`.
pub const PLATFORM: &str = if cfg!(all(windows, target_arch = "x86_64")) {
    "windows-x64"
} else if cfg!(all(windows, target_arch = "aarch64")) {
    "windows-arm64"
} else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
    "macos-arm64"
} else if cfg!(target_os = "macos") {
    "macos-x64"
} else if cfg!(target_arch = "aarch64") {
    "linux-arm64"
} else {
    "linux-x64"
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    #[serde(default)]
    pub generated: Option<String>,
    pub products: BTreeMap<String, Product>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Runtime,
    Service,
    Tool,
    Extension,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
    pub label: String,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_port: Option<u16>,
    /// For extensions: the product they plug into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    pub lines: BTreeMap<String, Line>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Line {
    pub latest: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lts: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eol: Option<String>,
    pub builds: BTreeMap<String, Build>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Zip,
    /// A gzip-compressed tarball (macOS and Linux releases).
    #[serde(rename = "tar.gz")]
    TarGz,
    /// A PHP archive saved under the marker name (e.g. `composer.phar`).
    Phar,
    /// A single executable saved under the marker name (e.g. `meilisearch.exe`).
    File,
    /// Formats this build of Werd cannot install yet.
    #[serde(other)]
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub url: String,
    pub sha256: String,
    pub format: Format,
    /// Path, relative to the install folder, whose presence means "installed".
    pub marker: String,
    /// The patch this build installs when it lags behind the line's `latest`
    /// (static macOS PHP builds follow php.net by a few weeks).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// More archives extracted into the same folder, e.g. `php-fpm` next to the
    /// PHP CLI on macOS, where they ship separately.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<Part>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Part {
    pub url: String,
    pub sha256: String,
    pub format: Format,
}

impl Catalog {
    pub fn parse(text: &str) -> Result<Self> {
        let catalog: Self = serde_json::from_str(text).context("Invalid runtime catalog")?;
        if catalog.schema != SCHEMA {
            bail!("Unsupported catalog schema {}", catalog.schema);
        }
        Ok(catalog)
    }

    pub fn embedded() -> Self {
        #[allow(clippy::expect_used)]
        Self::parse(EMBEDDED).expect("the embedded catalog is validated by tests")
    }

    /// The embedded catalog, or the cached remote copy when it is newer.
    pub fn load(root: &Path) -> Self {
        let embedded = Self::embedded();
        let cached = fs::read_to_string(root.join("catalog.json"))
            .ok()
            .and_then(|text| Self::parse(&text).ok());
        match cached {
            Some(cached) if cached.generated > embedded.generated => cached,
            _ => embedded,
        }
    }

    /// Downloads a catalog from `url` and caches it in `<home>/catalog.json`.
    pub fn refresh(root: &Path, url: &str) -> Result<Self> {
        let text = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?
            .get(url)
            .send()?
            .error_for_status()?
            .text()?;
        let catalog = Self::parse(&text)?;
        fs::write(root.join("catalog.json"), text)?;
        Ok(catalog)
    }

    pub fn product(&self, id: &str) -> Result<&Product> {
        self.products
            .get(id)
            .with_context(|| format!("Unknown product: {id}"))
    }

    /// The build of `product`/`line` for this platform.
    pub fn build(&self, product: &str, line: &str) -> Result<(&Line, &Build)> {
        let entry = self.product(product)?;
        let line_entry = entry
            .lines
            .get(line)
            .with_context(|| format!("Unknown {} version: {line}", entry.label))?;
        let build = line_entry
            .build_here()
            .with_context(|| format!("{} {line} is not available for {PLATFORM}", entry.label))?;
        Ok((line_entry, build))
    }
}

impl Line {
    /// The build for this platform, or the portable one.
    pub fn build_here(&self) -> Option<&Build> {
        self.builds.get(PLATFORM).or_else(|| self.builds.get("any"))
    }

    /// The newest patch installable on this platform.
    pub fn latest_here(&self) -> &str {
        self.build_here()
            .and_then(|build| build.version.as_deref())
            .unwrap_or(&self.latest)
    }

    /// Whether the installable patch is an alpha, beta or release candidate.
    pub fn is_prerelease(&self) -> bool {
        version_key(self.latest_here())
            .iter()
            .any(|(_, stage, _)| *stage < 3)
    }
}

impl Product {
    /// Lines available on this platform, newest first.
    pub fn available_lines(&self) -> Vec<(&String, &Line)> {
        let mut lines: Vec<_> = self
            .lines
            .iter()
            .filter(|(_, line)| line.build_here().is_some())
            .collect();
        lines.sort_by(|a, b| compare_versions(b.0, a.0));
        lines
    }
}

/// Numeric comparison of dotted versions: `8.10.2` > `8.9.9`, `18` > `9.6`.
/// Pre-releases sort before their release: `8.6.0alpha1` < `8.6.0RC2` < `8.6.0RC10` < `8.6.0`.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (left, right) = (version_key(a), version_key(b));
    for index in 0..left.len().max(right.len()) {
        let ordering = left
            .get(index)
            .unwrap_or(&RELEASE)
            .cmp(right.get(index).unwrap_or(&RELEASE));
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

/// A missing suffix: a final release, after every alpha, beta or RC.
const RELEASE: (u64, u8, u64) = (0, 3, 0);

/// Each dotted part as (number, stage, stage number): `0RC2` → (0, 2, 2).
fn version_key(value: &str) -> Vec<(u64, u8, u64)> {
    value
        .split(['.', '-'])
        .map(|part| {
            let digits = part.len() - part.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            let (number, suffix) = part.split_at(digits);
            let suffix = suffix.to_ascii_lowercase();
            let letters = suffix.trim_end_matches(|c: char| c.is_ascii_digit());
            let stage = match letters {
                "" => 3,
                "alpha" => 0,
                "beta" => 1,
                _ => 2,
            };
            (
                number.parse().unwrap_or(0),
                stage,
                suffix[letters.len()..].parse().unwrap_or(0),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_is_valid() {
        let catalog = Catalog::embedded();
        assert!(!catalog.products.is_empty());
        for (id, product) in &catalog.products {
            assert!(!product.lines.is_empty(), "{id} has no lines");
            for (line, entry) in &product.lines {
                assert!(!entry.latest.is_empty(), "{id} {line}");
                for (platform, build) in &entry.builds {
                    let parts = std::iter::once((&build.url, &build.sha256, build.format)).chain(
                        build
                            .extra
                            .iter()
                            .map(|part| (&part.url, &part.sha256, part.format)),
                    );
                    for (url, sha256, format) in parts {
                        assert!(url.starts_with("https://"), "{id} {line} {platform}");
                        assert_eq!(sha256.len(), 64, "{id} {line} {platform}");
                        assert!(
                            sha256.chars().all(|c| c.is_ascii_hexdigit()),
                            "{id} {line} {platform}"
                        );
                        assert_ne!(format, Format::Unsupported, "{id} {line} {platform}");
                    }
                    assert!(!build.marker.is_empty(), "{id} {line} {platform}");
                }
            }
            if let Some(parent) = &product.extends {
                assert!(
                    catalog.products.contains_key(parent),
                    "{id} extends unknown {parent}"
                );
            }
        }
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(compare_versions("8.10.2", "8.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("18", "9.6"), Ordering::Greater);
        assert_eq!(compare_versions("8.4.25", "8.4.26"), Ordering::Less);
        assert_eq!(compare_versions("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_versions("8.6.0RC2", "8.6.0"), Ordering::Less);
        assert_eq!(compare_versions("8.6.0RC10", "8.6.0RC2"), Ordering::Greater);
        assert_eq!(compare_versions("8.6.0beta3", "8.6.0RC1"), Ordering::Less);
        assert_eq!(compare_versions("8.6.0alpha1", "8.6.0beta1"), Ordering::Less);
        assert_eq!(compare_versions("8.6.0RC2", "8.5.11"), Ordering::Greater);
        assert_eq!(compare_versions("18.6-1", "18.6-2"), Ordering::Less);
    }

    #[test]
    fn builds_are_filtered_by_platform() {
        let catalog = Catalog::parse(&format!(
            r#"{{"schema":1,"products":{{"x":{{"label":"X","kind":"tool","lines":{{
                "1":{{"latest":"1.2","builds":{{"{PLATFORM}":{{"url":"https://a","sha256":"{zero}","format":"zip","marker":"x"}}}}}},
                "2":{{"latest":"2.0","builds":{{"other-os":{{"url":"https://b","sha256":"{zero}","format":"zip","marker":"x"}}}}}},
                "3":{{"latest":"3.1","builds":{{"any":{{"url":"https://c","sha256":"{zero}","format":"phar","marker":"x"}}}}}}
            }}}}}}}}"#,
            zero = "0".repeat(64)
        ))
        .unwrap();
        let product = catalog.product("x").unwrap();
        let lines: Vec<&str> = product
            .available_lines()
            .iter()
            .map(|(line, _)| line.as_str())
            .collect();
        assert_eq!(lines, ["3", "1"]);
        assert!(catalog
            .build("x", "2")
            .unwrap_err()
            .to_string()
            .contains("not available"));
        assert_eq!(catalog.build("x", "3").unwrap().1.format, Format::Phar);
    }

    #[test]
    fn unknown_formats_and_schemas_are_handled() {
        let zero = "0".repeat(64);
        let text = format!(
            r#"{{"schema":1,"products":{{"x":{{"label":"X","kind":"tool","lines":{{"1":{{"latest":"1","builds":{{"any":{{"url":"https://a","sha256":"{zero}","format":"dmg","marker":"x"}}}}}}}}}}}}}}"#
        );
        assert_eq!(
            Catalog::parse(&text).unwrap().build("x", "1").unwrap().1.format,
            Format::Unsupported
        );
        assert!(Catalog::parse(&text.replace("\"schema\":1", "\"schema\":9")).is_err());
    }

    #[test]
    fn newer_cached_catalog_wins() {
        let root = tempfile::tempdir().unwrap();
        let mut newer = Catalog::embedded();
        newer.generated = Some("9999-01-01T00:00:00Z".into());
        newer.products.clear();
        fs::write(
            root.path().join("catalog.json"),
            serde_json::to_string(&newer).unwrap(),
        )
        .unwrap();
        assert!(Catalog::load(root.path()).products.is_empty());

        newer.generated = Some("2000-01-01T00:00:00Z".into());
        fs::write(
            root.path().join("catalog.json"),
            serde_json::to_string(&newer).unwrap(),
        )
        .unwrap();
        assert!(!Catalog::load(root.path()).products.is_empty());
    }
}
