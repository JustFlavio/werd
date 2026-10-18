# Herd 2: initial inventory and implementation candidates

Reviewed on 2026-10-17. This first pass identifies the installed Windows artifact, checks which REA workflows are useful, and updates the feature comparison. It is not a complete reconstruction or a runtime compatibility test.

Follow-up: the first independent Werd MCP increment is now implemented in source as `werd mcp`; see [current MCP behavior](../mcp.md). The proposal below records the original investigation plan. It is an interface to Werd alone, not a connector to Herd. Full replacement, including Pro workflows, remains the product scope.

## Scope and result

| Question | Status | Finding |
| --- | --- | --- |
| Is REA connected? | Answered | The active MCP reports REA 6.5.0, 207 tools and six prompts. |
| Which Herd build is installed? | Answered | Windows uninstall metadata and the packaged manifest both report 2.0.1. |
| What application format is available? | Answered | An Electron distribution with an ASAR package, readable preload JavaScript, bytecode main modules and separate helpers. |
| Can useful inspection start without Ghidra? | Answered | Yes: ASAR inventory and selected JavaScript analysis ran successfully without a native engine. |
| Are the backend implementation and runtime behavior understood? | Partially answered | Renderer IPC syntax is available; main-process handlers, V8 bytecode semantics and native helper internals remain unexamined. |
| What should Werd implement first? | Proposed | Reuse the current daemon for a small MCP adapter, while continuing the existing reliability and platform work. |

No Herd settings, services, projects or running processes were changed by this investigation. The analysis did not execute the inspected JavaScript or bytecode. No additional analysis engines were installed.

## Artifact identity and observations

The local application is `C:/Program Files/Herd/Herd.exe`. Its package is `C:/Program Files/Herd/resources/app.asar`.

- ASAR SHA-256: `a058773356be61dd36f1597bc9fdb4dcc08b68d72cd74be9563ce8f9afa68cac`.
- Manifest entrypoint: `out/main/index.js`.
- That 72-byte entrypoint loads a bytecode loader and `out/main/index.jsc`. The latter contains 2,409,824 bytes. The main application's original JavaScript was not recovered.
- `out/preload/index.js` is readable JavaScript, 48,364 bytes, with SHA-256 `f9ad40b81c01d8cf5802e572cbe6f2368292ae6ce06f6d4becc90a841a117fe7`.
- Resources include a Herd MCP PHAR, Valet CLI components and workspace-related PHP files. Their filenames identify future inspection candidates; they do not establish behavior.
- Installed tools include `herd-devtools-helper.exe`, `herd-opencode.exe` and `pty-host.exe`. Their implementation and communication protocols were not inspected.

REA's selected analysis covered only `package.json`, `out/main/index.js` and `out/preload/index.js`. It parsed two JavaScript files with no parse failures and reported 416 IPC operations. These are static syntax observations, not 416 implemented features. This scope did not observe main-process handlers or establish any renderer-to-handler pairings. Dependencies and other application sources were intentionally outside the source-analysis scope.

A second inert AST pass recorded candidate IPC call locations in the preload, including MCP configuration, workspace and dump-related channel families. Its output remains local. These names are investigation entrypoints, not a specification of accepted parameters, permissions or results.

Ghidra is not required for the completed work. A future native-helper question may justify a native engine. Installing it would not by itself recover JavaScript from the main application's V8 bytecode.

## Integrity and coverage limits

The default ASAR inspection stopped on an integrity contradiction. A separate inspection with `integrity_policy: record-and-continue` retained all eight discrepancies and marked the affected unpacked bytes as untrusted:

- Devtools helper ZIPs for x64 and arm64.
- OpenCode ZIPs for x64 and arm64.
- Nginx and nvm executables.
- Whisper CLI executables for x64 and arm64.

The cause was not established. Packaging/signing changes and local modification remain unverified possibilities; this observation alone is not evidence of malicious modification. No equivalence claim is made for these files.

The three selected source entries had verified inventory hashes. The full artifact inventory and the partial application analysis have different coverage: a complete inventory does not imply complete code or behavior recovery. The application analysis leaves 16,438 inventoried relevant files outside its selected source scope. No runtime observations were collected.

## Documented Herd 2 behavior versus Werd

The following statements come from official documentation, independently of the artifact observations above. They were not tested against the running installation.

| Area | Documented Herd behavior | Werd baseline and proposed response |
| --- | --- | --- |
| MCP | Exposes site/runtime/service operations and agent context. | The daemon already exposes site, runtime, service, log and diagnostic RPC methods. Implement an independent adapter over that API. |
| Insights | Pro request diagnostics include dumps, queries, logs, jobs, HTTP and cache events, with CLI/MCP access; instrumentation uses a PHP extension. | Process logs exist; structured dumps and Laravel logs are planned. Start with a small event model carrying project identity; deeper request instrumentation is a separate design task. |
| Dev processes | With Studio enabled, runs `artisan dev` for supported Laravel versions or the project's package-manager `dev` script, with idle shutdown. Studio also documents a queue listener. | Werd already supervises Vite directly. Extend the supervisor to named workers/scheduler/Reverb rather than replacing Vite. Custom dev scripts remain a distinct compatibility requirement. |
| Workspaces | Branch copies on subdomains, existing Git worktree detection and shared/cloned/empty database choices. Windows uses full copies. | No workspace feature. Consider it after backup/restore, domain routing and process ownership are reliable. |
| Studio | Browser preview and coding agents alongside files, diffs, terminals and development tools. | No equivalent. Keep it optional and later than MCP and the dependable local environment. |
| Per-site services | Can apply database, queue, cache and broadcasting selections to the project environment. | Existing links and connection suggestions remain useful. Any future application of environment changes needs a preview and a recoverable backup. |

Sources:

- [AI integrations](https://herd.laravel.com/docs/windows/advanced-usage/ai-integrations).
- [Insights](https://herd.laravel.com/docs/windows/debugging/insights).
- [Studio](https://herd.laravel.com/docs/windows/herd-studio/overview).
- [Dev server](https://herd.laravel.com/docs/windows/herd-studio/dev-server).
- [Workspaces](https://herd.laravel.com/docs/windows/sites/workspaces).
- [Per-site services](https://herd.laravel.com/docs/windows/herd-pro-services/per-site-services).

## First implementation candidate: Werd MCP

This is a proposed increment, not a new release gate or a shipped feature.

Use a stdio MCP process as another client of the existing authenticated loopback RPC. Keep domain logic in `werd-core`; do not reproduce Herd's IPC names or couple Werd to a running Herd installation.

| Initial tool responsibility | Existing RPC |
| --- | --- |
| Daemon identity | `ping`, `system.info` |
| Site inventory and project details | `sites.list`, `sites.info` |
| Installed and available runtimes | `runtimes.list` |
| Service status | `services.list` |
| Project process logs | `logs` with project id and source |
| Environment diagnostics | `doctor` |

Acceptance criteria before calling the first adapter usable:

1. A real MCP client initializes, lists the declared tools and calls them over stdio. Standard output contains protocol messages only.
2. Parameters and site selection are validated; invalid ids, a missing daemon and a failed RPC produce useful structured errors.
3. Responses expose an intentional public view of daemon data. Review and redact credentials and secrets from service summaries and logs; never expose the local RPC token or unrestricted environment files.
4. Tests cover client/daemon integration and error cases, and use isolated Werd data. Do not change real projects or rely on Herd being installed.
5. The first tools do not install runtimes, start/stop services, change project files or request elevation. Add explicit modifying tools in a later increment.
6. Verify on Windows first, then run the same client/daemon checks on the macOS and Linux targets. Tool availability must not imply unvalidated runtime support.

Continue the official roadmap's platform catalogs, certificate trust, service lifecycle and backup/restore work. The new AI features do not remove those blockers. Neither an embedded chat UI nor an LLM API key is required for the adapter.

## Evidence ledger and reuse

Complete REA evidence was exported before closing the session to `.cache/rea/herd-2.0.1/evidence.json` (13 records, 324,146,146 bytes). `.cache/` is already Git-ignored. The bundle may contain target-derived material; it is local investigation data, not Werd source. Later projections are saved separately in the same ignored folder. No Herd implementation source or binary was added to tracked project files.

| Alias | Evidence ID | Scope |
| --- | --- | --- |
| I1 | `ev_6a5a6cd4f70971b931deb1f81968971a1f44fd8ee17c91399c56e2a5624f2fd4` | Complete artifact inspection with recorded integrity contradictions |
| I2 | `ev_7062a0d5cdcd93c2fd37c003a48a4889e2a34733aff14d0bd085a50bb5fd486b` | Underlying artifact inventory |
| J1 | `ev_e8fd9a2ee7835ea3f57af0167c0bd52bf7a44e9f59df5db184647069df4757b8` | Selected JavaScript application analysis |
| J2 | `ev_c0d5253d68ecd806b389cb044e005d0c3295ff855a5f594fe5a1be68074f3238` | Preload source-module projection |
| I3 | `ev_c663186fb750ac88154c1b4661532a74cb188471e4c03dfd3ec4efb557157196` | Later projection of all eight integrity contradictions |

Local auxiliary records are `artifact-identity.json`, `preload-ipc-observations.json`, `inventory-summary.json`, `javascript-summary.json` and `integrity-contradictions.json` in the same directory. They preserve hashes, selection and limits without copying application source into this report.

Retained Evidence IDs refer to the original REA session. Closing clears retained references; use the exported bundle for future sessions instead of assuming those IDs remain live. Compare the artifact SHA-256 before reusing any findings for a different Herd installation or version.

Unresolved work: main bytecode behavior, native helper boundaries, instrumentation protocol, MCP result shapes, workspace lifecycle and actual runtime acceptance tests. Each requires a focused question and its own evidence; none is claimed complete here.
