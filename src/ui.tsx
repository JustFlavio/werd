import { Check, Copy, X } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import type { ProjectStatus } from "./api";
import { useT } from "./i18n";

export function PageHeader({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <header className="page-header">
      <h1>{title}</h1>
      {children && <div className="page-header-actions">{children}</div>}
    </header>
  );
}

export function Section({
  title,
  description,
  action,
  children,
}: {
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <section className="section">
      <div className="section-head">
        <div>
          <h2>{title}</h2>
          {description && <p className="section-description">{description}</p>}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

export function StatusDot({ status, label }: { status: ProjectStatus | "ok" | "fail"; label?: boolean }) {
  const t = useT();
  const tone =
    status === "running" || status === "ok"
      ? "ok"
      : status === "error" || status === "fail"
        ? "fail"
        : status === "starting"
          ? "warn"
          : "off";
  return (
    <span className={`status status-${tone}`}>
      <i aria-hidden />
      {label && status !== "ok" && status !== "fail" && t.status[status]}
    </span>
  );
}

export function EmptyState({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty">
      <h2>{title}</h2>
      <p>{description}</p>
      {children}
    </div>
  );
}

export function CopyButton({ text }: { text: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="button"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? <Check size={14} /> : <Copy size={14} />}
      {copied ? t.common.copied : t.common.copy}
    </button>
  );
}

export function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const t = useT();
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop click is a mouse shortcut; keyboard users close with Escape (handled above).
    <div
      className="modal-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-modal aria-label={title}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button type="button" className="icon-button" aria-label={t.common.close} onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}
