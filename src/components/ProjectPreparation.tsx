import { type ReactNode, useCallback, useEffect, useState } from "react";
import {
  applyProjectEnv,
  certificateStatus,
  type DomainsStatus,
  type EnvPreview,
  installProjectDependencies,
  installRuntime,
  openSite,
  openUrl,
  type Project,
  type ProjectSetup,
  previewProjectEnv,
  projectSetup,
  setViteAutostart,
  startVite,
  trustCa,
} from "../api";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { Modal, Section } from "../ui";
import { Progress } from "./RuntimeTable";

export function ProjectPreparation({
  project,
  runtimes,
  domains,
  onUpdateHosts,
  onChanged,
}: {
  project: Project;
  runtimes: Runtimes;
  domains?: DomainsStatus | null;
  onUpdateHosts?: () => Promise<void>;
  onChanged: () => Promise<void>;
}) {
  const t = useT();
  const p = t.preparation;
  const [setup, setSetup] = useState<ProjectSetup | null>(null);
  const [certificate, setCertificate] = useState<{ exists: boolean; trusted: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<EnvPreview | null>(null);
  const [backup, setBackup] = useState<string | null>(null);
  const reload = useCallback(async () => {
    const [status, ca] = await Promise.all([projectSetup(project.id), certificateStatus()]);
    setSetup(status);
    setCertificate(ca);
  }, [project.id]);
  const links = JSON.stringify(project.links);
  const inventory = runtimes.rows.map((row) => `${row.product}:${row.line}:${row.installed ?? ""}`).join("|");
  const job = runtimes.jobFor("project-setup", project.id);
  const phpJob = runtimes.jobFor("php", project.php);
  const nodeRow = setup?.node_required
    ? runtimes.rows.find((row) => row.product === "node" && row.line === setup.node_required)
    : runtimes.rows
        .filter(
          (row) => row.product === "node" && row.lts && (!row.eol || row.eol >= new Date().toISOString().slice(0, 10)),
        )
        .sort((a, b) => b.line.localeCompare(a.line, undefined, { numeric: true }))[0];
  const nodeJob = nodeRow ? runtimes.jobFor("node", nodeRow.line) : undefined;
  const active = [job, phpJob, nodeJob].find((job) => job?.state === "running");
  // Inventory and connection changes invalidate these local checks.
  // biome-ignore lint/correctness/useExhaustiveDependencies: derived snapshots deliberately trigger a reload.
  useEffect(() => {
    let alive = true;
    void Promise.all([projectSetup(project.id), certificateStatus()])
      .then(([status, ca]) => {
        if (alive) {
          setSetup(status);
          setCertificate(ca);
          setError(null);
        }
      })
      .catch((cause) => {
        if (alive) setError(String(cause));
      });
    return () => {
      alive = false;
    };
  }, [project.id, project.status, links, inventory, job?.state]);
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      await reload();
      await onChanged();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };
  const phpMissing =
    runtimes.rows.length > 0 &&
    !runtimes.rows.some((row) => row.product === "php" && row.line === project.php && row.installed);
  const hostsMissing = Boolean(project.domain && domains?.missing.includes(project.domain));
  const environmentPending = setup && (!setup.env_exists || (setup.env_pending ?? 0) > 0);
  const frontendPreviewMissing = setup?.frontend && !setup.frontend_built && project.vite?.status !== "running";
  const needsWork =
    phpMissing ||
    hostsMissing ||
    !certificate?.trusted ||
    !setup ||
    !setup.vendor_installed ||
    !setup.app_key_present ||
    setup.configuration_cached ||
    environmentPending ||
    setup.env_error ||
    frontendPreviewMissing ||
    (setup.frontend && (!setup.frontend_installed || !setup.node_installed));
  const install = (product: string, line: string) =>
    run(async () => runtimes.track(await installRuntime(product, line)));
  const dependencies = (kind: "composer" | "npm" | "key" | "config") =>
    run(async () => runtimes.track(await installProjectDependencies(project.id, kind)));
  const disabled = busy || Boolean(active);
  const issue = (label: string, action?: ReactNode) => (
    <li>
      <span>{label}</span>
      {action}
    </li>
  );
  return (
    <Section title={p.title} description={p.hint}>
      {!setup && !error && (
        <p role="status" className="muted">
          {t.common.loading}
        </p>
      )}
      {!needsWork && <p className="prepared-status">{p.configured}</p>}
      <ul className="preparation-list">
        {phpMissing &&
          issue(
            p.phpMissing(project.php),
            <button
              type="button"
              className="button button-small"
              disabled={disabled}
              onClick={() => void install("php", project.php)}
            >
              {p.installPhp(project.php)}
            </button>,
          )}
        {hostsMissing &&
          issue(
            p.domainsNeeded,
            onUpdateHosts && (
              <button
                type="button"
                className="button button-small"
                disabled={disabled}
                onClick={() => void run(onUpdateHosts)}
              >
                {p.configureDomains}
              </button>
            ),
          )}
        {certificate &&
          !certificate.trusted &&
          issue(
            certificate.exists ? p.httpsNeeded : p.httpsOnStart,
            certificate.exists && (
              <button
                type="button"
                className="button button-small"
                disabled={disabled}
                onClick={() =>
                  void run(async () => {
                    await trustCa();
                    if (!(await certificateStatus()).trusted) throw new Error(t.wizard.trustIncomplete);
                    if (project.status === "running" && !hostsMissing) await openSite(project.id);
                  })
                }
              >
                {p.configureHttps}
              </button>
            ),
          )}
        {environmentPending &&
          issue(
            p.environmentNeeded,
            <button
              type="button"
              className="button button-small"
              disabled={disabled}
              onClick={() => void run(async () => setPreview(await previewProjectEnv(project.id)))}
            >
              {p.reviewEnv}
            </button>,
          )}
        {setup &&
          !setup.vendor_installed &&
          issue(
            p.composerNeeded,
            <button
              type="button"
              className="button button-small"
              disabled={disabled || phpMissing}
              onClick={() => void dependencies("composer")}
            >
              {p.installDependencies}
            </button>,
          )}
        {setup &&
          !setup.app_key_present &&
          issue(
            p.keyNeeded,
            <button
              type="button"
              className="button button-small"
              disabled={disabled || phpMissing || !setup.vendor_installed || !setup.env_exists}
              onClick={() => void dependencies("key")}
            >
              {p.generateKey}
            </button>,
          )}
        {setup?.configuration_cached &&
          issue(
            p.cachedConfiguration,
            <button
              type="button"
              className="button button-small"
              disabled={disabled || phpMissing || !setup.vendor_installed}
              onClick={() => void dependencies("config")}
            >
              {p.clearConfiguration}
            </button>,
          )}
        {setup?.frontend &&
          !setup.node_installed &&
          issue(
            p.nodeNeeded(setup.node_required ?? nodeRow?.line ?? ""),
            nodeRow ? (
              <button
                type="button"
                className="button button-small"
                disabled={disabled}
                onClick={() => void install("node", nodeRow.line)}
              >
                {p.installNode(nodeRow.line)}
              </button>
            ) : (
              <span className="muted">{p.manageNode}</span>
            ),
          )}
        {setup?.frontend &&
          !setup.frontend_installed &&
          issue(
            p.frontendNeeded,
            <button
              type="button"
              className="button button-small"
              disabled={disabled || !setup.node_installed}
              onClick={() => void dependencies("npm")}
            >
              {p.installFrontend}
            </button>,
          )}
        {frontendPreviewMissing &&
          issue(
            p.frontendPreviewNeeded,
            <button
              type="button"
              className="button button-small"
              disabled={disabled || !setup?.node_installed || !setup.frontend_installed}
              onClick={() =>
                void run(async () => {
                  await setViteAutostart(project.id, true);
                  if (project.status === "running") await startVite(project.id);
                })
              }
            >
              {p.enablePreview}
            </button>,
          )}
        {setup?.env_error &&
          issue(
            p.environmentManual,
            <details>
              <summary>{p.details}</summary>
              <p>{setup.env_error}</p>
            </details>,
          )}
      </ul>
      {active && <Progress job={active} />}
      {job?.state === "failed" && (
        <div className="callout callout-error">
          <strong>{p.dependenciesFailed}</strong>
          <details>
            <summary>{p.details}</summary>
            <p>{job.error}</p>
            {job.log?.length ? <pre className="log-box mono">{job.log.join("\n")}</pre> : null}
          </details>
        </div>
      )}
      {error && !preview && (
        <div role="alert" className="callout callout-error">
          <p>{error}</p>
          <button type="button" className="button button-small" disabled={disabled} onClick={() => void run(reload)}>
            {t.common.retry}
          </button>
        </div>
      )}
      {backup && (
        <p role="status" className="muted">
          {p.envApplied} <span className="mono">{backup}</span>
        </p>
      )}
      {hostsMissing && project.ports?.site && project.status === "running" && certificate?.trusted && (
        <button
          type="button"
          className="link"
          onClick={() => void run(() => openUrl(`https://localhost:${project.ports?.site}`))}
        >
          {p.openLocal}
        </button>
      )}
      {preview && (
        <Modal
          title={p.reviewEnv}
          onClose={() => {
            if (!busy) setPreview(null);
          }}
          wide
        >
          <p className="muted">{p.envHint}</p>
          {!preview.exists && <p className="callout">{p.createEnv}</p>}
          <table className="table">
            <thead>
              <tr>
                <th>{p.key}</th>
                <th>{p.current}</th>
                <th>{p.proposed}</th>
              </tr>
            </thead>
            <tbody>
              {preview.changes.map((change) => {
                const secret = /PASSWORD|SECRET|TOKEN|(?:^|_)KEY(?:_|$)/.test(change.key);
                return (
                  <tr key={change.key}>
                    <th className="mono">{change.key}</th>
                    <td className="mono">{secret && change.before ? "••••••" : (change.before ?? "—")}</td>
                    <td className="mono">{secret && change.after ? "••••••" : change.after || "—"}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <p className="muted">{p.envAfter}</p>
          {error && (
            <p role="alert" className="callout callout-error">
              {error}
            </p>
          )}
          {error && (
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() => void run(async () => setPreview(await previewProjectEnv(project.id)))}
            >
              {p.refreshPreview}
            </button>
          )}
          <div className="modal-actions">
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() => {
                setPreview(null);
                setError(null);
              }}
            >
              {t.common.cancel}
            </button>
            <button
              type="button"
              className="button button-primary"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const result = await applyProjectEnv(project.id, preview.revision);
                  setBackup(result.backup ?? p.envSaved);
                  setPreview(null);
                })
              }
            >
              {p.applyEnv}
            </button>
          </div>
        </Modal>
      )}
    </Section>
  );
}
