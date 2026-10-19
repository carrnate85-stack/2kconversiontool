import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { Profiles } from "./Workflows";
import type { Library, Settings } from "./types";

vi.mock("./api", () => ({
  call: vi.fn(),
  garmentImage: vi.fn(),
}));

const settings: Settings = {
  gameRoot: "C:\\NBA 2K27",
  modsRoot: "C:\\Mods\\Default",
  profile: "Default",
  autoShowOverlay: true,
  autoHideOverlay: true,
  removeDrone: false,
  removeDj: false,
  removeMc: false,
  removeCrowd: false,
  lockCamera: false,
  camerasEnabled: true,
  startupVideo: false,
  camera: "",
  arena: "",
  ball: "",
  scoreboard: "",
  theme: "dark",
  video: "",
  music: "",
  unlockWnba: false,
  unlockWnbaFreestyle: false,
  scaler: 1,
  scalerCoaches: true,
  scalerCheerleaders: true,
  scalerArenaPersonnel: true,
  autoRandomOutfits: true,
  usePlaylistInFreestyle: true,
};

function library(): Library {
  return {
    profiles: [
      { name: "Default", folder: "C:\\Mods\\Default" },
      { name: "Needs folder", folder: "" },
    ],
    cameras: [],
    playlists: [],
    outfits: [],
    tags: {},
  };
}

describe("Profiles workspace", () => {
  it("renders the saved profile list and session guide without mutating data", () => {
    let writes = 0;
    const html = renderToStaticMarkup(
      <Profiles
        library={library()}
        save={async () => {
          writes++;
        }}
        settings={settings}
        change={() => {
          writes++;
        }}
        status={() => {}}
      />,
    );
    expect(writes).toBe(0);
    expect(html).toContain("2 PROFILES");
    expect(html).toContain("How profiles work");
    expect(html).toContain("Automatic restoration");
    expect(html).toContain("Safe removal");
    expect(html).toContain("Folder not selected — edit this profile");
  });

  it("follows WPF row rules: radio activation, no Edit/Remove on the game folder, no Remove on the active row", () => {
    const html = renderToStaticMarkup(
      <Profiles
        library={{
          ...library(),
          profiles: [
            { name: "Game", folder: "C:\\NBA 2K27\\mods" },
            { name: "Default", folder: "C:\\Mods\\Default" },
            { name: "Retro", folder: "C:\\Mods\\Retro" },
          ],
        }}
        save={async () => {}}
        settings={settings}
        change={() => {}}
        status={() => {}}
      />,
    );
    expect(html).toContain('role="radiogroup"');
    expect(html).toContain("Choose a profile to make it active.");
    expect(html).not.toContain('aria-label="Edit Game profile"');
    expect(html).not.toContain('aria-label="Remove Game profile"');
    expect(html).toContain('aria-label="Edit Default profile"');
    expect(html).not.toContain('aria-label="Remove Default profile"');
    expect(html).toContain('aria-label="Remove Retro profile"');
    expect(html).toContain("C:\\Mods\\Default");
  });

  it("shows a repair message when the stored active profile is absent", () => {
    const html = renderToStaticMarkup(
      <Profiles
        library={library()}
        save={async () => {}}
        settings={{ ...settings, profile: "Missing" }}
        change={() => {}}
        status={() => {}}
      />,
    );
    expect(html).toContain("saved active profile is missing");
  });
});
