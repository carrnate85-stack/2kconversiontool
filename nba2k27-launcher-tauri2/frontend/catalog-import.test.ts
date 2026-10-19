import { describe, expect, it } from "vitest";
import { catalogSectionCounts, catalogSections } from "./catalog-import";
describe("reviewed WPF catalog sections", () => {
  it("counts only available mapped values, including zero", () => {
    expect(
      catalogSectionCounts(
        {
          info: {
            birthDate: "2000-01-01",
            yearsPro: 0,
            weightLbs: 180,
            potential: 99,
            fullNameId: 3,
          },
          fields: { rating: 0, hand: 1, unknown: 9, badge: 2 },
        },
        [
          { key: "rating", group: "attributes" },
          { key: "hand", group: "appearance" },
          { key: "badge", group: "badges" },
        ],
      ),
    ).toEqual({
      info: 2,
      appearance: 2,
      attributes: 2,
      tendencies: 0,
      personality: 0,
      hotzones: 0,
    });
  });
  it("never enables unverified badge, animation, asset, team or contract mappings", () => {
    expect(catalogSections.map((s) => s.key)).toEqual([
      "info",
      "appearance",
      "attributes",
      "tendencies",
      "personality",
      "hotzones",
    ]);
    expect(
      Object.values(catalogSectionCounts({ info: {}, fields: {} }, [])),
    ).toEqual([0, 0, 0, 0, 0, 0]);
  });
});
