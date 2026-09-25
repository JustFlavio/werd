import { REPOSITORY_URL } from "../api";
import { Logo } from "../Logo";
import { PageHeader, Section } from "../ui";

export function About({ daemonVersion, onOpenUrl }: { daemonVersion: string; onOpenUrl: (url: string) => void }) {
  return (
    <>
      <PageHeader title="Informazioni" />
      <div className="page-body">
        <div className="about">
          <Logo size={56} />
          <div>
            <h2>Werd</h2>
            <p className="muted">Ambienti Laravel locali, senza Docker né WSL.</p>
            <p className="mono muted">
              Versione {__APP_VERSION__} · gestore {daemonVersion}
            </p>
          </div>
        </div>
        <Section
          title="Open source"
          description="Werd è software libero rilasciato con licenza MIT. Segnalazioni e contributi sono benvenuti."
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
