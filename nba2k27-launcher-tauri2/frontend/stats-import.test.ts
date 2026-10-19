import { describe, it, expect } from "vitest";
import { referenceMapping, type ReferenceSource } from "./StatsImport";
describe("reference season review", () => {
  const source = (team: string, aggregate = false) =>
    ({ seasons: [{ year: 2026, team, aggregate }] }) as ReferenceSource;
  it("uses current teams only; auto-fills only a unique name", () => {
    const teams = [
      { id: 0, name: "76ers", category: "current" },
      { id: 100, name: "76ers", category: "historic" },
    ];
    expect(referenceMapping(source("76ers"), teams)[2026]).toBe("0");
    expect(referenceMapping(source("Unknown"), teams)[2026]).toBe("");
    expect(referenceMapping(source("76ers", true), teams)[2026]).toBe("");
    expect(
      referenceMapping(source("76ers"), [
        ...teams,
        { id: 1, name: "76ers", category: "current" },
      ])[2026],
    ).toBe("");
  });
});
