import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { checkVersions, newerVersion, planRelease } from "./plan.mjs";
import { validateRelease } from "./validate.mjs";

test("all actual application manifests agree", () => {
  checkVersions(fileURLToPath(new URL("../../", import.meta.url)));
});
test("preview stages are prereleases and must match the tag", () => {
  for (const stage of ["alpha", "beta", "rc"])
    assert.equal(planRelease(`0.1.0-${stage}.1`, `v0.1.0-${stage}.1`).prerelease, true);
  assert.throws(() => planRelease("0.1.0-alpha.1", "v0.3.0-rc.4"), /does not match/);
  assert.throws(() => planRelease("0.1.0-preview.1", "v0.1.0-preview.1"), /Unsupported/);
});
test("stable requires every declared platform; platform mistakes cannot be ignored", () => {
  assert.throws(() => planRelease("0.1.0", "v0.1.0", "windows"), /Stable releases require/);
  assert.throws(() => planRelease("0.1.0-alpha.1", "v0.1.0-alpha.1", "windows,linx"), /Unknown/);
  assert.throws(() => planRelease("0.1.0-alpha.1", "v0.1.0-alpha.1", ""), /Unknown/);
  const stable = planRelease("0.1.0", "v0.1.0", "windows,macos,linux");
  assert.equal(stable.channel, "stable");
  assert.equal(stable.matrix.include.length, 4);
});

test("channels advance by SemVer order, including numerical preview counters", () => {
  const sequence = [
    "0.1.0-alpha.1",
    "0.1.0-alpha.2",
    "0.1.0-alpha.10",
    "0.1.0-beta.1",
    "0.1.0-rc.1",
    "0.1.0",
    "0.1.1-alpha.1",
    "0.2.0",
    "1.0.0",
  ];
  for (let i = 1; i < sequence.length; i++) {
    assert.equal(newerVersion(sequence[i], sequence[i - 1]), true);
    assert.equal(newerVersion(sequence[i - 1], sequence[i]), false);
  }
  assert.equal(newerVersion("0.1.0", "0.1.0"), false);
});

function fixture() {
  const plan = planRelease("0.1.0-alpha.1", "v0.1.0-alpha.1");
  const name = "Werd_0.1.0-alpha.1_x64-setup.exe";
  const release = {
    tag_name: plan.tag,
    prerelease: true,
    assets: ["latest.json", name, `${name}.sig`].map((name) => ({ name, size: 100 })),
  };
  const manifest = {
    version: plan.version,
    platforms: {
      "windows-x86_64": {
        url: `https://github.com/JustFlavio/werd/releases/download/${plan.tag}/${name}`,
        signature: "signed bundle",
      },
    },
  };
  return { plan, release, manifest };
}
test("complete signed installer passes publication validation", () => {
  const { manifest, release, plan } = fixture();
  validateRelease(manifest, release, plan, "JustFlavio/werd");
});
test("partial or mismatched releases never pass publication validation", () => {
  const { manifest, release, plan } = fixture();
  const validate = () => validateRelease(manifest, release, plan, "JustFlavio/werd");
  manifest.version = "0.3.0-rc.4";
  assert.throws(validate, /versions must agree/);
  manifest.version = plan.version;
  release.prerelease = false;
  assert.throws(validate, /prerelease/);
  release.prerelease = true;
  manifest.platforms["windows-x86_64"].url =
    "https://github.com/JustFlavio/werd-releases/releases/latest/download/setup.exe";
  assert.throws(validate, /outside this release/);
});
test("a missing signature or architecture blocks a complete-looking release", () => {
  const { manifest, release, plan } = fixture();
  release.assets.pop();
  assert.throws(
    () => validateRelease(manifest, release, plan, "JustFlavio/werd"),
    /Missing updater bundle or signature/,
  );
  delete manifest.platforms["windows-x86_64"];
  assert.throws(() => validateRelease(manifest, release, plan, "JustFlavio/werd"), /Missing signed updater entry/);
});
