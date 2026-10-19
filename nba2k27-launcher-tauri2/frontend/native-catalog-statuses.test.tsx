import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { test, expect } from "vitest";
import { NativeCatalogStatuses } from "./NativeCatalogStatuses";
import type { ArenaStatus } from "./NativeArena";
import { previewOutfitStatus } from "./NativeOutfits";
test("outfit errors and native confirmation remain visible when F8 is hidden", () => {
  const outfits = {
    ...previewOutfitStatus(),
    phase: "held",
    revision: 0,
    message: "Native outfit result was not confirmed; no retry",
  };
  const html = renderToStaticMarkup(
    <NativeCatalogStatuses phase="game-running" provider={{ outfits }} />,
  );
  expect(html).toContain("Outfits: held");
  expect(html).toContain("not confirmed");
});
test("launcher shows restoration while the live F8 window is hidden at a safe menu", () => {
  const provider = {
    ball: {
      phase: "restored",
      revision: 1,
      message: "Original ball restored. Next choice waits for selection.",
    } as ArenaStatus,
  };
  const html = renderToStaticMarkup(
    <NativeCatalogStatuses phase="game-running" provider={provider} />,
  );
  expect(html).toContain("Ball: restored");
  expect(html).toContain("Original ball restored");
  expect(html).toContain('role="status"');
});
test("scoreboard preparation and restore status stay visible outside F8", () => {
  const html = renderToStaticMarkup(
    <NativeCatalogStatuses
      phase="game-running"
      provider={{
        scoreboard: {
          phase: "restored",
          revision: 1,
          message: "Original scoreboard restored; next selection will rearm",
        } as ArenaStatus,
      }}
    />,
  );
  expect(html).toContain("Scoreboard: restored");
  expect(html).toContain("next selection will rearm");
});
test("unrequested and ended helpers stay quiet; message text is escaped", () => {
  const ball = {
    phase: "waiting-for-unload",
    revision: 1,
    message: "<script>untrusted</script>",
  } as ArenaStatus;
  expect(
    renderToStaticMarkup(
      <NativeCatalogStatuses phase="closed" provider={{ ball }} />,
    ),
  ).toBe("");
  expect(
    renderToStaticMarkup(
      <NativeCatalogStatuses
        phase="game-running"
        provider={{ ball: { ...ball, revision: 0 } }}
      />,
    ),
  ).toBe("");
  expect(
    renderToStaticMarkup(
      <NativeCatalogStatuses
        phase="game-running"
        provider={{ ball: { ...ball, phase: "ended" } }}
      />,
    ),
  ).toBe("");
  const html = renderToStaticMarkup(
    <NativeCatalogStatuses phase="game-running" provider={{ ball }} />,
  );
  expect(html).not.toContain("<script>");
  expect(html).toContain("&lt;script&gt;");
});
