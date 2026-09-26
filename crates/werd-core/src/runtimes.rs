//! Runtime manager: installs, updates and removes catalog products per line.
//!
//! Layout: `<home>/runtimes/<product>/<line>/`, with the installed patch of each
//! line recorded in `<home>/runtimes/installed.json`. Downloads are verified with
//! SHA-256 before extraction. Provenance is documented in `docs/runtime-sources.md`.

use crate::catalog::{compare_versions, Build, Catalog, Format, Kind};
use crate::jobs::Progress;
use crate::process::hidden_command;
use crate::settings::Settings;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::ZipArchive;

const MAX_ARCHIVE_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(900);

/// PHP extensions enabled when their DLL ships with the build.
const PHP_EXTENSIONS: &[&str] = &[
    "bz2",
    "curl",
    "exif",
    "fileinfo",
    "gd",
    "gettext",
    "intl",
    "mbstring",
    "mysqli",
    "openssl",
    "pdo_mysql",
    "pdo_pgsql",
    "pdo_sqlite",
    "pgsql",
    "sodium",
    "sqlite3",
    "zip",
];

// ---- Installed state -------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InstalledLine {
    pub version: String,
}

/// `installed.json`: product → line → installed patch.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Installed(pub BTreeMap<String, BTreeMap<String, InstalledLine>>);

impl Installed {
    fn path(root: &Path) -> PathBuf {
        root.join("runtimes").join("installed.json")
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = Self::path(root);
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(&path)?).with_context(|| format!("Corrupted {}", path.display()))
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = Self::path(root);
        fs::create_dir_all(path.parent().context("Invalid runtimes path")?)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn version(&self, product: &str, line: &str) -> Option<&str> {
        self.0.get(product)?.get(line).map(|entry| entry.version.as_str())
    }

    pub fn set(&mut self, product: &str, line: &str, version: &str) {
        self.0.entry(product.into()).or_default().insert(
            line.into(),
            InstalledLine {
                version: version.into(),
            },
        );
    }

    pub fn remove(&mut self, product: &str, line: &str) {
        if let Some(lines) = self.0.get_mut(product) {
            lines.remove(line);
            if lines.is_empty() {
                self.0.remove(product);
            }
        }
    }

    /// Installed lines of a product, newest first.
    pub fn lines(&self, product: &str) -> Vec<String> {
        let mut lines: Vec<String> = self
            .0
            .get(product)
            .map(|lines| lines.keys().cloned().collect())
            .unwrap_or_default();
        lines.sort_by(|a, b| compare_versions(b, a));
        lines
    }
}

// ---- Paths -----------------------------------------------------------------

pub fn line_dir(root: &Path, product: &str, line: &str) -> PathBuf {
    root.join("runtimes").join(product).join(line)
}

/// `name` with `.exe` on Windows.
pub fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    }
}

/// Picks `preferred` when installed, otherwise the newest installed line.
pub fn resolve_line(root: &Path, product: &str, preferred: Option<&str>, label: &str) -> Result<String> {
    let installed = Installed::load(root)?;
    if let Some(line) = preferred {
        if installed.version(product, line).is_some() {
            return Ok(line.into());
        }
        bail!("{label} {line} is not installed. Install it from Werd first.");
    }
    installed
        .lines(product)
        .into_iter()
        .next()
        .with_context(|| format!("{label} is not installed"))
}

// ---- Listing ---------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeLine {
    pub product: String,
    pub label: String,
    pub kind: Kind,
    pub line: String,
    /// Newest patch in the catalog for this platform, if the line is still listed.
    pub latest: Option<String>,
    pub installed: Option<String>,
    pub update_available: bool,
    pub is_default: bool,
    pub lts: bool,
    pub eol: Option<String>,
}

/// Every catalog line for this platform plus installed lines no longer listed.
pub fn list(root: &Path, catalog: &Catalog, settings: &Settings) -> Result<Vec<RuntimeLine>> {
    let installed = Installed::load(root)?;
    let mut rows = Vec::new();
    for (id, product) in &catalog.products {
        let default = match id.as_str() {
            "php" => settings.default_php.as_deref(),
            "node" => settings.default_node.as_deref(),
            _ => None,
        };
        let listed = product.available_lines();
        for (line, entry) in &listed {
            let current = installed.version(id, line).map(str::to_string);
            rows.push(RuntimeLine {
                product: id.clone(),
                label: product.label.clone(),
                kind: product.kind,
                line: (*line).clone(),
                latest: Some(entry.latest.clone()),
                update_available: current
                    .as_deref()
                    .is_some_and(|version| compare_versions(version, &entry.latest) == Ordering::Less),
                installed: current,
                is_default: default == Some(line.as_str()),
                lts: entry.lts,
                eol: entry.eol.clone(),
            });
        }
        for line in installed.lines(id) {
            if !listed.iter().any(|(listed_line, _)| **listed_line == line) {
                rows.push(RuntimeLine {
                    product: id.clone(),
                    label: product.label.clone(),
                    kind: product.kind,
                    installed: installed.version(id, &line).map(str::to_string),
                    is_default: default == Some(line.as_str()),
                    line,
                    latest: None,
                    update_available: false,
                    lts: false,
                    eol: None,
                });
            }
        }
    }
    Ok(rows)
}

// ---- Downloads -------------------------------------------------------------

/// Where archives come from. Tests substitute a local implementation.
pub trait Fetcher: Send + Sync {
    /// Streams `url` into `output`, reporting `(downloaded, total)` bytes.
    fn fetch(&self, url: &str, output: &mut dyn Write, progress: &dyn Fn(u64, Option<u64>)) -> Result<u64>;
}

pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, url: &str, output: &mut dyn Write, progress: &dyn Fn(u64, Option<u64>)) -> Result<u64> {
        let client = reqwest::blocking::Client::builder()
            .timeout(DOWNLOAD_TIMEOUT)
            .user_agent(concat!("werd/", env!("CARGO_PKG_VERSION")))
            .build()?;
        let mut response = client.get(url).send()?.error_for_status()?;
        let total = response.content_length();
        if total.is_some_and(|length| length > MAX_ARCHIVE_BYTES) {
            bail!("Download too large: {url}");
        }
        let mut buffer = vec![0u8; 256 * 1024];
        let mut downloaded = 0u64;
        loop {
            let count = response.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            downloaded += count as u64;
            if downloaded > MAX_ARCHIVE_BYTES {
                bail!("Download too large: {url}");
            }
            output.write_all(&buffer[..count])?;
            progress(downloaded, total);
        }
        Ok(downloaded)
    }
}

/// Downloads `build` once into `downloads/`, rejecting any checksum mismatch.
fn download(root: &Path, fetcher: &dyn Fetcher, build: &Build, progress: &Progress) -> Result<PathBuf> {
    let downloads = root.join("downloads");
    fs::create_dir_all(&downloads)?;
    let archive = downloads.join(&build.sha256);
    if archive.is_file() && verify_sha256(&archive, &build.sha256)? {
        return Ok(archive);
    }
    let partial = downloads.join(format!("{}.part", build.sha256));
    progress.step("Downloading");
    {
        let mut file = File::create(&partial)?;
        fetcher.fetch(&build.url, &mut file, &|done, total| progress.bytes(done, total))?;
        file.sync_all()?;
    }
    progress.step("Verifying");
    if !verify_sha256(&partial, &build.sha256)? {
        let _ = fs::remove_file(&partial);
        bail!("Checksum mismatch for {}: download rejected", build.url);
    }
    fs::rename(partial, &archive)?;
    Ok(archive)
}

// ---- Install / update / uninstall -------------------------------------------

/// Installs or updates `product`/`line` to the catalog's latest patch.
pub fn install(
    root: &Path,
    catalog: &Catalog,
    fetcher: &dyn Fetcher,
    product: &str,
    line: &str,
    progress: &Progress,
) -> Result<String> {
    let (entry, build) = catalog.build(product, line)?;
    if build.format == Format::Unsupported {
        bail!("This version of Werd cannot install {product} {line}; update Werd");
    }
    if product == "pgvector" && Installed::load(root)?.lines("postgresql").is_empty() {
        bail!("Install PostgreSQL before pgvector");
    }
    let archive = download(root, fetcher, build, progress)?;
    let destination = line_dir(root, product, line);
    progress.step("Installing");
    match build.format {
        Format::Zip => extract_into(&archive, &destination, &build.marker)?,
        Format::Phar | Format::File => place_file(&archive, &destination, &build.marker)?,
        Format::Unsupported => bail!("Unsupported archive format"),
    }

    let mut installed = Installed::load(root)?;
    installed.set(product, line, &entry.latest);
    installed.save(root)?;

    match product {
        "php" => {
            // PHP on Windows ships without CA certificates; HTTPS (Composer, Guzzle) needs them.
            if installed.lines("cacert").is_empty() && catalog.products.contains_key("cacert") {
                install(root, catalog, fetcher, "cacert", "mozilla", progress)?;
            }
            write_php_ini(root, line, &Settings::load(root)?)?;
        }
        "cacert" => write_all_php_ini(root, &Settings::load(root)?)?,
        "pgvector" => {
            progress.step("Building pgvector");
            for postgres in installed.lines("postgresql") {
                build_pgvector(&destination, &line_dir(root, "postgresql", &postgres))?;
            }
        }
        "postgresql" => {
            // Keep pgvector available on every PostgreSQL line once it is installed.
            if let Some(vector) = installed.lines("pgvector").first() {
                progress.step("Building pgvector");
                build_pgvector(&line_dir(root, "pgvector", vector), &destination)?;
            }
        }
        _ => {}
    }
    Ok(entry.latest.clone())
}

pub fn uninstall(root: &Path, product: &str, line: &str) -> Result<()> {
    let mut installed = Installed::load(root)?;
    if installed.version(product, line).is_none() {
        bail!("{product} {line} is not installed");
    }
    let directory = line_dir(root, product, line);
    if directory.exists() {
        fs::remove_dir_all(&directory).with_context(|| {
            format!(
                "Cannot remove {}; stop anything that uses it",
                directory.display()
            )
        })?;
    }
    installed.remove(product, line);
    installed.save(root)?;
    let mut settings = Settings::load(root)?;
    let default = match product {
        "php" => &mut settings.default_php,
        "node" => &mut settings.default_node,
        _ => return Ok(()),
    };
    if default.as_deref() == Some(line) {
        *default = None;
        settings.save(root)?;
    }
    Ok(())
}

/// Writes `php.ini` for one PHP line from the user settings.
pub fn write_php_ini(root: &Path, line: &str, settings: &Settings) -> Result<()> {
    let directory = line_dir(root, "php", line);
    let mut ini = String::from(
        "; Generated by Werd. Change limits from the PHP page; edits here are overwritten.\n[PHP]\n",
    );
    ini.push_str("extension_dir=ext\n");
    for extension in PHP_EXTENSIONS {
        let library = if cfg!(windows) {
            format!("php_{extension}.dll")
        } else {
            format!("{extension}.so")
        };
        if directory.join("ext").join(library).is_file() {
            ini.push_str(&format!("extension={extension}\n"));
        }
    }
    let memory = if settings.memory_limit_mb < 0 {
        "-1".into()
    } else {
        format!("{}M", settings.memory_limit_mb)
    };
    ini.push_str(&format!(
        "memory_limit={memory}\nupload_max_filesize={upload}M\npost_max_size={upload}M\ndate.timezone=UTC\n",
        upload = settings.upload_max_mb
    ));
    let bundle = Installed::load(root)?
        .lines("cacert")
        .first()
        .map(|cacert| line_dir(root, "cacert", cacert).join("cacert.pem"))
        .filter(|path| path.is_file());
    if let Some(bundle) = bundle {
        let bundle = bundle.to_string_lossy().replace('\\', "/");
        ini.push_str(&format!(
            "curl.cainfo=\"{bundle}\"\nopenssl.cafile=\"{bundle}\"\n"
        ));
    }
    fs::write(directory.join("php.ini"), ini)?;
    Ok(())
}

/// Rewrites `php.ini` of every installed PHP line, e.g. after a settings change.
pub fn write_all_php_ini(root: &Path, settings: &Settings) -> Result<()> {
    for line in Installed::load(root)?.lines("php") {
        write_php_ini(root, &line, settings)?;
    }
    Ok(())
}

// ---- pgvector ---------------------------------------------------------------

pub fn has_pgvector(postgres: &Path) -> bool {
    let library = if cfg!(windows) {
        "lib/vector.dll"
    } else {
        "lib/vector.so"
    };
    postgres.join(library).is_file() && postgres.join("share/extension/vector.control").is_file()
}

/// Builds pgvector with MSVC against a PostgreSQL install (Windows only for now).
fn build_pgvector(source: &Path, postgres: &Path) -> Result<()> {
    if has_pgvector(postgres) {
        return Ok(());
    }
    if !cfg!(windows) {
        bail!("Building pgvector is only supported on Windows for now");
    }
    let vswhere = Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    if !vswhere.is_file() {
        bail!("Visual Studio 2022 Build Tools with MSVC are required to build pgvector");
    }
    let output = hidden_command(vswhere)
        .args(["-latest", "-products", "*", "-property", "installationPath"])
        .output()?;
    let location = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || location.is_empty() {
        bail!("Visual Studio Build Tools not found");
    }
    let devcmd = Path::new(&location).join("Common7/Tools/VsDevCmd.bat");
    if !devcmd.is_file() {
        bail!("VsDevCmd.bat not found in {location}");
    }
    // Build in a scratch copy so several PostgreSQL lines never share object files.
    let scratch = postgres.join(".pgvector-build");
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    copy_dir(source, &scratch)?;
    let script = scratch.join("werd-build.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\ncall \"{}\" -arch=x64\r\nif errorlevel 1 exit /b 1\r\nnmake /F Makefile.win\r\n\
             if errorlevel 1 exit /b 1\r\nnmake /F Makefile.win install\r\n",
            devcmd.display()
        ),
    )?;
    let output = hidden_command("cmd.exe")
        .arg("/c")
        .arg(&script)
        .current_dir(&scratch)
        .env("PGROOT", postgres)
        .output()
        .context("The pgvector build did not start")?;
    let _ = fs::remove_dir_all(&scratch);
    if !output.status.success() {
        bail!(
            "Building pgvector failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !has_pgvector(postgres) {
        bail!("The pgvector build is incomplete: vector.dll or vector.control is missing");
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

// ---- Archives ----------------------------------------------------------------

fn verify_sha256(path: &Path, expected: &str) -> Result<bool> {
    let mut reader = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()) == expected)
}

/// Copies a single-file download (e.g. a .phar) to `destination/name`.
fn place_file(download: &Path, destination: &Path, name: &str) -> Result<()> {
    let staging = staging_dir(destination)?;
    fs::copy(download, staging.join(name))?;
    swap_into(&staging, destination)
}

/// Extracts into a staging folder, then swaps it into place, so a failed or
/// interrupted install never leaves a half-written runtime behind.
fn extract_into(archive: &Path, destination: &Path, marker: &str) -> Result<()> {
    let staging = staging_dir(destination)?;
    let result = extract(archive, &staging).and_then(|()| {
        if staging.join(marker).is_file() {
            Ok(())
        } else {
            bail!("The archive is incomplete: {marker} is missing")
        }
    });
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    swap_into(&staging, destination)
}

fn staging_dir(destination: &Path) -> Result<PathBuf> {
    let parent = destination.parent().context("Invalid runtime destination")?;
    let name = destination
        .file_name()
        .context("Invalid runtime destination")?
        .to_string_lossy();
    let staging = parent.join(format!(".{name}.staging"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    Ok(staging)
}

fn swap_into(staging: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination).with_context(|| {
            format!(
                "Cannot replace {}; stop anything that uses it",
                destination.display()
            )
        })?;
    }
    fs::rename(staging, destination)?;
    Ok(())
}

/// Extracts a ZIP, dropping a single top-level folder when every entry shares it
/// (`node-v22-win-x64/`, `pgsql/`, …). Paths escaping `destination` are refused.
fn extract(path: &Path, destination: &Path) -> Result<()> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name() else {
            bail!("Unsafe path in ZIP archive: {}", entry.name())
        };
        names.push(enclosed.to_string_lossy().replace('\\', "/"));
    }
    // Directory entries come back without a trailing slash ("pgsql"), files with it ("pgsql/bin/…").
    let first = names
        .first()
        .and_then(|name| name.split('/').next())
        .unwrap_or_default()
        .to_string();
    let prefix = format!("{first}/");
    let single_root = names.len() > 1
        && names
            .iter()
            .all(|name| *name == first || name.starts_with(&prefix));
    let strip = if single_root { prefix.len() } else { 0 };

    let mut expanded: u64 = 0;
    for (index, name) in names.iter().enumerate() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = name.get(strip..).filter(|relative| !relative.is_empty()) else {
            continue;
        };
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARCHIVE_BYTES {
            bail!("ZIP archive too large once extracted");
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = File::create(output)?;
        std::io::copy(&mut entry, &mut file)?;
        file.flush()?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use zip::write::SimpleFileOptions;

    /// Serves URLs from local files.
    pub struct LocalFetcher(pub HashMap<String, PathBuf>);

    impl Fetcher for LocalFetcher {
        fn fetch(
            &self,
            url: &str,
            output: &mut dyn Write,
            progress: &dyn Fn(u64, Option<u64>),
        ) -> Result<u64> {
            let bytes = fs::read(self.0.get(url).with_context(|| format!("no fixture for {url}"))?)?;
            output.write_all(&bytes)?;
            progress(bytes.len() as u64, Some(bytes.len() as u64));
            Ok(bytes.len() as u64)
        }
    }

    pub fn zip_with(path: &Path, entries: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, content) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    fn sha(path: &Path) -> String {
        format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
    }

    /// A catalog with one "tool" product whose lines point at local zip fixtures.
    pub fn fixture(dir: &Path, versions: &[(&str, &str)]) -> (Catalog, LocalFetcher) {
        let mut fetcher = HashMap::new();
        let mut lines = serde_json::Map::new();
        for (line, version) in versions {
            let archive = dir.join(format!("tool-{version}.zip"));
            zip_with(
                &archive,
                &[
                    (&format!("tool-{version}/tool.exe"), version),
                    (&format!("tool-{version}/README"), "r"),
                ],
            );
            let url = format!("https://example.test/tool-{version}.zip");
            lines.insert(
                (*line).into(),
                serde_json::json!({ "latest": version, "builds": { crate::catalog::PLATFORM: {
                    "url": url, "sha256": sha(&archive), "format": "zip", "marker": "tool.exe" } } }),
            );
            fetcher.insert(url, archive);
        }
        let catalog = serde_json::json!({ "schema": 1, "generated": "2026-01-01",
            "products": { "tool": { "label": "Tool", "kind": "tool", "lines": lines } } });
        (
            Catalog::parse(&catalog.to_string()).unwrap(),
            LocalFetcher(fetcher),
        )
    }

    #[test]
    fn install_update_and_uninstall_a_line() {
        let root = tempfile::tempdir().unwrap();
        let (old, fetcher_old) = fixture(root.path(), &[("1", "1.0.0")]);
        install(
            root.path(),
            &old,
            &fetcher_old,
            "tool",
            "1",
            &Progress::detached(),
        )
        .unwrap();
        let binary = line_dir(root.path(), "tool", "1").join("tool.exe");
        assert_eq!(
            fs::read_to_string(&binary).unwrap(),
            "1.0.0",
            "the single top folder is stripped"
        );

        let (new, fetcher_new) = fixture(root.path(), &[("1", "1.2.0")]);
        let rows = list(root.path(), &new, &Settings::default()).unwrap();
        assert!(rows[0].update_available && rows[0].installed.as_deref() == Some("1.0.0"));

        install(
            root.path(),
            &new,
            &fetcher_new,
            "tool",
            "1",
            &Progress::detached(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&binary).unwrap(), "1.2.0");
        let rows = list(root.path(), &new, &Settings::default()).unwrap();
        assert!(!rows[0].update_available);

        uninstall(root.path(), "tool", "1").unwrap();
        assert!(!binary.exists());
        assert!(Installed::load(root.path()).unwrap().lines("tool").is_empty());
        assert!(uninstall(root.path(), "tool", "1").is_err());
    }

    #[test]
    fn checksum_mismatch_is_rejected_and_nothing_is_installed() {
        let root = tempfile::tempdir().unwrap();
        let (mut catalog, fetcher) = fixture(root.path(), &[("1", "1.0.0")]);
        let line = catalog
            .products
            .get_mut("tool")
            .unwrap()
            .lines
            .get_mut("1")
            .unwrap();
        line.builds
            .values_mut()
            .for_each(|build| build.sha256 = "0".repeat(64));
        let error = install(
            root.path(),
            &catalog,
            &fetcher,
            "tool",
            "1",
            &Progress::detached(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Checksum mismatch"));
        assert!(!line_dir(root.path(), "tool", "1").exists());
        assert!(Installed::load(root.path()).unwrap().0.is_empty());
    }

    #[test]
    fn installed_lines_missing_from_the_catalog_are_still_listed() {
        let root = tempfile::tempdir().unwrap();
        let (catalog, _) = fixture(root.path(), &[("2", "2.0.0")]);
        let mut installed = Installed::default();
        installed.set("tool", "1", "1.9.0");
        installed.save(root.path()).unwrap();
        let rows = list(root.path(), &catalog, &Settings::default()).unwrap();
        let old = rows.iter().find(|row| row.line == "1").unwrap();
        assert_eq!(
            (old.installed.as_deref(), old.latest.as_deref()),
            (Some("1.9.0"), None)
        );
    }

    #[test]
    fn resolve_line_prefers_the_requested_line() {
        let root = tempfile::tempdir().unwrap();
        let mut installed = Installed::default();
        installed.set("php", "8.4", "8.4.25");
        installed.set("php", "8.5", "8.5.11");
        installed.save(root.path()).unwrap();
        assert_eq!(
            resolve_line(root.path(), "php", Some("8.4"), "PHP").unwrap(),
            "8.4"
        );
        assert_eq!(resolve_line(root.path(), "php", None, "PHP").unwrap(), "8.5");
        assert!(resolve_line(root.path(), "php", Some("7.4"), "PHP")
            .unwrap_err()
            .to_string()
            .contains("7.4"));
    }

    #[test]
    fn php_ini_reflects_settings_and_available_extensions() {
        let root = tempfile::tempdir().unwrap();
        let ext = line_dir(root.path(), "php", "8.4").join("ext");
        fs::create_dir_all(&ext).unwrap();
        let library = if cfg!(windows) { "php_intl.dll" } else { "intl.so" };
        fs::write(ext.join(library), "").unwrap();
        let settings = Settings {
            upload_max_mb: 64,
            memory_limit_mb: -1,
            ..Settings::default()
        };
        write_php_ini(root.path(), "8.4", &settings).unwrap();
        let ini = fs::read_to_string(line_dir(root.path(), "php", "8.4").join("php.ini")).unwrap();
        assert!(ini.contains("extension=intl\n"));
        assert!(!ini.contains("extension=curl"), "missing DLLs are not enabled");
        assert!(ini.contains("memory_limit=-1"));
        assert!(ini.contains("upload_max_filesize=64M") && ini.contains("post_max_size=64M"));
    }

    #[test]
    fn extract_rejects_path_traversal() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("evil.zip");
        zip_with(&archive, &[("../evil.txt", "boom")]);
        assert!(extract(&archive, &directory.path().join("out")).is_err());
        assert!(!directory.path().join("evil.txt").exists());
    }

    #[test]
    fn extract_keeps_flat_archives_as_they_are() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("flat.zip");
        zip_with(&archive, &[("php-cgi.exe", "x"), ("ext/php_intl.dll", "y")]);
        let out = directory.path().join("out");
        extract(&archive, &out).unwrap();
        assert!(out.join("php-cgi.exe").is_file() && out.join("ext/php_intl.dll").is_file());
    }
}
