import { describe, expect, it } from "vitest";
import {
  wnbaFreestyleAvailability,
  wnbaBlacktopAvailability,
  blacktopUnlockAvailable,
} from "./feature-availability";

describe("verified WNBA Freestyle feature", () => {
  it("exposes the saved toggle after successful team selection and on-court testing", () => {
    expect(wnbaFreestyleAvailability.enabled).toBe(true);
    expect(wnbaFreestyleAvailability.message).toContain("passed dev testing");
    expect(wnbaFreestyleAvailability.message).toContain("next Play session");
    expect(wnbaBlacktopAvailability.enabled).toBe(true);
  });
  it("keeps an older hot-reloaded launcher and a release runtime unavailable", () => {
    expect(blacktopUnlockAvailable(undefined)).toBe(false);
    expect(blacktopUnlockAvailable({})).toBe(false);
    expect(blacktopUnlockAvailable({ capabilities: { wnbaBlacktop: false } })).toBe(false);
    expect(blacktopUnlockAvailable({ capabilities: { wnbaBlacktop: true } })).toBe(true);
  });
});
