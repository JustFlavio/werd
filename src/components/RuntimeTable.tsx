import { Check, Trash2 } from "lucide-react";
import type { Job, RuntimeLine } from "../api";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";

function Progress({ job }: { job: Job }) {
  const t = useT();
  const percent = job.total ? Math.min(100, Math.round((job.downloaded / job.total) * 100)) : null;
  return (
    <div
      className="progress"
      role="progressbar"
      aria-valuenow={percent ?? undefined}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="progress-track">
        <div
          className={`progress-fill ${percent === null ? "indeterminate" : ""}`}
          style={{ width: `${percent ?? 40}%` }}
        />
      </div>
      <span className="progress-label">{percent === null ? t.runtimes.step(job.step) : `${percent}%`}</span>
    </div>
  );
}

/** Version lines of one product with install, update, default and uninstall actions. */
export function RuntimeTable({
  rows,
  runtimes,
  showDefault = false,
  showLts = false,
}: {
  rows: RuntimeLine[];
  runtimes: Runtimes;
  showDefault?: boolean;
  showLts?: boolean;
}) {
  const t = useT();
  if (rows.length === 0) return <p className="muted">{t.runtimes.unavailable}</p>;

  return (
    <table className="table runtime-table">
      <thead>
        <tr>
          <th>{t.runtimes.version}</th>
          <th>{t.runtimes.status}</th>
          <th className="cell-action" />
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const job = runtimes.jobFor(row.product, row.line);
          const busy = job?.state === "running";
          const failed = job?.state === "failed" ? job.error : null;
          return (
            <tr key={`${row.product}-${row.line}`}>
              <td>
                <span className="runtime-version">
                  {row.label === "PHP" || row.label === "Node.js" ? row.line : `${row.label} ${row.line}`}
                  {row.installed && <span className="muted"> ({row.installed})</span>}
                </span>
                {showDefault && row.is_default && <span className="badge badge-accent">{t.runtimes.default}</span>}
                {showLts && row.lts && <span className="badge">LTS</span>}
              </td>
              <td>
                {busy && job ? (
                  <Progress job={job} />
                ) : failed ? (
                  <span className="error-text" title={failed}>
                    {t.runtimes.failed}
                  </span>
                ) : row.update_available ? (
                  <span className="muted">{t.runtimes.updateAvailable(row.latest ?? "")}</span>
                ) : row.installed ? (
                  <span className="installed">
                    <Check size={15} aria-hidden /> {t.runtimes.installed}
                  </span>
                ) : row.latest ? (
                  <span className="muted">{row.latest}</span>
                ) : null}
              </td>
              <td className="cell-action">
                <div className="button-row button-row-end">
                  {!row.installed && (
                    <button
                      type="button"
                      className="button button-small"
                      disabled={busy}
                      onClick={() => runtimes.install(row)}
                    >
                      {t.runtimes.install}
                    </button>
                  )}
                  {row.update_available && (
                    <button
                      type="button"
                      className="button button-small button-primary"
                      disabled={busy}
                      onClick={() => runtimes.update(row)}
                    >
                      {t.runtimes.update}
                    </button>
                  )}
                  {showDefault && row.installed && !row.is_default && (
                    <button
                      type="button"
                      className="button button-small"
                      disabled={busy}
                      onClick={() => runtimes.setDefault(row)}
                    >
                      {t.runtimes.makeDefault}
                    </button>
                  )}
                  {row.installed && (
                    <button
                      type="button"
                      className="icon-button"
                      disabled={busy}
                      title={t.runtimes.uninstall}
                      aria-label={`${t.runtimes.uninstall} ${row.label} ${row.line}`}
                      onClick={() => {
                        if (window.confirm(t.runtimes.confirmUninstall(row.label, row.line))) runtimes.uninstall(row);
                      }}
                    >
                      <Trash2 size={15} />
                    </button>
                  )}
                </div>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
