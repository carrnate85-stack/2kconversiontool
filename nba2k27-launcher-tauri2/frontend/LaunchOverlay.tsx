import { useEffect, useRef, useState } from "react";

// WPF launchOverlay parity (renderer index.html launchOverlay, app.js
// LAUNCH_PHASES / showLaunchOverlay / updateLaunchOverlay). WPF stage names,
// progress and wording, mapped onto the Rust provider's phase order so the
// bar only moves forward. The message is WPF's user-facing text, not the
// provider's technical log line.
type Stage = { progress: number; step: string; message: string };
const STAGES: Record<string, Stage> = {
  queued: {
    progress: 8,
    step: "Preparing launch",
    message: "Preparing the selected game session…",
  },
  "preparing-mods": {
    progress: 8,
    step: "Preparing launch",
    message: "Preparing the selected mod profile…",
  },
  "verifying-game": {
    progress: 8,
    step: "Preparing launch",
    message: "Preparing this game session…",
  },
  preparing: {
    progress: 16,
    step: "Activating mod profile",
    message: "Activating the selected mod profile…",
  },
  "preparing-players": {
    progress: 22,
    step: "Scanning selected profile",
    message: "Scanning the selected profile and preparing player selectors…",
  },
  "preparing-cameras": {
    progress: 30,
    step: "Preparing camera names",
    message: "Checking the optional in-game camera names…",
  },
  "preparing-jukebox": {
    progress: 38,
    step: "Preparing playlist audio",
    message: "Preparing the selected playlist for in-game playback…",
  },
  "preparing-scoreboards": {
    progress: 46,
    step: "Preparing scoreboards",
    message: "Preparing scoreboard data bindings…",
  },
  "preparing-selectors": {
    progress: 54,
    step: "Scanning selected profile",
    message: "Scanning the selected profile and preparing player selectors…",
  },
  "preparing-loader": {
    progress: 66,
    step: "Starting local asset loader",
    message:
      "Checking the game build and preparing the local missing-asset route…",
  },
  "waiting-for-game": {
    progress: 78,
    step: "Opening NBA 2K27",
    message: "Local provider ready. Launching NBA 2K27 without EAC…",
  },
  "waiting-for-loader": {
    progress: 86,
    step: "Waiting for the game",
    message: "Waiting for NBA 2K27 to load the verified session proxy…",
  },
  "loader-armed": {
    progress: 91,
    step: "Game connected",
    message:
      "Missing-asset route armed. Waiting 20 seconds for game startup to settle…",
  },
  "activating-loader": {
    progress: 96,
    step: "Finishing unique assets",
    message:
      "Activating stock-first lookup for assets missing from the game manifest…",
  },
  "loader-active": {
    progress: 100,
    step: "Ready",
    message:
      "Unique assets are active. Stock files stay stock except explicitly enabled launcher assets such as camera names.",
  },
};
const FAILED = new Set([
  "held-until-game-exit",
  "needs-review",
  "waiting-for-safe-cleanup",
  "restoring-files",
  "restoring-session",
  "closed",
]);
export type LaunchProgress = {
  title: string;
  message: string;
  step: string;
  progress: number;
  state: "running" | "ready" | "failed";
};
export function launchProgress(
  phase: string | null | undefined,
  error?: string | null,
): LaunchProgress {
  if (error || (phase && FAILED.has(phase)))
    return {
      title: "Startup needs attention",
      message: error || "The game could not be started.",
      step: "Stopped safely",
      progress: 100,
      state: "failed",
    };
  const stage = (phase && STAGES[phase]) || {
    progress: 12,
    step: phase ? phase.replaceAll("-", " ") : "Preparing",
    message: "Preparing NBA 2K27…",
  };
  const ready = stage.progress >= 100;
  return {
    title: ready ? "NBA 2K27 is ready" : "Starting NBA 2K27…",
    message: stage.message,
    step: stage.step,
    progress: stage.progress,
    state: ready ? "ready" : "running",
  };
}
/// The bar never moves backwards while one launch is shown.
export function forwardProgress(previous: number, next: LaunchProgress) {
  return next.state === "running" ? Math.max(previous, next.progress) : 100;
}
export function elapsedLabel(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${seconds} second${seconds === 1 ? "" : "s"}`;
}
export function LaunchOverlay({
  startedAt,
  phase,
  error,
  onDone,
}: {
  startedAt: number;
  phase: string | null | undefined;
  error?: string | null;
  onDone: () => void;
}) {
  const [now, setNow] = useState(Date.now);
  const [bar, setBar] = useState(8);
  const value = launchProgress(phase, error);
  const done = useRef(onDone);
  done.current = onDone;
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    setBar((old) => forwardProgress(old, value));
    if (value.state === "running") return;
    // WPF: ready clears after 0.9 s; a failure stays readable for 2.6 s.
    const timer = setTimeout(
      () => done.current(),
      value.state === "ready" ? 900 : 2600,
    );
    return () => clearTimeout(timer);
  }, [value.state, value.progress]);
  return (
    <div
      className="launch-overlay"
      role="status"
      aria-live="polite"
      aria-label="NBA 2K27 startup progress"
    >
      <section className="launch-dialog">
        <div className="launch-spinner" aria-hidden="true">
          <span />
        </div>
        <p className="eyebrow">STARTING GAME</p>
        <h2>{value.title}</h2>
        <p>{value.message}</p>
        <div className="launch-progress" aria-hidden="true">
          <span style={{ width: `${bar}%` }} />
        </div>
        <div className="launch-meta">
          <span>{value.step}</span>
          <span>{elapsedLabel(now - startedAt)}</span>
        </div>
        <small>
          Keep this launcher open. This window clears automatically when the
          game is ready.
        </small>
      </section>
    </div>
  );
}
