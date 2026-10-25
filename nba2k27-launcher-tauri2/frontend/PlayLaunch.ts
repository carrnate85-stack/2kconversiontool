// Presentation/admission helpers only. The native backend repeats its own
// process/build/ownership checks immediately before any Steam dispatch.
export type PlayReport = {
  readOnly: true;
  checksPassed: boolean;
  processPresenceKnown: boolean;
  gameRunning: boolean | null;
  recoveryRequired: boolean;
  liveGameEnabled: false;
  blockers: string[];
  note: string;
  launchPlan: { appId: string; label: string; steamUrl: string } | null;
  processes: {
    name: string;
    pid: number;
    identity: { started: string; path: string } | null;
  }[];
};
export function readyToConfirm(report: PlayReport | null) {
  return (
    !!report &&
    report.readOnly === true &&
    report.checksPassed === true &&
    report.processPresenceKnown === true &&
    report.gameRunning === false &&
    report.recoveryRequired === false &&
    !!report.launchPlan
  );
}
/// Normal modded Play is one click once fresh read-only checks pass, matching
/// WPF. Unsafe states (recovery pending, game running, unsupported build,
/// unknown process presence) never reach launch: readyToConfirm blocks them.
/// Only the Advanced Steam-only QA path, which skips the launcher pipeline,
/// still asks for an explicit acknowledgement.
export function playNeedsConfirm(intent: "modded" | "steam-only") {
  return intent !== "modded";
}
export async function inspectPlay(
  request: <T>(channel: string, args?: unknown) => Promise<T>,
) {
  // Never reuse the last displayed report for a new click on Play.
  const report = await request<PlayReport>("launch-preflight");
  if (
    !report ||
    report.readOnly !== true ||
    !Array.isArray(report.blockers) ||
    !Array.isArray(report.processes)
  ) {
    throw new Error(
      "Launch checks could not be confirmed. No game launch was sent.",
    );
  }
  return report;
}
export function launchRequest(
  modded: boolean,
  gameRoot: string,
  modsRoot: string,
  debugMenus = false,
) {
  return {
    channel: modded ? "native-launch-modded" : "native-launch-steam-only",
    args: {
      acknowledgement: modded
        ? "native-modded-qa-explicit-blacktop"
        : "steam-only-no-features",
      gameRoot,
      ...(modded ? { modsRoot, debugMenus } : {}),
    },
  };
}
export async function launchAfterSettingsSaved<T>(
  waitForSettings: (() => Promise<void>) | undefined,
  launch: () => Promise<T>,
) {
  if (waitForSettings) await waitForSettings();
  return launch();
}
export function playSummary(
  session: {
    phase: string;
    message?: string;
    error?: string | null;
    nativeProvider?: {
      phase?: string;
      message?: string;
      error?: string | null;
    } | null;
  } | null,
  report: PlayReport | null,
) {
  if (
    session?.error ||
    session?.phase === "needs-review" ||
    (session?.phase !== "closed" && session?.nativeProvider?.error) ||
    report?.recoveryRequired ||
    (report && !readyToConfirm(report))
  ) {
    return {
      label: "Needs attention",
      title: undefined as string | undefined,
      message:
        session?.error ||
        (session?.phase !== "closed" ? session?.nativeProvider?.error : null) ||
        "Review the launch or session details below before starting another game.",
      tone: "attention",
    };
  }
  const provider = session?.nativeProvider;
  if (
    session?.phase !== "closed" &&
    provider?.phase &&
    (provider.phase.startsWith("preparing") ||
      [
        "verifying-game",
        "queued",
        "preparing",
        "preparing-mods",
        "preparing-players",
        "preparing-cameras",
        "preparing-jukebox",
        "preparing-scoreboards",
        "preparing-selectors",
        "waiting-for-game",
        "waiting-for-loader",
        "activating-loader",
      ].includes(provider.phase))
  ) {
    return {
      label: "Session active",
      title: "Preparing your session",
      message: provider.message || "Preparing your session.",
      tone: "running",
    };
  }
  if (session?.phase === "game-running") {
    return {
      label: "Running",
      title: "Session in progress",
      message: "Session in progress. Keep the launcher open while you play.",
      tone: "running",
    };
  }
  if (session?.phase === "waiting-for-game") {
    return {
      label: "Session active",
      title: "Preparing your session",
      message: session?.message || "Preparing your session.",
      tone: "running",
    };
  }
  return {
    label: "Ready",
    message: "Game and profile folders checked.",
    tone: "ready",
  };
}

export function sessionPollInterval(
  phase: string | null | undefined,
  advanced: boolean,
) {
  return advanced ||
    ["waiting-for-game", "game-running", "needs-review"].includes(phase ?? "")
    ? 1000
    : 5000;
}

export function terminalLaunchFailure(
  session: {
    phase?: string;
    message?: string;
    error?: string | null;
    nativeProvider?: { error?: string | null } | null;
  },
  elapsedMs: number,
) {
  if (session.phase !== "closed" || elapsedMs < 1000) return null;
  return (
    session.error ||
    session.nativeProvider?.error ||
    session.message ||
    "The game session closed before Steam launch. Try Play again after reviewing the launcher details."
  );
}
