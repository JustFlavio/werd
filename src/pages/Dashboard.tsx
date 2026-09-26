import { Info } from "lucide-react";
import { useMemo } from "react";
import type { Page } from "../App";
import type { Project, ServiceName } from "../api";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { SERVICES, serviceUrl } from "../services";
import { PageHeader, Section, StatusDot } from "../ui";

interface ActiveService {
  id: string;
  label: string;
  detail: string;
}

export function Dashboard({
  projects,
  runtimes,
  busy,
  onStopAll,
  onNavigate,
  onOpenUrl,
}: {
  projects: Project[];
  runtimes: Runtimes;
  busy: boolean;
  onStopAll: () => void;
  onNavigate: (page: Page) => void;
  onOpenUrl: (url: string) => void;
}) {
  const t = useT();
  const running = useMemo(() => projects.filter((project) => project.status === "running"), [projects]);

  const active = useMemo<ActiveService[]>(() => {
    if (running.length === 0) return [];
    const names = (list: Project[]) => list.map((project) => project.name).join(", ");
    const rows: ActiveService[] = [
      { id: "caddy", label: "Caddy", detail: t.dashboard.servesHttps(names(running)) },
      {
        id: "php",
        label: `PHP ${[...new Set(running.map((project) => project.php))].sort().join(", ")}`,
        detail: t.dashboard.servesFastcgi(names(running)),
      },
    ];
    for (const service of Object.keys(SERVICES) as ServiceName[]) {
      const using = running.filter((project) => project.services.includes(service));
      if (using.length) {
        rows.push({ id: service, label: SERVICES[service].label, detail: t.dashboard.instancesFor(names(using)) });
      }
    }
    return rows;
  }, [running, t]);

  const phpLines = runtimes.rows.filter((row) => row.product === "php" && row.installed);
  const defaultPhp = phpLines.find((row) => row.is_default);
  const inboxes = running.map((project) => serviceUrl(project, "mailpit")).filter((url): url is string => url !== null);
  const link = (page: Page) => (
    <button key={page} type="button" className="link" onClick={() => onNavigate(page)}>
      {t.nav[page]}
    </button>
  );

  return (
    <>
      <PageHeader title={t.dashboard.title} />
      <div className="page-body">
        <div className="dashboard-top">
          <Section
            title={t.dashboard.activeServices}
            action={
              <button type="button" className="button" disabled={running.length === 0 || busy} onClick={onStopAll}>
                {t.dashboard.stopAll}
              </button>
            }
          >
            {active.length === 0 ? (
              <p className="muted">{t.dashboard.noneRunning(link("sites"))}</p>
            ) : (
              <ul className="service-list">
                {active.map((service) => (
                  <li key={service.id}>
                    <StatusDot status="running" />
                    <span>{service.label}</span>
                    <span className="info" title={service.detail}>
                      <Info size={15} aria-label={service.detail} />
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Section>

          <aside className="quick-links">
            <h2>{t.dashboard.quickLinks}</h2>
            <button
              type="button"
              className="button button-block"
              disabled={inboxes.length === 0}
              title={inboxes.length === 0 ? t.dashboard.openMailHint : undefined}
              onClick={() => (inboxes.length === 1 ? onOpenUrl(inboxes[0]) : onNavigate("services"))}
            >
              {t.dashboard.openMail}
            </button>
            <button type="button" className="button button-block" onClick={() => onNavigate("logs")}>
              {t.dashboard.openLogs}
            </button>
            <button type="button" className="button button-block" onClick={() => onNavigate("general")}>
              {t.dashboard.diagnostics}
            </button>
          </aside>
        </div>

        <Section
          title={t.dashboard.phpVersion}
          description={t.dashboard.phpVersionHint(link("php"))}
          action={
            <select
              className="select"
              value={defaultPhp?.line ?? ""}
              disabled={phpLines.length === 0}
              aria-label={t.dashboard.phpVersion}
              onChange={(event) => {
                const row = phpLines.find((candidate) => candidate.line === event.target.value);
                if (row) runtimes.setDefault(row);
              }}
            >
              {!defaultPhp && <option value="">{phpLines.length ? "—" : t.dashboard.noPhp}</option>}
              {phpLines.map((row) => (
                <option key={row.line} value={row.line}>
                  PHP {row.line}
                </option>
              ))}
            </select>
          }
        />

        <Section
          title={t.nav.sites}
          description={t.dashboard.sitesSummary(projects.length, running.length)}
          action={
            <button type="button" className="button" onClick={() => onNavigate("sites")}>
              {t.dashboard.manageSites}
            </button>
          }
        />
      </div>
    </>
  );
}
