import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  LaunchOverlay,
  elapsedLabel,
  forwardProgress,
  launchProgress,
} from "./LaunchOverlay";

describe("WPF launch overlay", () => {
  it("uses WPF stage names, wording and a forward-only bar", () => {
    for (const phase of ["queued", "preparing-mods", "verifying-game"]) {
      expect(launchProgress(phase).step).toBe("Preparing launch");
    }
    const jukebox = launchProgress("preparing-jukebox");
    expect(jukebox.step).toBe("Preparing playlist audio");
    expect(jukebox.message).toBe(
      "Preparing the selected playlist for in-game playback…",
    );
    expect(launchProgress("waiting-for-game").step).toBe("Opening NBA 2K27");
    expect(launchProgress("loader-armed").step).toBe("Game connected");
    expect(forwardProgress(66, launchProgress("preparing-cameras"))).toBe(66);
    const ready = launchProgress("loader-active");
    expect(ready.title).toBe("NBA 2K27 is ready");
    expect(ready.state).toBe("ready");
    expect(launchProgress("mystery-phase").step).toBe("mystery phase");
  });
  it("stops safely on a failure like WPF loader-failed", () => {
    for (const value of [
      launchProgress("preparing-loader", "Steam refused the launch"),
      launchProgress("held-until-game-exit"),
      launchProgress("needs-review"),
    ]) {
      expect(value.title).toBe("Startup needs attention");
      expect(value.step).toBe("Stopped safely");
      expect(value.progress).toBe(100);
    }
    expect(launchProgress(null, "boom").message).toBe("boom");
  });
  it("renders the WPF dialog markup and elapsed seconds", () => {
    expect(elapsedLabel(1200)).toBe("1 second");
    expect(elapsedLabel(5900)).toBe("5 seconds");
    const html = renderToStaticMarkup(
      <LaunchOverlay
        startedAt={Date.now()}
        phase="preparing-loader"
        onDone={() => {}}
      />,
    );
    expect(html).toContain('aria-label="NBA 2K27 startup progress"');
    expect(html).toContain("STARTING GAME");
    expect(html).toContain("Starting NBA 2K27…");
    expect(html).toContain("Preparing mod support");
    expect(html).toContain(
      "Keep this launcher open. This window clears automatically when the game is ready.",
    );
  });
  it("keeps failure recovery and technical details available", () => {
    const html = renderToStaticMarkup(<LaunchOverlay startedAt={Date.now()} phase="needs-review" error="Scoreboard restore descriptor ownership data changed: 0x123" onDone={() => {}} />);
    expect(html).toContain("Scoreboard switching is paused");
    expect(html).toContain("Copy report");
    expect(html).toContain("Dismiss");
    expect(html).toContain("0x123");
  });
});
