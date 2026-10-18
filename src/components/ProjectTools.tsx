import { Plus } from "lucide-react";
import { useEffect, useState } from "react";
import {
  type Category,
  createService,
  linkProject,
  openUrl,
  type Project,
  prepareService,
  type ServiceInstance,
  type ServiceOffering,
  serviceCatalog,
} from "../api";
import { CATEGORIES } from "../categories";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { EmptyState, Modal, Section, StatusDot } from "../ui";
import { DatabaseBackups } from "./DatabaseBackups";
import { Progress } from "./RuntimeTable";

interface Props {
  project: Project;
  projects: Project[];
  instances: ServiceInstance[];
  runtimes: Runtimes;
  onChanged: () => Promise<void>;
  onError: (cause: unknown) => void;
  onConnected?: () => void;
}

/** Project actions never stop or delete a shared engine. Connections live in the advanced section. */
export function ProjectTools(props: Props) {
  const { project, projects, instances, runtimes, onChanged, onError } = props;
  const t = useT();
  const [adding, setAdding] = useState(false);
  const [backup, setBackup] = useState<{ instance: ServiceInstance; database: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const configured = CATEGORIES.filter((category) => project.links?.[category]);
  const add = configured.length < CATEGORIES.length && (
    <button type="button" className="button" onClick={() => setAdding(true)}>
      <Plus size={14} />
      {t.workspace.addTool}
    </button>
  );
  return (
    <>
      <Section title={t.workspace.tools} description={t.workspace.toolsHint} action={configured.length > 0 && add}>
        {configured.length === 0 ? (
          <EmptyState title={t.workspace.noTools} description={t.workspace.noToolsHint}>
            {add}
          </EmptyState>
        ) : (
          <div className="project-tools">
            {configured.map((category) => {
              const link = project.links?.[category];
              const instance = instances.find((candidate) => candidate.id === link?.instance);
              const shared = projects.filter(
                (other) =>
                  other.id !== project.id &&
                  Object.values(other.links ?? {}).some((value) => value.instance === link?.instance),
              );
              const latest = instance ? runtimes.jobFor(instance.product, instance.line) : undefined;
              const job = latest && ["install", "update"].includes(latest.action) ? latest : undefined;
              return (
                <article className="project-tool" key={category}>
                  <div className="project-card-head">
                    <h3>{t.services.categories[category]}</h3>
                    {instance && <StatusDot status={instance.status} label />}
                  </div>
                  {instance ? (
                    <>
                      <p className="muted">
                        {instance.name}
                        {link?.database && <span className="tool-database mono">{link.database}</span>}
                      </p>
                      {instance.error && <p className="callout callout-error">{instance.error}</p>}
                      {job?.state === "running" && <Progress job={job} />}
                      {job?.state === "failed" && <p className="callout callout-error">{job.error}</p>}
                      {shared.length > 0 && (
                        <p className="shared-notice">
                          {t.workspace.shared(shared.map((other) => other.name).join(", "))}
                        </p>
                      )}
                      <div className="button-row">
                        {instance.status !== "running" && (
                          <button
                            type="button"
                            className="button button-small"
                            disabled={busy !== null || instance.status === "starting" || job?.state === "running"}
                            onClick={() => {
                              setBusy(instance.id);
                              void prepareService(instance.id)
                                .then(({ job }) => {
                                  if (job) runtimes.track(job);
                                  return onChanged();
                                })
                                .catch(onError)
                                .finally(() => setBusy(null));
                            }}
                          >
                            {t.workspace.prepare}
                          </button>
                        )}
                        {instance.web_ui && (
                          <button
                            type="button"
                            className="button button-small"
                            disabled={instance.status !== "running"}
                            onClick={() => void openUrl(instance.web_ui ?? "").catch(onError)}
                          >
                            {t.common.open} {t.services.categories[category]}
                          </button>
                        )}
                        {category === "database" &&
                          link?.database &&
                          ["postgresql", "mysql", "mariadb"].includes(instance.product) && (
                            <button
                              type="button"
                              className="button button-small"
                              onClick={() => setBackup({ instance, database: link.database ?? "" })}
                            >
                              {t.backups.title}
                            </button>
                          )}
                      </div>
                    </>
                  ) : (
                    <p className="callout callout-warn">{t.workspace.missingEngine}</p>
                  )}
                </article>
              );
            })}
          </div>
        )}
      </Section>
      {adding && <AddProjectTool {...props} onClose={() => setAdding(false)} />}
      {backup && (
        <DatabaseBackups
          key={`${backup.instance.id}:${backup.database}`}
          instance={instances.find((instance) => instance.id === backup.instance.id) ?? backup.instance}
          projectDatabase={backup.database}
          onClose={() => setBackup(null)}
        />
      )}
    </>
  );
}

function AddProjectTool({
  project,
  instances,
  runtimes,
  onChanged,
  onClose,
  onConnected,
}: Props & { onClose: () => void }) {
  const t = useT();
  const available = CATEGORIES.filter((category) => !project.links?.[category]);
  const [category, setCategory] = useState<Category>(available[0] ?? "database");
  const [offerings, setOfferings] = useState<ServiceOffering[] | null>(null);
  const [choice, setChoice] = useState("");
  const [line, setLine] = useState("");
  const [pgvector, setPgvector] = useState(false);
  const [created, setCreated] = useState<ServiceInstance | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    void serviceCatalog()
      .then((rows) => {
        if (alive) setOfferings(rows);
      })
      .catch((cause) => {
        if (alive) setError(String(cause));
      });
    return () => {
      alive = false;
    };
  }, []);
  const requirement = project.requirements?.find((candidate) => candidate.category === category);
  const options = (offerings ?? []).filter(
    (offering) => offering.categories.includes(category) && (!requirement || offering.product === requirement.product),
  );
  const offering = options.find((option) => option.product === choice) ?? options[0];
  const lines = (offering?.lines ?? []).filter(
    (candidate) => !requirement?.line || candidate.line === requirement.line,
  );
  const version = lines.find((candidate) => candidate.line === line) ?? lines[0];
  const redisLink =
    category === "cache" ? project.links?.queue : category === "queue" ? project.links?.cache : undefined;
  const redis = instances.find((instance) => instance.id === redisLink?.instance && instance.product === "redis");
  const redisConflict =
    redis &&
    requirement &&
    (redis.product !== requirement.product ||
      (requirement.line && redis.line !== requirement.line) ||
      requirement.extensions?.some((extension) => !redis.extensions?.includes(extension)));
  const submit = async () => {
    if (saving || redisConflict || (!created && !redis && (!offering || !version))) return;
    setSaving(true);
    setError(null);
    try {
      let instance = created ?? redis;
      if (!instance && offering && version) {
        const result = await createService({
          product: offering.product,
          line: version.line,
          name: `${project.name} · ${t.services.categories[category]}`,
          autostart: false,
          extensions:
            requirement?.extensions ?? (pgvector && offering.extensions.includes("pgvector") ? ["pgvector"] : []),
        });
        instance = result.instance;
        // Retain the created instance if linking fails, so retry never creates another engine.
        setCreated(instance);
        if (result.job) runtimes.track(result.job);
      }
      if (!instance) return;
      await linkProject(project.id, category, instance.id);
      await onChanged();
      onClose();
      onConnected?.();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };
  return (
    <Modal
      title={`${t.workspace.addTool} · ${project.name}`}
      onClose={() => {
        if (!saving) onClose();
      }}
    >
      <p className="muted">{t.workspace.addHint}</p>
      {error && (
        <p role="alert" className="callout callout-error">
          {error}
        </p>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <fieldset disabled={saving || created !== null} className="tool-fields">
          <label className="field">
            <span>{t.services.category}</span>
            <select
              className="select"
              value={category}
              onChange={(event) => {
                setCategory(event.target.value as Category);
                setChoice("");
                setLine("");
                setPgvector(false);
              }}
            >
              {available.map((candidate) => (
                <option key={candidate} value={candidate}>
                  {t.services.categories[candidate]}
                </option>
              ))}
            </select>
          </label>
          {redis ? (
            <p className={redisConflict ? "callout callout-warn" : undefined}>
              {redisConflict ? t.workspace.redisConflict : t.workspace.redisReuse}
            </p>
          ) : offerings === null ? (
            <p>{t.common.loading}</p>
          ) : (
            <>
              <label className="field">
                <span>{t.workspace.engine}</span>
                <select
                  className="select"
                  value={offering?.product ?? ""}
                  onChange={(event) => {
                    setChoice(event.target.value);
                    setLine("");
                    setPgvector(false);
                  }}
                >
                  {options.map((option) => (
                    <option key={option.product} value={option.product}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
              {version && (
                <p className="field-hint">
                  {offering?.label} {version.line}
                </p>
              )}
              {!version && <p className="callout callout-warn">{t.runtimes.unavailable}</p>}
              <details className="advanced-panel">
                <summary>{t.workspace.advanced}</summary>
                <label className="field">
                  <span>{t.workspace.version}</span>
                  <select
                    className="select"
                    value={version?.line ?? ""}
                    onChange={(event) => setLine(event.target.value)}
                  >
                    {lines.map((candidate) => (
                      <option key={candidate.line} value={candidate.line}>
                        {candidate.line} ({candidate.latest})
                      </option>
                    ))}
                  </select>
                </label>
                {offering?.extensions.includes("pgvector") && (
                  <label className="checkbox">
                    <input
                      type="checkbox"
                      checked={pgvector || (requirement?.extensions?.includes("pgvector") ?? false)}
                      disabled={requirement !== undefined}
                      onChange={(event) => setPgvector(event.target.checked)}
                    />
                    {t.services.withPgvector}
                  </label>
                )}
              </details>
            </>
          )}
        </fieldset>
        <div className="modal-actions">
          <button type="button" className="button" disabled={saving} onClick={onClose}>
            {t.common.cancel}
          </button>
          <button
            type="submit"
            className="button button-primary"
            disabled={saving || Boolean(redisConflict) || (!created && !redis && !version)}
          >
            {created ? t.workspace.retryLink : t.workspace.addTool}
          </button>
        </div>
      </form>
    </Modal>
  );
}
