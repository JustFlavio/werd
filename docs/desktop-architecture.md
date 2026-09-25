# Desktop app and architecture

## Stack

The desktop shell is **Tauri 2** with a TypeScript/React frontend and a Rust backend. Tauri uses the system WebView: WebView2 on Windows, WebKit on macOS and WebKitGTK on Linux. This avoids shipping a copy of Chromium. It does not remove the cost of the services the user starts. Electron remains an alternative if the prototype shows concrete compatibility or experience problems.

The service manager (`werd-daemon`) lives in a process separate from the window, so closing the dashboard never stops databases or workers by accident. The GUI and the CLI talk to the same daemon through a local API: loopback TCP with a per-session token stored in the user's data directory. The daemon owns the project state, supervises processes, collects logs and runs health checks. Privileged operations, such as trusting a local CA or editing the hosts file, are isolated and only requested when needed.

## Code layout

| Crate / folder | Content |
| --- | --- |
| `crates/werd-core` | `model` (shared types), `manifest` (`werd.yml`), `state`, `projects` (lifecycle), `services/*` (one module per service behind a `Service` trait), `proxy` (PHP FastCGI + Caddy), `runtimes` (download catalog), `platform` (OS integration), `rpc` and `daemon`. |
| `crates/werd-cli` | The `werd` command, built with clap. |
| `src-tauri` | Tauri commands that forward to the daemon. |
| `src` | UI pages, shared components and i18n dictionaries. |

## Required screens

| Screen | Main actions |
| --- | --- |
| Dashboard | See active projects, shared services, busy ports and problems to fix; start or stop a project. |
| Sites | Add a folder, detect Laravel, choose PHP and domain, open the site and a terminal. |
| Site details | Choose dependencies, start or stop worker/scheduler/Reverb, read the suggested `.env` and service status. |
| Services | Install versions, configure and control PostgreSQL/pgvector, MySQL/MariaDB, Redis, Mailpit, RustFS and the other supported services. |
| Databases and storage | Create databases, users and buckets; back up and restore. |
| Diagnostics | Read logs, mail and dumps; see errors, ports, processes and repair actions. |
| Settings | Manage PHP, Node, certificates, paths, launch at login, updates and language. |

The GUI must always show the daemon's real state, not only the last command sent. Errors and long operations need progress and a visible way to recover.

## First usable release

The minimal complete flow covers:
- onboarding;
- adding a Laravel project and choosing its PHP version;
- an HTTPS domain, and starting and stopping the project;
- PostgreSQL with pgvector, Redis, Mailpit and RustFS;
- status and logs.

Everything is available from the desktop app. The CLI is built against the same API, with no separate logic.

## Checks before confirming the stack

- Startup time and idle resource usage of the GUI on every platform. The daemon baseline is tracked by `npm run metrics`.
- Windows, tray, notifications and updates on every platform.
- Safe execution and supervision of processes while the window is closed.
- Distribution, signing and updating of the installers.

References: [Tauri](https://tauri.app/start/), [Tauri prerequisites](https://tauri.app/start/prerequisites/), [Electron](https://www.electronjs.org/docs/latest).
