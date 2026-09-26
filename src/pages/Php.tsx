import { useEffect, useState } from "react";
import type { Settings } from "../api";
import { RuntimeTable } from "../components/RuntimeTable";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { PageHeader, Section } from "../ui";

function LimitField({
  label,
  hint,
  value,
  min,
  onChange,
}: {
  label: string;
  hint: string;
  value: number;
  min: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="limit-field">
      <span className="limit-label">{label}</span>
      <span className="muted">{hint}</span>
      <span className="input-with-unit">
        <input
          className="input"
          type="number"
          min={min}
          value={Number.isNaN(value) ? "" : value}
          onChange={(event) => onChange(event.target.valueAsNumber)}
        />
        <span className="muted">MB</span>
      </span>
    </label>
  );
}

export function Php({
  runtimes,
  settings,
  onSaveLimits,
}: {
  runtimes: Runtimes;
  settings: Settings | null;
  onSaveLimits: (limits: { upload_max_mb: number; memory_limit_mb: number }) => Promise<void>;
}) {
  const t = useT();
  const [upload, setUpload] = useState(settings?.upload_max_mb ?? 100);
  const [memory, setMemory] = useState(settings?.memory_limit_mb ?? 512);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!settings) return;
    setUpload(settings.upload_max_mb);
    setMemory(settings.memory_limit_mb);
  }, [settings]);

  const php = runtimes.rows.filter((row) => row.product === "php");
  const tools = runtimes.rows.filter((row) => row.product === "composer");
  const changed = settings !== null && (upload !== settings.upload_max_mb || memory !== settings.memory_limit_mb);
  const valid = upload >= 1 && (memory === -1 || memory >= 16);

  return (
    <>
      <PageHeader title={t.php.title} />
      <div className="page-body">
        <Section title={t.php.versions} description={t.php.versionsHint}>
          <RuntimeTable rows={php} runtimes={runtimes} showDefault />
        </Section>

        <Section title={t.php.limits} description={t.php.limitsHint}>
          <div className="limits">
            <LimitField
              label={t.php.uploadMax}
              hint={t.php.uploadMaxHint}
              value={upload}
              min={1}
              onChange={setUpload}
            />
            <LimitField
              label={t.php.memoryLimit}
              hint={t.php.memoryLimitHint}
              value={memory}
              min={-1}
              onChange={setMemory}
            />
          </div>
          <div className="button-row">
            <button
              type="button"
              className="button button-primary"
              disabled={!changed || !valid || saving}
              onClick={() => {
                setSaving(true);
                void onSaveLimits({ upload_max_mb: upload, memory_limit_mb: memory }).finally(() => setSaving(false));
              }}
            >
              {t.php.saveLimits}
            </button>
            {!valid && <span className="error-text">{t.php.invalidLimits}</span>}
          </div>
        </Section>

        <Section title={t.php.tools} description={t.php.toolsHint}>
          <RuntimeTable rows={tools} runtimes={runtimes} />
        </Section>
      </div>
    </>
  );
}
