//! Logical database snapshots. Restores always create a new database.

use crate::instances::{Credentials, InstanceContext, ServiceInstance};
use crate::jobs::Progress;
use crate::process::{hidden_command, spawn_managed, tail_file};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DATA: &str = "database.dump";
const META: &str = "metadata.json";
const TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Backup {
    pub schema: u32,
    pub id: String,
    pub instance: String,
    pub service_name: String,
    pub product: String,
    pub line: String,
    pub database: String,
    pub created_at: u64,
    pub bytes: u64,
    pub sha256: String,
}

pub(crate) fn supported(product: &str) -> bool {
    matches!(product, "postgresql" | "mysql" | "mariadb")
}

pub(crate) fn database_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 63
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        || matches!(
            name,
            "postgres"
                | "template0"
                | "template1"
                | "mysql"
                | "sys"
                | "information_schema"
                | "performance_schema"
        )
    {
        bail!("Use a non-system database name: 1–63 lowercase letters, digits or underscores, starting with a letter");
    }
    Ok(())
}

fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    if uuid::Uuid::parse_str(id)
        .ok()
        .is_none_or(|value| value.to_string() != id)
    {
        bail!("Invalid backup id");
    }
    Ok(root.join("backups").join(id))
}

fn regular(path: &Path, is_dir: bool) -> Result<()> {
    let kind = fs::symlink_metadata(path)?.file_type();
    if kind.is_symlink() || (is_dir && !kind.is_dir()) || (!is_dir && !kind.is_file()) {
        bail!("Invalid backup entry: {}", path.display());
    }
    Ok(())
}

pub(crate) fn load(root: &Path, id: &str) -> Result<Backup> {
    let dir = directory(root, id)?;
    regular(&dir, true)?;
    let meta = dir.join(META);
    regular(&meta, false)?;
    let mut bytes = Vec::new();
    File::open(meta)?.take(65_537).read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        bail!("Backup metadata is too large");
    }
    let backup: Backup = serde_json::from_slice(&bytes).context("Invalid backup metadata")?;
    if backup.schema != 1 || backup.id != id || !supported(&backup.product) {
        bail!("Unsupported or inconsistent backup metadata");
    }
    database_name(&backup.database)?;
    Ok(backup)
}

pub(crate) fn list(root: &Path) -> Result<Vec<Backup>> {
    let parent = root.join("backups");
    if !parent.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.starts_with('.') {
            continue;
        } // Interrupted staging is never a usable backup.
        records.push(load(root, &id).with_context(|| format!("Cannot read backup {id}"))?);
    }
    records.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
    Ok(records)
}

fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn private_directory(path: &Path) -> Result<()> {
    fs::create_dir(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn digest(file: &mut File) -> Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

trait Database {
    fn dump(&self, database: &str, output: File, work: &Path) -> Result<()>;
    /// Must fail if the target already exists, including an empty database.
    fn create(&self, database: &str, work: &Path) -> Result<()>;
    fn restore(&self, database: &str, input: File, work: &Path) -> Result<()>;
}

pub(crate) fn create(
    root: &Path,
    instance: &ServiceInstance,
    database: &str,
    progress: &Progress,
) -> Result<Backup> {
    create_with(root, instance, database, progress, &Native::new(root, instance)?)
}

fn create_with(
    root: &Path,
    instance: &ServiceInstance,
    database: &str,
    progress: &Progress,
    backend: &dyn Database,
) -> Result<Backup> {
    database_name(database)?;
    if !supported(&instance.product) {
        bail!("Database backups are not supported for {}", instance.product);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let parent = root.join("backups");
    fs::create_dir_all(&parent)?;
    let staging = parent.join(format!(".{id}.staging"));
    private_directory(&staging)?;
    let result = (|| {
        progress.step("Exporting database");
        let data = staging.join(DATA);
        let output = private_file(&data)?;
        backend.dump(database, output.try_clone()?, &staging)?;
        output.sync_all()?;
        drop(output);
        progress.step("Verifying backup");
        let bytes = fs::metadata(&data)?.len();
        if bytes == 0 {
            bail!("The database tool produced an empty backup");
        }
        let backup = Backup {
            schema: 1,
            id: id.clone(),
            instance: instance.id.clone(),
            service_name: instance.name.clone(),
            product: instance.product.clone(),
            line: instance.line.clone(),
            database: database.into(),
            created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            bytes,
            sha256: digest(&mut File::open(&data)?)?,
        };
        let mut meta = private_file(&staging.join(META))?;
        meta.write_all(&serde_json::to_vec_pretty(&backup)?)?;
        meta.sync_all()?;
        drop(meta);
        // Native tool scratch output is not part of the published backup.
        let _ = fs::remove_file(staging.join("command.log"));
        fs::rename(&staging, directory(root, &id)?)?;
        Ok(backup)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub(crate) fn restore(
    root: &Path,
    instance: &ServiceInstance,
    id: &str,
    database: &str,
    progress: &Progress,
) -> Result<()> {
    restore_with(
        root,
        instance,
        id,
        database,
        progress,
        &Native::new(root, instance)?,
    )
}

fn restore_with(
    root: &Path,
    instance: &ServiceInstance,
    id: &str,
    database: &str,
    progress: &Progress,
    backend: &dyn Database,
) -> Result<()> {
    database_name(database)?;
    let backup = load(root, id)?;
    if (backup.product.as_str(), backup.line.as_str()) != (instance.product.as_str(), instance.line.as_str())
    {
        bail!(
            "Restore requires the same database engine and version line: {} {}",
            backup.product,
            backup.line
        );
    }
    let dir = directory(root, id)?;
    let data = dir.join(DATA);
    regular(&data, false)?;
    // Verify and restore through the same handle; replacing the path afterwards
    // must not substitute another dump between verification and execution.
    let mut input = File::open(&data)?;
    progress.step("Verifying backup");
    if input.metadata()?.len() != backup.bytes || digest(&mut input)? != backup.sha256 {
        bail!("Backup checksum mismatch; no database was created");
    }
    use std::io::{Seek, SeekFrom};
    input.seek(SeekFrom::Start(0))?;
    let work = root
        .join("backups")
        .join(format!(".restore-{}", uuid::Uuid::new_v4()));
    private_directory(&work)?;
    let result = (|| {
        progress.step("Creating new database");
        backend.create(database, &work)?;
        progress.step("Restoring database");
        backend.restore(database, input, &work).with_context(|| format!("Restore failed; new database {database} was kept for inspection. Existing databases were not replaced"))
    })();
    let _ = fs::remove_dir_all(work);
    result
}

struct Native<'a> {
    context: InstanceContext<'a>,
    credentials: Credentials,
}

impl<'a> Native<'a> {
    fn new(root: &'a Path, instance: &'a ServiceInstance) -> Result<Self> {
        if !supported(&instance.product) {
            bail!("Database backups are not supported for {}", instance.product);
        }
        let context = InstanceContext { root, instance };
        // Never regenerate credentials as a side effect of a backup operation.
        let credentials = serde_json::from_slice(&fs::read(context.dir().join("credentials.json"))?)?;
        Ok(Self { context, credentials })
    }

    fn command(&self, program: &str) -> Result<Command> {
        let mut command = hidden_command(self.context.binary(program)?);
        if self.context.instance.product == "postgresql" {
            command
                .args(["--host=127.0.0.1", "--no-password"])
                .arg(format!("--port={}", self.context.instance.port))
                .arg(format!("--username={}", self.credentials.username))
                .env("PGPASSWORD", &self.credentials.password)
                .env("PGCONNECT_TIMEOUT", "10")
                .env_remove("PGOPTIONS")
                .env_remove("PGSERVICE")
                .env_remove("PGSERVICEFILE");
        } else {
            command
                .args(["--no-defaults", "--host=127.0.0.1", "--protocol=TCP"])
                .arg(format!("--port={}", self.context.instance.port))
                .arg(format!("--user={}", self.credentials.username))
                .env("MYSQL_PWD", &self.credentials.password);
        }
        Ok(command)
    }

    fn run(
        &self,
        mut command: Command,
        work: &Path,
        input: Option<File>,
        output: Option<File>,
    ) -> Result<()> {
        let log = work.join("command.log");
        command
            .stdin(input.map_or_else(Stdio::null, Stdio::from))
            .stdout(output.map_or_else(Stdio::null, Stdio::from))
            .stderr(Stdio::from(File::create(&log)?));
        let mut process = spawn_managed("database backup tool", command)?;
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            if let Some(status) = process.child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                process.kill();
                bail!("Database operation exceeded 30 minutes");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        // Finalize the process tree and unregister it on both success and failure.
        process.kill();
        if !status.success() {
            let detail = tail_file(&log)?
                .join("\n")
                .replace(&self.credentials.password, "[REDACTED]");
            bail!("Database tool failed ({status}): {detail}");
        }
        Ok(())
    }
}

impl Database for Native<'_> {
    fn dump(&self, database: &str, output: File, work: &Path) -> Result<()> {
        let pg = self.context.instance.product == "postgresql";
        let program = match self.context.instance.product.as_str() {
            "postgresql" => "bin/pg_dump",
            "mysql" => "bin/mysqldump",
            _ => "bin/mariadb-dump",
        };
        let mut command = self.command(program)?;
        if pg {
            command
                .args(["--format=custom", "--no-owner", "--no-privileges"])
                .arg(format!("--dbname={database}"));
        } else {
            command.args([
                "--single-transaction",
                "--quick",
                "--routines",
                "--events",
                "--triggers",
                "--hex-blob",
            ]);
            if self.context.instance.product == "mysql" {
                command.arg("--set-gtid-purged=OFF");
            }
            command.arg(database);
        }
        self.run(command, work, None, Some(output))
    }

    fn create(&self, database: &str, work: &Path) -> Result<()> {
        let pg = self.context.instance.product == "postgresql";
        let program = if pg {
            "bin/psql"
        } else if self.context.instance.product == "mysql" {
            "bin/mysql"
        } else {
            "bin/mariadb"
        };
        let mut command = self.command(program)?;
        if pg {
            command
                .args(["-X", "--set=ON_ERROR_STOP=1", "--dbname=postgres", "--command"])
                .arg(format!("CREATE DATABASE \"{database}\" TEMPLATE template0"));
        } else {
            command
                .arg("--execute")
                .arg(format!("CREATE DATABASE `{database}`"));
        }
        self.run(command, work, None, None)
    }

    fn restore(&self, database: &str, input: File, work: &Path) -> Result<()> {
        let pg = self.context.instance.product == "postgresql";
        let program = if pg {
            "bin/pg_restore"
        } else if self.context.instance.product == "mysql" {
            "bin/mysql"
        } else {
            "bin/mariadb"
        };
        let mut command = self.command(program)?;
        if pg {
            command
                .args([
                    "--exit-on-error",
                    "--single-transaction",
                    "--no-owner",
                    "--no-privileges",
                ])
                .arg(format!("--dbname={database}"));
        } else {
            command.args(["--binary-mode", "--database"]).arg(database);
        }
        self.run(command, work, Some(input), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[cfg(unix)]
    #[test]
    fn completed_native_tools_are_unregistered_even_on_failure() {
        const TEST_HOME: &str = "WERD_BACKUP_PROCESS_TEST_HOME";
        let Some(root) = std::env::var_os(TEST_HOME) else {
            // Keep the process-wide registry isolated from other unit tests.
            let root = tempfile::tempdir().unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "backups::tests::completed_native_tools_are_unregistered_even_on_failure",
                    "--nocapture",
                ])
                .env(TEST_HOME, root.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "registry regression test failed: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        };
        let root = Path::new(&root);
        crate::process::stop_leftover_children(root);
        let instance = instance();
        let native = Native {
            context: InstanceContext {
                root,
                instance: &instance,
            },
            credentials: Credentials {
                username: "fixture".into(),
                password: String::new(),
            },
        };
        for code in [0, 1] {
            let observed = root.join(format!("registered-{code}"));
            let mut command = Command::new("/bin/sh");
            // The tool cannot exit before spawn_managed has recorded its PID.
            command
                .args([
                    "-c",
                    r#"
                attempt=0
                while [ "$attempt" -lt 500 ]; do
                    if grep -q "\"pid\":$$," "$1"; then
                        touch "$3"
                        exit "$2"
                    fi
                    attempt=$((attempt + 1))
                    sleep 0.01
                done
                exit 99
            "#,
                    "backup-fixture",
                ])
                .arg(root.join("children.json"))
                .arg(code.to_string())
                .arg(&observed);
            assert_eq!(native.run(command, root, None, None).is_ok(), code == 0);
            assert!(observed.is_file(), "the fake tool was never registered");
            let records: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("children.json")).unwrap()).unwrap();
            assert_eq!(records, serde_json::json!([]));
        }
    }

    #[test]
    fn checksum_matches_standard_sha256_hex_encoding() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("input");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            digest(&mut File::open(path).unwrap()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[derive(Default)]
    struct Fixture {
        databases: RefCell<BTreeMap<String, Vec<u8>>>,
        fail_dump: bool,
        fail_restore: bool,
    }

    impl Database for Fixture {
        fn dump(&self, _: &str, mut output: File, _: &Path) -> Result<()> {
            output.write_all(b"snapshot with schema, rows and sequence state")?;
            if self.fail_dump {
                bail!("dump failed after partial output");
            }
            Ok(())
        }
        fn create(&self, name: &str, _: &Path) -> Result<()> {
            use std::collections::btree_map::Entry;
            match self.databases.borrow_mut().entry(name.into()) {
                Entry::Occupied(_) => bail!("database already exists"),
                Entry::Vacant(entry) => {
                    entry.insert(Vec::new());
                }
            }
            Ok(())
        }
        fn restore(&self, name: &str, mut input: File, _: &Path) -> Result<()> {
            if self.fail_restore {
                bail!("restore failed");
            }
            let mut bytes = Vec::new();
            input.read_to_end(&mut bytes)?;
            self.databases.borrow_mut().insert(name.into(), bytes);
            Ok(())
        }
    }

    fn instance() -> ServiceInstance {
        ServiceInstance {
            id: "fixture".into(),
            name: "Fixture PostgreSQL".into(),
            product: "postgresql".into(),
            line: "18".into(),
            port: 5432,
            extra_ports: Default::default(),
            autostart: false,
            extensions: Vec::new(),
            status: crate::model::ProjectStatus::Running,
            error: None,
        }
    }

    #[test]
    fn round_trip_preserves_existing_data_and_refuses_existing_targets() {
        let root = tempfile::tempdir().unwrap();
        let backend = Fixture::default();
        backend
            .databases
            .borrow_mut()
            .insert("source".into(), b"live data".to_vec());
        backend
            .databases
            .borrow_mut()
            .insert("existing".into(), b"never replace".to_vec());
        let backup = create_with(
            root.path(),
            &instance(),
            "source",
            &Progress::detached(),
            &backend,
        )
        .unwrap();
        assert_eq!(list(root.path()).unwrap().len(), 1);
        assert!(restore_with(
            root.path(),
            &instance(),
            &backup.id,
            "existing",
            &Progress::detached(),
            &backend
        )
        .is_err());
        restore_with(
            root.path(),
            &instance(),
            &backup.id,
            "recovered",
            &Progress::detached(),
            &backend,
        )
        .unwrap();
        let databases = backend.databases.borrow();
        assert_eq!(databases["source"], b"live data");
        assert_eq!(databases["existing"], b"never replace");
        assert_eq!(
            databases["recovered"],
            b"snapshot with schema, rows and sequence state"
        );
    }

    #[test]
    fn failed_dumps_are_never_published_and_interrupted_staging_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        let backend = Fixture {
            fail_dump: true,
            ..Default::default()
        };
        assert!(create_with(
            root.path(),
            &instance(),
            "source",
            &Progress::detached(),
            &backend
        )
        .is_err());
        assert_eq!(fs::read_dir(root.path().join("backups")).unwrap().count(), 0);
        fs::create_dir(root.path().join("backups/.interrupted.staging")).unwrap();
        assert!(list(root.path()).unwrap().is_empty());
    }

    #[test]
    fn tampered_backup_is_rejected_before_a_database_is_created() {
        let root = tempfile::tempdir().unwrap();
        let backend = Fixture::default();
        let backup = create_with(
            root.path(),
            &instance(),
            "source",
            &Progress::detached(),
            &backend,
        )
        .unwrap();
        fs::write(directory(root.path(), &backup.id).unwrap().join(DATA), b"corrupt").unwrap();
        let error = restore_with(
            root.path(),
            &instance(),
            &backup.id,
            "recovered",
            &Progress::detached(),
            &backend,
        )
        .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
        assert!(backend.databases.borrow().is_empty());
    }

    #[test]
    fn incompatible_versions_and_unsafe_identifiers_cannot_mutate_data() {
        let root = tempfile::tempdir().unwrap();
        let backend = Fixture::default();
        let backup = create_with(
            root.path(),
            &instance(),
            "source",
            &Progress::detached(),
            &backend,
        )
        .unwrap();
        for product in ["postgresql", "mysql", "mariadb"] {
            let mut target = instance();
            target.product = product.into();
            target.line = "17".into();
            assert!(restore_with(
                root.path(),
                &target,
                &backup.id,
                "recovered",
                &Progress::detached(),
                &backend
            )
            .is_err());
        }
        for name in [
            "postgres",
            "mysql",
            "../source",
            "--flag",
            "one'; DROP DATABASE source;--",
            "A",
            "",
        ] {
            assert!(database_name(name).is_err(), "{name}");
        }
        assert!(load(root.path(), "../state.json").is_err());
        assert!(backend.databases.borrow().is_empty());
    }

    #[test]
    fn failed_restore_keeps_new_database_and_backup_for_inspection() {
        let root = tempfile::tempdir().unwrap();
        let backend = Fixture {
            fail_restore: true,
            ..Default::default()
        };
        let backup = create_with(
            root.path(),
            &instance(),
            "source",
            &Progress::detached(),
            &backend,
        )
        .unwrap();
        let error = restore_with(
            root.path(),
            &instance(),
            &backup.id,
            "recovered",
            &Progress::detached(),
            &backend,
        )
        .unwrap_err();
        assert!(error.to_string().contains("kept for inspection"));
        assert!(backend.databases.borrow()["recovered"].is_empty());
        assert_eq!(list(root.path()).unwrap().len(), 1);
        assert_eq!(fs::read_dir(root.path().join("backups")).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn backup_files_are_private_and_a_symlink_cannot_substitute_the_dump() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let backend = Fixture::default();
        let backup = create_with(
            root.path(),
            &instance(),
            "source",
            &Progress::detached(),
            &backend,
        )
        .unwrap();
        let dir = directory(root.path(), &backup.id).unwrap();
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        for name in [DATA, META] {
            assert_eq!(
                fs::metadata(dir.join(name)).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let outside = root.path().join("outside");
        fs::copy(dir.join(DATA), &outside).unwrap();
        fs::remove_file(dir.join(DATA)).unwrap();
        symlink(outside, dir.join(DATA)).unwrap();
        assert!(restore_with(
            root.path(),
            &instance(),
            &backup.id,
            "recovered",
            &Progress::detached(),
            &backend
        )
        .is_err());
        assert!(backend.databases.borrow().is_empty());
    }
}
