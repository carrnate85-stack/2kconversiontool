import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { NativeFeatureStatus } from "./NativeFeatureStatus";
describe("Version 2 per-feature status", () => {
  it("surfaces exact Blacktop waiting and held progress in the launcher", () => {
    const text = renderToStaticMarkup(
      <NativeFeatureStatus
        issuesOnly
        status={{
          features: {
            "wnba-blacktop": {
              phase: "held",
              message: "Category owner changed; close the game",
            },
          },
        }}
      />,
    );
    expect(text).toContain("WNBA Blacktop");
    expect(text).toContain("Category owner changed; close the game");
    expect(text).toContain('role="alert"');
    const waiting = renderToStaticMarkup(
      <NativeFeatureStatus
        status={{
          features: {
            "wnba-blacktop": {
              phase: "waiting",
              message: "Waiting for stable player selection",
            },
          },
        }}
      />,
    );
    expect(waiting).toContain("Waiting for stable player selection");
    expect(waiting).not.toContain('role="alert"');
  });
  it("surfaces camera/roster failures outside Advanced without duplicating healthy details", () => {
    const text = renderToStaticMarkup(
      <NativeFeatureStatus
        issuesOnly
        status={{
          features: {
            cameras: { phase: "held", message: "Relocation check refused" },
            scaler: { phase: "active", message: "Shared input verified" },
            "wnba-freestyle": {
              phase: "unavailable",
              message: "Loaded roster has 0/15 WNBA clubs; not applied",
            },
            presentation: { phase: "disabled", message: "Not selected" },
          },
        }}
      />,
    );
    expect(text).toContain("Relocation check refused");
    expect(text).toContain("0/15 WNBA clubs");
    expect(text.match(/role="alert"/g)).toHaveLength(2);
    expect(text).not.toContain("Shared input verified");
    expect(text).not.toContain("Not selected");
    expect(
      renderToStaticMarkup(
        <NativeFeatureStatus
          issuesOnly
          status={{ features: { scaler: { phase: "active", message: "OK" } } }}
        />,
      ),
    ).toBe("");
  });
  it("makes no enabled or verified claim before provider status exists", () => {
    expect(renderToStaticMarkup(<NativeFeatureStatus />)).toBe("");
    expect(renderToStaticMarkup(<NativeFeatureStatus status={null} />)).toBe(
      "",
    );
  });
  it("keeps mod-loader success distinct from waiting and held game features", () => {
    const text = renderToStaticMarkup(
      <NativeFeatureStatus
        status={{
          features: {
            cameras: { phase: "waiting", message: "Return to main menu" },
            scaler: {
              phase: "held",
              message: "Input changed outside this session",
            },
            "wnba-freestyle": {
              phase: "active",
              message: "Navigation prepared; gameplay verification pending",
            },
          },
        }}
      />,
    );
    expect(text).toContain("Cameras");
    expect(text).toContain("Return to main menu");
    expect(text).toContain('role="alert"');
    expect(text).toContain("Input changed outside this session");
    expect(text).toContain("gameplay verification pending");
    expect(text).not.toContain("verified in game");
  });
  it("renders diagnostic text safely and does not invent unknown controls", () => {
    const text = renderToStaticMarkup(
      <NativeFeatureStatus
        status={{
          features: {
            cameras: { phase: "held", message: "<script>unknown</script>" },
            unknown: { phase: "active", message: "unmapped control" },
          },
        }}
      />,
    );
    expect(text).toContain("&lt;script&gt;");
    expect(text).not.toContain("<script>");
    expect(text).not.toContain("unmapped control");
  });
  it("shows court presentation separately without claiming a pending crowd is removed", () => {
    const text = renderToStaticMarkup(
      <NativeFeatureStatus
        status={{
          features: {
            presentation: {
              phase: "waiting",
              message: "Waiting for stable Blacktop selection",
            },
          },
        }}
      />,
    );
    expect(text).toContain("Court presentation");
    expect(text).toContain("Waiting for stable Blacktop selection");
    expect(text).not.toContain("active");
  });
  it("surfaces camera/scaler resets that require returning to a safe menu", () => {
    const text = renderToStaticMarkup(
      <NativeFeatureStatus
        issuesOnly
        status={{
          features: {
            cameras: { phase: "reload", message: "Camera data is changing" },
            scaler: { phase: "reset", message: "Return to Main" },
          },
        }}
      />,
    );
    expect(text).toContain("Camera data is changing");
    expect(text).toContain("Return to Main");
    expect(text.match(/role="alert"/g)).toHaveLength(2);
  });
});
