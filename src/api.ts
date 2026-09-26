import { invoke } from "@tauri-apps/api/core";
import { demoRpc } from "./demo";

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
  versions?: Record<string, string>;
  extensions?: string[];
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

export type ProductKind = "runtime" | "service" | "tool" | "extension";

/** One version line of a catalog product, with its install state. */
export interface RuntimeLine {
  product: string;
  label: string;
  kind: ProductKind;
  line: string;
  latest: string | null;
  installed: string | null;
  update_available: boolean;
  is_default: boolean;
  lts: boolean;
  eol: string | null;
}

export type JobState = "running" | "done" | "failed";

export interface Job {
  id: string;
  product: string;
  line: string;
  action: string;
  state: JobState;
  downloaded: number;
  total: number | null;
  step: string;
  error: string | null;
  started_at: number;
}

export interface Settings {
  layout_version: number;
  default_php: string | null;
  default_node: string | null;
  upload_max_mb: number;
  memory_limit_mb: number;
  catalog_url: string | null;
  path_enabled: boolean;
}

export interface SystemInfo {
  version: string;
  protocol: number;
  home: string;
  bin: string;
  platform: string;
  catalog_generated: string | null;
  catalog_refreshable: boolean;
  path_enabled: boolean;
}

const desktop = "__TAURI_INTERNALS__" in window;

/** Browser-only sample backend for UI work without the Rust daemon: open http://127.0.0.1:1420/?demo */
export const demo = !desktop && new URLSearchParams(window.location.search).has("demo");

/** Calls a daemon method. Every other function in this file is a typed wrapper around it. */
export async function rpc<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  if (demo) return demoRpc(method, params) as Promise<T>;
  if (!desktop) throw new Error("Open the Werd desktop app, or add ?demo to the URL for sample data.");
  return invoke<T>("rpc", { method, params });
}

// ---- Sites -------------------------------------------------------------------

export const listProjects = () => rpc<Snapshot>("sites.list");
export const addProject = (path: string) => rpc<Project>("sites.add", { path });
export const startProject = (id: string) => rpc<Project>("sites.start", { id });
export const stopProject = (id: string) => rpc<Project>("sites.stop", { id });
export const resetPorts = (id: string) => rpc<Project>("sites.reset-ports", { id });
export const openSite = (id: string) => rpc<string>("sites.open", { id });
export const projectEnv = (id: string) => rpc<string>("sites.env", { id });
export const projectLogs = (id: string, service = "werd") => rpc<string[]>("sites.logs", { id, service });

// ---- Runtimes, jobs, settings ------------------------------------------------

export const listRuntimes = () => rpc<RuntimeLine[]>("runtimes.list");
export const installRuntime = (product: string, line: string) => rpc<Job>("runtimes.install", { product, line });
export const updateRuntime = (product: string, line: string) => rpc<Job>("runtimes.update", { product, line });
export const uninstallRuntime = (product: string, line: string) => rpc<null>("runtimes.uninstall", { product, line });
export const setDefaultRuntime = (product: string, line: string) =>
  rpc<Settings>("runtimes.default", { product, line });
export const listJobs = () => rpc<Job[]>("jobs.list");
export const getSettings = () => rpc<Settings>("settings.get");
export const updateSettings = (changes: Partial<Pick<Settings, "upload_max_mb" | "memory_limit_mb">>) =>
  rpc<Settings>("settings.set", changes);

// ---- Service instances ---------------------------------------------------------

export interface ServiceInstance {
  id: string;
  name: string;
  product: string;
  line: string;
  port: number;
  extra_ports?: Record<string, number>;
  autostart: boolean;
  extensions?: string[];
  status: ProjectStatus;
  error?: string | null;
  web_ui?: string | null;
}

export interface ServiceOffering {
  product: string;
  label: string;
  categories: string[];
  default_port: number | null;
  extensions: string[];
  lines: { line: string; latest: string; lts: boolean; eol: string | null; installed: string | null }[];
}

export interface ServiceDetails {
  instance: ServiceInstance;
  credentials: { username: string; password: string } | null;
  web_ui: string | null;
  env: string;
}

export interface NewService {
  product: string;
  line: string;
  name?: string;
  port?: number;
  autostart: boolean;
  extensions: string[];
}

export const listServices = () => rpc<ServiceInstance[]>("services.list");
export const serviceCatalog = () => rpc<ServiceOffering[]>("services.catalog");
export const createService = (service: NewService) =>
  rpc<{ instance: ServiceInstance; job: Job | null }>("services.create", { ...service });
export const startService = (id: string) => rpc<ServiceInstance>("services.start", { id });
export const stopService = (id: string) => rpc<ServiceInstance>("services.stop", { id });
export const deleteService = (id: string, keepData: boolean) =>
  rpc<null>("services.delete", { id, keep_data: keepData });
export const setServiceAutostart = (id: string, autostart: boolean) =>
  rpc<ServiceInstance>("services.autostart", { id, autostart });
export const serviceDetails = (id: string) => rpc<ServiceDetails>("services.details", { id });
export const serviceLogs = (id: string) => rpc<string[]>("services.logs", { id });

// ---- System ------------------------------------------------------------------

export const systemInfo = () => rpc<SystemInfo>("system.info");
export const refreshCatalog = () => rpc<{ generated: string }>("catalog.refresh");
export const enablePath = () => rpc<Settings>("path.enable");
export const disablePath = () => rpc<Settings>("path.disable");
export const doctor = () => rpc<DoctorResult[]>("doctor");
export const trustCa = () => rpc<string>("trust-ca");

export const REPOSITORY_URL = "https://github.com/JustFlavio/werd";

export async function openUrl(url: string): Promise<void> {
  if (!desktop) {
    window.open(url, "_blank");
    return;
  }
  return invoke<void>("open_url", { url });
}
