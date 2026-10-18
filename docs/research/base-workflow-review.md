# Werd base workflow review — 2026-10-17

Werd remains an independent replacement for Herd. Keep the existing daemon,
runtime catalog, shared service instances, CLI and desktop architecture. This
review repairs existing behavior before adding more features. It does not
establish complete Herd parity or certify every supported target OS.

## Dependency maintenance

| Dependency | Previous version | Reviewed change |
| --- | --- | --- |
| rustls | 0.23.45 | 0.23.46, addressing RUSTSEC-2026-0346/0347/0348/0349 |
| zip | 2.4.2 directly, 4.6.1 through the updater | 9.0.4 throughout the resolved graph, addressing RUSTSEC-2026-0367/0368/0369 |
| yoke-derive | 0.8.3, yanked | 0.8.4 |
| source-map-js | 1.2.1, development dependency | 1.2.2, addressing GHSA-68fv-2mgg-jv7q |

The published `tauri-plugin-updater` 2.14.0 still depends on ZIP 4. The desktop
therefore uses the official Tauri v2 source at the exact commit
[`f36ba7ba48695776bc11cce0bfdc45678ec24f72`](https://github.com/tauri-apps/plugins-workspace/tree/f36ba7ba48695776bc11cce0bfdc45678ec24f72/plugins/updater),
which uses ZIP 9. Its Git source is explicitly allowed in `deny.toml`; unknown
Git sources remain denied. No vulnerability or yanked-package exceptions were
added. Return to a crates.io release once its published manifest contains the
fix. The compatible Tauri 2 dependencies also advance to Tauri 2.12.4.

`cargo deny --offline --format json check` passes advisories, licenses, bans
and sources. Duplicate-version and transitive maintenance warnings remain;
passing this check is not a claim that the software has no vulnerabilities.
`npm audit` reports zero known vulnerabilities after the lockfile update.
Local audit logs live in ignored `.cache/deny.jsonl` and `.cache/npm-audit.json`.

## Existing behavior corrected

| Finding | Previous failure | Correction and regression evidence |
| --- | --- | --- |
| PostgreSQL initialization | A failed read of an existing `PG_VERSION` could trigger initialization, followed by deletion of the data directory on failure. | Existing data requires a readable, matching marker. New clusters initialize in a unique staging directory and publish only after success. Tests preserve sentinel data for missing, invalid, unreadable and mismatched markers; cover success, failure/retry and a destination appearing during initialization. |
| Concurrent runtime installation | Separate load/change/save operations lost other jobs' registry entries. Shared partial-download and staging paths could collide for common dependencies. | Short registry transactions merge under a mutex. A per-root/product/line lock serializes installs/removals, including recursively installed dependencies. Downloads have unique partial files. Tests merge 16 concurrent additions with removal, and install two lines concurrently from the same archive. |
| Project registry persistence | Saving removed `state.json` before renaming its replacement, leaving a missing-file window. | Unique sibling temporary files, synced contents and replacement by rename retain the old destination on publication failure. The helper also saves the runtime registry. Tests continuously read while replacing and verify cleanup after a failed publication. |
| Redis cache/queue selection | Linking different Redis instances produced one deduplicated `REDIS_*` endpoint, silently pointing both roles at one service. | Reject incompatible selections before changing project state, including manifest auto-linking. Starting a persisted conflicting site and generating its environment suggestions also return an actionable error. Tests cover both link orders, persisted conflicts, repair and manifest rejection. |
| Windows process ownership | If a Vite parent exited first, its remaining workers could survive a site stop because `taskkill /T` no longer had the parent tree. | Each supervised child owns a kill-on-close Job Object, assigned before resuming the new process. Stopping or dropping it terminates descendants even after the parent exits. Tests use disposable parent/worker processes, also inside the daemon's outer job. No private Windows APIs are used. |

The registry mutexes coordinate writers inside the single Werd daemon, not
arbitrary external processes editing its files. Atomic publication prevents
partial documents and the deliberate unlink window; power-loss durability of
filesystem directory metadata is not established by these tests. Standard
Laravel environment suggestions currently support a single Redis instance for
cache and queue. Separate endpoints need explicit application connection
configuration and remain a product gap.

ZIP extraction retains the existing manual path validation and permission mask.
ZIP symlinks are materialized as regular files rather than followed. A regression
fixture confirms that an outside target remains untouched. Existing archive,
checksum, update, PHP extension and TAR tests also run against ZIP 9.

## Reconciliation of existing features

| Workflow | Retained implementation / evidence | Still to validate or implement |
| --- | --- | --- |
| PHP/Node versions, Composer, extensions | Catalog, checked downloads, per-line runtime storage, PHP settings and project-aware shims. Local archive fixtures cover install/update/remove and PECL dependencies. | Complete platform catalogs; real runtime and extension compatibility; recovery after interruption between filesystem and registry updates. |
| Sites, parked folders and manifests | Existing project registry, unique domains, pending service requirements, versioned manifests, parking and environment suggestions. Tests cover these behaviors and state migration. | Fresh-machine onboarding and multiple real projects with different runtime selections; separate Redis endpoint configuration. |
| Domains and HTTPS | Existing Caddy routes, loopback upstreams, hosts helper and CA trust actions. Configuration tests include escaped paths and Vite HTTPS routing. | Real hosts changes, browser trust, occupied ports and elevation on each OS; these were not applied to the user's machine. |
| Vite and process lifecycle | Existing independent Vite state, logs, autostart, Node selection and guarded hot-file cleanup; Windows ownership fixed above. | Starter-kit/configuration compatibility, multiple live sites and real HMR. Queue workers, Horizon, scheduler and Reverb remain separate features. |
| Database/cache/mail/object services | Existing shared service instances and per-site links; PostgreSQL initialization and Redis conflict handling repaired. | Real data persistence and backup/restore, engine-version compatibility, project attribution for shared mail/object services. No live database was started during this review. |
| CLI, desktop and diagnostics | Existing daemon/RPC separation, command interface, UI, doctor and process logs; frontend tests/build and Rust workspace checks. | Richer doctor checks, Laravel log/dump/query inspection, integrated inbox, real desktop/tray/onboarding acceptance. |
| AI tools | Independent read-only `werd mcp`, seven tools, official SDK and authenticated Werd RPC. Fixture integration tests and isolated real-daemon smoke. | Mutating tools and structured application diagnostics; macOS/Linux smoke. No running Herd or LLM service is required. |
| Application updates | Signed Tauri updater remains enabled; corrected upstream dependency compiles with the desktop. | Installing a signed update and recovering interrupted upgrades in disposable packaged environments. No installed application was updated here. |

The [Herd 2 investigation](herd-v2.md) remains the feature reference. Another
broad binary inventory is unnecessary for these source-level fixes. Further
Herd investigation should answer a specific unresolved question, especially
Insights instrumentation/event semantics or workspace/database cloning, when
that subsystem is being designed. Do not infer implementation completeness from
an IPC name or documentation entry.

## Verification and next work

Validated on Windows x64:

- `cargo test --offline --locked --workspace`: 122 tests passed.
- `cargo clippy --offline --locked --workspace --all-targets -- -D warnings`:
  passed with the installed Windows SDK resource compiler on PATH.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo deny --offline --format json check`: passed.
- `npm audit`: zero known vulnerabilities.
- `npm test`: 22 tests passed; `npm run check` and `npm run build`: passed.
- `node --test scripts/release/*.test.mjs`: 7 release tests passed.
- Rebuilt CLI/daemon and official TypeScript MCP client 2.3.1: all seven tools
  succeeded against a real isolated daemon; no runtime downloads or fixture
  environment changes; secret sentinel checks passed.

All test state is temporary; the user's installed Herd, Werd, projects and
services are untouched. macOS/Linux runtime behavior requires execution on
those systems. Compilation and unit tests do not establish signed installer
upgrade behavior or a complete real Laravel development loop.

Follow the [official roadmap](../../README.md#official-roadmap): first validate
the real Windows development loop and build backup/restore with an isolated
restore test, then extend process supervision to named Laravel workers. Preserve
the existing implementation while closing these gaps. Herd-specific deeper
analysis should support those concrete milestones, rather than delay reliability
work in Werd.

### Follow-up — 2026-10-18

The [database backup increment](../database-backups.md) implements local logical
SQL snapshots and restore into a new database through the daemon, CLI and GUI.
Real Windows PostgreSQL/MySQL/MariaDB restore checks and a Laravel HTTPS request
against restored PostgreSQL data now pass. The detailed report records versions,
reproduction scripts, failure cases and remaining platform/upgrade/export gates;
this follow-up does not close the entire daily-workflow or multi-platform review.
