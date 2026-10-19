import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  type ScalerGroups,
  ScalerSettings,
  scalerFactor,
  scalerPercent,
  scalerSupported,
} from "./ScalerSettings";

const ALL: ScalerGroups = {
  coaches: true,
  cheerleaders: true,
  arenaPersonnel: true,
};
const noop = () => {};

describe("staged player scale settings", () => {
  it("converts only the supported whole-percent range", () => {
    expect(scalerPercent(0.97)).toBe(97);
    expect(scalerPercent(0.8)).toBe(95);
    expect(scalerPercent(1.2)).toBe(100);
    expect(scalerPercent(Number.NaN)).toBe(100);
    expect(scalerPercent(Number.POSITIVE_INFINITY)).toBe(100);
    expect(scalerFactor(97)).toBe(0.97);
    expect(scalerFactor(10)).toBe(0.95);
    expect(scalerFactor(200)).toBe(1);
    expect(scalerSupported(0.95)).toBe(true);
    expect(scalerSupported(0.971)).toBe(false);
    expect(scalerSupported(0.8)).toBe(false);
  });

  it("renders one staged save workflow instead of persisting every slider input", () => {
    let writes = 0;
    const html = renderToStaticMarkup(
      <ScalerSettings
        value={0.97}
        onSave={() => writes++}
        groups={ALL}
        onGroupsChange={noop}
      />,
    );
    expect(writes).toBe(0);
    expect(html).toContain("97%");
    expect(html).toContain("Save size");
    expect(html).toContain("Reset to 100%");
    expect(html).toContain("Players &amp; referees");
    expect(html).toContain("Coaches");
    expect(html).not.toContain("Not mapped");
    expect(html).toContain("97% is saved for the next Play launch");
  });

  it("makes an old unsupported draft visibly repairable instead of silently clamping it", () => {
    const html = renderToStaticMarkup(
      <ScalerSettings
        value={0.8}
        onSave={() => {}}
        groups={ALL}
        onGroupsChange={noop}
      />,
    );
    expect(html).toContain("outside the Version 2 range");
    expect(html).toMatch(/<button id="scalerSave" class="primary" type="button">Save size<\/button>/);
  });
  it("keeps Reset enabled when an unsupported value clamps visually to 100%", () => {
    for (const value of [1.2, Number.NaN]) {
      const html = renderToStaticMarkup(
        <ScalerSettings
          value={value}
          onSave={() => {}}
          groups={ALL}
          onGroupsChange={noop}
        />,
      );
      expect(html).toContain("outside the Version 2 range");
      expect(html).toMatch(
        /<button id="scalerReset" class="secondary" type="button">Reset to 100%<\/button>/,
      );
    }
  });
  it("shows each character group as a real toggle reflecting the saved choice", () => {
    const html = renderToStaticMarkup(
      <ScalerSettings
        value={0.97}
        onSave={noop}
        groups={{ coaches: true, cheerleaders: false, arenaPersonnel: true }}
        onGroupsChange={noop}
      />,
    );
    // Players & referees is the fixed, always-included primary row.
    expect(html.match(/type="checkbox"/g)?.length).toBe(4);
    expect(html.match(/checked=""/g)?.length).toBe(3);
    expect(html).toContain('aria-label="Scale cheerleaders"');
  });
});
