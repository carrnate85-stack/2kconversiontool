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
    message: "Preparing your scoreboard choices…",
  },
  "preparing-selectors": {
    progress: 54,
    step: "Scanning selected profile",
    message: "Scanning the selected profile and preparing player selectors…",
  },
  "preparing-loader": {
    progress: 66,
    step: "Preparing mod support",
    message:
      "Checking compatibility and preparing your mod files…",
  },
  "waiting-for-game": {
    progress: 78,
    step: "Opening NBA 2K27",
    message: "Opening NBA 2K27 for this modded session…",
  },
  "waiting-for-loader": {
    progress: 86,
    step: "Waiting for the game",
    message: "Waiting for the game to connect to the launcher…",
  },
  "loader-armed": {
    progress: 91,
    step: "Game connected",
    message:
      "The game is connected. Waiting for startup to settle…",
  },
  "activating-loader": {
    progress: 96,
    step: "Connecting features",
    message:
      "Finishing support for your selected mods…",
  },
  "loader-active": {
    progress: 100,
    step: "Ready",
    message:
      "Your session is ready. Enjoy the game.",
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
