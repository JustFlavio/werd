import type { Project } from "../api";
import { useT } from "../i18n";
import { describePorts, SERVICE_NAMES, SERVICES, serviceUrl } from "../services";
import { PageHeader, Section, StatusDot } from "../ui";

export function Services({ projects, onOpenUrl }: { projects: Project[]; onOpenUrl: (url: string) => void }) {
  const t = useT();
  return (
    <>
      <PageHeader title={t.services.title} />
      <div className="page-body">
        <p className="muted lead">{t.services.lead(<code key="file">werd.yml</code>)}</p>
        {SERVICE_NAMES.map((service) => {
          const meta = SERVICES[service];
          const using = projects.filter((project) => project.services.includes(service));
          return (
            <Section key={service} title={meta.label} description={t.services.version(meta.version)}>
              {using.length === 0 ? (
                <p className="muted">{t.services.unused}</p>
              ) : (
                <table className="table">
                  <thead>
                    <tr>
                      <th>{t.services.site}</th>
                      <th>{t.services.status}</th>
                      <th>{t.services.ports}</th>
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
                          <td className="mono muted">{describePorts(project, service, t.ports)}</td>
                          <td className="cell-action">
                            {url && (
                              <button type="button" className="button button-small" onClick={() => onOpenUrl(url)}>
                                {t.common.open}
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
