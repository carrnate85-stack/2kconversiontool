import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { UnlocksPanel, freestyleUnlockMessage } from "./UnlocksPanel";

function render(
  freestyle: boolean,
  blacktopAvailable = false,
  debugMenusAvailable = false,
  debugMenus = false,
) {
  let writes = 0;
  const html = renderToStaticMarkup(
    <UnlocksPanel
      blacktop
      onBlacktopChange={() => writes++}
      freestyle={freestyle}
      onFreestyleChange={() => writes++}
      freestyleMessage={freestyleUnlockMessage(freestyle)}
      blacktopAvailable={blacktopAvailable}
      debugMenus={debugMenus}
      debugMenusAvailable={debugMenusAvailable}
      onDebugMenusChange={() => writes++}
    />,
  );
  return { html, writes };
}

describe("Unlocks panel", () => {
  it("enables the verified Freestyle route and leaves unavailable routes disabled", () => {
    const { html, writes } = render(false);
    expect(writes).toBe(0);
    expect(html).toContain("WNBA Teams in Freestyle");
    expect(html).toContain("WNBA Teams in Blacktop");
    expect(html).toContain("Enable Debug Menus");
    expect(html.match(/disabled=""/g)).toHaveLength(2);
    expect(html.match(/unlock-choice unavailable/g)).toHaveLength(2);
    expect(html).not.toMatch(/id="unlockWnbaFreestyle"[^>]*disabled/);
    expect(html).not.toMatch(/id="unlockWnbaFreestyle"[^>]*checked/);
    expect(html).not.toContain("wnbaRosterVariant");
    expect(html).not.toContain("gameplay remains unverified");
    expect(html).toContain("Saved: Off.");
  });

  it("shows the saved Freestyle setting and next-launch behavior", () => {
    const { html } = render(true);
    expect(html).toMatch(/id="unlockWnbaFreestyle"[^>]*checked/);
    expect(html).toContain("Saved: On.");
    expect(html).toContain("next Play session");
  });

  it("only exposes Blacktop when its native route is available", () => {
    const { html } = render(false, true);
    expect(html.match(/disabled=""/g)).toHaveLength(1);
    expect(html).not.toMatch(/id="unlockWnbaBlacktop"[^>]*disabled/);
    expect(html).toContain("Use Y to cycle to WNBA teams in Blacktop");
  });

  it("exposes the session-only debug choice only in its dev capability", () => {
    const { html } = render(false, true, true, true);
    expect(html.match(/disabled=""/g) ?? []).toHaveLength(0);
    expect(html).toMatch(/id="enableDebugMenus"[^>]*checked/);
    expect(html).toContain("Armed for the next Play session");
    expect(html).toContain("never saved");
  });
});
