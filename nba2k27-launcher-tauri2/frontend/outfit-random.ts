// Random player / Random all players / automatic random at selection.
// Pure orchestration over the existing List/Apply/Restore outfit queue: one
// apply at a time (the native request expires after 3 s), each waiting for its
// own verified result before the next, as WPF's blacktop-batch.js did.
import type { NativeOutfitPlayer, NativeOutfitStatus } from "./NativeOutfits";
import type { Outfit } from "./types";

export type RandomMode = "clothes" | "accessories" | "both";
// WPF's Random player menu (blacktop.js). Team clothes needs the player's team
// identity, which the V2 list does not carry yet, so it is not offered.
export const randomPlayerModes: readonly (readonly [RandomMode, string])[] = [
  ["clothes", "Random streetwear"],
  ["both", "Random all"],
  ["accessories", "Random accessories"],
];
export type RandomTarget = {
  binding: string;
  slot: number;
  name: string;
  pool: "male" | "female";
};
// A lineup slot that cannot be randomized, with the native reason.
export type RandomSkip = { slot: number; name: string; reason: string };
export function lineupSignature(
  players: readonly NativeOutfitPlayer[] | null | undefined,
) {
  return players?.length
    ? players
        .map((p) => p.binding)
        .sort()
        .join("|")
    : "";
}
// Readable selection players with a clothing pool are targets; every other
// occupied slot is reported with its native reason.
// strict (Random all, WPF blacktop-batch.js): refuse unless EVERY player has a
// verified pool. Lenient (automatic, WPF autoRandomSelection): skip the
// unavailable player and continue.
export function randomAllTargets(
  status: NativeOutfitStatus | null,
  strict = false,
): { targets: RandomTarget[]; skipped: RandomSkip[] } | string {
  const s = status?.selection;
  if (status?.boundary !== "selection" || !s)
    return "Refresh players at Blacktop player selection first.";
  const skipped: RandomSkip[] = s.unavailable.map((slot) => ({
    slot,
    name: `Player ${slot + 1}`,
    reason:
      s.unavailableReasons?.find((r) => r.slot === slot)?.reason ||
      "Player could not be read",
  }));
  const targets: RandomTarget[] = [];
  for (const { binding, slot, name, pool } of s.players) {
    if (pool) targets.push({ binding, slot, name, pool });
    else
      skipped.push({
        slot,
        name,
        reason: "Clothing pool could not be verified",
      });
  }
  skipped.sort((a, b) => a.slot - b.slot);
  if (strict && skipped.length)
    return `Refresh players; every selected player must have a verified clothing fit · ${describeSkips(skipped)}`;
  if (!targets.length)
    return skipped.length
      ? `No player can be randomized · ${describeSkips(skipped)}`
      : "No Blacktop players are selected yet.";
  return { targets, skipped };
}
export function describeSkips(skipped: readonly RandomSkip[]) {
  return skipped
    .map((s) => `${s.slot + 1}. ${s.name}: ${s.reason}`)
    .join(" · ");
}
// Only a ready reply that the native worker verified counts as applied.
export function randomApplyVerified(s: NativeOutfitStatus | null) {
  return (
    !!s &&
    s.phase === "ready" &&
    !s.busy &&
    s.completedRevision >= s.revision &&
    /verified/i.test(s.message)
  );
}
export type RandomDeps = {
  outfitFor: (target: RandomTarget) => Promise<Outfit>;
  // Resolves to the queued revision, or null when the queue refused it.
  apply: (binding: string, outfit: Outfit) => Promise<number | null>;
  wait: (revision: number) => Promise<NativeOutfitStatus | null>;
  // Current lineup signature (null while a refresh hides the list); a
  // change stops the batch.
  signature: () => string | null;
  onStep?: (target: RandomTarget, index: number, total: number) => void;
  cancelled?: () => boolean;
};
export type RandomOutcome = {
  target: RandomTarget;
  ok: boolean;
  reason: string;
};
export type RandomResult = {
  applied: string[];
  outcomes: RandomOutcome[];
  stopped: string | null;
  last: NativeOutfitStatus | null;
};
export async function applyRandomSequence(
  targets: readonly RandomTarget[],
  deps: RandomDeps,
): Promise<RandomResult> {
  const start = deps.signature();
  const changed = () => {
    const now = deps.signature();
    return start !== null && now !== null && now !== start;
  };
  const applied: string[] = [];
  const outcomes: RandomOutcome[] = [];
  let last: NativeOutfitStatus | null = null;
  const done = (stopped: string | null) => ({
    applied,
    outcomes,
    stopped,
    last,
  });
  // One pass first: every player's random outfit is composed before any
  // apply (launcher-only work, no game access), then slots apply back to back.
  const prepared: ({ outfit: Outfit } | { error: string })[] = [];
  for (const target of targets) {
    try {
      prepared.push({ outfit: await deps.outfitFor(target) });
    } catch (e) {
      prepared.push({ error: String(e) });
    }
  }
  for (const [i, target] of targets.entries()) {
    if (deps.cancelled?.()) return done("Random outfits cancelled.");
    if (changed())
      return done("Blacktop lineup changed; random outfits stopped.");
    deps.onStep?.(target, i, targets.length);
    const ready = prepared[i];
    if ("error" in ready) {
      // WPF stops at the first player it cannot resolve; nothing was sent.
      outcomes.push({ target, ok: false, reason: ready.error });
      return done(`${target.name}: ${ready.error}; random outfits stopped.`);
    }
    const outfit = ready.outfit;
    const revision = await deps.apply(target.binding, outfit);
    if (revision === null) {
      outcomes.push({
        target,
        ok: false,
        reason: "the outfit request was not queued",
      });
      return done(`${target.name}: the outfit request was not queued.`);
    }
    last = await deps.wait(revision);
    if (randomApplyVerified(last)) {
      applied.push(target.binding);
      outcomes.push({ target, ok: true, reason: last!.message });
    } else {
      const reason = last?.message || "no verified outfit result";
      outcomes.push({ target, ok: false, reason });
      // WPF blacktop-batch.js / autoRandomSelection: stop at the first
      // unconfirmed player; nothing is retried.
      return done(`Stopped after ${applied.length} confirmed outfits; ${target.name}: ${reason}`);
    }
    // A verified reply carries a fresh list; a different lineup stops here.
    if (
      start !== null &&
      last?.selection &&
      lineupSignature(last.selection.players) !== start
    )
      return done("Blacktop lineup changed; random outfits stopped.");
  }
  return done(null);
}
// One line per player for the overlay status: what applied, what refused, why.
export function describeRandomResult(
  result: RandomResult,
  skipped: readonly RandomSkip[],
  total: number,
) {
  const parts = [
    `${result.applied.length} of ${total} verified`,
    ...result.outcomes
      .filter((o) => !o.ok)
      .map((o) => `${o.target.slot + 1}. ${o.target.name}: ${o.reason}`),
    ...skipped.map((s) => `${s.slot + 1}. ${s.name}: ${s.reason}`),
  ];
  if (result.stopped) parts.push(result.stopped);
  return parts.join(" · ");
}
// WPF autoRandomSelection: once the same lineup is seen twice, each new
// occupant without a manual choice gets one random apply. The runner marks a
// player seen right before its write, so an uncertain result never re-applies.
//
// 0.53 live fixes (auto-random never fired at selection):
// - Stability needs the same lineup in two distinct List replies. Replies in
//   between that carry no lineup (queued, refreshing, a transient List error
//   at selection, rearming without a list) neither confirm nor reset it; only
//   a DIFFERENT lineup or leaving selection does. Previously every error reply
//   reset it, and at selection List errors arrive about once a second, so two
//   consecutive good reads rarely happened.
// - Seen players are per selection visit (WPF identities were recreated on
//   every return to selection): a match or menu boundary forgets them, so a
//   returning lineup is randomized again. Manual choices are re-marked by the
//   caller on every observation, so they still win.
// - A player handled this visit stays handled when its binding is re-minted
//   (the native key includes the preview model, which a clothing refresh can
//   replace): the same name in the same slot is not randomized twice.
// - unmark(): a batch the queue refused wrote nothing, so it may retry.
export class AutoRandomPolicy {
  private session: string | undefined;
  private seen = new Set<string>();
  // slot:name of players handled this visit (see binding re-mint above).
  private handled = new Set<string>();
  private lineup = new Map<string, string>();
  private previous = "";
  private update = -1;
  private static alias(p: { slot: number; name: string }) {
    return `${p.slot}:${p.name}`;
  }
  private forget() {
    this.seen.clear();
    this.handled.clear();
    this.lineup.clear();
    this.previous = "";
  }
  // Manual applies win; automatic applies call this before their write.
  mark(binding: string) {
    this.seen.add(binding);
    const alias = this.lineup.get(binding);
    if (alias) this.handled.add(alias);
  }
  // Nothing was written (the queue refused the batch): eligible again.
  unmark(binding: string) {
    this.seen.delete(binding);
    const alias = this.lineup.get(binding);
    if (alias) this.handled.delete(alias);
  }
  private isSeen(p: { binding: string; slot: number; name: string }) {
    return this.seen.has(p.binding) || this.handled.has(AutoRandomPolicy.alias(p));
  }
  seenAll(players: readonly NativeOutfitPlayer[]) {
    return players.every((p) => !p.pool || this.isSeen(p));
  }
  // Returns the players to randomize now, or [] when nothing is due.
  observe(
    session: string | undefined,
    status: NativeOutfitStatus | null,
  ): RandomTarget[] {
    if (session !== this.session) {
      this.session = session;
      this.forget();
      this.update = -1;
    }
    // Two observations means two distinct replies, not two renders.
    if (!status || status.update === this.update) return [];
    this.update = status.update;
    // Leaving selection ends this visit; the next one randomizes again.
    if (["match", "menu", "ended"].includes(status.boundary)) {
      this.forget();
      return [];
    }
    // Replies without a fresh lineup neither confirm nor reset stability
    // ("rearming" re-publishes the previous list; it is not a new read).
    if (
      status.boundary !== "selection" ||
      status.phase !== "ready" ||
      status.busy ||
      !status.selection
    )
      return [];
    const found = randomAllTargets(status);
    if (typeof found === "string") return [];
    const targets = found.targets;
    const signature = lineupSignature(status.selection.players);
    const stable = signature === this.previous;
    this.previous = signature;
    this.lineup = new Map(
      status.selection.players.map((p) => [p.binding, AutoRandomPolicy.alias(p)]),
    );
    for (const key of [...this.seen])
      if (!this.lineup.has(key)) this.seen.delete(key);
    const aliases = new Set(this.lineup.values());
    for (const key of [...this.handled])
      if (!aliases.has(key)) this.handled.delete(key);
    // Seen bindings carry their alias forward (manual marks included).
    for (const [binding, alias] of this.lineup)
      if (this.seen.has(binding)) this.handled.add(alias);
    if (!stable) return [];
    return targets.filter((t) => !this.isSeen(t));
  }
}
// When automatic random should issue its next read-only List, in ms, or null
// for no scan. A plain List refusal (boundary "unknown", e.g. selection still
// settling on entry) keeps retrying; outside selection it waits for the
// worker to report selection again. Held/ended sessions are never eligible.
// WPF cadence: 100 ms selection ticks, act on two identical consecutive
// lineups, retry the same lineup at most once per second. Each Rust List is a
// queued native read. Match the 100 ms selection cadence while still requiring
// two distinct settled replies before automatic randomization.
// Keep observing established lineups at that cadence while the bar is visible:
// a new occupant must not wait a full second before appearing in the dropdown.
export function autoRandomScanDelay(
  status: NativeOutfitStatus | null,
  settled: boolean,
  visible = false,
): number | null {
  if (!status || status.phase === "idle") return 100;
  if (status.phase === "error") return visible ? 100 : 1000;
  if (status.boundary !== "selection") return null;
  return settled && !visible ? 1000 : 100;
}

export type RandomBatchDeps = {
  outfitFor: (target: RandomTarget) => Promise<Outfit>;
  // Queues every slot as one native batch; the queued revision, or null.
  applyMany: (
    items: { playerBinding: string; outfit: Outfit }[],
  ) => Promise<number | null>;
  wait: (revision: number, timeoutMs: number) => Promise<NativeOutfitStatus | null>;
  onStep?: (text: string) => void;
};
// Random all (0.51 speed): compose every outfit in one pass, write all slots
// back to back in ONE native batch, then wait once. The worker still journals,
// writes and reads back each slot separately; an unconfirmed slot holds the
// session (no retry), as the sequential path did.
export async function applyRandomBatch(
  targets: readonly RandomTarget[],
  deps: RandomBatchDeps,
): Promise<RandomResult> {
  const outcomes: RandomOutcome[] = [];
  const items: { playerBinding: string; outfit: Outfit }[] = [];
  // Composition only reads immutable catalog data; dispatch independent
  // requests together rather than adding one IPC round trip per player.
  // Drain every result before applying, retaining lineup order and the first
  // failed player. A failure still sends no native batch and never retries.
  const prepared = await Promise.allSettled(
    targets.map((target) => Promise.resolve().then(() => deps.outfitFor(target))),
  );
  for (const [index, target] of targets.entries()) {
    const result = prepared[index];
    if (result.status === "rejected") {
      outcomes.push({ target, ok: false, reason: String(result.reason) });
      return { applied: [], outcomes, stopped: `${target.name}: ${String(result.reason)}; random outfits stopped.`, last: null };
    }
    items.push({ playerBinding: target.binding, outfit: result.value });
  }
  deps.onStep?.(`Writing ${items.length} outfits…`);
  const revision = await deps.applyMany(items);
  if (revision === null)
    return { applied: [], outcomes, stopped: "The outfit request was not queued.", last: null };
  const last = await deps.wait(revision, 5000 + 3000 * items.length);
  if (randomApplyVerified(last)) {
    for (const target of targets)
      outcomes.push({ target, ok: true, reason: last!.message });
    return { applied: targets.map((t) => t.binding), outcomes, stopped: null, last };
  }
  const reason = last?.message || "no verified outfit result";
  return {
    applied: [],
    outcomes,
    stopped: `Random outfits not confirmed: ${reason}`,
    last,
  };
}
