# Security policy

Werd runs local servers, downloads third-party runtimes and, for `.test` domains, uses a privileged helper. We take security reports seriously.

## Reporting a vulnerability

Please **do not open a public issue**. Report it privately through GitHub's [private vulnerability reporting](https://github.com/JustFlavio/werd/security/advisories/new).

Include affected versions, the OS, steps to reproduce and the impact you observed. You will get an acknowledgement within 7 days. We will agree on a disclosure date once a fix is available.

## Supported versions

Werd is pre-1.0. Only the latest release receives security fixes.

## Scope notes

- The daemon only listens on `127.0.0.1` and requires a per-session token stored in the user's data directory.
- Runtime archives are pinned by version and verified with SHA-256 before extraction.
- The desktop app can open only loopback URLs and the project repository in the browser.
