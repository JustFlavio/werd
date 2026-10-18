import { ArrowRight, CircleCheck, FileCode2, FolderOpen, Loader2, Puzzle, Sparkles, TriangleAlert } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  addProject,
  boostUpdate,
  createProject,
  domainsStatus,
  getSettings,
  inspectFolder,
  installRuntime,
  type Job,
  listJobs,
  listRuntimes,
  type NewProject,
  type ProjectInfo,
  pickFolder,
  type StarterKit,
  syncHosts,
} from "../api";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { Modal } from "../ui";
import { LivewireMark, ReactMark, SvelteMark, VueMark } from "./KitIcons";

type Step = "choose" | "link" | "kit" | "options" | "progress";
type Outcome = "running" | "done" | "failed";

const LOCATION_KEY = "werd.newProjectDirectory";
const NAME_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

const delay = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

function savedLocation(): string {
  try {
    return localStorage.getItem(LOCATION_KEY) ?? "";
  } catch {
    return "";
  }
}

function rememberLocation(directory: string) {
  try {
    localStorage.setItem(LOCATION_KEY, directory);
  } catch {
    // Only a convenience.
  }
}

/** Polls a job until it ends, passing new output lines and step changes along. */
async function follow(job: Job, onLine: (line: string) => void): Promise<Job> {
  let printed = 0;
  let step = "";
  for (;;) {
    const current = (await listJobs()).find((candidate) => candidate.id === job.id);
    if (!current) throw new Error("The job disappeared");
    if (current.step !== step && current.step !== "Done") {
      step = current.step;
      onLine(`==> ${step}`);
    }
    const dropped = current.log_dropped ?? 0;
    const log = current.log ?? [];
    for (const line of log.slice(Math.max(0, printed - dropped))) onLine(line);
    printed = dropped + log.length;
    if (current.state === "done") return current;
    if (current.state === "failed") throw new Error(current.error ?? "Failed");
    await delay(600);
  }
}

export function AddSiteWizard({
  runtimes,
  onClose,
  onDone,
}: {
  runtimes: Runtimes;
  onClose: () => void;
  onDone: (projectId: string) => Promise<void>;
}) {
  const t = useT();
  const w = t.wizard;
  const [step, setStep] = useState<Step>("choose");
  const [history, setHistory] = useState<Step[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Link an existing project.
  const [path, setPath] = useState("");
  const [info, setInfo] = useState<ProjectInfo | null>(null);
  const [name, setName] = useState("");
  const [php, setPhp] = useState("");
  const [updateEnv, setUpdateEnv] = useState(false);
  const [updateBoost, setUpdateBoost] = useState(false);
  const usesBoost = info?.php_packages.some((item) => item.name === "laravel/boost") ?? false;

  // New project.
  const [kit, setKit] = useState<StarterKit | null>(null);
  const [using, setUsing] = useState("");
  const [auth, setAuth] = useState<NewProject["auth"]>("laravel");
  const [teams, setTeams] = useState(false);
  const [testing, setTesting] = useState<NewProject["testing"]>("pest");
  const [boost, setBoost] = useState(true);
  const [git, setGit] = useState(false);
  const [npm, setNpm] = useState(true);
  const [directory, setDirectory] = useState(savedLocation);

  // Progress.
  const [lines, setLines] = useState<string[]>([]);
  const [outcome, setOutcome] = useState<Outcome>("running");
  const [created, setCreated] = useState<string | null>(null);
  const [showOutput, setShowOutput] = useState(false);
  const [opening, setOpening] = useState(false);
  const [phase, setPhase] = useState("");
  const [openError, setOpenError] = useState<string | null>(null);
  const [hostsSkipped, setHostsSkipped] = useState(false);
  const logBox = useRef<HTMLDivElement>(null);

  const phpRows = runtimes.rows.filter((row) => row.product === "php");
  const installedPhp = phpRows.filter((row) => row.installed);
  const defaultPhp = phpRows.find((row) => row.is_default)?.line ?? installedPhp[0]?.line ?? phpRows[0]?.line ?? "";

  useEffect(() => {
    if (!php && defaultPhp && step === "options") setPhp(defaultPhp);
  }, [php, defaultPhp, step]);

  useEffect(() => {
    const box = logBox.current;
    if (box && lines.length) box.scrollTop = box.scrollHeight;
  }, [lines]);

  useEffect(() => {
    if (directory) return;
    void getSettings()
      .then((settings) => {
        if (settings.parked[0]) setDirectory(settings.parked[0]);
      })
      .catch(() => {});
  }, [directory]);

  const go = (next: Step) => {
    setHistory((previous) => [...previous, step]);
    setError(null);
    setStep(next);
  };
  const back = () => {
    setError(null);
    setHistory((previous) => {
      const copy = [...previous];
      setStep(copy.pop() ?? "choose");
      return copy;
    });
  };
  const log = (line: string) => setLines((previous) => [...previous, line]);

  async function chooseFolder() {
    const picked = await pickFolder(w.pickProject);
    if (!picked) return false;
    await readFolder(picked);
    return true;
  }

  async function readFolder(folder: string) {
    setPath(folder);
    setError(null);
    try {
      const described = await inspectFolder(folder);
      setInfo(described);
      setName(described.name);
      setPhp(described.suggested_php ?? defaultPhp);
    } catch (cause) {
      setInfo(null);
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function ensureRuntime(product: string, line: string) {
    const message = w.installingRuntime(product === "php" ? "PHP" : "Node.js", line);
    setPhase(message);
    const setup = (await listJobs()).find((job) => job.product === "setup" && job.state === "running");
    if (setup) await follow(setup, log);
    if ((await listRuntimes()).some((row) => row.product === product && row.line === line && row.installed)) return;
    log(message);
    const active = (await listJobs()).find(
      (job) => job.product === product && job.line === line && job.state === "running",
    );
    const job = active ?? (await installRuntime(product, line));
    runtimes.track(job);
    await follow(job, log);
    await runtimes.refresh();
  }

  async function ensureNode() {
    const rows = await listRuntimes();
    if (rows.some((row) => row.product === "node" && row.installed)) return;
    const recommended = rows
      .filter(
        (row) => row.product === "node" && row.lts && (!row.eol || row.eol >= new Date().toISOString().slice(0, 10)),
      )
      .sort((a, b) => b.line.localeCompare(a.line, undefined, { numeric: true }))[0];
    if (!recommended) throw new Error(w.noNodeAvailable);
    await ensureRuntime("node", recommended.line);
  }

  async function updateHosts() {
    const hosts = await domainsStatus();
    if (hosts.missing.length === 0) return;
    log(w.updatingHosts(hosts.missing.join(", ")));
    try {
      await syncHosts();
    } catch (cause) {
      setHostsSkipped(true);
      log(`${w.hostsSkipped}: ${cause instanceof Error ? cause.message : String(cause)}`);
    }
  }

  async function run(work: () => Promise<string>) {
    go("progress");
    setLines([]);
    setShowOutput(false);
    setOutcome("running");
    try {
      const id = await work();
      await updateHosts();
      setCreated(id);
      setOutcome("done");
      await onDone(id);
    } catch (cause) {
      log(cause instanceof Error ? cause.message : String(cause));
      setOutcome("failed");
    }
  }

  const link = () =>
    run(async () => {
      await ensureRuntime("php", php);
      setPhase(w.linking(path));
      log(w.linking(path));
      const project = await addProject(path, name.trim(), php, updateEnv);
      if (updateEnv) log(w.envUpdated(project.domain ?? ""));
      if (updateBoost && usesBoost) {
        log("==> php artisan boost:update");
        try {
          for (const line of (await boostUpdate(project.id)).split(/\r?\n/)) log(line);
        } catch (cause) {
          log(cause instanceof Error ? cause.message : String(cause));
        }
      }
      log(w.linked(project.name, project.domain ?? ""));
      return project.id;
    });

  const create = () =>
    run(async () => {
      rememberLocation(directory);
      await ensureRuntime("php", php);
      if (npm) await ensureNode();
      setPhase(w.creating);
      const job = await createProject({
        name: name.trim(),
        directory,
        kit,
        using: kit === "custom" ? using.trim() : undefined,
        auth,
        teams,
        testing,
        boost,
        git,
        npm,
        php,
      });
      const finished = await follow(job, log);
      if (!finished.result) throw new Error(w.noSite);
      return finished.result;
    });

  const kits: { id: StarterKit | null; label: string; icon: ReactNode }[] = [
    { id: null, label: w.noKit, icon: <FileCode2 size={30} strokeWidth={1.4} /> },
    { id: "react", label: "React", icon: <ReactMark /> },
    { id: "vue", label: "Vue", icon: <VueMark /> },
    { id: "svelte", label: "Svelte", icon: <SvelteMark /> },
    { id: "livewire", label: "Livewire", icon: <LivewireMark /> },
    { id: "custom", label: w.customKit, icon: <Puzzle size={30} strokeWidth={1.4} /> },
  ];
  const starterKit = kit !== null && kit !== "custom";
  const nameValid = NAME_PATTERN.test(name.trim());
  const canCreate =
    nameValid && Boolean(directory) && Boolean(php) && (kit !== "custom" || /^[\w.-]+\/[\w.-]+/.test(using.trim()));

  const busy = step === "progress" && outcome === "running";
  const title =
    step === "choose"
      ? w.title
      : step === "link" || (step === "progress" && !history.includes("kit"))
        ? w.linkTitle
        : w.newTitle;

  return (
    <Modal title={title} onClose={busy ? () => {} : onClose} wide>
      <div className="wizard-body">
        {step === "choose" && (
          <div className="choice-grid choice-grid-2">
            <button type="button" className="choice-card" onClick={() => go("kit")}>
              <Sparkles size={30} strokeWidth={1.4} />
              <strong>{w.newProject}</strong>
              <small>{w.newProjectHint}</small>
            </button>
            <button
              type="button"
              className="choice-card"
              onClick={() => {
                go("link");
                void chooseFolder();
              }}
            >
              <FolderOpen size={30} strokeWidth={1.4} />
              <strong>{w.linkProject}</strong>
              <small>{w.linkProjectHint}</small>
            </button>
          </div>
        )}

        {step === "link" && (
          <div className="form-grid">
            <p className="form-span muted">{w.linkGuide}</p>
            <label htmlFor="wizard-path">{w.path}</label>
            <div className="inline-form">
              <input
                id="wizard-path"
                className="input mono"
                value={path}
                placeholder={t.sites.folderPlaceholder}
                onChange={(event) => setPath(event.target.value)}
                onBlur={() => path.trim() && void readFolder(path.trim())}
              />
              <button type="button" className="button" onClick={() => void chooseFolder()}>
                {t.sites.browse}
              </button>
            </div>

            {info && !info.laravel && (
              <div className="form-span callout callout-warn">
                <TriangleAlert size={14} /> {w.notLaravel}
              </div>
            )}

            <label htmlFor="wizard-name">{w.name}</label>
            <div>
              <input
                id="wizard-name"
                className="input"
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
              {name.trim() && <p className="field-hint">{w.domainPreview(name)}</p>}
            </div>

            {php && <p className="form-span muted">{w.linkPhp(php)}</p>}
            <details className="advanced-panel form-span">
              <summary>{w.customize}</summary>
              <div className="form-grid">
                <span>{w.extras}</span>
                <div className="checkbox-stack">
                  <label className="checkbox">
                    <input
                      type="checkbox"
                      checked={updateEnv}
                      onChange={(event) => setUpdateEnv(event.target.checked)}
                    />
                    {w.updateEnv}
                  </label>
                  {usesBoost && (
                    <label className="checkbox">
                      <input
                        type="checkbox"
                        checked={updateBoost}
                        onChange={(event) => setUpdateBoost(event.target.checked)}
                      />
                      {w.updateBoost}
                    </label>
                  )}
                  {usesBoost && <p className="field-hint">{w.updateBoostHint}</p>}
                </div>

                <label htmlFor="wizard-php">PHP</label>
                <div>
                  <select
                    id="wizard-php"
                    className="select"
                    value={php}
                    onChange={(event) => setPhp(event.target.value)}
                  >
                    {phpRows.map((row) => (
                      <option key={row.line} value={row.line}>
                        PHP {row.line}
                        {row.installed ? "" : ` — ${w.willInstall}`}
                      </option>
                    ))}
                  </select>
                  {info?.php_constraint && <p className="field-hint">{w.requires(info.php_constraint)}</p>}
                </div>
              </div>
            </details>
          </div>
        )}

        {step === "kit" && (
          <>
            <p className="muted">{w.kitGuide}</p>
            <div className="choice-grid choice-grid-3">
              {kits.map((option) => (
                <button
                  type="button"
                  key={option.id ?? "none"}
                  className={`choice-card ${kit === option.id ? "selected" : ""}`}
                  aria-pressed={kit === option.id}
                  aria-label={option.label}
                  aria-describedby={`kit-${option.id ?? "none"}-hint`}
                  onClick={() => setKit(option.id)}
                  onDoubleClick={() => go("options")}
                >
                  {option.icon}
                  <strong>{option.label}</strong>
                  <small id={`kit-${option.id ?? "none"}-hint`}>{w.kitHints[option.id ?? "none"]}</small>
                </button>
              ))}
            </div>
          </>
        )}

        {step === "options" && (
          <div className="form-grid">
            <label htmlFor="wizard-new-name">{w.projectName}</label>
            <div>
              <input
                id="wizard-new-name"
                className="input"
                value={name}
                placeholder="my-app"
                aria-invalid={Boolean(name) && !nameValid}
                onChange={(event) => setName(event.target.value)}
              />
              <p className="field-hint">{name && !nameValid ? w.nameRule : w.domainPreview(name || "my-app")}</p>
            </div>

            {kit === "custom" && (
              <>
                <label htmlFor="wizard-using">{w.package}</label>
                <input
                  id="wizard-using"
                  className="input mono"
                  value={using}
                  placeholder="vendor/starter-kit"
                  onChange={(event) => setUsing(event.target.value)}
                />
              </>
            )}

            <label htmlFor="wizard-location">{w.location}</label>
            <div>
              <div className="inline-form">
                <input
                  id="wizard-location"
                  className="input mono"
                  value={directory}
                  placeholder={t.sites.folderPlaceholder}
                  onChange={(event) => setDirectory(event.target.value)}
                />
                <button
                  type="button"
                  className="button"
                  onClick={() =>
                    void pickFolder(w.pickLocation).then((picked) => {
                      if (picked) setDirectory(picked);
                    })
                  }
                >
                  {t.sites.browse}
                </button>
              </div>
              <p className="field-hint">{w.locationHint}</p>
            </div>
            <p className="form-span muted">{w.recommended(php)}</p>
            <details className="form-span advanced-panel">
              <summary>{w.customize}</summary>
              <div className="form-grid">
                {starterKit && (
                  <>
                    <label htmlFor="wizard-auth">{w.auth}</label>
                    <select
                      id="wizard-auth"
                      className="select"
                      value={auth}
                      onChange={(event) => setAuth(event.target.value as NewProject["auth"])}
                    >
                      <option value="laravel">{w.authLaravel}</option>
                      <option value="workos">WorkOS</option>
                      <option value="none">{w.authNone}</option>
                    </select>

                    <span>{w.teams}</span>
                    <label className="checkbox">
                      <input type="checkbox" checked={teams} onChange={(event) => setTeams(event.target.checked)} />
                      {w.teamsHint}
                    </label>
                  </>
                )}

                <label htmlFor="wizard-testing">{w.testing}</label>
                <select
                  id="wizard-testing"
                  className="select"
                  value={testing}
                  onChange={(event) => setTesting(event.target.value as NewProject["testing"])}
                >
                  <option value="pest">Pest</option>
                  <option value="phpunit">PHPUnit</option>
                </select>

                <span>{w.extras}</span>
                <div className="checkbox-stack">
                  <label className="checkbox">
                    <input type="checkbox" checked={boost} onChange={(event) => setBoost(event.target.checked)} />
                    {w.boost}
                  </label>
                  <label className="checkbox">
                    <input type="checkbox" checked={git} onChange={(event) => setGit(event.target.checked)} />
                    {w.git}
                  </label>
                  <label className="checkbox">
                    <input type="checkbox" checked={npm} onChange={(event) => setNpm(event.target.checked)} />
                    {w.npm}
                  </label>
                </div>

                <label htmlFor="wizard-new-php">PHP</label>
                <select
                  id="wizard-new-php"
                  className="select"
                  value={php}
                  onChange={(event) => setPhp(event.target.value)}
                >
                  {phpRows.map((row) => (
                    <option key={row.line} value={row.line}>
                      PHP {row.line}
                      {row.installed ? "" : ` — ${w.willInstall}`}
                    </option>
                  ))}
                </select>
              </div>
            </details>
          </div>
        )}

        {step === "progress" && outcome === "done" && !showOutput && (
          <div className="wizard-success">
            <CircleCheck size={40} strokeWidth={1.4} />
            <p>{history.includes("kit") ? w.createdMessage : w.linkedMessage}</p>
            <button type="button" className="link-button" onClick={() => setShowOutput(true)}>
              {w.showOutput}
            </button>
          </div>
        )}

        {step === "progress" && outcome === "done" && hostsSkipped && (
          <div className="callout callout-warn">
            <p>{w.hostsSkippedHint}</p>
            <button
              type="button"
              className="button"
              disabled={opening}
              onClick={() => {
                setOpening(true);
                void syncHosts()
                  .then(() => setHostsSkipped(false))
                  .catch((cause) => setOpenError(String(cause)))
                  .finally(() => setOpening(false));
              }}
            >
              {t.preparation.configureDomains}
            </button>
          </div>
        )}
        {openError && (
          <p role="alert" className="callout callout-error">
            {openError}
          </p>
        )}

        {step === "progress" && (outcome !== "done" || showOutput) && (
          <div className="wizard-progress">
            <p className={`wizard-status wizard-status-${outcome}`}>
              {outcome === "running" && <Loader2 size={16} className="spin" />}
              {outcome === "done" && <CircleCheck size={16} />}
              {outcome === "failed" && <TriangleAlert size={16} />}
              {outcome === "running" ? phase || w.working : outcome === "done" ? w.done : w.failed}
            </p>
            <details className="advanced-panel" open={showOutput || undefined}>
              <summary>{w.showOutput}</summary>
              <div ref={logBox} className="log-box mono" role="log" aria-live="polite">
                {lines.map((line, index) => (
                  // Output lines can repeat; their position is their identity.
                  // biome-ignore lint/suspicious/noArrayIndexKey: append-only log
                  <div key={index}>{line || " "}</div>
                ))}
              </div>
            </details>
          </div>
        )}

        {error && step !== "progress" && <div className="callout callout-error">{error}</div>}
      </div>

      <div className="modal-actions wizard-actions">
        {step !== "choose" && step !== "progress" && (
          <button type="button" className="button" onClick={back}>
            {w.previous}
          </button>
        )}
        {step === "choose" && (
          <button type="button" className="button" onClick={onClose}>
            {t.common.cancel}
          </button>
        )}
        {step === "link" && (
          <button
            type="button"
            className="button button-primary"
            disabled={!info?.laravel || !name.trim() || !php}
            onClick={() => void link()}
          >
            {w.addSite}
          </button>
        )}
        {step === "kit" && (
          <button type="button" className="button button-primary" onClick={() => go("options")}>
            {w.next}
          </button>
        )}
        {step === "options" && (
          <button type="button" className="button button-primary" disabled={!canCreate} onClick={() => void create()}>
            {w.create}
          </button>
        )}
        {step === "progress" && outcome === "failed" && (
          <button type="button" className="button" onClick={back}>
            {w.previous}
          </button>
        )}
        {step === "progress" && outcome === "done" && (
          <button type="button" className="button button-primary" disabled={opening || !created} onClick={onClose}>
            <ArrowRight size={14} /> {w.goToProject}
          </button>
        )}
        {step === "progress" && outcome !== "done" && (
          <button type="button" className="button" disabled={outcome === "running"} onClick={onClose}>
            {t.common.close}
          </button>
        )}
      </div>
    </Modal>
  );
}
