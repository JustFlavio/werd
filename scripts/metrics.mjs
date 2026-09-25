#!/usr/bin/env node
// Collects project health metrics into metrics/metrics.json and metrics/metrics.md.
// Each metric is optional: missing inputs are reported as "n/a" instead of failing.
//
//   npm run metrics                     # sizes + daemon startup/memory
//   npm run metrics -- --coverage       # also run Rust and TS coverage
//
// Inputs it looks for: dist/ (npm run build), target/release binaries
// (cargo build --release), target/release/bundle (installers).
// In GitHub Actions the markdown table is appended to the job summary.

import { execFileSync, spawn } from "node:child_process";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const withCoverage = process.argv.includes("--coverage");
const exe = process.platform === "win32" ? ".exe" : "";
const metrics = {};

const kib = (bytes) => (bytes == null ? "n/a" : `${(bytes / 1024).toFixed(1)} KiB`);
const mib = (bytes) => (bytes == null ? "n/a" : `${(bytes / 1024 / 1024).toFixed(2)} MiB`);
const pct = (value) => (value == null ? "n/a" : `${value.toFixed(1)}%`);
const size = (path) => (existsSync(path) ? statSync(path).size : null);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function files(directory) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(directory, entry.name)) : [join(directory, entry.name)],
  );
}

function run(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    shell: process.platform === "win32",
  });
}

// ---- Coverage -------------------------------------------------------------

if (withCoverage) {
  try {
    const report = JSON.parse(run("cargo", ["llvm-cov", "--workspace", "--json", "--summary-only"]));
    metrics.rustLineCoverage = report.data[0].totals.lines.percent;
  } catch (error) {
    console.warn(`Rust coverage skipped: ${error.message}`);
  }
  try {
    run("npx", ["vitest", "run", "--coverage", "--silent"]);
  } catch (error) {
    console.warn(`TS coverage failed: ${error.message}`);
  }
}
const tsSummary = join(root, "coverage", "coverage-summary.json");
if (existsSync(tsSummary)) {
  metrics.tsLineCoverage = JSON.parse(readFileSync(tsSummary, "utf8")).total.lines.pct;
}

// ---- Sizes ----------------------------------------------------------------

const assets = files(join(root, "dist", "assets")).filter((path) => /\.(js|css)$/.test(path));
if (assets.length) {
  metrics.bundleBytes = assets.reduce((sum, path) => sum + statSync(path).size, 0);
  metrics.bundleGzipBytes = assets.reduce((sum, path) => sum + gzipSync(readFileSync(path)).length, 0);
}

const release = join(root, "target", "release");
metrics.daemonBytes = size(join(release, `werd-daemon${exe}`));
metrics.cliBytes = size(join(release, `werd${exe}`));
metrics.desktopBytes = size(join(release, `werd-desktop${exe}`));
const installers = files(join(release, "bundle")).filter((path) => /\.(exe|msi|dmg|AppImage|deb|rpm)$/.test(path));
metrics.installers = Object.fromEntries(
  installers.map((path) => [path.slice(release.length + 1).replaceAll("\\", "/"), statSync(path).size]),
);

// ---- Daemon startup time and idle memory ---------------------------------

function ping(endpoint) {
  return new Promise((resolve, reject) => {
    const socket = connect(endpoint.port, "127.0.0.1");
    let data = "";
    socket.setTimeout(1000, () => socket.destroy(new Error("timeout")));
    socket.on("connect", () =>
      socket.write(`${JSON.stringify({ token: endpoint.token, method: "ping", params: {} })}\n`),
    );
    socket.on("data", (chunk) => {
      data += chunk;
      if (data.includes("\n")) {
        socket.end();
        resolve(JSON.parse(data).ok === true);
      }
    });
    socket.on("error", reject);
  });
}

function residentBytes(pid) {
  if (process.platform === "win32") {
    const out = run("powershell", ["-NoProfile", "-Command", `(Get-Process -Id ${pid}).WorkingSet64`]);
    return Number(out.trim());
  }
  return Number(run("ps", ["-o", "rss=", "-p", String(pid)]).trim()) * 1024;
}

async function measureDaemon(binary) {
  const home = mkdtempSync(join(tmpdir(), "werd-metrics-"));
  const started = performance.now();
  const child = spawn(binary, [], { env: { ...process.env, WERD_HOME: home }, stdio: "ignore" });
  try {
    const endpointFile = join(home, "daemon.json");
    while (performance.now() - started < 15000) {
      if (existsSync(endpointFile)) {
        try {
          if (await ping(JSON.parse(readFileSync(endpointFile, "utf8")))) {
            const startupMs = performance.now() - started;
            await sleep(2000); // let it settle before sampling memory
            return { startupMs, idleRssBytes: residentBytes(child.pid) };
          }
        } catch {
          // endpoint written but not accepting yet
        }
      }
      await sleep(10);
    }
    throw new Error("daemon did not answer within 15s");
  } finally {
    child.kill();
    await sleep(200);
    rmSync(home, { recursive: true, force: true });
  }
}

const daemon = join(release, `werd-daemon${exe}`);
if (existsSync(daemon)) {
  try {
    Object.assign(metrics, await measureDaemon(daemon));
  } catch (error) {
    console.warn(`Daemon measurement skipped: ${error.message}`);
  }
}

// ---- Report ---------------------------------------------------------------

const rows = [
  ["Rust line coverage", pct(metrics.rustLineCoverage)],
  ["TypeScript line coverage", pct(metrics.tsLineCoverage)],
  ["Frontend bundle (js+css)", kib(metrics.bundleBytes)],
  ["Frontend bundle gzip", kib(metrics.bundleGzipBytes)],
  ["werd-daemon binary", mib(metrics.daemonBytes)],
  ["werd CLI binary", mib(metrics.cliBytes)],
  ["werd-desktop binary", mib(metrics.desktopBytes)],
  ...Object.entries(metrics.installers).map(([name, bytes]) => [`Installer ${name}`, mib(bytes)]),
  ["Daemon cold start (to first ping)", metrics.startupMs == null ? "n/a" : `${metrics.startupMs.toFixed(0)} ms`],
  ["Daemon idle memory (RSS)", mib(metrics.idleRssBytes)],
];
const markdown = [
  `## Werd metrics (${process.platform}-${process.arch})`,
  "",
  "| Metric | Value |",
  "| --- | --- |",
  ...rows.map(([name, value]) => `| ${name} | ${value} |`),
  "",
].join("\n");

const out = join(root, "metrics");
mkdirSync(out, { recursive: true });
writeFileSync(
  join(out, "metrics.json"),
  `${JSON.stringify({ platform: `${process.platform}-${process.arch}`, ...metrics }, null, 2)}\n`,
);
writeFileSync(join(out, "metrics.md"), markdown);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${markdown}\n`);
console.log(markdown);
