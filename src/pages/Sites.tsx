import { ExternalLink, FolderPlus, Play, Square, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  type Category,
  type Job,
  linkProject,
  type Project,
  pickFolder,
  projectEnv,
  removeProject,
  resolveProject,
  type ServiceInstance,
  setProjectDomain,
  setProjectNode,
  setProjectPhp,
  unlinkProject,
} from "../api";
import { CATEGORIES, instancesFor } from "../categories";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { CopyButton, EmptyState, Modal, PageHeader, Section, StatusDot } from "../ui";

export interface SitesProps {
  projects: Project[];
  instances: ServiceInstance[];
  runtimes: Runtimes;
  busy: string | null;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onAdd: (path: string) => Promise<boolean>;
  onToggle: (project: Project) => void;
  onOpenSite: (project: Project) => void;
  onResetPorts: (project: Project) => void;
  onShowLogs: (project: Project) => void;
  onChanged: () => Promise<void>;
  onError: (cause: unknown) => void;
}

export function Sites(props: SitesProps) {
  const { projects, selectedId, onSelect, onAdd, busy } = props;
  const t = useT();
  const [adding, setAdding] = useState(false);
  const selected = projects.find((project) => project.id === selectedId) ?? null;
  const addButton = (
    <button type="button" className="button button-primary" onClick={() => setAdding(true)}>
      <FolderPlus size={15} /> {t.sites.add}
    </button>
  );

  return (
    <>
      <PageHeader title={t.sites.title}>{projects.length > 0 && addButton}</PageHeader>

      {projects.length === 0 ? (
        <EmptyState title={t.sites.emptyTitle} description={t.sites.emptyDescription}>
          {addButton}
        </EmptyState>
      ) : (
        <div className="split">
          <ul className="site-list">
            {projects.map((project) => (
              <li key={project.id}>
                <button
                  type="button"
                  aria-current={project.id === selectedId ? "true" : undefined}
                  className={`site-row ${project.id === selectedId ? "selected" : ""}`}
                  onClick={() => onSelect(project.id)}
                >
                  <StatusDot status={project.status} />
                  <span className="site-row-text">
                    <strong>{project.name}</strong>
                    <small>{project.domain ?? project.path}</small>
                  </span>
                </button>
              </li>
            ))}
          </ul>
          <div className="split-detail">
            {selected ? (
              <SiteDetail key={selected.id} project={selected} {...props} />
            ) : (
              <p className="muted split-placeholder">{t.sites.selectHint}</p>
            )}
          </div>
        </div>
      )}

      {adding && (
        <AddSiteModal
          busy={busy === "add"}
          onClose={() => setAdding(false)}
          onSubmit={async (path) => {
            if (await onAdd(path)) setAdding(false);
          }}
        />
      )}
    </>
  );
}

function SiteDetail({
  project,
  instances,
  runtimes,
  busy,
  onToggle,
  onOpenSite,
  onResetPorts,
  onShowLogs,
  onChanged,
  onError,
  onSelect,
}: SitesProps & { project: Project }) {
  const t = useT();
  const [env, setEnv] = useState("");
  const [removing, setRemoving] = useState(false);
  const [domain, setDomain] = useState(project.domain ?? "");
  useEffect(() => setDomain(project.domain ?? ""), [project.domain]);
  const running = project.status === "running";
  const links = project.links ?? {};
  const pending = project.requirements ?? [];
  const phpLines = runtimes.rows.filter((row) => row.product === "php" && row.installed);
  const nodeLines = runtimes.rows.filter((row) => row.product === "node" && row.installed);

  // The .env block changes when the site URL or its links change.
  const envKey = `${project.url ?? ""}|${JSON.stringify(project.links ?? {})}`;
  useEffect(() => {
    if (!envKey) return;
    void projectEnv(project.id)
      .then(setEnv)
      .catch(() => setEnv(""));
  }, [project.id, envKey]);

  const change = (action: () => Promise<unknown>) => void action().then(onChanged).catch(onError);

  return (
    <div className="page-body">
      <div className="detail-head">
        <div>
          <h2>{project.name}</h2>
          <StatusDot status={project.status} label />
        </div>
        <div className="button-row">
          {running && (
            <button type="button" className="button" onClick={() => onOpenSite(project)}>
              <ExternalLink size={14} /> {t.common.open}
            </button>
          )}
          <button
            type="button"
            className={`button ${running ? "" : "button-primary"}`}
            disabled={busy === project.id || pending.length > 0}
            title={pending.length > 0 ? t.sites.resolveFirst : undefined}
            onClick={() => onToggle(project)}
          >
            {running ? (
              <>
                <Square size={13} /> {t.sites.stop}
              </>
            ) : (
              <>
                <Play size={13} /> {busy === project.id ? t.sites.starting : t.sites.start}
              </>
            )}
          </button>
        </div>
      </div>

      {project.error && <div className="callout callout-error">{project.error}</div>}

      {pending.length > 0 && (
        <div className="callout callout-warn">
          <p>{t.sites.pendingIntro}</p>
          <ul className="pending-list">
            {pending.map((requirement) => (
              <li key={requirement.category}>
                <strong>{t.services.categories[requirement.category]}</strong>: {requirement.product}
                {requirement.line ? ` ${requirement.line}` : ""}
                {requirement.extensions?.length ? ` + ${requirement.extensions.join(", ")}` : ""}
              </li>
            ))}
          </ul>
          <button
            type="button"
            className="button button-primary"
            onClick={() =>
              void resolveProject(project.id)
                .then(({ jobs }) => {
                  for (const job of jobs as Job[]) runtimes.track(job);
                  return onChanged();
                })
                .catch(onError)
            }
          >
            {t.sites.createMissing}
          </button>
        </div>
      )}

      <Section title={t.sites.general}>
        <dl className="fields">
          <dt>{t.sites.path}</dt>
          <dd className="mono">{project.path}</dd>
          {project.parked && (
            <>
              <dt>{t.sites.parkedIn}</dt>
              <dd className="mono">{project.parked}</dd>
            </>
          )}
          <dt>{t.sites.domain}</dt>
          <dd>
            <form
              className="inline-form"
              onSubmit={(event) => {
                event.preventDefault();
                change(() => setProjectDomain(project.id, domain));
              }}
            >
              <input
                className="input mono"
                value={domain}
                aria-label={t.sites.domain}
                onChange={(event) => setDomain(event.target.value)}
              />
              {domain.trim() && domain !== project.domain && (
                <button type="submit" className="button button-small">
                  {t.common.save}
                </button>
              )}
            </form>
          </dd>
          <dt>{t.sites.url}</dt>
          <dd className="mono">{project.url ?? <span className="muted">{t.sites.urlPending}</span>}</dd>
          <dt>PHP</dt>
          <dd>
            <select
              className="select"
              value={project.php}
              aria-label="PHP"
              onChange={(event) => change(() => setProjectPhp(project.id, event.target.value))}
            >
              {!phpLines.some((row) => row.line === project.php) && (
                <option value={project.php}>
                  PHP {project.php} ({t.sites.notInstalled})
                </option>
              )}
              {phpLines.map((row) => (
                <option key={row.line} value={row.line}>
                  PHP {row.line}
                </option>
              ))}
            </select>
            {running && <span className="muted">{t.sites.nextStart}</span>}
          </dd>
          <dt>Node.js</dt>
          <dd>
            <select
              className="select"
              value={project.node ?? ""}
              aria-label="Node.js"
              onChange={(event) => change(() => setProjectNode(project.id, event.target.value || null))}
            >
              <option value="">{t.sites.nodeDefault}</option>
              {nodeLines.map((row) => (
                <option key={row.line} value={row.line}>
                  Node {row.line}
                </option>
              ))}
            </select>
          </dd>
        </dl>
      </Section>

      <Section title={t.sites.services} description={t.sites.servicesHint}>
        <table className="table">
          <thead>
            <tr>
              <th>{t.services.category}</th>
              <th>{t.sites.service}</th>
              <th>{t.sites.database}</th>
            </tr>
          </thead>
          <tbody>
            {CATEGORIES.map((category: Category) => {
              const link = links[category];
              const choices = instancesFor(category, instances);
              const linked = instances.find((instance) => instance.id === link?.instance);
              return (
                <tr key={category}>
                  <td>{t.services.categories[category]}</td>
                  <td>
                    <span className="link-cell">
                      {linked && <StatusDot status={linked.status} />}
                      <select
                        className="select"
                        value={link?.instance ?? ""}
                        aria-label={t.services.categories[category]}
                        onChange={(event) =>
                          change(() =>
                            event.target.value
                              ? linkProject(project.id, category, event.target.value)
                              : unlinkProject(project.id, category),
                          )
                        }
                      >
                        <option value="">{t.sites.none}</option>
                        {choices.map((instance) => (
                          <option key={instance.id} value={instance.id}>
                            {instance.name}
                          </option>
                        ))}
                      </select>
                    </span>
                  </td>
                  <td className="mono muted">{link?.database ?? "—"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </Section>

      <Section title={t.sites.envTitle} description={t.sites.envHint} action={env && <CopyButton text={env} />}>
        {env ? <pre className="code">{env}</pre> : <p className="muted">{t.sites.envPending}</p>}
      </Section>

      <Section title={t.sites.maintenance}>
        <div className="button-row">
          <button type="button" className="button" onClick={() => onShowLogs(project)}>
            {t.sites.viewLogs}
          </button>
          <button
            type="button"
            className="button"
            disabled={running || busy === project.id}
            title={running ? t.sites.stopFirst : undefined}
            onClick={() => onResetPorts(project)}
          >
            {t.sites.resetPorts}
          </button>
          <button
            type="button"
            className="button"
            disabled={Boolean(project.parked)}
            title={project.parked ? t.sites.removeParked : undefined}
            onClick={() => setRemoving(true)}
          >
            <Trash2 size={14} /> {t.sites.remove}
          </button>
        </div>
      </Section>

      {removing && (
        <Modal title={t.sites.removeTitle(project.name)} onClose={() => setRemoving(false)}>
          <p className="muted">{t.sites.removeHint}</p>
          <div className="modal-actions">
            <button type="button" className="button" onClick={() => setRemoving(false)}>
              {t.common.cancel}
            </button>
            <button
              type="button"
              className="button button-danger"
              onClick={() => {
                setRemoving(false);
                onSelect(null);
                change(() => removeProject(project.id));
              }}
            >
              {t.sites.remove}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}

function AddSiteModal({
  busy,
  onClose,
  onSubmit,
}: {
  busy: boolean;
  onClose: () => void;
  onSubmit: (path: string) => void;
}) {
  const t = useT();
  const [path, setPath] = useState("");
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  return (
    <Modal title={t.sites.add} onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit(path.trim());
        }}
      >
        <p className="muted">{t.sites.addDialogHint(<code key="file">werd.yml</code>)}</p>
        <label className="field">
          <span>{t.sites.folder}</span>
          <span className="inline-form">
            <input
              ref={input}
              className="input mono"
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder={t.sites.folderPlaceholder}
            />
            <button
              type="button"
              className="button"
              onClick={() =>
                void pickFolder(t.sites.folderPick).then((picked) => {
                  if (picked) setPath(picked);
                })
              }
            >
              {t.sites.browse}
            </button>
          </span>
        </label>
        <div className="modal-actions">
          <button type="button" className="button" onClick={onClose}>
            {t.common.cancel}
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !path.trim()}>
            {busy ? t.sites.linking : t.sites.addConfirm}
          </button>
        </div>
      </form>
    </Modal>
  );
}
