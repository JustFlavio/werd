import { Boxes, Cpu, Globe, Info, LayoutDashboard, ScrollText, Settings, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  addProject,
  type DoctorResult,
  doctor,
  installRuntime,
  listProjects,
  openSite,
  openUrl,
  type Project,
  type RuntimeInfo,
  resetPorts,
  runtimes,
  startProject,
  stopProject,
  trustCa,
} from "./api";
import { Logo } from "./Logo";
import { About } from "./pages/About";
import { Dashboard } from "./pages/Dashboard";
import { General } from "./pages/General";
import { Logs } from "./pages/Logs";
import { Php } from "./pages/Php";
import { Services } from "./pages/Services";
import { Sites } from "./pages/Sites";

export type Page = "dashboard" | "sites" | "php" | "services" | "logs" | "general" | "about";

const NAV: { id: Page; label: string; icon: typeof Globe }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "sites", label: "Siti", icon: Globe },
  { id: "php", label: "PHP", icon: Cpu },
  { id: "services", label: "Servizi", icon: Boxes },
  { id: "logs", label: "Log", icon: ScrollText },
  { id: "general", label: "Generale", icon: Settings },
  { id: "about", label: "Informazioni", icon: Info },
];

type Toast = { tone: "error" | "info"; text: string };

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [projects, setProjects] = useState<Project[]>([]);
  const [daemonVersion, setDaemonVersion] = useState("—");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [offline, setOffline] = useState<string | null>(null);
  const [checks, setChecks] = useState<DoctorResult[]>([]);
  const [runtimeList, setRuntimeList] = useState<RuntimeInfo[]>([]);

  const fail = (cause: unknown) => setToast({ tone: "error", text: String(cause) });

  const refresh = useCallback(async () => {
    try {
      const snapshot = await listProjects();
      setProjects(snapshot.projects);
      setDaemonVersion(snapshot.daemon_version);
      setOffline(null);
    } catch (cause) {
      setOffline(String(cause));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (page === "general") void doctor().then(setChecks).catch(fail);
    if (page === "php") void runtimes().then(setRuntimeList).catch(fail);
  }, [page]);

  async function run(key: string, action: () => Promise<unknown>) {
    setBusy(key);
    try {
      await action();
      await refresh();
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  }

  const toggle = (project: Project) =>
    void run(project.id, () => (project.status === "running" ? stopProject(project.id) : startProject(project.id)));

  const stopAll = () =>
    void run("stop-all", async () => {
      for (const project of projects.filter((item) => item.status === "running")) await stopProject(project.id);
    });

  async function add(path: string): Promise<boolean> {
    setBusy("add");
    try {
      const project = await addProject(path);
      setSelectedId(project.id);
      await refresh();
      return true;
    } catch (cause) {
      fail(cause);
      return false;
    } finally {
      setBusy(null);
    }
  }

  const open = (url: string) => void openUrl(url).catch(fail);

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <Logo />
          <span>Werd</span>
        </div>
        <nav aria-label="Navigazione principale">
          {NAV.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              className={`nav-item ${page === id ? "active" : ""}`}
              aria-current={page === id ? "page" : undefined}
              onClick={() => setPage(id)}
            >
              <Icon size={17} strokeWidth={1.75} />
              <span>{label}</span>
              {id === "sites" && projects.length > 0 && <span className="nav-count">{projects.length}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className={`status status-${offline ? "fail" : "ok"}`}>
            <i aria-hidden />
          </span>
          {offline ? "Gestore non raggiungibile" : `v${__APP_VERSION__}`}
        </div>
      </aside>

      <main className="main">
        {offline && (
          <div className="banner banner-error" role="alert">
            <span>Impossibile contattare il gestore Werd: {offline}</span>
            <button className="button button-small" onClick={() => void refresh()}>
              Riprova
            </button>
          </div>
        )}

        {page === "dashboard" && (
          <Dashboard
            projects={projects}
            busy={busy === "stop-all"}
            onStopAll={stopAll}
            onNavigate={setPage}
            onOpenUrl={open}
          />
        )}
        {page === "sites" && (
          <Sites
            projects={projects}
            busy={busy}
            selectedId={selectedId}
            onSelect={setSelectedId}
            onAdd={add}
            onToggle={toggle}
            onOpenSite={(project) => void openSite(project.id).catch(fail)}
            onOpenUrl={open}
            onResetPorts={(project) =>
              void run(project.id, async () => {
                await resetPorts(project.id);
                setToast({
                  tone: "info",
                  text: "Le porte verranno riassegnate al prossimo avvio. Ricorda di aggiornare il .env.",
                });
              })
            }
            onShowLogs={(project) => {
              setSelectedId(project.id);
              setPage("logs");
            }}
          />
        )}
        {page === "php" && (
          <Php
            runtimes={runtimeList}
            busy={busy?.startsWith("install:") ? busy.slice(8) : null}
            onInstall={(id) =>
              void run(`install:${id}`, async () => {
                await installRuntime(id);
                setRuntimeList(await runtimes());
              })
            }
          />
        )}
        {page === "services" && <Services projects={projects} onOpenUrl={open} />}
        {page === "logs" && <Logs projects={projects} projectId={selectedId} onProjectChange={setSelectedId} />}
        {page === "general" && (
          <General
            checks={checks}
            daemonVersion={daemonVersion}
            onRecheck={() => void doctor().then(setChecks).catch(fail)}
            onTrustCa={() =>
              void trustCa()
                .then((text) => setToast({ tone: "info", text }))
                .catch(fail)
            }
          />
        )}
        {page === "about" && <About daemonVersion={daemonVersion} onOpenUrl={open} />}
      </main>

      {toast && (
        <div className={`toast toast-${toast.tone}`} role={toast.tone === "error" ? "alert" : "status"}>
          <span>{toast.text}</span>
          <button className="icon-button" aria-label="Chiudi" onClick={() => setToast(null)}>
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
