import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity, ArrowUpRight, Box, ChevronDown, ChevronRight, CircleHelp,
  Command, Database, FolderOpen, HardDrive, HeartPulse, Mail, Plus,
  RefreshCw, Search, Server, Settings2, Square, Zap,
} from "lucide-react";
import {
  addProject, doctor, installRuntime, listProjects, openSite, projectEnv, projectLogs, resetPorts, runtimes, startProject, stopProject, trustCa,
  type DoctorResult, type Project, type RuntimeInfo, type ServiceName,
} from "./api";

type Page = "overview" | "projects" | "services" | "diagnostics" | "settings";

const serviceNames: Record<ServiceName, string> = {
  postgres: "PostgreSQL + pgvector",
  redis: "Redis",
  mailpit: "Mailpit",
  rustfs: "RustFS",
};

const serviceIcons: Record<ServiceName, typeof Database> = {
  postgres: Database,
  redis: Zap,
  mailpit: Mail,
  rustfs: HardDrive,
};

function Badge({ status }: { status: Project["status"] }) {
  const labels = { running: "In esecuzione", starting: "Avvio", stopped: "Fermo", error: "Errore" };
  return <span className={`status status-${status}`}><i />{labels[status]}</span>;
}

function PageHeader({ eyebrow, title, description, children }: {
  eyebrow: string; title: string; description: string; children?: React.ReactNode;
}) {
  return <div className="page-header">
    <div><div className="eyebrow">{eyebrow}</div><h1>{title}</h1><p>{description}</p></div>
    {children && <div className="header-actions">{children}</div>}
  </div>;
}

export default function App() {
  const [page, setPage] = useState<Page>("overview");
  const [projects, setProjects] = useState<Project[]>([]);
  const [daemonVersion, setDaemonVersion] = useState("—");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [newPath, setNewPath] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [logSource, setLogSource] = useState("werd");
  const [checks, setChecks] = useState<DoctorResult[]>([]);
  const [runtimeList, setRuntimeList] = useState<RuntimeInfo[]>([]);
  const [envSnippet, setEnvSnippet] = useState("");
  const [filter, setFilter] = useState("");

  const refresh = useCallback(async () => {
    try {
      const snapshot = await listProjects();
      setProjects(snapshot.projects);
      setDaemonVersion(snapshot.daemon_version);
      setConnectionError(null);
    } catch (cause) {
      setConnectionError(String(cause));
    }
  }, []);

  useEffect(() => { void refresh(); const timer = window.setInterval(() => void refresh(), 2500); return () => window.clearInterval(timer); }, [refresh]);
  useEffect(() => {
    if (!selectedId) { setLogs([]); return; }
    void projectLogs(selectedId, logSource).then(setLogs).catch(() => setLogs([]));
  }, [selectedId, projects, logSource]);
  useEffect(() => {
    if (page !== "diagnostics") return;
    void doctor().then(setChecks).catch((cause) => setError(String(cause)));
    void runtimes().then(setRuntimeList).catch((cause) => setError(String(cause)));
  }, [page]);
  useEffect(() => {
    const project = projects.find((item) => item.id === selectedId);
    if (!project || !project.ports) { setEnvSnippet(""); return; }
    void projectEnv(project.id).then(setEnvSnippet).catch(() => setEnvSnippet(""));
  }, [selectedId, projects]);

  async function install(id: string) {
    setBusy(`install-${id}`);
    try {
      await installRuntime(id);
      setRuntimeList(await runtimes());
      setChecks(await doctor());
      setError(null);
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(null); }
  }

  const selected = projects.find((project) => project.id === selectedId) ?? null;
  const runningCount = projects.filter((project) => project.status === "running").length;
  const visible = projects.filter((project) => `${project.name} ${project.path}`.toLowerCase().includes(filter.toLowerCase()));
  const activeServices = useMemo(() => new Set(projects.filter((project) => project.status === "running").flatMap((project) => project.services)), [projects]);

  async function submitProject(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy("add");
    try {
      const project = await addProject(newPath.trim());
      setError(null);
      setNewPath(""); setShowAdd(false); setSelectedId(project.id); setPage("projects");
      await refresh();
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(null); }
  }

  async function toggleProject(project: Project) {
    setBusy(project.id);
    try {
      if (project.status === "running") await stopProject(project.id);
      else await startProject(project.id);
      setError(null);
      await refresh();
    } catch (cause) { setError(String(cause)); }
    finally { setBusy(null); }
  }

  async function reassignProjectPorts(project: Project) {
    setBusy(project.id);
    try { await resetPorts(project.id); await refresh(); setNotice("Le porte verranno riassegnate al prossimo avvio. Aggiorna le variabili .env mostrate nel progetto."); }
    catch (cause) { setError(String(cause)); }
    finally { setBusy(null); }
  }

  const nav: { id: Page; label: string; icon: typeof Box }[] = [
    { id: "overview", label: "Panoramica", icon: Box },
    { id: "projects", label: "Progetti", icon: FolderOpen },
    { id: "services", label: "Servizi", icon: Server },
    { id: "diagnostics", label: "Diagnostica", icon: HeartPulse },
    { id: "settings", label: "Impostazioni", icon: Settings2 },
  ];

  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><span>W</span></span><div><strong>werd<span className="brand-dot">.</span></strong><small>LOCAL DEVELOPMENT</small></div></div>
      <div className="sidebar-section-label">WORKSPACE</div>
      <nav aria-label="Navigazione principale">
        {nav.map(({ id, label, icon: Icon }) => <button key={id} className={`nav-item ${page === id ? "active" : ""}`} onClick={() => setPage(id)}><Icon size={18} strokeWidth={1.8} /><span>{label}</span>{id === "projects" && projects.length > 0 && <b>{projects.length}</b>}</button>)}
      </nav>
      <div className="sidebar-bottom">
        <div className="sidebar-help"><CircleHelp size={18} /><div><strong>Serve una mano?</strong><span>Controlla la diagnostica</span></div><ChevronRight size={15} /></div>
        <div className="sidebar-version"><span className="version-light" /> Werd {daemonVersion}</div>
      </div>
    </aside>

    <main className="main">
      <div className="topbar"><div className="breadcrumb">WORKSPACE <ChevronRight size={13} /> <strong>{nav.find((item) => item.id === page)?.label}</strong></div><div className="topbar-right"><span className="local-badge"><span /> Ambiente locale</span><button className="icon-button" title="Aggiorna" onClick={() => void refresh()}><RefreshCw size={16} /></button></div></div>
      {(error || connectionError) && <div role="alert" className="error-banner"><strong>Operazione non riuscita</strong><span>{error || connectionError}</span><button onClick={() => { setError(null); setConnectionError(null); }}>Chiudi</button></div>}
      {notice && <div role="status" className="notice-banner"><span>{notice}</span><button onClick={() => setNotice(null)}>Chiudi</button></div>}

      {page === "overview" && <>
        <PageHeader eyebrow="IL TUO AMBIENTE" title="Tutto pronto per creare." description="Un posto solo per i tuoi progetti Laravel e i servizi locali."><button className="primary-button" onClick={() => setShowAdd(true)}><Plus size={17} /> Aggiungi progetto</button></PageHeader>
        <section className="metrics">
          <div className="metric"><div className="metric-icon purple"><FolderOpen size={20} /></div><span>Progetti</span><strong>{projects.length.toString().padStart(2, "0")}</strong><small>{runningCount} in esecuzione</small></div>
          <div className="metric"><div className="metric-icon green"><Activity size={20} /></div><span>Servizi attivi</span><strong>{activeServices.size.toString().padStart(2, "0")}</strong><small>Associati ai progetti</small></div>
          <div className="metric"><div className="metric-icon orange"><Database size={20} /></div><span>Database</span><strong>{projects.filter((project) => project.services.includes("postgres")).length.toString().padStart(2, "0")}</strong><small>Configurati nei progetti</small></div>
        </section>
        <div className="section-heading"><div><h2>I tuoi progetti</h2><p>Apri, avvia e controlla ogni ambiente.</p></div><button className="text-button" onClick={() => setPage("projects")}>Vedi tutti <ArrowUpRight size={16} /></button></div>
        {projects.length === 0 ? <div className="empty-state"><div className="empty-illustration"><div className="empty-inner"><Command size={30} /></div></div><h3>Il tuo primo progetto parte da qui</h3><p>Collega una cartella Laravel per vedere il sito e tutti i servizi in questa dashboard.</p><button className="secondary-button" onClick={() => setShowAdd(true)}><Plus size={16} /> Collega un progetto</button></div> : <div className="project-grid">{projects.slice(0, 6).map((project) => <ProjectCard key={project.id} project={project} busy={busy === project.id} onSelect={() => { setSelectedId(project.id); setPage("projects"); }} onToggle={() => void toggleProject(project)} />)}</div>}
      </>}

      {page === "projects" && <>
        <PageHeader eyebrow="WORKSPACE" title="Progetti" description="Ogni progetto ha il suo ambiente, i suoi servizi e i suoi dati."><button className="primary-button" onClick={() => setShowAdd(true)}><Plus size={17} /> Aggiungi progetto</button></PageHeader>
        <div className="toolbar"><label className="search"><Search size={17} /><input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="Cerca un progetto..." /></label><span>{visible.length} progetti</span></div>
        {visible.length === 0 ? <div className="empty-state compact"><FolderOpen size={35} /><h3>Nessun progetto da mostrare</h3><p>Aggiungi una cartella Laravel per iniziare.</p></div> : <div className="project-layout"><div className="project-list">{visible.map((project) => <button key={project.id} className={`project-row ${selectedId === project.id ? "selected" : ""}`} onClick={() => setSelectedId(project.id)}><span className="project-avatar">{project.name.slice(0, 1).toUpperCase()}</span><span className="project-row-body"><strong>{project.name}</strong><small>{project.path}</small></span><Badge status={project.status} /><ChevronRight size={16} /></button>)}</div><div className="project-detail">{selected ? <><div className="detail-top"><span className="project-avatar large">{selected.name.slice(0, 1).toUpperCase()}</span><Badge status={selected.status} /></div><h2>{selected.name}</h2><p className="muted path">{selected.path}</p><div className="detail-actions"><button className="primary-button" disabled={busy === selected.id} onClick={() => void toggleProject(selected)}>{selected.status === "running" ? <Square size={15} /> : <Zap size={16} />}{selected.status === "running" ? "Ferma progetto" : "Avvia progetto"}</button>{selected.url && <button className="secondary-button" onClick={() => void openSite(selected.id).catch((cause) => setError(String(cause)))}>Apri sito <ArrowUpRight size={15} /></button>}</div>{selected.error && <div className="detail-error">{selected.error}</div>}{selected.status !== "running" && <button className="text-button reset-ports" disabled={busy === selected.id} onClick={() => void reassignProjectPorts(selected)}>Riassegna porte</button>}<div className="detail-divider" /><h3>Ambiente</h3><div className="detail-line"><span>PHP</span><strong>{selected.php}</strong></div><div className="detail-line"><span>Dominio</span><strong>{selected.url ?? "Disponibile dopo l’avvio"}</strong></div><h3 className="detail-subtitle">Servizi</h3><div className="chips">{selected.services.map((service) => <span key={service}>{serviceNames[service]}</span>)}</div>{envSnippet && <><div className="detail-divider" /><h3>Variabili .env</h3><p className="muted">Copia i valori nel tuo progetto; Werd non modifica il file.</p><pre className="env-code">{envSnippet}</pre></>}<div className="detail-divider" /><div className="logs-title"><h3>Log recenti</h3><select value={logSource} onChange={(event) => setLogSource(event.target.value)} aria-label="Sorgente log">{["werd", "postgres", "redis", "mailpit", "rustfs", "php", "caddy"].map((name) => <option key={name} value={name}>{name}</option>)}</select></div><div className="logs">{logs.length ? logs.slice(-12).map((line, index) => <div key={index}>{line}</div>) : <span>Nessun evento registrato.</span>}</div></> : <div className="detail-placeholder"><FolderOpen size={30} /><h3>Seleziona un progetto</h3><p>Qui troverai controlli, collegamenti e log.</p></div>}</div></div>}
      </>}

      {page === "services" && <><PageHeader eyebrow="STACK LOCALE" title="Servizi" description="I servizi si accendono insieme al progetto che ne ha bisogno." /><div className="service-grid">{(Object.keys(serviceNames) as ServiceName[]).map((service) => { const Icon = serviceIcons[service]; const count = projects.filter((project) => project.services.includes(service)).length; return <div className="service-card" key={service}><span className={`service-icon ${service}`}><Icon size={22} /></span><div><h3>{serviceNames[service]}</h3><p>{count} {count === 1 ? "progetto configurato" : "progetti configurati"}</p></div><span className={`service-indicator ${activeServices.has(service) ? "on" : ""}`}>{activeServices.has(service) ? "Attivo" : "Inattivo"}</span></div>; })}</div><div className="info-panel"><div className="info-icon"><Server size={21} /></div><div><strong>Servizi isolati per progetto</strong><p>Ogni ambiente usa porte e dati propri. I servizi degli altri progetti rimangono disponibili quando ne fermi uno.</p></div></div></>}

      {page === "diagnostics" && <><PageHeader eyebrow="SALUTE AMBIENTE" title="Diagnostica" description="Controlli e indicazioni per risolvere i problemi locali."><button className="secondary-button" onClick={() => void doctor().then(setChecks)}><RefreshCw size={16} /> Ricontrolla</button></PageHeader><div className="checks">{checks.length ? checks.map((check) => <div className="check-row" key={check.label}><span className={`check-dot ${check.ok ? "ok" : "fail"}`} /><div><strong>{check.label}</strong><p>{check.detail}</p></div></div>) : <div className="empty-state compact"><HeartPulse size={32} /><h3>Controlli non disponibili</h3><p>Apri l’app desktop per verificare l’ambiente.</p></div>}</div></>}

      {page === "diagnostics" && <div className="runtime-panel">
        <h2>Runtime disponibili</h2><p>Scaricati solo su richiesta, con versione e checksum fissati.</p>
        {runtimeList.map((runtime) => <div className="runtime-row" key={runtime.id}><div><strong>{runtime.id} {runtime.version}</strong><small>{runtime.note}</small></div><span>{runtime.installed ? "Installato" : "Da installare"}</span><button className="secondary-button" disabled={runtime.installed || busy === `install-${runtime.id}`} onClick={() => void install(runtime.id)}>{busy === `install-${runtime.id}` ? "Download..." : runtime.installed ? "Pronto" : "Installa"}</button></div>)}
        <div className="ca-trust"><div><strong>Certificato HTTPS locale</strong><p>Per aprire i siti senza avvisi del browser, aggiungi la CA generata da Werd alle radici attendibili del tuo utente Windows.</p></div><button className="secondary-button" onClick={() => void trustCa().then(setNotice).catch((cause) => setError(String(cause)))}>Rendi attendibile</button></div>
      </div>}

      {page === "settings" && <><PageHeader eyebrow="PREFERENZE" title="Impostazioni" description="Configurazione locale dell’applicazione." /><div className="settings-card"><div className="settings-icon"><Settings2 size={23} /></div><div><h3>Gestione indipendente</h3><p>Werd conserva i propri runtime e dati separatamente da Herd, DBngin e Docker. Le configurazioni condivisibili sono salvate nei progetti come <code>werd.yml</code>.</p></div></div><div className="settings-card"><div className="settings-icon"><Activity size={23} /></div><div><h3>Gestore locale</h3><p>Versione: {daemonVersion}. La dashboard può essere chiusa senza interrompere gli ambienti attivi.</p></div></div></>}
    </main>

    {showAdd && <div className="modal-backdrop" onMouseDown={(event) => { if (event.target === event.currentTarget) setShowAdd(false); }}><form className="modal" onSubmit={(event) => void submitProject(event)}><div className="modal-icon"><FolderOpen size={24} /></div><h2>Collega un progetto Laravel</h2><p>Scegli la cartella del progetto. Werd leggerà o creerà la configurazione <code>werd.yml</code>.</p><label>PERCORSO DELLA CARTELLA<input autoFocus value={newPath} onChange={(event) => setNewPath(event.target.value)} placeholder="C:\\Users\\nome\\Developer\\progetto" required /></label><div className="modal-actions"><button type="button" className="secondary-button" onClick={() => setShowAdd(false)}>Annulla</button><button type="submit" className="primary-button" disabled={busy === "add" || !newPath.trim()}>{busy === "add" ? "Collegamento..." : "Collega progetto"}</button></div></form></div>}
  </div>;
}

function ProjectCard({ project, busy, onSelect, onToggle }: { project: Project; busy: boolean; onSelect: () => void; onToggle: () => void }) {
  return <div className="project-card"><div className="project-card-top"><span className="project-avatar">{project.name.slice(0, 1).toUpperCase()}</span><button className="small-icon-button" title="Dettagli" onClick={onSelect}><ChevronDown size={17} /></button></div><h3>{project.name}</h3><p>{project.path}</p><div className="project-card-services">{project.services.slice(0, 3).map((service) => <span key={service}>{service === "postgres" ? "Postgres" : serviceNames[service]}</span>)}</div><div className="project-card-footer"><Badge status={project.status} /><button disabled={busy} onClick={onToggle}>{project.status === "running" ? "Ferma" : "Avvia"} <ArrowUpRight size={14} /></button></div></div>;
}
