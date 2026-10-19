// WPF Settings cards ported from renderer\index.html:455-489,
// renderer\launcher-updates.js, renderer\launcher-quality.js and the hidden
// Developer diagnostics page (index.html:495-537, Ctrl+Shift+D).
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { call } from "./api";

type Status = (text: string) => void;

function ToolCard({
  title,
  action,
  className,
  children,
}: {
  title: string;
  action?: React.ReactNode;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <article
      className={`card settings-path-card settings-tool ${className ?? ""}`}
    >
      <div className="settings-card-heading">
        <strong>{title}</strong>
        {action}
      </div>
      <div className="card-content">{children}</div>
    </article>
  );
}
const message = (e: unknown) =>
  String(e instanceof Error ? e.message : e).replace(/^Error:\s*/, "");

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
}

// ---------- Launcher updates (launcher-updates.js) ----------

type BuildInfo = {
  version: string;
  channel: string;
  builtAt: string;
  notes: string[];
  previous: { version: string; channel: string } | null;
  portable: boolean;
  pending: { version: string; channel: string; notes: string[]; rollback: boolean } | null;
  busy: boolean;
  recoveryNeeded: boolean;
  lastResult: { ok: boolean; message: string; at: string } | null;
};

export function LauncherUpdatesCard({ setStatus }: { setStatus: Status }) {
  const [info, setInfo] = useState<BuildInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState("");
  const [refreshError, setRefreshError] = useState("");
  const refresh = () =>
    call<BuildInfo>("launcher-tools-build-info")
      .then((next) => { setInfo(next); setRefreshError(""); })
      .catch((error) => setRefreshError(message(error)));
  useEffect(() => {
    void refresh();
    const off = listen<{ message: string }>("launcher-update-progress", (e) =>
      setText(e.payload.message),
    );
    return () => void off.then((f) => f());
  }, []);
  if (!info) return <ToolCard title="Launcher updates"><p role="status">{refreshError || "Loading update information…"}</p><button onClick={() => void refresh()}>Retry</button></ToolCard>;
  const pending = info.pending;
  const idleText =
    text || (info.lastResult ? info.lastResult.message : "");
  async function run(fn: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      setText(message(e));
      setStatus(message(e));
    } finally {
      setBusy(false);
      void refresh();
    }
  }
  return (
    <ToolCard
      title="Launcher updates"
      className="launcher-update-card"
      action={
        <span className="settings-badge" id="launcherBuildLabel">
          {info.version} · {info.channel}
        </span>
      }
    >
      {refreshError && <p role="status">Update information could not be refreshed: {refreshError}</p>}
      {info.builtAt && (
        <p className="settings-description" id="launcherBuildDate">
          Built {new Date(info.builtAt).toLocaleDateString()}
        </p>
      )}
      <details>
        <summary>What changed</summary>
        <ul className="launcher-notes">
          {info.notes.map((n, i) => (
            <li key={i}>{n}</li>
          ))}
        </ul>
      </details>
      <p className="settings-description">
        Extract your downloaded launcher release, then choose its folder to
        update. The previous version is kept for rollback.
      </p>
      <div className="settings-support-actions">
        <button className="secondary"
          disabled={busy || !!pending || info.busy}
          onClick={() =>
            void run(async () => {
              setText("Checking update…");
              const r = await call<{ canceled?: boolean; message: string }>(
                "launcher-tools-update-choose",
              );
              setText(r.message);
            })
          }
        >
          Install update folder
        </button>
        <button className="secondary"
          disabled={busy || !!pending || info.busy || !info.previous}
          onClick={() =>
            void run(async () => {
              setText("Checking update…");
              const r = await call<{ message: string }>(
                "launcher-tools-update-rollback",
              );
              setText(r.message);
            })
          }
        >
          {info.previous
            ? `Restore ${info.previous.version}`
            : "Restore previous version"}
        </button>
      </div>
      {busy && <progress className="launcher-update-progress" />}
      {pending && (
        <div className="launcher-update-review" role="group">
          <strong>
            {pending.rollback ? "Restore" : "Install"} {pending.version} ·{" "}
            {pending.channel}
          </strong>
          {pending.notes.length > 0 && (
            <ul className="launcher-notes">
              {pending.notes.map((n, i) => (
                <li key={i}>{n}</li>
              ))}
            </ul>
          )}
          <div className="settings-support-actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  setText(
                    "Finishing session cleanup, then installing and restarting…",
                  );
                  await call("launcher-tools-update-restart");
                })
              }
            >
              Install and restart
            </button>
            <button className="secondary"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  const r = await call<{ message: string }>(
                    "launcher-tools-update-cancel",
                  );
                  setText(r.message);
                })
              }
            >
              Cancel
            </button>
          </div>
        </div>
      )}
      {info.recoveryNeeded && (
        <p className="settings-description" role="alert">
          An interrupted update needs recovery. Use
          Recovery/Updates/Runner/Recover launcher.cmd.
        </p>
      )}
      <p className="settings-description" role="status">
        {idleText}
      </p>
    </ToolCard>
  );
}

// ---------- Diagnostics & compatibility (launcher-quality.js) ----------

type Helper = {
  key: string;
  label: string;
  status: string;
  phase: string;
  message: string;
  updatedAt: string | null;
};
type Perf = {
  name: string;
  lane: string;
  count: number;
  lastMs: number | null;
  medianMs: number | null;
  baselineMs: number | null;
  regression: boolean;
};
type Health = {
  helpers: Helper[];
  performance: Perf[];
  compatibility: {
    build: string | null;
    buildLabel: string | null;
    known: boolean;
    message: string;
    features: { key: string; label: string; supported: boolean; reason: string }[];
  };
};
const PERF_LABELS: Record<string, string> = {
  startup: "Host startup",
  shutdown: "Shutdown cleanup",
  "catalog.ball": "Ball catalog",
  "catalog.scoreboard": "Scoreboard catalog",
  "catalog.arena": "Arena catalog",
  "catalog.outfit": "Outfit catalog",
  "catalog.rules": "Rules pane",
};
const ms = (v: number | null) => (v == null ? "—" : `${Math.round(v)} ms`);

export function LauncherHealthCard({ setStatus }: { setStatus: Status }) {
  const [health, setHealth] = useState<Health | null>(null);
  const [error, setError] = useState("");
  const refresh = () =>
    call<Health>("launcher-tools-health")
      .then((h) => {
        setHealth(h);
        setError("");
      })
      .catch((e) => setError(message(e)));
  useEffect(() => {
    void refresh();
    // WPF: on load and every 5 s while Settings is the active page.
    const timer = setInterval(() => void refresh(), 5000);
    return () => clearInterval(timer);
  }, []);
  const c = health?.compatibility;
  return (
    <ToolCard
      title="Diagnostics & compatibility"
      className="launcher-health-card"
      action={<button className="secondary" onClick={() => void refresh()}>Refresh</button>}
    >
      <p id="launcherCompatibilitySummary">
        {c
          ? `${c.buildLabel ?? c.build ?? "No game build"} · ${c.message}`
          : error || "Checking…"}
      </p>
      {c && c.features.length > 0 && (
        <details>
          <summary>Feature compatibility</summary>
          <table className="health-table">
            <thead>
              <tr>
                <th>Feature</th>
                <th>Support</th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {c.features.map((f) => (
                <tr key={f.key}>
                  <td>{f.label}</td>
                  <td>{f.supported ? "Recognized" : "Unavailable"}</td>
                  <td>{f.reason}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
      {health && (
        <table className="health-table">
          <thead>
            <tr>
              <th>Helper</th>
              <th>State</th>
              <th>Latest report</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {health.helpers.map((h) => (
              <tr key={h.key} data-state={h.status.toLowerCase()}>
                <td>{h.label}</td>
                <td>{h.status}</td>
                <td>{h.message || h.phase}</td>
                <td>
                  {h.updatedAt
                    ? new Date(h.updatedAt).toLocaleTimeString()
                    : "Not observed"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="settings-description">
        Latest reported helper states, with timestamps—not a guarantee that a
        pending effect is already visible in-game.
      </p>
      {health && (
        <details>
          <summary>Performance measurements</summary>
          <p className="settings-description">
            Host startup, shutdown cleanup, and catalog click-to-visible time. A
            regression needs at least three samples per build and a median
            increase above 30% and 100 ms. QA tests are kept separate.
          </p>
          <table className="health-table">
            <thead>
              <tr>
                <th>Operation</th>
                <th>Samples</th>
                <th>Latest</th>
                <th>Median</th>
                <th>Previous build median</th>
                <th>Trend</th>
              </tr>
            </thead>
            <tbody>
              {health.performance.map((p) => (
                <tr key={`${p.lane}:${p.name}`}>
                  <td>
                    {PERF_LABELS[p.name] ?? p.name}
                    {p.lane === "qa" ? " (QA)" : ""}
                  </td>
                  <td>{p.count}</td>
                  <td>{ms(p.lastMs)}</td>
                  <td>{ms(p.medianMs)}</td>
                  <td>{ms(p.baselineMs)}</td>
                  <td>
                    {p.regression
                      ? "Slower—investigate"
                      : p.count < 3
                        ? "Collecting samples"
                        : "Recorded"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
      <div className="settings-support-actions">
        <button className="secondary"
          onClick={() =>
            void call<{ text: string }>("launcher-tools-health-copy")
              .then((r) => copyText(r.text))
              .then(() =>
                setStatus(
                  "Diagnostic report copied. Local paths are redacted; nothing was uploaded.",
                ),
              )
              .catch((e) => setStatus(message(e)))
          }
        >
          Copy diagnostic report
        </button>
      </div>
    </ToolCard>
  );
}

// ---------- Previous WPF library import ----------

type Source = {
  index: number;
  label: string;
  path: string;
  outfits: {
    total: number;
    ready: number;
    error?: string;
    items: { name: string; status: string }[];
  };
  previews: {
    packs: number;
    newest: { name: string; bytes: number; alreadyActive: boolean } | null;
    rustPackActive: boolean;
  };
  advanced: { key: string; label: string; current: unknown; wpf: unknown }[];
  arenaFavorites: string[];
};
const show = (v: unknown) =>
  typeof v === "boolean" ? (v ? "On" : "Off") : String(v);

export function WpfImportCard({ setStatus }: { setStatus: Status }) {
  const [scan, setScan] = useState<{ sources: Source[]; imported: Record<string, { at: string }> } | null>(null);
  const [pick, setPick] = useState(0);
  const [cats, setCats] = useState<Record<string, boolean>>({
    outfits: true,
    previews: true,
    advanced: true,
  });
  const [busy, setBusy] = useState(false);
  const [text, setText] = useState("");
  const load = () =>
    call<typeof scan>("launcher-tools-migration-scan")
      .then((s) => setScan(s))
      .catch((e) => setText(message(e)));
  useEffect(() => void load(), []);
  const source = scan?.sources[pick];
  return (
    <ToolCard title="Previous launcher library" className="wpf-import-card">
      <p className="settings-description">
        Import outfits, preview packs and advanced settings from the WPF
        launcher. Settings, music and videos were imported when this launcher
        first opened. The old library is only read and is preserved either way.
      </p>
      {!scan ? (
        <p className="settings-description">{text || "Looking for a previous library…"}</p>
      ) : scan.sources.length === 0 ? (
        <p className="settings-description">No previous launcher library was found.</p>
      ) : (
        <>
          {scan.sources.length > 1 && (
            <label className="field">
              Previous library
              <select
                value={pick}
                onChange={(e) => setPick(Number(e.target.value))}
              >
                {scan.sources.map((s) => (
                  <option key={s.index} value={s.index}>
                    {s.label}
                  </option>
                ))}
              </select>
            </label>
          )}
          {source && (
            <>
              <p className="path-line muted">{source.path}</p>
              <label className="check">
                <input
                  type="checkbox"
                  checked={cats.outfits}
                  onChange={(e) => setCats({ ...cats, outfits: e.target.checked })}
                />
                Outfits · {source.outfits.ready} ready of {source.outfits.total}
              </label>
              {source.outfits.error && (
                <p className="settings-description">{source.outfits.error}</p>
              )}
              {source.outfits.items.length > 0 && (
                <details>
                  <summary>Outfit details</summary>
                  <ul className="launcher-notes">
                    {source.outfits.items.map((o, i) => (
                      <li key={i}>
                        {o.name} · {o.status}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              <label className="check">
                <input
                  type="checkbox"
                  checked={cats.previews}
                  onChange={(e) => setCats({ ...cats, previews: e.target.checked })}
                />
                Preview pack ·{" "}
                {source.previews.newest
                  ? source.previews.newest.alreadyActive
                    ? "already active"
                    : source.previews.rustPackActive
                      ? "a pack is already active here; it will be kept"
                      : `${source.previews.newest.name} (${(source.previews.newest.bytes / 1048576).toFixed(0)} MB copy)`
                  : "none found"}
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={cats.advanced}
                  onChange={(e) => setCats({ ...cats, advanced: e.target.checked })}
                />
                Advanced settings · {source.advanced.length} change
                {source.advanced.length === 1 ? "" : "s"}
                {source.arenaFavorites.length > 0 &&
                  ` · ${source.arenaFavorites.length} arena favorite${source.arenaFavorites.length === 1 ? "" : "s"}`}
              </label>
              {source.advanced.length > 0 && (
                <ul className="launcher-notes">
                  {source.advanced.map((a) => (
                    <li key={a.key}>
                      {a.label}: {show(a.current)} → {show(a.wpf)}
                    </li>
                  ))}
                </ul>
              )}
              <div className="settings-support-actions">
                <button className="secondary"
                  disabled={busy || !Object.values(cats).some(Boolean)}
                  onClick={async () => {
                    if (busy) return;
                    setBusy(true);
                    setText("Importing previous library…");
                    try {
                      const r = await call<{ message: string }>(
                        "launcher-tools-migration-apply",
                        {
                          index: source.index,
                          path: source.path,
                          categories: Object.keys(cats).filter((k) => cats[k]),
                        },
                      );
                      setText(r.message);
                      setStatus(r.message);
                      await load();
                    } catch (e) {
                      setText(message(e));
                    } finally {
                      setBusy(false);
                    }
                  }}
                >
                  Import previous library
                </button>
                <button className="secondary" disabled={busy} onClick={() => void load()}>
                  Scan again
                </button>
              </div>
            </>
          )}
          {Object.keys(scan.imported ?? {}).length > 0 && (
            <p className="settings-description">
              Imported before:{" "}
              {Object.entries(scan.imported)
                .map(([k, v]) => `${k} (${new Date(v.at).toLocaleDateString()})`)
                .join(", ")}
            </p>
          )}
          <p className="settings-description" role="status">
            {text}
          </p>
        </>
      )}
    </ToolCard>
  );
}

// ---------- Developer diagnostics (hidden; Ctrl+Shift+D) ----------

type Resolver = {
  status: string;
  message: string;
  pinned?: { label: string; message: string };
  report: unknown;
};

export function DeveloperTools({ setStatus }: { setStatus: Status }) {
  const [open, setOpen] = useState(false);
  const [resolver, setResolver] = useState<Resolver | null>(null);
  const [path, setPath] = useState("png10000.iff");
  const [result, setResult] = useState("");
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "d") {
        e.preventDefault();
        setOpen((o) => {
          if (!o)
            setStatus("Developer diagnostics unlocked for this launcher window.");
          return !o;
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setStatus]);
  useEffect(() => {
    if (!open) return;
    setResolver({ status: "scanning", message: "Resolving the provider layout from the local executable…", report: null });
    void call<Resolver>("launcher-tools-resolver")
      .then(setResolver)
      .catch((e) => setResolver({ status: "error", message: message(e), report: null }));
    ref.current?.scrollIntoView({ block: "nearest" });
  }, [open]);
  if (!open) return null;
  async function run(fn: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      setResult(`FAIL · ${message(e)}`);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      ref={ref}
      className="card settings-path-card settings-tool developer-tools"
      aria-label="Developer diagnostics"
    >
      <div className="settings-card-heading">
        <strong>Developer diagnostics</strong>
        <span className={`settings-badge resolver-${resolver?.status ?? "pending"}`} id="resolverState">
          {(resolver?.status ?? "pending").toUpperCase()}
        </span>
      </div>
      <div className="card-content">
        <h4>Provider-layout resolver</h4>
        <p className="settings-description">
          This diagnostic maps the executable file read-only. It does not attach
          to the game, write game memory, install hooks, or register the
          provider.
        </p>
        <p>{resolver?.message}</p>
        {resolver?.pinned && (
          <p className="settings-description">
            {resolver.pinned.label} · {resolver.pinned.message}
          </p>
        )}
        <div className="settings-support-actions">
          <button className="secondary"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setResolver({ status: "scanning", message: "Resolving the provider layout from the local executable…", report: null });
                const r = await call<Resolver>("launcher-tools-resolver", { force: true });
                setResolver(r);
                if (r.status === "ready") setStatus("Current game build resolved and cached.");
              })
            }
          >
            Rescan executable
          </button>
          <button className="secondary"
            disabled={busy || !resolver?.report}
            onClick={() =>
              void run(async () => {
                const r = await call<{ text: string }>("launcher-tools-resolver-copy");
                await copyText(r.text);
                setStatus("Resolver report copied.");
              })
            }
          >
            Copy report
          </button>
        </div>
        {resolver?.report != null && (
          <pre id="resolverOutput">{JSON.stringify(resolver.report, null, 2)}</pre>
        )}
        <h4>Provider core and bridge</h4>
        <label className="field">
          Offline asset-path test
          <input value={path} onChange={(e) => setPath(e.target.value)} />
        </label>
        <div className="settings-support-actions">
          <button className="secondary"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setResult("Testing provider core…");
                const r = await call<{ message: string }>("launcher-tools-provider-test", { path });
                setResult(r.message);
              })
            }
          >
            Test provider core
          </button>
          <button className="secondary"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setResult("Running read-only bridge audit…");
                const r = await call<{ message: string }>("launcher-tools-bridge-audit", { path });
                setResult(r.message);
              })
            }
          >
            Run bridge audit
          </button>
        </div>
        <p role="status">{result}</p>
        <h4>Controlled game handshake</h4>
        <p className="settings-description">
          The live registration probe is not part of this launcher. It needs a
          running game and an in-game add-on, so it stays off. Nothing here
          launches, attaches to or writes the game.
        </p>
      </div>
    </section>
  );
}

// ---------- Ctrl+Shift+F12 state (Support card) ----------

type HotkeyState = {
  phase: "off" | "waiting-for-game" | "registered" | "held";
  message: string;
  presses: number;
  lastPressMs: number | null;
  lastSource: "hotkey" | "keyboard-poll" | null;
};
const HOTKEY_LABEL: Record<HotkeyState["phase"], string> = {
  off: "Off",
  "waiting-for-game": "Waiting for game",
  registered: "Registered",
  held: "Held by another app",
};

export function hotkeyText(h: HotkeyState) {
  const last =
    h.lastPressMs != null
      ? ` · last press ${new Date(h.lastPressMs).toLocaleTimeString()}${h.lastSource === "keyboard-poll" ? " (key watch)" : ""}`
      : "";
  return `Ctrl+Shift+F12: ${HOTKEY_LABEL[h.phase] ?? h.phase} · ${h.message}${last}`;
}

export function HotkeyStatus() {
  const [state, setState] = useState<HotkeyState | null>(null);
  useEffect(() => {
    const load = () =>
      call<HotkeyState>("launcher-tools-hotkey")
        .then(setState)
        .catch(() => setState(null));
    void load();
    const timer = setInterval(load, 3000);
    return () => clearInterval(timer);
  }, []);
  if (!state) return null;
  return (
    <small id="gameCaptureHotkeyState" data-phase={state.phase}>
      {hotkeyText(state)}
    </small>
  );
}

// ---------- Game build compatibility (Settings > Advanced) ----------

type FeatureCompat = {
  key: string;
  label: string;
  status: "Mapped" | "Re-derived" | "Needs mapping";
  message: string;
  anchors: number;
  found: number;
  constants: number;
  derived: number;
};
type CompatReport = {
  build: string | null;
  buildLabel?: string;
  pinned?: boolean;
  modLoadingAdmitted?: boolean;
  message: string;
  features: FeatureCompat[];
};

export function compatDetail(f: FeatureCompat) {
  const proofs =
    f.anchors > 0
      ? ` · ${f.found}/${f.anchors} code anchors${f.constants ? ` · ${f.derived}/${f.constants} game globals` : ""}`
      : "";
  return `${f.message}${proofs}`;
}

export function GameBuildCompatibility() {
  const [report, setReport] = useState<CompatReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const load = async (force: boolean) => {
    setBusy(true);
    setError("");
    try {
      setReport(
        await call<CompatReport>("launcher-tools-compatibility", { force }),
      );
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <ToolCard
      title="Game build compatibility"
      className="game-build-compatibility"
      action={
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void load(true)}
        >
          {busy
            ? "Checking…"
            : report
              ? "Rescan game build"
              : "Check game build"}
        </button>
      }
    >
      <p className="settings-description">
        Run this manual check after a game version update. It reads the game
        file and reports which features have a verified mapping.
      </p>
      {!report && !busy && !error && (
        <p className="settings-description" id="gameBuildManualStatus">
          Not checked. Select Check game build when you need a compatibility
          report.
        </p>
      )}
      {report && (
        <p className="settings-description" id="gameBuildSummary">
          {report.build
            ? `${report.buildLabel ?? report.build} (${report.build}) · `
            : ""}
          {report.message}
        </p>
      )}
      {error && (
        <p className="settings-description" role="alert">
          {error}
        </p>
      )}
      {report && report.features.length > 0 && (
        <table className="health-table" id="gameBuildCompatibility">
          <thead>
            <tr>
              <th>Feature</th>
              <th>Status</th>
              <th>Details</th>
            </tr>
          </thead>
          <tbody>
            {report.features.map((f) => (
              <tr
                key={f.key}
                data-state={f.status === "Needs mapping" ? "failed" : "active"}
              >
                <td>{f.label}</td>
                <td>{f.status}</td>
                <td>{compatDetail(f)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </ToolCard>
  );
}
