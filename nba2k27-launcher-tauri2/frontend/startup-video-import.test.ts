import { describe, expect, it } from "vitest";
import {
  startupVideoImportCanCancel,
  startupVideoImportForOperation,
  startupVideoImportLabel,
  startupVideoImportOperationId,
  startupVideoImportPercent,
  startupVideoImportProgress,
} from "./startup-video-import";

describe("startup-video import progress", () => {
  const progress = {
    operationId: "generation-1",
    phase: "copying" as const,
    bytes: 50,
    total: 100,
  };

  it("creates one strict operation identity before the native request starts", () => {
    expect(
      startupVideoImportOperationId("00112233-4455-6677-8899-AABBCCDDEEFF"),
    ).toBe("video-import-00112233-4455-6677-8899-aabbccddeeff");
    for (const value of [
      "",
      "00112233445566778899aabbccddeeff",
      "../../00112233-4455-6677-8899-aabbccddeeff",
      "00112233-4455-6677-8899-aabbccddeefg",
    ])
      expect(() => startupVideoImportOperationId(value)).toThrow();
  });

  it("ignores delayed events from an earlier import generation", () => {
    expect(startupVideoImportForOperation(progress, "generation-1")).toEqual(
      progress,
    );
    expect(startupVideoImportForOperation(progress, "generation-2")).toBeNull();
  });

  it("accepts bounded owned progress and rejects malformed or stale-looking data", () => {
    expect(startupVideoImportProgress(progress)).toEqual(progress);
    expect(startupVideoImportProgress({ ...progress, total: 0 })).toBeNull();
    expect(startupVideoImportProgress({ ...progress, bytes: 101 })).toBeNull();
    expect(
      startupVideoImportProgress({ ...progress, phase: "published" }),
    ).toBeNull();
    expect(
      startupVideoImportProgress({ ...progress, operationId: "" }),
    ).toBeNull();
  });

  it("never displays 100 percent before the catalog commit finishes", () => {
    expect(startupVideoImportPercent(progress)).toBe(50);
    expect(
      startupVideoImportPercent({
        ...progress,
        bytes: 100,
        phase: "verifying",
      }),
    ).toBe(99);
    expect(
      startupVideoImportPercent({
        ...progress,
        bytes: 100,
        phase: "committing",
      }),
    ).toBe(99);
    expect(
      startupVideoImportPercent({ ...progress, bytes: 100, phase: "done" }),
    ).toBe(100);
  });

  it("exposes cancel only before commit and gives every phase a stable label", () => {
    expect(startupVideoImportCanCancel(progress)).toBe(true);
    expect(
      startupVideoImportCanCancel({ ...progress, phase: "committing" }),
    ).toBe(false);
    expect(startupVideoImportCanCancel({ ...progress, phase: "done" })).toBe(
      false,
    );
    expect(startupVideoImportLabel("checking")).toBe("Checking MP4");
    expect(startupVideoImportLabel("done")).toBe("Startup video ready");
  });
});
