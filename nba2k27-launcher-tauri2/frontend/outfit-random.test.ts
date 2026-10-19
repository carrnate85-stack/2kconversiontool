import { expect, test } from "vitest";
import type { NativeOutfitPlayer, NativeOutfitStatus } from "./NativeOutfits";
import {
  AutoRandomPolicy,
  applyRandomBatch,
  applyRandomSequence,
  autoRandomScanDelay,
  lineupSignature,
  randomAllTargets,
  describeRandomResult,
  randomApplyVerified,
  type RandomDeps,
  type RandomTarget,
} from "./outfit-random";
import { initialOutfit } from "./state";

const player = (slot: number, pool: "male" | "female" | null = "male") =>
  ({
    binding: String(slot).repeat(48),
    token: "t",
    slot,
    name: `P${slot + 1}`,
    cyberface: slot,
    pool,
  }) satisfies NativeOutfitPlayer;
function status(
  players: NativeOutfitPlayer[],
  extra: Partial<NativeOutfitStatus> = {},
): NativeOutfitStatus {
  return {
    phase: "ready",
    message: "Native Blacktop players refreshed.",
    revision: 1,
    completedRevision: 1,
    update: 1,
    busy: false,
    boundary: "selection",
    selection: {
      required: players.length,
      selected: players.length,
      players,
      unavailable: [],
    },
    ...extra,
  };
}
const verified = (revision: number, players: NativeOutfitPlayer[]) =>
  status(players, {
    revision,
    completedRevision: revision,
    message: "P · slot 1 · outfit verified; queued for this Blacktop match",
  });

const targetsOf = (s: NativeOutfitStatus): RandomTarget[] => {
  const found = randomAllTargets(s);
  if (typeof found === "string") throw Error(found);
  return found.targets;
};
test("random all includes every readable player and names each refused slot", () => {
  const four = [player(0), player(1), player(2), player(3)];
  expect(targetsOf(status(four))).toHaveLength(4);
  expect(typeof randomAllTargets(null)).toBe("string");
  expect(typeof randomAllTargets(status(four, { boundary: "match" }))).toBe(
    "string",
  );
  // A pool-less player and a native-refused slot are reported, not dropped.
  const partial = status([player(0), player(1), player(2, null)]);
  partial.selection!.unavailable = [3];
  partial.selection!.unavailableReasons = [
    { slot: 3, reason: "Unknown private outfit page; not adopted" },
  ];
  const found = randomAllTargets(partial);
  if (typeof found === "string") throw Error(found);
  expect(found.targets.map((t) => t.slot)).toEqual([0, 1]);
  expect(found.skipped.map((s) => [s.slot, s.reason])).toEqual([
    [2, "Clothing pool could not be verified"],
    [3, "Unknown private outfit page; not adopted"],
  ]);
  // Random all (strict, WPF blacktop-batch.js) refuses the whole batch and
  // names every unverified slot.
  const strict = randomAllTargets(partial, true);
  expect(strict).toContain("every selected player must have a verified clothing fit");
  expect(strict).toContain("4. Player 4: Unknown private outfit page; not adopted");
  expect(targetsOf(status(four))).toHaveLength(4);
  expect(typeof randomAllTargets(status(four), true)).not.toBe("string");
  const none = status([player(0, null)]);
  expect(randomAllTargets(none)).toContain("Clothing pool could not be verified");
  expect(lineupSignature(four)).toBe(lineupSignature([...four].reverse()));
});

test("only a completed, verified ready reply counts as applied", () => {
  const p = [player(0)];
  expect(randomApplyVerified(verified(2, p))).toBe(true);
  expect(randomApplyVerified({ ...verified(2, p), phase: "error" })).toBe(false);
  expect(randomApplyVerified({ ...verified(2, p), busy: true })).toBe(false);
  expect(randomApplyVerified({ ...verified(2, p), completedRevision: 1 })).toBe(
    false,
  );
  expect(randomApplyVerified(status(p))).toBe(false);
  expect(randomApplyVerified(null)).toBe(false);
});

function deps(
  players: NativeOutfitPlayer[],
  reply: (revision: number) => NativeOutfitStatus | null,
  log: string[],
): RandomDeps {
  let revision = 1;
  return {
    outfitFor: async (t) => ({ ...initialOutfit, pool: t.pool }),
    apply: async (binding) => {
      log.push(binding);
      return ++revision;
    },
    wait: async (r) => reply(r),
    signature: () => lineupSignature(players),
  };
}

test("random all applies one player at a time and stops on the first unverified result", async () => {
  const players = [player(0), player(1), player(2)];
  const targets = targetsOf(status(players));
  const log: string[] = [];
  const ok = await applyRandomSequence(
    targets,
    deps(players, (r) => verified(r, players), log),
  );
  expect(ok.stopped).toBeNull();
  expect(ok.applied).toEqual(players.map((p) => p.binding));
  expect(log).toEqual(players.map((p) => p.binding));

  const held: string[] = [];
  const stop = await applyRandomSequence(
    targets,
    deps(
      players,
      (r) =>
        r === 3
          ? { ...verified(r, players), phase: "held", message: "held" }
          : verified(r, players),
      held,
    ),
  );
  expect(stop.applied).toHaveLength(1);
  expect(held).toHaveLength(2);
  expect(stop.stopped).toContain("held");

  const timeout = await applyRandomSequence(
    targets,
    deps(players, () => null, []),
  );
  expect(timeout.applied).toHaveLength(0);
  expect(timeout.stopped).toContain("no verified outfit result");
});

test("random all stops when the lineup changes and when the queue refuses", async () => {
  const players = [player(0), player(1)];
  const targets = targetsOf(status(players));
  const swapped = [player(0), player(5)];
  const changed = await applyRandomSequence(
    targets,
    deps(players, (r) => verified(r, swapped), []),
  );
  expect(changed.applied).toHaveLength(1);
  expect(changed.stopped).toContain("lineup changed");

  const refused = await applyRandomSequence(targets, {
    ...deps(players, (r) => verified(r, players), []),
    apply: async () => null,
  });
  expect(refused.applied).toHaveLength(0);
  expect(refused.stopped).toContain("not queued");
});

test("automatic random waits for two distinct identical reads, skips manual and seen players", () => {
  const players = [player(0), player(1)];
  const policy = new AutoRandomPolicy();
  const s1 = status(players, { update: 5 });
  expect(policy.observe("g1", s1)).toEqual([]);
  // A re-render of the same reply is not a second observation.
  expect(policy.observe("g1", s1)).toEqual([]);
  // Busy/list-less replies neither confirm nor reset stability.
  expect(
    policy.observe("g1", { ...s1, update: 6, busy: true, selection: null }),
  ).toEqual([]);
  policy.mark(players[1].binding);
  const due = policy.observe("g1", status(players, { update: 7 }));
  expect(due.map((t) => t.binding)).toEqual([players[0].binding]);
  policy.mark(players[0].binding);
  expect(policy.observe("g1", status(players, { update: 8 }))).toEqual([]);
  expect(policy.seenAll(players)).toBe(true);
  // A new occupant needs its own stable pair of reads.
  const next = [player(0), player(3)];
  expect(policy.observe("g1", status(next, { update: 9 }))).toEqual([]);
  expect(
    policy.observe("g1", status(next, { update: 10 })).map((t) => t.slot),
  ).toEqual([3]);
  // Leaving selection resets stability; a new game session forgets seen players.
  expect(
    policy.observe("g1", status(next, { update: 11, boundary: "match" })),
  ).toEqual([]);
  expect(policy.observe("g2", status(next, { update: 1 }))).toEqual([]);
  expect(policy.observe("g2", status(next, { update: 2 }))).toHaveLength(2);
});

test("random all stops at the first unconfirmed player like WPF and names it", async () => {
  const players = [player(0), player(1), player(2), player(3)];
  const targets = targetsOf(status(players));
  const log: string[] = [];
  const result = await applyRandomSequence(
    targets,
    deps(
      players,
      (r) =>
        r === 4
          ? {
              ...verified(r, players),
              phase: "error",
              message: "Unsupported selected outfit count",
            }
          : verified(r, players),
      log,
    ),
  );
  // Players 1-2 applied, player 3 refused, player 4 never sent (no retry).
  expect(log).toHaveLength(3);
  expect(result.applied).toHaveLength(2);
  expect(result.stopped).toContain("Stopped after 2 confirmed outfits");
  const text = describeRandomResult(result, [], 4);
  expect(text).toContain("2 of 4 verified");
  expect(text).toContain("3. P3: Unsupported selected outfit count");
});

test("automatic random keeps retrying after a List refusal instead of stopping for good", () => {
  const p = [player(0)];
  // 0.49 live: phase error, boundary unknown, "Open Blacktop player selection".
  expect(
    autoRandomScanDelay(
      status(p, {
        phase: "error",
        boundary: "unknown",
        selection: null,
        message: "Open Blacktop player selection",
      }),
      false,
    ),
  ).toBe(1000);
  expect(autoRandomScanDelay(null, false)).toBe(100);
  expect(autoRandomScanDelay(status(p, { phase: "idle", boundary: "unknown" }), false)).toBe(100);
  expect(autoRandomScanDelay(status(p), false)).toBe(100);
  expect(autoRandomScanDelay(status(p), true)).toBe(1000);
  expect(autoRandomScanDelay(status(p), true, true)).toBe(100);
  expect(autoRandomScanDelay(status(p, { phase: "error", boundary: "unknown", selection: null }), false, true)).toBe(100);
  // In a match or menu, wait for the worker to report selection again.
  expect(
    autoRandomScanDelay(status(p, { phase: "waiting-for-selection", boundary: "menu" }), false),
  ).toBeNull();
});

test("random all composes every outfit in one pass before the first apply", async () => {
  const players = [player(0), player(1), player(2), player(3)];
  const targets = targetsOf(status(players));
  const events: string[] = [];
  const base = deps(players, (r) => verified(r, players), []);
  const result = await applyRandomSequence(targets, {
    ...base,
    outfitFor: async (t) => {
      events.push(`compose ${t.slot}`);
      return base.outfitFor(t);
    },
    apply: async (binding, o) => {
      events.push(`apply ${binding[0]}`);
      return base.apply(binding, o);
    },
  });
  expect(result.applied).toHaveLength(4);
  expect(events.slice(0, 4)).toEqual(["compose 0", "compose 1", "compose 2", "compose 3"]);
  expect(events.slice(4)).toEqual(["apply 0", "apply 1", "apply 2", "apply 3"]);
});

test("random all batch composes everything, sends ONE native batch and waits once", async () => {
  const players = [player(0), player(1), player(2), player(3)];
  const targets = targetsOf(status(players));
  const events: string[] = [];
  let waitedFor = 0;
  const result = await applyRandomBatch(targets, {
    outfitFor: async (t) => {
      events.push(`compose ${t.slot}`);
      return { ...initialOutfit, pool: t.pool };
    },
    applyMany: async (items) => {
      events.push(`batch ${items.length}`);
      return 7;
    },
    wait: async (revision, timeoutMs) => {
      events.push(`wait ${revision}`);
      waitedFor = timeoutMs;
      return {
        ...verified(revision, players),
        message: "4 outfits verified (4 queued for this Blacktop match): 1. P1, 2. P2, 3. P3, 4. P4 · write 40 ms, game refresh 180 ms",
      };
    },
  });
  expect(events).toEqual(["compose 0", "compose 1", "compose 2", "compose 3", "batch 4", "wait 7"]);
  expect(waitedFor).toBe(5000 + 3000 * 4);
  expect(result.applied).toHaveLength(4);
  expect(result.stopped).toBeNull();
  expect(describeRandomResult(result, [], 4)).toContain("4 of 4 verified");
});
test("random all batch sends nothing if any outfit cannot be composed, and reports an unconfirmed batch", async () => {
  const players = [player(0), player(1)];
  const targets = targetsOf(status(players));
  let sent = false;
  const refused = await applyRandomBatch(targets, {
    outfitFor: async (t) => {
      if (t.slot === 1) throw Error("No compatible outfit template");
      return { ...initialOutfit, pool: t.pool };
    },
    applyMany: async () => {
      sent = true;
      return 1;
    },
    wait: async () => null,
  });
  expect(sent).toBe(false);
  expect(refused.stopped).toContain("No compatible outfit template");
  const held = await applyRandomBatch(targets, {
    outfitFor: async (t) => ({ ...initialOutfit, pool: t.pool }),
    applyMany: async () => 3,
    wait: async (r) => ({ ...verified(r, players), phase: "held", message: "Player refresh was not confirmed within 6 seconds" }),
  });
  expect(held.applied).toHaveLength(0);
  expect(held.stopped).toContain("not confirmed within 6 seconds");
});

// 0.53 live: auto-random never fired at Blacktop selection although manual
// Random all worked. The replies below follow the native queue: each List is
// "queued" (busy) then "ready" with the lineup; a List at selection can fail
// (phase error, boundary unknown, no list) between two good reads.
test("0.53: List errors between two identical reads do not block automatic random", () => {
  const players = [player(0), player(1), player(2), player(3)];
  const policy = new AutoRandomPolicy();
  const error = (update: number) =>
    status(players, {
      update,
      phase: "error",
      boundary: "unknown",
      selection: null,
      message: "Open Blacktop player selection (game reports transition)",
    });
  const queued = (update: number) =>
    status(players, { update, phase: "queued", busy: true });
  expect(policy.observe("g", status(players, { update: 2 }))).toEqual([]);
  expect(policy.observe("g", queued(3))).toEqual([]);
  expect(policy.observe("g", error(4))).toEqual([]);
  expect(policy.observe("g", queued(5))).toEqual([]);
  // Same lineup again: every player is due (previously the error reset it).
  expect(policy.observe("g", status(players, { update: 6 }))).toHaveLength(4);
  // A different lineup still needs its own pair of reads.
  const other = [player(0), player(1), player(2), player(5)];
  const fresh = new AutoRandomPolicy();
  fresh.observe("g", status(players, { update: 1 }));
  expect(fresh.observe("g", status(other, { update: 2 }))).toEqual([]);
  expect(fresh.observe("g", status(other, { update: 3 }))).toHaveLength(4);
});

test("0.53: a returning lineup is randomized again on every selection visit; manual still wins", () => {
  const players = [player(0), player(1)];
  const policy = new AutoRandomPolicy();
  policy.observe("g", status(players, { update: 1 }));
  expect(policy.observe("g", status(players, { update: 2 }))).toHaveLength(2);
  for (const p of players) policy.mark(p.binding);
  expect(policy.observe("g", status(players, { update: 3 }))).toEqual([]);
  // Match, then back to selection with the same native bindings (same session).
  policy.observe("g", status(players, { update: 4, boundary: "match", selection: null }));
  // The caller re-marks manual choices on every observation.
  policy.mark(players[1].binding);
  expect(policy.observe("g", status(players, { update: 5 }))).toEqual([]);
  policy.mark(players[1].binding);
  expect(
    policy.observe("g", status(players, { update: 6 })).map((t) => t.slot),
  ).toEqual([0]);
});

test("0.53: a re-minted binding for the same player and slot is not randomized twice", () => {
  const before = [player(0), player(1)];
  const policy = new AutoRandomPolicy();
  policy.observe("g", status(before, { update: 1 }));
  policy.observe("g", status(before, { update: 2 }));
  for (const p of before) policy.mark(p.binding);
  // A clothing refresh rebuilt slot 1's preview model: new binding, same player.
  const after = [player(0), { ...player(1), binding: "f".repeat(48) }];
  policy.observe("g", status(after, { update: 3 }));
  expect(policy.observe("g", status(after, { update: 4 }))).toEqual([]);
  expect(policy.seenAll(after)).toBe(true);
  // A different player in that slot is new.
  const swapped = [player(0), { ...player(1), binding: "e".repeat(48), name: "New" }];
  policy.observe("g", status(swapped, { update: 5 }));
  expect(
    policy.observe("g", status(swapped, { update: 6 })).map((t) => t.name),
  ).toEqual(["New"]);
});

test("0.53: a refused (never queued) automatic batch may retry; rearming is not a fresh read", () => {
  const players = [player(0)];
  const policy = new AutoRandomPolicy();
  policy.observe("g", status(players, { update: 1 }));
  // Rearming re-publishes the old list; it does not confirm stability.
  expect(policy.observe("g", status(players, { update: 2, phase: "rearming" }))).toEqual([]);
  expect(policy.observe("g", status(players, { update: 3 }))).toHaveLength(1);
  policy.mark(players[0].binding);
  policy.unmark(players[0].binding);
  expect(policy.observe("g", status(players, { update: 4 }))).toHaveLength(1);
});


test("random all starts independent composition together and preserves slot order before one write", async () => {
  const players = [player(0), player(1), player(2)];
  const targets = targetsOf(status(players));
  const started: number[] = [];
  const finish: ((value: typeof initialOutfit) => void)[] = [];
  const batches: string[][] = [];
  const pending = applyRandomBatch(targets, {
    outfitFor: (target) => {
      started.push(target.slot);
      return new Promise((resolve) => { finish[target.slot] = resolve; });
    },
    applyMany: async (items) => {
      batches.push(items.map((item) => item.playerBinding));
      return 1;
    },
    wait: async (revision) => verified(revision, players),
  });
  await Promise.resolve();
  expect(started).toEqual([0, 1, 2]);
  finish[2]({ ...initialOutfit });
  finish[0]({ ...initialOutfit });
  await Promise.resolve();
  expect(batches).toEqual([]);
  finish[1]({ ...initialOutfit });
  expect((await pending).stopped).toBeNull();
  expect(batches).toEqual([targets.map((target) => target.binding)]);
});

test("random all drains failed compositions and reports the first lineup failure without a write", async () => {
  const players = [player(0), player(1)];
  const targets = targetsOf(status(players));
  let resolveFirst!: (value: typeof initialOutfit) => void;
  let sent = false;
  let completed = false;
  const pending = applyRandomBatch(targets, {
    outfitFor: (target) => target.slot === 0
      ? new Promise((resolve) => { resolveFirst = resolve; })
      : Promise.reject(Error("second slot failed")),
    applyMany: async () => { sent = true; return 1; },
    wait: async () => null,
  }).then((result) => { completed = true; return result; });
  await Promise.resolve();
  await Promise.resolve();
  expect(completed).toBe(false);
  resolveFirst({ ...initialOutfit });
  const result = await pending;
  expect(result.stopped).toContain("second slot failed");
  expect(sent).toBe(false);
});
