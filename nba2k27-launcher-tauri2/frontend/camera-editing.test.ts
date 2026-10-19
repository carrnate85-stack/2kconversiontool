import { describe, expect, it } from "vitest";
import {
  cameraDraftError,
  cameraNameSlot,
  cameraSetupError,
  renameCamera,
  resetCameraName,
} from "./camera-editing";
const camera = {
  name: "Broadcast Low 2K18",
  enabled: true,
  height: 1,
  zoom: 1,
};
describe("WPF camera name editing", () => {
  it("pins legacy native identity before renaming and does not mutate saved rows", () => {
    const renamed = renameCamera(camera, "My broadcast");
    expect(renamed.id).toBe(51);
    expect(camera.name).toBe("Broadcast Low 2K18");
    expect(cameraDraftError([renamed])).toBeNull();
    expect(cameraNameSlot(renamed)?.max).toBe(19);
    expect(renameCamera(renamed, "Custom").id).toBe(51);
  });
  it("keeps stock labels and unknown types locked", () => {
    for (const c of [
      { ...camera, id: 59, name: "Side" },
      { ...camera, id: 55, name: "Nosebleeds" },
      { ...camera, id: 999 },
    ])
      expect(renameCamera(c, "Changed")).toBe(c);
    expect(cameraDraftError([{ ...camera, id: 999 }])).toContain("unknown");
  });
  it("restores only an editable camera to its native stock label", () => {
    const renamed = renameCamera(camera, "My broadcast");
    expect(resetCameraName(renamed)).toEqual({
      ...renamed,
      name: "Broadcast Low 2K18",
    });
    const side = { ...camera, id: 59, name: "Side" };
    expect(resetCameraName(side)).toBe(side);
  });
  it("rejects invalid labels and fractional steps before saving", () => {
    for (const name of ["", " ", "x".repeat(20), "é", "line\nbreak"])
      expect(cameraDraftError([{ ...camera, id: 51, name }])).not.toBeNull();
    expect(cameraDraftError([{ ...camera, height: 1.5 }])).toContain(
      "whole steps",
    );
    expect(cameraDraftError([{ ...camera, zoom: 101 }])).toContain(
      "whole steps",
    );
    expect(
      cameraDraftError([
        { ...camera, id: 51, name: "Same" },
        { ...camera, id: 53, name: "same" },
      ]),
    ).toContain("unique");
  });
  it("refuses an enabled next-launch plan with no selected camera", () => {
    const none = [{ ...camera, enabled: false }];
    expect(cameraSetupError(none, true)).toContain("at least one camera");
    expect(cameraSetupError(none, false)).toBeNull();
    expect(cameraSetupError([{ ...camera, enabled: true }], true)).toBeNull();
  });
});
