import { useCallback, useEffect, useRef, useState } from "react";
import {
  certificateStatus,
  type DomainsStatus,
  type Job,
  type Project,
  type ProjectSetup,
  projectSetup,
  type RuntimeLine,
} from "./api";
import type { Runtimes } from "./runtimes";

type Certificate = { exists: boolean; trusted: boolean };
export type PreparationIssue =
  | "requirements"
  | "php"
  | "hosts"
  | "https"
  | "environment"
  | "composer"
  | "key"
  | "config"
  | "node"
  | "frontend"
  | "assets";

/** Local preparation, separate from process status and application health. */
export function preparationIssues(
  project: Project,
  setup: ProjectSetup,
  certificate: Certificate,
  rows: RuntimeLine[],
  domains?: Pick<DomainsStatus, "missing"> | null,
): PreparationIssue[] {
  const issues: PreparationIssue[] = [];
  if (project.requirements?.length) issues.push("requirements");
  if (!rows.some((row) => row.product === "php" && row.line === project.php && row.installed)) issues.push("php");
  if (project.domain && domains?.missing.includes(project.domain)) issues.push("hosts");
  if (!certificate.trusted) issues.push("https");
  if (!setup.env_exists || (setup.env_pending ?? 0) > 0 || setup.env_error) issues.push("environment");
  if (!setup.vendor_installed) issues.push("composer");
  if (!setup.app_key_present) issues.push("key");
  if (setup.configuration_cached) issues.push("config");
  if (setup.frontend) {
    if (!setup.node_installed) issues.push("node");
    if (!setup.frontend_installed) issues.push("frontend");
    if (
      !setup.frontend_built &&
      project.vite?.status !== "running" &&
      !(project.status !== "running" && project.vite?.autostart)
    )
      issues.push("assets");
  }
  return issues;
}

/** A first server start creates the CA; it must remain reachable from preparation. */
export function needsPreparationBeforeStart(issues: PreparationIssue[], certificate: Certificate | null) {
  return issues.some((issue) => issue !== "https" || certificate?.exists);
}

export function preparationRunning(project: Project, setup: ProjectSetup | null, jobs: Job[]) {
  return jobs.some(
    (job) =>
      job.state === "running" &&
      (job.product === "setup" ||
        (job.product === "project-setup" && job.line === project.id) ||
        (job.product === "php" && job.line === project.php) ||
        (job.product === "node" && setup?.frontend && (!setup.node_required || job.line === setup.node_required))),
  );
}

export function useProjectPreparation(
  project: Project,
  runtimes: Runtimes,
  domains?: Pick<DomainsStatus, "missing"> | null,
) {
  const [snapshot, setSnapshot] = useState<{ setup: ProjectSetup; certificate: Certificate } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const reload = useCallback(async () => {
    const request = ++generation.current;
    try {
      const [setup, certificate] = await Promise.all([projectSetup(project.id), certificateStatus()]);
      if (request === generation.current) {
        setSnapshot({ setup, certificate });
        setError(null);
      }
    } catch (cause) {
      if (request === generation.current) {
        setSnapshot(null);
        setError(String(cause));
      }
      throw cause;
    }
  }, [project.id]);
  const connections = JSON.stringify(project.links);
  const inventory = runtimes.rows.map((row) => `${row.product}:${row.line}:${row.installed ?? ""}`).join("|");
  const jobs = runtimes.jobs.map((job) => `${job.id}:${job.state}`).join("|");
  // These snapshots invalidate checks after connection changes or completed downloads.
  // biome-ignore lint/correctness/useExhaustiveDependencies: derived snapshots deliberately trigger a reload.
  useEffect(() => {
    setSnapshot(null);
    void reload().catch(() => {});
    // Recheck changes made in an editor as well, without overlapping polls.
    let cancelled = false;
    let timer = 0;
    const poll = async () => {
      await reload().catch(() => {});
      if (!cancelled) timer = window.setTimeout(poll, 5000);
    };
    timer = window.setTimeout(poll, 5000);
    return () => {
      cancelled = true;
      ++generation.current;
      window.clearTimeout(timer);
    };
  }, [reload, project.php, project.node, project.status, connections, inventory, jobs]);
  const setup = snapshot?.setup ?? null;
  const certificate = snapshot?.certificate ?? null;
  const issues = snapshot
    ? preparationIssues(project, snapshot.setup, snapshot.certificate, runtimes.rows, domains)
    : [];
  return {
    setup,
    certificate,
    issues,
    error,
    reload,
    working: preparationRunning(project, setup, runtimes.jobs),
    loading: !snapshot && !error,
  };
}

export type ProjectPreparationState = ReturnType<typeof useProjectPreparation>;
