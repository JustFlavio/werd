import type { SystemInfo } from "../api";
import { LOCALES, type Locale, useI18n } from "../i18n";
import { CopyButton, PageHeader, Section } from "../ui";

export function General({
  system,
  busy,
  onTogglePath,
  onRefreshCatalog,
  onTrustCa,
}: {
  system: SystemInfo | null;
  busy: string | null;
  onTogglePath: (enable: boolean) => void;
  onRefreshCatalog: () => void;
  onTrustCa: () => void;
}) {
  const { t, locale, setLocale } = useI18n();
  const generated = system?.catalog_generated ? new Date(system.catalog_generated).toLocaleDateString(locale) : "—";

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
          title={t.general.cli}
          description={t.general.cliHint}
          action={
            <label className="checkbox">
              <input
                type="checkbox"
                checked={system?.path_enabled ?? false}
                disabled={!system || busy === "path"}
                onChange={(event) => onTogglePath(event.target.checked)}
              />
              {t.general.cliToggle}
            </label>
          }
        >
          {system?.path_enabled && <p className="muted">{t.general.cliEnabled(system.bin)}</p>}
        </Section>

        <Section
          title={t.general.catalog}
          description={t.general.catalogHint(generated)}
          action={
            <button
              type="button"
              className="button"
              disabled={!system?.catalog_refreshable || busy === "catalog"}
              title={system?.catalog_refreshable ? undefined : t.general.catalogUnavailable}
              onClick={onRefreshCatalog}
            >
              {t.general.checkUpdates}
            </button>
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

        <Section title={t.general.dataFolder} description={t.general.dataFolderHint}>
          <div className="path-row">
            <code className="mono">{system?.home ?? "—"}</code>
            {system && <CopyButton text={system.home} />}
          </div>
        </Section>
      </div>
    </>
  );
}
