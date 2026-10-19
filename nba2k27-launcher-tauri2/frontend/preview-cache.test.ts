import { describe, expect, it } from "vitest";
import { clothingCacheText } from "./PreviewCache";
import { readableName } from "./FitPreview";

describe("WPF preview text parity", () => {
  it("formats Prepare all previews progress like clothing-cache.js", () => {
    const base = { total: 10, completed: 4, ready: 3, missing: 1, failed: 0, batch: 1, batches: 1 };
    expect(clothingCacheText({ ...base, phase: "idle" })).toBe("Previews also save as you browse.");
    expect(clothingCacheText({ ...base, phase: "indexing" })).toBe("Finding available clothing previews…");
    expect(clothingCacheText({ ...base, phase: "running" })).toBe(
      "Batch 1 / 1 · 4 / 10 unique previews checked · 3 ready · 1 without previews · 0 failed",
    );
    expect(clothingCacheText({ ...base, phase: "cancelled" })).toMatch(/^Stopped\. .*Saved pictures will be reused next time\.$/);
    expect(clothingCacheText({ ...base, phase: "failed", error: "x" })).toBe("Could not prepare previews: x");
  });
  it("names mannequin cards like fit-preview-ui.js readableName", () => {
    expect(readableName("tanktop_solid_gry")).toBe("gray tank top");
    expect(readableName("acc_sock_solid_wht_crew")).toBe("white crew socks");
    expect(readableName("shoe_0000_default")).toBe("Default shoes");
  });
});
