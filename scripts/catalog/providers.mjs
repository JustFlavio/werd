// One provider per product. Each returns the product entry for catalog.json.
// `ctx.sha256(url, upstreamHash?)` returns the upstream hash when given,
// the cached hash for a known URL, or downloads and hashes the file.

import { compareVersions, exists, fetchJson, fetchText, githubHeaders, githubReleases, latestPerLine } from "./lib.mjs";

const WINDOWS = "windows-x64";
/** Catalog platform → architecture names used upstream (static-php-cli, Node.js). */
const MACOS = [
  { platform: "macos-arm64", arch: "arm64", spc: "aarch64", node: "darwin-arm64", files: "osx-arm64-tar" },
  { platform: "macos-x64", arch: "x64", spc: "x86_64", node: "darwin-x64", files: "osx-x64-tar" },
];
/** This repository: macOS PHP builds are published in its `php-<version>` releases. */
const WERD_REPO = process.env.GITHUB_REPOSITORY ?? "JustFlavio/werd";
const minor = (version) => version.split(".").slice(0, 2).join(".");
const major = (version) => version.split(".")[0];

async function php(ctx) {
  const lines = {};
  await addWindowsPhp(ctx, lines, "https://downloads.php.net/~windows/releases/");
  // The QA folder adds the next minor (8.6.0RC2) before its release; its RCs of
  // existing lines are skipped.
  await addWindowsPhp(ctx, lines, "https://downloads.php.net/~windows/qa/", { newLinesOnly: true });
  await addWerdPhp(ctx, lines);
  await addStaticPhp(ctx, lines);
  return { label: "PHP", kind: "runtime", lines };
}

async function addWindowsPhp(ctx, lines, base, { newLinesOnly = false } = {}) {
  const releases = await fetchJson(`${base}releases.json`);
  for (const [line, release] of Object.entries(releases)) {
    const key = Object.keys(release).find((name) => /^nts-v[cs]\d+-x64$/.test(name));
    if (!key || compareVersions(line, "7.4") < 0) continue;
    if (newLinesOnly && (lines[line] || !/(alpha|beta|RC)\d+$/.test(release.version))) continue;
    const zip = release[key].zip;
    lines[line] = {
      latest: release.version,
      builds: {
        [WINDOWS]: {
          url: base + zip.path,
          sha256: await ctx.sha256(base + zip.path, zip.sha256),
          format: "zip",
          marker: "php-cgi.exe",
        },
      },
    };
  }
}

/**
 * macOS PHP built by `.github/workflows/php-macos.yml`: php and php-fpm in one
 * archive per architecture, released as `php-<version>`. The newest release of
 * each line wins; GitHub's asset digest is the upstream checksum.
 */
async function addWerdPhp(ctx, lines) {
  const releases = await githubReleases(WERD_REPO, 5, { prereleases: true });
  for (const { platform, arch } of MACOS) {
    const newest = new Map();
    for (const release of releases) {
      const version = release.tag_name.match(/^php-(\d+\.\d+\.\d+(?:(?:alpha|beta|RC)\d+)?)$/)?.[1];
      const asset =
        version && release.assets.find((candidate) => candidate.name === `php-${version}-macos-${arch}.tar.gz`);
      if (!asset) continue;
      const line = minor(version);
      if (!newest.has(line) || compareVersions(version, newest.get(line).version) > 0) {
        newest.set(line, { version, asset });
      }
    }
    for (const [line, { version, asset }] of newest) {
      const url = asset.browser_download_url;
      const upstream = asset.digest?.startsWith("sha256:") ? asset.digest.slice(7) : undefined;
      lines[line] ??= { latest: version, builds: {} };
      if (compareVersions(version, lines[line].latest) > 0) lines[line].latest = version;
      lines[line].builds[platform] = {
        url,
        sha256: await ctx.sha256(url, upstream),
        format: "tar.gz",
        marker: "php-fpm",
        ...(version === lines[line].latest ? {} : { version }),
      };
    }
  }
}

/**
 * macOS PHP from static-php-cli's prebuilt "bulk" distribution, for lines Werd
 * does not build itself (8.0 and 8.1, which php.net no longer updates).
 * Self-contained binaries with the common extensions compiled in (list in its
 * README.txt). There is no CGI SAPI, so the php-fpm archive is installed next
 * to the CLI. Each build records its own version when it differs from Windows.
 */
async function addStaticPhp(ctx, lines) {
  const base = "https://dl.static-php.dev/static-php-cli/bulk/";
  const listing = await fetchJson(`${base}?format=json`);
  const names = new Set(listing.map((entry) => entry.name));
  for (const { platform, spc } of MACOS) {
    const newest = new Map();
    for (const name of names) {
      const version = name.match(new RegExp(`^php-(\\d+\\.\\d+\\.\\d+)-cli-macos-${spc}\\.tar\\.gz$`))?.[1];
      if (!version || !names.has(`php-${version}-fpm-macos-${spc}.tar.gz`)) continue;
      const line = minor(version);
      if (!newest.has(line) || compareVersions(version, newest.get(line)) > 0) newest.set(line, version);
    }
    for (const [line, version] of newest) {
      if (lines[line]?.builds[platform]) continue;
      const cli = `${base}php-${version}-cli-macos-${spc}.tar.gz`;
      const fpm = `${base}php-${version}-fpm-macos-${spc}.tar.gz`;
      lines[line] ??= { latest: version, builds: {} };
      if (compareVersions(version, lines[line].latest) > 0) lines[line].latest = version;
      lines[line].builds[platform] = {
        url: cli,
        sha256: await ctx.sha256(cli),
        format: "tar.gz",
        marker: "php-fpm",
        ...(version === lines[line].latest ? {} : { version }),
        extra: [{ url: fpm, sha256: await ctx.sha256(fpm), format: "tar.gz" }],
      };
    }
  }
}

async function node(ctx) {
  const index = await fetchJson("https://nodejs.org/dist/index.json");
  const byMajor = new Map();
  for (const release of index) {
    const version = release.version.slice(1);
    const line = major(version);
    if (Number(line) < 16 || !release.files.includes("win-x64-zip")) continue;
    if (!byMajor.has(line) || compareVersions(version, byMajor.get(line).version) > 0) {
      byMajor.set(line, { version, lts: release.lts !== false });
    }
  }
  const lines = {};
  for (const [line, { version, lts }] of byMajor) {
    const file = `node-v${version}-win-x64.zip`;
    const url = `https://nodejs.org/dist/v${version}/${file}`;
    const sums = await fetchText(`https://nodejs.org/dist/v${version}/SHASUMS256.txt`);
    const upstream = sums
      .split("\n")
      .find((row) => row.endsWith(`  ${file}`))
      ?.split(" ")[0];
    const builds = { [WINDOWS]: { url, sha256: await ctx.sha256(url, upstream), format: "zip", marker: "node.exe" } };
    for (const { platform, node, files } of MACOS) {
      if (!index.find((release) => release.version === `v${version}`)?.files.includes(files)) continue;
      const tarball = `node-v${version}-${node}.tar.gz`;
      const hash = sums
        .split("\n")
        .find((row) => row.endsWith(`  ${tarball}`))
        ?.split(" ")[0];
      if (!hash) continue;
      const tarballUrl = `https://nodejs.org/dist/v${version}/${tarball}`;
      builds[platform] = {
        url: tarballUrl,
        sha256: await ctx.sha256(tarballUrl, hash),
        format: "tar.gz",
        marker: "bin/node",
      };
    }
    lines[line] = { latest: version, lts, builds };
  }
  return { label: "Node.js", kind: "runtime", lines };
}

async function composer(ctx) {
  const versions = await fetchJson("https://getcomposer.org/versions");
  const stable = versions.stable.find((entry) => !entry.lts);
  const url = `https://getcomposer.org${stable.path}`;
  const upstream = (await fetchText(`${url}.sha256sum`)).trim().split(/\s+/)[0];
  const build = { url, sha256: await ctx.sha256(url, upstream), format: "phar", marker: "composer.phar" };
  // A .phar runs everywhere PHP runs.
  return {
    label: "Composer",
    kind: "tool",
    lines: { [major(stable.version)]: { latest: stable.version, builds: { any: build } } },
  };
}

/** Mozilla CA bundle from curl.se: PHP on Windows needs it for HTTPS (Composer, Guzzle). */
async function cacert(ctx) {
  const url = "https://curl.se/ca/cacert.pem";
  const upstream = (await fetchText(`${url}.sha256`)).trim().split(/\s+/)[0];
  const header = (await fetchText(url)).slice(0, 400);
  const date = header.match(/as of: \w+ (\w+) (\d+) [\d:]+ (\d{4})/);
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const version = date
    ? `${date[3]}.${String(months.indexOf(date[1]) + 1).padStart(2, "0")}.${date[2].padStart(2, "0")}`
    : "0";
  return {
    label: "CA certificates",
    kind: "tool",
    lines: {
      mozilla: {
        latest: version,
        builds: { any: { url, sha256: await ctx.sha256(url, upstream), format: "phar", marker: "cacert.pem" } },
      },
    },
  };
}

/** Latest release per line from GitHub, one Windows zip asset per release. */
function github({ repo, label, kind, categories, defaultPort, lineOf, asset, marker, minLine }) {
  return async (ctx) => {
    const releases = await githubReleases(repo);
    const byVersion = new Map();
    for (const release of releases) {
      const version = release.tag_name.replace(/^v/, "");
      const file = release.assets.find((candidate) => asset.test(candidate.name));
      if (file) byVersion.set(version, file.browser_download_url);
    }
    const latest = latestPerLine([...byVersion.keys()], lineOf);
    const lines = {};
    for (const [line, version] of latest) {
      if (minLine && compareVersions(line, minLine) < 0) continue;
      const url = byVersion.get(version);
      lines[line] = {
        latest: version,
        builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url), format: "zip", marker } },
      };
    }
    return {
      label,
      kind,
      ...(categories ? { categories } : {}),
      ...(defaultPort ? { default_port: defaultPort } : {}),
      lines,
    };
  };
}

async function postgresql(ctx) {
  const versions = await fetchJson("https://www.postgresql.org/versions.json");
  const lines = {};
  for (const release of versions) {
    if (!release.supported || Number(release.major) < 14) continue;
    // EDB publishes Windows binaries a few days after a release; fall back to older minors.
    for (let patch = Number(release.latestMinor); patch >= 0; patch--) {
      const version = `${release.major}.${patch}`;
      const url = `https://get.enterprisedb.com/postgresql/postgresql-${version}-1-windows-x64-binaries.zip`;
      if (!ctx.known(url) && !(await exists(url))) continue;
      lines[release.major] = {
        latest: version,
        eol: release.eolDate,
        builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url), format: "zip", marker: "bin/postgres.exe" } },
      };
      break;
    }
  }
  return { label: "PostgreSQL", kind: "service", categories: ["database"], default_port: 5432, lines };
}

async function pgvector(ctx) {
  const tags = await fetchJson("https://api.github.com/repos/pgvector/pgvector/tags?per_page=20");
  const versions = tags.map((tag) => tag.name.replace(/^v/, "")).filter((name) => /^\d+\.\d+\.\d+$/.test(name));
  const lines = {};
  for (const [line, version] of latestPerLine(versions, minor)) {
    if (compareVersions(line, "0.8") < 0) continue;
    const url = `https://codeload.github.com/pgvector/pgvector/zip/refs/tags/v${version}`;
    // Source archive: built locally against the PostgreSQL runtime (Windows needs MSVC).
    lines[line] = {
      latest: version,
      builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url), format: "zip", marker: "Makefile.win" } },
    };
  }
  return { label: "pgvector", kind: "extension", extends: "postgresql", lines };
}

/** MySQL server versions follow the MySQL Cluster tags on GitHub; zips move to /archives once superseded. */
async function mysql(ctx) {
  const tags = [];
  for (let page = 1; page <= 3; page++) {
    const batch = await fetchJson(
      `https://api.github.com/repos/mysql/mysql-server/tags?per_page=100&page=${page}`,
      githubHeaders(),
    );
    tags.push(...batch.map((tag) => tag.name));
    if (batch.length < 100) break;
  }
  const versions = tags.map((name) => name.match(/^mysql-cluster-(\d+\.\d+\.\d+)$/)?.[1]).filter(Boolean);
  const perLine = latestPerLine(versions, minor);
  // LTS lines plus the newest innovation release.
  const innovation = [...perLine.keys()]
    .filter((line) => line.startsWith("9."))
    .sort(compareVersions)
    .pop();
  const wanted = ["8.0", "8.4", innovation].filter(Boolean);
  const lines = {};
  for (const line of wanted) {
    const [lineMajor, lineMinor, latestPatch] = perLine.get(line).split(".").map(Number);
    for (let patch = latestPatch; patch >= 0; patch--) {
      const version = `${lineMajor}.${lineMinor}.${patch}`;
      const file = `mysql-${version}-winx64.zip`;
      const candidates = [
        `https://cdn.mysql.com/Downloads/MySQL-${line}/${file}`,
        `https://cdn.mysql.com/archives/mysql-${line}/${file}`,
      ];
      let url = candidates.find((candidate) => ctx.known(candidate));
      for (const candidate of url ? [] : candidates) {
        if (await exists(candidate)) {
          url = candidate;
          break;
        }
      }
      if (!url) continue;
      lines[line] = {
        latest: version,
        lts: line !== innovation,
        builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url), format: "zip", marker: "bin/mysqld.exe" } },
      };
      break;
    }
  }
  return { label: "MySQL", kind: "service", categories: ["database"], default_port: 3306, lines };
}

async function mariadb(ctx) {
  const api = "https://downloads.mariadb.org/rest-api/mariadb/";
  const { major_releases: majors } = await fetchJson(api);
  const lines = {};
  for (const major of majors) {
    if (major.release_status !== "Stable" || compareVersions(major.release_id, "10.11") < 0) continue;
    const latest = await fetchJson(`${api}${major.release_id}/latest/`);
    const release = Object.values(latest.releases)[0];
    const file = release?.files.find((candidate) => /-winx64\.zip$/.test(candidate.file_name));
    if (!file) continue;
    const url = file.file_download_url.replace(/^http:/, "https:");
    lines[major.release_id] = {
      latest: release.release_id,
      lts: major.release_support_type === "Long Term Support",
      ...(major.release_eol_date ? { eol: major.release_eol_date } : {}),
      builds: {
        [WINDOWS]: {
          url,
          sha256: await ctx.sha256(url, file.checksum.sha256sum),
          format: "zip",
          marker: "bin/mariadbd.exe",
        },
      },
    };
  }
  return { label: "MariaDB", kind: "service", categories: ["database"], default_port: 3306, lines };
}

async function mongodb(ctx) {
  const { versions } = await fetchJson("https://downloads.mongodb.org/full.json");
  const byLine = new Map();
  for (const release of versions) {
    if (!release.production_release || compareVersions(release.version, "7.0") < 0) continue;
    const download = release.downloads.find((item) => item.target === "windows" && item.arch === "x86_64");
    if (!download?.archive) continue;
    const line = minor(release.version);
    if (!byLine.has(line) || compareVersions(release.version, byLine.get(line).version) > 0) {
      byLine.set(line, { version: release.version, archive: download.archive });
    }
  }
  const lines = {};
  for (const [line, { version, archive }] of byLine) {
    lines[line] = {
      latest: version,
      builds: {
        [WINDOWS]: {
          url: archive.url,
          sha256: await ctx.sha256(archive.url, archive.sha256),
          format: "zip",
          marker: "bin/mongod.exe",
        },
      },
    };
  }
  return { label: "MongoDB", kind: "service", categories: ["database"], default_port: 27017, lines };
}

/** Single-executable GitHub releases (Meilisearch ships a bare .exe). */
function githubExecutable({ repo, label, categories, defaultPort, lineOf, asset, marker, minLine }) {
  return async (ctx) => {
    const product = await github({
      repo,
      label,
      kind: "service",
      categories,
      defaultPort,
      lineOf,
      asset,
      marker,
      minLine,
    })(ctx);
    for (const line of Object.values(product.lines)) line.builds[WINDOWS].format = "file";
    return product;
  };
}

/**
 * phpredis from the official PECL Windows builds on php.net, one line per PHP line.
 * Laravel uses it by default (REDIS_CLIENT=phpredis); PHP for Windows does not ship it.
 */
async function phpredis(ctx) {
  const base = "https://downloads.php.net/~windows/pecl/releases/redis/";
  const versions = [...(await fetchText(base)).matchAll(/href="(\d+\.\d+\.\d+)\/"/g)].map((match) => match[1]);
  const version = versions.sort(compareVersions).at(-1);
  if (!version) throw new Error("no phpredis release found");
  const listing = await fetchText(`${base}${version}/`);
  const files = [...listing.matchAll(/href="(php_redis-[^"]+-nts-v[cs]\d+-x64\.zip)"/g)].map((match) => match[1]);
  const lines = {};
  for (const file of files) {
    const line = file.match(/^php_redis-[^-]+-(\d+\.\d+)-nts/)?.[1];
    if (!line || compareVersions(line, "7.4") < 0) continue;
    const url = `${base}${version}/${file}`;
    lines[line] = {
      latest: version,
      builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url), format: "zip", marker: "php_redis.dll" } },
    };
  }
  return { label: "phpredis", kind: "extension", extends: "php", lines };
}

/** The newest PECL Windows DLL for each supported PHP line. */
async function phpmongodb(ctx) {
  const base = "https://downloads.php.net/~windows/pecl/releases/mongodb/";
  const versions = [...(await fetchText(base)).matchAll(/href="(\d+\.\d+\.\d+)\/"/g)]
    .map((match) => match[1])
    .sort(compareVersions);
  // MongoDB 2.x requires PHP 8.1+. The last 1.20 release covers PHP 7.4 and 8.0.
  const current = versions.at(-1);
  const legacy = versions.filter((version) => compareVersions(version, "1.21.0") < 0).at(-1);
  if (!current || !legacy) throw new Error("no compatible phpmongodb releases found");
  const lines = {};
  for (const version of [current, legacy]) {
    const listing = await fetchText(`${base}${version}/`);
    const files = [...listing.matchAll(/href="(php_mongodb-[^"]+-nts-v[cs]\d+-x64\.zip)"/g)].map((match) => match[1]);
    for (const file of files) {
      const line = file.match(/^php_mongodb-[^-]+-(\d+\.\d+)-nts/)?.[1];
      if (!line || compareVersions(line, "7.4") < 0 || lines[line]) continue;
      const url = `${base}${version}/${file}`;
      lines[line] = {
        latest: version,
        builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url), format: "zip", marker: "php_mongodb.dll" } },
      };
    }
  }
  for (const line of ["7.4", "8.0", "8.1", "8.2", "8.3", "8.4", "8.5"]) {
    if (!lines[line]) throw new Error(`no phpmongodb build for PHP ${line}`);
  }
  return { label: "phpmongodb", kind: "extension", extends: "php", lines };
}

export const providers = {
  php,
  node,
  composer,
  cacert,
  phpredis,
  phpmongodb,
  caddy: github({
    repo: "caddyserver/caddy",
    label: "Caddy",
    kind: "tool",
    lineOf: major,
    asset: /_windows_amd64\.zip$/,
    marker: "caddy.exe",
    minLine: "2",
  }),
  postgresql,
  pgvector,
  mysql,
  mariadb,
  mongodb,
  meilisearch: githubExecutable({
    repo: "meilisearch/meilisearch",
    label: "Meilisearch",
    categories: ["search"],
    defaultPort: 7700,
    lineOf: major,
    asset: /^meilisearch-windows-amd64\.exe$/,
    marker: "meilisearch.exe",
    minLine: "1",
  }),
  redis: github({
    repo: "redis-windows/redis-windows",
    label: "Redis",
    kind: "service",
    categories: ["cache", "queue"],
    defaultPort: 6379,
    lineOf: minor,
    asset: /-Windows-x64-msys2\.zip$/,
    marker: "redis-server.exe",
    minLine: "7.2",
  }),
  mailpit: github({
    repo: "axllent/mailpit",
    label: "Mailpit",
    kind: "service",
    categories: ["mail"],
    defaultPort: 1025,
    lineOf: major,
    asset: /^mailpit-windows-amd64\.zip$/,
    marker: "mailpit.exe",
    minLine: "1",
  }),
  rustfs: github({
    repo: "rustfs/rustfs",
    label: "RustFS",
    kind: "service",
    categories: ["storage"],
    defaultPort: 9000,
    lineOf: major,
    asset: /^rustfs-windows-x86_64-v[\d.]+\.zip$/,
    marker: "rustfs.exe",
    minLine: "1",
  }),
};
