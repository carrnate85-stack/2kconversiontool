import { expect, it } from "vitest";
import {
  playerEditorLayout,
  playerDisplay,
  playerRaw,
} from "./player-web-presentation";
import fixture from "../assets/player-web-regressions.json";
import { playerFieldValue } from "./player-field-value";
it("reuses WPF field categories, ordering and personality pairs exactly", () => {
  expect(playerEditorLayout.categories).toEqual(fixture.categories);
  expect(playerEditorLayout.personalityPairs).toEqual(fixture.personalityPairs);
  for (const f of fixture.fields) {
    expect(playerEditorLayout.category(f)).toBe(f.category);
    expect(playerEditorLayout.order(f)).toBe(f.order);
  }
});
it("preserves exact imported raw values on unchanged display, and bounds new edits", () => {
  const f = {
    key: "DrivingLayup",
    label: "Driving layup",
    group: "attributes",
    maximum: 255,
  };
  for (let raw = 0; raw <= 255; raw++)
    expect(playerFieldValue(f, String(playerDisplay(f, raw)), raw)).toBe(raw);
  expect(playerFieldValue(f, "99", 255)).toBe(221);
  expect(playerFieldValue(f, "25", 255)).toBe(0);
  expect(playerFieldValue(f, "", 255)).toBeUndefined();
  for (const value of ["24", "100", "28.5", "Infinity", " "])
    expect(() => playerFieldValue(f, value, 0)).toThrow();
  const salary = {
    key: "SalaryYear1",
    label: "Salary",
    group: "contract",
    money: true,
    maximum: 200000000,
  };
  expect(playerFieldValue(salary, "1.234567")).toBe(1234567);
  expect(playerFieldValue(salary, "200")).toBe(200000000);
  for (const value of ["200.000001", "1.2345678", "1e2", "-1"])
    expect(() => playerFieldValue(salary, value)).toThrow();
});
it("reuses WPF attribute and salary conversions exactly", () => {
  for (const c of fixture.conversions) {
    if ("raw" in c) expect(playerDisplay(c.field, c.raw!)).toBe(c.display);
    else expect(playerRaw(c.field, c.value!)).toBe(c.changed);
  }
});
