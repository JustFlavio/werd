import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.([1-9]\d*))?$/;

export function newerVersion(left, right) {
  const parts = (version) => {
    const match = VERSION.exec(version);
    if (!match) throw new Error(`Invalid channel version: ${version}`);
    return [
      Number(match[1]),
      Number(match[2]),
      Number(match[3]),
      { alpha: 0, beta: 1, rc: 2 }[match[4]] ?? 3,
      Number(match[5] ?? 0),
    ];
  };
  const a = parts(left);
  const b = parts(right);
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return false;
}
export const PLATFORMS = {
  windows: {
    platform: "windows",
    os: "windows-latest",
    config: "src-tauri/tauri.nsis.conf.json",
    updater: "windows-x86_64",
    installers: ["_x64-setup.exe"],
  },
  "macos-arm64": {
    platform: "macos-arm64",
    os: "macos-15",
    config: "src-tauri/tauri.dmg.conf.json",
    updater: "darwin-aarch64",
    installers: ["_aarch64.dmg"],
  },
  "macos-x64": {
    platform: "macos-x64",
    os: "macos-15-intel",
    config: "src-tauri/tauri.dmg.conf.json",
    updater: "darwin-x86_64",
    installers: ["_x64.dmg"],
  },
  linux: {
    platform: "linux",
    os: "ubuntu-22.04",
    config: "src-tauri/tauri.deb.conf.json",
    updater: "linux-x86_64",
    installers: ["_amd64.deb", "_amd64.AppImage"],
  },
};

export function planRelease(version, tag, platforms = "windows") {
  if (!VERSION.test(version)) throw new Error(`Unsupported application version: ${version}`);
  if (tag !== `v${version}`) throw new Error(`Tag ${tag} does not match manifest version v${version}`);
  const names = [
    ...new Set(
      platforms.split(",").flatMap((name) => (name.trim() === "macos" ? ["macos-arm64", "macos-x64"] : [name.trim()])),
    ),
  ];
  if (!names.length || names.some((name) => !Object.hasOwn(PLATFORMS, name)))
    throw new Error(`Unknown or empty release platforms: ${platforms}`);
  const prerelease = version.includes("-");
  if (!prerelease && Object.keys(PLATFORMS).some((name) => !names.includes(name)))
    throw new Error("Stable releases require Windows, both macOS architectures and Linux");
  return {
    version,
    tag,
    prerelease,
    channel: prerelease ? "preview" : "stable",
    matrix: { include: names.map((name) => PLATFORMS[name]) },
  };
}

export function checkVersions(root) {
  const json = (path) => JSON.parse(readFileSync(resolve(root, path), "utf8"));
  const version = json("package.json").version;
  const lock = json("package-lock.json");
  const cargo = readFileSync(resolve(root, "Cargo.toml"), "utf8").match(
    /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/,
  )?.[1];
  const entries = readFileSync(resolve(root, "Cargo.lock"), "utf8")
    .split("[[package]]")
    .flatMap((entry) => {
      const name = entry.match(/\nname = "(werd-[^"]+)"/)?.[1];
      return name ? [[name, entry.match(/\nversion = "([^"]+)"/)?.[1]]] : [];
    });
  const versions = [
    ["package-lock", lock.version],
    ["package-lock root", lock.packages?.[""].version],
    ["Tauri", json("src-tauri/tauri.conf.json").version],
    ["Cargo workspace", cargo],
    ...entries,
  ];
  if (entries.length !== 5) throw new Error("Expected five Werd packages in Cargo.lock");
  for (const [name, actual] of versions)
    if (actual !== version) throw new Error(`${name} version ${actual} differs from ${version}`);
  if (!VERSION.test(version)) throw new Error(`Unsupported application version: ${version}`);
  return version;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const root = fileURLToPath(new URL("../../", import.meta.url));
  const version = checkVersions(root);
  if (process.argv.includes("--check")) {
    console.log(`All manifests agree: ${version}`);
  } else {
    const plan = planRelease(version, process.env.RELEASE_TAG, process.env.PLATFORMS ?? "windows");
    const tagged = execFileSync("git", ["rev-parse", `refs/tags/${plan.tag}^{commit}`], {
      cwd: root,
      encoding: "utf8",
    }).trim();
    const head = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
    if (tagged !== head) throw new Error("Release checkout must match the application tag");
    for (const [key, value] of Object.entries(plan)) {
      const line = `${key}=${typeof value === "object" ? JSON.stringify(value) : value}\n`;
      if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, line);
      else process.stdout.write(line);
    }
  }
}
