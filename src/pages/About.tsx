import { RefreshCw } from "lucide-react";
import { useState } from "react";
import { type DoctorResult, doctor, REPOSITORY_URL } from "../api";
import { useT } from "../i18n";
import { Logo } from "../Logo";
import { PageHeader, Section, StatusDot } from "../ui";

export function About({
  daemonVersion,
  onOpenUrl,
  onError,
}: {
  daemonVersion: string;
  onOpenUrl: (url: string) => void;
  onError: (cause: unknown) => void;
}) {
  const t = useT();
  const [checks, setChecks] = useState<DoctorResult[] | null>(null);

  return (
    <>
      <PageHeader title={t.about.title} />
      <div className="page-body">
        <div className="about">
          <Logo size={56} />
          <div>
            <h2>Werd</h2>
            <p className="muted">{t.about.tagline}</p>
            <p className="mono muted">{t.about.versions(__APP_VERSION__, daemonVersion)}</p>
          </div>
        </div>
        <Section
          title={t.about.openSource}
          description={t.about.openSourceHint}
          action={
            <button type="button" className="button" onClick={() => onOpenUrl(REPOSITORY_URL)}>
              GitHub
            </button>
          }
        />
        <Section
          title={t.about.troubleshooting}
          description={t.about.troubleshootingHint}
          action={
            <button type="button" className="button" onClick={() => void doctor().then(setChecks).catch(onError)}>
              <RefreshCw size={14} /> {t.about.runChecks}
            </button>
          }
        >
          {checks && (
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
