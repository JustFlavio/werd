import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { demoRpc } from "./demo";

export type ProjectStatus = "stopped" | "starting" | "running" | "error";

/** Site categories that can be linked to a service instance. */
export type Category = "database" | "cache" | "queue" | "mail" | "storage" | "search";

export interface Link {
  instance: string;
  database?: string | null;
}

export interface Requirement {
  category: Category;
  product: string;
  line?: string | null;
  extensions?: string[];
}

export interface Project {
  id: string;
  name: string;
  path: string;
  /** `.test` domain, e.g. `shop.test`. */
  domain?: string | null;
  /** Start the site whenever Werd starts. */
  autostart?: boolean;
  vite?: ViteState;
  /** Parked folder the site comes from; such sites follow their folder. */
  parked?: string | null;
  php: string;
  node?: string | null;
  links?: Partial<Record<Category, Link>>;
  requirements?: Requirement[];
  status: ProjectStatus;
  url?: string;
  error?: string;
  ports?: Record<string, number>;
}

export interface Snapshot {
  projects: Project[];
  daemon_version: string;
}

export interface ViteState {
  available: boolean;
  autostart: boolean;
  status: ProjectStatus;
  error?: string | null;
  url?: string | null;
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
  /** Output of the commands the job runs (last lines). */
  log?: string[];
  /** Lines dropped before `log[0]`. */
  log_dropped?: number;
  /** What the job produced, e.g. the id of a created site. */
  result?: string | null;
}

export interface Settings {
  layout_version: number;
  default_php: string | null;
  default_node: string | null;
  upload_max_mb: number;
  memory_limit_mb: number;
  catalog_url: string | null;
  path_enabled: boolean;
  domains: boolean;
  https_port: number;
  parked: string[];
}

export interface DomainsStatus {
  enabled: boolean;
  https_port: number;
  /** Whether the shared Caddy currently serves `.test` domains. */
  active: boolean;
  /** Why domains are not served although enabled (e.g. port 443 busy). */
  warning: string | null;
  domains: string[];
  /** Site domains the hosts file does not map yet. */
  missing: string[];
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
export const addProject = (path: string, name?: string, php?: string, updateEnv = false) =>
  rpc<Project>("sites.add", { path, name, php, update_env: updateEnv });

/** `php artisan about --json`: sections of key/value pairs, in Laravel's order. */
export type AboutReport = Record<string, Record<string, unknown>>;
export const siteAbout = (id: string) => rpc<AboutReport>("sites.about", { id });
/** Runs `php artisan boost:update` in the site; returns its output. */
export const boostUpdate = (id: string) => rpc<string>("sites.boost", { id });

export interface Package {
  name: string;
  label: string;
  version: string | null;
}

/** What Werd reads from a project folder (composer.json, package.json, …). */
export interface ProjectInfo {
  path: string;
  name: string;
  laravel: boolean;
  php_constraint: string | null;
  suggested_php: string | null;
  suggested_php_installed: boolean;
  php_packages: Package[];
  js_packages: Package[];
  node: string | null;
  werd_yml: boolean;
  env_file: boolean;
}

export type StarterKit = "react" | "vue" | "svelte" | "livewire" | "custom";

export interface NewProject {
  name: string;
  directory: string;
  kit: StarterKit | null;
  using?: string;
  auth: "laravel" | "workos" | "none";
  teams: boolean;
  testing: "pest" | "phpunit";
  boost: boolean;
  git: boolean;
  npm: boolean;
  php: string;
}

export const inspectFolder = (path: string) => rpc<ProjectInfo>("sites.inspect", { path });
export const siteInfo = (id: string) => rpc<ProjectInfo>("sites.info", { id });
export const createProject = (project: NewProject) => rpc<Job>("sites.create", { ...project });
export const startProject = (id: string) => rpc<Project>("sites.start", { id });
export const stopProject = (id: string) => rpc<Project>("sites.stop", { id });
export const startVite = (id: string) => rpc<Project>("sites.vite.start", { id });
export const stopVite = (id: string) => rpc<Project>("sites.vite.stop", { id });
export const setViteAutostart = (id: string, autostart: boolean) =>
  rpc<Project>("sites.vite.autostart", { id, autostart });
export const resetPorts = (id: string) => rpc<Project>("sites.reset-ports", { id });
export const openSite = (id: string) => rpc<string>("sites.open", { id });
export const projectEnv = (id: string) => rpc<string>("sites.env", { id });
export const projectLogs = (id: string, service = "werd") => rpc<string[]>("sites.logs", { id, service });
export const removeProject = (id: string) => rpc<null>("sites.remove", { id });
export const setProjectPhp = (id: string, line: string) => rpc<Project>("sites.php", { id, line });
export const setProjectNode = (id: string, line: string | null) => rpc<Project>("sites.node", { id, line });
export const linkProject = (id: string, category: Category, instance: string) =>
  rpc<Project>("sites.link", { id, category, instance });
export const unlinkProject = (id: string, category: Category) => rpc<Project>("sites.unlink", { id, category });
export const setProjectAutostart = (id: string, autostart: boolean) =>
  rpc<Project>("sites.autostart", { id, autostart });
export const setProjectDomain = (id: string, domain: string) => rpc<Project>("sites.domain", { id, domain });
export const listParks = () => rpc<string[]>("parks.list");
export const parkFolder = (path: string) => rpc<string[]>("parks.add", { path });
export const unparkFolder = (path: string) => rpc<string[]>("parks.remove", { path });
export const resolveProject = (id: string) => rpc<{ project: Project; jobs: Job[] }>("sites.resolve", { id });

// ---- Runtimes, jobs, settings ------------------------------------------------

export const listRuntimes = () => rpc<RuntimeLine[]>("runtimes.list");
export const installRuntime = (product: string, line: string) => rpc<Job>("runtimes.install", { product, line });
export const updateRuntime = (product: string, line: string) => rpc<Job>("runtimes.update", { product, line });
export const uninstallRuntime = (product: string, line: string) => rpc<null>("runtimes.uninstall", { product, line });
export const setDefaultRuntime = (product: string, line: string) =>
  rpc<Settings>("runtimes.default", { product, line });
export const listJobs = () => rpc<Job[]>("jobs.list");
/** Runs the first-run setup again (Caddy, PHP, Composer); null when nothing is missing. */
export const runSetup = () => rpc<Job | null>("setup.run");
export const getSettings = () => rpc<Settings>("settings.get");
export const updateSettings = (
  changes: Partial<Pick<Settings, "upload_max_mb" | "memory_limit_mb" | "domains" | "https_port">>,
) => rpc<Settings>("settings.set", changes);

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
export const renameService = (id: string, name: string) => rpc<ServiceInstance>("services.rename", { id, name });
export const serviceLogs = (id: string) => rpc<string[]>("services.logs", { id });
/** The last lines of the shared Caddy's log. */
export const routerLogs = () => rpc<string[]>("router.logs");

// ---- System ------------------------------------------------------------------

export const systemInfo = () => rpc<SystemInfo>("system.info");
export const refreshCatalog = () => rpc<{ generated: string }>("catalog.refresh");
export const enablePath = () => rpc<Settings>("path.enable");
export const disablePath = () => rpc<Settings>("path.disable");
export const doctor = () => rpc<DoctorResult[]>("doctor");
export const trustCa = () => rpc<string>("trust-ca");
/** Whether the local CA exists (after the first site start) and is trusted by the system. */
export const certificateStatus = () => rpc<{ exists: boolean; trusted: boolean }>("certificate.status");
export const domainsStatus = () => rpc<DomainsStatus>("domains.status");

/** Adds every site domain to the hosts file. The system asks for administrator approval. */
export async function syncHosts(): Promise<void> {
  if (demo) {
    await demoRpc("hosts.sync", {});
    return;
  }
  return invoke<void>("sync_hosts");
}

// ---- Desktop shell ----------------------------------------------------------

let demoLaunchAtLogin = false;

export interface Editor {
  id: string;
  label: string;
}

/** Code editors installed on this computer. */
export async function listEditors(): Promise<Editor[]> {
  if (!desktop) return demo ? [{ id: "vscode", label: "VS Code" }] : [];
  return invoke<Editor[]>("editors");
}

/** `folder`, `terminal`, `tinker` or `editor:<id>` for a site. */
export async function siteAction(id: string, action: string): Promise<void> {
  if (!desktop) return;
  return invoke<void>("site_action", { id, action });
}

// ---- Updates -------------------------------------------------------------------

export interface UpdateInfo {
  version: string;
  /** Release notes in markdown. */
  notes: string | null;
  date: string | null;
}

const demoUpdate: UpdateInfo = {
  version: "0.3.1",
  notes:
    "## [0.3.1]\n\n### Features\n\n- **ui:** Update Werd from the sidebar\n- **core:** Start sites with Werd\n\n### Bug fixes\n\n- **release:** Keep PATH and .test domains across updates",
  date: null,
};

/** Asks for a newer Werd; null when up to date. */
export async function checkUpdate(): Promise<UpdateInfo | null> {
  if (!desktop) return demo ? demoUpdate : null;
  return invoke<UpdateInfo | null>("check_update");
}

/** Downloads the update found by checkUpdate, reporting bytes as they arrive. */
export async function downloadUpdate(
  onProgress: (progress: { downloaded: number; total: number | null }) => void,
): Promise<void> {
  if (!desktop) {
    const total = 24_000_000;
    for (let downloaded = 0; downloaded <= total; downloaded += total / 20) {
      onProgress({ downloaded, total });
      await new Promise((resolve) => setTimeout(resolve, 120));
    }
    return;
  }
  const channel = new Channel<{ downloaded: number; total: number | null }>();
  channel.onmessage = onProgress;
  return invoke<void>("download_update", { onProgress: channel });
}

/** Installs the downloaded update and restarts Werd. */
export async function installUpdate(): Promise<void> {
  if (!desktop) {
    window.location.reload();
    return;
  }
  return invoke<void>("install_update");
}

/** Calls `handler` when the tray asks to check for updates. Returns an unsubscribe function. */
export function onCheckUpdateRequest(handler: () => void): () => void {
  if (!desktop) return () => {};
  const pending = listen("werd://check-update", handler);
  return () => void pending.then((unlisten) => unlisten());
}

/** Opens the system folder picker; null when cancelled. */
export async function pickFolder(title: string): Promise<string | null> {
  if (!desktop) return demo ? "C:\\Users\\dev\\Developer\\new-app" : null;
  return invoke<string | null>("pick_folder", { title });
}

/** Whether Werd starts (in the tray) when the user signs in. */
export async function launchAtLogin(): Promise<boolean> {
  if (!desktop) return demoLaunchAtLogin;
  return invoke<boolean>("launch_at_login");
}

export async function setLaunchAtLogin(enabled: boolean): Promise<boolean> {
  if (!desktop) {
    demoLaunchAtLogin = enabled;
    return enabled;
  }
  return invoke<boolean>("set_launch_at_login", { enabled });
}

/** Translates the tray menu; a no-op outside the desktop app. */
/** Translates the tray menu and rebuilds its PHP entries; a no-op outside the desktop app. */
export async function setTrayLabels(labels: {
  open: string;
  stop_all: string;
  quit: string;
  use_php: string;
  check_updates: string;
}): Promise<void> {
  if (!desktop) return;
  return invoke<void>("set_tray_labels", { labels });
}

/** Calls `handler` when the tray changed the global PHP version. Returns an unsubscribe function. */
export function onRuntimesChanged(handler: () => void): () => void {
  if (!desktop) return () => {};
  const pending = listen("werd://runtimes-changed", handler);
  return () => void pending.then((unlisten) => unlisten());
}

export const REPOSITORY_URL = "https://github.com/JustFlavio/werd";

export async function openUrl(url: string): Promise<void> {
  if (!desktop) {
    window.open(url, "_blank");
    return;
  }
  return invoke<void>("open_url", { url });
}
