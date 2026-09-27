import { Info, RotateCw, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { Page } from "../App";
import { type Project, projectLogs, routerLogs, runSetup, type ServiceInstance, serviceLogs } from "../api";
import { Progress } from "../components/RuntimeTable";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { Modal, PageHeader, Section, StatusDot } from "../ui";

interface ActiveService {
  id: string;
  label: string;
  detail: string;
  /** Loads the log shown by the (i) button. */
  logs: () => Promise<string[]>;
}

/** Logs of several sites' PHP processes, one titled block per site. */
async function phpLogs(sites: Project[]): Promise<string[]> {
  const blocks = await Promise.all(
    sites.map(async (site) => [`── ${site.name} ──`, ...(await projectLogs(site.id, "php").catch(() => []))]),
  );
  return blocks.flat();
}

function LogsModal({ service, onClose }: { service: ActiveService; onClose: () => void }) {
  const t = useT();
  const [lines, setLines] = useState<string[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    void service
      .logs()
      .then(setLines)
      .catch((cause) => setFailed(cause instanceof Error ? cause.message : String(cause)));
  }, [service]);
  return (
    <Modal title={t.dashboard.logsTitle(service.label)} onClose={onClose} wide>
      <p className="muted">{service.detail}</p>
      {failed ? (
        <div className="callout callout-error">{failed}</div>
      ) : (
        <pre className="log-box mono">
          {lines === null ? t.common.loading : lines.length ? lines.join("\n") : t.dashboard.noLogs}
        </pre>
      )}
    </Modal>
  );
}

export function Dashboard({
  projects,
  instances,
  runtimes,
  busy,
  onStopAll,
  onNavigate,
  onOpenUrl,
}: {
  projects: Project[];
  instances: ServiceInstance[];
  runtimes: Runtimes;
  busy: boolean;
  onStopAll: () => void;
  onNavigate: (page: Page) => void;
  onOpenUrl: (url: string) => void;
}) {
  const t = useT();
  const [showing, setShowing] = useState<ActiveService | null>(null);
  const running = useMemo(() => projects.filter((project) => project.status === "running"), [projects]);

  const runningInstances = useMemo(() => instances.filter((instance) => instance.status === "running"), [instances]);

  const active = useMemo<ActiveService[]>(() => {
    const rows: ActiveService[] = runningInstances.map((instance) => ({
      id: `instance:${instance.id}`,
      label: instance.name,
      detail: `${instance.product} ${instance.line} · 127.0.0.1:${instance.port}`,
      logs: () => serviceLogs(instance.id),
    }));
    if (running.length === 0) return rows;
    const names = (list: Project[]) => list.map((project) => project.name).join(", ");
    rows.push({
      id: "site:caddy",
      label: "Caddy",
      detail: t.dashboard.servesHttps(names(running)),
      logs: routerLogs,
    });
    // One row per PHP version in use, like Herd's PHP-8.5.
    for (const line of [...new Set(running.map((project) => project.php))].sort().reverse()) {
      const sites = running.filter((project) => project.php === line);
      rows.push({
        id: `php:${line}`,
        label: `PHP ${line}`,
        detail: t.dashboard.servesFastcgi(names(sites)),
        logs: () => phpLogs(sites),
      });
    }
    return rows;
  }, [running, runningInstances, t]);

  // The first-run setup job (Caddy, PHP, Composer), shown until it succeeds.
  const setup = runtimes.jobs.filter((job) => job.product === "setup").sort((a, b) => b.started_at - a.started_at)[0];
  const setupStep = setup?.step.match(/^Downloading (.+)$/)?.[1];

  const phpLines = runtimes.rows.filter((row) => row.product === "php" && row.installed);
  const defaultPhp = phpLines.find((row) => row.is_default);
  const inboxes = [
    ...runningInstances.filter((instance) => instance.product === "mailpit").map((instance) => instance.web_ui),
  ].filter((url): url is string => Boolean(url));
  const link = (page: Page) => (
    <button key={page} type="button" className="link" onClick={() => onNavigate(page)}>
      {t.nav[page]}
    </button>
  );

  return (
    <>
      <PageHeader title={t.dashboard.title} />
      <div className="page-body">
        {setup?.state === "running" && (
          <div className="setup-card" role="status">
            <div>
              <strong>{t.dashboard.setupTitle}</strong>
              <p className="muted">{setupStep ? t.dashboard.setupDownloading(setupStep) : t.dashboard.setupHint}</p>
            </div>
            <Progress job={setup} />
          </div>
        )}
        {setup?.state === "failed" && (
          <div className="callout callout-warn setup-failed">
            <TriangleAlert size={15} />
            <span>{t.dashboard.setupFailed(setup.error ?? "")}</span>
            <button
              type="button"
              className="button button-small"
              onClick={() =>
                void runSetup()
                  .then((job) => job && runtimes.track(job))
                  .catch(() => {})
              }
            >
              <RotateCw size={13} /> {t.common.retry}
            </button>
          </div>
        )}
        <div className="dashboard-top">
          <Section
            title={t.dashboard.activeServices}
            action={
              <button
                type="button"
                className="button"
                disabled={(running.length === 0 && runningInstances.length === 0) || busy}
                onClick={onStopAll}
              >
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
                    <button
                      type="button"
                      className="icon-button info"
                      title={t.dashboard.showLogs(service.label)}
                      aria-label={t.dashboard.showLogs(service.label)}
                      onClick={() => setShowing(service)}
                    >
                      <Info size={15} />
                    </button>
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
      {showing && <LogsModal service={showing} onClose={() => setShowing(null)} />}
    </>
  );
}
