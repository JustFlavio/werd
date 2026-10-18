import { ArrowRight, Plus, RotateCw, TriangleAlert } from "lucide-react";
import { type Project, runSetup } from "../api";
import { CATEGORIES } from "../categories";
import { Progress } from "../components/RuntimeTable";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { EmptyState, PageHeader, StatusDot } from "../ui";

export function Dashboard({
  projects,
  runtimes,
  onSelect,
  onAdd,
  onError,
  missingDomains = [],
}: {
  projects: Project[];
  runtimes: Runtimes;
  onSelect: (project: Project) => void;
  onAdd: () => void;
  onError: (cause: unknown) => void;
  missingDomains?: string[];
}) {
  const t = useT();
  const running = projects.filter((project) => project.status === "running").length;
  const setup = runtimes.jobs.filter((job) => job.product === "setup").sort((a, b) => b.started_at - a.started_at)[0];
  const setupStep = setup?.step.match(/^Downloading (.+)$/)?.[1];
  const add = (
    <button type="button" className="button button-primary" onClick={onAdd}>
      <Plus size={15} />
      {t.sites.add}
    </button>
  );
  return (
    <>
      <PageHeader title={t.workspace.home}>{projects.length > 0 && add}</PageHeader>
      <div className="page-body workspace-home">
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
                  .catch(onError)
              }
            >
              <RotateCw size={13} />
              {t.common.retry}
            </button>
          </div>
        )}
        {projects.length === 0 ? (
          <EmptyState title={t.sites.emptyTitle} description={t.sites.emptyDescription}>
            {add}
          </EmptyState>
        ) : (
          <>
            <div className="workspace-intro">
              <h2>{t.workspace.yourProjects}</h2>
              <p className="muted">{t.workspace.summary(projects.length, running)}</p>
            </div>
            <div className="project-grid">
              {projects.map((project) => (
                <button type="button" className="project-card" key={project.id} onClick={() => onSelect(project)}>
                  <span className="project-card-head">
                    <strong>{project.name}</strong>
                    <StatusDot status={project.status} label={t.preparation.serverStatus[project.status]} />
                  </span>
                  <span className="muted project-address">{project.domain ?? project.path}</span>
                  <span className="project-capabilities">
                    <span>PHP {project.php}</span>
                    {project.node && <span>Node.js {project.node}</span>}
                    {CATEGORIES.filter((category) => project.links?.[category]).map((category) => (
                      <span key={category}>{t.services.categories[category]}</span>
                    ))}
                  </span>
                  {(project.error ||
                    (project.requirements?.length ?? 0) > 0 ||
                    (project.domain && missingDomains.includes(project.domain))) && (
                    <span className="project-attention">
                      <TriangleAlert size={14} />
                      {project.error ? t.preparation.startFailed : t.workspace.needsSetup}
                    </span>
                  )}
                  <span className="project-card-footer">
                    {t.workspace.openProject}
                    <ArrowRight size={15} />
                  </span>
                </button>
              ))}
            </div>
          </>
        )}
      </div>
    </>
  );
}
