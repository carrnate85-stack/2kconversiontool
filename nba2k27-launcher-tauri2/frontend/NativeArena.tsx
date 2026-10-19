import { useEffect, useRef, useState } from "react";
import { call } from "./api";
export type ArenaStatus = {
  phase: string;
  requestedId: string;
  activeId: string;
  revision: number;
  appliedRevision: number;
  message: string;
  requestedName?: string;
};
export type CatalogFeature = "arena" | "ball" | "scoreboard";
/** Requested catalogs that have not reported their current request applied.
 * The F8 badge stays on "Preparing" until these are ready (WPF: wait for
 * Ready before starting the match). Held/error/ended are reported elsewhere. */
export function catalogsPreparing(
  statuses: Partial<Record<CatalogFeature, ArenaStatus | null | undefined>>,
): CatalogFeature[] {
  return (["arena", "ball", "scoreboard"] as const).filter((kind) => {
    const s = statuses[kind];
    return (
      !!s &&
      s.revision > 0 &&
      s.appliedRevision < s.revision &&
      !["held", "error", "ended", "unavailable"].includes(s.phase)
    );
  });
}
export type ArenaSession = {
  session?: string;
  phase: string;
  error?: string | null;
  nativeProvider?: {
    phase?: string;
    build?: {
      label?: string;
      nativeWritesMapped?: boolean;
      modProviderMapped?: boolean;
      version2ArenaBallMapped?: boolean;
      version2GameplayBallMapped?: boolean;
      version2ScoreboardMapped?: boolean;
    };
    arena?: ArenaStatus;
    ball?: ArenaStatus;
    scoreboard?: ArenaStatus;
  } | null;
};
export function arenaBuildMapped(
  build: NonNullable<ArenaSession["nativeProvider"]>["build"],
  feature: CatalogFeature = "arena",
) {
  if (feature === "scoreboard") {
    return (
      build?.label === "Version 2" &&
      build.modProviderMapped === true &&
      build.version2ScoreboardMapped === true
    );
  }
  return (
    build?.nativeWritesMapped === true ||
    (build?.label === "Version 2" &&
      build.modProviderMapped === true &&
      (feature === "ball"
        ? build.version2GameplayBallMapped
        : build.version2ArenaBallMapped) === true)
  );
}
export function arenaEligible(
  session: ArenaSession | null,
  feature: CatalogFeature = "arena",
) {
  return (
    session?.phase === "game-running" &&
    !session.error &&
    session.nativeProvider?.phase === "loader-active" &&
    arenaBuildMapped(session.nativeProvider.build, feature) &&
    /^[a-f0-9]{48}$/i.test(session.session ?? "") &&
    !["held", "ended"].includes(session.nativeProvider[feature]?.phase ?? "")
  );
}
export function mergeCatalogStatus(
  old: ArenaStatus | null,
  incoming: ArenaStatus | null,
  sameSession: boolean,
) {
  return sameSession &&
    old &&
    incoming &&
    !["held", "ended"].includes(incoming.phase) &&
    old.revision > incoming.revision
    ? old
    : incoming;
}
export function catalogReplyCurrent(
  expectedGeneration: number,
  currentGeneration: number,
  expectedSession: string | undefined,
  currentSession: string | undefined,
) {
  return (
    expectedGeneration === currentGeneration &&
    expectedSession === currentSession
  );
}
const featureName = (feature: CatalogFeature) =>
  feature[0].toUpperCase() + feature.slice(1);
// Plain status shown in place of WPF's (absent) per-feature confirmation: a
// choice simply applies at Blacktop player selection once the game is eligible.
export function catalogUnavailableMessage(
  session: ArenaSession | null,
  feature: CatalogFeature = "arena",
) {
  const name = featureName(feature);
  if (!session || session.phase !== "game-running" || session.error)
    return `${name} changes are available while the game launched from this launcher is running.`;
  const provider = session.nativeProvider;
  if (provider?.phase !== "loader-active")
    return `${name} changes become available when this game's native provider is active.`;
  if (!arenaBuildMapped(provider.build, feature))
    return `${name} editing is disabled for this game build while updated mappings are pending. Mod loading is available separately.`;
  if (["held", "ended"].includes(provider[feature]?.phase ?? ""))
    return (
      (feature === "scoreboard" && provider[feature]?.phase === "held"
        ? "Scoreboard changes stopped for this game. Close the game and restart before choosing another scoreboard. "
        : "") + (
      provider[feature]?.message ||
      `${name} changes have ended for this game session.`)
    );
  return `${name} changes are not available for this game session.`;
}
export function useNativeArena(
  live: boolean,
  gameRoot: string,
  feature: CatalogFeature = "arena",
  active = false,
) {
  const [session, setSession] = useState<ArenaSession | null>(null);
  const [status, setStatus] = useState<ArenaStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [pendingId, setPendingId] = useState("");
  const [error, setError] = useState("");
  const pending = useRef(false),
    generation = useRef(0),
    sessionRef = useRef<string | undefined>(undefined);
  const activeRef = useRef(active), statusRef = useRef(status);
  activeRef.current = active;
  statusRef.current = status;
  useEffect(() => {
    const g = ++generation.current;
    setSession(null);
    sessionRef.current = undefined;
    setStatus(null);
    setError("");
    pending.current = false;
    setBusy(false);
    setPendingId("");
    if (!live) return;
    let polling = false;
    async function poll() {
      if (polling) return;
      polling = true;
      try {
        const next = await call<ArenaSession>("native-session-status");
        if (g === generation.current) {
          setSession(next);
          const sameSession = next.session === sessionRef.current;
          setStatus((old) => {
            const incoming = next.nativeProvider?.[feature] ?? null;
            // A poll begun before our queued response cannot roll its
            // revision/status back to an earlier catalog request.
            return mergeCatalogStatus(old, incoming, sameSession);
          });
          sessionRef.current = next.session;
        }
      } catch (e) {
        if (g === generation.current) setError(String(e));
      } finally {
        polling = false;
      }
    }
    void poll();
    let idleTicks = 0;
    const timer = setInterval(() => {
      const awaiting = catalogsPreparing({ [feature]: statusRef.current }).length > 0;
      if (activeRef.current || pending.current || awaiting || ++idleTicks % 4 === 0)
        void poll();
    }, 250);
    return () => {
      ++generation.current;
      clearInterval(timer);
    };
  }, [live, gameRoot, feature]);
  useEffect(() => {
    pending.current = false;
    setBusy(false);
    setPendingId("");
    setError("");
  }, [session?.session]);
  const eligible = !live || arenaEligible(session, feature);
  async function select(assetId: string) {
    if (pending.current) return false;
    if (!live) {
      setStatus({
        phase: "preview",
        requestedId: assetId,
        activeId: assetId,
        revision: 0,
        appliedRevision: 0,
        message: "Offline example · no game change.",
      });
      return true;
    }
    if (!arenaEligible(session, feature)) {
      setError(catalogUnavailableMessage(session, feature));
      return false;
    }
    pending.current = true;
    setBusy(true);
    setPendingId(assetId);
    setError("");
    const g = generation.current;
    const requestedSession = session!.session;
    try {
      // The backend still requires its fixed per-feature token; picking an
      // item is the user's explicit choice, exactly as in the WPF overlay.
      const next = await call<ArenaStatus>(`native-${feature}-select`, {
        acknowledgement: `apply-one-${feature}-for-current-game`,
        gameRoot,
        session: session!.session,
        assetId,
      });
      if (
        !catalogReplyCurrent(
          g,
          generation.current,
          requestedSession,
          sessionRef.current,
        )
      )
        return false;
      setStatus(next);
      return true;
    } catch (e) {
      if (
        catalogReplyCurrent(
          g,
          generation.current,
          requestedSession,
          sessionRef.current,
        )
      )
        setError(String(e));
      return false;
    } finally {
      if (
        catalogReplyCurrent(
          g,
          generation.current,
          requestedSession,
          sessionRef.current,
        )
      ) {
        pending.current = false;
        setBusy(false);
        setPendingId("");
      }
    }
  }
  return {
    status,
    busy,
    pendingId,
    error:
      error ||
      (live &&
      session?.nativeProvider?.phase === "loader-active" &&
      !arenaBuildMapped(session.nativeProvider.build, feature)
        ? catalogUnavailableMessage(session, feature)
        : ""),
    select,
    eligible,
    unavailable: live && session && !eligible
      ? catalogUnavailableMessage(session, feature)
      : "",
  };
}
export function CatalogStatus({
  live,
  control,
  feature = "arena",
}: {
  live: boolean;
  control: ReturnType<typeof useNativeArena>;
  feature?: CatalogFeature;
}) {
  const text =
    control.error ||
    control.unavailable ||
    (control.busy ? `Selecting ${feature}…` : control.status?.message) ||
    (live
      ? control.eligible
        ? `Choose a ${feature}; it applies at Blacktop player selection.`
        : "Waiting for this game’s native provider."
      : "Offline preview · dummy session · no game writes");
  return (
    <p
      role="status"
      className="native-arena-controls"
      title={text}
      data-phase={
        control.error || control.unavailable
          ? "error"
          : control.busy
            ? "pending"
            : (control.status?.phase ?? "idle")
      }
    >
      {text}
    </p>
  );
}
