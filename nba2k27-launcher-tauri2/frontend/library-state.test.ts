import { describe, it, expect } from "vitest";
import { modRows, move, uniqueName } from "./library-state";
describe("Portable library workflows", () => {
  it("keeps camera/playlist order and respects bounds", () => {
    expect(move(["a", "b", "c"], 1, 1)).toEqual(["a", "c", "b"]);
    expect(move(["a", "b"], 0, -1)).toEqual(["a", "b"]);
  });
  it("names are unique regardless of case", () =>
    expect(uniqueName("Profile", ["profile", "Profile 2"])).toBe("Profile 3"));
  const mods = [
    { name: "a.iff", path: "char\\sig\\a.iff", folder: "char\\sig", bytes: 4 },
    { name: "b.iff", path: "char\\b.iff", folder: "char", bytes: 5 },
  ];
  it("collapses and expands folders without changing file identity", () => {
    expect(
      modRows(mods, new Set(), " ", {}, "root").map((r) => r.path),
    ).toEqual(["char"]);
    expect(
      modRows(mods, new Set(["char", "char/sig"]), "", {}, "root").map(
        (r) => r.path,
      ),
    ).toEqual(["char", "char/sig", "char/sig/a.iff", "char/b.iff"]);
  });
  it("tag search reveals parent folders", () =>
    expect(
      modRows(
        mods,
        new Set(),
        "Curry",
        { "root|char\\sig\\a.iff": "Curry" },
        "root",
      ).map((r) => r.path),
    ).toEqual(["char", "char/sig", "char/sig/a.iff"]));
});
import {
  classifyMod,
  filterModGroups,
  modGroups,
  normalizeModTags,
  sortModGroups,
  withGroupTags,
} from "./library-state";
describe("WPF-style mod library bundles", () => {
  const files = [
    {
      name: "png1234.iff",
      path: "players\\png1234.iff",
      folder: "players",
      bytes: 10,
    },
    {
      name: "png1234_hair.iff",
      path: "players\\png1234_hair.iff",
      folder: "players",
      bytes: 5,
    },
    { name: "arena_999.iff", path: "arena_999.iff", folder: "", bytes: 40 },
    { name: "english.iff", path: "english.iff", folder: "", bytes: 1 },
  ];
  it("classifies like the WPF catalog", () => {
    expect(classifyMod("a\\png10000_x.iff")).toBe("Player");
    expect(classifyMod("arena_1.iff")).toBe("Arena");
    expect(classifyMod("uniform_lal.iff")).toBe("Uniform");
    expect(classifyMod("english.iff")).toBe("Localization");
    expect(classifyMod("misc.iff")).toBe("IFF");
    expect(classifyMod("x.dds")).toBe("Texture");
    expect(classifyMod("x.txt")).toBe("Other");
  });
  it("bundles player dependencies under the base iff", () => {
    const groups = modGroups(
      files,
      { "r|players\\png1234_hair.iff": "Curry" },
      "r",
    );
    const player = groups.find((g) => g.stem === "png1234")!;
    expect(player.title).toBe("players/png1234");
    expect(player.base.path).toBe("players\\png1234.iff");
    expect(player.files).toHaveLength(2);
    expect(player.bytes).toBe(15);
    expect(player.tags).toEqual(["Curry"]);
    expect(groups).toHaveLength(3);
  });
  it("sorts by every column and searches dependency names", () => {
    const groups = modGroups(files, {}, "r");
    expect(sortModGroups(groups, "size", -1)[0].title).toBe("arena_999.iff");
    expect(sortModGroups(groups, "category", 1).map((g) => g.category)).toEqual(
      ["Arena", "Localization", "Player"],
    );
    expect(filterModGroups(groups, "hair").map((g) => g.stem)).toEqual([
      "png1234",
    ]);
  });
  it("normalizes tags and stores them once per bundle", () => {
    expect(normalizeModTags(" Curry ,curry, GSW ,, ")).toBe("Curry, GSW");
    expect(() => normalizeModTags("x".repeat(41))).toThrow();
    const group = modGroups(files, {}, "r").find((g) => g.stem)!;
    expect(
      withGroupTags(
        { "r|players\\png1234_hair.iff": "old" },
        "r",
        group,
        "New",
      ),
    ).toEqual({ "r|players\\png1234.iff": "New" });
  });
});
