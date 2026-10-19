import { useEffect, useRef, useState, type ReactNode } from "react";
import { UiIcon } from "./wpf/WpfShell";
import { call } from "./api";
import { NativeRules } from "./NativeRules";
import { NativeOverlayControls } from "./NativeOverlay";
import { NativeCatalogStatuses } from "./NativeCatalogStatuses";
import { NativeFeatureStatus, type V2Features } from "./NativeFeatureStatus";
import { NativeLaunchTiming, type LaunchTimingValue } from "./LaunchTiming";
import { LaunchOverlay } from "./LaunchOverlay";
import type { ArenaStatus } from "./NativeArena";
import type { NativeOutfitStatus } from "./NativeOutfits";
import {
  NativeSelectorStatus,
  type SelectorSummary,
} from "./NativeSelectorStatus";
import {
  inspectPlay,
  launchRequest,
  playNeedsConfirm,
  playSummary,
  readyToConfirm,
  sessionPollInterval,
  type PlayReport as Report,
} from "./PlayLaunch";
import "./PlayHome.css";
type Session = {
  startupIntro?: { phase?: string; message?: string; finished?: boolean };
  phase: string;
  message: string;
  error: string | null;
  automaticFeaturesEnabled: false;
  verifiedRuleWrites: number;
  gameStoppedByLauncher: false;
  nativeProvider?: {
    updatedMs?: number;
    timing?: LaunchTimingValue;
    phase?: string;
    message?: string;
    error?: string | null;
    arena?: ArenaStatus;
    ball?: ArenaStatus;
    scoreboard?: ArenaStatus;
    outfits?: NativeOutfitStatus;
    selectors?: SelectorSummary;
    playerFolderFiles?: number;
    v2Features?: V2Features;
  } | null;
};
type BlacktopObservation = {
  phase: string;
  message: string;
  readOnly: true;
  writes: 0;
  sample: null | {
    boundary: string;
    requiredPlayers: number;
    selectedPlayers: number;
    loadedPlayers: number;
    lineupReady: boolean;
    playersReady: boolean;
    currentHash: string;
    alternateHash: string;
    stage: number;
    writes: 0;
  };
};
export function LaunchReadiness({
  gameRoot,
  modsRoot,
  gameFound,
  children,
}: {
  gameRoot: string;
  modsRoot: string;
  gameFound?: boolean;
  children?: ReactNode;
}) {
  const [report, setReport] = useState<Report | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [pollError, setPollError] = useState(""),
    [session, setSession] = useState<Session | null>(null),
    [observer, setObserver] = useState<BlacktopObservation | null>(null),
    [confirm, setConfirm] = useState(false),
    [ack, setAck] = useState(false);
  const [modded, setModded] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const [launchStartedAt, setLaunchStartedAt] = useState<number | null>(null);
  const [launchError, setLaunchError] = useState(""),
    [launchNotice, setLaunchNotice] = useState("");
  const pending = useRef(false),
    launching = useRef(false),
    generation = useRef(0);
  const dialog = useRef<HTMLElement>(null);
  const pollEvery = sessionPollInterval(session?.phase, advanced);
  useEffect(() => {
    if (!confirm) return;
    const before = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLInputElement>("input")?.focus();
    return () => {
      if (before?.isConnected) before.focus();
    };
  }, [confirm]);
  useEffect(() => {
    ++generation.current;
    setReport(null);
    setError("");
    setConfirm(false);
    setAck(false);
    return () => {
      ++generation.current;
    };
  }, [gameRoot, modsRoot]);
  useEffect(() => {
    let alive = true,
      polling = false;
    async function poll() {
      if (polling || (pending.current && !launching.current)) return;
      polling = true;
      try {
        const [s, o] = await Promise.all([
          call<Session>("native-session-status"),
          advanced && !launching.current
            ? call<BlacktopObservation>("native-blacktop-status")
            : Promise.resolve(null),
        ]);
        if (alive && (!pending.current || launching.current)) {
          if (launching.current && ["idle", "closed"].includes(s.phase)) return;
          setSession(s);
          setPollError("");
          if (s.phase === "closed" && !s.error) {
            setReport((old) => (old?.recoveryRequired ? null : old));
          }
          if (o) setObserver(o);
        }
      } catch (e) {
        if (alive) setPollError(String(e));
      } finally {
        polling = false;
      }
    }
    void poll();
    const timer = setInterval(() => void poll(), pollEvery);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [advanced, pollEvery]);
  async function observeBlacktop(stop: boolean) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      setObserver(
        await call<BlacktopObservation>(
          stop ? "native-blacktop-disable" : "native-blacktop-enable",
          stop
            ? undefined
            : { acknowledgement: "read-only-blacktop", gameRoot },
        ),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  async function launch(isModded: boolean, checked: Report | null) {
    if (pending.current || !readyToConfirm(checked)) return;
    if (
      playNeedsConfirm(isModded ? "modded" : "steam-only") &&
      (!ack || !confirm)
    )
      return;
    pending.current = true;
    launching.current = true;
    // WPF shows its startup overlay only for the normal modded Play.
    setLaunchError("");
    setLaunchNotice("");
    if (isModded) setLaunchStartedAt(Date.now());
    setBusy(true);
    setError("");
    setConfirm(false);
    setSession({
      phase: "waiting-for-game",
      message: "Checking your launch settings. Keep the launcher open.",
      error: null,
      automaticFeaturesEnabled: false,
      verifiedRuleWrites: 0,
      gameStoppedByLauncher: false,
    });
    try {
      const request = launchRequest(isModded, gameRoot, modsRoot);
      const started = await call<Session & { startupVideoSkipped?: string }>(
        request.channel,
        request.args,
      );
      setSession(started);
      if (started.startupVideoSkipped)
        setLaunchNotice(started.startupVideoSkipped);
      setConfirm(false);
      setReport(null);
    } catch (e) {
      setError(String(e));
      setLaunchError(String(e));
      setConfirm(false);
    } finally {
      launching.current = false;
      pending.current = false;
      setBusy(false);
    }
  }
  async function clear() {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      setSession(await call<Session>("native-session-clear"));
      setReport(null);
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  async function check(intent?: "modded" | "steam-only") {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    setConfirm(false);
    setAck(false);
    setReport(null);
    const g = generation.current;
    let launchNow: Report | null = null;
    try {
      const r = await inspectPlay(call);
      if (g === generation.current) {
        setReport(r);
        if (intent && readyToConfirm(r)) {
          setModded(intent === "modded");
          if (playNeedsConfirm(intent)) setConfirm(true);
          else launchNow = r;
        }
      }
    } catch (e) {
      if (g === generation.current) setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
    if (launchNow && g === generation.current) await launch(true, launchNow);
  }
  const summary = playSummary(session, report);
  const gameMissing = !gameRoot.trim() || gameFound === false;
  const displayedSummary =
    summary.label === "Ready" && gameMissing
      ? {
          label: "Check setup",
          title: undefined,
          message: "Game folder not found. Choose your NBA 2K27 folder.",
          tone: "attention",
        }
      : summary;
  const sessionActive =
    session?.phase === "game-running" || session?.phase === "waiting-for-game";
  // The overlay follows the provider of THIS launch; an earlier session's
  // closed/idle state is never read as this launch's outcome.
  const overlayPhase =
    session?.phase === "waiting-for-game" || session?.phase === "game-running"
      ? (session.nativeProvider?.phase ?? "queued")
      : session?.phase === "needs-review"
        ? "needs-review"
        : "queued";
  const overlayError =
    launchError ||
    session?.error ||
    (session?.phase !== "closed" ? session?.nativeProvider?.error : null) ||
    null;
  return (
    // Fragment: WPF rules target #page-play > .hero-card directly.
    <>
      {launchStartedAt !== null && (
        <LaunchOverlay
          startedAt={launchStartedAt}
          phase={overlayPhase}
          error={overlayError}
          onDone={() => setLaunchStartedAt(null)}
        />
      )}
      <div className="hero-card" aria-label="Play NBA 2K27">
        <div className="game-identity">
          <h2>
            NBA 2K27{" "}
            <span
              id="launchStatus"
              role="status"
              data-state={
                displayedSummary.tone === "ready"
                  ? "ready"
                  : displayedSummary.tone === "running"
                    ? "running"
                    : displayedSummary.tone === "attention"
                      ? "error"
                      : "busy"
              }
            >
              {displayedSummary.label}
            </span>
          </h2>
          <div
            id="homeSetup"
            className="home-setup"
            role="status"
            aria-live="polite"
            data-state={
              sessionActive
                ? "active"
                : gameMissing
                  ? "blocked"
                  : displayedSummary.label === "Ready"
                    ? "ready"
                    : "warning"
            }
          >
            <p id="homeSetupIssue">
              {displayedSummary.label === "Ready"
                ? "Game and profile folders checked."
                : displayedSummary.message}
            </p>
            {gameMissing && (
              <small id="homeSetupPath">
                Choose your NBA 2K27 folder in Settings.
              </small>
            )}
          </div>
        </div>
        <div className="hero-actions">
          {sessionActive ? (
            // WPF hides Play during a session and shows homeSessionActive.
            <div
              className="session-live-summary"
              id="homeSessionActive"
              role="status"
            >
              <span className="live-orbit" aria-hidden="true" />
              <strong id="homeSessionActiveTitle">
                {displayedSummary.title ?? "Session in progress"}
              </strong>
              <small>Keep the launcher open while you play.</small>
            </div>
          ) : (
            <button
              className="primary play-primary"
              id="startSession"
              data-primary-play
              aria-label="Play NBA 2K27"
              data-ui-icon="play"
              disabled={busy || gameMissing || launchStartedAt !== null}
              onClick={() => void check("modded")}
            >
              <UiIcon name="play" />
              Play session
            </button>
          )}
        </div>
      </div>
      {sessionActive && !advanced && (
        <NativeFeatureStatus
          status={session?.nativeProvider?.v2Features}
          issuesOnly
        />
      )}
      {launchNotice && (
        <p role="status" className="muted play-launch-alert">
          {launchNotice}
        </p>
      )}
      {(error || pollError || session?.error) && (
        <p role="alert" className="notice play-launch-alert">
          {error || pollError || session?.error}
        </p>
      )}
      {(report?.recoveryRequired ||
        session?.phase === "needs-review" ||
        session?.nativeProvider?.phase === "waiting-for-safe-cleanup") && (
        <div className="play-launch-blockers">
          <p>
            Close NBA 2K27 and let the previous session finish restoring its
            files.
          </p>
          <button
            disabled={busy || session?.phase === "game-running"}
            onClick={() => void clear()}
          >
            Finish cleanup
          </button>
        </div>
      )}
      {report && !readyToConfirm(report) && (
        <div className="play-launch-blockers" role="alert">
          <strong>Unable to start this session</strong>
          <ul>
            {report.blockers.slice(0, 3).map((b, i) => (
              <li key={i}>{b}</li>
            ))}
          </ul>
          <small>
            Open Advanced for the complete checks and recovery controls.
          </small>
        </div>
      )}
      {children}
      <details
        className="card play-advanced launch-readiness"
        data-play-advanced
        open={advanced}
        onToggle={(e) => {
          if (e.target === e.currentTarget) setAdvanced(e.currentTarget.open);
        }}
      >
        <summary>
          Advanced <span>Launch checks, recovery and in-game tools</span>
        </summary>
        {advanced && (
          <div className="card-content">
            <div className="toolbar">
              <button disabled={busy} onClick={() => void check()}>
                {busy ? "Checking…" : "Check launch readiness"}
              </button>
              <button
                disabled={busy || sessionActive}
                onClick={() => void check("steam-only")}
              >
                Launch game only (QA)
              </button>
              <span className="muted">
                Readiness is read-only. Play launches once checks pass; the
                game-only QA launch asks first.
              </span>
            </div>
            {error && (
              <p role="alert" className="notice">
                {error}
              </p>
            )}
            {session && (
              <div className="native-session-status">
                <p className="muted">
                  <strong>{session.phase}</strong> · {session.message}
                  {session.verifiedRuleWrites > 0 &&
                    ` · Verified manual rule changes: ${session.verifiedRuleWrites}`}
                </p>
                {session.error && (
                  <p role="alert" className="notice">
                    {session.error}
                  </p>
                )}
                {session.startupIntro?.phase &&
                  session.startupIntro.phase !== "idle" && (
                    <p className="muted">
                      Startup video: {session.startupIntro.phase}
                      {session.startupIntro.message
                        ? ` · ${session.startupIntro.message}`
                        : ""}
                    </p>
                  )}
                {session.nativeProvider?.phase && (
                  <p className="muted">
                    Provider: {session.nativeProvider.phase}{" "}
                    {session.nativeProvider.error &&
                      `· ${session.nativeProvider.error}`}
                  </p>
                )}
                <button disabled={busy} onClick={() => void clear()}>
                  Clear finished launch
                </button>
                <NativeSelectorStatus
                  phase={session.nativeProvider?.phase}
                  selectors={session.nativeProvider?.selectors}
                  playerFolderFiles={session.nativeProvider?.playerFolderFiles}
                />
                <NativeFeatureStatus
                  status={session.nativeProvider?.v2Features}
                />
                <NativeLaunchTiming timing={session.nativeProvider?.timing} />
                <NativeCatalogStatuses
                  phase={session.phase}
                  provider={session.nativeProvider}
                />
              </div>
            )}
            <div className="native-blacktop-observer">
              <h4>Native Blacktop observer · read-only QA</h4>
              <p className="muted">
                Observe this prototype’s launched game only. No outfits, balls,
                scoreboards, rules or crowd changes are applied by this
                observer. Reading is off until you start it; it is not enabled
                automatically on restart.
              </p>
              <button
                disabled={
                  busy ||
                  (session?.phase !== "game-running" &&
                    (!observer ||
                      ["disabled", "ended"].includes(observer.phase)))
                }
                onClick={() =>
                  void observeBlacktop(
                    !!observer &&
                      !["disabled", "ended"].includes(observer.phase),
                  )
                }
              >
                {observer && !["disabled", "ended"].includes(observer.phase)
                  ? "Stop read-only observer"
                  : "Start read-only observer"}
              </button>
              {observer && (
                <p className="muted">
                  <strong>{observer.phase}</strong> · {observer.message}
                </p>
              )}
              {observer?.sample && (
                <div className="info-grid">
                  <div>
                    <small>Blacktop boundary</small>
                    <strong>{observer.sample.boundary}</strong>
                  </div>
                  <div>
                    <small>Selected / required</small>
                    <strong>
                      {observer.sample.selectedPlayers} /{" "}
                      {observer.sample.requiredPlayers}
                    </strong>
                  </div>
                  <div>
                    <small>Loaded models</small>
                    <strong>{observer.sample.loadedPlayers}</strong>
                  </div>
                  <div>
                    <small>Lineup ready</small>
                    <strong>
                      {observer.sample.lineupReady ? "Yes" : "No"}
                    </strong>
                  </div>
                </div>
              )}
            </div>
            <NativeOverlayControls
              gameRoot={gameRoot}
              running={session?.phase === "game-running"}
            />
            <NativeRules
              gameRoot={gameRoot}
              running={session?.phase === "game-running"}
            />
            {report && (
              <>
                <div className="info-grid">
                  <div>
                    <small>Process presence</small>
                    <strong>
                      {report.processPresenceKnown
                        ? "Verified snapshot"
                        : "Unknown — blocked"}
                    </strong>
                  </div>
                  <div>
                    <small>NBA 2K27</small>
                    <strong>
                      {report.gameRunning === null
                        ? "Unknown"
                        : report.gameRunning
                          ? "Running"
                          : "Not running"}
                    </strong>
                  </div>
                  <div>
                    <small>Session recovery</small>
                    <strong>
                      {report.recoveryRequired
                        ? "Needs review"
                        : "No pending journal"}
                    </strong>
                  </div>
                  <div>
                    <small>File/process checks</small>
                    <strong>
                      {report.checksPassed
                        ? "Read-only checks passed"
                        : "Blocked / needs review"}
                    </strong>
                  </div>
                </div>
                {report.launchPlan && (
                  <p className="muted">
                    {report.launchPlan.label} · Steam app{" "}
                    {report.launchPlan.appId}
                  </p>
                )}
                {!!report.blockers.length && (
                  <ul>
                    {report.blockers.map((b, i) => (
                      <li key={i}>{b}</li>
                    ))}
                  </ul>
                )}
                <p className="muted compact-note">{report.note}</p>
                {!!report.processes.length && (
                  <details>
                    <summary>
                      Relevant processes · {report.processes.length}
                    </summary>
                    {report.processes.map((p) => (
                      <p key={p.pid}>
                        {p.name} · PID {p.pid} ·{" "}
                        {p.identity
                          ? "Start time and path available"
                          : "Identity unavailable — no ownership granted"}
                      </p>
                    ))}
                  </details>
                )}
              </>
            )}
          </div>
        )}
      </details>
      {confirm && (
        <div className="modal-backdrop">
          <section
            ref={dialog}
            className="modal native-launch-confirm"
            role="dialog"
            aria-modal="true"
            aria-labelledby="native-launch-title"
            onKeyDown={(e) => {
              if (e.key === "Escape" && !busy) {
                e.preventDefault();
                setConfirm(false);
              }
              if (e.key !== "Tab") return;
              const controls = [
                ...(dialog.current?.querySelectorAll<HTMLElement>(
                  "input:not(:disabled),button:not(:disabled)",
                ) ?? []),
              ];
              const first = controls[0],
                last = controls.at(-1);
              if (e.shiftKey && document.activeElement === first) {
                e.preventDefault();
                last?.focus();
              }
              if (!e.shiftKey && document.activeElement === last) {
                e.preventDefault();
                first?.focus();
              }
            }}
          >
            <header>
              <h2 id="native-launch-title">
                {modded
                  ? "Launch NBA 2K27?"
                  : "Launch without launcher features?"}
              </h2>
            </header>
            <div className="modal-body">
              <p>
                Launch NBA 2K27 through Steam’s without-EAC option. Use offline
                only.
              </p>
              <p className="muted">{gameRoot}</p>
              {modded ? (
                <p>
                  Use your selected mod profile and saved session choices. Only
                  independently supported features are applied. Keep the
                  launcher open so it can finish cleanup after the game closes.
                </p>
              ) : (
                <p>
                  No launcher-managed mods, outfits, arenas, balls, scoreboards,
                  cameras, unlocks or Jukebox will be applied. An explicitly
                  enabled external MP4 intro may still play after the game
                  window. Existing installed game mods are left untouched. This
                  is not the full Play pipeline.
                </p>
              )}
              {modded && <p className="muted">Mod profile: {modsRoot}</p>}
              <label>
                <input
                  type="checkbox"
                  checked={ack}
                  disabled={busy}
                  onChange={(e) => setAck(e.target.checked)}
                />{" "}
                {modded
                  ? "I will play offline using Steam’s without-EAC option."
                  : "I understand this is a Steam-only QA test, not a modded launcher session."}
              </label>
            </div>
            <footer>
              <button disabled={busy} onClick={() => setConfirm(false)}>
                Cancel
              </button>
              <button
                className="primary"
                disabled={!ack || busy}
                onClick={() => void launch(modded, report)}
              >
                {busy ? "Sending to Steam…" : "Launch via Steam"}
              </button>
            </footer>
          </section>
        </div>
      )}
    </>
  );
}
