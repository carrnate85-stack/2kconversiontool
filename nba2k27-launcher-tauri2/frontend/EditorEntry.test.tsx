import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { EditorEntry, LocalEditorTools } from "./EditorEntry";

describe("browser-first editing", () => {
  it.each(["players", "teams"] as const)(
    "leads %s with one browser action and an explicit sample preview",
    (section) => {
      const html = renderToStaticMarkup(
        <EditorEntry section={section} onError={() => {}} />,
      );
      expect(html.match(/data-editor-open/g)).toHaveLength(1);
      expect(html).toContain('class="primary"');
      expect(html).toContain("Preview with sample data");
      expect(html).toContain("Sample previews cannot read or change the game");
      expect(html).toContain("Live gameplay acceptance is pending");
    },
  );
  it("preserves local tools while keeping their separate save target collapsed", () => {
    const html = renderToStaticMarkup(
      <LocalEditorTools>
        <button>Export local DNA</button>
      </LocalEditorTools>,
    );
    expect(html).toContain("Export local DNA");
    expect(html).toContain("They do not apply changes to your game roster");
    expect(html).not.toMatch(/<details[^>]*\sopen(?:[\s=>])/);
  });
});
