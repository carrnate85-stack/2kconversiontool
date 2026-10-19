import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import type { Outfit } from "./types";
import { OutfitRequestQueue } from "./outfit-request-queue";

export type NativeOutfitPlayer = {
  binding: string;
  token: string;
  slot: number;
  name: string;
  cyberface: number;
  pool: "male" | "female" | null;
  // Additive read-only snapshot from the native selection reader. Older
  // providers may omit it; never mistake another player's editor for this one.
  currentOutfit?: Outfit | null;
  currentItems?: string[];
};
export type NativeOutfitStatus = {
  phase: string;
  message: string;
  revision: number;
  completedRevision: number;
  update: number;
  busy: boolean;
  automaticChoiceProtection?: boolean;
  requestAction?: string;
  requestOrigin?: string;
  boundary: string;
  selection: {
    required: number;
    selected: number;
    players: NativeOutfitPlayer[];
    unavailable: number[];
    // Why each unavailable slot was refused (V2 capture); absent on older replies.
    unavailableReasons?: { slot: number; reason: string }[];
  } | null;
};
export type OutfitSession = {
  phase: string;
  session?: string;
  error?: string | null;
  nativeProvider?: {
    phase?: string;
    outfits?: NativeOutfitStatus;
    build?: {
      label?: string;
      modProviderMapped?: boolean;
      nativeWritesMapped?: boolean;
      version2OutfitCarryMapped?: boolean;
      version2OutfitCarryError?: string | null;
    };
  } | null;
};
type Command =
  | { action: "list" }
  | { action: "apply"; playerBinding: string; outfit: Outfit }
  | { action: "restore"; playerBinding: string }
  | {
      action: "apply-many";
      automatic?: boolean;
      items: { playerBinding: string; outfit: Outfit }[];
    };
export function automaticOutfitsSupported(status: NativeOutfitStatus | null) {
  return status?.automaticChoiceProtection === true;
}
export function outfitBatchCommand(
  items: { playerBinding: string; outfit: Outfit }[],
  automatic: boolean,
  protectedRuntime: boolean,
): Command | null {
  if (automatic && !protectedRuntime) return null;
  // Manual requests remain compatible with an older hot-reloaded backend.
  return {
    action: "apply-many",
    items,
    ...(automatic ? { automatic: true } : {}),
  };
}
const dummy: NativeOutfitPlayer = {
  binding: "preview",
  token: "preview",
  slot: 0,
  name: "Stephen Curry",
  cyberface: 0,
  pool: "male",
};
export function previewOutfitStatus(): NativeOutfitStatus {
  return {
    phase: "preview",
    message: "Offline example · dummy player · no game writes",
    revision: 0,
    completedRevision: 0,
    update: 0,
    busy: false,
    boundary: "selection",
    selection: { required: 1, selected: 1, players: [dummy], unavailable: [] },
  };
}
export function outfitBuildMapped(
  build: NonNullable<OutfitSession["nativeProvider"]>["build"],
) {
  return (
    build?.nativeWritesMapped === true ||
    (build?.label === "Version 2" &&
      build.modProviderMapped === true &&
      build.version2OutfitCarryMapped === true)
  );
}
export function outfitsEligible(s: OutfitSession | null) {
  return (
    s?.phase === "game-running" &&
    !s.error &&
    s.nativeProvider?.phase === "loader-active" &&
    outfitBuildMapped(s.nativeProvider.build) &&
    /^[a-f0-9]{48}$/i.test(s.session ?? "") &&
    !["held", "ended"].includes(s.nativeProvider.outfits?.phase ?? "")
  );
}
export function mergeOutfitStatus(
  old: NativeOutfitStatus | null,
  incoming: NativeOutfitStatus | null,
  sameSession: boolean,
) {
  return sameSession &&
    old &&
    incoming &&
    !["held", "ended"].includes(incoming.phase) &&
    old.update > incoming.update
    ? old
    : incoming;
}
export function outfitReplyCurrent(
  g: number,
  current: number,
  session: string | undefined,
  currentSession: string | undefined,
) {
  return g === current && session === currentSession;
}
// Plain status shown when outfit changes cannot be sent for this game.
export function outfitUnavailableMessage(s: OutfitSession | null) {
  if (!s || s.phase !== "game-running" || s.error)
    return "Outfit changes are available while the game launched from this launcher is running.";
  if (s.nativeProvider?.phase !== "loader-active")
    return "Outfit changes become available when this game's native provider is active.";
  if (!outfitBuildMapped(s.nativeProvider.build))
    return (
      s.nativeProvider.build?.version2OutfitCarryError ||
      "Outfit editing is disabled for this game build while updated mappings are pending. Mod loading is available separately."
    );
  if (["held", "ended"].includes(s.nativeProvider.outfits?.phase ?? ""))
    return "Outfit changes have ended for this game session.";
  return "Outfit changes are not available for this game session.";
}
export function canApplyOutfit(
  live: boolean,
  eligible: boolean,
  busy: boolean,
  status: NativeOutfitStatus | null,
  player: NativeOutfitPlayer | undefined,
  outfit: Outfit,
) {
  return (
    !!player &&
    !!player.pool &&
    player.pool === (outfit.pool ?? "male") &&
    !busy &&
    (!live || (eligible && status?.boundary === "selection"))
  );
}
// WPF blacktop.js: with no current choice, the first readable lineup player
// is selected; a choice that left the lineup falls back the same way.
export function defaultOutfitBinding(
  players: readonly NativeOutfitPlayer[],
  binding: string,
) {
  if (players.some((p) => p.binding === binding)) return binding;
  return players.find((p) => p.pool)?.binding ?? "";
}
// Hydrate only a verified snapshot of THIS player. A generic pool template
// must not masquerade as their worn outfit when switching between two men (or
// two women). Call on player selection/editor open, not on every idle poll.
export function selectedPlayerOutfit(
  player: NativeOutfitPlayer | undefined,
): Outfit | null {
  const outfit = player?.currentOutfit;
  if (!player?.pool || !outfit || (outfit.pool ?? "male") !== player.pool)
    return null;
  return { ...outfit, accessories: [...outfit.accessories] };
}
// WPF's player menu always showed the last scanned lineup. Native writes
// publish "refreshing" with no list, and a re-list can fail while preview
// models refresh, so the last list read at selection is retained until the
// game leaves selection, the session holds/ends, or the game changes.
export function retainedLineup(
  previous: NativeOutfitStatus["selection"],
  status: NativeOutfitStatus | null,
): NativeOutfitStatus["selection"] {
  if (!status) return previous;
  if (["held", "ended"].includes(status.phase)) return null;
  if (status.selection)
    return status.boundary === "selection"
      ? sameLineup(previous, status.selection)
        ? previous
        : status.selection
      : null;
  return ["match", "menu", "ended"].includes(status.boundary) ? null : previous;
}
// What the PLAYER list shows. Each List mints fresh single-use tokens, so a
// background re-read returns a new object even when nothing visible changed;
// the previous object is kept so the dropdown only re-renders on a real change.
export function lineupDisplayKey(s: NativeOutfitStatus["selection"]) {
  return s
    ? JSON.stringify([
        s.required,
        s.selected,
        s.players.map((p) => [
          p.binding,
          p.slot,
          p.name,
          p.cyberface,
          p.pool,
          p.currentOutfit,
        ]),
        s.unavailable,
        s.unavailableReasons ?? [],
      ])
    : "";
}
export function sameLineup(
  a: NativeOutfitStatus["selection"],
  b: NativeOutfitStatus["selection"],
) {
  return !!a && !!b && lineupDisplayKey(a) === lineupDisplayKey(b);
}
// Automatic background List reads are invisible (0.53 flicker fix): while the
// worker is busy with one, the UI keeps showing the last settled status
// instead of "queued"/busy, so the player list does not disable and re-enable
// every second. Raw status still drives waits and the auto-random policy.
export function visibleOutfitStatus(
  raw: NativeOutfitStatus | null,
  settled: NativeOutfitStatus | null,
  backgroundRevision: number | null,
) {
  // Only the List's own "queued" state: match-carry ticks reuse the last
  // revision (e.g. busy "applying") and must stay visible.
  return raw &&
    raw.busy &&
    raw.phase === "queued" &&
    backgroundRevision !== null &&
    raw.revision === backgroundRevision
    ? (settled ?? raw)
    : raw;
}
export function useNativeOutfits(
  live: boolean,
  gameRoot: string,
  active: boolean,
) {
  const [session, setSession] = useState<OutfitSession | null>(null);
  const [status, setStatus] = useState<NativeOutfitStatus | null>(
    live ? null : previewOutfitStatus(),
  );
  const [binding, setBinding] = useState(live ? "" : "preview");
  const [sending, setSending] = useState(false),
    [error, setError] = useState(""),
    [pollError, setPollError] = useState("");
  const generation = useRef(0),
    sessionRef = useRef<string | undefined>(undefined),
    sessionStateRef = useRef<OutfitSession | null>(session),
    requests = useRef(new OutfitRequestQueue()),
    statusRef = useRef<NativeOutfitStatus | null>(null),
    lastLineup = useRef<NativeOutfitStatus["selection"]>(null),
    // Revision of the latest automatic (background) List, and the last status
    // shown before it; see visibleOutfitStatus.
    backgroundRevision = useRef<number | null>(null),
    settled = useRef<NativeOutfitStatus | null>(null),
    // Players the user fitted by hand this game; automatic random skips them.
    manual = useRef(new Set<string>());
  statusRef.current = status;
  sessionStateRef.current = session;
  const shown = visibleOutfitStatus(
    status,
    settled.current,
    backgroundRevision.current,
  );
  settled.current = shown;
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => {
    const g = ++generation.current;
    sessionRef.current = undefined;
    lastLineup.current = null;
    backgroundRevision.current = null;
    settled.current = null;
    requests.current = new OutfitRequestQueue();
    statusRef.current = null;
    sessionStateRef.current = null;
    setSession(null);
    setSending(false);
    setError("");
    setPollError("");
    setBinding(live ? "" : "preview");
    setStatus(live ? null : previewOutfitStatus());
    manual.current.clear();
    if (!live) return;
    let polling = false;
    async function poll() {
      if (polling) return;
      polling = true;
      try {
        const next = await call<OutfitSession>("native-session-status");
        if (g === generation.current) {
          const same = sessionRef.current === next.session;
          if (!same) {
            setBinding("");
            manual.current.clear();
            lastLineup.current = null;
            backgroundRevision.current = null;
            settled.current = null;
          }
          sessionRef.current = next.session;
          sessionStateRef.current = next;
          setSession(next);
          setPollError("");
          statusRef.current = mergeOutfitStatus(
            statusRef.current,
            next.nativeProvider?.outfits ?? null,
            same,
          );
          setStatus(statusRef.current);
        }
      } catch (e) {
        if (g === generation.current) setPollError(String(e));
      } finally {
        polling = false;
      }
    }
    void poll();
    let idleTicks = 0;
    const timer = setInterval(() => {
      if (activeRef.current || ++idleTicks % 10 === 0) void poll();
    }, 100);
    return () => {
      ++generation.current;
      clearInterval(timer);
    };
  }, [live, gameRoot]);
  // A fresh list may retain this player across clothing changes, but never
  // follows a reused slot, roster/model identity or a different game session.
  lastLineup.current = live
    ? retainedLineup(lastLineup.current, status)
    : (status?.selection ?? null);
  const lineup = lastLineup.current;
  useEffect(() => {
    if (!live || !lineup) return;
    const next = defaultOutfitBinding(lineup.players, binding);
    if (next !== binding) setBinding(next);
  }, [live, lineup, binding]);
  async function request(command: Command) {
    return (await send(command)) !== null;
  }
  // Resolves to the queued status, or null when nothing was queued.
  // background: an automatic lineup read; it never shows busy/queued, never
  // reports its own refusal (the next read retries) and never disables UI.
  async function send(
    command: Command,
    background = false,
  ): Promise<NativeOutfitStatus | null> {
    if (!live) {
      if (background) return previewOutfitStatus();
      setStatus({
        ...previewOutfitStatus(),
        message:
          command.action === "restore"
            ? "Dummy player's original outfit restored · no game writes"
            : "Dummy outfit preview updated · no game writes",
      });
      return previewOutfitStatus();
    }
    const current = sessionStateRef.current;
    if (!outfitsEligible(current)) {
      if (!background) setError(outfitUnavailableMessage(current));
      return null;
    }
    const g = generation.current,
      expected = current!.session,
      queue = requests.current;
    if (background && statusRef.current?.busy) return null;
    if (!background) {
      setSending(true);
      setError("");
    }
    try {
      return await queue.run(background, async () => {
        // A foreground reservation blocks further automatic Lists while the
        // in-flight List finishes. Its queued reply is not completion: wait for
        // the native worker too, without replaying any Apply after uncertainty.
        if (
          !outfitReplyCurrent(
            g,
            generation.current,
            expected,
            sessionRef.current,
          )
        )
          return null;
        const raw = statusRef.current;
        if (!background && raw?.busy) {
          const completed = await wait(raw.revision, 5000);
          if (
            !outfitReplyCurrent(
              g,
              generation.current,
              expected,
              sessionRef.current,
            )
          )
            return null;
          if (
            !completed ||
            completed.busy ||
            ["held", "ended"].includes(completed.phase)
          ) {
            setError(
              completed?.message ||
                "The previous outfit operation is still running; nothing new was sent. Try again when it finishes.",
            );
            return null;
          }
        }
        // Eligibility/identity may have changed while awaiting the read. Never
        // let a queued action leak into the next game or out of player selection.
        if (
          !outfitReplyCurrent(
            g,
            generation.current,
            expected,
            sessionRef.current,
          )
        )
          return null;
        if (!outfitsEligible(sessionStateRef.current)) {
          if (!background)
            setError(outfitUnavailableMessage(sessionStateRef.current));
          return null;
        }
        const result = await call<NativeOutfitStatus>("native-outfit-command", {
          gameRoot,
          session: expected,
          command,
          acknowledgement:
            command.action === "list"
              ? "read-blacktop-outfits"
              : "apply-one-outfit-for-current-game",
        });
        if (
          !outfitReplyCurrent(
            g,
            generation.current,
            expected,
            sessionRef.current,
          )
        )
          return null;
        // A background List's queued/busy replies stay hidden; a foreground
        // command is shown as usual.
        backgroundRevision.current =
          background && result.phase === "queued" ? result.revision : null;
        statusRef.current = mergeOutfitStatus(statusRef.current, result, true);
        setStatus(statusRef.current);
        return result;
      });
    } catch (e) {
      if (
        !background &&
        outfitReplyCurrent(g, generation.current, expected, sessionRef.current)
      )
        setError(String(e));
      return null;
    } finally {
      if (
        outfitReplyCurrent(g, generation.current, expected, sessionRef.current)
      ) {
        if (!background) setSending(queue.busy);
      }
    }
  }
  const player = lineup?.players.find((p) => p.binding === binding);
  // Waits for this request's own completed reply; null on timeout or session change.
  // Reads the provider directly every 50 ms instead of waiting for the
  // regular poll, so sequential Random all applies do not idle between players.
  async function wait(revision: number, timeoutMs = 20000) {
    const g = generation.current,
      expected = sessionRef.current,
      until = Date.now() + timeoutMs;
    while (g === generation.current && Date.now() < until) {
      let s = statusRef.current;
      try {
        const next = await call<OutfitSession>("native-session-status");
        if (g !== generation.current || next.session !== expected) return null;
        const incoming = next.nativeProvider?.outfits ?? null;
        if (next.session === expected && incoming) {
          s = mergeOutfitStatus(statusRef.current, incoming, true);
          statusRef.current = s;
          setStatus(s);
        }
      } catch {
        // The regular poll reports errors; keep waiting on the last status.
      }
      if (
        s &&
        (["held", "ended"].includes(s.phase) ||
          (s.revision === revision &&
            s.completedRevision >= revision &&
            !s.busy))
      )
        return s;
      await new Promise((r) => setTimeout(r, 50));
    }
    return null;
  }
  return {
    // What the UI shows: background List reads stay invisible.
    status: shown,
    player,
    binding,
    select: setBinding,
    error:
      error ||
      pollError ||
      (live &&
      session?.nativeProvider?.phase === "loader-active" &&
      !outfitBuildMapped(session.nativeProvider.build)
        ? session.nativeProvider.build?.version2OutfitCarryError ||
          "Outfit editing is disabled for this game build while updated mappings are pending. Mod loading is available separately."
        : ""),
    // User-visible busy: a foreground command or a shown busy status. An
    // automatic background read never toggles it (0.53 flicker fix).
    busy: sending || !!shown?.busy,
    // Any worker activity, background reads included.
    working: sending || requests.current.busy || !!status?.busy,
    eligible: !live || outfitsEligible(session),
    unavailable:
      live && session && !outfitsEligible(session)
        ? outfitUnavailableMessage(session)
        : "",
    scan: () => request({ action: "list" }),
    // Automatic lineup read (auto-random, dropdown open): invisible to the UI.
    refresh: async () => (await send({ action: "list" }, true)) !== null,
    // Last lineup read at this selection screen (see retainedLineup).
    lineup,
    apply: (outfit: Outfit) => {
      manual.current.add(binding);
      return request({ action: "apply", playerBinding: binding, outfit });
    },
    restore: () => {
      manual.current.add(binding);
      return request({ action: "restore", playerBinding: binding });
    },
    // One apply for any lineup player; resolves to its queued revision.
    queue: async (playerBinding: string, outfit: Outfit) =>
      (await send({ action: "apply", playerBinding, outfit }))?.revision ??
      null,
    // Random all: every slot in one native batch, one combined refresh wait.
    queueMany: async (
      items: { playerBinding: string; outfit: Outfit }[],
      automatic = false,
    ) => {
      const command = outfitBatchCommand(
        items,
        automatic,
        automaticOutfitsSupported(statusRef.current),
      );
      return command ? ((await send(command))?.revision ?? null) : null;
    },
    wait,
    manual: manual.current as ReadonlySet<string>,
    session: session?.session,
  };
}
// The overlay's PLAYER dropdown (WPF blacktop-player-menu.js): the live
// lineup from the outfit List command, two per row, the bound player checked.
// Slots the native reader refused stay listed, disabled, with the reason.
export function OverlayPlayerMenu({
  control,
  onChoose,
}: {
  control: ReturnType<typeof useNativeOutfits>;
  onChoose: (binding: string) => void;
}) {
  const selection = control.lineup ?? control.status?.selection;
  const players = selection?.players ?? [];
  const refused = (selection?.unavailable ?? []).map((slot) => ({
    slot,
    reason:
      selection?.unavailableReasons?.find((r) => r.slot === slot)?.reason ||
      "Player could not be read",
  }));
  const rows = [
    ...players.map((p) => ({ slot: p.slot, player: p, reason: "" })),
    ...refused.map((r) => ({ slot: r.slot, player: null, reason: r.reason })),
  ].sort((a, b) => a.slot - b.slot);
  if (!rows.length)
    return (
      <p className="overlay-player-empty muted" role="status">
        {control.busy
          ? "Reading selected players…"
          : control.error ||
            (control.status &&
            !["ready", "preview", "idle"].includes(control.status.phase)
              ? control.status.message
              : "No players to show. Select players in Blacktop, then reopen this list.")}
      </p>
    );
  return (
    <div
      className="overlay-player-menu"
      role="menu"
      aria-label="Blacktop players"
    >
      {rows.map(({ slot, player: p, reason }) =>
        p ? (
          <button
            key={p.binding}
            type="button"
            role="menuitemradio"
            aria-checked={p.binding === control.binding}
            // WPF blacktop-player-menu.js: items are disabled only when the
            // list is empty, never while an operation runs (no busy flicker).
            disabled={!p.pool}
            title={
              p.pool
                ? `Player ${p.slot + 1} · ${p.name}`
                : "Clothing pool could not be verified"
            }
            onClick={() => {
              control.select(p.binding);
              onChoose(p.binding);
            }}
          >
            {p.name || `Player ${p.slot + 1}`}
            {!p.pool && <small> · clothing pool not verified</small>}
          </button>
        ) : (
          <button
            key={`slot-${slot}`}
            type="button"
            role="menuitemradio"
            aria-checked={false}
            disabled
            title={reason}
          >
            Player {slot + 1} <small>· {reason}</small>
          </button>
        ),
      )}
    </div>
  );
}
export function NativeOutfitControls({
  live,
  control,
  outfit,
  restore,
}: {
  live: boolean;
  control: ReturnType<typeof useNativeOutfits>;
  outfit: Outfit;
  restore?: () => void;
}) {
  const players = control.status?.selection?.players ?? [];
  const canApply = canApplyOutfit(
    live,
    control.eligible,
    control.busy,
    control.status,
    control.player,
    outfit,
  );
  return (
    <div className="native-arena-controls native-outfit-controls">
      <div className="native-outfit-toolbar">
        <button
          disabled={control.busy || !control.eligible}
          onClick={() => void control.scan()}
        >
          Refresh players
        </button>
        <button
          className="primary"
          disabled={!canApply}
          onClick={() => void control.apply(outfit)}
        >
          Apply outfit
        </button>
        <button
          disabled={
            !control.player ||
            control.busy ||
            (live &&
              (!control.eligible || control.status?.boundary !== "selection"))
          }
          onClick={() => (restore ? restore() : void control.restore())}
        >
          Restore original
        </button>
      </div>
      <div className="native-outfit-players" aria-label="Blacktop players">
        {players.map((p) => (
          <button
            key={p.binding}
            disabled={control.busy || !p.pool}
            className={p.binding === control.binding ? "selected" : ""}
            title={`Player ${p.slot + 1} · ${p.name}`}
            onClick={() => control.select(p.binding)}
          >
            {p.slot + 1}. {p.name}
          </button>
        ))}
      </div>
      <p role="status" className="muted">
        {control.error ||
          control.unavailable ||
          control.status?.message ||
          "Open Blacktop player selection, then refresh players."}
      </p>
    </div>
  );
}
