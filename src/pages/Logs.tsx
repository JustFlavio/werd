import { useEffect, useRef, useState } from "react";
import { type Project, projectLogs, type ServiceInstance, serviceLogs } from "../api";
import { SITE_LOG_SOURCES } from "../categories";
import { useT } from "../i18n";
import { EmptyState, PageHeader } from "../ui";

/** `site:<id>` or `service:<id>`. */
type Target = string;

export function Logs({
  projects,
  instances,
  projectId,
  onProjectChange,
}: {
  projects: Project[];
  instances: ServiceInstance[];
  projectId: string | null;
  onProjectChange: (id: string) => void;
}) {
  const t = useT();
  const [source, setSource] = useState<string>("werd");
  const [service, setService] = useState<string | null>(null);
  const [lines, setLines] = useState<string[]>([]);
  const viewer = useRef<HTMLPreElement>(null);

  const site = projects.find((project) => project.id === projectId) ?? projects[0] ?? null;
  const target: Target | null = service ? `service:${service}` : site ? `site:${site.id}` : null;

  useEffect(() => {
    if (!target) return;
    let cancelled = false;
    const show = (next: string[]) => {
      if (cancelled) return;
      setLines(next);
      // Keep the newest lines in view after React paints them.
      requestAnimationFrame(() => {
        const element = viewer.current;
        if (element) element.scrollTop = element.scrollHeight;
      });
    };
    const [kind, id] = target.split(/:(.*)/s);
    const load = () =>
      void (kind === "service" ? serviceLogs(id) : projectLogs(id, source)).then(show).catch(() => show([]));
    load();
    const timer = window.setInterval(load, 2500);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [target, source]);

  if (!target)
    return (
      <>
        <PageHeader title={t.logs.title} />
        <EmptyState title={t.logs.emptyTitle} description={t.logs.emptyDescription} />
      </>
    );

  return (
    <>
      <PageHeader title={t.logs.title}>
        <select
          className="select"
          value={target}
          aria-label={t.logs.site}
          onChange={(event) => {
            const [kind, id] = event.target.value.split(/:(.*)/s);
            if (kind === "service") {
              setService(id);
            } else {
              setService(null);
              onProjectChange(id);
            }
          }}
        >
          {projects.length > 0 && (
            <optgroup label={t.logs.sites}>
              {projects.map((project) => (
                <option key={project.id} value={`site:${project.id}`}>
                  {project.name}
                </option>
              ))}
            </optgroup>
          )}
          {instances.length > 0 && (
            <optgroup label={t.logs.services}>
              {instances.map((instance) => (
                <option key={instance.id} value={`service:${instance.id}`}>
                  {instance.name}
                </option>
              ))}
            </optgroup>
          )}
        </select>
        {!service && (
          <select
            className="select"
            value={source}
            onChange={(event) => setSource(event.target.value)}
            aria-label={t.logs.source}
          >
            {SITE_LOG_SOURCES.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        )}
      </PageHeader>
      <pre ref={viewer} className="log-viewer">
        {lines.length ? lines.join("\n") : <span className="muted">{t.logs.empty}</span>}
      </pre>
    </>
  );
}
