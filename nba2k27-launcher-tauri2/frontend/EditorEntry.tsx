import { useRef, useState, type ReactNode } from "react";
import { call } from "./api";

export function EditorEntry({
  section,
  onError,
}: {
  section: "players" | "teams";
  onError: (message: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const name = section === "players" ? "roster" : "team";
  async function open(preview: boolean) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      await call("open-browser-editor", { section, preview });
    } catch (error) {
      onError(String(error));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <article
      className="card settings-path-card editor-entry"
      data-editor-entry={section}
    >
      <div className="settings-card-heading">
        <strong>{section === "players" ? "Roster editor" : "Team editor"}</strong>
        <span className="settings-badge">Opens in your browser</span>
      </div>
      <div className="card-content">
        <p className="settings-description">
          Open the familiar editor in your browser. Keep the launcher open while
          editing.
        </p>
        <div className="settings-support-actions">
          <button
            className="primary"
            data-editor-open
            disabled={busy}
            onClick={() => void open(false)}
          >
            Open {name} editor in browser ↗
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void open(true)}
          >
            Preview with sample data ↗
          </button>
        </div>
        <p className="settings-description compact-note">
          Changes are reviewed before Save &amp; verify. Sample previews cannot
          read or change the game.
          {section === "teams"
            ? " Basic team and uniform editing is connected; staff and rotation writes are not yet available."
            : " Player editing is connected; remaining services are still being integrated."}{" "}
          Live gameplay acceptance is pending.
        </p>
      </div>
    </article>
  );
}

export function LocalEditorTools({ children }: { children: ReactNode }) {
  return (
    <details
      className="presentation-section section-disclosure local-editor-tools"
      data-local-editor-tools
    >
      <summary>
        Local DNA tools{" "}
        <small>
          Drafts, catalogs and sharing · separate from the game editor
        </small>
      </summary>
      <div className="disclosure-content">
        <p className="notice">
          These tools save local files only. They do not apply changes to your
          game roster.
        </p>
        {children}
      </div>
    </details>
  );
}
