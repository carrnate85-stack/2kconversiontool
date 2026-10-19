import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ModInstallDialog, type ModInstallState } from "./Workflows";
import type { ModInstallPlan } from "./mod-install";

vi.mock("./api", () => ({
  call: vi.fn(),
  garmentImage: vi.fn(),
}));

const plan: ModInstallPlan = {
  cancelled: false,
  planId: "abc",
  root: "D:\\Mods\\Default",
  items: [
    {
      name: "arena_999.iff",
      logicalPath: "arenas/arena_999.iff",
      routing: "manifest",
      size: 2048,
      existing: { size: 1024, modifiedMs: 1 },
    },
    {
      name: "png1234.iff",
      logicalPath: "char/sig/png1234.iff",
      routing: "player-char-sig",
      size: 10,
      existing: null,
    },
  ],
  issues: [],
  ignored: [{ name: "readme.txt", reason: "Package documentation." }],
  totalBytes: 2058,
  replacements: 1,
  archives: 1,
  folders: 0,
};
const render = (state: ModInstallState, busy = false) =>
  renderToStaticMarkup(
    <ModInstallDialog
      state={state}
      busy={busy}
      error=""
      close={() => undefined}
      choose={() => undefined}
      decide={() => undefined}
      commit={() => undefined}
    />,
  );

describe("ModInstallDialog", () => {
  it("offers files/archives or a folder", () => {
    const html = render({ stage: "choose" });
    expect(html).toContain("Files or archives");
    expect(html).toContain("Folder");
    expect(html).toContain("Nothing is installed until you confirm.");
    expect(html).toContain("A folder is");
  });
  it("lists every conflict with Replace / Keep existing, keep by default", () => {
    const html = render({
      stage: "review",
      plan,
      decisions: { "arenas/arena_999.iff": "keep" },
    });
    expect(html).toContain("Replace existing mod files?");
    expect(html).toContain("1 destination already contains a mod.");
    expect(html).toContain("arenas/arena_999.iff");
    expect(html).toContain('aria-pressed="true">Keep existing');
    expect(html).toContain('aria-pressed="false">Replace');
    expect(html).toContain("1 new file will be added");
    expect(html).toContain("1 to install · 0 replaced · 1 kept");
    expect(html).toContain(">Install<");
    const replacing = render({
      stage: "review",
      plan,
      decisions: { "arenas/arena_999.iff": "replace" },
    });
    expect(replacing).toContain("Replace and install");
  });
  it("shows routing issues and installs nothing", () => {
    const html = render({
      stage: "issues",
      plan: {
        ...plan,
        planId: null,
        issues: [{ name: "mystery.iff", reason: "Filename is not listed." }],
      },
    });
    expect(html).toContain("1 file could not be routed");
    expect(html).toContain("mystery.iff");
    expect(html).toContain("The mods folder was not changed.");
    expect(html).not.toContain(">Install<");
  });
});
