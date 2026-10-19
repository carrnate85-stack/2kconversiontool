import { describe, it, expect } from "vitest";
import {
  canApplyRule,
  previewRules,
  ruleUsesToggle,
  type Rule,
} from "./NativeRules";
const rule: Rule = {
  id: "ThreePointShotRestriction",
  label: "3Pt Shots",
  value: 0,
  supported: true,
  choices: [
    { value: 0, label: "Enabled" },
    { value: 1, label: "Enabled after steal or block" },
    { value: 2, label: "Disabled" },
  ],
};
describe("native rules choose-to-apply", () => {
  it("offline preview uses the full shared rule model without a live lease", () => {
    const p = previewRules();
    expect(p.rules).toHaveLength(39);
    expect(p.token).toBe("offline-preview");
    expect(
      p.rules.find((r) => r.label === "3Pt Shots")?.choices.map((c) => c.label),
    ).toEqual(rule.choices.map((c) => c.label));
    expect(p.rules.find((r) => r.label === "Foul Out")?.choices).toHaveLength(
      7,
    );
  });
  it("choosing a different mapped value is the change, as in WPF", () => {
    expect(canApplyRule(rule, 0, false)).toBe(false);
    expect(canApplyRule(rule, 2, false)).toBe(true);
  });
  it("rejects busy, unsupported or unmapped values", () => {
    expect(canApplyRule(rule, 2, true)).toBe(false);
    expect(canApplyRule({ ...rule, supported: false }, 2, false)).toBe(false);
    expect(canApplyRule(rule, 99, false)).toBe(false);
  });
  it("uses WPF's Off/On buttons only for two-state rules", () => {
    const rules = previewRules().rules;
    expect(ruleUsesToggle(rules.find((r) => r.label === "Shot Clock")!)).toBe(
      true,
    );
    expect(ruleUsesToggle(rule)).toBe(false);
    expect(ruleUsesToggle(rules.find((r) => r.label === "Foul Out")!)).toBe(
      false,
    );
  });
});
