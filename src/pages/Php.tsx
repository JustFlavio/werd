import { Check } from "lucide-react";
import type { RuntimeInfo } from "../api";
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
  return (
    <table className="table">
      <thead>
        <tr>
          <th>Nome</th>
          <th>Versione</th>
          <th>Note</th>
          <th className="cell-action">Installato</th>
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
                <Check size={16} className="ok-icon" aria-label="Installato" />
              ) : (
                <button className="button button-small" disabled={busy !== null} onClick={() => onInstall(runtime.id)}>
                  {busy === runtime.id ? "Download…" : "Installa"}
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
  const php = runtimes.filter((runtime) => runtime.id === "php");
  const others = runtimes.filter((runtime) => runtime.id !== "php");

  return (
    <>
      <PageHeader title="PHP e runtime" />
      <div className="page-body">
        <Section
          title="Versioni PHP"
          description="Build NTS ufficiali, scaricate solo quando servono e verificate con checksum."
        >
          {php.length ? (
            <RuntimeTable runtimes={php} busy={busy} onInstall={onInstall} />
          ) : (
            <p className="muted">Catalogo non disponibile.</p>
          )}
        </Section>
        <Section
          title="Server e servizi"
          description="Binari usati dai siti. Ogni sito avvia istanze proprie a partire da questi."
        >
          {others.length ? (
            <RuntimeTable runtimes={others} busy={busy} onInstall={onInstall} />
          ) : (
            <p className="muted">Catalogo non disponibile.</p>
          )}
        </Section>
      </div>
    </>
  );
}
