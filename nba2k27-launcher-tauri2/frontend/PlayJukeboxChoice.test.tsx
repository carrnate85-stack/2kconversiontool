import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  PlayJukeboxChoice,
  playJukeboxCopy,
  validatePlayJukeboxSummary,
} from "./PlayJukeboxChoice";

const selected = {
  libraryReady: true,
  revision: "a".repeat(64),
  playlistCount: 2,
  playlists: [
    {
      id: "b".repeat(48),
      name: "NBA 2K12",
      songCount: 28,
    },
    {
      id: "c".repeat(48),
      name: "Warmups",
      songCount: 12,
    },
  ],
  playlistId: "b".repeat(48),
  playlistName: "NBA 2K12",
  songCount: 28,
};

describe("Play Jukebox selection controls", () => {
  it("shows the authoritative playlist choices and enabled next-Play state", () => {
    const html = renderToStaticMarkup(
      <PlayJukeboxChoice
        rememberedId=""
        onRemember={() => undefined}
        initialSummary={selected}
      />,
    );
    expect(html).toContain("Jukebox playlist");
    expect(html).toContain("NBA 2K12");
    expect(html).toContain("Warmups");
    expect(html).toContain('aria-label="Enable Jukebox playlist"');
    expect(html).toMatch(/Enable Jukebox playlist[^>]*checked/);
    expect(html).not.toContain("Selected playlist songs");
  });
  it("retains a remembered dropdown choice while Jukebox is off", () => {
    const disabled = {
      ...selected,
      playlistId: null,
      playlistName: null,
      songCount: 0,
    };
    const html = renderToStaticMarkup(
      <PlayJukeboxChoice
        rememberedId={"c".repeat(48)}
        onRemember={() => undefined}
        initialSummary={disabled}
      />,
    );
    expect(html).toContain("Warmups");
    expect(html).not.toMatch(/Enable Jukebox playlist[^>]*checked/);
    expect(playJukeboxCopy(disabled)).toContain("off");
  });
  it("rejects inconsistent, stale-looking or renderer-invented summaries", () => {
    expect(() =>
      validatePlayJukeboxSummary({ ...selected, playlistId: "../bad" }),
    ).toThrow("invalid");
    expect(() =>
      validatePlayJukeboxSummary({ ...selected, playlistName: null }),
    ).toThrow("invalid");
    expect(() =>
      validatePlayJukeboxSummary({
        ...selected,
        playlists: [{ ...selected.playlists[0], songCount: 29 }],
      }),
    ).toThrow("invalid");
    expect(() =>
      validatePlayJukeboxSummary({ ...selected, revision: "old" }),
    ).toThrow("invalid");
  });
});
