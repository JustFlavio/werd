import { Check } from "lucide-react";
import type { RuntimeInfo } from "../api";
import { useT } from "../i18n";
import { PageHeader, Section } from "../ui";

const LABELS: Record<string, string> = {
  php: "PHP",
  caddy: "Caddy",
  postgres: "PostgreSQL",
  pgvector: "pgvector",
  redis: "Redis",
  mailpit: "Mailpit",
  rustfs: "RustFS",
};

function RuntimeTable({
  runtimes,
  busy,
  onInstall,
}: {
  runtimes: RuntimeInfo[];
  busy: string | null;
  onInstall: (id: string) => void;
}) {
  const t = useT();
  return (
    <table className="table">
      <thead>
        <tr>
          <th>{t.php.name}</th>
          <th>{t.php.version}</th>
          <th>{t.php.notes}</th>
          <th className="cell-action">{t.php.installed}</th>
        </tr>
      </thead>
      <tbody>
        {runtimes.map((runtime) => (
          <tr key={runtime.id}>
            <td>{LABELS[runtime.id] ?? runtime.id}</td>
            <td className="mono">{runtime.version}</td>
            <td className="muted">{runtime.note}</td>
            <td className="cell-action">
              {runtime.installed ? (
                <Check size={16} className="ok-icon" aria-label={t.php.installed} />
              ) : (
                <button
                  type="button"
                  className="button button-small"
                  disabled={busy !== null}
                  onClick={() => onInstall(runtime.id)}
                >
                  {busy === runtime.id ? t.php.downloading : t.php.install}
                </button>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function Php({
  runtimes,
  busy,
  onInstall,
}: {
  runtimes: RuntimeInfo[];
  busy: string | null;
  onInstall: (id: string) => void;
}) {
  const t = useT();
  const php = runtimes.filter((runtime) => runtime.id === "php");
  const others = runtimes.filter((runtime) => runtime.id !== "php");

  return (
    <>
      <PageHeader title={t.php.title} />
      <div className="page-body">
        <Section title={t.php.versions} description={t.php.versionsHint}>
          {php.length ? (
            <RuntimeTable runtimes={php} busy={busy} onInstall={onInstall} />
          ) : (
            <p className="muted">{t.php.unavailable}</p>
          )}
        </Section>
        <Section title={t.php.servers} description={t.php.serversHint}>
          {others.length ? (
            <RuntimeTable runtimes={others} busy={busy} onInstall={onInstall} />
          ) : (
            <p className="muted">{t.php.unavailable}</p>
          )}
        </Section>
      </div>
    </>
  );
}
