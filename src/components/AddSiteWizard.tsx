import { Check, FileCode2, FolderOpen, Loader2, Puzzle, Sparkles, TriangleAlert } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  addProject,
  createProject,
  domainsStatus,
  getSettings,
  inspectFolder,
  installRuntime,
  type Job,
  listJobs,
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

  // New project.
  const [kit, setKit] = useState<StarterKit | null>(null);
  const [using, setUsing] = useState("");
  const [auth, setAuth] = useState<NewProject["auth"]>("laravel");
  const [teams, setTeams] = useState(false);
  const [testing, setTesting] = useState<NewProject["testing"]>("pest");
  const [boost, setBoost] = useState(true);
  const [git, setGit] = useState(false);
  const nodeInstalled = runtimes.rows.some((row) => row.product === "node" && row.installed);
  const [npm, setNpm] = useState(nodeInstalled);
  const [directory, setDirectory] = useState(savedLocation);

  // Progress.
  const [lines, setLines] = useState<string[]>([]);
  const [outcome, setOutcome] = useState<Outcome>("running");
  const [created, setCreated] = useState<string | null>(null);
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

  async function ensurePhp(line: string) {
    if (installedPhp.some((row) => row.line === line)) return;
    log(w.installingPhp(line));
    const job = await installRuntime("php", line);
    runtimes.track(job);
    await follow(job, () => {});
  }

  async function updateHosts() {
    const hosts = await domainsStatus();
    if (hosts.missing.length === 0) return;
    log(w.updatingHosts(hosts.missing.join(", ")));
    try {
      await syncHosts();
    } catch (cause) {
      log(`${w.hostsSkipped}: ${cause instanceof Error ? cause.message : String(cause)}`);
    }
  }

  async function run(work: () => Promise<string>) {
    go("progress");
    setLines([]);
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
      await ensurePhp(php);
      log(w.linking(path));
      const project = await addProject(path, name.trim(), php);
      log(w.linked(project.name, project.domain ?? ""));
      return project.id;
    });

  const create = () =>
    run(async () => {
      rememberLocation(directory);
      await ensurePhp(php);
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
        npm: npm && nodeInstalled,
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

            <label htmlFor="wizard-php">PHP</label>
            <div>
              <select id="wizard-php" className="select" value={php} onChange={(event) => setPhp(event.target.value)}>
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
        )}

        {step === "kit" && (
          <div className="choice-grid choice-grid-3">
            {kits.map((option) => (
              <button
                type="button"
                key={option.id ?? "none"}
                className={`choice-card ${kit === option.id ? "selected" : ""}`}
                aria-pressed={kit === option.id}
                onClick={() => setKit(option.id)}
                onDoubleClick={() => go("options")}
              >
                {option.icon}
                <strong>{option.label}</strong>
              </button>
            ))}
          </div>
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
              <label className="checkbox" title={nodeInstalled ? undefined : w.npmNeedsNode}>
                <input
                  type="checkbox"
                  checked={npm && nodeInstalled}
                  disabled={!nodeInstalled}
                  onChange={(event) => setNpm(event.target.checked)}
                />
                {nodeInstalled ? w.npm : w.npmNeedsNode}
              </label>
            </div>

            <label htmlFor="wizard-new-php">PHP</label>
            <select id="wizard-new-php" className="select" value={php} onChange={(event) => setPhp(event.target.value)}>
              {phpRows.map((row) => (
                <option key={row.line} value={row.line}>
                  PHP {row.line}
                  {row.installed ? "" : ` — ${w.willInstall}`}
                </option>
              ))}
            </select>

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
          </div>
        )}

        {step === "progress" && (
          <div className="wizard-progress">
            <p className={`wizard-status wizard-status-${outcome}`}>
              {outcome === "running" && <Loader2 size={16} className="spin" />}
              {outcome === "done" && <Check size={16} />}
              {outcome === "failed" && <TriangleAlert size={16} />}
              {outcome === "running" ? w.working : outcome === "done" ? w.done : w.failed}
            </p>
            <div ref={logBox} className="log-box mono" role="log" aria-live="polite">
              {lines.map((line, index) => (
                // Output lines can repeat; their position is their identity.
                // biome-ignore lint/suspicious/noArrayIndexKey: append-only log
                <div key={index}>{line || " "}</div>
              ))}
            </div>
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
        {step === "progress" && (
          <button
            type="button"
            className={`button ${outcome === "done" ? "button-primary" : ""}`}
            disabled={outcome === "running"}
            onClick={onClose}
          >
            {created ? w.close : t.common.close}
          </button>
        )}
      </div>
    </Modal>
  );
}
