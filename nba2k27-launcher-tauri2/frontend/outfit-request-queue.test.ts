import { expect, test } from "vitest";
import { OutfitRequestQueue } from "./outfit-request-queue";
import { AutoRandomPolicy, applyRandomBatch } from "./outfit-random";
import { previewOutfitStatus, visibleOutfitStatus } from "./NativeOutfits";
import { initialOutfit } from "./state";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

test("foreground outfit requests reserve priority over an in-flight refresh, never silently drop", async () => {
  const queue = new OutfitRequestQueue();
  const read = deferred<number>();
  const events: string[] = [];
  const refresh = queue.run(true, () => {
    events.push("refresh");
    return read.promise;
  });
  const apply = queue.run(false, async () => {
    events.push("apply");
    return 8;
  });
  // A polling tick after the click must not renew the read ahead of Apply.
  expect(
    await queue.run(true, async () => {
      events.push("unexpected read");
      return 9;
    }),
  ).toBeNull();
  expect(queue.busy).toBe(true);
  await Promise.resolve();
  expect(events).toEqual(["refresh"]);
  read.resolve(7);
  expect(await refresh).toBe(7);
  expect(await apply).toBe(8);
  expect(events).toEqual(["refresh", "apply"]);
  expect(queue.busy).toBe(false);
});

test("a double-click is explicitly refused and a failed read never poisons the reserved action", async () => {
  const queue = new OutfitRequestQueue();
  const read = deferred<number>();
  const refresh = queue.run(true, () => read.promise);
  const first = queue.run(false, async () => 1);
  const second = queue.run(false, async () => 2);
  const duplicate = expect(second).rejects.toThrow(
    "An outfit operation is already running.",
  );
  const rejected = expect(refresh).rejects.toThrow("read failed");
  read.reject(new Error("read failed"));
  await rejected;
  await duplicate;
  expect(await first).toBe(1);
  expect(queue.busy).toBe(false);
});

test("failed foreground writes are not retried; the next explicit request can proceed", async () => {
  const queue = new OutfitRequestQueue();
  let writes = 0;
  await expect(
    queue.run(false, async () => {
      writes++;
      throw Error("uncertain write");
    }),
  ).rejects.toThrow("uncertain write");
  expect(writes).toBe(1);
  expect(await queue.run(true, async () => "read only")).toBe("read only");
  expect(writes).toBe(1);
});

test("0.53 live sequence: queued/ready idle reads plus refresh in flight still dispatch automatic batch once", async () => {
  const queue = new OutfitRequestQueue();
  const policy = new AutoRandomPolicy();
  const sample = previewOutfitStatus();
  const players = [0, 1, 2, 3].map((slot) => ({
    ...sample.selection!.players[0],
    binding: `player-${slot}`,
    slot,
    name: `P${slot}`,
  }));
  const ready = {
    ...sample,
    phase: "ready",
    update: 2,
    revision: 1,
    completedRevision: 1,
    selection: { ...sample.selection!, required: 4, selected: 4, players },
  };
  expect(policy.observe("game", ready)).toEqual([]);
  const queued = {
    ...ready,
    phase: "queued",
    update: 3,
    revision: 2,
    busy: true,
  };
  expect(visibleOutfitStatus(queued, ready, 2)).toBe(ready);
  expect(policy.observe("game", queued)).toEqual([]);
  const second = { ...ready, update: 4, revision: 2, completedRevision: 2 };
  const due = policy.observe("game", second);
  expect(due).toHaveLength(4);

  // Another background IPC has started before the effect composes its batch.
  const readIpc = deferred<number>();
  const nativeReadDone = deferred<void>();
  const refresh = queue.run(true, () => readIpc.promise);
  let writes = 0;
  const result = applyRandomBatch(due, {
    outfitFor: async () => initialOutfit,
    applyMany: (items) =>
      queue.run(false, async () => {
        // Same native completion wait used by useNativeOutfits.send.
        await nativeReadDone.promise;
        for (const item of items) policy.mark(item.playerBinding);
        writes++;
        return 4;
      }),
    wait: async () => ({
      ...second,
      update: 8,
      revision: 4,
      completedRevision: 4,
      message: "4 outfits verified",
    }),
  });
  // Finish composing so the foreground reservation is installed.
  for (let i = 0; i < 8; i++) await Promise.resolve();
  expect(await queue.run(true, async () => 99)).toBeNull();
  readIpc.resolve(3);
  await refresh;
  expect(writes).toBe(0); // queued read is NOT native completion
  nativeReadDone.resolve();
  expect((await result).applied).toHaveLength(4);
  expect(writes).toBe(1);
  expect(policy.observe("game", { ...second, update: 10 })).toEqual([]);
});
