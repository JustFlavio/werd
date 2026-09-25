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

/** Non-prerelease GitHub releases, newest first. */
export async function githubReleases(repo, pages = 2) {
  const releases = [];
  for (let page = 1; page <= pages; page++) {
    const batch = await fetchJson(
      `https://api.github.com/repos/${repo}/releases?per_page=100&page=${page}`,
      githubHeaders(),
    );
    releases.push(...batch.filter((release) => !release.prerelease && !release.draft));
    if (batch.length < 100) break;
  }
  return releases;
}

/** Numeric comparison of dotted versions ("8.10.2" > "8.9.9"). */
export function compareVersions(a, b) {
  const left = a.split(/[.-]/).map((part) => Number.parseInt(part, 10) || 0);
  const right = b.split(/[.-]/).map((part) => Number.parseInt(part, 10) || 0);
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0);
    if (difference !== 0) return difference;
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
