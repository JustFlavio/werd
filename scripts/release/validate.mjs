import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { planRelease } from "./plan.mjs";

export function validateRelease(manifest, release, plan, repository) {
  if (manifest.version !== plan.version || release.tag_name !== plan.tag)
    throw new Error("Release, updater and application versions must agree");
  if (release.prerelease !== plan.prerelease) throw new Error("Incorrect prerelease classification");
  const assets = new Map(release.assets.map((asset) => [asset.name, asset]));
  if (!assets.has("latest.json")) throw new Error("Missing updater manifest");
  for (const platform of plan.matrix.include) {
    for (const suffix of platform.installers) {
      if (![...assets.keys()].some((name) => name.startsWith(`Werd_${plan.version}_`) && name.endsWith(suffix)))
        throw new Error(`Missing ${platform.platform} installer ${suffix}`);
    }
    const update = manifest.platforms?.[platform.updater];
    if (!update?.signature?.trim() || !update.url)
      throw new Error(`Missing signed updater entry for ${platform.updater}`);
    const prefix = `https://github.com/${repository}/releases/download/${plan.tag}/`;
    if (!update.url.startsWith(prefix)) throw new Error(`Updater URL is outside this release: ${update.url}`);
    const name = decodeURIComponent(update.url.slice(prefix.length));
    if (!assets.has(name) || !assets.has(`${name}.sig`))
      throw new Error(`Missing updater bundle or signature: ${name}`);
    if (assets.get(name).size <= 0 || assets.get(`${name}.sig`).size <= 0)
      throw new Error(`Empty updater asset: ${name}`);
  }
  console.log(`Validated ${plan.tag}: ${plan.matrix.include.map((entry) => entry.platform).join(", ")}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  const [manifestPath, releasePath] = process.argv.slice(2);
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const releases = JSON.parse(readFileSync(releasePath, "utf8"));
  const release = Array.isArray(releases)
    ? releases.flat().find((entry) => entry.tag_name === process.env.RELEASE_TAG)
    : releases;
  if (!release) throw new Error(`Draft release not found: ${process.env.RELEASE_TAG}`);
  validateRelease(
    manifest,
    release,
    planRelease(manifest.version, process.env.RELEASE_TAG, process.env.PLATFORMS),
    process.env.GITHUB_REPOSITORY,
  );
}
