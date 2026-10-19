import { describe, expect, it } from "vitest";
import { cameraActions, cameraDot } from "./CameraLiveStatus";
import { formatVideoLength } from "./state";

describe("camera live status", () => {
  it("offers Retry only for a stop or a restorable hold", () => {
    expect(cameraActions({ phase: "stopped", message: "" }).retry).toBe(true);
    expect(
      cameraActions({ phase: "held", message: "", retryable: true }).retry,
    ).toBe(true);
    expect(cameraActions({ phase: "held", message: "" }).retry).toBe(false);
    expect(cameraActions({ phase: "active", message: "" }).retry).toBe(false);
    expect(cameraActions(null)).toEqual({ retry: false, stop: false });
  });
  it("offers Stop & restore while cameras are waiting or active", () => {
    for (const phase of ["waiting", "active", "reload", "reset"])
      expect(cameraActions({ phase, message: "" }).stop).toBe(true);
    for (const phase of ["held", "stopped", "disabled", "ended"])
      expect(cameraActions({ phase, message: "" }).stop).toBe(false);
  });
  it("maps phases to the WPF status dot", () => {
    expect(cameraDot("active")).toBe("live");
    expect(cameraDot("waiting")).toBe("waiting");
    expect(cameraDot("held")).toBe("error");
    expect(cameraDot("stopped")).toBe("");
  });
});

describe("startup video length", () => {
  it("formats catalog durations", () => {
    expect(formatVideoLength(45000)).toBe("45 sec");
    expect(formatVideoLength(125400)).toBe("2:05");
    expect(formatVideoLength(undefined)).toBe("—");
  });
});
