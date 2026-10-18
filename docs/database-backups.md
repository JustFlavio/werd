# Database backups

Current source supports local logical backups of a single PostgreSQL, MySQL or
MariaDB database. In **Services → Database backups**, enter the source database
name and create a backup. Werd runs it as a background job; the window may close
while it continues. Completed backups appear in that dialog.

To restore, select a backup and enter a **new database name**. The target service
must be running and use the same engine and version line as the backup. Werd
refuses an existing destination, including an empty database. It does not change
the site's service link or `.env`; connect to the recovered database explicitly
after checking its contents.

## CLI

```text
werd service backup "PostgreSQL 18" shop
werd service backups
werd service restore "PostgreSQL 18" <backup-id> shop_recovered
```

Service selectors accept an id or unique name. All three commands support
`--json`; progress goes to stderr. Database names use 1–63 lowercase letters,
digits or underscores, starting with a letter. System databases are excluded.

The same commands work with MySQL and MariaDB instances. Restores can use another
instance of the same engine/version line, and never replace an existing database.
Supported extensions must also be available on the destination.

## Storage and failure handling

Each completed backup is a directory at
`<WERD_HOME>/backups/<backup-id>/`, containing:

- `database.dump`: a PostgreSQL custom archive or a MySQL/MariaDB SQL dump;
- `metadata.json`: schema version, engine/version line, source service/database,
  creation time, size and SHA-256. It contains no connection credentials.

Werd writes into a unique staging directory and publishes it after the database
tool succeeds and the files have been synced. Interrupted staging directories
are never listed as usable backups. Restore checks size and SHA-256 before
creating a database, then reads the same verified file handle. Backup ids are
validated and dump/metadata symlinks are refused.

These are unencrypted local snapshots. Copy the complete backup directory to a
separate disk for safekeeping; keeping it beside the database does not protect
against losing that disk. On Unix, published backup directories are owner-only
and their data/metadata files have mode `0600`; Windows uses inherited ACLs.
There is no automatic retention or remote backup service in this increment.

If restoration fails after creating the new database, Werd retains that database
for inspection and reports the failure. The original database and backup remain
available. PostgreSQL restores run in one transaction; MySQL/MariaDB DDL is not
transactional, so a failed target may contain partial objects. Retry with another
new name after resolving the error.

Only one background job can use a runtime line at a time. Werd refuses stopping
or deleting its services and removing its runtime during backup/restore. The
operation releases the daemon state lock, so inventory and other RPC calls keep
answering. Database tools have a 30-minute deadline and owned process trees.

## Scope

PostgreSQL uses `pg_dump` custom archives and `pg_restore` with error stopping,
one transaction and without replaying ownership/ACL changes. These are
single-database snapshots; cluster roles and global configuration are excluded.
See the official [pg_dump](https://www.postgresql.org/docs/18/app-pgdump.html) and
[pg_restore](https://www.postgresql.org/docs/18/app-pgrestore.html) documentation.

MySQL/MariaDB use their matching dump/client binaries. Dumps include routines,
events, triggers and binary values. `--single-transaction` provides a consistent
snapshot for transactional tables; it does not make nontransactional tables or
concurrent schema changes safe. Use this with a trusted local development
database and pause schema changes while taking the snapshot. See
[mysqldump](https://dev.mysql.com/doc/refman/8.4/en/mysqldump.html).

Restoring a database executes code stored in its dump. SHA-256 detects damaged
files; it does not authenticate a dump from another person. This increment
restores Werd-managed backups and does not provide arbitrary SQL-file import,
cluster snapshots, engine migrations, automatic pre-upgrade backups or an
overwrite mode. MongoDB, Redis and object storage require their own backup
designs. The seven MCP tools remain read-only.

## Verification — 2026-10-18

On Windows x64, real-engine acceptance passed with:

| Engine | Version | Checked behavior |
| --- | --- | --- |
| PostgreSQL | 18.6 | Schema, rows and sequence restoration; source preservation; existing-target and checksum rejection; service/daemon restart |
| MySQL | 8.4.11 | Actual CLI backup/restore, tables, rows, view, routine, AUTO_INCREMENT, source preservation, existing-target/checksum rejection and service restart |
| MariaDB | 11.4.13 | Same CLI and database checks as MySQL |

A newly installed Laravel 13.37.0 application, using PHP 8.4.26 and Werd's Caddy,
read the restored PostgreSQL database over a real HTTPS request, before and after
restarting the daemon, service and site. The client trusted only that fixture's
CA. Hosts, user PATH and system certificate trust were unchanged. All services,
projects, credentials, Composer state and runtime copies used temporary homes;
the installed Werd/Herd services and the user's projects were untouched.

Unit tests cover incomplete dumps, integrity rejection before database creation,
version/engine mismatch, unsafe names/ids, existing destinations, failed restore
preservation and guards against stopping/deleting busy services. UI tests cover
completed-job publication, backend refusal, compatibility filtering, stopped
services and reopening a running job. The Windows workspace has 129 passing Rust
tests and 27 frontend tests. Unix-only permission/symlink tests still require CI
execution on those platforms.

Reproduce the real-engine checks after building the CLI and daemon:

```powershell
node scripts/acceptance/windows-development.mjs --runtime-home "$env:LOCALAPPDATA/Werd"
node scripts/acceptance/sql-backups.mjs --product mysql --line 8.4
node scripts/acceptance/sql-backups.mjs --product mariadb --line 11.4
```

The first script requires existing Werd PHP 8.4, its PECL extensions, PostgreSQL,
Caddy, Composer and CA-bundle binaries to copy; it downloads Laravel dependencies.
The SQL scripts download their requested runtimes through Werd's checked catalog.
They retain their temporary homes and print their locations for inspection.
Local results for this run are in ignored `.cache/windows-acceptance.json`,
`.cache/mysql-acceptance.json` and `.cache/mariadb-acceptance.json`.

These runs do not validate Vite/HMR, queue workers, browser trust/onboarding,
packaged installers or macOS/Linux runtime behavior. Those remain roadmap gates.
