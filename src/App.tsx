import { Globe, LayoutDashboard, Loader2, Settings as SettingsIcon, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  certificateStatus,
  type DomainsStatus,
  disablePath,
  domainsStatus,
  enablePath,
  getSettings,
  launchAtLogin,
  listProjects,
  listServices,
  onRuntimesChanged,
  openSite,
  openUrl,
  type Project,
  parkFolder,
  refreshCatalog,
  resetPorts,
  type ServiceInstance,
  type Settings,
  type SystemInfo,
  setLaunchAtLogin,
  setTrayLabels,
  startProject,
  stopProject,
  syncHosts,
  systemInfo,
  trustCa,
  unparkFolder,
  updateSettings,
} from "./api";
import { AddSiteWizard } from "./components/AddSiteWizard";
import { UpdateButton } from "./components/UpdateButton";
import { WhatsNew } from "./components/WhatsNew";
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
  { id: "general", icon: SettingsIcon },
];

const ADVANCED: Page[] = ["php", "node", "services", "logs", "about"];

type Toast = { tone: "error" | "info"; text: string };

const message = (cause: unknown) => (cause instanceof Error ? cause.message : String(cause));

export default function App() {
  const t = useT();
  const [page, setPage] = useState<Page>("dashboard");
  const [creating, setCreating] = useState(false);
  const [projects, setProjects] = useState<Project[]>([]);
  const [instances, setInstances] = useState<ServiceInstance[]>([]);
  const [daemonVersion, setDaemonVersion] = useState("—");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [offline, setOffline] = useState<string | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [system, setSystem] = useState<SystemInfo | null>(null);
  const [domains, setDomains] = useState<DomainsStatus | null>(null);
  /** False until the daemon answered once, so pages never flash empty at start. */
  const [loaded, setLoaded] = useState(false);
  const [atLogin, setAtLogin] = useState<boolean | null>(null);
  const [certificate, setCertificate] = useState<{ exists: boolean; trusted: boolean } | null>(null);

  const fail = useCallback((cause: unknown) => setToast({ tone: "error", text: message(cause) }), []);
  const runtimes = useRuntimes(fail);

  const refresh = useCallback(async () => {
    try {
      const [snapshot, services, hosts] = await Promise.all([listProjects(), listServices(), domainsStatus()]);
      setProjects(snapshot.projects);
      setInstances(services);
      setDomains(hosts);
      setDaemonVersion(snapshot.daemon_version);
      setOffline(null);
    } catch (cause) {
      setOffline(message(cause));
    } finally {
      setLoaded(true);
    }
  }, []);

  // The tray lists installed PHP versions with the global one checked.
  const phpKey = runtimes.rows
    .filter((row) => row.product === "php" && row.installed)
    .map((row) => `${row.line}${row.is_default ? "*" : ""}`)
    .join(",");
  // biome-ignore lint/correctness/useExhaustiveDependencies: phpKey only triggers the rebuild; the tray reads the versions from the daemon.
  useEffect(() => {
    void setTrayLabels({
      open: t.tray.open,
      stop_all: t.tray.stopAll,
      quit: t.tray.quit,
      use_php: t.tray.usePhp,
      check_updates: t.tray.checkUpdates,
    }).catch(() => {});
  }, [t, phpKey]);

  const refreshRuntimes = runtimes.refresh;
  useEffect(() => onRuntimesChanged(() => void refreshRuntimes()), [refreshRuntimes]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2500);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (page === "php") void getSettings().then(setSettings).catch(fail);
    if (page === "general") {
      void systemInfo().then(setSystem).catch(fail);
      void getSettings().then(setSettings).catch(fail);
      void launchAtLogin().then(setAtLogin).catch(fail);
      void certificateStatus().then(setCertificate).catch(fail);
    }
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

  /** Asks for the hosts update when new sites brought domains the file lacks. */
  async function syncNewDomains() {
    const hosts = await domainsStatus();
    if (hosts.missing.length === 0) return;
    await syncHosts()
      .then(() => setToast({ tone: "info", text: t.shell.hostsUpdated }))
      .catch(fail);
    await refresh();
  }

  /** Adds missing site domains to the hosts file; the system asks for administrator approval. */
  const updateHosts = () =>
    run("hosts", async () => {
      await syncHosts();
      setToast({ tone: "info", text: t.shell.hostsUpdated });
    });

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
              className={`nav-item ${page === id || (id === "general" && ADVANCED.includes(page)) ? "active" : ""}`}
              aria-current={page === id || (id === "general" && ADVANCED.includes(page)) ? "page" : undefined}
              // Narrow windows hide the label and keep only the icon.
              aria-label={t.nav[id]}
              title={t.nav[id]}
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
          <span className="sidebar-version">{offline ? t.shell.daemonOffline : `v${__APP_VERSION__}`}</span>
          <UpdateButton onError={fail} onUpToDate={() => setToast({ tone: "info", text: t.update.upToDate })} />
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

        {domains && domains.missing.length > 0 && (
          <div className="banner banner-warn" role="status">
            <span>{t.workspace.domainsMissing(domains.missing)}</span>
            <button
              type="button"
              className="button button-small"
              disabled={busy === "hosts"}
              onClick={() => void updateHosts()}
            >
              {t.shell.updateHosts}
            </button>
          </div>
        )}

        {!loaded ? (
          <div className="app-loading" role="status">
            <Loader2 size={22} className="spin" />
            <span>{t.shell.starting}</span>
          </div>
        ) : (
          <>
            {ADVANCED.includes(page) && (
              <div className="advanced-back">
                <button type="button" className="link" onClick={() => setPage("general")}>
                  {t.workspace.backToSettings}
                </button>
              </div>
            )}
            {page === "dashboard" && (
              <Dashboard
                projects={projects}
                runtimes={runtimes}
                onSelect={(project) => {
                  setSelectedId(project.id);
                  setPage("sites");
                }}
                onAdd={() => setCreating(true)}
                onError={fail}
              />
            )}
            {page === "sites" && (
              <Sites
                projects={projects}
                instances={instances}
                runtimes={runtimes}
                onChanged={refresh}
                onError={fail}
                busy={busy}
                selectedId={selectedId}
                onSelect={setSelectedId}
                onAdded={async (id) => {
                  setSelectedId(id);
                  await refresh();
                }}
                onToggle={toggle}
                onOpenSite={(project) => void openSite(project.id).catch(fail)}
                onShowLogs={(project) => {
                  setSelectedId(project.id);
                  setPage("logs");
                }}
                onResetPorts={(project) =>
                  void run(project.id, async () => {
                    await resetPorts(project.id);
                    setToast({ tone: "info", text: t.shell.portsReset });
                  })
                }
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
            {page === "logs" && (
              <Logs projects={projects} instances={instances} projectId={selectedId} onProjectChange={setSelectedId} />
            )}
            {page === "general" && (
              <>
                <General
                  system={system}
                  settings={settings}
                  domains={domains}
                  busy={busy}
                  onSaveDomains={(changes) =>
                    void run("domains", async () => {
                      setSettings(await updateSettings(changes));
                      if (changes.domains) {
                        const hosts = await domainsStatus();
                        if (hosts.missing.length > 0) await syncHosts();
                      }
                    })
                  }
                  onUpdateHosts={() => void updateHosts()}
                  onPark={(path) =>
                    void run("parks", async () => {
                      const parked = await parkFolder(path);
                      setSettings((current) => (current ? { ...current, parked } : current));
                      await refresh();
                      await syncNewDomains();
                    })
                  }
                  onUnpark={(path) =>
                    void run("parks", async () => {
                      const parked = await unparkFolder(path);
                      setSettings((current) => (current ? { ...current, parked } : current));
                    })
                  }
                  launchAtLogin={atLogin}
                  onToggleLaunchAtLogin={(enabled) =>
                    void run("login", async () => setAtLogin(await setLaunchAtLogin(enabled)))
                  }
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
                  certificate={certificate}
                  onTrustCa={() =>
                    void trustCa()
                      .then(async (text) => {
                        setToast({ tone: "info", text });
                        setCertificate(await certificateStatus());
                      })
                      .catch(fail)
                  }
                />
                <div className="page-body">
                  <details className="advanced-panel">
                    <summary>{t.workspace.advanced}</summary>
                    <p className="muted">{t.workspace.advancedHint}</p>
                    <div className="button-row">
                      {ADVANCED.map((id) => (
                        <button type="button" className="button" key={id} onClick={() => setPage(id)}>
                          {id === "services" ? t.workspace.infrastructure : t.nav[id]}
                        </button>
                      ))}
                    </div>
                  </details>
                </div>
              </>
            )}
            {page === "about" && <About daemonVersion={daemonVersion} onOpenUrl={open} onError={fail} />}
          </>
        )}
      </main>

      <WhatsNew />
      {creating && (
        <AddSiteWizard
          runtimes={runtimes}
          onClose={() => setCreating(false)}
          onDone={async (id) => {
            setSelectedId(id);
            await refresh();
            setCreating(false);
            setPage("sites");
          }}
        />
      )}

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
