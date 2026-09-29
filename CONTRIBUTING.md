# Contributing to Werd

Thanks for helping! Werd is an open-source local development environment for Laravel/PHP on Windows, macOS and Linux.

## Project layout

| Path | What lives there |
| --- | --- |
| `crates/werd-core` | The daemon (`werd-daemon`): runtimes, services, project supervision and the local RPC API. |
| `crates/werd-cli` | The `werd` command line client. |
| `src-tauri` | The Tauri desktop shell. It only forwards calls to the daemon. |
| `src` | The React + TypeScript UI. |
| `scripts` | Packaging, version bump and metrics scripts. |
| `docs` | Product scope, architecture and runtime provenance. |

The GUI and the CLI never contain business logic: both talk to the same daemon.

## Setup

Prerequisites: Node.js 22+, Rust stable (installed via [rustup](https://rustup.rs)), the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
npm install            # also installs the git hooks (lefthook)
cargo build --workspace # the daemon, CLI and shims; `tauri dev` builds only the app
npm run tauri dev      # desktop app with hot reload
npm run dev            # UI only in the browser; open http://127.0.0.1:1420/?demo for sample data
```

Optional tools used by CI:

```bash
cargo install cargo-deny cargo-llvm-cov git-cliff
```

## Checks

| Task | Command |
| --- | --- |
| Format everything | `npm run format` and `cargo fmt --all` |
| Lint | `npm run check` and `cargo clippy --workspace --all-targets -- -D warnings` |
| Type check | `npm run typecheck` |
| Tests | `npm test` and `cargo test --workspace` |
| Coverage | `npm run coverage` and `cargo llvm-cov --workspace` |
| Dependency audit | `cargo deny check` |
| Metrics report | `npm run metrics -- --coverage` (after `npm run build` and `cargo build --release`) |

The pre-commit hook runs Biome, `tsc` and `rustfmt` on staged files.

To save CI minutes, pull requests are checked on Linux only and pushes to `main` on Linux and Windows. macOS and the metrics report run when the CI workflow is started by hand (**Actions → CI → Run workflow**). Do that before a release.

## Commits

We use [Conventional Commits](https://www.conventionalcommits.org/). The commit-msg hook validates them.

```text
<type>(<optional scope>): <summary in imperative mood>
```

- **Types:** `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `style`, `chore`, `revert`.
- **Scopes:** `core`, `cli`, `desktop`, `ui`, `runtimes`, `services`, `proxy`, `i18n`, `ci`, `docs`, `deps`, `deps-dev`, `release`.
- **Granularity:** keep commits small and focused. A formatting-only change goes in its own `style:` commit.

`CHANGELOG.md` is generated from commit messages with `git cliff`.

## Releases

Follow the [release policy](docs/releases.md) and the README's [official roadmap](README.md#official-roadmap). The next release line starts at `0.1.0-alpha.1`; alpha, beta and RC are prereleases. Application installers belong in `werd`, custom runtime builds in `werd-runtimes`.

1. `npm run version:bump -- X.Y.Z-alpha.N` (or the next stage/version) updates every manifest.
2. `node scripts/release/plan.mjs --check` and `node --test scripts/release/*.test.mjs` check version consistency and publication guards.
3. `git cliff --tag vX.Y.Z-alpha.N -o CHANGELOG.md` regenerates the application changelog; runtime tags are ignored.
4. Commit as `chore(release): vX.Y.Z-alpha.N`, then create an annotated application tag on that commit and push it. Manual workflow retries require the existing tag.

The release stays a draft until all selected installers and signed updater entries pass validation. Stable builds require all four initial OS/architecture targets; successful builds do not replace the manual acceptance gates.

## Adding a runtime

Runtimes are downloaded on demand and verified with SHA-256. Add the provider in `scripts/catalog/providers.mjs`, regenerate `catalog/catalog.json`, and implement any runtime-specific behavior in `crates/werd-core/src/runtimes.rs`. Document its source URL, checksum and license in `docs/runtime-sources.md`. Publish custom binaries in `werd-runtimes`, never as application releases.
