//! Explicit, previewed updates to Werd-managed environment values.
use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

#[derive(Serialize)]
pub(crate) struct Change {
    pub key: String,
    pub before: Option<String>,
    pub after: String,
}
#[derive(Serialize)]
pub(crate) struct Preview {
    pub revision: String,
    pub exists: bool,
    pub changes: Vec<Change>,
}

fn regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(true),
        Ok(_) => bail!(
            "{} must be a regular file; symbolic links are not updated",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn read(folder: &Path) -> Result<(bool, String)> {
    let exists = regular(&folder.join(".env"))?;
    let source = folder.join(if exists { ".env" } else { ".env.example" });
    let contents = if regular(&source)? {
        fs::read_to_string(source)?
    } else {
        String::new()
    };
    Ok((exists, contents))
}

fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start_matches('\u{feff}').trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
    .then_some((key, value.trim()))
}

fn value(raw: &str) -> Result<String> {
    if raw.starts_with(['\'', '"']) {
        let quote = raw.as_bytes()[0];
        let mut escaped = false;
        for (index, byte) in raw.bytes().enumerate().skip(1) {
            if byte == quote && !escaped {
                let rest = raw[index + 1..].trim();
                if !rest.is_empty() && !rest.starts_with('#') {
                    bail!("Unsupported environment syntax; edit .env manually");
                }
                return Ok(raw[1..index].to_string());
            }
            escaped = byte == b'\\' && !escaped;
        }
        bail!("Multiline environment values require manual editing");
    }
    Ok(raw
        .split_once(" #")
        .map_or(raw, |(text, _)| text)
        .trim()
        .to_string())
}

fn plan(exists: bool, contents: &str, suggested: &str) -> Result<(Preview, String)> {
    let wanted: BTreeMap<_, _> = suggested
        .lines()
        .filter_map(assignment)
        .map(|(key, raw)| Ok((key.to_string(), value(raw)?)))
        .collect::<Result<_>>()?;
    let mut seen = BTreeSet::new();
    let mut current = BTreeMap::new();
    for line in contents.lines() {
        if let Some((key, raw)) = assignment(line) {
            let parsed = value(raw)?; // Refuse multiline files instead of changing text inside another value.
            if wanted.contains_key("DB_CONNECTION")
                && matches!(key, "DB_URL" | "DATABASE_URL")
                && !parsed.is_empty()
            {
                bail!("{key} overrides individual database settings. Review it manually before applying local connections.");
            }
            if wanted.contains_key(key) {
                if !seen.insert(key) {
                    bail!("Duplicate {key} entries; edit .env manually before continuing");
                }
                current.insert(key.to_string(), parsed);
            }
        }
    }
    let changes = wanted
        .iter()
        .filter(|(key, after)| current.get(*key) != Some(*after))
        .map(|(key, after)| Change {
            key: key.clone(),
            before: current.get(key).cloned(),
            after: after.clone(),
        })
        .collect();
    let revision = Sha256::digest(format!("{exists}\0{contents}\0{suggested}"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let newline = if contents.contains("\r\n") { "\r\n" } else { "\n" };
    let mut updated = Vec::new();
    for line in contents.lines() {
        match assignment(line).and_then(|(key, _)| wanted.get_key_value(key)) {
            Some((key, expected)) if current.get(key) != Some(expected) => {
                updated.push(format!("{key}={expected}"))
            }
            _ => updated.push(line.to_string()),
        }
    }
    for (key, expected) in &wanted {
        if !seen.contains(key.as_str()) {
            updated.push(format!("{key}={expected}"));
        }
    }
    Ok((
        Preview {
            revision,
            exists,
            changes,
        },
        updated.join(newline) + newline,
    ))
}

pub(crate) fn preview(folder: &Path, suggested: &str) -> Result<Preview> {
    let (exists, contents) = read(folder)?;
    Ok(plan(exists, &contents, suggested)?.0)
}

pub(crate) fn has_app_key(folder: &Path) -> bool {
    fs::read_to_string(folder.join(".env"))
        .ok()
        .is_some_and(|contents| {
            contents
                .lines()
                .filter_map(assignment)
                .any(|(key, raw)| key == "APP_KEY" && value(raw).is_ok_and(|text| !text.is_empty()))
        })
}

fn private_write(path: &Path, contents: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn apply(
    folder: &Path,
    backup_dir: &Path,
    suggested: &str,
    revision: &str,
) -> Result<Option<String>> {
    let (exists, contents) = read(folder)?;
    let (preview, updated) = plan(exists, &contents, suggested)?;
    if preview.revision != revision {
        bail!("Environment values changed since the preview. Review the new preview before applying it.");
    }
    if exists && preview.changes.is_empty() {
        return Ok(None);
    }
    let backup = if exists {
        fs::create_dir_all(backup_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(backup_dir, fs::Permissions::from_mode(0o700))?;
        }
        let path = backup_dir.join(format!("{}.env", uuid::Uuid::new_v4()));
        private_write(&path, contents.as_bytes())?;
        Some(path.to_string_lossy().into_owned())
    } else {
        None
    };
    let temporary = folder.join(format!(".werd-env-{}.tmp", uuid::Uuid::new_v4()));
    let result: Result<()> = (|| {
        private_write(&temporary, updated.as_bytes())?;
        fs::rename(&temporary, folder.join(".env"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.context("Cannot update the project's .env")?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_apply_preserves_unmanaged_values_and_backs_up_the_original() {
        let folder = tempfile::tempdir().unwrap();
        let backups = tempfile::tempdir().unwrap();
        let original = "# project\r\nAPP_KEY=keep\r\nDB_HOST=remote\r\nDB_PASSWORD='old'\r\n";
        fs::write(folder.path().join(".env"), original).unwrap();
        let suggested = "DB_HOST=127.0.0.1\nDB_PASSWORD=new\nDB_DATABASE=shop";
        let preview = preview(folder.path(), suggested).unwrap();
        assert_eq!(preview.changes.len(), 3);
        let backup = apply(folder.path(), backups.path(), suggested, &preview.revision)
            .unwrap()
            .unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), original);
        let updated = fs::read_to_string(folder.path().join(".env")).unwrap();
        assert!(updated.contains("APP_KEY=keep\r\nDB_HOST=127.0.0.1\r\n"));
        assert!(super::preview(folder.path(), suggested)
            .unwrap()
            .changes
            .is_empty());
    }
    #[test]
    fn stale_previews_and_complex_files_are_not_written() {
        let folder = tempfile::tempdir().unwrap();
        let suggested = "DB_HOST=127.0.0.1";
        fs::write(folder.path().join(".env"), "DB_HOST=remote").unwrap();
        let old = preview(folder.path(), suggested).unwrap();
        for contents in [
            "DB_HOST=edited",
            "DB_HOST=a\nDB_HOST=b",
            "OTHER=\"multiline\nDB_HOST=inside\n\"",
        ] {
            fs::write(folder.path().join(".env"), contents).unwrap();
            assert!(apply(
                folder.path(),
                &folder.path().join("backups"),
                suggested,
                &old.revision
            )
            .is_err());
            assert_eq!(fs::read_to_string(folder.path().join(".env")).unwrap(), contents);
        }
    }

    #[test]
    fn database_url_overrides_require_manual_review_and_matching_quotes_are_unchanged() {
        assert!(plan(true, "DB_URL=mysql://remote\n", "DB_CONNECTION=pgsql").is_err());
        let (preview, _) = plan(
            true,
            "export DB_HOST=\"127.0.0.1\" # local\nAPP_KEY=keep",
            "DB_HOST=127.0.0.1",
        )
        .unwrap();
        assert!(preview.changes.is_empty());
    }
    #[test]
    fn missing_env_starts_from_example_and_detects_changed_connections() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(
            folder.path().join(".env.example"),
            "APP_KEY=\nDB_HOST=localhost\n",
        )
        .unwrap();
        let old = preview(folder.path(), "DB_HOST=127.0.0.1").unwrap();
        assert!(!old.exists);
        assert!(apply(
            folder.path(),
            &folder.path().join("backups"),
            "DB_HOST=other",
            &old.revision
        )
        .is_err());
        apply(
            folder.path(),
            &folder.path().join("backups"),
            "DB_HOST=127.0.0.1",
            &old.revision,
        )
        .unwrap();
        assert!(fs::read_to_string(folder.path().join(".env"))
            .unwrap()
            .contains("APP_KEY="));
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_environment_is_never_updated() {
        let folder = tempfile::tempdir().unwrap();
        fs::write(folder.path().join("original"), "DB_HOST=remote").unwrap();
        std::os::unix::fs::symlink(folder.path().join("original"), folder.path().join(".env")).unwrap();
        assert!(preview(folder.path(), "DB_HOST=local").is_err());
    }
}
