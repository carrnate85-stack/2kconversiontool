import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { LaunchReadiness } from "./LaunchReadiness";
import {
  inspectPlay,
  launchRequest,
  playNeedsConfirm,
  playSummary,
  readyToConfirm,
  sessionPollInterval,
  type PlayReport,
} from "./PlayLaunch";

vi.mock("./api", () => ({
  call: vi.fn(() => {
    throw new Error("No backend call during static rendering");
  }),
}));
function ready(): PlayReport {
  return {
    readOnly: true,
    checksPassed: true,
    processPresenceKnown: true,
    gameRunning: false,
    recoveryRequired: false,
    liveGameEnabled: false,
    blockers: [],
    note: "Read-only fixture",
    processes: [],
    launchPlan: {
      appId: "123456",
      label: "Without EAC",
      steamUrl: "steam://launch/123456/option1",
    },
  };
}
describe("Simple guarded Play home", () => {
  it("shows the real preparation stage instead of claiming Steam has already launched", () => {
    const value = playSummary(
      {
        phase: "waiting-for-game",
        nativeProvider: {
          phase: "preparing-cameras",
          message: "Preparing camera names",
        },
      },
      null,
    );
    expect(value.label).toBe("Session active");
    expect(value.title).toBe("Preparing your session");
    expect(value.message).toBe("Preparing camera names");
    expect(
      playSummary(
        {
          phase: "game-running",
          nativeProvider: {
            phase: "waiting-for-loader",
            message: "Waiting for the native loader",
          },
        },
        null,
      ).label,
    ).toBe("Session active");
  });
  it("retains active failures but does not call a fully closed session blocked by a historical error", () => {
    const provider = {
      phase: "closed",
      error: "The previous launch was interrupted",
    };
    expect(
      playSummary({ phase: "closed", nativeProvider: provider }, null).label,
    ).toBe("Ready");
    const active = playSummary(
      { phase: "game-running", nativeProvider: provider },
      null,
    );
    expect(active.label).toBe("Needs attention");
    expect(active.message).toContain("interrupted");
    expect(
      playSummary(
        { phase: "closed", nativeProvider: provider },
        { ...ready(), recoveryRequired: true },
      ).label,
    ).toBe("Needs attention");
  });
  it("shows one real Play button and preserved choices, with Advanced closed and no mounted QA stack", () => {
    const html = renderToStaticMarkup(
      <LaunchReadiness gameRoot="C:/Game" modsRoot="C:/Mods">
        <div>Mod profile · Jukebox playlist · Startup video</div>
      </LaunchReadiness>,
    );
    expect(html.match(/data-primary-play=/g)).toHaveLength(1);
    expect(html).toContain('aria-label="Play NBA 2K27"');
    expect(html).toContain("Play session");
    expect(html).toContain('data-play-advanced="true"');
    expect(html).not.toMatch(/<details[^>]*\sopen(?:=|\s|>)/);
    expect(html).toContain("Advanced");
    expect(html).toContain("Mod profile · Jukebox playlist · Startup video");
    expect(html).not.toContain("Preview session");
    expect(html).not.toContain("Launch with mod profile (QA)");
    expect(html).not.toContain("Native Blacktop observer");
    expect(html).not.toContain("native-launch-confirm");
  });
  it("does not expose a clickable Play when no game folder is configured", () => {
    const html = renderToStaticMarkup(
      <LaunchReadiness gameRoot="" modsRoot="" />,
    );
    expect(html).toMatch(/<button[^>]*data-primary-play[^>]*disabled/);
    expect(html).toContain("Choose your NBA 2K27 folder in Settings.");
  });
  it("runs a fresh read-only preflight on every Play preparation and never dispatches a launch", async () => {
    const channels: string[] = [];
    const request = async <T,>(channel: string) => {
      channels.push(channel);
      return ready() as T;
    };
    expect(readyToConfirm(await inspectPlay(request))).toBe(true);
    expect(readyToConfirm(await inspectPlay(request))).toBe(true);
    expect(channels).toEqual(["launch-preflight", "launch-preflight"]);
  });
  it("requires all known safe preflight conditions rather than one truthy checksPassed field", () => {
    expect(readyToConfirm(ready())).toBe(true);
    expect(readyToConfirm(null)).toBe(false);
    for (const change of [
      { checksPassed: false },
      { processPresenceKnown: false },
      { gameRunning: true },
      { gameRunning: null },
      { recoveryRequired: true },
      { readOnly: false },
      { launchPlan: null },
      { checksPassed: "true" },
    ]) {
      expect(readyToConfirm({ ...ready(), ...change } as PlayReport)).toBe(
        false,
      );
    }
  });
  it("does not confirm malformed or failed readiness responses", async () => {
    await expect(
      inspectPlay(async <T,>() => ({ checksPassed: true }) as T),
    ).rejects.toThrow("No game launch was sent");
    const blocked = {
      ...ready(),
      checksPassed: false,
      blockers: ["Another launcher owns the session"],
    };
    expect(
      readyToConfirm(await inspectPlay(async <T,>() => blocked as T)),
    ).toBe(false);
  });
  it("launches normal modded Play in one click and confirms only the Steam-only QA path", () => {
    expect(playNeedsConfirm("modded")).toBe(false);
    expect(playNeedsConfirm("steam-only")).toBe(true);
    // Risky states never reach launch at all; they are blocked, not confirmed.
    for (const change of [
      { recoveryRequired: true },
      { checksPassed: false, blockers: ["Unsupported game build"] },
      { gameRunning: true },
      { processPresenceKnown: false },
    ]) {
      expect(readyToConfirm({ ...ready(), ...change } as PlayReport)).toBe(
        false,
      );
    }
    const html = renderToStaticMarkup(
      <LaunchReadiness gameRoot="C:/Game" modsRoot="C:/Mods" />,
    );
    expect(html).not.toContain('type="checkbox"');
    expect(html).not.toContain("I will play offline");
  });
  it("preserves exact native modded and Steam-only command acknowledgements", () => {
    expect(launchRequest(true, "C:/Game", "C:/Mods")).toEqual({
      channel: "native-launch-modded",
      args: {
        acknowledgement: "native-modded-qa-explicit-blacktop",
        gameRoot: "C:/Game",
        modsRoot: "C:/Mods",
      },
    });
    expect(launchRequest(false, "C:/Game", "C:/Mods")).toEqual({
      channel: "native-launch-steam-only",
      args: { acknowledgement: "steam-only-no-features", gameRoot: "C:/Game" },
    });
  });
  it("avoids idle status IPC churn while keeping active launch monitoring responsive", () => {
    expect(sessionPollInterval(null, false)).toBe(5000);
    expect(sessionPollInterval("idle", false)).toBe(5000);
    expect(sessionPollInterval("closed", false)).toBe(5000);
    expect(sessionPollInterval("waiting-for-game", false)).toBe(1000);
    expect(sessionPollInterval("game-running", false)).toBe(1000);
    expect(sessionPollInterval("needs-review", false)).toBe(1000);
    expect(sessionPollInterval("idle", true)).toBe(1000);
  });
  it("uses real session phases without claiming game feature or playback verification", () => {
    expect(playSummary({ phase: "game-running" }, null).label).toBe(
      "Running",
    );
    expect(playSummary({ phase: "game-running" }, null).title).toBe(
      "Session in progress",
    );
    expect(playSummary({ phase: "waiting-for-game" }, null).label).toBe(
      "Session active",
    );
    expect(playSummary({ phase: "needs-review" }, null).label).toBe(
      "Needs attention",
    );
    expect(
      playSummary(
        {
          phase: "game-running",
          nativeProvider: { error: "Owned feature held" },
        },
        null,
      ).label,
    ).toBe("Needs attention");
    expect(playSummary({ phase: "closed" }, null).message).not.toContain(
      "verified",
    );
  });
});
