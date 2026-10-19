import { describe, expect, it } from "vitest";
import {
  PROFILES_LOCKED,
  defaultModsFolder,
  isDefaultProfile,
  newProfileName,
  profileDialogError,
  profilesAfterDialog,
  profilesLocked,
  samePath,
} from "./profiles-state";

const profiles = [
  { name: "Default", folder: "C:\\Games\\NBA 2K27\\mods" },
  { name: "Retro", folder: "C:\\Mods\\Retro" },
  { name: "WNBA", folder: "C:\\Mods\\WNBA" },
];

describe("profile workflow (WPF rules)", () => {
  it("recognises the game's default mods folder", () => {
    expect(defaultModsFolder("C:\\Games\\NBA 2K27\\")).toBe(
      "C:\\Games\\NBA 2K27\\mods",
    );
    expect(isDefaultProfile(profiles[0], "c:\\games\\nba 2k27")).toBe(true);
    expect(isDefaultProfile(profiles[1], "C:\\Games\\NBA 2K27")).toBe(false);
    expect(isDefaultProfile(profiles[0], "")).toBe(false);
    expect(samePath("C:\\Mods\\Retro\\", "c:/mods/retro")).toBe(false);
    expect(samePath("C:\\Mods\\Retro\\", "c:\\mods\\retro")).toBe(true);
  });

  it("names new profiles after their folder and keeps names unique", () => {
    expect(newProfileName(profiles, "D:\\Packs\\Classic 2K\\")).toBe(
      "Classic 2K",
    );
    expect(newProfileName(profiles, "D:\\Other\\retro")).toBe("retro 2");
    expect(newProfileName(profiles, "D:\\")).toBe("D:");
  });

  it("validates the dialog like WPF", () => {
    const create = {
      mode: "create" as const,
      original: null,
      folder: "D:\\New",
      name: "New",
    };
    expect(profileDialogError(profiles, create, true)).toBe(PROFILES_LOCKED);
    expect(profileDialogError(profiles, { ...create, name: " " }, false)).toBe(
      "Enter a profile name of 1–80 characters.",
    );
    expect(
      profileDialogError(
        profiles,
        { ...create, folder: "c:\\mods\\retro\\" },
        false,
      ),
    ).toContain("already saved in another profile");
    expect(
      profileDialogError(profiles, { ...create, name: "wnba" }, false),
    ).toBe("A profile with that name already exists.");
    expect(profileDialogError(profiles, create, false)).toBe("");
    const edit = {
      mode: "edit" as const,
      original: profiles[1],
      folder: profiles[1].folder,
      name: "RETRO",
    };
    expect(profileDialogError(profiles, edit, false)).toBe("");
    expect(
      profileDialogError(
        profiles,
        { ...edit, folder: profiles[2].folder },
        false,
      ),
    ).toContain("already saved");
    expect(
      profileDialogError(
        profiles,
        {
          ...edit,
          folder: "D:\\Elsewhere",
          original: { name: "Gone", folder: "X" },
        },
        false,
      ),
    ).toContain("no longer saved");
  });

  it("creates and edits without touching other profiles", () => {
    expect(
      profilesAfterDialog(profiles, {
        mode: "create",
        original: null,
        folder: " D:\\New ",
        name: " New ",
      }),
    ).toEqual([...profiles, { name: "New", folder: "D:\\New" }]);
    expect(
      profilesAfterDialog(profiles, {
        mode: "edit",
        original: profiles[1],
        folder: "D:\\R",
        name: "Retro 2",
      })[1],
    ).toEqual({ name: "Retro 2", folder: "D:\\R" });
  });

  it("locks profiles only while a game session is active", () => {
    expect(profilesLocked("game-running")).toBe(true);
    expect(profilesLocked("waiting-for-game")).toBe(true);
    expect(profilesLocked("idle")).toBe(false);
    expect(profilesLocked(undefined)).toBe(false);
  });
});
