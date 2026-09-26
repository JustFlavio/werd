import { useCallback, useEffect, useRef, useState } from "react";
import {
  installRuntime,
  type Job,
  listJobs,
  listRuntimes,
  type RuntimeLine,
  setDefaultRuntime,
  uninstallRuntime,
  updateRuntime,
} from "./api";

export interface Runtimes {
  rows: RuntimeLine[];
  jobs: Job[];
  /** The newest job for a product line, if any. */
  jobFor: (product: string, line: string) => Job | undefined;
  install: (row: RuntimeLine) => void;
  update: (row: RuntimeLine) => void;
  uninstall: (row: RuntimeLine) => void;
  setDefault: (row: RuntimeLine) => void;
  refresh: () => Promise<void>;
}

const FAST_POLL = 400;
const SLOW_POLL = 5000;

/** Runtime lines and background jobs, polled quickly while a download runs. */
export function useRuntimes(fail: (cause: unknown) => void): Runtimes {
  const [rows, setRows] = useState<RuntimeLine[]>([]);
  const [jobs, setJobs] = useState<Job[]>([]);
  const running = useRef(new Set<string>());

  const refresh = useCallback(async () => {
    try {
      setRows(await listRuntimes());
    } catch (cause) {
      fail(cause);
    }
  }, [fail]);

  const pollJobs = useCallback(async () => {
    try {
      const next = await listJobs();
      setJobs(next);
      const nowRunning = new Set(next.filter((job) => job.state === "running").map((job) => job.id));
      const finished = [...running.current].some((id) => !nowRunning.has(id));
      running.current = nowRunning;
      if (finished) await refresh();
      return nowRunning.size > 0;
    } catch {
      return false;
    }
  }, [refresh]);

  useEffect(() => {
    void refresh();
    let timer = 0;
    let cancelled = false;
    const loop = async () => {
      const busy = await pollJobs();
      if (!cancelled) timer = window.setTimeout(loop, busy ? FAST_POLL : SLOW_POLL);
    };
    void loop();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [refresh, pollJobs]);

  const startJob = useCallback(
    (action: typeof installRuntime) => (row: RuntimeLine) => {
      void action(row.product, row.line)
        .then((job) => {
          running.current.add(job.id);
          setJobs((current) => [...current, job]);
          // Switch to fast polling right away.
          void pollJobs();
        })
        .catch(fail);
    },
    [fail, pollJobs],
  );

  const jobFor = useCallback(
    (product: string, line: string) =>
      jobs.filter((job) => job.product === product && job.line === line).sort((a, b) => b.started_at - a.started_at)[0],
    [jobs],
  );

  return {
    rows,
    jobs,
    jobFor,
    install: startJob(installRuntime),
    update: startJob(updateRuntime),
    uninstall: (row) => void uninstallRuntime(row.product, row.line).then(refresh).catch(fail),
    setDefault: (row) => void setDefaultRuntime(row.product, row.line).then(refresh).catch(fail),
    refresh,
  };
}
