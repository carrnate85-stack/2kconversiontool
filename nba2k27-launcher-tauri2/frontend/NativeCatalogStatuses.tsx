import React from "react";
import type { ArenaStatus } from "./NativeArena";
import type { NativeOutfitStatus } from "./NativeOutfits";
export function NativeCatalogStatuses({
  phase,
  provider,
}: {
  phase: string;
  provider?: {
    arena?: ArenaStatus;
    ball?: ArenaStatus;
    scoreboard?: ArenaStatus;
    outfits?: NativeOutfitStatus;
  } | null;
}) {
  if (phase !== "game-running") return null;
  return (
    <>
      {(["arena", "ball", "scoreboard", "outfits"] as const).map((kind) => {
        const s = provider?.[kind];
        if (!s || (s.revision < 1 && s.phase !== "held") || s.phase === "ended")
          return null;
        return (
          <p className="muted" role="status" key={kind}>
            <strong>
              {kind === "ball"
                ? "Ball"
                : kind === "outfits"
                  ? "Outfits"
                  : kind === "scoreboard"
                    ? "Scoreboard"
                    : "Arena"}
              : {s.phase}
            </strong>{" "}
            · {s.message}
          </p>
        );
      })}
    </>
  );
}
