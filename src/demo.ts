// In-memory stand-in for the daemon, used by `?demo` in the browser.
// It keeps state for the session so installs, updates and defaults behave like the real app.

import type {
  DoctorResult,
  Job,
  Project,
  RuntimeLine,
  ServiceInstance,
  ServiceOffering,
  Settings,
  Snapshot,
  SystemInfo,
} from "./api";

const projects: Project[] = [
  {
    id: "shop",
    name: "shop",
    path: "C:\\Users\\dev\\Developer\\shop",
    php: "8.4",
    services: ["postgres", "redis", "mailpit", "rustfs"],
    status: "running",
    url: "https://localhost:52011",
    ports: {
      site: 52011,
      fastcgi: 52012,
      postgres: 52013,
      redis: 52014,
      mailpit_smtp: 52015,
      mailpit_ui: 52016,
      rustfs_api: 52017,
      rustfs_console: 52018,
    },
    versions: { postgresql: "18", redis: "7.2" },
    extensions: ["pgvector"],
  },
  {
    id: "blog",
    name: "blog",
    path: "C:\\Users\\dev\\Developer\\blog",
    php: "8.5",
    services: ["postgres", "mailpit"],
    status: "stopped",
  },
  {
    id: "api",
    name: "billing-api",
    path: "C:\\Users\\dev\\Developer\\billing-api",
    php: "8.3",
    services: ["postgres", "redis"],
    status: "error",
    error: "Port 52031 (postgres) is in use by another process. Stop it or reassign the project's ports",
  },
];

type Row = Omit<RuntimeLine, "update_available" | "is_default">;
const row = (
  product: string,
  label: string,
  kind: RuntimeLine["kind"],
  line: string,
  latest: string,
  extra: Partial<Row> = {},
): Row => ({
  product,
  label,
  kind,
  line,
  latest,
  installed: null,
  lts: false,
  eol: null,
  ...extra,
});

const catalog: Row[] = [
  ...["8.5:8.5.11", "8.4:8.4.26", "8.3:8.3.35", "8.2:8.2.34", "8.1:8.1.34", "8.0:8.0.30", "7.4:7.4.33"].map((entry) => {
    const [line, latest] = entry.split(":");
    return row("php", "PHP", "runtime", line, latest);
  }),
  ...[
    "26:26.10.0",
    "25:25.9.0",
    "24:24.21.0:lts",
    "22:22.23.3:lts",
    "20:20.20.2:lts",
    "18:18.20.8:lts",
    "16:16.20.2:lts",
  ].map((entry) => {
    const [line, latest, lts] = entry.split(":");
    return row("node", "Node.js", "runtime", line, latest, { lts: lts === "lts" });
  }),
  row("composer", "Composer", "tool", "2", "2.10.3"),
  ...["18:18.6", "17:17.11", "16:16.15"].map((entry) => {
    const [line, latest] = entry.split(":");
    return row("postgresql", "PostgreSQL", "service", line, latest);
  }),
  row("redis", "Redis", "service", "7.2", "7.2.16"),
  row("mailpit", "Mailpit", "service", "1", "1.31.2"),
  row("rustfs", "RustFS", "service", "1", "1.0.0"),
];

const installed = new Map<string, string>([
  ["php/8.5", "8.5.11"],
  ["php/8.4", "8.4.25"],
  ["node/22", "22.23.3"],
  ["composer/2", "2.10.3"],
  ["postgresql/18", "18.6"],
  ["redis/7.2", "7.2.8"],
]);

const settings: Settings = {
  layout_version: 2,
  default_php: "8.5",
  default_node: "22",
  upload_max_mb: 100,
  memory_limit_mb: 512,
  catalog_url: null,
  path_enabled: false,
};

const jobs: Job[] = [];

const offerings: ServiceOffering[] = [
  ["mariadb", "MariaDB", "database", 3306, ["12.3:12.3.3", "11.8:11.8.9", "11.4:11.4.13"]],
  ["mongodb", "MongoDB", "database", 27017, ["8.2:8.2.12", "8.0:8.0.32", "7.0:7.0.43"]],
  ["mysql", "MySQL", "database", 3306, ["9.7:9.7.2", "8.4:8.4.11", "8.0:8.0.46"]],
  ["postgresql", "PostgreSQL", "database", 5432, ["18:18.6", "17:17.11", "16:16.15", "15:15.19", "14:14.24"]],
  ["redis", "Redis", "cache", 6379, ["8.10:8.10.2", "8.8:8.8.3", "7.4:7.4.11", "7.2:7.2.16"]],
  ["meilisearch", "Meilisearch", "search", 7700, ["1:1.54.0"]],
  ["rustfs", "RustFS", "storage", 9000, ["1:1.0.0"]],
  ["mailpit", "Mailpit", "mail", 1025, ["1:1.31.2"]],
].map(([product, label, category, port, lines]) => ({
  product: product as string,
  label: label as string,
  categories: category === "cache" ? ["cache", "queue"] : [category as string],
  default_port: port as number,
  extensions: product === "postgresql" ? ["pgvector"] : [],
  lines: (lines as string[]).map((entry) => {
    const [line, latest] = entry.split(":");
    return { line, latest, lts: false, eol: null, installed: installed.get(`${product}/${line}`) ?? null };
  }),
}));

const instances: ServiceInstance[] = [
  {
    id: "pg",
    name: "PostgreSQL 18",
    product: "postgresql",
    line: "18",
    port: 5432,
    autostart: true,
    extensions: ["pgvector"],
    status: "running",
  },
  { id: "redis", name: "Redis 7.2", product: "redis", line: "7.2", port: 6379, autostart: true, status: "running" },
  {
    id: "mail",
    name: "Mailpit",
    product: "mailpit",
    line: "1",
    port: 1025,
    extra_ports: { ui: 8025 },
    autostart: false,
    status: "stopped",
    web_ui: "http://127.0.0.1:8025",
  },
];

function findInstance(id: unknown) {
  const instance = instances.find((candidate) => candidate.id === id);
  if (!instance) throw new Error("Service not found");
  return instance;
}

function compare(a: string, b: string) {
  const left = a.split(".").map(Number);
  const right = b.split(".").map(Number);
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0);
    if (difference) return difference;
  }
  return 0;
}

function runtimes(): RuntimeLine[] {
  return catalog.map((entry) => {
    const current = installed.get(`${entry.product}/${entry.line}`) ?? null;
    const defaultLine =
      entry.product === "php" ? settings.default_php : entry.product === "node" ? settings.default_node : null;
    return {
      ...entry,
      installed: current,
      update_available: current !== null && entry.latest !== null && compare(current, entry.latest) < 0,
      is_default: defaultLine === entry.line,
    };
  });
}

/** Simulates a download: about three seconds, then the line is installed at the latest patch. */
function startJob(product: string, line: string, action: string): Job {
  const entry = catalog.find((candidate) => candidate.product === product && candidate.line === line);
  if (!entry) throw new Error(`Unknown ${product} version: ${line}`);
  if (jobs.some((job) => job.state === "running" && job.product === product && job.line === line)) {
    throw new Error(`${product} ${line} is already being installed or updated`);
  }
  const total = 30 * 1024 * 1024;
  const job: Job = {
    id: crypto.randomUUID(),
    product,
    line,
    action,
    state: "running",
    downloaded: 0,
    total,
    step: "Downloading",
    error: null,
    started_at: Date.now() / 1000,
  };
  jobs.push(job);
  const timer = window.setInterval(() => {
    job.downloaded = Math.min(total, job.downloaded + total / 12);
    if (job.downloaded >= total) {
      window.clearInterval(timer);
      job.step = "Done";
      job.state = "done";
      installed.set(`${product}/${line}`, entry.latest ?? line);
    }
  }, 250);
  return { ...job };
}

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export async function demoRpc(method: string, params: Record<string, unknown>): Promise<unknown> {
  await delay(60);
  const product = String(params.product ?? "");
  const line = String(params.line ?? "");
  switch (method) {
    case "sites.list":
      return { projects, daemon_version: "0.2.0 (demo)" } satisfies Snapshot;
    case "sites.logs":
      return [
        `[12:04:10] ${params.id}: starting ${params.service}`,
        `[12:04:11] ${params.id}: ready`,
        "[12:04:11] Site started: https://localhost:52011",
      ];
    case "sites.env":
      return params.id === "shop"
        ? "DB_CONNECTION=pgsql\nDB_HOST=127.0.0.1\nDB_PORT=52013\nREDIS_HOST=127.0.0.1\nREDIS_PORT=52014\nMAIL_MAILER=smtp\nMAIL_HOST=127.0.0.1\nMAIL_PORT=52015"
        : "";
    case "runtimes.list":
      return runtimes();
    case "runtimes.install":
      return startJob(product, line, "install");
    case "runtimes.update":
      if (!installed.has(`${product}/${line}`)) throw new Error(`${product} ${line} is not installed`);
      return startJob(product, line, "update");
    case "runtimes.uninstall":
      if (product === "php" && projects.some((project) => project.status === "running" && project.php === line)) {
        throw new Error(`Stop shop before removing php ${line}`);
      }
      installed.delete(`${product}/${line}`);
      if (product === "php" && settings.default_php === line) settings.default_php = null;
      if (product === "node" && settings.default_node === line) settings.default_node = null;
      return null;
    case "runtimes.default":
      if (!installed.has(`${product}/${line}`))
        throw new Error(`Install ${product} ${line} before making it the default`);
      if (product === "php") settings.default_php = line;
      if (product === "node") settings.default_node = line;
      return { ...settings };
    case "jobs.list":
      return jobs.map((job) => ({ ...job }));
    case "settings.get":
      return { ...settings };
    case "settings.set":
      Object.assign(settings, params);
      return { ...settings };
    case "system.info":
      return {
        version: "0.2.0 (demo)",
        protocol: 2,
        home: "C:\\Users\\dev\\AppData\\Local\\Werd",
        bin: "C:\\Users\\dev\\AppData\\Local\\Werd\\bin",
        platform: "windows-x64",
        catalog_generated: "2026-09-25T23:54:42Z",
        catalog_refreshable: false,
        path_enabled: settings.path_enabled,
      } satisfies SystemInfo;
    case "path.enable":
    case "path.disable":
      settings.path_enabled = method === "path.enable";
      return { ...settings };
    case "catalog.refresh":
      throw new Error("Online catalog updates are not available yet; update Werd to get newer runtimes");
    case "services.list":
      return instances.map((instance) => ({ ...instance }));
    case "services.catalog":
      return offerings;
    case "services.create": {
      const offering = offerings.find((candidate) => candidate.product === product);
      if (!offering) throw new Error(`Werd cannot run ${product} as a service yet`);
      const taken = instances.flatMap((instance) => [instance.port, ...Object.values(instance.extra_ports ?? {})]);
      let port = Number(params.port) || offering.default_port || 10000;
      if (params.port && taken.includes(port)) throw new Error(`Port ${port} is already used by another Werd service`);
      while (taken.includes(port)) port++;
      const instance: ServiceInstance = {
        id: crypto.randomUUID(),
        name: String(params.name || `${offering.label} ${line}`),
        product,
        line,
        port,
        autostart: Boolean(params.autostart),
        extensions: (params.extensions as string[]) ?? [],
        status: "stopped",
        web_ui: product === "mailpit" ? "http://127.0.0.1:8026" : null,
      };
      instances.push(instance);
      if (installed.has(`${product}/${line}`)) {
        instance.status = "running";
        return { instance, job: null };
      }
      const job = startJob(product, line, "install");
      const timer = window.setInterval(() => {
        if (installed.has(`${product}/${line}`)) {
          window.clearInterval(timer);
          instance.status = "running";
        }
      }, 200);
      return { instance, job };
    }
    case "services.start":
      findInstance(params.id).status = "running";
      return findInstance(params.id);
    case "services.stop":
      findInstance(params.id).status = "stopped";
      return findInstance(params.id);
    case "services.autostart":
      findInstance(params.id).autostart = Boolean(params.autostart);
      return findInstance(params.id);
    case "services.delete":
      instances.splice(instances.indexOf(findInstance(params.id)), 1);
      return null;
    case "services.details": {
      const instance = findInstance(params.id);
      const isDatabase = ["postgresql", "mysql", "mariadb"].includes(instance.product);
      return {
        instance,
        credentials: isDatabase ? { username: "werd", password: "4f1c9a7e2b8d4c6fa0e3b5d7c9e1f2a4" } : null,
        web_ui: instance.web_ui ?? null,
        env: isDatabase
          ? `DB_CONNECTION=${instance.product === "postgresql" ? "pgsql" : instance.product}\nDB_HOST=127.0.0.1\nDB_PORT=${instance.port}\nDB_DATABASE=laravel\nDB_USERNAME=werd\nDB_PASSWORD=4f1c9a7e2b8d4c6fa0e3b5d7c9e1f2a4`
          : `REDIS_HOST=127.0.0.1\nREDIS_PORT=${instance.port}`,
      };
    }
    case "services.logs":
      return [`[12:00:01] ${findInstance(params.id).name} ready to accept connections`];
    case "doctor":
      return [
        { label: "Werd daemon", ok: true, detail: "Running, data folder C:\\Users\\dev\\AppData\\Local\\Werd" },
        { label: "PHP 8.4", ok: true, detail: "8.4.25 installed" },
        { label: "PostgreSQL 18", ok: false, detail: "18.6 recorded but files are missing; reinstall it" },
      ] satisfies DoctorResult[];
    case "trust-ca":
      return "Werd's local CA was added to the current user's trusted roots";
    default:
      throw new Error(`Not available in the demo: ${method}`);
  }
}
