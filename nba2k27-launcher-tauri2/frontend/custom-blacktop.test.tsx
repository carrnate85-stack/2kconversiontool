import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { suggestedCourt } from "./CustomBlacktopImport";
import { BlacktopCatalog, catalogChoicePatch } from "./BlacktopCatalog";
import type { Settings } from "./types";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(() => new Promise(() => {})), convertFileSrc: (p: string) => p }));
describe("custom Blacktop import", () => {
  it("matches a sibling court without mixing identically named mod folders", () => {
    expect(suggestedCourt(["A/arena_test_int_floor.iff", "B/arena_test_int_floor.iff"], "B/arena_test_int.iff")).toBe("B/arena_test_int_floor.iff");
    expect(suggestedCourt(["A/FLOOR_015_INT_FLOOR.IFF"], "A/arena_015_int.iff")).toBe("A/FLOOR_015_INT_FLOOR.IFF");
    expect(suggestedCourt(["other.iff"], "arena.iff")).toBe("");
  });
  it("only offers importing in the dev arena catalog", () => {
    const props = { settings: { gameRoot: "C:/Game" } as Settings, change: () => {}, onClose: () => {} };
    expect(renderToStaticMarkup(<BlacktopCatalog {...props} kind="arena" customArenas />)).toContain("Add from mods");
    expect(renderToStaticMarkup(<BlacktopCatalog {...props} kind="arena" />)).not.toContain("Add from mods");
    expect(renderToStaticMarkup(<BlacktopCatalog {...props} kind="ball" customArenas />)).not.toContain("Add from mods");
  });
  it("persists a custom catalog identity alongside the friendly name", () => {
    expect(catalogChoicePatch("arena", { id: "custom-123", name: "My court" })).toEqual({ arena: "My court", arenaId: "custom-123" });
  });
});
