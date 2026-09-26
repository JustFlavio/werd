import {
  Code2,
  ExternalLink,
  FlaskConical,
  FolderOpen,
  Play,
  Plus,
  Search,
  Square,
  SquareTerminal,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  type Category,
  type Editor,
  type Job,
  linkProject,
  listEditors,
  type Project,
  type ProjectInfo,
  projectEnv,
  removeProject,
  resolveProject,
  type ServiceInstance,
  setProjectDomain,
  setProjectNode,
  setProjectPhp,
  siteAction,
  siteInfo,
  unlinkProject,
} from "../api";
import { CATEGORIES, instancesFor } from "../categories";
import { AddSiteWizard } from "../components/AddSiteWizard";
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
  /** A site was added or created: select it and refresh. */
  onAdded: (id: string) => Promise<void>;
  onToggle: (project: Project) => void;
  onOpenSite: (project: Project) => void;
  onResetPorts: (project: Project) => void;
  onShowLogs: (project: Project) => void;
  onChanged: () => Promise<void>;
  onError: (cause: unknown) => void;
}

type Tab = "general" | "services" | "information";

export function Sites(props: SitesProps) {
  const { projects, selectedId, onSelect, onAdded, runtimes } = props;
  const t = useT();
  const [adding, setAdding] = useState(false);
  const [query, setQuery] = useState("");
  const [editors, setEditors] = useState<Editor[]>([]);
  useEffect(() => {
    void listEditors()
      .then(setEditors)
      .catch(() => setEditors([]));
  }, []);

  const selected = projects.find((project) => project.id === selectedId) ?? null;
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return projects;
    return projects.filter((project) =>
      [project.name, project.domain ?? "", project.path].some((text) => text.toLowerCase().includes(needle)),
    );
  }, [projects, query]);

  const addButton = (
    <button type="button" className="button button-primary" onClick={() => setAdding(true)}>
      <Plus size={15} /> {t.sites.add}
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
          <div className="site-list-pane">
            <label className="search">
              <Search size={14} aria-hidden />
              <input
                type="search"
                value={query}
                placeholder={t.sites.search}
                aria-label={t.sites.search}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <ul className="site-list">
              {visible.map((project) => (
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
              {visible.length === 0 && <li className="muted site-list-empty">{t.sites.noMatch}</li>}
            </ul>
          </div>
          <div className="split-detail">
            {selected ? (
              <SiteDetail key={selected.id} project={selected} editors={editors} {...props} />
            ) : (
              <p className="muted split-placeholder">{t.sites.selectHint}</p>
            )}
          </div>
        </div>
      )}

      {adding && (
        <AddSiteWizard
          runtimes={runtimes}
          onClose={() => setAdding(false)}
          onDone={async (id) => {
            await onAdded(id);
            onSelect(id);
          }}
        />
      )}
    </>
  );
}

function SiteDetail({
  project,
  editors,
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
}: SitesProps & { project: Project; editors: Editor[] }) {
  const t = useT();
  const [tab, setTab] = useState<Tab>("general");
  const [removing, setRemoving] = useState(false);
  const running = project.status === "running";
  const pending = project.requirements ?? [];
  const action = (name: string) => void siteAction(project.id, name).catch(onError);

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

      <div className="quick-actions">
        <button type="button" className="button" onClick={() => action("terminal")}>
          <SquareTerminal size={14} /> {t.sites.terminal}
        </button>
        <button type="button" className="button" onClick={() => action("tinker")}>
          <FlaskConical size={14} /> Tinker
        </button>
        {editors.map((editor) => (
          <button key={editor.id} type="button" className="button" onClick={() => action(`editor:${editor.id}`)}>
            <Code2 size={14} /> {editor.label}
          </button>
        ))}
        <button type="button" className="button" onClick={() => action("folder")}>
          <FolderOpen size={14} /> {t.sites.folderAction}
        </button>
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

      <div className="tabs" role="tablist">
        {(["general", "services", "information"] as const).map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={`tab ${tab === id ? "active" : ""}`}
            onClick={() => setTab(id)}
          >
            {t.sites.tabs[id]}
          </button>
        ))}
      </div>

      {tab === "general" && (
        <GeneralTab
          project={project}
          runtimes={runtimes}
          busy={busy}
          onChanged={onChanged}
          onError={onError}
          onShowLogs={onShowLogs}
          onResetPorts={onResetPorts}
          onRemove={() => setRemoving(true)}
        />
      )}
      {tab === "services" && (
        <ServicesTab project={project} instances={instances} onChanged={onChanged} onError={onError} />
      )}
      {tab === "information" && <InformationTab project={project} />}

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
                void removeProject(project.id).then(onChanged).catch(onError);
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

function GeneralTab({
  project,
  runtimes,
  busy,
  onChanged,
  onError,
  onShowLogs,
  onResetPorts,
  onRemove,
}: {
  project: Project;
  runtimes: Runtimes;
  busy: string | null;
  onChanged: () => Promise<void>;
  onError: (cause: unknown) => void;
  onShowLogs: (project: Project) => void;
  onResetPorts: (project: Project) => void;
  onRemove: () => void;
}) {
  const t = useT();
  const [domain, setDomain] = useState(project.domain ?? "");
  useEffect(() => setDomain(project.domain ?? ""), [project.domain]);
  const running = project.status === "running";
  const phpLines = runtimes.rows.filter((row) => row.product === "php" && row.installed);
  const nodeLines = runtimes.rows.filter((row) => row.product === "node" && row.installed);
  const change = (action: () => Promise<unknown>) => void action().then(onChanged).catch(onError);

  return (
    <>
      <Section title={t.sites.general}>
        <dl className="fields">
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
          <dt>{t.sites.path}</dt>
          <dd className="mono">{project.path}</dd>
          {project.parked && (
            <>
              <dt>{t.sites.parkedIn}</dt>
              <dd className="mono">{project.parked}</dd>
            </>
          )}
        </dl>
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
            onClick={onRemove}
          >
            <Trash2 size={14} /> {t.sites.remove}
          </button>
        </div>
      </Section>
    </>
  );
}

function ServicesTab({
  project,
  instances,
  onChanged,
  onError,
}: {
  project: Project;
  instances: ServiceInstance[];
  onChanged: () => Promise<void>;
  onError: (cause: unknown) => void;
}) {
  const t = useT();
  const [env, setEnv] = useState("");
  const links = project.links ?? {};
  const change = (action: () => Promise<unknown>) => void action().then(onChanged).catch(onError);

  // The .env block changes when the site URL or its links change.
  const envKey = `${project.url ?? ""}|${project.domain ?? ""}|${JSON.stringify(links)}`;
  useEffect(() => {
    if (!envKey) return;
    void projectEnv(project.id)
      .then(setEnv)
      .catch(() => setEnv(""));
  }, [project.id, envKey]);

  return (
    <>
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
    </>
  );
}

function InformationTab({ project }: { project: Project }) {
  const t = useT();
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    void siteInfo(project.id)
      .then(setInfo)
      .catch(() => setFailed(true));
  }, [project.id]);

  if (failed) return <p className="muted">{t.sites.infoUnavailable}</p>;
  if (!info) return <p className="muted">{t.common.loading}</p>;
  const laravel = info.php_packages.find((item) => item.name === "laravel/framework");
  const others = info.php_packages.filter((item) => item.name !== "laravel/framework");
  const yes = (value: boolean) => (value ? t.sites.yes : t.sites.no);

  return (
    <>
      <Section title={t.sites.stack}>
        <dl className="fields">
          <dt>Laravel</dt>
          <dd>{laravel?.version ?? "—"}</dd>
          <dt>{t.sites.phpRequired}</dt>
          <dd className="mono">{info.php_constraint ?? "—"}</dd>
          <dt>Node.js</dt>
          <dd className="mono">{info.node ?? "—"}</dd>
          <dt>werd.yml</dt>
          <dd>{yes(info.werd_yml)}</dd>
          <dt>.env</dt>
          <dd>{yes(info.env_file)}</dd>
        </dl>
      </Section>
      <Section title={t.sites.phpPackages}>
        {others.length ? <PackageList packages={others} /> : <p className="muted">{t.sites.noPackages}</p>}
      </Section>
      <Section title={t.sites.frontend}>
        {info.js_packages.length ? (
          <PackageList packages={info.js_packages} />
        ) : (
          <p className="muted">{t.sites.noPackages}</p>
        )}
      </Section>
    </>
  );
}

function PackageList({ packages }: { packages: ProjectInfo["php_packages"] }) {
  return (
    <ul className="chips">
      {packages.map((item) => (
        <li key={item.name} className="chip" title={item.name}>
          <strong>{item.label}</strong>
          {item.version && <span className="mono">{item.version}</span>}
        </li>
      ))}
    </ul>
  );
}
