import { describe, it, expect } from "vitest";
import {
  initialOutfit,
  toggleAccessory,
  togglePanel,
  stagedSeasons,
} from "./state";
describe("Prototype state", () => {
  it("same catalog button closes its panel", () =>
    expect(togglePanel("ball", "ball")).toBeNull());
  it("switches directly between catalogs", () =>
    expect(togglePanel("ball", "scoreboard")).toBe("scoreboard"));
  it("never exceeds three accessories", () => {
    let o = initialOutfit;
    for (const a of ["a", "b", "c", "d"]) o = toggleAccessory(o, a);
    expect(o.accessories).toEqual(["a", "b", "c"]);
  });
  it("removes an existing accessory", () =>
    expect(
      toggleAccessory({ ...initialOutfit, accessories: ["a"] }, "a")
        .accessories,
    ).toEqual([]));
  it("only stages checked seasons", () =>
    expect(stagedSeasons(new Set([2]), [{ id: 1 }, { id: 2 }])).toEqual([
      { id: 2 },
    ]));
  it("accessory changes preserve shoes", () =>
    expect(toggleAccessory(initialOutfit, "a").shoes).toBe(
      initialOutfit.shoes,
    ));
});
