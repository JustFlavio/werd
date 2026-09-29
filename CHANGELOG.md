# Changelog

All notable changes to Werd are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Pre-launch experiment tags were retired before the first alpha. This history keeps
the source changes, including earlier distribution experiments. See the
[release policy](docs/releases.md) for the current version and channel rules.

## [0.1.0-alpha.2] - 2026-09-29

### Bug fixes
- **core:** Select stable PHP during first-run setup

## [0.1.0-alpha.1] - 2026-09-29

### Features
- **core:** Add project manager daemon and runtime catalog
- **cli:** Add werd command line interface
- **ui:** Add project management interface
- **desktop:** Add tauri shell with daemon sidecar
- **desktop:** Add open_url command for local service UIs
- **ui:** Redesign interface with sidebar layout and Werd identity
- **cli:** Rewrite the CLI with clap
- **i18n:** Add typed translation dictionaries with English and Italian
- **ui:** Translate the interface and add a language setting
- **runtimes:** Load the runtime catalog in the daemon
- **runtimes:** Manage runtimes per version line from the catalog
- **cli:** Add php, node, install, update and uninstall commands
- **cli:** Add php, composer, node, npm and npx shims
- **core:** Add opt-in PATH integration for the command line shims
- **ui:** Manage PHP and Node versions from the app
- **cli:** Add werd path enable and disable
- **services:** Add shared service instances
- **cli:** Add werd service commands
- **ui:** Manage service instances from the Services page
- **core:** Link sites to shared service instances
- **cli:** Let the shims use the versions chosen for a site
- **cli:** Add site info, set, link, unlink, resolve and remove
- **ui:** Configure site versions and linked services
- **core:** Manage .test domains in the hosts file through an elevated helper
- **proxy:** Serve every site from one shared Caddy with .test domains
- **cli:** Add werd domains and werd set --domain
- **ui:** Show and edit .test domains and keep the hosts file up to date
- **desktop:** Add a tray icon, launch at login and a single instance
- **ui:** Add the launch at login setting and translate the tray menu
- **core:** Add parked folders
- **cli:** Add werd park, unpark and parked
- **desktop:** Add a native folder picker
- **ui:** Pick folders with the system dialog and manage parked folders
- **core:** Describe a project from its composer and package files
- **core:** Create new Laravel projects with the official installer
- **cli:** Add werd new and show the project stack in werd info
- **desktop:** Open a site in the file manager, a terminal, Tinker or an editor
- **ui:** Add the client calls for project inspection, creation and site actions
- **ui:** Add sites the Herd way and split the site page into tabs
- **core:** Read php artisan about and run boost:update for a site
- **ui:** Show php artisan about on the Information tab and a success screen
- **services:** Rename service instances
- **desktop:** Switch the global PHP version from the tray
- **ui:** Call the default PHP the global PHP version and explain the PATH
- **ui:** Style select menus to match Werd
- **core:** Set up Caddy, PHP and Composer on the first start
- **ui:** Show the first-run setup on the Dashboard
- **ui:** Show the progress of each service created for a site
- **runtimes:** Install phpredis with PHP on Windows
- **runtimes:** Add MongoDB PHP driver and bundled modules
- **core:** Report whether the local CA is trusted
- **core:** Start sites with Werd and expose the Caddy log
- **ui:** Add Start with Werd to sites and logs to Dashboard services
- **desktop:** Update Werd from werd-releases with signed packages
- **ui:** Add the update button and What's new
- **runtimes:** Install tar.gz and multi-archive builds
- **proxy:** Serve sites with php-fpm on macOS and Linux
- **core:** Support the command line shims on macOS
- **runtimes:** Add macOS PHP and Node.js, and PHP 8.6 RC
- **runtimes:** Add Caddy for macOS
- **core:** Update the hosts file and trust the local CA on macOS
- **runtimes:** Use Werd's own macOS PHP builds for PHP 8.2 to 8.6
- **core:** Manage per-site Vite development servers

### Bug fixes
- **ui:** Resolve accessibility and hook dependency lint findings
- **runtimes:** Correct PostgreSQL 18.6 archive checksum
- **core:** Keep the daemon from holding the CLI's pipes on Windows
- **core:** Allow the Windows-only PATH helpers on other platforms
- **core:** Stop child processes when the daemon exits abnormally
- **release:** Stop the daemon and clean up when Werd is updated or uninstalled
- **desktop:** Use the amber Werd mark for the app, tray and installer icons
- **desktop:** Give the Werd mark SVGs a title
- **core:** Migrate 0.1 services without data as pending requirements
- **release:** Install Werd outside its data folder
- **core:** Put Werd first on the user PATH
- **core:** Download Caddy when the first site starts
- **cli:** Wait for a site that starts after downloading Caddy
- **ui:** Wait for the site to start before opening it
- **core:** Download Caddy only for a site that can start
- **core:** Clear a failed start when a site's services are linked
- **release:** Keep PATH, .test domains and launch at login across updates
- **runtimes:** Backfill every bundled PECL extension and refresh php.ini at start
- **ui:** Show the HTTPS certificate as trusted when it already is
- **cli:** Let other installs run when Werd has no version of a tool
- **ui:** Show Starting Werd until the daemon answers
- **release:** Rename the Linux bundle config so Tauri does not merge it
- **desktop:** Keep the downloaded update until Restart
- **ui:** Hide sidebar labels in the compact layout
- **core:** Check the runtime files of the current platform in doctor
- **core:** Stop the whole process group of a child on macOS and Linux
- **core:** Stop processes left running by a crashed daemon on macOS and Linux

### Refactoring
- **core:** Split the daemon into focused modules
- **desktop:** Delegate open_url to the core allow-list
- **core:** Write process logs to any folder

### Documentation
- Add readme and project documentation
- Add contributing guide, code of conduct and security policy
- Generate changelog from conventional commits with git-cliff
- Translate README and design docs to English
- Update changelog
- List the deps-dev commit scope
- Describe version management and the new CLI commands
- Document werd.yml version 2, services and site commands
- Document .test domains, the tray and the shared Caddy
- Update the changelog
- Document the add site flows and werd new
- Clarify release candidate status and download
- Document macOS runtime sources and the daemon build step
- Define the roadmap to a stable multiplatform release

### Tests
- **ui:** Add vitest setup with first component and helper tests
- **cli:** Build shim test paths for the current platform
- **core:** Give the doctor test a build for the current platform
- **ui:** Only run the tests of this checkout
- **runtimes:** Avoid Unix symlink privileges in Windows fixtures

### Build and CI
- Set up cargo workspace
- **ui:** Set up vite, react and typescript
- Add packaging and helper scripts
- Configure rust toolchain, workspace metadata and lints
- Add frontend dev tooling
- Enforce formatting and conventional commits with git hooks
- **deps:** Audit licenses and advisories with cargo-deny
- Add script to bump the version in every manifest
- Add metrics script for coverage, sizes and daemon footprint
- Add workflow for lint, tests, audit and metrics on three platforms
- Allow the deps-dev scope used by Dependabot
- Scale the platform matrix with the event to save minutes
- **runtimes:** Add runtime catalog generator
- **runtimes:** Add generated runtime catalog
- Refresh the runtime catalog weekly
- Ship werd-shim with the installers and track its size
- **runtimes:** Add MySQL, MariaDB, MongoDB and Meilisearch to the catalog
- **release:** Ship werd-helper with the Windows installer
- Format the JSON files the version script rewrites
- **runtimes:** Add phpredis to the runtime catalog
- **release:** Package every platform with one script
- Publish releases to werd-releases from version tags
- Build static PHP for macOS with static-php-cli
- **release:** Unify app versions and separate runtime distribution

<!-- generated by git-cliff -->
