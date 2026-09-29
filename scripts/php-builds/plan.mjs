#!/usr/bin/env node
// Decides which macOS PHP builds are missing from werd-runtimes releases.
//
//   node scripts/php-builds/plan.mjs            # prints the build matrix as JSON
//   node scripts/php-builds/plan.mjs 8.5.11     # only these versions, rebuilt even if published
//
// Wanted: the newest patch of every line php.net lists as active, plus the
// current pre-release (8.6.0RC2 today). Each version is built for arm64 and x64
// and published as the release `php-<version>` with one archive per architecture.

import { appendFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fetchJson, githubHeaders } from "../catalog/lib.mjs";

export const ARCHITECTURES = [
  { arch: "arm64", runner: "macos-15" },
  { arch: "x64", runner: "macos-15-intel" },
];

export const assetName = (version, arch) => `php-${version}-macos-${arch}.tar.gz`;

/** Source tarballs to build: `{ version, url, sha256 }`, stable lines and the pre-release. */
export async function wantedVersions() {
  const wanted = [];
  const active = await fetchJson("https://www.php.net/releases/active.php");
  for (const lines of Object.values(active)) {
    for (const release of Object.values(lines)) {
      const source = release.source.find((file) => file.filename.endsWith(".tar.xz"));
      if (!source) continue;
      wanted.push({
        version: release.version,
        url: `https://www.php.net/distributions/${source.filename}`,
        sha256: source.sha256,
      });
    }
  }
  const preReleases = await fetchJson("https://www.php.net/pre-release-builds.php?format=json");
  for (const entry of Object.values(preReleases)) {
    const release = entry.release;
    const file = release?.files?.xz;
    // Entries without files are RC slots for patch releases that are not open yet.
    if (!entry.active || !release?.version || !file?.path || !file.sha256) continue;
    const line = release.version.split(".").slice(0, 2).join(".");
    if (wanted.some((existing) => existing.version.startsWith(`${line}.`))) continue;
    wanted.push({ version: release.version, url: file.path, sha256: file.sha256 });
  }
  return wanted;
}

async function publishedAssets(repository) {
  const names = new Set();
  for (let page = 1; page <= 5; page++) {
    const releases = await fetchJson(
      `https://api.github.com/repos/${repository}/releases?per_page=100&page=${page}`,
      githubHeaders(),
    );
    for (const release of releases) {
      if (release.tag_name.startsWith("php-")) for (const asset of release.assets) names.add(asset.name);
    }
    if (releases.length < 100) break;
  }
  return names;
}

async function main() {
  const repository = process.env.WERD_RUNTIMES_REPOSITORY ?? "JustFlavio/werd-runtimes";
  const only = process.argv.slice(2);
  const published = only.length ? new Set() : await publishedAssets(repository);
  const include = [];
  for (const source of await wantedVersions()) {
    if (only.length && !only.includes(source.version)) continue;
    for (const { arch, runner } of ARCHITECTURES) {
      if (published.has(assetName(source.version, arch))) continue;
      include.push({ ...source, arch, runner, prerelease: /[a-z]/i.test(source.version) });
    }
  }
  const matrix = JSON.stringify({ include });
  console.log(matrix);
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `matrix=${matrix}\ncount=${include.length}\n`);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) await main();
