// The WPF launcher shell (index.html .app-shell, sidebar-icons.js, theme.js),
// rendered by React with the same markup and class names.
import React, { useEffect, useRef } from "react";

// renderer/sidebar-icons.js shapes, verbatim.
export const wpfIconShapes: Record<string, string[]> = {
  play: ["M8 5l11 7-11 7z"],
  mods: ["M3 7l9-4 9 4-9 4z", "M3 12l9 4 9-4", "M3 17l9 4 9-4"],
  jukebox: ["M9 18V6l11-2v12", "M9 6l11-2v4L9 10", "M9 18c0 2-6 3-6 0s6-3 6 0", "M20 16c0 2-6 3-6 0s6-3 6 0"],
  presentation: ["M3 4h18v14H3z", "M10 8l5 3-5 3z", "M12 18v3", "M8 21h8"],
  profiles: ["M3 7V5h7l2 3h9v12H3z"],
  players: ["M15.5 7a3.5 3.5 0 1 1-7 0 3.5 3.5 0 0 1 7 0", "M5 21v-3a7 7 0 0 1 14 0v3"],
  teams: ["M11 8a3 3 0 1 1-6 0 3 3 0 0 1 6 0", "M2 20v-3a6 6 0 0 1 12 0v3", "M16 5a3 3 0 0 1 0 6", "M17 14a5 5 0 0 1 5 5v1"],
  blacktop: ["M3 4h18v16H3z", "M12 4v16", "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0", "M3 9h3v6H3", "M21 9h-3v6h3"],
  crown: ["M2 7l5 5 5-8 5 8 5-5-2 13H4z", "M4 20h16", "M7 4v2", "M17 4v2"],
  cameras: ["M3 7h12v12H3z", "M15 11l6-4v12l-6-4z", "M6 4h6"],
  scaler: ["M4 8V4h4", "M16 4h4v4", "M20 16v4h-4", "M8 20H4v-4", "M8 16l8-8", "M11 8h5v5"],
  unlocks: ["M7 11V7a5 5 0 0 1 9-3", "M4 11h16v10H4z", "M12 15v3"],
  settings: ["M4 6h16", "M4 12h16", "M4 18h16", "M8 3v6", "M16 9v6", "M10 15v6"],
  developer: ["M8 6l-6 6 6 6", "M16 6l6 6-6 6", "M14 3l-4 18"],
  book: ["M12 5v16", "M12 5C8 2 4 3 2 4v15c3-1 6-1 10 2", "M12 5c4-3 8-2 10-1v15c-3-1-6-1-10 2"],
  tag: ["M3 3h8l10 10-8 8L3 11z", "M7 7h.01"],
  plus: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18", "M12 8v8", "M8 12h8"],
  pencil: ["M15 4l5 5", "M3 21l1-6L16 3a2 2 0 0 1 5 5L9 20z", "M4 15l5 5"],
  clock: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18", "M12 7v5l3 2"],
  file: ["M5 3h9l5 5v13H5z", "M14 3v6h5", "M8 13h8", "M8 17h6"],
  download: ["M12 3v12", "M7 10l5 5 5-5", "M4 15v6h16v-6"],
  refresh: ["M20 7a9 9 0 1 0 1 8", "M20 2v5h-5"],
  search: ["M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0", "M15 15l6 6"],
  uniform: ["M8 3l4 2 4-2 6 5-4 4-2-2v11H8V10l-2 2-4-4z"],
  ball: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18", "M3 12h18", "M12 3v18", "M5 6c7 2 7 10 0 12", "M19 6c-7 2-7 10 0 12"],
  monitor: ["M3 4h18v14H3z", "M12 18v3", "M8 21h8"],
  drone: ["M8 9h8v6H8z", "M4 5l5 4", "M20 5l-5 4", "M4 19l5-4", "M20 19l-5-4", "M2 5h4", "M18 5h4", "M2 19h4", "M18 19h4"],
  microphone: ["M9 4a3 3 0 0 1 6 0v8a3 3 0 0 1-6 0z", "M6 10v2a6 6 0 0 0 12 0v-2", "M12 18v4", "M8 22h8"],
  shuffle: ["M3 7h3c5 0 7 10 12 10h3", "M17 14l4 3-4 3", "M3 17h3c2 0 3-1 4-3", "M17 4l4 3-4 3", "M15 7h6"],
  check: ["M3 12l6 6L21 5"],
  hide: ["M3 3l18 18", "M10.6 5.1A10 10 0 0 1 12 5c5.5 0 9 7 9 7a17 17 0 0 1-3.2 3.5", "M6.2 6.2A17 17 0 0 0 3 12s3.5 7 9 7c1 0 2-.2 2.9-.6", "M10 10a3 3 0 0 0 4 4"],
};

/** createLauncherIcon(name): `<svg class="nav-icon">`. */
export function UiIcon({ name, className }: { name: string; className?: string }) {
  return (
    <svg
      className={className ? `nav-icon ${className}` : "nav-icon"}
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      {(wpfIconShapes[name] ?? wpfIconShapes.file).map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}

/** Rust page label -> WPF data-page id (page-<id>). */
export const wpfPageIds: Record<string, string> = {
  "Play session": "play",
  Profiles: "profiles",
  "Mod library": "mods",
  Jukebox: "jukebox",
  Presentation: "presentation",
  "Roster Editor": "players",
  "Team Editor": "teams",
  Blacktop: "blacktop",
  Unlocks: "unlocks",
  Scaler: "scaler",
  Settings: "settings",
};
const navLabels: Record<string, string> = {
  "Roster Editor": "Roster Editor ↗",
  "Team Editor": "Team Editor ↗",
};
// app.js nav click: $('pageTitle') text per page.
const wpfTitles: Record<string, string> = {
  play: "2K Launcher",
  mods: "Mod library",
  jukebox: "Jukebox",
  presentation: "Presentation",
  profiles: "Profiles",
  players: "Player Editor",
  teams: "Team Editor",
  unlocks: "Unlocks",
  scaler: "Scaler",
  blacktop: "Blacktop",
  settings: "Launcher settings.",
};

export function WpfSidebar({
  pages,
  page,
  onPage,
  providerReady,
  providerLabel,
  theme,
  onTheme,
  onHelp,
}: {
  pages: string[];
  page: string;
  onPage: (p: string) => void;
  providerReady: boolean;
  providerLabel: string;
  theme: "dark" | "light";
  onTheme: (t: "dark" | "light") => void;
  onHelp: () => void;
}) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <img className="brand-mark" src="./brand.png" alt="" />
        <div>
          <strong>2K Launcher</strong>
          <small>NBA 2K27</small>
        </div>
      </div>
      <nav>
        {pages.map((p) => (
          <React.Fragment key={p}>
            {p === "Roster Editor" && <hr className="nav-divider" aria-hidden="true" />}
            <button
              className={page === p ? "nav-item active" : "nav-item"}
              data-page={wpfPageIds[p]}
              onClick={() => onPage(p)}
            >
              <UiIcon name={wpfPageIds[p]} />
              {navLabels[p] ?? p}
            </button>
          </React.Fragment>
        ))}
      </nav>
      <div className="sidebar-foot">
        <button
          type="button"
          className="launcher-help-link"
          aria-haspopup="dialog"
          aria-controls="launcherHelpDialog"
          onClick={onHelp}
        >
          Help
        </button>
        <span className={providerReady ? "status-dot ready" : "status-dot"} id="providerDot" />
        <div>
          <small>ASSET PROVIDER</small>
          <strong id="providerLabel">{providerLabel}</strong>
        </div>
      </div>
      <div className="theme-switch" role="group" aria-label="Appearance">
        <button
          type="button"
          className="theme-choice"
          aria-label="Light theme"
          aria-pressed={theme === "light"}
          onClick={() => onTheme("light")}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5" />
          </svg>
          <span>Light</span>
        </button>
        <button
          type="button"
          className="theme-choice"
          aria-label="Dark theme"
          aria-pressed={theme === "dark"}
          onClick={() => onTheme("dark")}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M20 15a9 9 0 0 1-11-11 9 9 0 1 0 11 11Z" />
          </svg>
          <span>Dark</span>
        </button>
      </div>
    </aside>
  );
}

export function WpfHeader({ page, version }: { page: string; version: string }) {
  const id = wpfPageIds[page];
  return (
    <header>
      <div>
        <p className="eyebrow">NBA 2K27</p>
        <h1 id="pageTitle" data-ui-icon="play">
          {id === "play" && <UiIcon name="play" />}
          {wpfTitles[id] ?? page}
        </h1>
        <p className="home-intro">Configure your session, load your content, and jump into NBA 2K27.</p>
        <p className="profiles-intro">Keep separate mod collections and choose one for Play.</p>
        <p className="mods-intro">Search, tag, and organize the files in your mod profile.</p>
        <p className="jukebox-intro">Build, manage, and preview your custom NBA 2K27 soundtrack.</p>
        <p className="presentation-intro">Manage cameras and startup videos for your next session.</p>
        <p className="blacktop-intro">Customize your streetball setup, presentation, and outfits.</p>
        <p className="unlocks-intro">Enable the supported team options and session-only debug menus.</p>
      </div>
      <span className="version" id="version">
        v{version}
      </span>
    </header>
  );
}

/** index.html #launcherHelpDialog. */
export function WpfHelpDialog({
  open,
  onClose,
  onDiagnostics,
}: {
  open: boolean;
  onClose: () => void;
  onDiagnostics: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) d.showModal?.();
    if (!open && d.open) d.close();
  }, [open]);
  return (
    <dialog
      ref={ref}
      id="launcherHelpDialog"
      className="launcher-help-dialog"
      aria-labelledby="launcherHelpTitle"
      onClose={onClose}
    >
      <div className="launcher-help-header">
        <h2 id="launcherHelpTitle">What Play does</h2>
        <button type="button" className="secondary small" autoFocus onClick={onClose}>
          Close
        </button>
      </div>
      <div className="boundary">
        {[
          ["Loads your selected mod profile", "The profile checked under Profiles becomes the game's mods folder for this session."],
          ["Loads unique assets", "Local players and arenas that are missing from the stock manifest can load automatically."],
          ["Starts your saved cameras", "Your selected camera unlocks and settings activate after the game starts."],
          ["Cleans up when you quit", "The temporary loader and profile redirect are removed after NBA 2K27 closes."],
        ].map(([title, text]) => (
          <div key={title}>
            <span className="check">✓</span>
            <p>
              <strong>{title}</strong>
              <small>{text}</small>
            </p>
          </div>
        ))}
      </div>
      <div className="launcher-help-footer">
        <p>For current loader, game and camera progress, open Session diagnostics.</p>
        <button type="button" className="secondary" onClick={onDiagnostics}>
          Session diagnostics
        </button>
      </div>
    </dialog>
  );
}

/** index.html #toast: app.js toast() shows a message for 5 s. */
export function WpfToast({ text, error = false }: { text: string; error?: boolean }) {
  const [shown, setShown] = React.useState(false);
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    if (!text) return;
    setShown(true);
    const t = setTimeout(() => setShown(false), 5000);
    return () => clearTimeout(t);
  }, [text]);
  return (
    <div
      className={`toast${shown ? "" : " hidden"}${error ? " error" : ""}`}
      id="toast"
      role="status"
    >
      {text}
    </div>
  );
}

/** A WPF-style `<dialog>` opened with showModal() while mounted. Esc calls onCancel. */
export function ModalDialog({
  id,
  className,
  labelledBy,
  onCancel,
  children,
}: {
  id?: string;
  className: string;
  labelledBy?: string;
  onCancel: () => void;
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (d && !d.open) {
      if (typeof d.showModal === "function") d.showModal();
      else d.setAttribute("open", "");
    }
  }, []);
  return (
    <dialog
      ref={ref}
      id={id}
      className={className}
      aria-labelledby={labelledBy}
      onCancel={(e) => {
        e.preventDefault();
        onCancel();
      }}
      onMouseDown={(e) => {
        // Backdrop clicks land on the dialog itself, outside its box.
        const r = e.currentTarget.getBoundingClientRect();
        if (
          e.target === e.currentTarget &&
          (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom)
        )
          onCancel();
      }}
    >
      {children}
    </dialog>
  );
}

/** accessory-controls.js icon(kind): `<svg class="outfit-item-icon">`. */
export function OutfitItemIcon({ d }: { d: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className="outfit-item-icon">
      <path
        d={d}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  );
}
