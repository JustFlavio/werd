# Werd

[![CI](https://github.com/JustFlavio/werd/actions/workflows/ci.yml/badge.svg)](https://github.com/JustFlavio/werd/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Werd is an open-source local development environment for Laravel and PHP, in the spirit of [Laravel Herd](https://herd.laravel.com), [Lerd](https://github.com/lerd-env/lerd) and [Yerd](https://yerd.app). It targets **Windows, macOS and Linux** and runs native processes, with no Docker or WSL. Everything is free, including the parts that other tools keep behind a paid tier: databases, mail capture and dumps.

A desktop app and a `werd` CLI share one Rust daemon. Sites run their own PHP FastCGI behind one shared Caddy, on `https://<name>.test`, and link to shared services you create once: PostgreSQL (with optional pgvector), MySQL, MariaDB, MongoDB, Redis, Mailpit, RustFS (S3) and Meilisearch.

> **Status: pre-beta.** Windows works end to end. macOS and Linux are next. See the [roadmap](#roadmap).

## What works today (Windows x64)

- Install, update and remove versions from a runtime catalog generated weekly from official sources, with SHA-256 checks:
  - PHP 7.4–8.5;
  - Node.js 16–26;
  - PostgreSQL 14–18 (pgvector optional), MySQL 8.0/8.4/9.x, MariaDB 10.11–13, MongoDB 7–8, Redis 7.2–8.x, Meilisearch;
  - Composer, Mailpit, RustFS and Caddy.
- When a newer patch ships, **Update** replaces the installed one (e.g. PHP 8.4.25 → 8.4.26).
- **Services** like Herd's: add PostgreSQL 17, MySQL 8.4 or Redis once, choose name, port and autostart, and share them between sites. Each site gets its own database inside them.
- **Add a site like in Herd:** link an existing project (Werd reads composer.json and proposes a PHP version that fits), or create a new one with the official Laravel installer and a starter kit (React, Vue, Svelte, Livewire or a community kit). From the site page, open it in a terminal, Tinker, VS Code, Cursor or PhpStorm.
- **Sites** choose their PHP line and Node version and link a database, cache, queue, mail, storage and search service. An optional [`werd.yml`](docs/werd-yml.md) declares all of this for your team; missing services are created with one click.
- `php`, `composer`, `node`, `npm` and `npx` on your PATH pick the version of the folder you are in (`werd.yml`, `.nvmrc`, the site settings), or your default.
- Start a site with one action on `https://<name>.test`, with a local CA you can trust from the app. Werd keeps the domains in the hosts file (Windows asks for administrator approval) and every site also keeps a stable `https://localhost:<port>` address. If another program uses port 443, choose another HTTPS port in **General**.
- Park a folder and every Laravel project inside it becomes a site, like Herd's paths. New projects appear on their own; deleted ones disappear.
- Werd lives in the tray and can start at login, so services marked to start automatically are ready when you are. If Werd crashes, its PHP, Caddy and service processes stop with it instead of keeping ports busy.
- See the `.env` values to paste into your project. Werd never edits your files.
- Read site and service logs, open the Mailpit inbox, the RustFS console and Meilisearch, see service credentials.
- Data survives restarts; ports stay stable, so your `.env` keeps working. Sites and services from Werd 0.1 are migrated automatically.

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

1. Install PHP from the **PHP** page (or `werd php install 8.4`).
2. Add the services you need in **Services → Add service** (or `werd service add postgresql@18 --with pgvector`). Building pgvector currently needs Visual Studio Build Tools.
3. Add the folder in **Sites → Add site** (or `werd add`), link its services, start it, copy the `.env` values from the site page and run your migrations.
4. To avoid browser warnings, trust the local CA in **General → HTTPS certificate** (or `werd trust-ca`).

Some Laravel packages you will need:
- **Redis:** install `predis/predis` and set `REDIS_CLIENT=predis`. The bundled PHP has no phpredis extension yet.
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
werd logs <project> [source]    werd | php | caddy
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

## Roadmap

1. **macOS and Linux:** runtime catalogs, tar archives, CA trust and PATH integration on each OS.
2. **Public repository:** online catalog updates, prebuilt pgvector (no compiler needed), signed releases.
3. **Services:** database backup and restore, Typesense and Reverb, separate Redis connections for cache and queue.
4. **Debugging:** `dump()` viewer, built-in mail viewer, Laravel log viewer, Xdebug toggle.
5. **Desktop polish:** auto-update, light theme, onboarding.
6. **Sharing:** public URLs and LAN access.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md): project layout, checks, Conventional Commits and releases. Security issues: [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Downloaded runtimes keep their own licenses, listed in [docs/runtime-sources.md](docs/runtime-sources.md).
