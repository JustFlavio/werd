// Opt-in Windows SQL acceptance: downloads the requested Werd runtime into a
// fresh temporary home and exercises the actual CLI against a real engine.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import {
  appendFileSync,
  closeSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs, promisify } from "node:util";

const { values } = parseArgs({ options: { product: { type: "string" }, line: { type: "string" } } });
assert.equal(process.platform, "win32");
assert.ok(["mysql", "mariadb"].includes(values.product), "Pass --product mysql|mariadb and --line <catalog line>");
assert.match(values.line ?? "", /^\d+\.\d+$/);
const { product, line } = values;
const checkout = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const root = mkdtempSync(join(tmpdir(), `werd ${product} acceptance `));
const env = { ...process.env, WERD_HOME: root };
mkdirSync(join(root, "runtimes"));
writeFileSync(
  join(root, "config.json"),
  JSON.stringify({ layout_version: 3, setup_done: true, domains: false, parked: [] }),
);
const fd = openSync(join(root, "daemon.log"), "a");
const daemon = spawn(join(checkout, "target/debug/werd-daemon.exe"), [], {
  env,
  windowsHide: true,
  stdio: ["ignore", fd, fd],
});
closeSync(fd);
const execute = promisify(execFile);
const cli = async (...args) =>
  (
    await execute(join(checkout, "target/debug/werd.exe"), ["--json", ...args], {
      env,
      windowsHide: true,
      timeout: 15 * 60 * 1000,
      maxBuffer: 4 * 1024 * 1024,
    })
  ).stdout;
let instance;
let credentials;
const checks = [];
console.error(`Isolated acceptance home: ${root}`);

async function sql(database, statement) {
  const program = product === "mysql" ? "mysql.exe" : "mariadb.exe";
  const { stdout } = await execute(
    join(root, "runtimes", product, line, "bin", program),
    [
      "--no-defaults",
      "--protocol=TCP",
      "--host=127.0.0.1",
      `--port=${instance.port}`,
      `--user=${credentials.username}`,
      "--batch",
      "--skip-column-names",
      `--database=${database}`,
      "--execute",
      statement,
    ],
    { env: { ...env, MYSQL_PWD: credentials.password }, windowsHide: true, timeout: 30_000 },
  );
  return stdout.trim();
}

try {
  for (let attempt = 0; attempt < 100 && !existsSync(join(root, "daemon.json")); attempt++) {
    assert.equal(daemon.exitCode, null);
    await new Promise((done) => setTimeout(done, 100));
  }
  const details = JSON.parse(await cli("service", "add", `${product}@${line}`, "--name", "Acceptance SQL"));
  ({ instance, credentials } = details);
  assert.equal(instance.status, "running", instance.error);
  await cli("service", "db", instance.id, "acceptance_source");
  await sql(
    "acceptance_source",
    "CREATE TABLE sentinel (id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY, message TEXT); INSERT INTO sentinel(message) VALUES ('original sentinel'); CREATE VIEW sentinel_view AS SELECT message FROM sentinel; CREATE PROCEDURE sentinel_count() SELECT count(*) FROM sentinel;",
  );
  const backup = JSON.parse(await cli("service", "backup", instance.id, "acceptance_source"));
  assert.ok(backup.bytes > 0);
  await sql("acceptance_source", "UPDATE sentinel SET message='changed after backup';");
  const restored = JSON.parse(await cli("service", "restore", instance.id, backup.id, "acceptance_restored"));
  assert.equal(restored.database, "acceptance_restored");
  assert.equal(await sql("acceptance_restored", "SELECT message FROM sentinel_view;"), "original sentinel");
  assert.equal(await sql("acceptance_restored", "CALL sentinel_count();"), "1");
  await sql("acceptance_restored", "INSERT INTO sentinel(message) VALUES ('sequence restored');");
  assert.equal(await sql("acceptance_restored", "SELECT max(id) FROM sentinel;"), "2");
  assert.equal(await sql("acceptance_source", "SELECT message FROM sentinel;"), "changed after backup");
  checks.push("CLI backup/restore: tables, rows, view, routine, AUTO_INCREMENT and unchanged source");
  await assert.rejects(cli("service", "restore", instance.id, backup.id, "acceptance_restored"));
  assert.equal(await sql("acceptance_restored", "SELECT count(*) FROM sentinel;"), "2");
  checks.push("existing target refused and preserved");
  const path = join(root, "backups", backup.id, "database.dump");
  const original = readFileSync(path);
  appendFileSync(path, "corrupt");
  await assert.rejects(cli("service", "restore", instance.id, backup.id, "acceptance_corrupt"), /checksum mismatch/);
  assert.equal(
    await sql(
      "acceptance_source",
      "SELECT count(*) FROM information_schema.schemata WHERE schema_name='acceptance_corrupt';",
    ),
    "0",
  );
  writeFileSync(path, original);
  checks.push("checksum mismatch refused before creating a database");
  await cli("service", "stop", instance.id);
  await cli("service", "start", instance.id);
  assert.equal(await sql("acceptance_restored", "SELECT count(*) FROM sentinel;"), "2");
  assert.equal(JSON.parse(await cli("service", "backups")).length, 1);
  checks.push("service restart preserves data and backup listing");
  const installed = JSON.parse(readFileSync(join(root, "runtimes/installed.json"), "utf8"));
  console.log(
    JSON.stringify(
      { healthy: true, isolatedHome: root, product, line, version: installed[product][line].version, checks },
      null,
      2,
    ),
  );
} finally {
  if (instance) await cli("service", "stop", instance.id).catch(() => {});
  if (daemon.exitCode === null) {
    daemon.kill();
    await once(daemon, "exit");
  }
}
