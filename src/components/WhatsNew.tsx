import { type ReactNode, useState } from "react";
import { useT } from "../i18n";
import { Modal } from "../ui";

const KEY = "werd.pendingNotes";

/** Saves the notes of the update being installed, to show them after the restart. */
export function rememberNotes(version: string, notes: string | null) {
  try {
    localStorage.setItem(KEY, JSON.stringify({ version, notes: notes ?? "" }));
  } catch {
    // Only a convenience.
  }
}

/** The notes saved before updating, once the app runs that version. */
function pendingNotes(): { version: string; notes: string } | null {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (saved?.version !== __APP_VERSION__) return null;
    return saved;
  } catch {
    return null;
  }
}

function inline(text: string): ReactNode[] {
  // **bold** and `code`, the only inline marks git-cliff produces.
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).map((part, index) => {
    // biome-ignore lint/suspicious/noArrayIndexKey: parts of one static line
    if (part.startsWith("**")) return <strong key={index}>{part.slice(2, -2)}</strong>;
    // biome-ignore lint/suspicious/noArrayIndexKey: parts of one static line
    if (part.startsWith("`")) return <code key={index}>{part.slice(1, -1)}</code>;
    return part;
  });
}

/** Renders the small markdown subset of the changelog: headings, lists, paragraphs. */
export function Notes({ markdown }: { markdown: string }) {
  const blocks: ReactNode[] = [];
  let items: string[] = [];
  const flush = () => {
    if (items.length) {
      blocks.push(
        <ul key={`list-${blocks.length}`}>
          {items.map((item) => (
            <li key={item}>{inline(item)}</li>
          ))}
        </ul>,
      );
      items = [];
    }
  };
  for (const raw of markdown.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith("- ") || line.startsWith("* ")) {
      items.push(line.slice(2));
      continue;
    }
    flush();
    if (line.startsWith("#")) {
      const text = line.replace(/^#+\s*/, "");
      // The version heading is already the modal title.
      if (!/^\[?\d+\.\d+/.test(text)) blocks.push(<h3 key={`h-${blocks.length}`}>{inline(text)}</h3>);
    } else {
      blocks.push(<p key={`p-${blocks.length}`}>{inline(line)}</p>);
    }
  }
  flush();
  return <div className="notes">{blocks}</div>;
}

/** "What's new" shown once after an update, with the notes saved before it. */
export function WhatsNew() {
  const t = useT();
  const [saved, setSaved] = useState(pendingNotes);
  if (!saved) return null;
  const close = () => {
    try {
      localStorage.removeItem(KEY);
    } catch {
      // Nothing to clean up.
    }
    setSaved(null);
  };
  return (
    <Modal title={t.update.whatsNew(saved.version)} onClose={close} wide>
      {saved.notes.trim() ? <Notes markdown={saved.notes} /> : <p className="muted">{t.update.noNotes}</p>}
      <div className="modal-actions">
        <button type="button" className="button button-primary" onClick={close}>
          {t.update.gotIt}
        </button>
      </div>
    </Modal>
  );
}
