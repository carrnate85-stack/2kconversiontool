import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { arenaTextureLimitReached, ModelPreviewDialog, PreviewObjectInspector } from "./ModelPreviewDialog";

vi.mock("./api", () => ({ call: vi.fn() }));

describe("ModelPreviewDialog", () => {
  it("allows Load all textures to continue beyond the staged memory threshold", () => {
    expect(arenaTextureLimitReached(false, 120 * 1024 * 1024, 16 * 1024 * 1024)).toBe(true);
    expect(arenaTextureLimitReached(true, 120 * 1024 * 1024, 16 * 1024 * 1024)).toBe(false);
  });

  it("opens with the Exporter-style accessible preview spinner", () => {
    const html = renderToStaticMarkup(
      <ModelPreviewDialog
        mod={{ name: "png0001.iff", path: "players\\png0001.iff", bytes: 1, folder: "players" }}
        title="Player png0001"
        onClose={() => {}}
      />,
    );
    expect(html).toContain('class="model-preview-loading"');
    expect(html).toContain('aria-busy="true"');
    expect(html).toContain("Reading the selected mod");
    expect(html).toContain("Cancel preview");
  });

  it("shows the selected arena object's renderer metadata", () => {
    const html = renderToStaticMarkup(<PreviewObjectInspector pick={{
      name: "basket0",
      target: "arena_015:basket",
      model: "basket_model",
      primitive: "rim_mesh",
      material: { name: "basket_metal", shader: "default", source: "arena", textures: [{ name: "basket_d.dds", channel: "albedo", applied: true, status: "loaded" }] },
    }} />);
    expect(html).toContain("basket0");
    expect(html).toContain("rim_mesh");
    expect(html).toContain("basket_d.dds");
    expect(html).toContain("loaded · applied");
  });
});
