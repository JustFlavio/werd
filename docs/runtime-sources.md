# Windows runtimes: provenance and release blockers

Runtimes are downloaded on demand and never bundled with the Werd installer. The catalog lives in `catalog/catalog.json`. Every download is verified with SHA-256 before extraction, and a unit test checks that every pinned checksum is well formed.

| Runtime | Version and source | Archive SHA-256 | Main license |
| --- | --- | --- | --- |
| PHP NTS x64 | [windows.php.net](https://windows.php.net/downloads/releases/php-8.5.11-nts-Win32-vs17-x64.zip), 8.5.11 | `0ea96e0d2b9b737a6036f05cf4e95c49313faa6d0f27bd97edb2742503f0c043` | PHP License 3.01 |
| phpredis for PHP 8.5 NTS x64 | [PECL Windows build](https://downloads.php.net/~windows/pecl/releases/redis/6.3.0/php_redis-6.3.0-8.5-nts-vs17-x64.zip), 6.3.0 | `481d6d1af45060ab41af6abe250faa270276fc47a493badc2e178024cdf6e255` | PHP License 3.01 |
| Caddy x64 | [GitHub release](https://github.com/caddyserver/caddy/releases/download/v2.11.4/caddy_2.11.4_windows_amd64.zip), 2.11.4 | `1708333f79e274c7697285afe6d592ab39314e0b131e9ec6bea08ad27df62ebf` | Apache-2.0 |
| PostgreSQL x64 | [EnterpriseDB binaries](https://get.enterprisedb.com/postgresql/postgresql-18.6-1-windows-x64-binaries.zip), 18.6-1 | `fbe23da234ee31547bf8a36d29dfd81e82b849df2d2b78d2eecb43d360252f8c` | PostgreSQL License; check the notices bundled in the EDB distribution |
| Redis port x64 MSYS2 | [redis-windows release](https://github.com/redis-windows/redis-windows/releases/download/7.2.8/Redis-7.2.8-Windows-x64-msys2.zip), 7.2.8 | `aa6d4206a08d1189dd7a57c78201540a53b12350e1ad09ee7caca04120600656` | Redis 7.2 BSD-3-Clause; MSYS2 dependencies to be reviewed |
| Mailpit x64 | [GitHub release](https://github.com/axllent/mailpit/releases/download/v1.31.2/mailpit-windows-amd64.zip), 1.31.2 | `42c20e5c3254125ea7489847811f10d70e39de573fe41d03a61412c87913e995` | MIT |
| RustFS x64 | [GitHub release](https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-windows-x86_64-v1.0.0.zip), 1.0.0 | `4ccf5858ce8e6f70f01af2394c8cc0e0878ee77faa6c20d3179153476554b7d8` | Apache-2.0 |

pgvector v0.8.6 is built from [source](https://github.com/pgvector/pgvector/tree/v0.8.6) at commit `8ee86c96f0fd72390f890aa8a336fda6d3ab4c6c`. It is compiled against the PostgreSQL 18.6-1 headers and binaries listed above. The source archive from `codeload.github.com` has SHA-256 `e93a1567219c9ce523ca16473f6c41cc80e01345b2d91ccdee40b473b7c5dd0a`.

The app, or `scripts/build-pgvector-windows.ps1`, installs `vector.dll`, the SQL files and `vector.control` into the local PostgreSQL runtime. Never use a DLL built for a different PostgreSQL build.

## Before the public beta

- Review the licenses and notices of the MSYS2 DLLs in the Windows Redis package (`msys-2.0.dll`, `msys-crypto-3.dll`, `msys-ssl-3.dll`) and of dependencies embedded in the other binaries. The Redis port repository states that its license does not replace Redis's own.
- Produce a reproducible, verified pgvector artifact matching the PostgreSQL 18.6-1 package exactly, so end users do not need Visual Studio Build Tools.
- Create the equivalent catalogs for macOS (Apple Silicon and Intel) and Linux (x64 and arm64), with checksums, licenses and real tests on each platform.
- Verify installers and updates on every platform and keep the required third-party notices.

These items block the public beta. The Windows prototype exists to validate the flow locally.
