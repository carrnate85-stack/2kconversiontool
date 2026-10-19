import { describe, expect, it } from "vitest";
import { compatDetail, hotkeyText } from "./SettingsTools";

describe("Ctrl+Shift+F12 state text", () => {
  it("names each registration state", () => {
    expect(
      hotkeyText({ phase: "waiting-for-game", message: "m", presses: 0, lastPressMs: null, lastSource: null }),
    ).toBe("Ctrl+Shift+F12: Waiting for game · m");
    expect(
      hotkeyText({ phase: "held", message: "m", presses: 0, lastPressMs: null, lastSource: null }),
    ).toContain("Held by another app");
  });
  it("shows the last press and when key watching caught it", () => {
    const text = hotkeyText({ phase: "registered", message: "m", presses: 1, lastPressMs: 0, lastSource: "keyboard-poll" });
    expect(text).toContain("Registered");
    expect(text).toContain("last press");
    expect(text).toContain("(key watch)");
  });
});

describe("game build compatibility rows", () => {
  it("shows the proof counts behind each status", () => {
    expect(
      compatDetail({ key: "cameras", label: "Cameras", status: "Needs mapping", message: "Game updated: Cameras needs a new mapping.", anchors: 10, found: 10, constants: 5, derived: 5 }),
    ).toBe("Game updated: Cameras needs a new mapping. · 10/10 code anchors · 5/5 game globals");
    expect(
      compatDetail({ key: "outfits", label: "Outfits", status: "Needs mapping", message: "m", anchors: 0, found: 0, constants: 0, derived: 0 }),
    ).toBe("m");
  });
});
