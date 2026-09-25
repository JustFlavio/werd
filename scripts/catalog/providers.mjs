// One provider per product. Each returns the product entry for catalog.json.
// `ctx.sha256(url, upstreamHash?)` returns the upstream hash when given,
// the cached hash for a known URL, or downloads and hashes the file.

import { compareVersions, exists, fetchJson, fetchText, githubReleases, latestPerLine } from "./lib.mjs";

const WINDOWS = "windows-x64";
const minor = (version) => version.split(".").slice(0, 2).join(".");
const major = (version) => version.split(".")[0];

async function php(ctx) {
  const base = "https://downloads.php.net/~windows/releases/";
  const releases = await fetchJson(`${base}releases.json`);
  const lines = {};
  for (const [line, release] of Object.entries(releases)) {
    const key = Object.keys(release).find((name) => /^nts-v[cs]\d+-x64$/.test(name));
    if (!key || compareVersions(line, "7.4") < 0) continue;
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
  return { label: "PHP", kind: "runtime", lines };
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
    lines[line] = {
      latest: version,
      lts,
      builds: { [WINDOWS]: { url, sha256: await ctx.sha256(url, upstream), format: "zip", marker: "node.exe" } },
    };
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

export const providers = {
  php,
  node,
  composer,
  cacert,
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
