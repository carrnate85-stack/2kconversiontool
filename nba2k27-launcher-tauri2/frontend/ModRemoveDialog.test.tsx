import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ModRemoveDialog } from "./Workflows";
import type { Mod } from "./types";

vi.mock("./api", () => ({ call: vi.fn(), garmentImage: vi.fn() }));

const file = (path: string): Mod => ({
  name: path.split("\\").at(-1) ?? path,
  path,
  bytes: 100,
  folder: path.split("\\").slice(0, -1).join("\\"),
});

describe("ModRemoveDialog", () => {
  it("names every bundled file and promises a recoverable Recycle Bin move", () => {
    const html = renderToStaticMarkup(
      <ModRemoveDialog
        group={{
          id: "png1",
          title: "Player png1",
          stem: "png1",
          category: "Player",
          tags: [],
          bytes: 200,
          base: file("players\\png1.iff"),
          files: [file("players\\png1.iff"), file("players\\png1_item_hair.iff")],
        }}
        busy={false}
        error=""
        cancel={() => undefined}
        confirm={() => undefined}
      />,
    );
    expect(html).toContain("Player png1");
    expect(html).toContain("includes 2 bundled files");
    expect(html).toContain("players/png1.iff");
    expect(html).toContain("players/png1_item_hair.iff");
    expect(html).toContain("Windows Recycle Bin");
    expect(html).toContain("Move to Recycle Bin");
  });

  it("locks both choices while the recycle operation is running", () => {
    const html = renderToStaticMarkup(
      <ModRemoveDialog
        group={{
          id: "arena",
          title: "Arena",
          stem: "",
          category: "Arena",
          tags: [],
          bytes: 100,
          base: file("arenas\\arena.iff"),
          files: [file("arenas\\arena.iff")],
        }}
        busy
        error=""
        cancel={() => undefined}
        confirm={() => undefined}
      />,
    );
    expect(html).toContain("is one file");
    expect(html).toContain("Moving…");
    expect(html.match(/disabled/g)).toHaveLength(2);
  });
});
