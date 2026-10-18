// Opt-in real-engine acceptance. Only copies runtime binaries from --runtime-home;
// all configuration, databases, certificates, Composer state and projects use a
// fresh temporary directory. No hosts, PATH or system trust changes are made.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { once } from "node:events";
import {
  appendFileSync,
  closeSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import https from "node:https";
import net from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const { values } = parseArgs({ options: { "runtime-home": { type: "string" } } });
assert.equal(process.platform, "win32", "This acceptance harness currently targets Windows");
assert.ok(values["runtime-home"], "Pass --runtime-home pointing to installed Werd runtimes");
const checkout = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const source = resolve(values["runtime-home"]);
const installed = JSON.parse(readFileSync(join(source, "runtimes/installed.json"), "utf8"));
const root = mkdtempSync(join(tmpdir(), "werd acceptance "));
const env = {
  ...process.env,
  WERD_HOME: root,
  COMPOSER_HOME: join(root, "composer-home"),
  COMPOSER_CACHE_DIR: join(root, "composer-cache"),
};
const runtimes = {};
const lines = {};
for (const product of ["postgresql", "php", "caddy", "composer", "cacert", "phpredis", "phpmongodb"]) {
  const line = ["php", "phpredis", "phpmongodb"].includes(product)
    ? "8.4"
    : Object.keys(installed[product] ?? {}).sort((a, b) => b.localeCompare(a, undefined, { numeric: true }))[0];
  assert.ok(installed[product]?.[line], `Missing ${product} ${line ?? ""}`);
  cpSync(join(source, "runtimes", product, line), join(root, "runtimes", product, line), { recursive: true });
  runtimes[product] = { [line]: installed[product][line] };
  lines[product] = line;
}
writeFileSync(join(root, "runtimes/installed.json"), JSON.stringify(runtimes));
writeFileSync(
  join(root, "config.json"),
  JSON.stringify({ layout_version: 3, setup_done: true, domains: false, parked: [], default_php: lines.php }),
);
let daemon;
let endpoint;
let site;
let service;
const checks = [];
const pause = (ms) => new Promise((done) => setTimeout(done, ms));

async function run(program, args, extra = {}) {
  const fd = openSync(join(root, "commands.log"), "a");
  const child = spawn(program, args, { env: { ...env, ...extra }, windowsHide: true, stdio: ["ignore", fd, fd] });
  closeSync(fd);
  const timeout = setTimeout(
    () => {
      // This is the pid we just spawned, never an installed daemon or server.
      const killer = spawn("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        windowsHide: true,
        stdio: "ignore",
      });
      killer.on("error", () => child.kill());
    },
    15 * 60 * 1000,
  );
  try {
    const [code] = await once(child, "exit");
    assert.equal(code, 0, `Command failed; inspect ${join(root, "commands.log")}`);
  } finally {
    clearTimeout(timeout);
  }
}

async function startDaemon() {
  const fd = openSync(join(root, "daemon.log"), "a");
  daemon = spawn(join(checkout, "target/debug/werd-daemon.exe"), [], {
    env,
    windowsHide: true,
    stdio: ["ignore", fd, fd],
  });
  closeSync(fd);
  for (let attempt = 0; attempt < 150; attempt++) {
    assert.equal(daemon.exitCode, null, `Daemon exited; inspect ${root}`);
    if (existsSync(join(root, "daemon.json"))) {
      endpoint = JSON.parse(readFileSync(join(root, "daemon.json"), "utf8"));
      if (endpoint.pid === daemon.pid) return;
    }
    await pause(100);
  }
  throw new Error("Isolated daemon did not start");
}

function rpc(method, params = {}) {
  return new Promise((done, fail) => {
    const socket = net.createConnection({ host: "127.0.0.1", port: endpoint.port });
    let buffer = "";
    socket.setTimeout(90_000, () => socket.destroy(new Error(`RPC timed out: ${method}`)));
    socket.on("error", fail);
    socket.on("connect", () => socket.write(`${JSON.stringify({ token: endpoint.token, method, params })}\n`));
    socket.on("data", (chunk) => {
      buffer += chunk;
      if (buffer.length > 2_000_000) socket.destroy(new Error("Response too large"));
      if (!buffer.includes("\n")) return;
      socket.end();
      try {
        const response = JSON.parse(buffer.split("\n")[0]);
        if (!response.ok) throw new Error(response.error);
        done(response.data);
      } catch (error) {
        fail(error);
      }
    });
  });
}

async function job(method, params) {
  const started = await rpc(method, params);
  for (let attempt = 0; attempt < 3600; attempt++) {
    const current = (await rpc("jobs.list")).find((candidate) => candidate.id === started.id);
    assert.ok(current, "Job disappeared");
    if (current.state === "failed") throw new Error(current.error);
    if (current.state === "done") return current.result;
    await pause(250);
  }
  throw new Error("Database job exceeded acceptance deadline");
}

async function sql(database, query) {
  const { credentials } = await rpc("services.details", { id: service.id });
  await run(
    join(root, "runtimes/postgresql", lines.postgresql, "bin/psql.exe"),
    [
      "-X",
      "--set=ON_ERROR_STOP=1",
      "-h",
      "127.0.0.1",
      "-p",
      String(service.port),
      "-U",
      credentials.username,
      "-d",
      database,
      "-c",
      query,
    ],
    { PGPASSWORD: credentials.password },
  );
}

function probe(url) {
  return new Promise((done, fail) => {
    const certificate = readFileSync(join(root, "caddy-data/caddy/pki/authorities/local/root.crt"));
    const request = https.get(new URL("/werd-acceptance", url), { ca: certificate, family: 4 }, (response) => {
      let text = "";
      response.on("data", (chunk) => {
        text += chunk;
      });
      response.on("end", () => {
        try {
          assert.equal(response.statusCode, 200, text.slice(0, 200));
          done(JSON.parse(text));
        } catch (error) {
          fail(error);
        }
      });
    });
    request.setTimeout(20_000, () => request.destroy(new Error("Laravel request timed out")));
    request.on("error", fail);
  });
}

try {
  await startDaemon();
  service = (
    await rpc("services.create", {
      product: "postgresql",
      line: lines.postgresql,
      name: "Acceptance PostgreSQL",
      autostart: false,
      start: true,
    })
  ).instance;
  assert.equal(service.status, "running", service.error);
  await rpc("services.database", { id: service.id, name: "acceptance_source" });
  await sql(
    "acceptance_source",
    "CREATE TABLE werd_acceptance (id bigserial PRIMARY KEY, message text NOT NULL); INSERT INTO werd_acceptance(message) VALUES ('original sentinel');",
  );
  const backup = await job("backups.create", { id: service.id, database: "acceptance_source" });
  const before = await rpc("backups.info", { id: backup });
  assert.ok(before.bytes > 0);
  await sql("acceptance_source", "UPDATE werd_acceptance SET message = 'changed after backup';");
  await job("backups.restore", { id: service.id, backup, database: "acceptance_restored" });
  await sql(
    "acceptance_restored",
    "DO $$ BEGIN IF (SELECT message FROM werd_acceptance LIMIT 1) <> 'original sentinel' THEN RAISE EXCEPTION 'restore mismatch'; END IF; END $$; INSERT INTO werd_acceptance(message) VALUES ('sequence restored');",
  );
  await sql(
    "acceptance_source",
    "DO $$ BEGIN IF (SELECT message FROM werd_acceptance LIMIT 1) <> 'changed after backup' THEN RAISE EXCEPTION 'source changed'; END IF; END $$;",
  );
  checks.push("real PostgreSQL schema/data/sequence restore; source preserved");
  await assert.rejects(job("backups.restore", { id: service.id, backup, database: "acceptance_restored" }));
  await sql(
    "acceptance_restored",
    "DO $$ BEGIN IF (SELECT count(*) FROM werd_acceptance) <> 2 THEN RAISE EXCEPTION 'existing target changed'; END IF; END $$;",
  );
  checks.push("existing target refused");
  const data = join(root, "backups", backup, "database.dump");
  const original = readFileSync(data);
  appendFileSync(data, "tampered");
  await assert.rejects(
    job("backups.restore", { id: service.id, backup, database: "acceptance_corrupt" }),
    /checksum mismatch/,
  );
  await sql(
    "postgres",
    "DO $$ BEGIN IF EXISTS (SELECT 1 FROM pg_database WHERE datname='acceptance_corrupt') THEN RAISE EXCEPTION 'corrupt target created'; END IF; END $$;",
  );
  writeFileSync(data, original);
  checks.push("corrupted backup refused before database creation");

  const php = join(root, "runtimes/php", lines.php, "php.exe");
  const ini = join(root, "runtimes/php", lines.php, "php.ini");
  const composer = join(root, "runtimes/composer", lines.composer, "composer.phar");
  const project = join(root, "Laravel fixture");
  await run(php, [
    "-c",
    ini,
    composer,
    "create-project",
    "laravel/laravel:^13.0",
    project,
    "--prefer-dist",
    "--no-interaction",
    "--no-dev",
    "--no-scripts",
  ]);
  mkdirSync(join(project, "storage/framework/views"), { recursive: true });
  appendFileSync(
    join(project, "routes/web.php"),
    "\n\\Illuminate\\Support\\Facades\\Route::get('/werd-acceptance', fn () => ['laravel' => app()->version(), 'message' => \\Illuminate\\Support\\Facades\\DB::table('werd_acceptance')->orderBy('id')->value('message')]);\n",
  );
  site = await rpc("sites.add", { path: project, php: lines.php });
  await rpc("sites.link", { id: site.id, category: "database", instance: service.id, database: "acceptance_restored" });
  const block = await rpc("sites.env", { id: site.id });
  writeFileSync(
    join(project, ".env"),
    `APP_NAME=WerdAcceptance\nAPP_ENV=local\nAPP_DEBUG=false\nAPP_KEY=base64:${randomBytes(32).toString("base64")}\nLOG_CHANNEL=stderr\nSESSION_DRIVER=file\nCACHE_STORE=file\nQUEUE_CONNECTION=sync\n${block}\n`,
  );
  site = await rpc("sites.start", { id: site.id });
  assert.equal(site.status, "running", site.error);
  let response = await probe(site.url);
  assert.equal(response.message, "original sentinel");
  checks.push("real Laravel HTTPS request through Werd Caddy/PHP to restored PostgreSQL database");
  await rpc("sites.stop", { id: site.id });
  await rpc("services.stop", { id: service.id });
  daemon.kill();
  await once(daemon, "exit");
  await startDaemon();
  service = await rpc("services.start", { id: service.id });
  assert.equal(service.status, "running", service.error);
  site = await rpc("sites.start", { id: site.id });
  response = await probe(site.url);
  assert.equal(response.message, "original sentinel");
  assert.equal((await rpc("backups.list")).length, 1);
  checks.push("daemon/service/site restart preserves restored data and backup registry");
  console.log(
    JSON.stringify(
      {
        healthy: true,
        isolatedHome: root,
        runtimes,
        laravel: response.laravel,
        checks,
        note: "No system trust/hosts/PATH changes; no Herd dependency. MySQL/MariaDB, Vite and other OS are outside this run.",
      },
      null,
      2,
    ),
  );
} finally {
  if (daemon?.exitCode === null) {
    if (site) await rpc("sites.stop", { id: site.id }).catch(() => {});
    if (service) await rpc("services.stop", { id: service.id }).catch(() => {});
    daemon.kill();
    await once(daemon, "exit");
  }
}
