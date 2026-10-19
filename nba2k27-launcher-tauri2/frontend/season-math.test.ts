import { it, expect } from "vitest";
import { seasonMath } from "./season-ui-math";
import cases from "../assets/season-math-regressions.json";
it("every UI edit matches the WPF and native arithmetic oracle", () => {
  for (const c of cases) {
    let result = null;
    try {
      result = seasonMath.change(c.before, c.key, c.value);
    } catch {}
    expect(result).toEqual(c.expected);
  }
});
