import type { Project } from "../api";
import { PORT_LABELS, SERVICE_NAMES, SERVICES, serviceUrl } from "../services";
import { PageHeader, Section, StatusDot } from "../ui";

export function Services({ projects, onOpenUrl }: { projects: Project[]; onOpenUrl: (url: string) => void }) {
  return (
    <>
      <PageHeader title="Servizi" />
      <div className="page-body">
        <p className="muted lead">
          I servizi partono insieme al sito che li richiede in <code>werd.yml</code>. Ogni sito ha istanze, porte e dati
          separati, quindi fermarne uno non tocca gli altri.
        </p>
        {SERVICE_NAMES.map((service) => {
          const meta = SERVICES[service];
          const using = projects.filter((project) => project.services.includes(service));
          return (
            <Section key={service} title={meta.label} description={`Versione ${meta.version}`}>
              {using.length === 0 ? (
                <p className="muted">Nessun sito usa questo servizio.</p>
              ) : (
                <table className="table">
                  <thead>
                    <tr>
                      <th>Sito</th>
                      <th>Stato</th>
                      <th>Porte</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {using.map((project) => {
                      const url = serviceUrl(project, service);
                      return (
                        <tr key={project.id}>
                          <td>{project.name}</td>
                          <td>
                            <StatusDot status={project.status} label />
                          </td>
                          <td className="mono muted">
                            {meta.ports
                              .map((key) => (project.ports?.[key] ? `${PORT_LABELS[key]} ${project.ports[key]}` : null))
                              .filter(Boolean)
                              .join(" · ") || "—"}
                          </td>
                          <td className="cell-action">
                            {url && (
                              <button type="button" className="button button-small" onClick={() => onOpenUrl(url)}>
                                Apri
                              </button>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              )}
            </Section>
          );
        })}
      </div>
    </>
  );
}
