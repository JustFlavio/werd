//! Publish complete metadata without removing the last good copy first.

use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// The temporary file lives beside the destination so rename stays on one
/// filesystem. Each writer owns its temporary file; callers must separately
/// serialize read/modify/write transactions when merging shared metadata.
pub(crate) fn replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Invalid metadata path")?;
    let name = path
        .file_name()
        .context("Invalid metadata path")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let result: Result<()> = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // fs::rename replaces an existing file on Windows and Unix. Never
        // unlink the destination: failure must leave the previous copy usable.
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("Cannot save {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_readers_always_see_a_complete_document() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        replace(&path, b"[0]").unwrap();
        std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                for value in 1..100 {
                    replace(&path, format!("[{value}]").as_bytes()).unwrap();
                }
            });
            for _ in 0..500 {
                let values: Vec<u32> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                assert_eq!(values.len(), 1);
            }
            writer.join().unwrap();
        });
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_publish_preserves_destination_and_cleans_its_temporary_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("keep"), "old data").unwrap();
        assert!(replace(&path, b"[]").is_err());
        assert_eq!(fs::read_to_string(path.join("keep")).unwrap(), "old data");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }
}
