import { expect, it } from "vitest";
import {
  playerMeasurements as math,
  colorHex,
  argbToInput,
  inputToArgb,
} from "./editor-units";
import fixture from "../assets/editor-units-regressions.json";
const input = (value = "") => ({ value, dataset: {} });
const attempt = (fn: () => unknown) => {
  try {
    return fn();
  } catch {
    return null;
  }
};
it("keeps WPF feet/inches displays and exact centimetre round trips", () => {
  for (const c of fixture.measurementCases) {
    expect(math.parts(c.cm)).toEqual(c.parts);
    expect(math.inches(c.cm)).toBe(c.inches);
    const feet = input(),
      inches = input(),
      wing = input();
    math.writeHeight(feet, inches, c.cm);
    math.writeWingspan(wing, c.cm);
    expect(attempt(() => math.readHeight(feet, inches))).toBe(c.height);
    expect(attempt(() => math.readWingspan(wing))).toBe(c.wingspan);
  }
  for (const c of fixture.editedCases)
    expect(attempt(() => math.readHeight(input(c.feet), input(c.inches)))).toBe(
      c.expected,
    );
});
it("retains all 256 existing alpha bytes when picking a uniform RGB color", () => {
  for (const c of fixture.colorCases) {
    expect(colorHex(c.original)).toBe(c.hex);
    expect(argbToInput(c.original)).toBe(c.input);
    expect(inputToArgb(c.choice, c.original)).toBe(c.changed);
    expect(inputToArgb(c.choice, c.original) >>> 24).toBe(c.original >>> 24);
  }
});
