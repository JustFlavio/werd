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

export async function listProjects(): Promise<Snapshot> {
  if (!desktop) return { projects: [], daemon_version: "modalità anteprima" };
  return invoke<Snapshot>("list_projects");
}

export async function addProject(path: string): Promise<Project> {
  if (!desktop) throw new Error("Apri l’app desktop per aggiungere un progetto.");
  return invoke<Project>("add_project", { path });
}

export async function startProject(id: string): Promise<Project> {
  if (!desktop) throw new Error("Apri l’app desktop per avviare un progetto.");
  return invoke<Project>("start_project", { id });
}

export async function stopProject(id: string): Promise<Project> {
  if (!desktop) throw new Error("Apri l’app desktop per fermare un progetto.");
  return invoke<Project>("stop_project", { id });
}

export async function resetPorts(id: string): Promise<Project> {
  if (!desktop) throw new Error("Apri l’app desktop per riassegnare le porte.");
  return invoke<Project>("reset_ports", { id });
}

export async function openSite(id: string): Promise<string> {
  if (!desktop) throw new Error("Apri l’app desktop per aprire il sito.");
  return invoke<string>("open_site", { id });
}

export async function projectLogs(id: string, service = "werd"): Promise<string[]> {
  if (!desktop) return [];
  return invoke<string[]>("project_logs", { id, service });
}

export async function doctor(): Promise<DoctorResult[]> {
  if (!desktop) return [];
  return invoke<DoctorResult[]>("doctor");
}

export async function runtimes(): Promise<RuntimeInfo[]> {
  if (!desktop) return [];
  return invoke<RuntimeInfo[]>("runtimes");
}

export async function installRuntime(id: string): Promise<RuntimeInfo> {
  if (!desktop) throw new Error("Apri l’app desktop per installare un runtime.");
  return invoke<RuntimeInfo>("install_runtime", { id });
}

export async function projectEnv(id: string): Promise<string> {
  if (!desktop) return "";
  return invoke<string>("project_env", { id });
}

export async function trustCa(): Promise<string> {
  if (!desktop) throw new Error("Apri l’app desktop per installare il certificato locale.");
  return invoke<string>("trust_ca");
}
