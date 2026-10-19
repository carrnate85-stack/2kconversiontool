import React from "react";
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  NativeSelectorStatus,
  type SelectorSummary,
} from "./NativeSelectorStatus";

const prepared: SelectorSummary = {
  enabled: true,
  players: 3430,
  source: "session-frozen memory copy",
  sha256: {},
};
describe("selected mod-folder player configuration status", () => {
  it("distinguishes native generation and unsupported sets without claiming visual acceptance", () => {
    const status = renderToStaticMarkup(<NativeSelectorStatus phase="loader-active" selectors={{ ...prepared, generatedPlayers: 9, replacementPlayers: 2, skippedPlayers: 1 }} />);
    expect(status).toContain("native-generated bundle active");
    expect(status).toContain("Added 9 player IDs");
    expect(status).toContain("updated 2 configuration sets");
    expect(status).toContain("1 configuration sets skipped by the selector planner");
    expect(status).not.toContain("visually verified");
  });
  it("reports the verified existing bundle only after loader activation", () => {
    const status = renderToStaticMarkup(
      <NativeSelectorStatus
        phase="loader-active"
        selectors={prepared}
        playerFolderFiles={217}
      />,
    );
    expect(status).toContain("3,430 records");
    expect(status).toContain(
      "validated mod-folder bundle active for this session",
    );
    expect(status).toContain("217 shared-data links");
    expect(status).toContain("no copied player data");
    for (const phase of [
      undefined,
      "queued",
      "preparing-selectors",
      "waiting-for-loader",
      "held",
      "ended",
      "closed",
    ]) {
      expect(
        renderToStaticMarkup(
          <NativeSelectorStatus phase={phase} selectors={prepared} />,
        ),
      ).toBe("");
    }
  });
  it("does not pretend missing selectors have been generated or enabled", () => {
    expect(
      renderToStaticMarkup(
        <NativeSelectorStatus
          phase="loader-active"
          selectors={{ ...prepared, enabled: false, players: 0 }}
        />,
      ),
    ).toContain("stock tables · no prepared bundle");
    expect(
      renderToStaticMarkup(<NativeSelectorStatus phase="loader-active" />),
    ).toBe("");
  });
});
