import { REPOSITORY_URL } from "../api";
import { useT } from "../i18n";
import { Logo } from "../Logo";
import { PageHeader, Section } from "../ui";

export function About({ daemonVersion, onOpenUrl }: { daemonVersion: string; onOpenUrl: (url: string) => void }) {
  const t = useT();
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
      </div>
    </>
  );
}
