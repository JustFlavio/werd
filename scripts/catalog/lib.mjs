// Shared helpers for catalog providers.

import { createHash } from "node:crypto";

const USER_AGENT = "werd-catalog-generator (+https://github.com/JustFlavio/werd)";

export async function fetchText(url, headers = {}) {
  const response = await fetch(url, { headers: { "user-agent": USER_AGENT, ...headers } });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.text();
}

export async function fetchJson(url, headers = {}) {
  return JSON.parse(await fetchText(url, headers));
}

export async function exists(url) {
  const response = await fetch(url, { method: "HEAD", headers: { "user-agent": USER_AGENT }, redirect: "follow" });
  return response.ok;
}

/** Streams a download through SHA-256 without keeping it in memory. */
export async function hashUrl(url) {
  const response = await fetch(url, { headers: { "user-agent": USER_AGENT }, redirect: "follow" });
  if (!response.ok || !response.body) throw new Error(`${url}: HTTP ${response.status}`);
  const hash = createHash("sha256");
  let bytes = 0;
  for await (const chunk of response.body) {
    hash.update(chunk);
    bytes += chunk.length;
  }
  console.log(`  hashed ${url} (${(bytes / 1024 / 1024).toFixed(1)} MiB)`);
  return hash.digest("hex");
}

export function githubHeaders() {
  const token = process.env.GITHUB_TOKEN;
  return {
    accept: "application/vnd.github+json",
    ...(token ? { authorization: `Bearer ${token}` } : {}),
  };
}

/** Published GitHub releases, newest first; pre-releases only when asked for. */
export async function githubReleases(repo, pages = 2, { prereleases = false } = {}) {
  const releases = [];
  for (let page = 1; page <= pages; page++) {
    const batch = await fetchJson(
      `https://api.github.com/repos/${repo}/releases?per_page=100&page=${page}`,
      githubHeaders(),
    );
    releases.push(...batch.filter((release) => !release.draft && (prereleases || !release.prerelease)));
    if (batch.length < 100) break;
  }
  return releases;
}

/** Each dotted part as [number, stage, stage number]; a final release (stage 3) sorts after alpha, beta and RC. */
function versionKey(value) {
  return value.split(/[.-]/).map((part) => {
    const [, number, letters, stageNumber] = part.toLowerCase().match(/^(\d*)([a-z]*)(\d*)$/) ?? [];
    const stage = !letters ? 3 : letters === "alpha" ? 0 : letters === "beta" ? 1 : 2;
    return [Number.parseInt(number, 10) || 0, stage, Number.parseInt(stageNumber, 10) || 0];
  });
}

/** Numeric comparison of dotted versions ("8.10.2" > "8.9.9", "8.6.0RC2" < "8.6.0"). */
export function compareVersions(a, b) {
  const left = versionKey(a);
  const right = versionKey(b);
  const release = [0, 3, 0];
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const l = left[index] ?? release;
    const r = right[index] ?? release;
    for (let field = 0; field < 3; field++) {
      if (l[field] !== r[field]) return l[field] - r[field];
    }
  }
  return 0;
}

/** Keeps the newest version for each line computed by `lineOf`. */
export function latestPerLine(versions, lineOf) {
  const lines = new Map();
  for (const version of versions) {
    const line = lineOf(version);
    if (line && (!lines.has(line) || compareVersions(version, lines.get(line)) > 0)) lines.set(line, version);
  }
  return lines;
}
