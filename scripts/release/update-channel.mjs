// The updates branch stores channel manifests without adding channel tags or app releases.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { newerVersion, VERSION } from "./plan.mjs";

const repository = process.env.GITHUB_REPOSITORY;
const channel = process.env.CHANNEL;
if (!repository || !["preview", "stable"].includes(channel)) throw new Error("Repository and channel are required");
const content = readFileSync(process.argv[2], "utf8");
const incoming = JSON.parse(content);
if (!VERSION.test(incoming.version) || (channel === "stable" && incoming.version.includes("-")))
  throw new Error("Invalid channel version");
const api = (path, ...args) =>
  JSON.parse(execFileSync("gh", ["api", `repos/${repository}/${path}`, ...args], { encoding: "utf8" }));
function optional(path) {
  try {
    return api(path);
  } catch (error) {
    if (error.stderr?.toString().includes("HTTP 404")) return undefined;
    throw error;
  }
}

if (!optional("git/ref/heads/updates")) {
  api("git/refs", "--method", "POST", "-f", "ref=refs/heads/updates", "-f", `sha=${process.env.GITHUB_SHA}`);
}
// Stable graduates preview users too, unless a newer preview is already published.
for (const target of channel === "stable" ? ["stable", "preview"] : ["preview"]) {
  const previous = optional(`contents/${target}.json?ref=updates`);
  if (previous) {
    const old = JSON.parse(Buffer.from(previous.content, "base64").toString("utf8"));
    if (newerVersion(old.version, incoming.version)) {
      if (target === "preview" && channel === "stable") continue;
      throw new Error(`Refusing to move ${target} from ${old.version} back to ${incoming.version}`);
    }
  }
  api(
    `contents/${target}.json`,
    "--method",
    "PUT",
    "-f",
    "branch=updates",
    "-f",
    `message=chore(release): update ${target} to ${incoming.version}`,
    "-f",
    `content=${Buffer.from(content).toString("base64")}`,
    ...(previous ? ["-f", `sha=${previous.sha}`] : []),
  );
  console.log(`${target} now points to ${incoming.version}`);
}
