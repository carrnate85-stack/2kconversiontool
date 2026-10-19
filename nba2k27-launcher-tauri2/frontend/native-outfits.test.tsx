import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, test } from "vitest";
import {
  canApplyOutfit,
  outfitBuildMapped,
  mergeOutfitStatus,
  outfitsEligible,
  outfitReplyCurrent,
  outfitUnavailableMessage,
  previewOutfitStatus,
  NativeOutfitControls,
  OverlayPlayerMenu,
  defaultOutfitBinding,
  retainedLineup,
  lineupDisplayKey,
  visibleOutfitStatus,
  useNativeOutfits,
  selectedPlayerOutfit,
} from "./NativeOutfits";
import type { NativeOutfitStatus } from "./NativeOutfits";
import { initialOutfit } from "./state";
const session = {
  phase: "game-running",
  session: "a".repeat(48),
  nativeProvider: {
    phase: "loader-active",
    build: { nativeWritesMapped: true },
    outfits: { ...previewOutfitStatus(), phase: "ready" },
  },
};
test("live outfits require current provider, selection, pool and no pending work (no confirmation gate)", () => {
  const status = previewOutfitStatus(),
    player = status.selection!.players[0];
  expect(outfitsEligible(session)).toBe(true);
  for (const bad of [
    { ...session, nativeProvider: { phase: "loader-active" } },
    {
      ...session,
      nativeProvider: {
        phase: "loader-active",
        build: { label: "Version 2", nativeWritesMapped: false },
      },
    },
    { ...session, phase: "closed" },
    { ...session, error: "held" },
    { ...session, session: "short" },
    { ...session, nativeProvider: { phase: "loader-armed" } },
    {
      ...session,
      nativeProvider: {
        ...session.nativeProvider,
        outfits: { ...status, phase: "held" },
      },
    },
  ])
    expect(outfitsEligible(bad)).toBe(false);
  expect(canApplyOutfit(true, true, false, status, player, initialOutfit)).toBe(
    true,
  );
  expect(
    canApplyOutfit(true, false, false, status, player, initialOutfit),
  ).toBe(false);
  expect(canApplyOutfit(true, true, true, status, player, initialOutfit)).toBe(
    false,
  );
  expect(
    canApplyOutfit(
      true,
      true,
      false,
      { ...status, boundary: "match" },
      player,
      initialOutfit,
    ),
  ).toBe(false);
  expect(
    canApplyOutfit(
      true,
      true,
      false,
      status,
      { ...player, pool: "female" },
      initialOutfit,
    ),
  ).toBe(false);
  expect(
    canApplyOutfit(true, true, false, status, undefined, initialOutfit),
  ).toBe(false);
});
test("Version 2 outfits require their independent provider capability, not another feature gate", () => {
  const build = {
    label: "Version 2",
    modProviderMapped: true,
    nativeWritesMapped: false,
    version2OutfitCarryMapped: true,
  };
  expect(outfitBuildMapped(build)).toBe(true);
  expect(
    outfitsEligible({
      ...session,
      nativeProvider: { ...session.nativeProvider, build },
    }),
  ).toBe(true);
  for (const unsupported of [
    undefined,
    { ...build, version2OutfitCarryMapped: false },
    { ...build, modProviderMapped: false },
    { ...build, label: "Unknown" },
    { label: "Version 2", modProviderMapped: true },
    { label: "Version 2", version2OutfitCarryMapped: true },
    {
      label: "Version 2",
      modProviderMapped: true,
      version2ArenaBallMapped: true,
    },
    {
      label: "Version 2",
      modProviderMapped: true,
      version2ScoreboardMapped: true,
    },
  ])
    expect(outfitBuildMapped(unsupported)).toBe(false);
  expect(build.nativeWritesMapped).toBe(false);
  expect(outfitBuildMapped({ nativeWritesMapped: true })).toBe(true);
});
test("old polls and replies cannot replace a newer request or cross game generations", () => {
  const old = { ...previewOutfitStatus(), update: 10, revision: 2 };
  const incoming = { ...old, update: 9, revision: 1 };
  expect(mergeOutfitStatus(old, incoming, true)).toBe(old);
  expect(mergeOutfitStatus(old, incoming, false)).toBe(incoming);
  expect(
    mergeOutfitStatus(old, { ...incoming, phase: "held" }, true)?.phase,
  ).toBe("held");
  expect(outfitReplyCurrent(1, 1, "a", "a")).toBe(true);
  expect(outfitReplyCurrent(1, 2, "a", "a")).toBe(false);
  expect(outfitReplyCurrent(1, 1, "a", "b")).toBe(false);
});
test("offline example retains identical outfit actions and escapes player names/status", () => {
  const status = previewOutfitStatus();
  status.selection!.players[0].name = "<script>player</script>";
  status.message = "<b>untrusted</b>";
  const control = {
    status,
    player: status.selection!.players[0],
    binding: "preview",
    select: () => {},
    error: "",
    unavailable: "",
    busy: false,
    working: false,
    eligible: true,
    scan: async () => true,
    refresh: async () => true,
    apply: async () => true,
    restore: async () => true,
    queue: async () => null,
    queueMany: async () => null,
    lineup: null,
    wait: async () => null,
    manual: new Set<string>(),
    session: undefined,
  } as ReturnType<typeof useNativeOutfits>;
  const html = renderToStaticMarkup(
    <NativeOutfitControls
      live={false}
      control={control}
      outfit={initialOutfit}
    />,
  );
  expect(html).toContain("Apply outfit");
  expect(html).toContain("Restore original");
  expect(html).toContain("Refresh players");
  expect(html).not.toContain('type="checkbox"');
  expect(html).not.toMatch(/Enable outfit changes/);
  expect(html).toContain("&lt;script&gt;player&lt;/script&gt;");
  expect(html).not.toContain("<script>");
  expect(html).toContain('role="status"');
});

const lineupPlayer = (
  slot: number,
  pool: "male" | "female" | null = "male",
) => ({
  binding: String(slot).repeat(48),
  token: "t",
  slot,
  name: `<b>P${slot + 1}</b>`,
  cyberface: slot,
  pool,
});
test("player dropdown defaults to the first readable player and keeps a current choice", () => {
  const a = lineupPlayer(0, null),
    b = lineupPlayer(1),
    c = lineupPlayer(2);
  expect(defaultOutfitBinding([a, b, c], "")).toBe(b.binding);
  expect(defaultOutfitBinding([a, b, c], c.binding)).toBe(c.binding);
  // A player that left the lineup falls back, never to a pool-less slot.
  expect(defaultOutfitBinding([a, c], b.binding)).toBe(c.binding);
  expect(defaultOutfitBinding([a], "")).toBe("");
  expect(defaultOutfitBinding([], b.binding)).toBe("");
});

test("selected-player hydration reads each player's verified worn outfit, not a same-pool template", () => {
  const first = {
    ...lineupPlayer(0),
    currentOutfit: {
      ...initialOutfit,
      top: "First shirt",
      accessories: ["hat"],
    },
  };
  const second = {
    ...lineupPlayer(1),
    currentOutfit: { ...initialOutfit, top: "Second shirt", accessories: [] },
  };
  expect(selectedPlayerOutfit(first)?.top).toBe("First shirt");
  expect(selectedPlayerOutfit(second)?.top).toBe("Second shirt");
  // Editing the local draft cannot mutate a retained native snapshot.
  selectedPlayerOutfit(first)!.accessories.push("glasses");
  expect(first.currentOutfit.accessories).toEqual(["hat"]);
  expect(selectedPlayerOutfit(lineupPlayer(2))).toBeNull();
  expect(selectedPlayerOutfit({ ...first, pool: "female" })).toBeNull();
  expect(selectedPlayerOutfit(undefined)).toBeNull();
});
test("player dropdown lists provider lineup players, checks the bound one and names refused slots", () => {
  const status = {
    ...previewOutfitStatus(),
    selection: {
      required: 4,
      selected: 4,
      players: [lineupPlayer(0), lineupPlayer(1, null), lineupPlayer(3)],
      unavailable: [2],
      unavailableReasons: [
        {
          slot: 2,
          reason: "Loaded wardrobe does not match a verified CLOF layout",
        },
      ],
    },
  };
  const control = {
    status,
    binding: lineupPlayer(0).binding,
    busy: false,
    eligible: true,
    select: () => {},
    scan: async () => true,
  } as unknown as ReturnType<typeof useNativeOutfits>;
  const html = renderToStaticMarkup(
    <OverlayPlayerMenu control={control} onChoose={() => {}} />,
  );
  expect(html.match(/role="menuitemradio"/g)).toHaveLength(4);
  expect(html.match(/aria-checked="true"/g)).toHaveLength(1);
  expect(html).toContain("&lt;b&gt;P1&lt;/b&gt;");
  expect(html).toContain("clothing pool not verified");
  expect(html).toContain(
    "Player 3 <small>· Loaded wardrobe does not match a verified CLOF layout",
  );
  // Slot order follows the game lineup, refused slot included.
  expect(html.indexOf("Player 3")).toBeLessThan(html.indexOf("P4"));
  const empty = renderToStaticMarkup(
    <OverlayPlayerMenu
      control={{ ...control, status: { ...status, selection: null } }}
      onChoose={() => {}}
    />,
  );
  expect(empty).not.toContain("menuitemradio");
  expect(empty).toContain("No players to show");
});
test("an ineligible game explains itself in plain text instead of a confirmation", () => {
  expect(outfitUnavailableMessage(null)).toContain("while the game launched");
  expect(
    outfitUnavailableMessage({
      ...session,
      nativeProvider: { phase: "loader-armed" },
    }),
  ).toContain("native provider is active");
  expect(
    outfitUnavailableMessage({
      ...session,
      nativeProvider: { phase: "loader-active", build: { label: "Version 2" } },
    }),
  ).toContain("disabled for this game build");
  for (const text of [
    outfitUnavailableMessage(null),
    outfitUnavailableMessage({ ...session, phase: "closed" }),
  ])
    expect(text).not.toMatch(/enable outfit changes|confirm|tick/i);
});

test("the player dropdown keeps the last selection lineup across write replies, never across screens", () => {
  const lineup = {
    required: 4,
    selected: 4,
    players: [lineupPlayer(0), lineupPlayer(1)],
    unavailable: [],
  };
  const ready = {
    ...previewOutfitStatus(),
    phase: "ready",
    boundary: "selection",
    selection: lineup,
  };
  expect(retainedLineup(null, ready)).toBe(lineup);
  // Native writes publish no list ("refreshing"), and a failed re-list after
  // a verified apply publishes ready with no list: keep the lineup.
  for (const phase of ["queued", "refreshing", "ready", "error"])
    expect(
      retainedLineup(lineup, {
        ...ready,
        phase,
        selection: null,
        boundary: phase === "error" ? "unknown" : "selection",
      }),
    ).toBe(lineup);
  // Leaving selection, a hold or the end of the game drops it.
  expect(
    retainedLineup(lineup, { ...ready, selection: null, boundary: "match" }),
  ).toBeNull();
  expect(
    retainedLineup(lineup, { ...ready, selection: null, boundary: "menu" }),
  ).toBeNull();
  expect(
    retainedLineup(lineup, { ...ready, phase: "held", selection: null }),
  ).toBeNull();
  expect(
    retainedLineup(lineup, {
      ...ready,
      phase: "ended",
      boundary: "ended",
      selection: null,
    }),
  ).toBeNull();
  // A list from outside selection (Rearming keeps a stale one) is not shown.
  expect(retainedLineup(lineup, { ...ready, boundary: "unknown" })).toBeNull();
});
test("an empty player dropdown says why instead of a generic message", () => {
  const control = {
    status: {
      ...previewOutfitStatus(),
      phase: "error",
      boundary: "unknown",
      selection: null,
      message: "Open Blacktop player selection (game reports transition)",
    },
    lineup: null,
    binding: "",
    busy: false,
    eligible: true,
    error: "",
    select: () => {},
    scan: async () => true,
  } as unknown as ReturnType<typeof useNativeOutfits>;
  const html = renderToStaticMarkup(
    <OverlayPlayerMenu control={control} onChoose={() => {}} />,
  );
  expect(html).toContain("game reports transition");
});

// 0.53 live: the overlay flickered about once a second, mostly the PLAYER
// list. Automatic reads re-issue a List every second; each one used to flip
// busy (queued reply) and replace the lineup object, so every player button
// was disabled and re-enabled.
test("0.53: an automatic background List is invisible: same status, same lineup, no busy", () => {
  const lineup: NonNullable<NativeOutfitStatus["selection"]> = {
    required: 2,
    selected: 2,
    players: [lineupPlayer(0), lineupPlayer(1)],
    unavailable: [],
  };
  const ready: NativeOutfitStatus = {
    ...previewOutfitStatus(),
    phase: "ready",
    message: "Native Blacktop players refreshed.",
    revision: 7,
    completedRevision: 7,
    update: 14,
    boundary: "selection",
    selection: lineup,
  };
  const queued: NativeOutfitStatus = {
    ...ready,
    phase: "queued",
    message: "Outfit request queued for the native worker.",
    revision: 8,
    update: 15,
    busy: true,
  };
  // Background read in flight: the last settled status stays on screen.
  expect(visibleOutfitStatus(queued, ready, 8)).toBe(ready);
  // A foreground command (or another revision) is shown as busy as before.
  expect(visibleOutfitStatus(queued, ready, null)).toBe(queued);
  expect(visibleOutfitStatus({ ...queued, revision: 9 }, ready, 8)?.busy).toBe(
    true,
  );
  // A hold, or a busy match-carry tick on the same revision, is never hidden.
  const held = { ...queued, phase: "held", message: "stopped" };
  expect(visibleOutfitStatus(held, ready, 8)).toBe(held);
  const applying = {
    ...queued,
    phase: "applying",
    boundary: "match",
    selection: null,
  };
  expect(visibleOutfitStatus(applying, ready, 8)).toBe(applying);
  // The reply re-mints single-use tokens: same players -> same lineup object.
  const reread: NativeOutfitStatus = {
    ...ready,
    revision: 8,
    completedRevision: 8,
    update: 16,
    selection: {
      ...lineup,
      players: lineup.players.map((p) => ({ ...p, token: "fresh" })),
    },
  };
  expect(retainedLineup(lineup, reread)).toBe(lineup);
  expect(lineupDisplayKey(reread.selection)).toBe(lineupDisplayKey(lineup));
  // A real change (new occupant, pool now verified) replaces it.
  const changed: NativeOutfitStatus = {
    ...reread,
    selection: { ...lineup, players: [lineupPlayer(0), lineupPlayer(2)] },
  };
  expect(retainedLineup(lineup, changed)).toBe(changed.selection);
  // The dropdown renders identically before, during and after the read.
  const render = (status: NativeOutfitStatus) =>
    renderToStaticMarkup(
      <OverlayPlayerMenu
        control={
          {
            status,
            lineup: retainedLineup(lineup, status),
            binding: lineupPlayer(0).binding,
            busy: !!status.busy,
            eligible: true,
            error: "",
            select: () => {},
          } as unknown as ReturnType<typeof useNativeOutfits>
        }
        onChoose={() => {}}
      />,
    );
  const before = render(ready);
  expect(before).not.toContain("disabled");
  expect(render(visibleOutfitStatus(queued, ready, 8)!)).toBe(before);
  expect(render(reread)).toBe(before);
  // WPF: player items stay enabled while a foreground operation runs.
  const busy = renderToStaticMarkup(
    <OverlayPlayerMenu
      control={
        {
          status: ready,
          lineup,
          binding: lineupPlayer(0).binding,
          busy: true,
          eligible: true,
          error: "",
          select: () => {},
        } as unknown as ReturnType<typeof useNativeOutfits>
      }
      onChoose={() => {}}
    />,
  );
  expect(busy).toBe(before);
});

describe("automatic outfit origin and old-backend guard", () => {
  it("requires actual native duplicate-choice protection before automatic writes", async () => {
    const { automaticOutfitsSupported, outfitBatchCommand } =
      await import("./NativeOutfits");
    const items = [
      { playerBinding: "a".repeat(48), outfit: {} as import("./types").Outfit },
    ];
    expect(automaticOutfitsSupported(null)).toBe(false);
    expect(outfitBatchCommand(items, true, false)).toBeNull();
    expect(outfitBatchCommand(items, true, true)).toEqual({
      action: "apply-many",
      items,
      automatic: true,
    });
    expect(outfitBatchCommand(items, false, false)).toEqual({
      action: "apply-many",
      items,
    });
  });
});
