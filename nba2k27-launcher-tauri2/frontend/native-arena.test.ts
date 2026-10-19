import { expect, test } from "vitest";
import {
  arenaEligible,
  mergeCatalogStatus,
  catalogReplyCurrent,
  catalogUnavailableMessage,
  type ArenaSession,
  type ArenaStatus,
} from "./NativeArena";
test("arena controls require the current active modded session and reject held/ended owners", () => {
  const valid: ArenaSession = {
    session: "a".repeat(48),
    phase: "game-running",
    nativeProvider: {
      phase: "loader-active",
      build: { nativeWritesMapped: true },
    },
  };
  expect(arenaEligible(valid)).toBe(true);
  for (const session of [
    null,
    { ...valid, session: "old" },
    { ...valid, error: "needs review" },
    { ...valid, phase: "waiting-for-game" },
    { ...valid, nativeProvider: { phase: "waiting-for-loader" } },
  ])
    expect(arenaEligible(session)).toBe(false);
  for (const phase of ["held", "ended"])
    expect(
      arenaEligible({
        ...valid,
        nativeProvider: { phase: "loader-active", arena: { phase } as any },
      }),
    ).toBe(false);
});
test("stale catalog polls cannot erase an acknowledged request, but new sessions and terminal states replace it", () => {
  const queued = { phase: "queued", revision: 2 } as ArenaStatus,
    older = { phase: "ready", revision: 1 } as ArenaStatus;
  expect(mergeCatalogStatus(queued, older, true)).toBe(queued);
  expect(mergeCatalogStatus(queued, older, false)).toBe(older);
  for (const phase of ["held", "ended"]) {
    const terminal = { ...older, phase };
    expect(mergeCatalogStatus(queued, terminal, true)).toBe(terminal);
  }
  expect(mergeCatalogStatus(queued, null, true)).toBeNull();
});
test("catalog replies cannot cross game sessions or a changed UI generation", () => {
  expect(catalogReplyCurrent(1, 1, "a", "a")).toBe(true);
  expect(catalogReplyCurrent(1, 2, "a", "a")).toBe(false);
  expect(catalogReplyCurrent(1, 1, "a", "b")).toBe(false);
  expect(catalogReplyCurrent(1, 1, "a", undefined)).toBe(false);
});
test("ball eligibility is independent of arena errors and requires its own active owner", () => {
  const valid: ArenaSession = {
    session: "b".repeat(48),
    phase: "game-running",
    nativeProvider: {
      phase: "loader-active",
      build: { nativeWritesMapped: true },
      arena: { phase: "held" } as any,
    },
  };
  expect(arenaEligible(valid, "arena")).toBe(false);
  expect(arenaEligible(valid, "ball")).toBe(true);
  for (const phase of ["held", "ended"]) {
    expect(
      arenaEligible(
        {
          ...valid,
          nativeProvider: { phase: "loader-active", ball: { phase } as any },
        },
        "ball",
      ),
    ).toBe(false);
  }
  expect(
    arenaEligible(
      { ...valid, nativeProvider: { phase: "waiting-for-loader" } },
      "ball",
    ),
  ).toBe(false);
});
test("missing independent build authority disables both memory catalogs", () => {
  for (const build of [
    undefined,
    { label: "Version 2", nativeWritesMapped: false },
    { label: "Version 2", version2ArenaBallMapped: true },
    {
      label: "Unknown",
      modProviderMapped: true,
      version2ArenaBallMapped: true,
    },
  ]) {
    const s: ArenaSession = {
      phase: "game-running",
      session: "a".repeat(48),
      nativeProvider: { phase: "loader-active", build },
    };
    expect(arenaEligible(s, "arena")).toBe(false);
    expect(arenaEligible(s, "ball")).toBe(false);
  }
});
test("Version 2 arena and ball use their own capability without enabling legacy writes", () => {
  const s: ArenaSession = {
    phase: "game-running",
    session: "a".repeat(48),
    nativeProvider: {
      phase: "loader-active",
      build: {
        label: "Version 2",
        nativeWritesMapped: false,
        modProviderMapped: true,
        version2ArenaBallMapped: true,
        version2GameplayBallMapped: true,
      },
    },
  };
  expect(arenaEligible(s, "arena")).toBe(true);
  expect(arenaEligible(s, "ball")).toBe(true);
  expect(s.nativeProvider?.build?.nativeWritesMapped).toBe(false);
  for (const phase of ["held", "ended"]) {
    expect(
      arenaEligible(
        {
          ...s,
          nativeProvider: {
            ...s.nativeProvider,
            arena: { phase } as ArenaStatus,
          },
        },
        "arena",
      ),
    ).toBe(false);
    expect(
      arenaEligible(
        {
          ...s,
          nativeProvider: {
            ...s.nativeProvider,
            ball: { phase } as ArenaStatus,
          },
        },
        "ball",
      ),
    ).toBe(false);
  }
});
test("scoreboards require their own Version 2 capability and do not inherit legacy or arena authority", () => {
  const session: ArenaSession = {
    phase: "game-running",
    session: "c".repeat(48),
    nativeProvider: {
      phase: "loader-active",
      build: {
        label: "Version 2",
        modProviderMapped: true,
        nativeWritesMapped: false,
        version2ScoreboardMapped: true,
      },
    },
  };
  expect(arenaEligible(session, "scoreboard")).toBe(true);
  expect(arenaEligible(session, "arena")).toBe(false);
  expect(arenaEligible(session, "ball")).toBe(false);
  for (const build of [
    { nativeWritesMapped: true },
    {
      label: "Version 2",
      modProviderMapped: true,
      version2ArenaBallMapped: true,
    },
    {
      label: "Unknown",
      modProviderMapped: true,
      version2ScoreboardMapped: true,
    },
    { label: "Version 2", version2ScoreboardMapped: true },
  ])
    expect(
      arenaEligible(
        { ...session, nativeProvider: { phase: "loader-active", build } },
        "scoreboard",
      ),
    ).toBe(false);
  for (const phase of ["held", "ended"]) {
    expect(
      arenaEligible(
        {
          ...session,
          nativeProvider: {
            ...session.nativeProvider,
            scoreboard: { phase } as ArenaStatus,
          },
        },
        "scoreboard",
      ),
    ).toBe(false);
  }
});
test("gameplay balls require their own Version 2 capability, not the former shared-prop mapping", () => {
  const session: ArenaSession = {
    phase: "game-running",
    session: "c".repeat(48),
    nativeProvider: {
      phase: "loader-active",
      build: {
        label: "Version 2",
        modProviderMapped: true,
        nativeWritesMapped: false,
        version2GameplayBallMapped: true,
      },
    },
  };
  expect(arenaEligible(session, "ball")).toBe(true);
  expect(arenaEligible(session, "arena")).toBe(false);
  expect(arenaEligible(session, "scoreboard")).toBe(false);
  for (const build of [
    {
      label: "Version 2",
      modProviderMapped: true,
      version2ArenaBallMapped: true,
    },
    {
      label: "Version 2",
      modProviderMapped: true,
      version2ScoreboardMapped: true,
    },
    {
      label: "Unknown",
      modProviderMapped: true,
      version2GameplayBallMapped: true,
    },
    { label: "Version 2", version2GameplayBallMapped: true },
  ])
    expect(
      arenaEligible(
        { ...session, nativeProvider: { phase: "loader-active", build } },
        "ball",
      ),
    ).toBe(false);
});
test("ineligible catalogs explain why in plain text instead of asking for a confirmation", () => {
  const base: ArenaSession = {
    session: "c".repeat(48),
    phase: "game-running",
    nativeProvider: { phase: "waiting-for-loader" },
  };
  expect(catalogUnavailableMessage(null, "ball")).toContain(
    "while the game launched from this launcher is running",
  );
  expect(catalogUnavailableMessage(base, "ball")).toContain(
    "native provider is active",
  );
  expect(
    catalogUnavailableMessage(
      { ...base, nativeProvider: { phase: "loader-active", build: {} } },
      "scoreboard",
    ),
  ).toContain("Scoreboard editing is disabled for this game build");
  const held: ArenaSession = {
    ...base,
    nativeProvider: {
      phase: "loader-active",
      build: { label: "Version 2", modProviderMapped: true, version2ScoreboardMapped: true },
      scoreboard: { phase: "held", message: "Scoreboard descriptor ownership changed" } as ArenaStatus,
    },
  };
  expect(catalogUnavailableMessage(held, "scoreboard")).toContain("restart before choosing another scoreboard");
  expect(catalogUnavailableMessage(held, "scoreboard")).toContain("descriptor ownership changed");
  expect(arenaEligible(held, "scoreboard")).toBe(false);
  for (const text of [
    catalogUnavailableMessage(base, "arena"),
    catalogUnavailableMessage(null, "scoreboard"),
  ])
    expect(text).not.toMatch(/enable|confirm|tick|check the box/i);
});
