import { ExternalLink, Info, Pencil, Play, Plus, Square, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  createService,
  deleteService,
  type Job,
  type NewService,
  renameService,
  type ServiceDetails,
  type ServiceInstance,
  type ServiceOffering,
  serviceCatalog,
  serviceDetails,
  setServiceAutostart,
  startService,
  stopService,
} from "../api";
import { RuntimeTable } from "../components/RuntimeTable";
import { useT } from "../i18n";
import type { Runtimes } from "../runtimes";
import { CopyButton, EmptyState, Modal, PageHeader, Section, StatusDot } from "../ui";

const CATEGORIES = ["database", "cache", "queue", "search", "storage", "mail"] as const;

export function Services({
  instances,
  runtimes,
  onChanged,
  onOpenUrl,
  onError,
}: {
  instances: ServiceInstance[];
  runtimes: Runtimes;
  onChanged: () => Promise<void>;
  onOpenUrl: (url: string) => void;
  onError: (cause: unknown) => void;
}) {
  const t = useT();
  const [adding, setAdding] = useState(false);
  const [details, setDetails] = useState<ServiceDetails | null>(null);
  const [removing, setRemoving] = useState<ServiceInstance | null>(null);
  const [renaming, setRenaming] = useState<ServiceInstance | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const act = (id: string, action: () => Promise<unknown>) => {
    setBusy(id);
    void action()
      .then(onChanged)
      .catch(onError)
      .finally(() => setBusy(null));
  };

  const binaries = runtimes.rows.filter((row) => (row.kind === "service" || row.kind === "extension") && row.installed);
  const addButton = (
    <button type="button" className="button button-primary" onClick={() => setAdding(true)}>
      <Plus size={15} /> {t.services.add}
    </button>
  );

  return (
    <>
      <PageHeader title={t.services.title}>{instances.length > 0 && addButton}</PageHeader>
      {instances.length === 0 ? (
        <EmptyState title={t.services.emptyTitle} description={t.services.emptyDescription}>
          {addButton}
        </EmptyState>
      ) : (
        <div className="page-body">
          <Section title={t.services.instances} description={t.services.instancesHint}>
            <table className="table">
              <thead>
                <tr>
                  <th>{t.services.name}</th>
                  <th>{t.services.status}</th>
                  <th>{t.services.port}</th>
                  <th>{t.services.autostart}</th>
                  <th className="cell-action" />
                </tr>
              </thead>
              <tbody>
                {instances.map((instance) => {
                  const job = runtimes.jobFor(instance.product, instance.line);
                  const installing = job?.state === "running" && instance.status !== "running";
                  const running = instance.status === "running";
                  return (
                    <tr key={instance.id}>
                      <td>
                        <strong className="strong">{instance.name}</strong>
                        {instance.extensions?.map((extension) => (
                          <span key={extension} className="badge">
                            {extension}
                          </span>
                        ))}
                      </td>
                      <td>
                        {installing ? (
                          <span className="muted">{t.services.installing}</span>
                        ) : (
                          <span title={instance.error ?? undefined}>
                            <StatusDot status={instance.status} label />
                          </span>
                        )}
                      </td>
                      <td className="mono">{instance.port}</td>
                      <td>
                        <input
                          type="checkbox"
                          className="checkbox-input"
                          checked={instance.autostart}
                          aria-label={`${t.services.autostart} ${instance.name}`}
                          onChange={(event) =>
                            act(instance.id, () => setServiceAutostart(instance.id, event.target.checked))
                          }
                        />
                      </td>
                      <td className="cell-action">
                        <div className="button-row button-row-end">
                          {instance.web_ui && running && (
                            <button
                              type="button"
                              className="button button-small"
                              onClick={() => instance.web_ui && onOpenUrl(instance.web_ui)}
                            >
                              <ExternalLink size={13} /> {t.common.open}
                            </button>
                          )}
                          <button
                            type="button"
                            className="button button-small"
                            disabled={busy === instance.id || installing}
                            onClick={() =>
                              act(instance.id, () => (running ? stopService(instance.id) : startService(instance.id)))
                            }
                          >
                            {running ? <Square size={12} /> : <Play size={12} />}
                            {running ? t.services.stop : t.services.start}
                          </button>
                          <button
                            type="button"
                            className="icon-button"
                            title={t.services.rename}
                            aria-label={`${t.services.rename} ${instance.name}`}
                            onClick={() => setRenaming(instance)}
                          >
                            <Pencil size={15} />
                          </button>
                          <button
                            type="button"
                            className="icon-button"
                            title={t.services.details}
                            aria-label={`${t.services.details} ${instance.name}`}
                            onClick={() => void serviceDetails(instance.id).then(setDetails).catch(onError)}
                          >
                            <Info size={15} />
                          </button>
                          <button
                            type="button"
                            className="icon-button"
                            title={t.services.remove}
                            aria-label={`${t.services.remove} ${instance.name}`}
                            onClick={() => setRemoving(instance)}
                          >
                            <Trash2 size={15} />
                          </button>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            {instances
              .filter((instance) => instance.error)
              .map((instance) => (
                <div key={instance.id} className="callout callout-error">
                  <strong>{instance.name}:</strong> {instance.error}
                </div>
              ))}
          </Section>

          {binaries.length > 0 && (
            <Section title={t.services.versions} description={t.services.versionsHint}>
              <RuntimeTable rows={binaries} runtimes={runtimes} />
            </Section>
          )}
        </div>
      )}

      {adding && (
        <AddServiceModal
          onClose={() => setAdding(false)}
          onError={onError}
          onCreated={async (job) => {
            setAdding(false);
            if (job) runtimes.track(job);
            await onChanged();
          }}
        />
      )}
      {details && <DetailsModal details={details} onClose={() => setDetails(null)} />}
      {renaming && (
        <RenameModal
          instance={renaming}
          onClose={() => setRenaming(null)}
          onConfirm={(name) => {
            const id = renaming.id;
            setRenaming(null);
            act(id, () => renameService(id, name));
          }}
        />
      )}
      {removing && (
        <RemoveModal
          instance={removing}
          onClose={() => setRemoving(null)}
          onConfirm={(keepData) => {
            const id = removing.id;
            setRemoving(null);
            act(id, () => deleteService(id, keepData));
          }}
        />
      )}
    </>
  );
}

function AddServiceModal({
  onClose,
  onCreated,
  onError,
}: {
  onClose: () => void;
  onCreated: (job: Job | null) => Promise<void>;
  onError: (cause: unknown) => void;
}) {
  const t = useT();
  const [offerings, setOfferings] = useState<ServiceOffering[] | null>(null);
  const [category, setCategory] = useState<string>("database");
  const [choice, setChoice] = useState("");
  const [onlyLatest, setOnlyLatest] = useState(true);
  const [name, setName] = useState("");
  const [port, setPort] = useState("");
  const [autostart, setAutostart] = useState(true);
  const [pgvector, setPgvector] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    void serviceCatalog().then(setOfferings).catch(onError);
  }, [onError]);

  const options = useMemo(
    () =>
      (offerings ?? [])
        .filter((offering) => offering.categories.includes(category))
        .flatMap((offering) =>
          (onlyLatest ? offering.lines.slice(0, 1) : offering.lines).map((line) => ({
            key: `${offering.product}@${line.line}`,
            offering,
            line,
          })),
        ),
    [offerings, category, onlyLatest],
  );
  const selected = options.find((option) => option.key === choice) ?? options[0];
  const categories = CATEGORIES.filter((candidate) =>
    (offerings ?? []).some((offering) => offering.categories.includes(candidate)),
  );

  const submit = () => {
    if (!selected) return;
    const service: NewService = {
      product: selected.offering.product,
      line: selected.line.line,
      name: name.trim() || undefined,
      port: port ? Number(port) : undefined,
      autostart,
      extensions: pgvector && selected.offering.extensions.includes("pgvector") ? ["pgvector"] : [],
    };
    setSaving(true);
    void createService(service)
      .then(({ job }) => onCreated(job))
      .catch(onError)
      .finally(() => setSaving(false));
  };

  return (
    <Modal title={t.services.addTitle} onClose={onClose}>
      {offerings === null ? (
        <p className="muted">{t.services.loading}</p>
      ) : (
        <form
          className="form-grid"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <label htmlFor="service-category">{t.services.category}</label>
          <select
            id="service-category"
            className="select"
            value={category}
            onChange={(event) => {
              setCategory(event.target.value);
              setChoice("");
            }}
          >
            {categories.map((candidate) => (
              <option key={candidate} value={candidate}>
                {t.services.categories[candidate]}
              </option>
            ))}
          </select>

          <label htmlFor="service-product">{t.services.service}</label>
          <div className="stack">
            <select
              id="service-product"
              className="select"
              value={selected?.key ?? ""}
              onChange={(event) => setChoice(event.target.value)}
            >
              {options.map((option) => (
                <option key={option.key} value={option.key}>
                  {option.offering.label} ({option.line.latest}){option.line.installed ? " ✓" : ""}
                </option>
              ))}
            </select>
            <label className="checkbox">
              <input type="checkbox" checked={onlyLatest} onChange={(event) => setOnlyLatest(event.target.checked)} />
              {t.services.onlyLatest}
            </label>
          </div>

          <label htmlFor="service-name">{t.services.name}</label>
          <input
            id="service-name"
            className="input"
            value={name}
            placeholder={selected ? `${selected.offering.label} ${selected.line.line}` : ""}
            onChange={(event) => setName(event.target.value)}
          />

          <label htmlFor="service-port">{t.services.port}</label>
          <input
            id="service-port"
            className="input"
            type="number"
            min={1}
            max={65535}
            value={port}
            placeholder={selected?.offering.default_port ? t.services.portAuto(selected.offering.default_port) : ""}
            onChange={(event) => setPort(event.target.value)}
          />

          <span />
          <div className="stack">
            {selected?.offering.extensions.includes("pgvector") && (
              <label className="checkbox">
                <input type="checkbox" checked={pgvector} onChange={(event) => setPgvector(event.target.checked)} />
                {t.services.withPgvector}
              </label>
            )}
            <label className="checkbox">
              <input type="checkbox" checked={autostart} onChange={(event) => setAutostart(event.target.checked)} />
              {t.services.autostartLong}
            </label>
            {selected && !selected.line.installed && <p className="muted">{t.services.willDownload}</p>}
          </div>

          <div className="modal-actions form-actions">
            <button type="button" className="button" onClick={onClose}>
              {t.common.cancel}
            </button>
            <button type="submit" className="button button-primary" disabled={!selected || saving}>
              {t.services.create}
            </button>
          </div>
        </form>
      )}
    </Modal>
  );
}

function DetailsModal({ details, onClose }: { details: ServiceDetails; onClose: () => void }) {
  const t = useT();
  return (
    <Modal title={details.instance.name} onClose={onClose}>
      <dl className="fields">
        <dt>{t.services.port}</dt>
        <dd className="mono">{details.instance.port}</dd>
        {details.web_ui && (
          <>
            <dt>{t.services.webUi}</dt>
            <dd className="mono">{details.web_ui}</dd>
          </>
        )}
        {details.credentials && (
          <>
            <dt>{t.services.username}</dt>
            <dd className="mono">{details.credentials.username}</dd>
            <dt>{t.services.password}</dt>
            <dd className="mono">
              {details.credentials.password} <CopyButton text={details.credentials.password} />
            </dd>
          </>
        )}
      </dl>
      <div className="section-head env-head">
        <h2>{t.sites.envTitle}</h2>
        <CopyButton text={details.env} />
      </div>
      <pre className="code">{details.env}</pre>
    </Modal>
  );
}

function RemoveModal({
  instance,
  onClose,
  onConfirm,
}: {
  instance: ServiceInstance;
  onClose: () => void;
  onConfirm: (keepData: boolean) => void;
}) {
  const t = useT();
  const [keepData, setKeepData] = useState(false);
  return (
    <Modal title={t.services.removeTitle(instance.name)} onClose={onClose}>
      <p className="muted">{t.services.removeHint}</p>
      <label className="checkbox remove-keep">
        <input type="checkbox" checked={keepData} onChange={(event) => setKeepData(event.target.checked)} />
        {t.services.keepData}
      </label>
      <div className="modal-actions">
        <button type="button" className="button" onClick={onClose}>
          {t.common.cancel}
        </button>
        <button type="button" className="button button-danger" onClick={() => onConfirm(keepData)}>
          {t.services.remove}
        </button>
      </div>
    </Modal>
  );
}

function RenameModal({
  instance,
  onClose,
  onConfirm,
}: {
  instance: ServiceInstance;
  onClose: () => void;
  onConfirm: (name: string) => void;
}) {
  const t = useT();
  const [name, setName] = useState(instance.name);
  const valid = name.trim().length > 0 && name.trim().length <= 80;
  return (
    <Modal title={t.services.renameTitle(instance.name)} onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (valid) onConfirm(name.trim());
        }}
      >
        <label className="field">
          <span>{t.services.name}</span>
          <input className="input" value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <div className="modal-actions">
          <button type="button" className="button" onClick={onClose}>
            {t.common.cancel}
          </button>
          <button type="submit" className="button button-primary" disabled={!valid || name.trim() === instance.name}>
            {t.services.rename}
          </button>
        </div>
      </form>
    </Modal>
  );
}
