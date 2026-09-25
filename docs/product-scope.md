# Product scope

## Functional reference

The goal is to cover every publicly documented feature of Laravel Herd for Windows and macOS, Pro features included, with an independent implementation. Werd also targets Linux. Features and their priority are validated with real use cases, not by visual similarity alone.

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
| Interfaces | A complete desktop app with dashboard, project details, service catalog, mail, logs and settings. The CLI exposes the same operations for automation. |

Integrations with external services, such as Forge or public sharing tools, need a separate review of the available APIs. They must not block parity on local features.

## First complete use case

A Laravel project on Windows and one on macOS declare PHP, PostgreSQL with pgvector, Redis, Mailpit and RustFS. The user can do the whole flow from the GUI:

1. Link the repository without editing system configuration by hand.
2. Start the site and its services with one action.
3. Reach the site over local HTTPS.
4. See status, logs, mail and connection details from the GUI or the CLI.
5. Stop the project without stopping services other projects still use.
6. Resume work without losing databases or stored objects.

The same essential operations are available in the CLI. The first usable release includes both interfaces.

This use case is a first milestone, not the limit of the product: parity with the features listed above remains the goal.

## Technical decisions to validate with prototypes

- Distribution of PHP and its extensions on every platform.
- DNS, privileged ports and local certificates without repeated administrator prompts.
- A reliable match between PostgreSQL and pgvector versions on Windows, macOS and Linux.
- Format and compatibility rules of the project configuration.
- Update strategy and data migration for services.
- An optional container engine, and its memory cost while running.

## Sources

- [Laravel Herd for Windows](https://herd.laravel.com/windows)
- [Laravel Herd for macOS](https://herd.laravel.com/)
- [pgvector](https://github.com/pgvector/pgvector)
- [RustFS](https://docs.rustfs.com/en/installation)
