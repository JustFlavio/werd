import { ExternalLink, FolderPlus, Play, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { type Project, projectEnv } from "../api";
import { useT } from "../i18n";
import { describePorts, SERVICES, serviceUrl } from "../services";
import { CopyButton, EmptyState, Modal, PageHeader, Section, StatusDot } from "../ui";

export function Sites({
  projects,
  busy,
  selectedId,
  onSelect,
  onAdd,
  onToggle,
  onOpenSite,
  onOpenUrl,
  onResetPorts,
  onShowLogs,
}: {
  projects: Project[];
  busy: string | null;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onAdd: (path: string) => Promise<boolean>;
  onToggle: (project: Project) => void;
  onOpenSite: (project: Project) => void;
  onOpenUrl: (url: string) => void;
  onResetPorts: (project: Project) => void;
  onShowLogs: (project: Project) => void;
}) {
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
                    <small>{project.path}</small>
                  </span>
                </button>
              </li>
            ))}
          </ul>
          <div className="split-detail">
            {selected ? (
              <SiteDetail
                key={selected.id}
                project={selected}
                busy={busy === selected.id}
                onToggle={onToggle}
                onOpenSite={onOpenSite}
                onOpenUrl={onOpenUrl}
                onResetPorts={onResetPorts}
                onShowLogs={onShowLogs}
              />
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
  busy,
  onToggle,
  onOpenSite,
  onOpenUrl,
  onResetPorts,
  onShowLogs,
}: {
  project: Project;
  busy: boolean;
  onToggle: (project: Project) => void;
  onOpenSite: (project: Project) => void;
  onOpenUrl: (url: string) => void;
  onResetPorts: (project: Project) => void;
  onShowLogs: (project: Project) => void;
}) {
  const t = useT();
  const [env, setEnv] = useState("");
  const running = project.status === "running";

  useEffect(() => {
    if (!project.ports) {
      setEnv("");
      return;
    }
    void projectEnv(project.id)
      .then(setEnv)
      .catch(() => setEnv(""));
  }, [project.id, project.ports]);

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
            disabled={busy}
            onClick={() => onToggle(project)}
          >
            {running ? (
              <>
                <Square size={13} /> {t.sites.stop}
              </>
            ) : (
              <>
                <Play size={13} /> {busy ? t.sites.starting : t.sites.start}
              </>
            )}
          </button>
        </div>
      </div>

      {project.error && <div className="callout callout-error">{project.error}</div>}

      <Section title={t.sites.general}>
        <dl className="fields">
          <dt>{t.sites.path}</dt>
          <dd className="mono">{project.path}</dd>
          <dt>{t.sites.url}</dt>
          <dd className="mono">{project.url ?? <span className="muted">{t.sites.urlPending}</span>}</dd>
          <dt>PHP</dt>
          <dd>{project.php}</dd>
        </dl>
      </Section>

      <Section title={t.sites.services} description={t.sites.servicesHint}>
        {project.services.length === 0 ? (
          <p className="muted">{t.sites.noServices(<code key="file">werd.yml</code>)}</p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>{t.sites.service}</th>
                <th>{t.sites.version}</th>
                <th>{t.sites.ports}</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {project.services.map((service) => {
                const url = serviceUrl(project, service);
                return (
                  <tr key={service}>
                    <td>{SERVICES[service].label}</td>
                    <td className="muted">{SERVICES[service].version}</td>
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
            disabled={running || busy}
            title={running ? t.sites.stopFirst : undefined}
            onClick={() => onResetPorts(project)}
          >
            {t.sites.resetPorts}
          </button>
        </div>
      </Section>
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
          <input
            ref={input}
            className="input mono"
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder={t.sites.folderPlaceholder}
          />
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
