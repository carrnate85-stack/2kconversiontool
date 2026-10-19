import React from "react";

export type SelectorSummary = {
  enabled: boolean;
  players: number;
  source: string;
  sha256: Record<string, string>;
  generatedPlayers?: number;
  replacementPlayers?: number;
  skippedPlayers?: number;
  warnings?: { id: number; reason: string }[];
};

export function NativeSelectorStatus({
  phase,
  selectors,
  playerFolderFiles = 0,
}: {
  phase?: string;
  selectors?: SelectorSummary;
  playerFolderFiles?: number;
}) {
  if (!selectors || phase !== "loader-active") return null;
  return (
    <>
      {playerFolderFiles > 0 && (
        <p className="muted native-player-folder-status">
          Player-folder mods: {playerFolderFiles.toLocaleString("en-US")}{" "}
          shared-data links · no copied player data
        </p>
      )}
      <p className="muted native-selector-status">
        {selectors.enabled
          ? `Player configurations: ${selectors.players.toLocaleString("en-US")} records · ${selectors.generatedPlayers || selectors.replacementPlayers ? "native-generated bundle" : "validated mod-folder bundle"} active for this session`
          : "Player configurations: stock tables · no prepared bundle in this mod folder"}
      </p>
      {!!(selectors.generatedPlayers || selectors.replacementPlayers) && (
        <p className="muted">Added {selectors.generatedPlayers ?? 0} player IDs · updated {selectors.replacementPlayers ?? 0} configuration sets · temporary owned tables, originals preserved</p>
      )}
      {!!selectors.skippedPlayers && (
        <p className="muted">{selectors.skippedPlayers} configuration sets skipped by the selector planner (includes stock/single-style sets). Mod files are still routed; details are in this session’s native-selectors report in Logs.</p>
      )}
    </>
  );
}
