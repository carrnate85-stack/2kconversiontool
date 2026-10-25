import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ActionError, friendlyError } from "./UxFeedback";

describe("actionable failures", () => {
  it("explains known failures while retaining the complete evidence", () => {
    const text = "Scoreboard restore descriptor ownership data changed: span 16, 0x987654";
    const html = renderToStaticMarkup(<ActionError text={text} />);
    expect(html).toContain("Close the game normally");
    expect(html).toContain(text);
    expect(html).toContain("Copy report");
  });
  it("retains short actionable errors and directs unsupported builds to compatibility", () => {
    expect(friendlyError("Error: Choose a video first")).toBe("Choose a video first");
    expect(friendlyError("Unsupported game build")).toContain("Settings");
    expect(renderToStaticMarkup(<ActionError text="" />)).toBe("");
  });
});
