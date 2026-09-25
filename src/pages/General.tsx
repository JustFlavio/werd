import { RefreshCw } from "lucide-react";
import type { DoctorResult } from "../api";
import { PageHeader, Section, StatusDot } from "../ui";

export function General({
  checks,
  daemonVersion,
  onRecheck,
  onTrustCa,
}: {
  checks: DoctorResult[];
  daemonVersion: string;
  onRecheck: () => void;
  onTrustCa: () => void;
}) {
  return (
    <>
      <PageHeader title="Generale" />
      <div className="page-body">
        <Section
          title="Certificato HTTPS"
          description="Werd firma i certificati dei siti con una CA locale. Rendila attendibile per il tuo utente per aprire i siti senza avvisi del browser."
          action={
            <button type="button" className="button" onClick={onTrustCa}>
              Rendi attendibile
            </button>
          }
        />

        <Section
          title="Gestore locale"
          description="Il gestore tiene attivi i siti anche quando chiudi questa finestra. GUI e CLI (werd) parlano con lo stesso processo."
        >
          <dl className="fields">
            <dt>Stato</dt>
            <dd>
              <StatusDot status={daemonVersion === "—" ? "fail" : "ok"} />{" "}
              {daemonVersion === "—" ? "Non raggiungibile" : "In esecuzione"}
            </dd>
            <dt>Versione</dt>
            <dd className="mono">{daemonVersion}</dd>
          </dl>
        </Section>

        <Section
          title="Diagnostica"
          description="Controlli sull’ambiente: porte, runtime installati e prerequisiti."
          action={
            <button type="button" className="button" onClick={onRecheck}>
              <RefreshCw size={14} /> Ricontrolla
            </button>
          }
        >
          {checks.length === 0 ? (
            <p className="muted">Nessun controllo disponibile.</p>
          ) : (
            <ul className="checks">
              {checks.map((check) => (
                <li key={check.label}>
                  <StatusDot status={check.ok ? "ok" : "fail"} />
                  <div>
                    <strong>{check.label}</strong>
                    <span className="muted">{check.detail}</span>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Section>
      </div>
    </>
  );
}
