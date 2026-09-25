//! Runtime catalog: pinned downloads verified with SHA-256 and extracted under `runtimes/`.
//! Provenance and licenses are documented in `docs/runtime-sources.md`.

use crate::process::hidden_command;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;
use zip::ZipArchive;

const MAX_ARCHIVE_BYTES: u64 = 3 * 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Serialize)]
pub struct RuntimeInfo {
    pub id: &'static str,
    pub version: &'static str,
    pub installed: bool,
    pub note: &'static str,
}

struct Package {
    id: &'static str,
    version: &'static str,
    url: &'static str,
    sha256: &'static str,
    /// Install folder under `runtimes/`.
    destination: &'static str,
    /// Archive prefix removed while extracting.
    strip: &'static str,
    /// File whose presence means the runtime is installed.
    marker: &'static str,
    note: &'static str,
}

impl Package {
    fn info(&self, installed: bool) -> RuntimeInfo {
        RuntimeInfo {
            id: self.id,
            version: self.version,
            installed,
            note: self.note,
        }
    }
}

#[rustfmt::skip]
const WINDOWS_PACKAGES: &[Package] = &[
    Package { id: "php", version: "8.5.11", url: "https://windows.php.net/downloads/releases/php-8.5.11-nts-Win32-vs17-x64.zip", sha256: "0ea96e0d2b9b737a6036f05cf4e95c49313faa6d0f27bd97edb2742503f0c043", destination: "php/8.5", strip: "", marker: "php-cgi.exe", note: "Official PHP NTS build" },
    Package { id: "caddy", version: "2.11.4", url: "https://github.com/caddyserver/caddy/releases/download/v2.11.4/caddy_2.11.4_windows_amd64.zip", sha256: "1708333f79e274c7697285afe6d592ab39314e0b131e9ec6bea08ad27df62ebf", destination: "caddy/2.11.4", strip: "", marker: "caddy.exe", note: "Local HTTPS server" },
    Package { id: "postgres", version: "18.6", url: "https://get.enterprisedb.com/postgresql/postgresql-18.6-1-windows-x64-binaries.zip", sha256: "fbe23da234ee31547bf8a36d29dfd81e82b849df2d2b78d2eecb43d360252f8c", destination: "postgres/18", strip: "pgsql/", marker: "bin/postgres.exe", note: "EDB binaries; pgvector is built against them" },
    Package { id: "pgvector", version: "0.8.6", url: "https://codeload.github.com/pgvector/pgvector/zip/refs/tags/v0.8.6", sha256: "e93a1567219c9ce523ca16473f6c41cc80e01345b2d91ccdee40b473b7c5dd0a", destination: "sources/pgvector/0.8.6", strip: "pgvector-0.8.6/", marker: "Makefile.win", note: "Built on demand against PostgreSQL 18.6; needs Visual Studio Build Tools" },
    Package { id: "redis", version: "7.2.8", url: "https://github.com/redis-windows/redis-windows/releases/download/7.2.8/Redis-7.2.8-Windows-x64-msys2.zip", sha256: "aa6d4206a08d1189dd7a57c78201540a53b12350e1ad09ee7caca04120600656", destination: "redis/7.2", strip: "Redis-7.2.8-Windows-x64-msys2/", marker: "redis-server.exe", note: "Community Windows port" },
    Package { id: "mailpit", version: "1.31.2", url: "https://github.com/axllent/mailpit/releases/download/v1.31.2/mailpit-windows-amd64.zip", sha256: "42c20e5c3254125ea7489847811f10d70e39de573fe41d03a61412c87913e995", destination: "mailpit/1.31.2", strip: "", marker: "mailpit.exe", note: "Local SMTP and inbox" },
    Package { id: "rustfs", version: "1.0.0", url: "https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-windows-x86_64-v1.0.0.zip", sha256: "4ccf5858ce8e6f70f01af2394c8cc0e0878ee77faa6c20d3179153476554b7d8", destination: "rustfs/1.0.0", strip: "", marker: "rustfs.exe", note: "Local S3-compatible storage" },
];

/// The PHP extensions enabled in the generated `php.ini`.
const PHP_INI: &str = "[PHP]\nextension_dir=ext\nextension=curl\nextension=fileinfo\nextension=intl\n\
extension=mbstring\nextension=openssl\nextension=pdo_pgsql\nextension=pgsql\nextension=pdo_sqlite\n\
extension=sqlite3\nextension=zip\nextension=sodium\ndate.timezone=UTC\n";

/// Packages installable on this platform. Other platforms get their catalog in a later release.
fn catalog() -> &'static [Package] {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        WINDOWS_PACKAGES
    } else {
        &[]
    }
}

fn is_installed(root: &Path, package: &Package) -> bool {
    if package.id == "pgvector" {
        return vector_installed(root);
    }
    root.join("runtimes")
        .join(package.destination)
        .join(package.marker)
        .is_file()
}

pub fn list(root: &Path) -> Vec<RuntimeInfo> {
    catalog()
        .iter()
        .map(|package| package.info(is_installed(root, package)))
        .collect()
}

pub fn install(root: &Path, id: &str) -> Result<RuntimeInfo> {
    if catalog().is_empty() {
        bail!("The runtime catalog for this platform is not available yet");
    }
    let package = catalog()
        .iter()
        .find(|package| package.id == id)
        .context("Unknown runtime")?;
    if is_installed(root, package) {
        return Ok(package.info(true));
    }
    let runtime_root = root.join("runtimes");
    if id == "pgvector" && !runtime_root.join("postgres/18/bin/postgres.exe").is_file() {
        bail!("Install PostgreSQL 18 before pgvector");
    }
    let destination = runtime_root.join(package.destination);
    if !destination.join(package.marker).is_file() {
        let archive = download(root, package)?;
        extract_into(&archive, &destination, package)?;
    }
    match id {
        "pgvector" => build_pgvector(&destination, &runtime_root.join("postgres/18"))?,
        "php" => fs::write(destination.join("php.ini"), PHP_INI)?,
        _ => {}
    }
    Ok(package.info(true))
}

/// Downloads the archive once into `downloads/`, rejecting any checksum mismatch.
fn download(root: &Path, package: &Package) -> Result<std::path::PathBuf> {
    let downloads = root.join("downloads");
    fs::create_dir_all(&downloads)?;
    let archive = downloads.join(format!("{}-{}.zip", package.id, package.version));
    if archive.is_file() && verify_sha256(&archive, package.sha256)? {
        return Ok(archive);
    }
    let partial = downloads.join(format!("{}-{}.part", package.id, package.version));
    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()?;
    let response = client.get(package.url).send()?.error_for_status()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        bail!("The {} archive is too large", package.id);
    }
    let mut file = File::create(&partial)?;
    let downloaded = std::io::copy(&mut response.take(MAX_ARCHIVE_BYTES + 1), &mut file)?;
    if downloaded > MAX_ARCHIVE_BYTES {
        bail!("The {} archive is too large", package.id);
    }
    file.sync_all()?;
    if !verify_sha256(&partial, package.sha256)? {
        let _ = fs::remove_file(&partial);
        bail!("Checksum mismatch for {}: download rejected", package.id);
    }
    if archive.exists() {
        fs::remove_file(&archive)?;
    }
    fs::rename(partial, &archive)?;
    Ok(archive)
}

/// Extracts into a staging folder first, so a failed extraction never leaves a half-installed runtime.
fn extract_into(archive: &Path, destination: &Path, package: &Package) -> Result<()> {
    let parent = destination.parent().context("Invalid runtime destination")?;
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".{}-{}-extract", package.id, package.version));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let result = extract(archive, &staging, package.strip).and_then(|()| {
        if staging.join(package.marker).is_file() {
            Ok(())
        } else {
            bail!(
                "The {} archive is incomplete: {} is missing",
                package.id,
                package.marker
            )
        }
    });
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    fs::rename(&staging, destination)?;
    Ok(())
}

fn vector_installed(root: &Path) -> bool {
    has_pgvector(&root.join("runtimes/postgres/18"))
}

fn has_pgvector(postgres: &Path) -> bool {
    postgres.join("lib/vector.dll").is_file() && postgres.join("share/extension/vector.control").is_file()
}

/// Builds pgvector with MSVC against the downloaded PostgreSQL (Windows only).
fn build_pgvector(source: &Path, postgres: &Path) -> Result<()> {
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
    let script = source.join("werd-build.cmd");
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
        .current_dir(source)
        .env("PGROOT", postgres)
        .output()
        .context("The pgvector build did not start")?;
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

/// Extracts the entries under `strip`, refusing paths that escape `destination`.
fn extract(path: &Path, destination: &Path, strip: &str) -> Result<()> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let mut expanded: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name() else {
            bail!("Unsafe path in ZIP archive: {}", entry.name())
        };
        let name = enclosed.to_string_lossy().replace('\\', "/");
        let Some(relative) = name.strip_prefix(strip) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
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
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn zip_with(path: &Path, entries: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, content) in entries {
            writer.start_file(*name, SimpleFileOptions::default()).unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn verify_sha256_matches_known_digest() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("hello.txt");
        fs::write(&file, "hello").unwrap();
        let digest = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        assert!(verify_sha256(&file, digest).unwrap());
        assert!(!verify_sha256(&file, &digest.replace('2', "3")).unwrap());
    }

    #[test]
    fn extract_strips_the_prefix_and_skips_other_entries() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("a.zip");
        zip_with(
            &archive,
            &[
                ("pkg-1.0/bin/tool.exe", "x"),
                ("pkg-1.0/README", "r"),
                ("other/file", "o"),
            ],
        );
        let out = directory.path().join("out");
        extract(&archive, &out, "pkg-1.0/").unwrap();
        assert_eq!(fs::read_to_string(out.join("bin/tool.exe")).unwrap(), "x");
        assert!(out.join("README").is_file());
        assert!(!out.join("other").exists());
    }

    #[test]
    fn extract_rejects_path_traversal() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("evil.zip");
        zip_with(&archive, &[("../evil.txt", "boom")]);
        assert!(extract(&archive, &directory.path().join("out"), "").is_err());
        assert!(!directory.path().join("evil.txt").exists());
    }

    #[test]
    fn extract_into_leaves_no_partial_install_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("a.zip");
        zip_with(&archive, &[("readme.txt", "no marker here")]);
        let destination = directory.path().join("runtimes/tool/1.0");
        let error = extract_into(&archive, &destination, &WINDOWS_PACKAGES[0])
            .unwrap_err()
            .to_string();
        assert!(error.contains("incomplete"));
        assert!(!destination.exists());
        assert_eq!(
            fs::read_dir(directory.path().join("runtimes/tool"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn catalog_entries_are_well_formed() {
        for package in WINDOWS_PACKAGES {
            assert_eq!(package.sha256.len(), 64, "{}", package.id);
            assert!(package.url.starts_with("https://"), "{}", package.id);
        }
    }
}
