import { useEffect, useRef, useState } from "react";
import { type Project, projectLogs } from "../api";
import { useT } from "../i18n";
import { LOG_SOURCES } from "../services";
import { EmptyState, PageHeader } from "../ui";

export function Logs({
  projects,
  projectId,
  onProjectChange,
}: {
  projects: Project[];
  projectId: string | null;
  onProjectChange: (id: string) => void;
}) {
  const t = useT();
  const [source, setSource] = useState<string>("werd");
  const [lines, setLines] = useState<string[]>([]);
  const viewer = useRef<HTMLPreElement>(null);
  const current = projects.find((project) => project.id === projectId) ?? projects[0] ?? null;
  const currentId = current?.id;

  useEffect(() => {
    if (!currentId) return;
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
    const load = () =>
      void projectLogs(currentId, source)
        .then(show)
        .catch(() => show([]));
    load();
    const timer = window.setInterval(load, 2500);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [currentId, source]);

  if (!current)
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
          value={current.id}
          onChange={(event) => onProjectChange(event.target.value)}
          aria-label={t.logs.site}
        >
          {projects.map((project) => (
            <option key={project.id} value={project.id}>
              {project.name}
            </option>
          ))}
        </select>
        <select
          className="select"
          value={source}
          onChange={(event) => setSource(event.target.value)}
          aria-label={t.logs.source}
        >
          {LOG_SOURCES.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
      </PageHeader>
      <pre ref={viewer} className="log-viewer">
        {lines.length ? lines.join("\n") : <span className="muted">{t.logs.noLines(source)}</span>}
      </pre>
    </>
  );
}
