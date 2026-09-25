import type { Project, ServiceName } from "./api";

export interface ServiceMeta {
  label: string;
  version: string;
  /** Port keys assigned by the daemon for this service. */
  ports: string[];
  /** Port key of a web UI that can be opened in the browser, if any. */
  ui?: string;
}

export const SERVICES: Record<ServiceName, ServiceMeta> = {
  postgres: { label: "PostgreSQL", version: "18 + pgvector", ports: ["postgres"] },
  redis: { label: "Redis", version: "7.2", ports: ["redis"] },
  mailpit: { label: "Mailpit", version: "1.31", ports: ["mailpit_smtp", "mailpit_ui"], ui: "mailpit_ui" },
  rustfs: { label: "RustFS (S3)", version: "1.0", ports: ["rustfs_api", "rustfs_console"], ui: "rustfs_console" },
};

export const SERVICE_NAMES = Object.keys(SERVICES) as ServiceName[];

/** "SMTP 52015 · Web inbox 52016", or "—" before ports are assigned. */
export function describePorts(project: Project, service: ServiceName, labels: Record<string, string>): string {
  const described = SERVICES[service].ports.flatMap((key) => {
    const port = project.ports?.[key];
    return port ? [`${labels[key] ?? key} ${port}`] : [];
  });
  return described.join(" · ") || "—";
}

export const LOG_SOURCES = ["werd", "php", "caddy", "postgres", "redis", "mailpit", "rustfs"] as const;

export function serviceUrl(project: Project, service: ServiceName): string | null {
  const key = SERVICES[service].ui;
  const port = key ? project.ports?.[key] : undefined;
  return project.status === "running" && port ? `http://127.0.0.1:${port}` : null;
}
