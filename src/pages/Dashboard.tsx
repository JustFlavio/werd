import { Info } from "lucide-react";
import { useMemo } from "react";
import type { Page } from "../App";
import type { Project, ServiceName } from "../api";
import { SERVICES, serviceUrl } from "../services";
import { PageHeader, Section, StatusDot } from "../ui";

interface ActiveService {
  id: string;
  label: string;
  detail: string;
}

export function Dashboard({
  projects,
  busy,
  onStopAll,
  onNavigate,
  onOpenUrl,
}: {
  projects: Project[];
  busy: boolean;
  onStopAll: () => void;
  onNavigate: (page: Page) => void;
  onOpenUrl: (url: string) => void;
}) {
  const running = projects.filter((project) => project.status === "running");

  const active = useMemo<ActiveService[]>(() => {
    if (running.length === 0) return [];
    const names = (list: Project[]) => list.map((project) => project.name).join(", ");
    const rows: ActiveService[] = [
      { id: "caddy", label: "Caddy", detail: `HTTPS per ${names(running)}` },
      { id: "php", label: "PHP 8.5", detail: `FastCGI per ${names(running)}` },
    ];
    for (const service of Object.keys(SERVICES) as ServiceName[]) {
      const using = running.filter((project) => project.services.includes(service));
      if (using.length)
        rows.push({ id: service, label: SERVICES[service].label, detail: `Istanze per ${names(using)}` });
    }
    return rows;
  }, [running]);

  const inboxes = running.map((project) => serviceUrl(project, "mailpit")).filter((url): url is string => url !== null);

  return (
    <>
      <PageHeader title="Dashboard" />
      <div className="page-body">
        <div className="dashboard-top">
          <Section
            title="Servizi attivi"
            action={
              <button className="button" disabled={running.length === 0 || busy} onClick={onStopAll}>
                Ferma tutto
              </button>
            }
          >
            {active.length === 0 ? (
              <p className="muted">
                Nessun servizio in esecuzione. Avvia un sito dalla pagina{" "}
                <button className="link" onClick={() => onNavigate("sites")}>
                  Siti
                </button>
                .
              </p>
            ) : (
              <ul className="service-list">
                {active.map((service) => (
                  <li key={service.id}>
                    <StatusDot status="running" />
                    <span>{service.label}</span>
                    <span className="info" title={service.detail}>
                      <Info size={15} />
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </Section>

          <aside className="quick-links">
            <h2>Collegamenti rapidi</h2>
            <button
              className="button button-block"
              disabled={inboxes.length === 0}
              title={inboxes.length === 0 ? "Avvia un sito con Mailpit" : undefined}
              onClick={() => (inboxes.length === 1 ? onOpenUrl(inboxes[0]) : onNavigate("services"))}
            >
              Apri Mail
            </button>
            <button className="button button-block" onClick={() => onNavigate("logs")}>
              Apri log
            </button>
            <button className="button button-block" onClick={() => onNavigate("general")}>
              Diagnostica
            </button>
          </aside>
        </div>

        <Section
          title="Versione PHP"
          description={
            <>
              Tutti i siti usano PHP 8.5. Le altre versioni arriveranno dalla pagina{" "}
              <button className="link" onClick={() => onNavigate("php")}>
                PHP
              </button>
              .
            </>
          }
          action={
            <select className="select" value="8.5" disabled aria-label="Versione PHP">
              <option value="8.5">PHP 8.5</option>
            </select>
          }
        />

        <Section
          title="Siti"
          description={`${projects.length} ${projects.length === 1 ? "sito collegato" : "siti collegati"}, ${running.length} in esecuzione.`}
          action={
            <button className="button" onClick={() => onNavigate("sites")}>
              Gestisci siti
            </button>
          }
        />
      </div>
    </>
  );
}
