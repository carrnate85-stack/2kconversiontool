import { describe, expect, it } from "vitest";
import { gameCaptureMessage } from "./GameCapture";

describe("gameCaptureMessage", () => {
  it("describes capture progress, results and errors", () => {
    expect(gameCaptureMessage({ phase: "capturing" })).toMatch(/pause briefly/);
    expect(
      gameCaptureMessage({
        phase: "captured",
        result: { file: "hang-2.dmp", snapshot: 2, bytes: 3 * 1048576, windowHung: true },
      }),
    ).toBe(
      "Game snapshot 2/3 saved locally · 3.0 MB · game window not responding · hang-2.dmp",
    );
    expect(
      gameCaptureMessage({
        phase: "captured",
        result: { file: "hang-1.dmp", snapshot: 1, bytes: 0, windowHung: false },
      }),
    ).not.toMatch(/not responding/);
    expect(gameCaptureMessage({ phase: "error", message: "Start NBA 2K27 first" })).toBe(
      "Game snapshot: Start NBA 2K27 first",
    );
  });
});
