import { Download, RotateCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { checkUpdate, downloadUpdate, installUpdate, onCheckUpdateRequest, type UpdateInfo } from "../api";
import { useT } from "../i18n";
import { rememberNotes } from "./WhatsNew";

type Phase = "idle" | "available" | "downloading" | "ready" | "installing";

const CHECK_EVERY_MS = 6 * 60 * 60 * 1000;
const RING = 2 * Math.PI * 10;

/**
 * The update control at the right of the version in the sidebar footer.
 * A dot that widens on hover to "Update available"; clicking turns it into a
 * ring that fills while downloading, then it offers Restart.
 */
export function UpdateButton({ onError, onUpToDate }: { onError: (cause: unknown) => void; onUpToDate: () => void }) {
  const t = useT();
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState(0);
  // True from the download until Restart: checking then would add nothing.
  const busy = useRef(false);
  // The callbacks change on every render of App; keep the latest without
  // re-running the effect, which would check for updates every few seconds.
  const callbacks = useRef({ onError, onUpToDate });
  callbacks.current = { onError, onUpToDate };

  const check = useCallback(async (manual: boolean) => {
    if (busy.current) return;
    try {
      const found = await checkUpdate();
      if (found) {
        setUpdate(found);
        setPhase((current) => (current === "idle" ? "available" : current));
      } else if (manual) {
        callbacks.current.onUpToDate();
      }
    } catch (cause) {
      if (manual) callbacks.current.onError(cause);
    }
  }, []);

  useEffect(() => {
    void check(false);
    const timer = window.setInterval(() => void check(false), CHECK_EVERY_MS);
    const stop = onCheckUpdateRequest(() => void check(true));
    return () => {
      window.clearInterval(timer);
      stop();
    };
  }, [check]);

  async function download() {
    busy.current = true;
    setPhase("downloading");
    setProgress(0);
    try {
      await downloadUpdate(({ downloaded, total }) => setProgress(total ? downloaded / total : 0));
      setProgress(1);
      setPhase("ready");
    } catch (cause) {
      busy.current = false;
      setPhase("available");
      onError(cause);
    }
  }

  async function install() {
    if (!update) return;
    setPhase("installing");
    rememberNotes(update.version, update.notes);
    try {
      await installUpdate();
    } catch (cause) {
      setPhase("ready");
      onError(cause);
    }
  }

  if (phase === "idle" || !update) return null;

  const label =
    phase === "available"
      ? t.update.available(update.version)
      : phase === "ready"
        ? t.update.restart
        : phase === "installing"
          ? t.update.installing
          : t.update.downloading(Math.round(progress * 100));
  const wide = phase === "ready" || phase === "installing";

  return (
    <button
      type="button"
      className={`update-button update-${phase} ${wide ? "update-wide" : ""}`}
      aria-label={label}
      title={label}
      disabled={phase === "downloading" || phase === "installing"}
      onClick={() => void (phase === "available" ? download() : phase === "ready" ? install() : undefined)}
    >
      <span className="update-icon" aria-hidden>
        {phase === "downloading" ? (
          <svg viewBox="0 0 24 24" className="update-ring" aria-hidden="true">
            <circle cx="12" cy="12" r="10" className="update-ring-track" />
            <circle
              cx="12"
              cy="12"
              r="10"
              className="update-ring-fill"
              strokeDasharray={RING}
              strokeDashoffset={RING * (1 - progress)}
            />
          </svg>
        ) : null}
        {phase === "ready" || phase === "installing" ? <RotateCw size={13} /> : <Download size={13} />}
      </span>
      <span className="update-label">{phase === "ready" || phase === "installing" ? label : t.update.short}</span>
    </button>
  );
}
