import { describe, it, expect } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  DownloadLibrary,
  artworkUrl,
  downloadPercent,
  mergeEditorial,
} from "./DownloadLibrary";
describe("curated library", () => {
  it("has both collections without showing curator before native capability", () => {
    const html = renderToStaticMarkup(<DownloadLibrary navigate={() => {}} />);
    expect(html).toContain("Startup videos");
    expect(html).toContain("Playlists");
    expect(html).toContain("Blacktop Previews");
    expect(html).not.toContain("Publish to GitHub");
  });
  it("only displays shipped or raw GitHub artwork", () => {
    expect(artworkUrl("javascript:alert(1)")).toBe("");
    expect(artworkUrl("https://evil.test/cover.jpg")).toBe("");
    expect(artworkUrl("art/nba-2k14.jpg")).not.toBe("");
  });
  it("clamps observed download progress", () => {
    expect(downloadPercent(50, 100)).toBe(50);
    expect(downloadPercent(200, 100)).toBe(100);
    expect(downloadPercent(0, 0)).toBe(0);
  });
  it("preserves curator edits when finding newly uploaded items", () => {
    const c = {
      schema: 1,
      items: [
        {
          id: "a",
          title: "Edited",
          description: "Keep",
          artwork: "",
          order: 3,
          hidden: true,
        },
      ],
    };
    const item = {
      id: "b",
      kind: "videos",
      name: "New",
      description: "",
      artwork: "",
      order: 5,
      bytes: 3,
      supported: true,
      reason: "",
      installed: false,
    };
    const result = mergeEditorial(c, [{ ...item, id: "a" }, item]);
    expect(result.items).toHaveLength(2);
    expect(result.items[0]).toEqual(c.items[0]);
    expect(result.items[1].title).toBe("New");
  });
});
