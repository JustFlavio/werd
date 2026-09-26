import {
  Boxes,
  Cpu,
  Globe,
  Hexagon,
  Info,
  LayoutDashboard,
  ScrollText,
  Settings as SettingsIcon,
  X,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  addProject,
  disablePath,
  enablePath,
  getSettings,
  listProjects,
  listServices,
  openSite,
  openUrl,
  type Project,
  refreshCatalog,
  resetPorts,
  type ServiceInstance,
  type Settings,
  type SystemInfo,
  startProject,
  stopProject,
  stopService,
  systemInfo,
  trustCa,
  updateSettings,
} from "./api";
import { useT } from "./i18n";
import { Logo } from "./Logo";
import { About } from "./pages/About";
import { Dashboard } from "./pages/Dashboard";
import { General } from "./pages/General";
import { Logs } from "./pages/Logs";
import { Node } from "./pages/Node";
import { Php } from "./pages/Php";
import { Services } from "./pages/Services";
import { Sites } from "./pages/Sites";
import { useRuntimes } from "./runtimes";

export type Page = "dashboard" | "sites" | "php" | "node" | "services" | "logs" | "general" | "about";

const NAV: { id: Page; icon: typeof Globe }[] = [
  { id: "dashboard", icon: LayoutDashboard },
  { id: "sites", icon: Globe },
  { id: "php", icon: Cpu },
  { id: "node", icon: Hexagon },
  { id: "services", icon: Boxes },
  { id: "logs", icon: ScrollText },
  { id: "general", icon: SettingsIcon },
  { id: "about", icon: Info },
];

type Toast = { tone: "error" | "info"; text: string };

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

export default function App() {
  const t = useT();
  const [page, setPage] = useState<Page>("dashboard");
  const [projects, setProjects] = useState<Project[]>([]);
  const [instances, setInstances] = useState<ServiceInstance[]>([]);
  const [daemonVersion, setDaemonVersion] = useState("—");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [offline, setOffline] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [system, setSystem] = useState<SystemInfo | null>(null);

  const fail = useCallback((cause: unknown) => setToast({ tone: "error", text: message(cause) }), []);
  const runtimes = useRuntimes(fail);

  const refresh = useCallback(async () => {
    try {
      const [snapshot, services] = await Promise.all([listProjects(), listServices()]);
      setProjects(snapshot.projects);
      setInstances(services);
      setDaemonVersion(snapshot.daemon_version);
      setOffline(null);
    } catch (cause) {
      setOffline(message(cause));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (page === "php") void getSettings().then(setSettings).catch(fail);
    if (page === "general") void systemInfo().then(setSystem).catch(fail);
  }, [page, fail]);

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
      for (const instance of instances.filter((item) => item.status === "running")) await stopService(instance.id);
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
        <nav aria-label={t.nav.label}>
          {NAV.map(({ id, icon: Icon }) => (
            <button
              type="button"
              key={id}
              className={`nav-item ${page === id ? "active" : ""}`}
              aria-current={page === id ? "page" : undefined}
              onClick={() => setPage(id)}
            >
              <Icon size={17} strokeWidth={1.75} />
              <span>{t.nav[id]}</span>
              {id === "sites" && projects.length > 0 && <span className="nav-count">{projects.length}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <span className={`status status-${offline ? "fail" : "ok"}`}>
            <i aria-hidden />
          </span>
          {offline ? t.shell.daemonOffline : `v${__APP_VERSION__}`}
        </div>
      </aside>

      <main className="main">
        {offline && (
          <div className="banner banner-error" role="alert">
            <span>{t.shell.daemonOfflineBanner(offline)}</span>
            <button type="button" className="button button-small" onClick={() => void refresh()}>
              {t.common.retry}
            </button>
          </div>
        )}

        {page === "dashboard" && (
          <Dashboard
            projects={projects}
            instances={instances}
            runtimes={runtimes}
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
                setToast({ tone: "info", text: t.shell.portsReset });
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
            runtimes={runtimes}
            settings={settings}
            onSaveLimits={async (limits) => {
              try {
                setSettings(await updateSettings(limits));
                setToast({ tone: "info", text: t.php.limitsSaved });
              } catch (cause) {
                fail(cause);
              }
            }}
          />
        )}
        {page === "node" && <Node runtimes={runtimes} />}
        {page === "services" && (
          <Services instances={instances} runtimes={runtimes} onChanged={refresh} onOpenUrl={open} onError={fail} />
        )}
        {page === "logs" && <Logs projects={projects} projectId={selectedId} onProjectChange={setSelectedId} />}
        {page === "general" && (
          <General
            system={system}
            busy={busy}
            onTogglePath={(enable) =>
              void run("path", async () => {
                await (enable ? enablePath() : disablePath());
                setSystem(await systemInfo());
                setToast({ tone: "info", text: enable ? t.general.cliEnabledToast : t.general.cliDisabledToast });
              })
            }
            onRefreshCatalog={() =>
              void run("catalog", async () => {
                await refreshCatalog();
                setSystem(await systemInfo());
                await runtimes.refresh();
              })
            }
            onTrustCa={() =>
              void trustCa()
                .then((text) => setToast({ tone: "info", text }))
                .catch(fail)
            }
          />
        )}
        {page === "about" && <About daemonVersion={daemonVersion} onOpenUrl={open} onError={fail} />}
      </main>

      {toast && (
        <div className={`toast toast-${toast.tone}`} role={toast.tone === "error" ? "alert" : "status"}>
          <span>{toast.text}</span>
          <button type="button" className="icon-button" aria-label={t.common.close} onClick={() => setToast(null)}>
            <X size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
