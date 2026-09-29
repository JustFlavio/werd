# Werd

[![CI](https://github.com/JustFlavio/werd/actions/workflows/ci.yml/badge.svg)](https://github.com/JustFlavio/werd/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Werd is a free, MIT-licensed local development environment for Laravel and PHP. It targets **Windows, macOS and Linux**, runs native processes without Docker or WSL, and aims to provide a complete alternative to [Laravel Herd](https://herd.laravel.com), including the local development tools offered by Herd Pro. Existing features and future work are listed separately below; Werd does not yet provide full Herd feature parity.

A desktop app and a `werd` CLI share one Rust daemon. Sites run their own PHP FastCGI behind one shared Caddy, on `https://<name>.test`, and link to shared services you create once: PostgreSQL (with optional pgvector), MySQL, MariaDB, MongoDB, Redis, Mailpit, RustFS (S3) and Meilisearch.

> **Status: `0.1.0-alpha.2`, available for Windows x64.** This is a new project with no public stable release. Windows x64 is the currently validated alpha platform; macOS support is partial and Linux still needs runtime coverage and validation. The first stable release will be gated by the [official roadmap](#official-roadmap), not by a date.

## Download

**[Download Werd 0.1.0-alpha.2 for Windows x64](https://github.com/JustFlavio/werd/releases/download/v0.1.0-alpha.2/Werd_0.1.0-alpha.2_x64-setup.exe)**, or read the [release notes and known limitations](https://github.com/JustFlavio/werd/releases/tag/v0.1.0-alpha.2). The alpha uses the signed preview update channel. macOS and Linux installers are not available yet; contributors can use the [development setup](#getting-started-development).

Application versions follow one sequence across operating systems: `0.1.0-alpha.N` → `0.1.0-beta.N` → `0.1.0-rc.N` → `0.1.0`. Alpha, beta and RC builds are GitHub prereleases. Platform names belong in asset names, not in application versions. Custom runtime binaries live separately in [werd-runtimes](https://github.com/JustFlavio/werd-runtimes), under tags such as `php-8.5.11`. See the [release policy](docs/releases.md) for publication and update channels.

## What works in the current source (Windows x64)

- Install, update and remove versions from a runtime catalog generated weekly from official sources, with SHA-256 checks:
  - PHP 7.4–8.5 with the matching phpredis and MongoDB extensions on Windows;
  - Node.js 16–26;
  - PostgreSQL 14–18 (pgvector optional), MySQL 8.0/8.4/9.x, MariaDB 10.11–13, MongoDB 7–8, Redis 7.2–8.x, Meilisearch;
  - Composer, Mailpit, RustFS and Caddy.
- When a newer patch ships, **Update** replaces the installed one (e.g. PHP 8.4.25 → 8.4.26).
- **Services** like Herd's: add PostgreSQL 17, MySQL 8.4 or Redis once, choose name, port and autostart, and share them between sites. Each site gets its own database inside them.
- **Add a site like in Herd:** link an existing project (Werd reads composer.json and proposes a PHP version that fits), or create a new one with the official Laravel installer and a starter kit (React, Vue, Svelte, Livewire or a community kit). From the site page, open it in a terminal, Tinker, VS Code, Cursor or PhpStorm.
- **Sites** choose their PHP line and Node version and link a database, cache, queue, mail, storage and search service. An optional [`werd.yml`](docs/werd-yml.md) declares all of this for your team; missing services are created with one click.
- `php`, `composer`, `node`, `npm` and `npx` on your PATH pick the version of the folder you are in (`werd.yml`, `.nvmrc`, the site settings), or your default.
- Start a site with one action on `https://<name>.test`, with a local CA you can trust from the app. Werd keeps the domains in the hosts file (Windows asks for administrator approval) and every site also keeps a stable `https://localhost:<port>` address. If another program uses port 443, choose another HTTPS port in **General**.
- Manage Vite per site from **Sites → General → Vite**: start, stop, view logs and optionally start it with the site. Assets and hot reload use HTTPS through Caddy. Stopping the site also stops Vite; frontend errors leave PHP available.
- Park a folder and every Laravel project inside it becomes a site, like Herd's paths. New projects appear on their own; deleted ones disappear.
- Werd lives in the tray and can start at login, so services marked to start automatically are ready when you are. If Werd crashes, its PHP, Caddy and service processes stop with it instead of keeping ports busy.
- See the `.env` values to paste into your project. Updating `APP_URL` in the site wizard is an explicit option; Werd does not silently rewrite service connection settings.
- Read site and service logs, open the Mailpit inbox, the RustFS console and Meilisearch, see service credentials.
- Data survives restarts; ports stay stable, so your `.env` keeps working.

## Herd vs Werd

This comparison describes documented Herd features and Werd's current source as of **2026-09-30**. **Current** means implemented, primarily exercised on Windows x64; **planned** means it is not available yet. A checked-in feature is not proof of support on every operating system.

| Development need | Laravel Herd | Werd today | Werd target |
| --- | --- | --- | --- |
| Platforms | Windows and macOS | Windows x64 exercised; macOS partial; Linux pending | The same daily workflow on Windows, macOS and Linux |
| Local `.test` sites and HTTPS | Available | Current: Caddy, local CA, linking and parking | Reliable setup, trust and recovery on every supported OS |
| PHP, Composer and Node versions | Available, including per-site PHP isolation | Current: catalog, project-aware CLI shims, per-site PHP/Node | Tested runtime and extension compatibility matrix |
| Laravel frontend with Vite | Node management documented; managed per-site Vite lifecycle is not documented in the reviewed pages | Current: start/stop, autostart, logs, HTTPS assets and HMR | Validate React, Vue, Svelte and Livewire flows across platforms |
| Databases, cache, search and S3 | Managed services in Pro | Current: PostgreSQL, MySQL, MariaDB, MongoDB, Redis, Meilisearch and RustFS | Backup/restore, safe upgrades, Reverb and additional engines |
| PostgreSQL with pgvector | PostgreSQL service documented; pgvector packaging is not established by the reviewed docs | Current on Windows: optional build requiring Build Tools | Verified matching binaries without a compiler on the user's machine |
| Project environment shared with a team | `herd.yml` | Current: `werd.yml`, linked services and environment suggestions | Schema validation, compatibility rules and reproducible setup |
| Captured email | Pro mail server and integrated viewer | Current: Mailpit service and browser inbox | Mail inside Werd, grouped by project, with attachments and search |
| `dump()` / `dd()` inspection | Pro dump viewer | Planned | Free structured dump viewer with source links |
| Laravel logs | Integrated structured log viewer | Current: process/service logs; Laravel log viewer planned | Search, severity filters, rotation and stack-trace links |
| Xdebug and profiling | Manual Xdebug in Free; automatic detection in Pro | Planned | Verified Xdebug/IDE setup first; profiling later |
| Queue workers and scheduler | Reverb and service management documented; per-site worker/scheduler supervision is not established by the reviewed docs | Redis service linking exists; Artisan workers and scheduler planned | Named supervised processes, logs, restart and per-site lifecycle |
| Plain PHP and other frameworks | Multiple framework drivers and custom drivers | Laravel-first site detection | Configurable document roots, plain PHP and Symfony validation |
| GUI and automation | Desktop app and CLI, including service commands | Current: shared daemon, desktop UI and CLI with `--json` | Feature parity between GUI/CLI and actionable diagnostics |
| Public sharing | Sharing through Expose | Planned, after the stable local workflow | Explicit opt-in temporary links and LAN access |

Sources: Herd's [Windows installation](https://herd.laravel.com/docs/windows/getting-started/installation), [Node management](https://herd.laravel.com/docs/windows/technology/node-versions), [services](https://herd.laravel.com/docs/windows/herd-pro-services/services), [project manifest](https://herd.laravel.com/docs/windows/sites/herd-yaml), [mail](https://herd.laravel.com/docs/windows/herd-pro-services/mail), [dumps](https://herd.laravel.com/docs/windows/debugging/dumps), [logs](https://herd.laravel.com/docs/windows/debugging/logs), [Xdebug](https://herd.laravel.com/docs/windows/debugging/xdebug), [profiler](https://herd.laravel.com/docs/windows/debugging/profiler), [frameworks](https://herd.laravel.com/docs/windows/extending-herd/supported-frameworks) and [sharing](https://herd.laravel.com/docs/windows/sites/sharing-sites). “Not documented” is a limit of this review, not a claim that Herd cannot do it. Herd already covers many of these needs; Werd's intended additions are Linux support, free access to the whole local toolset and supervision of the frontend and background work alongside PHP.

## Getting started (development)

Prerequisites:
- Node.js 22+ and Rust stable ([rustup](https://rustup.rs));
- the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS;
- on Windows, Visual Studio 2022 Build Tools with MSVC.

```bash
npm install              # dependencies + git hooks
cargo build -p werd-core -p werd-cli
npm run tauri dev        # desktop app with hot reload
```

UI-only work does not need Rust: `npm run dev` and open `http://127.0.0.1:1420/?demo` for sample data.

Werd keeps its data in `%LOCALAPPDATA%\Werd` on Windows and the equivalent local data directory elsewhere. Set `WERD_HOME` to use an isolated folder for experiments.

## First project

On its first start Werd downloads Caddy, the newest **stable** PHP available for the platform (made the global version) and Composer in the background; the Dashboard shows the progress. PHP alpha/beta/RC builds remain available for explicit installation. Internet is needed only to download versions and to create new projects; sites and services run offline.

On Windows, Werd installs phpredis and the MongoDB PHP driver with each new PHP version and enables them in `php.ini`, so Laravel can use `REDIS_CLIENT=phpredis`.

1. Install other PHP versions from the **PHP** page if you need them (or `werd php install 8.4`).
2. Add the services you need in **Services → Add service** (or `werd service add postgresql@18 --with pgvector`). Building pgvector currently needs Visual Studio Build Tools.
3. Add the folder in **Sites → Add site** (or `werd add`), link its services, start it, copy the `.env` values from the site page and run your migrations.
4. To avoid browser warnings, trust the local CA in **General → HTTPS certificate** (or `werd trust-ca`).

For Vite projects, install a compatible Node version from **Node** (or `werd node install 22`), select it for the site and install the project's npm dependencies before starting managed Vite.

Some Laravel packages you will need:
- **MongoDB:** install `mongodb/laravel-mongodb` to use a MongoDB connection from Laravel.
- **S3:** install `league/flysystem-aws-s3-v3` and create the `AWS_BUCKET` bucket once.

## CLI

```text
werd list                       linked projects and their status
werd new <name> [--kit react|vue|svelte|livewire] [--git] [--npm]   create a Laravel project
werd add [folder]               link a Laravel project (default: current folder)
werd up <project>               start (alias: start)
werd down <project>             stop (alias: stop)
werd open <project>             open in the browser
werd env <project>              print .env values
werd logs <project> [source]    werd | php | caddy | vite
werd vite <project> [start|stop|status] [--autostart true|false]
werd logs <project> vite        frontend dev server output
werd reset-ports <project>      pick new ports on next start
werd info <project>             versions, linked services, what is missing
werd set <project> --php 8.4 --node 22 --domain shop
werd link <project> <category> <service>   e.g. werd link shop database "PostgreSQL 18"
werd unlink <project> <category>
werd resolve <project>          create the services werd.yml asks for
werd remove <project>           forget a site (folder untouched)
werd park|unpark [folder]       every Laravel project in the folder becomes a site
werd parked                     list parked folders
werd service [list|available|add|start|stop|info|logs|db|rm]
werd php [list|install|update|uninstall|use|limits]
werd node [list|install|update|uninstall|use]
werd runtimes                   everything installable on this platform
werd install|uninstall <product>@<line>   e.g. postgresql@17, redis@8.2
werd update [<product>@<line>]  update one line, or everything outdated
werd domains [sync|enable|disable|port <n>]   .test domains and the hosts file
werd path enable|disable        put php, composer, node, npm, npx on PATH
werd trust-ca | doctor          certificate and diagnostics
werd completions <shell>        shell completion script
```

Projects can be referred to by name, domain or id. Add `--json` to any command for machine-readable output.

Vite management uses the site's installed Vite and `vite.config.*`, with the Node version selected by `werd.yml`, `.nvmrc` / `.node-version`, the site setting or your global default. Run `npm install` in the project first. Werd runs Vite directly, so custom npm `dev` scripts and their hooks are not executed. The standard Laravel `public/hot` location is required; projects with a custom hot-file location can keep using their own dev command. Werd overrides only the dev server's network settings in memory and uses separate ports for each site. It removes its own hot file when stopping or recovering from a daemon crash, and refuses to overwrite an existing hot file from another dev server. With Vite stopped, run `npm run build` to keep the frontend available at the site's URL using the last built assets.

## Architecture

| Path | Role |
| --- | --- |
| `crates/werd-core` | Daemon: runtimes, services, supervision, local RPC (loopback TCP + per-session token) |
| `crates/werd-cli` | `werd` CLI |
| `crates/werd-shim` | `php`, `composer`, `node`, `npm` and `npx` launchers |
| `crates/werd-helper` | Elevated helper that writes only Werd's block of the hosts file |
| `src-tauri` | Tauri 2 desktop shell. It only forwards calls to the daemon. |
| `src` | React + TypeScript UI with English and Italian translations |

Closing the window does not stop your sites: the daemon owns the processes. More details are in [docs/desktop-architecture.md](docs/desktop-architecture.md), [docs/product-scope.md](docs/product-scope.md) and [docs/runtime-sources.md](docs/runtime-sources.md).

## Official roadmap

This is the authoritative milestone plan. Unchecked items are commitments to investigate and implement, not shipped features. Milestones advance when their exit criteria are met; there are no promised release dates. The first stable version is **`0.1.0`**. Broader Herd feature parity remains the longer-term goal for **`1.0.0`**.

### 1. Alpha — finish the daily development loop

The goal is to open a project, start its environment and work without keeping several terminals alive.

- [x] Native PHP sites, local HTTPS, multiple runtimes, shared services, GUI/CLI and `werd.yml`.
- [x] Managed Vite with per-site autostart, HTTPS/HMR, logs and independent failure handling in the current source.
- [ ] Validate Vite against the supported Laravel starter kits, multiple simultaneous sites, built assets and project configuration variations; document unsupported cases.
- [ ] Supervise `queue:work`, optional project-installed Horizon, `schedule:work` and Reverb. Provide named processes, status, logs and explicit restart after code changes; distinguish linking Redis from running a worker.
- [ ] Add separate cache/queue service links and reproducible environment suggestions; validate `werd.yml` with useful errors and a documented schema version.
- [ ] Add a Laravel log viewer, structured `dump()`/`dd()` inspection and a Mailpit-backed inbox inside Werd with project attribution.
- [ ] Package and configure Xdebug where the PHP build supports it; verify PhpStorm/VS Code connections. Resolve the current static macOS PHP extension limitation before promising debugger support there.
- [ ] Strengthen `werd doctor`: runtime/extension requirements from Composer, Node compatibility, occupied ports, CA trust, database connectivity and stale Vite hot files, with concrete remedies and secret redaction.
- [ ] Add database backup/restore and export/import, test restoration into an isolated instance, and protect data before runtime changes. Service upgrades must respect engine compatibility instead of assuming every new binary accepts old data.
- [ ] Distribute verified pgvector builds matched to supported PostgreSQL binaries, with checksums and third-party notices.

**Exit criterion:** a clean Windows x64 machine can run a Laravel/Inertia project with Vite, a database, queue, mail and S3; inspect a failure; stop/restart; and restore a backup through the GUI and the equivalent CLI operations. No orphan processes, lost data or manual edits to system configuration are required.

### 2. Beta — deliver the same workflow on every target platform

- [ ] Complete the runtime catalogs and service supervision for macOS arm64/x64 and Linux x64. Publish explicit PHP/extension/service availability instead of silently falling back to a different version.
- [ ] Verify `.test` routing, local certificate trust, PATH/shims, login startup, tray behavior and privilege handling on each OS, including paths with spaces and non-ASCII characters.
- [ ] Exercise fresh install, offline restart after downloads, port collisions, suspend/resume, process crashes and uninstall with data preserved on real systems.
- [ ] Support configurable document roots and validate plain PHP and Symfony alongside Laravel. Keep Laravel conveniences optional for other projects.
- [ ] Refine onboarding and recovery, keyboard navigation, light/dark themes, translations and clear process status; measure startup time and idle resource use.
- [ ] Verify native installers, Tauri update signatures, platform signing/notarization requirements, preview updates and rollback/recovery procedures.

**Initial stable platform targets:** Windows x64; macOS Apple Silicon and Intel; Linux x64 on Ubuntu 22.04 and 24.04. These are validation targets, not current support claims. Minimum Windows/macOS versions and Linux dependencies must be recorded before beta. Other Linux distributions and ARM64 Linux follow only after their own checks.

**Exit criterion:** the alpha use case passes on each target OS/architecture with saved validation results. A successful cross-platform compilation alone does not qualify.

### 3. Release candidate — freeze the scope and prove reliability

- [ ] Freeze features; fix defects and finish documentation, accessibility and error messages.
- [ ] Run the complete acceptance flow on clean machines for all declared platforms, including two projects using different PHP/Node versions and shared services.
- [ ] Validate backups, data/schema migrations, runtime upgrades, signed app updates and recovery after interrupted downloads or crashes.
- [ ] Audit runtime provenance, checksums, third-party licenses and support boundaries; publish a known-issues list with no unresolved blocking defects.
- [ ] Build all installers from the same application tag. Keep the release draft until every required installer and updater entry has been validated.

**Exit criterion:** every stable gate below passes for the same RC version. Any blocking defect requires another RC.

### 4. `0.1.0` — first stable multiplatform release

Release only when all of these conditions are met:

1. The documented daily workflow works on every declared stable platform, through both GUI and CLI.
2. Sites, Vite, workers and services have predictable lifecycle, usable diagnostics and no leftover processes after shutdown or crashes.
3. Databases and stored objects survive restarts; supported upgrades have documented compatibility and a tested backup/restore path.
4. All intended installers and signed updater artifacts exist for one version; preview and stable channels are separate.
5. Installation, first project, environment configuration, troubleshooting, update and uninstall are documented and exercised from a clean machine.

### Beyond `0.1.0` — deepen the developer toolbox toward `1.0.0`

These improvements do not block the first stable local environment:

- A project task runner for Composer/Artisan commands, migrations/seeders and Pest/PHPUnit, with saved commands and readable results; integrate existing Laravel tooling rather than duplicate it.
- Request diagnostics and profiling, with optional integrations for Telescope, Pulse and existing IDE tools; finish the remaining documented Herd local feature gaps.
- Additional engines such as Typesense and Valkey, database clone/snapshots and a better seed-data workflow; keep the core service set dependable first.
- Optional public sharing and LAN access, with clear lifetime and exposure controls; evaluate Forge and other external integrations independently.
- More Linux distributions/architectures, custom framework drivers and project process definitions beyond Laravel.

Priorities are driven by reproducible developer problems. New scope must state its use case, supported platforms and acceptance criteria before becoming a milestone requirement. Detailed implementation boundaries are in [product scope](docs/product-scope.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md): project layout, checks, Conventional Commits and releases. Security issues: [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Downloaded runtimes keep their own licenses, listed in [docs/runtime-sources.md](docs/runtime-sources.md).
