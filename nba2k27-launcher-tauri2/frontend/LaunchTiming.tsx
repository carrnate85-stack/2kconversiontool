export type LaunchTimingValue = {
  elapsedMs?: number;
  phaseElapsedMs?: number;
  transitions?: Array<{
    phase?: string;
    elapsedMs?: number;
    completed?: boolean;
  }>;
};

export function launchTimingRows(timing?: LaunchTimingValue | null) {
  if (!Array.isArray(timing?.transitions)) return [];
  return timing.transitions
    .slice(-16)
    .filter(
      (row) =>
        typeof row.phase === "string" &&
        row.phase.length > 0 &&
        Number.isFinite(row.elapsedMs) &&
        (row.elapsedMs ?? -1) >= 0,
    )
    .map((row) => ({
      phase: row.phase!.replaceAll("-", " "),
      elapsedMs: Math.round(row.elapsedMs!),
      completed: row.completed === true,
    }));
}

export function NativeLaunchTiming({
  timing,
}: {
  timing?: LaunchTimingValue | null;
}) {
  const rows = launchTimingRows(timing);
  if (!rows.length) return null;
  return (
    <details className="native-launch-timing">
      <summary>
        Launch preparation timing · {Math.round(timing?.elapsedMs ?? 0)} ms
      </summary>
      <div className="timing-grid">
        {rows.map((row, index) => (
          <div key={`${row.phase}-${index}`}>
            <span>{row.phase}</span>
            <strong>
              {row.elapsedMs} ms{row.completed ? "" : " · current"}
            </strong>
          </div>
        ))}
      </div>
      <p className="muted compact-note">
        These are launcher stages, not proof that a camera or scale is visible
        in gameplay.
      </p>
    </details>
  );
}
