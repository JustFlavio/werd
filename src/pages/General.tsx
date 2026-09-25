import { RefreshCw } from "lucide-react";
import type { DoctorResult } from "../api";
import { LOCALES, type Locale, useI18n } from "../i18n";
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
  const { t, locale, setLocale } = useI18n();
  const reachable = daemonVersion !== "—";

  return (
    <>
      <PageHeader title={t.general.title} />
      <div className="page-body">
        <Section
          title={t.general.language}
          description={t.general.languageHint}
          action={
            <select
              className="select"
              value={locale}
              aria-label={t.general.language}
              onChange={(event) => setLocale(event.target.value as Locale)}
            >
              {Object.entries(LOCALES).map(([code, name]) => (
                <option key={code} value={code}>
                  {name}
                </option>
              ))}
            </select>
          }
        />

        <Section
          title={t.general.certificate}
          description={t.general.certificateHint}
          action={
            <button type="button" className="button" onClick={onTrustCa}>
              {t.general.trust}
            </button>
          }
        />

        <Section title={t.general.daemon} description={t.general.daemonHint}>
          <dl className="fields">
            <dt>{t.general.state}</dt>
            <dd>
              <StatusDot status={reachable ? "ok" : "fail"} />
              {reachable ? t.general.running : t.general.unreachable}
            </dd>
            <dt>{t.general.version}</dt>
            <dd className="mono">{daemonVersion}</dd>
          </dl>
        </Section>

        <Section
          title={t.general.diagnostics}
          description={t.general.diagnosticsHint}
          action={
            <button type="button" className="button" onClick={onRecheck}>
              <RefreshCw size={14} /> {t.general.recheck}
            </button>
          }
        >
          {checks.length === 0 ? (
            <p className="muted">{t.general.noChecks}</p>
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
