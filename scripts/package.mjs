#!/usr/bin/env node
// Builds Werd's sidecars (daemon, CLI, shim, helper) and, unless --sidecars-only,
// the installer for this platform.
//
//   node scripts/package.mjs                 # Windows: NSIS, macOS: dmg, Linux: deb + AppImage
//   node scripts/package.mjs --sidecars-only # CI: tauri-action builds the bundle afterwards
//
// Set TAURI_SIGNING_PRIVATE_KEY (and _PASSWORD) to sign update packages.

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const windows = process.platform === "win32";
const sidecarsOnly = process.argv.includes("--sidecars-only");
const SIDECARS = [
  ["werd-daemon", "werd-core"],
  ["werd", "werd-cli"],
  ["werd-shim", "werd-shim"],
  ["werd-helper", "werd-helper"],
];
const CONFIGS = {
  win32: "src-tauri/tauri.nsis.conf.json",
  darwin: "src-tauri/tauri.dmg.conf.json",
  linux: "src-tauri/tauri.linux.conf.json",
};

function run(command, args, options = {}) {
  execFileSync(command, args, { cwd: root, stdio: "inherit", shell: windows, ...options });
}

/** Local Windows builds need the SDK resource compiler on PATH; CI runners have it. */
function windowsResourceCompiler(env) {
  if (!windows || env.RC) return env;
  const kits = "C:\\Program Files (x86)\\Windows Kits\\10\\bin";
  if (!existsSync(kits)) return env;
  const versions = readdirSync(kits)
    .filter((name) => existsSync(join(kits, name, "x64", "rc.exe")))
    .sort();
  const latest = versions.at(-1);
  if (!latest) return env;
  const dir = join(kits, latest, "x64");
  return { ...env, RC: join(dir, "rc.exe"), PATH: `${dir};${env.PATH ?? env.Path ?? ""}` };
}

const triple = execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const env = windowsResourceCompiler({ ...process.env });
run("cargo", ["build", "--release", ...SIDECARS.flatMap(([, crate]) => ["-p", crate])], { env });

const suffix = windows ? ".exe" : "";
const binaries = join(root, "src-tauri", "binaries");
mkdirSync(binaries, { recursive: true });
for (const [name] of SIDECARS) {
  copyFileSync(join(root, "target", "release", `${name}${suffix}`), join(binaries, `${name}-${triple}${suffix}`));
}
console.log(`Sidecars ready for ${triple}`);

if (!sidecarsOnly) {
  const config = CONFIGS[process.platform];
  if (!config) throw new Error(`Unsupported platform: ${process.platform}`);
  run(join("node_modules", ".bin", windows ? "tauri.cmd" : "tauri"), ["build", "--config", config], { env });
  console.log(`Installer ready in target/release/bundle`);
}
