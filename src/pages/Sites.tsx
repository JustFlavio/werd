import { ExternalLink, FolderPlus, Play, Square } from "lucide-react";
import { useEffect, useState } from "react";
import { type Project, projectEnv } from "../api";
import { PORT_LABELS, SERVICES, serviceUrl } from "../services";
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
  const [adding, setAdding] = useState(false);
  const selected = projects.find((project) => project.id === selectedId) ?? null;

  return (
    <>
      <PageHeader title="Siti">
        {projects.length > 0 && (
          <button className="button button-primary" onClick={() => setAdding(true)}>
            <FolderPlus size={15} /> Aggiungi sito
          </button>
        )}
      </PageHeader>

      {projects.length === 0 ? (
        <EmptyState
          title="Nessun sito"
          description="Collega la cartella di un progetto Laravel per servirlo in HTTPS con i suoi servizi."
        >
          <button className="button button-primary" onClick={() => setAdding(true)}>
            <FolderPlus size={15} /> Aggiungi sito
          </button>
        </EmptyState>
      ) : (
        <div className="split">
          <div className="site-list" role="list">
            {projects.map((project) => (
              <button
                key={project.id}
                role="listitem"
                className={`site-row ${project.id === selectedId ? "selected" : ""}`}
                onClick={() => onSelect(project.id)}
              >
                <StatusDot status={project.status} />
                <span className="site-row-text">
                  <strong>{project.name}</strong>
                  <small>{project.path}</small>
                </span>
              </button>
            ))}
          </div>
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
              <p className="muted split-placeholder">Seleziona un sito per vederne i dettagli.</p>
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
            <button className="button" onClick={() => onOpenSite(project)}>
              <ExternalLink size={14} /> Apri
            </button>
          )}
          <button
            className={`button ${running ? "" : "button-primary"}`}
            disabled={busy}
            onClick={() => onToggle(project)}
          >
            {running ? (
              <>
                <Square size={13} /> Ferma
              </>
            ) : (
              <>
                <Play size={13} /> {busy ? "Avvio…" : "Avvia"}
              </>
            )}
          </button>
        </div>
      </div>

      {project.error && <div className="callout callout-error">{project.error}</div>}

      <Section title="Generale">
        <dl className="fields">
          <dt>Percorso</dt>
          <dd className="mono">{project.path}</dd>
          <dt>URL</dt>
          <dd className="mono">{project.url ?? <span className="muted">Disponibile dopo l’avvio</span>}</dd>
          <dt>PHP</dt>
          <dd>{project.php}</dd>
        </dl>
      </Section>

      <Section title="Servizi" description="Ogni sito ha istanze, porte e dati propri.">
        {project.services.length === 0 ? (
          <p className="muted">
            Nessun servizio configurato in <code>werd.yml</code>.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Servizio</th>
                <th>Versione</th>
                <th>Porte</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {project.services.map((service) => {
                const meta = SERVICES[service];
                const url = serviceUrl(project, service);
                return (
                  <tr key={service}>
                    <td>{meta.label}</td>
                    <td className="muted">{meta.version}</td>
                    <td className="mono muted">
                      {meta.ports
                        .map((key) => (project.ports?.[key] ? `${PORT_LABELS[key]} ${project.ports[key]}` : null))
                        .filter(Boolean)
                        .join(" · ") || "—"}
                    </td>
                    <td className="cell-action">
                      {url && (
                        <button className="button button-small" onClick={() => onOpenUrl(url)}>
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

      <Section
        title="Variabili .env"
        description="Copia questi valori nel .env del progetto. Werd non modifica i tuoi file."
        action={env && <CopyButton text={env} />}
      >
        {env ? <pre className="code">{env}</pre> : <p className="muted">Le porte vengono assegnate al primo avvio.</p>}
      </Section>

      <Section title="Manutenzione">
        <div className="button-row">
          <button className="button" onClick={() => onShowLogs(project)}>
            Vedi log
          </button>
          <button
            className="button"
            disabled={running || busy}
            title={running ? "Ferma prima il sito" : undefined}
            onClick={() => onResetPorts(project)}
          >
            Riassegna porte
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
  const [path, setPath] = useState("");
  return (
    <Modal title="Aggiungi sito" onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit(path.trim());
        }}
      >
        <p className="muted">
          Indica la cartella di un progetto Laravel. Werd leggerà <code>werd.yml</code> o ne creerà uno con i valori
          predefiniti.
        </p>
        <label className="field">
          <span>Cartella del progetto</span>
          <input
            className="input mono"
            autoFocus
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder="C:\Users\nome\Developer\progetto"
          />
        </label>
        <div className="modal-actions">
          <button type="button" className="button" onClick={onClose}>
            Annulla
          </button>
          <button type="submit" className="button button-primary" disabled={busy || !path.trim()}>
            {busy ? "Collegamento…" : "Aggiungi"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
