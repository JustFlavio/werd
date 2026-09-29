# Release policy

The README's [official roadmap](../README.md#official-roadmap) defines the gates for the first stable release. This policy defines how a validated build is versioned and distributed.

## Repositories and versions

- **[werd](https://github.com/JustFlavio/werd):** source, issues, roadmap, changelog and application installers. App tags are `v<SemVer>` and refer to the source used to build every installer in that release.
- **[werd-runtimes](https://github.com/JustFlavio/werd-runtimes):** custom runtime archives and checksums. Tags such as `php-8.5.11` describe PHP, never the Werd application. Runtime build recipes remain in `scripts/php-builds/` in the source repository; the runtime repository owns the scheduled publishing workflow and uses its own `GITHUB_TOKEN`.

The pre-launch experiments using `0.1.1`, `0.3.0-beta.*` and `0.3.0-rc.*` are retired. Their metadata, refs and downloaded assets are archived locally before cleanup; source commits remain in Git history. New development starts at **`0.1.0-alpha.1`**. This is a one-time reset for an unlaunched project, not a policy of rewriting released versions.

| Stage | Meaning | GitHub classification |
| --- | --- | --- |
| `0.1.0-alpha.N` | Features and architecture can still change; platform support can be incomplete | Prerelease |
| `0.1.0-beta.N` | Planned stable workflows are complete; consolidate and validate platforms | Prerelease |
| `0.1.0-rc.N` | Feature freeze; fix defects and validate the release gates | Prerelease |
| `0.1.0` | First stable daily development environment on all declared platforms | Stable |
| `0.1.1`, `0.2.0`, … | Fixes and subsequent improvements under SemVer | Stable or an explicit prerelease |

One application version applies to every OS. Do not create macOS-only application versions or reuse a published tag for different code. Ordinary development builds belong in Actions artifacts, not GitHub Releases. Runtime tags are excluded from `git-cliff`.

## Publication

1. Complete the applicable roadmap gates and run the checks in CONTRIBUTING, including the all-platform CI before beta/RC/stable.
2. Run `npm run version:bump -- 0.1.0-alpha.N` (or the next stage/version). `node scripts/release/plan.mjs --check` verifies package.json, package-lock.json, Cargo.toml, Werd's Cargo.lock entries and Tauri agree.
3. Generate the changelog, review the release notes, commit, create an annotated `v<version>` tag on that commit and push it. A manual retry must select that same existing tag.
4. The release workflow validates the tag/checkout/manifests, prepares one draft and builds the selected platform installers. `RELEASE_PLATFORMS` defaults to `windows` for alpha development. Accepted values: `windows`, `macos` (both architectures), `macos-arm64`, `macos-x64`, `linux`, comma-separated.
5. A stable version is rejected unless Windows x64, macOS arm64/x64 and Linux x64 are all selected. Compilation is only the publication check; human acceptance results are still required by the roadmap.
6. After all builds succeed, verify the updater version, all intended installers, bundle URLs and signature assets. Only then publish the draft. Alpha/beta/RC use `prerelease=true` and `latest=false`; stable releases can become Latest.
7. Advance the appropriate channel manifest. If a build fails, the draft stays unpublished. Retry from the same tag; if the code needs changes, create a new version. Published app releases are immutable.

The app workflow uses the source repository's `GITHUB_TOKEN`; no cross-repository release token is needed. Tauri signing keys remain repository secrets. Tauri updater signatures and OS installer signing/notarization are different requirements; passing one does not prove the other.

## Update channels

The first application publication creates the `updates` branch of `werd` and its `preview.json`; the first stable publication adds `stable.json`. These are static Tauri updater manifests with immutable, versioned GitHub asset URLs and signatures. This avoids channel tags or extra releases and does not require a website.

- **Preview:** alpha, beta and RC installers read `https://raw.githubusercontent.com/JustFlavio/werd/updates/preview.json`.
- **Stable:** stable installers read `https://raw.githubusercontent.com/JustFlavio/werd/updates/stable.json`.

Stable publication advances the stable manifest and graduates preview users to that version unless a newer preview already exists. No channel moves backwards automatically. Once installed, the stable build follows stable updates. A user-facing channel selector is future work.

Do not rely on GitHub's `/releases/latest` for prerelease updates or mark an alpha as stable to make the updater see it. The channel manifest must contain entries only for platforms that actually built; unsupported platforms must never receive another architecture's bundle.

Sources: [Semantic Versioning](https://semver.org/spec/v2.0.0.html), [Tauri static updater manifests](https://v2.tauri.app/plugin/updater/#static-json-file), [Tauri release action](https://github.com/tauri-apps/tauri-action).
