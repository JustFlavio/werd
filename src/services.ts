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

export const PORT_LABELS: Record<string, string> = {
  site: "HTTPS",
  fastcgi: "FastCGI",
  postgres: "PostgreSQL",
  redis: "Redis",
  mailpit_smtp: "SMTP",
  mailpit_ui: "Inbox web",
  rustfs_api: "API S3",
  rustfs_console: "Console",
};

export const LOG_SOURCES = ["werd", "php", "caddy", "postgres", "redis", "mailpit", "rustfs"] as const;

export function serviceUrl(project: Project, service: ServiceName): string | null {
  const key = SERVICES[service].ui;
  const port = key ? project.ports?.[key] : undefined;
  return project.status === "running" && port ? `http://127.0.0.1:${port}` : null;
}
