# Werd

[![CI](https://github.com/JustFlavio/werd/actions/workflows/ci.yml/badge.svg)](https://github.com/JustFlavio/werd/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Werd is an open-source local development environment for Laravel and PHP, in the spirit of [Laravel Herd](https://herd.laravel.com), [Lerd](https://github.com/lerd-env/lerd) and [Yerd](https://yerd.app). It targets **Windows, macOS and Linux** and runs native processes, with no Docker or WSL. Everything is free, including the parts that other tools keep behind a paid tier: databases, mail capture and dumps.

A desktop app and a `werd` CLI share one Rust daemon. Each site gets its own PHP FastCGI, a Caddy HTTPS endpoint, and the services it declares in `werd.yml`: PostgreSQL with pgvector, Redis, Mailpit and RustFS (S3-compatible).

> **Status: pre-beta.** The Windows prototype works end to end. macOS and Linux runtime catalogs, `.test` domains, multiple PHP versions and shared services are next. See the [roadmap](#roadmap).

## What works today (Windows x64)

- Link a Laravel folder. Werd creates a `werd.yml` with the default stack if the folder has none.
- Download runtimes on demand. Versions are pinned and every archive is verified with SHA-256 before extraction.
  - PHP 8.5, Caddy, PostgreSQL 18 + pgvector, Redis 7.2, Mailpit, RustFS.
- Start a site with one action: `https://localhost:<port>` with a local CA you can trust from the app.
- See the `.env` values to paste into your project. Werd never edits your files.
- Read per-service logs, open the Mailpit inbox and the RustFS console, reassign ports after a conflict.
- Data survives restarts. Ports stay stable per project, so your `.env` keeps working.

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

1. Add the folder in **Sites → Add site**, or run `werd add` from the project folder.
2. Install the runtimes from the **PHP** page, or with `werd install <runtime>`. Building pgvector currently needs Visual Studio Build Tools; `scripts/build-pgvector-windows.ps1` does the same build by hand.
3. Start the site. Copy the `.env` values from the site page, then run your migrations.
4. To avoid browser warnings, trust the local CA in **General → HTTPS certificate** (or `werd trust-ca`).

Some Laravel packages you will need:
- **Redis:** install `predis/predis` and set `REDIS_CLIENT=predis`. The bundled PHP has no phpredis extension yet.
- **S3:** install `league/flysystem-aws-s3-v3` and create the `AWS_BUCKET` bucket once.

## CLI

```text
werd list                       linked projects and their status
werd add [folder]               link a Laravel project (default: current folder)
werd up <project>               start (alias: start)
werd down <project>             stop (alias: stop)
werd open <project>             open in the browser
werd env <project>              print .env values
werd logs <project> [source]    werd | php | caddy | postgres | redis | mailpit | rustfs
werd reset-ports <project>      pick new ports on next start
werd runtimes | install <id>    runtime catalog
werd trust-ca | doctor          certificate and diagnostics
werd completions <shell>        shell completion script
```

Projects can be referred to by name or id. Add `--json` to any command for machine-readable output.

## Architecture

| Path | Role |
| --- | --- |
| `crates/werd-core` | Daemon: runtimes, services, supervision, local RPC (loopback TCP + per-session token) |
| `crates/werd-cli` | `werd` CLI |
| `src-tauri` | Tauri 2 desktop shell. It only forwards calls to the daemon. |
| `src` | React + TypeScript UI with English and Italian translations |

Closing the window does not stop your sites: the daemon owns the processes. More details are in [docs/desktop-architecture.md](docs/desktop-architecture.md), [docs/product-scope.md](docs/product-scope.md) and [docs/runtime-sources.md](docs/runtime-sources.md).

## Roadmap

1. **Runtime catalogs for macOS and Linux**, several PHP versions, and a prebuilt pgvector so no compiler is needed.
2. **Sites and domains:** `https://name.test` through a shared proxy, per-site PHP version, parked folders. A small privileged helper manages the hosts file.
3. **Shared services** with one database per site, plus MySQL/MariaDB, Meilisearch and Typesense. Database backup and restore.
4. **Developer tools:** Composer, Laravel installer, Node versions, and per-folder `php`/`composer`/`node` shims.
5. **Debugging:** `dump()` viewer, built-in mail viewer, Laravel log viewer, Xdebug toggle.
6. **Desktop polish:** tray, launch at login, auto-update, light theme, onboarding.
7. **Sharing:** public URLs and LAN access.
8. **Signed releases** for all three platforms.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md): project layout, checks, Conventional Commits and releases. Security issues: [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Downloaded runtimes keep their own licenses, listed in [docs/runtime-sources.md](docs/runtime-sources.md).
