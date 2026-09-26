import { useEffect, useState } from "react";
import type { DomainsStatus, Settings, SystemInfo } from "../api";
import { LOCALES, type Locale, useI18n } from "../i18n";
import { CopyButton, PageHeader, Section } from "../ui";

export function General({
  system,
  settings,
  domains,
  busy,
  onSaveDomains,
  onUpdateHosts,
  onTogglePath,
  onRefreshCatalog,
  onTrustCa,
}: {
  system: SystemInfo | null;
  settings: Settings | null;
  domains: DomainsStatus | null;
  busy: string | null;
  onSaveDomains: (changes: Partial<Pick<Settings, "domains" | "https_port">>) => void;
  onUpdateHosts: () => void;
  onTogglePath: (enable: boolean) => void;
  onRefreshCatalog: () => void;
  onTrustCa: () => void;
}) {
  const { t, locale, setLocale } = useI18n();
  const [port, setPort] = useState("");
  useEffect(() => setPort(settings ? String(settings.https_port) : ""), [settings]);
  const portNumber = Number(port);
  const portValid = Number.isInteger(portNumber) && portNumber >= 1 && portNumber <= 65535;
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
          title={t.general.domains}
          description={t.general.domainsHint}
          action={
            <label className="checkbox">
              <input
                type="checkbox"
                checked={settings?.domains ?? false}
                disabled={!settings || busy === "domains"}
                onChange={(event) => onSaveDomains({ domains: event.target.checked })}
              />
              {t.general.domainsToggle}
            </label>
          }
        >
          {settings?.domains && (
            <>
              {domains?.warning && <div className="callout callout-warn">{domains.warning}</div>}
              <form
                className="inline-form"
                onSubmit={(event) => {
                  event.preventDefault();
                  if (portValid) onSaveDomains({ https_port: portNumber });
                }}
              >
                <label htmlFor="https-port">{t.general.httpsPort}</label>
                <input
                  id="https-port"
                  className="input input-narrow"
                  inputMode="numeric"
                  value={port}
                  aria-invalid={!portValid}
                  onChange={(event) => setPort(event.target.value)}
                />
                <button
                  type="submit"
                  className="button"
                  disabled={!portValid || portNumber === settings.https_port || busy === "domains"}
                >
                  {t.common.save}
                </button>
              </form>
              <p className="muted">{t.general.httpsPortHint}</p>
              <div className="button-row">
                <button
                  type="button"
                  className="button"
                  disabled={busy === "hosts" || !domains?.domains.length}
                  onClick={onUpdateHosts}
                >
                  {t.shell.updateHosts}
                </button>
                {domains && domains.missing.length === 0 && domains.domains.length > 0 && (
                  <span className="muted">{t.general.hostsOk(domains.domains.length)}</span>
                )}
              </div>
            </>
          )}
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
