# Product scope

## Functional reference

The goal is to cover every publicly documented feature of Laravel Herd for Windows and macOS, Pro features included, with an independent implementation. Werd also targets Linux. Features and their priority are validated with real use cases, not by visual similarity alone.

The [README roadmap](../README.md#official-roadmap) is authoritative for milestone order, current status and the acceptance gates for `0.1.0`. Full local feature parity is a longer-term `1.0.0` goal; this document describes scope rather than claiming features are shipped.

| Area | Required behavior |
| --- | --- |
| Sites | Link a project or park a folder; detect the Laravel document root; assign a `.test` domain; open the site in the browser. |
| Web and TLS | Serve many sites at once, route each site to its PHP version, install locally trusted certificates. |
| PHP | Install and update several versions; choose a version per site and for the CLI; manage the needed extensions. |
| Tools | Provide Composer, the Laravel installer and selectable Node.js versions. |
| Services | Install, configure, start, stop and update PostgreSQL, MySQL/MariaDB, Redis, Mailpit, local S3 storage, Typesense, Meilisearch and Reverb. |
| Databases | Create databases and users, keep data, back up and restore; support PostgreSQL with pgvector explicitly. |
| Mail | Capture outgoing mail and show it per project. |
| Diagnostics | Show PHP and Laravel logs, capture `dump()`/`dd()`, manage Xdebug and show process status. |
| Projects | Store a versionable per-project configuration; start and stop only the dependencies it needs; handle port collisions and shared resources. |
| Frontend and background work | Supervise per-site Vite with HTTPS/HMR, queue workers, scheduler and Reverb; expose lifecycle, logs and actionable failures. |
| Interfaces | A complete desktop app with dashboard, project details, service catalog, mail, logs and settings. The CLI exposes the same operations for automation. |

Integrations with external services, such as Forge or public sharing tools, need a separate review of the available APIs. They must not block parity on local features.

## First complete use case

A Laravel/Inertia project on each declared Windows, macOS and Linux target uses PHP, Vite, PostgreSQL with pgvector, Redis, a queue worker, scheduler, Mailpit and RustFS. The user can do the whole flow from the GUI:

1. Link the repository without editing system configuration by hand.
2. Start the site and its services with one action.
3. Reach the site over local HTTPS.
4. See status, logs, mail and connection details from the GUI or the CLI.
5. Stop the project without stopping services other projects still use.
6. Resume work without losing databases or stored objects.
7. Diagnose an application failure, capture a dump, attach an IDE debugger and restore a database backup into an isolated instance.

The same essential operations are available in the CLI. The first usable release includes both interfaces.

This use case is a first milestone, not the limit of the product: parity with the features listed above remains the goal.

## Technical decisions to validate with prototypes

- Distribution of PHP and its extensions on every platform.
- DNS, privileged ports and local certificates without repeated administrator prompts.
- A reliable match between PostgreSQL and pgvector versions on Windows, macOS and Linux.
- Format and compatibility rules of the project configuration.
- Update strategy and data migration for services.
- Any future optional container integration must justify a use case that the native environment cannot cover; it is outside the first stable release.

## Sources

- [Laravel Herd for Windows](https://herd.laravel.com/windows)
- [Laravel Herd for macOS](https://herd.laravel.com/)
- [pgvector](https://github.com/pgvector/pgvector)
- [RustFS](https://docs.rustfs.com/en/installation)
