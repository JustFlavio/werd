#!/usr/bin/env node
// Sets one version everywhere it lives: package.json, package-lock.json,
// the Cargo workspace (and Cargo.lock) and tauri.conf.json.
//
//   npm run version:bump -- 0.2.0
//
// Files are written as UTF-8 without BOM: Vite and Cargo reject a BOM.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const version = process.argv[2];

if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("Usage: npm run version:bump -- <major.minor.patch[-pre]>");
  process.exit(1);
}

function edit(relative, transform) {
  const path = join(root, relative);
  const before = readFileSync(path, "utf8");
  const after = transform(before);
  if (after === before) throw new Error(`${relative}: version field not found`);
  writeFileSync(path, after);
  console.log(`updated ${relative}`);
}

const updateJson = (relative, apply) =>
  edit(relative, (text) => {
    const data = JSON.parse(text);
    apply(data);
    return `${JSON.stringify(data, null, 2)}\n`;
  });

updateJson("package.json", (data) => {
  data.version = version;
});
updateJson("package-lock.json", (data) => {
  data.version = version;
  data.packages[""].version = version;
});
updateJson("src-tauri/tauri.conf.json", (data) => {
  data.version = version;
});
edit("Cargo.toml", (text) => text.replace(/(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/, `$1${version}$2`));

// Refresh the workspace entries in Cargo.lock without touching dependencies.
execFileSync("cargo", ["update", "--workspace", "--offline"], { cwd: root, stdio: "inherit" });
console.log(`\nVersion set to ${version}. Next: commit "chore(release): v${version}" and tag v${version}.`);
