import { useEffect, useState } from "react";
import {
  backupDatabase,
  type DatabaseBackup,
  databaseBackups,
  type Job,
  listJobs,
  restoreDatabase,
  type ServiceInstance,
} from "../api";
import { useT } from "../i18n";
import { Modal } from "../ui";

const validName = (name: string) => /^[a-z][a-z0-9_]{0,62}$/.test(name);

export function DatabaseBackups({
  instance,
  projectDatabase,
  onClose,
}: {
  instance: ServiceInstance;
  projectDatabase?: string;
  onClose: () => void;
}) {
  const t = useT();
  const [records, setRecords] = useState<DatabaseBackup[] | null>(null);
  const [database, setDatabase] = useState(projectDatabase ?? "");
  const [selected, setSelected] = useState("");
  const [target, setTarget] = useState("");
  const [job, setJob] = useState<Job | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [engineBusy, setEngineBusy] = useState(projectDatabase !== undefined);
  const [error, setError] = useState<string | null>(null);
  const watchedJob = job?.id;

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const jobs = await listJobs();
        if (alive) {
          // Jobs have no instance/database identity. Block concurrent engine work without
          // presenting another project's job as this project's backup or restore.
          setEngineBusy(
            projectDatabase !== undefined &&
              jobs.some(
                (candidate) =>
                  candidate.product === instance.product &&
                  candidate.line === instance.line &&
                  candidate.state === "running" &&
                  candidate.id !== watchedJob,
              ),
          );
          setJob((current) => {
            // Ignore inventory requested before this operation was selected.
            if (current?.id !== watchedJob) return current;
            return current
              ? (jobs.find((candidate) => candidate.id === current.id) ??
                  (current.state === "running" ? { ...current, state: "failed", error: t.backups.lost } : current))
              : projectDatabase !== undefined
                ? null
                : (jobs.find(
                    (candidate) =>
                      candidate.product === instance.product &&
                      candidate.line === instance.line &&
                      ["backup", "restore"].includes(candidate.action) &&
                      candidate.state === "running",
                  ) ?? null);
          });
        }
      } catch (cause) {
        if (alive) setError(String(cause));
      } finally {
        if (alive) timer = setTimeout(poll, 500);
      }
    };
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [instance.product, instance.line, projectDatabase, watchedJob, t.backups.lost]);

  const state = job?.state;
  useEffect(() => {
    if (state === "running" || engineBusy) return;
    let alive = true;
    void databaseBackups()
      .then((backups) => {
        if (alive) setRecords(backups);
      })
      .catch((cause) => {
        if (alive) setError(String(cause));
      });
    return () => {
      alive = false;
    };
  }, [state, engineBusy]);

  const compatible = (records ?? []).filter(
    (record) =>
      record.product === instance.product &&
      record.line === instance.line &&
      (projectDatabase === undefined || (record.instance === instance.id && record.database === projectDatabase)),
  );
  const choice = compatible.find((record) => record.id === selected) ?? compatible[0];
  const busy = submitting || job?.state === "running" || engineBusy;
  const stopped = instance.status !== "running";
  const start = async (restore: boolean) => {
    setSubmitting(true);
    setError(null);
    try {
      const started = restore
        ? await restoreDatabase(instance.id, choice.id, target)
        : await backupDatabase(instance.id, database);
      setJob(started);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal title={`${t.backups.title} · ${instance.name}`} onClose={onClose}>
      <p className="muted">{t.backups.hint}</p>
      {stopped && <p className="callout">{t.backups.stopped}</p>}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void start(false);
        }}
      >
        <label className="field">
          <span>{t.backups.database}</span>
          <input
            className="input"
            value={database}
            readOnly={projectDatabase !== undefined}
            disabled={busy}
            onChange={(event) => setDatabase(event.target.value)}
          />
        </label>
        <button type="submit" className="button" disabled={busy || stopped || !validName(database)}>
          {t.backups.create}
        </button>
      </form>
      {records === null ? (
        <p>{t.backups.loading}</p>
      ) : compatible.length === 0 ? (
        <p className="muted">{t.backups.empty}</p>
      ) : (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void start(true);
          }}
        >
          <label className="field">
            <span>{t.backups.choose}</span>
            <select
              className="input"
              value={choice.id}
              disabled={busy}
              onChange={(event) => setSelected(event.target.value)}
            >
              {compatible.map((record) => (
                <option key={record.id} value={record.id}>
                  {record.database} · {new Date(record.created_at * 1000).toLocaleString()} · {record.service_name}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{t.backups.target}</span>
            <input
              className="input"
              value={target}
              disabled={busy}
              onChange={(event) => setTarget(event.target.value)}
            />
          </label>
          <p className="muted">{t.backups.targetHint}</p>
          <button type="submit" className="button" disabled={busy || stopped || !validName(target)}>
            {t.backups.restore}
          </button>
        </form>
      )}
      {busy && <p role="status">{engineBusy ? t.workspace.engineBusy : t.backups.working}</p>}
      {job?.state === "done" && (
        <p role="status">{job.action === "restore" ? t.backups.restored(job.result ?? target) : t.backups.created}</p>
      )}
      {(error || job?.state === "failed") && (
        <p role="alert" className="callout callout-error">
          {error ?? job?.error}
        </p>
      )}
    </Modal>
  );
}
