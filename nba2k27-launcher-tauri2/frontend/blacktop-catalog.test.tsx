import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  BlacktopCatalog,
  catalogChoicePatch,
  catalogEmptyText,
  catalogLabel,
  catalogSavedMessage,
  catalogFilters,
  catalogPaging,
  catalogSelected,
} from "./BlacktopCatalog";
import type { Settings } from "./types";
import { useNativeArena } from "./NativeArena";

vi.mock("./NativeArena", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./NativeArena")>();
  return {
    ...actual,
    useNativeArena: vi.fn(() => ({
      status: null,
      busy: false,
      pendingId: "",
      error: "",
      select: async () => false,
      eligible: false,
      unavailable: "",
    })),
  };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(() => new Promise(() => {})),
  convertFileSrc: (p: string) => `asset://${p}`,
}));

const settings = {
  gameRoot: "C:\\Game",
  arena: "Historic Warriors / Oracle Arena",
  arenaId: "arena_637_int",
  scoreboard: "Broadcast",
  ball: "Currently installed",
} as Settings;

describe("WPF launcher Blacktop catalog", () => {
  it("orders filters like blacktop-setup-picker.js, with Favorites only for arenas", () => {
    expect(catalogFilters("arena", ["Other", "Gyms", "NBA"])).toEqual([
      "All",
      "Favorites",
      "NBA",
      "Gyms",
      "Other",
    ]);
    expect(
      catalogFilters("scoreboard", ["Experimental", "Eras", "Classic"]),
    ).toEqual(["All", "Classic", "Eras", "Experimental"]);
    expect(catalogFilters("ball", ["League", "Installed"])[1]).toBe(
      "Installed",
    );
  });
  it("uses WPF paging and empty-state text", () => {
    expect(catalogPaging(0, 165)).toBe("1–24 of 165");
    expect(catalogPaging(144, 165)).toBe("145–165 of 165");
    expect(catalogPaging(0, 0)).toBe("No results");
    expect(catalogEmptyText("Favorites")).toBe(
      "Star an arena to find it here.",
    );
    expect(catalogEmptyText("NBA")).toBe("No matching choices.");
  });
  it("saves the catalog id beside the name so repeated arena names stay distinct", () => {
    const row = { id: "arena_634_int", name: settings.arena };
    expect(catalogChoicePatch("arena", row)).toEqual({
      arena: settings.arena,
      arenaId: "arena_634_int",
    });
    expect(catalogSelected("arena", settings, row)).toBe(false);
    expect(
      catalogSelected("arena", settings, { ...row, id: "arena_637_int" }),
    ).toBe(true);
    expect(
      catalogSelected("arena", { ...settings, arenaId: "" }, row),
    ).toBe(true);
    expect(
      catalogChoicePatch("scoreboard", { id: "broadcast", name: "Broadcast" }),
    ).toEqual({ scoreboard: "Broadcast", scoreboardId: "broadcast" });
    expect(catalogChoicePatch("ball", { id: "current", name: "x" })).toEqual({
      ball: "x",
    });
  });
  it("shows WPF idle and saved messages and the stock ball label", () => {
    expect(catalogLabel("Currently installed")).toBe("Current installed");
    expect(catalogSavedMessage("ball", settings)).toBe(
      "Current installed ball will be used.",
    );
    expect(catalogSavedMessage("ball", { ...settings, ball: "Allstar" })).toMatch(
      /^Saved for the next Play session/,
    );
    expect(catalogSavedMessage("scoreboard", settings)).toBe(
      "Saved — waiting for Blacktop.",
    );
    expect(
      catalogSavedMessage("scoreboard", { ...settings, scoreboard: "Default" }),
    ).toBe("Uses the stock Blacktop scoreboard.");
    expect(
      catalogSavedMessage("arena", { ...settings, arena: "Default · Day / Night" }),
    ).toBe("Uses the game’s Day / Night choice.");
  });
  it("renders the WPF dialog shell", () => {
    const html = renderToStaticMarkup(
      <BlacktopCatalog
        kind="arena"
        settings={settings}
        change={() => {}}
        onClose={() => {}}
      />,
    );
    expect(html).toContain('class="blacktop-catalog"');
    expect(html).toContain('data-catalog-kind="arena"');
    expect(html).toContain("BLACKTOP PRESENTATION");
    expect(html).toContain("Arena catalog");
    expect(html).toContain('placeholder="Search arena catalog"');
    expect(html).toContain("★ Favorites");
    expect(html).toContain("No results");
  });
});


describe("scoreboard catalog live holds", () => {
  it("shows the worker hold reason inside the catalog instead of implying it is waiting to apply", () => {
    const reason = "Scoreboard changes stopped for this game. Close the game and restart before choosing another scoreboard. Scoreboard restore descriptor ownership data changed";
    vi.mocked(useNativeArena).mockReturnValueOnce({
      status: {
        phase: "held", requestedId: "broadcast", activeId: "broadcast",
        revision: 1, appliedRevision: 1,
        message: "Scoreboard restore descriptor ownership data changed",
      },
      busy: false, pendingId: "", error: "", select: async () => false,
      eligible: false, unavailable: reason,
    });
    const html = renderToStaticMarkup(
      <BlacktopCatalog kind="scoreboard" settings={settings} change={() => {}} onClose={() => {}} />,
    );
    expect(html).toContain(reason);
    expect(html).not.toContain("Saved — waiting for Blacktop.");
    expect(useNativeArena).toHaveBeenLastCalledWith(true, settings.gameRoot, "scoreboard", true);
  });
});
