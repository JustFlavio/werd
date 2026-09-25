use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;
use zip::ZipArchive;

#[derive(Clone, Serialize)]
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
    destination: &'static str,
    strip: &'static str,
    marker: &'static str,
    note: &'static str,
}

const WINDOWS_PACKAGES: &[Package] = &[
    Package { id: "php", version: "8.5.11", url: "https://windows.php.net/downloads/releases/php-8.5.11-nts-Win32-vs17-x64.zip", sha256: "0ea96e0d2b9b737a6036f05cf4e95c49313faa6d0f27bd97edb2742503f0c043", destination: "php/8.5", strip: "", marker: "php-cgi.exe", note: "PHP NTS ufficiale; estensioni configurabili localmente" },
    Package { id: "caddy", version: "2.11.4", url: "https://github.com/caddyserver/caddy/releases/download/v2.11.4/caddy_2.11.4_windows_amd64.zip", sha256: "1708333f79e274c7697285afe6d592ab39314e0b131e9ec6bea08ad27df62ebf", destination: "caddy/2.11.4", strip: "", marker: "caddy.exe", note: "Server HTTPS locale" },
    Package { id: "postgres", version: "18.6", url: "https://get.enterprisedb.com/postgresql/postgresql-18.6-1-windows-x64-binaries.zip", sha256: "fbe23da234ee31547bf8a36d29dfd81e82b849df2d2b78d2eecb43d360252f8c", destination: "postgres/18", strip: "pgsql/", marker: "bin/postgres.exe", note: "pgvector richiede la build abbinata a questi binari" },
    Package { id: "pgvector", version: "0.8.6", url: "https://codeload.github.com/pgvector/pgvector/zip/refs/tags/v0.8.6", sha256: "e93a1567219c9ce523ca16473f6c41cc80e01345b2d91ccdee40b473b7c5dd0a", destination: "sources/pgvector/0.8.6", strip: "pgvector-0.8.6/", marker: "Makefile.win", note: "Compilato su richiesta contro PostgreSQL 18.6; richiede Visual Studio Build Tools" },
    Package { id: "redis", version: "7.2.8", url: "https://github.com/redis-windows/redis-windows/releases/download/7.2.8/Redis-7.2.8-Windows-x64-msys2.zip", sha256: "aa6d4206a08d1189dd7a57c78201540a53b12350e1ad09ee7caca04120600656", destination: "redis/7.2", strip: "Redis-7.2.8-Windows-x64-msys2/", marker: "redis-server.exe", note: "Port comunitario Windows; audit licenze dipendenze prima della beta" },
    Package { id: "mailpit", version: "1.31.2", url: "https://github.com/axllent/mailpit/releases/download/v1.31.2/mailpit-windows-amd64.zip", sha256: "42c20e5c3254125ea7489847811f10d70e39de573fe41d03a61412c87913e995", destination: "mailpit/1.31.2", strip: "", marker: "mailpit.exe", note: "SMTP e inbox locali" },
    Package { id: "rustfs", version: "1.0.0", url: "https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-windows-x86_64-v1.0.0.zip", sha256: "4ccf5858ce8e6f70f01af2394c8cc0e0878ee77faa6c20d3179153476554b7d8", destination: "rustfs/1.0.0", strip: "", marker: "rustfs.exe", note: "Storage oggetti S3 locale" },
];

fn catalog() -> Result<&'static [Package]> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Ok(WINDOWS_PACKAGES)
    } else {
        bail!("Il catalogo dei runtime per questa piattaforma non è ancora disponibile")
    }
}

pub fn list(root: &Path) -> Vec<RuntimeInfo> {
    WINDOWS_PACKAGES
        .iter()
        .map(|package| RuntimeInfo {
            id: package.id,
            version: package.version,
            installed: if package.id == "pgvector" {
                vector_installed(root)
            } else {
                root.join("runtimes")
                    .join(package.destination)
                    .join(package.marker)
                    .is_file()
            },
            note: package.note,
        })
        .collect()
}

pub fn install(root: &Path, id: &str) -> Result<RuntimeInfo> {
    let package = catalog()?
        .iter()
        .find(|package| package.id == id)
        .context("Runtime sconosciuto")?;
    let runtime_root = root.join("runtimes");
    let destination = runtime_root.join(package.destination);
    if id == "pgvector" && vector_installed(root) {
        return Ok(RuntimeInfo {
            id: package.id,
            version: package.version,
            installed: true,
            note: package.note,
        });
    }
    if id == "pgvector" && !runtime_root.join("postgres/18/bin/postgres.exe").is_file() {
        bail!("Installa PostgreSQL 18 prima di pgvector");
    }
    if id != "pgvector" && destination.join(package.marker).is_file() {
        return Ok(RuntimeInfo {
            id: package.id,
            version: package.version,
            installed: true,
            note: package.note,
        });
    }
    if !destination.join(package.marker).is_file() {
        let downloads = root.join("downloads");
        fs::create_dir_all(&downloads)?;
        let archive_path = downloads.join(format!("{}-{}.zip", package.id, package.version));
        if !archive_path.is_file() || !verify_sha256(&archive_path, package.sha256)? {
            let temporary = downloads.join(format!("{}-{}.part", package.id, package.version));
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(600))
                .build()?;
            let response = client.get(package.url).send()?.error_for_status()?;
            if response
                .content_length()
                .is_some_and(|length| length > 3 * 1024 * 1024 * 1024)
            {
                bail!("Archivio {} troppo grande", package.id);
            }
            let mut file = File::create(&temporary)?;
            let downloaded = std::io::copy(&mut response.take(3 * 1024 * 1024 * 1024 + 1), &mut file)?;
            if downloaded > 3 * 1024 * 1024 * 1024 {
                bail!("Archivio {} troppo grande", package.id);
            }
            file.sync_all()?;
            if !verify_sha256(&temporary, package.sha256)? {
                let _ = fs::remove_file(&temporary);
                bail!("Checksum non valido per {}: download rifiutato", package.id);
            }
            if archive_path.exists() {
                fs::remove_file(&archive_path)?;
            }
            fs::rename(temporary, &archive_path)?;
        }
        let staging = runtime_root.join(format!(".{}-{}-extract", package.id, package.version));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        let result = extract(&archive_path, &staging, package.strip).and_then(|_| {
            if staging.join(package.marker).is_file() {
                Ok(())
            } else {
                bail!("Archivio {} incompleto: {} assente", package.id, package.marker)
            }
        });
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        if destination.exists() {
            fs::remove_dir_all(&destination)?;
        }
        fs::rename(&staging, &destination)?;
    }
    if id == "pgvector" {
        build_pgvector(&destination, &runtime_root.join("postgres/18"))?;
    }
    if id == "php" {
        fs::write(destination.join("php.ini"), "[PHP]\nextension_dir=ext\nextension=curl\nextension=fileinfo\nextension=intl\nextension=mbstring\nextension=openssl\nextension=pdo_pgsql\nextension=pgsql\nextension=pdo_sqlite\nextension=sqlite3\nextension=zip\nextension=sodium\ndate.timezone=UTC\n")?;
    }
    Ok(RuntimeInfo {
        id: package.id,
        version: package.version,
        installed: true,
        note: package.note,
    })
}

fn vector_installed(root: &Path) -> bool {
    let postgres = root.join("runtimes/postgres/18");
    postgres.join("lib/vector.dll").is_file() && postgres.join("share/extension/vector.control").is_file()
}

fn build_pgvector(source: &Path, postgres: &Path) -> Result<()> {
    let vswhere = Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    if !vswhere.is_file() {
        bail!("Visual Studio 2022 Build Tools con MSVC richiesti per compilare pgvector");
    }
    let output = super::hidden_command(vswhere)
        .args(["-latest", "-products", "*", "-property", "installationPath"])
        .output()?;
    if !output.status.success() {
        bail!("Visual Studio Build Tools non trovati");
    }
    let location = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if location.is_empty() {
        bail!("Visual Studio Build Tools non trovati");
    }
    let devcmd = Path::new(&location).join("Common7/Tools/VsDevCmd.bat");
    if !devcmd.is_file() {
        bail!("VsDevCmd.bat non trovato");
    }
    let script = source.join("werd-build.cmd");
    fs::write(&script, format!("@echo off\r\ncall \"{}\" -arch=x64\r\nif errorlevel 1 exit /b 1\r\nnmake /F Makefile.win\r\nif errorlevel 1 exit /b 1\r\nnmake /F Makefile.win install\r\n", devcmd.display()))?;
    let output = super::hidden_command("cmd.exe")
        .arg("/c")
        .arg(&script)
        .current_dir(source)
        .env("PGROOT", postgres)
        .output()
        .context("Compilazione pgvector non avviata")?;
    if !output.status.success() {
        bail!(
            "Compilazione pgvector fallita: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !postgres.join("lib/vector.dll").is_file()
        || !postgres.join("share/extension/vector.control").is_file()
    {
        bail!("Build pgvector incompleta: vector.dll o vector.control mancanti");
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

fn extract(path: &Path, destination: &Path, strip: &str) -> Result<()> {
    let mut archive = ZipArchive::new(File::open(path)?)?;
    let mut expanded: u64 = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(enclosed) = entry.enclosed_name() else {
            bail!("Percorso ZIP non sicuro")
        };
        let name = enclosed.to_string_lossy().replace('\\', "/");
        let Some(relative) = name.strip_prefix(strip) else {
            continue;
        };
        if relative.is_empty() {
            continue;
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > 3 * 1024 * 1024 * 1024 {
            bail!("Archivio ZIP troppo grande");
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
