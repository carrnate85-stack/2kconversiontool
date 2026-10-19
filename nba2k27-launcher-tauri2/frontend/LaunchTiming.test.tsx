import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { NativeLaunchTiming, launchTimingRows } from "./LaunchTiming";

describe("bounded launch timing diagnostics", () => {
  it("keeps only bounded numeric phase rows", () => {
    const transitions = Array.from({ length: 20 }, (_, index) => ({
      phase: `phase-${index}`,
      elapsedMs: index,
      completed: index < 19,
    }));
    const rows = launchTimingRows({ transitions });
    expect(rows).toHaveLength(16);
    expect(rows[0].phase).toBe("phase 4");
    expect(rows.at(-1)).toEqual({
      phase: "phase 19",
      elapsedMs: 19,
      completed: false,
    });
    expect(
      launchTimingRows({
        transitions: [{ phase: "bad", elapsedMs: Number.NaN }],
      }),
    ).toEqual([]);
  });

  it("labels measured launcher work without claiming gameplay verification", () => {
    const html = renderToStaticMarkup(
      <NativeLaunchTiming
        timing={{
          elapsedMs: 750,
          transitions: [
            { phase: "provider-indexing", elapsedMs: 500, completed: true },
            { phase: "preparing-cameras", elapsedMs: 250, completed: false },
          ],
        }}
      />,
    );
    expect(html).toContain("Launch preparation timing · 750 ms");
    expect(html).toContain("provider indexing");
    expect(html).toContain("250 ms · current");
    expect(html).toContain("not proof");
  });
});
