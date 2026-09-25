import { invoke } from "@tauri-apps/api/core";

export type ServiceName = "postgres" | "redis" | "mailpit" | "rustfs";
export type ProjectStatus = "stopped" | "starting" | "running" | "error";

export interface Project {
  id: string;
  name: string;
  path: string;
  php: string;
  services: ServiceName[];
  status: ProjectStatus;
  url?: string;
  error?: string;
  ports?: Record<string, number>;
}

export interface Snapshot {
  projects: Project[];
  daemon_version: string;
}

export interface DoctorResult {
  label: string;
  ok: boolean;
  detail: string;
}

export interface RuntimeInfo {
  id: string;
  version: string;
  installed: boolean;
  note: string;
}

const desktop = "__TAURI_INTERNALS__" in window;

/** Browser-only sample data for UI work without the Rust daemon: open http://127.0.0.1:1420/?demo */
const demo = !desktop && new URLSearchParams(window.location.search).has("demo");
const demoProjects: Project[] = [
  {
    id: "shop",
    name: "shop",
    path: "C:\\Users\\dev\\Developer\\shop",
    php: "8.5",
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
    php: "8.5",
    services: ["postgres", "redis"],
    status: "error",
    error: "Port 52031 (postgres) is in use by another process. Stop it or reassign the project's ports",
  },
];

export async function listProjects(): Promise<Snapshot> {
  if (demo) return { projects: demoProjects, daemon_version: "0.1.1 (demo)" };
  if (!desktop) return { projects: [], daemon_version: "browser preview" };
  return invoke<Snapshot>("list_projects");
}

export async function addProject(path: string): Promise<Project> {
  if (!desktop) throw new Error("Open the desktop app to add a project.");
  return invoke<Project>("add_project", { path });
}

export async function startProject(id: string): Promise<Project> {
  if (!desktop) throw new Error("Open the desktop app to start a project.");
  return invoke<Project>("start_project", { id });
}

export async function stopProject(id: string): Promise<Project> {
  if (!desktop) throw new Error("Open the desktop app to stop a project.");
  return invoke<Project>("stop_project", { id });
}

export async function resetPorts(id: string): Promise<Project> {
  if (!desktop) throw new Error("Open the desktop app to reassign ports.");
  return invoke<Project>("reset_ports", { id });
}

export async function openSite(id: string): Promise<string> {
  if (!desktop) throw new Error("Open the desktop app to open the site.");
  return invoke<string>("open_site", { id });
}

export const REPOSITORY_URL = "https://github.com/JustFlavio/werd";

export async function openUrl(url: string): Promise<void> {
  if (!desktop) {
    window.open(url, "_blank");
    return;
  }
  return invoke<void>("open_url", { url });
}

export async function projectLogs(id: string, service = "werd"): Promise<string[]> {
  if (demo)
    return [
      `[12:04:10] ${id}: starting ${service}`,
      `[12:04:11] ${id}: ready`,
      `[12:04:11] Site started: https://localhost:52011`,
    ];
  if (!desktop) return [];
  return invoke<string[]>("project_logs", { id, service });
}

export async function doctor(): Promise<DoctorResult[]> {
  if (demo)
    return [
      { label: "Werd daemon", ok: true, detail: "The local daemon is responding" },
      { label: "Web port 443", ok: true, detail: "Free" },
      { label: "pgvector", ok: false, detail: "Needs Visual Studio Build Tools" },
    ];
  if (!desktop) return [];
  return invoke<DoctorResult[]>("doctor");
}

export async function runtimes(): Promise<RuntimeInfo[]> {
  if (demo)
    return [
      { id: "php", version: "8.5.11", installed: true, note: "Official PHP NTS build" },
      { id: "caddy", version: "2.11.4", installed: true, note: "Local HTTPS server" },
      { id: "postgres", version: "18.6", installed: true, note: "EDB binaries" },
      {
        id: "pgvector",
        version: "0.8.6",
        installed: false,
        note: "Built on demand; needs Visual Studio Build Tools",
      },
      { id: "redis", version: "7.2.8", installed: true, note: "Community Windows port" },
      { id: "mailpit", version: "1.31.2", installed: false, note: "Local SMTP and inbox" },
      { id: "rustfs", version: "1.0.0", installed: false, note: "Local S3-compatible storage" },
    ];
  if (!desktop) return [];
  return invoke<RuntimeInfo[]>("runtimes");
}

export async function installRuntime(id: string): Promise<RuntimeInfo> {
  if (!desktop) throw new Error("Open the desktop app to install a runtime.");
  return invoke<RuntimeInfo>("install_runtime", { id });
}

export async function projectEnv(id: string): Promise<string> {
  if (demo && id === "shop")
    return "DB_CONNECTION=pgsql\nDB_HOST=127.0.0.1\nDB_PORT=52013\nREDIS_HOST=127.0.0.1\nREDIS_PORT=52014\nMAIL_MAILER=smtp\nMAIL_HOST=127.0.0.1\nMAIL_PORT=52015";
  if (!desktop) return "";
  return invoke<string>("project_env", { id });
}

export async function trustCa(): Promise<string> {
  if (!desktop) throw new Error("Open the desktop app to trust the local certificate.");
  return invoke<string>("trust_ca");
}
