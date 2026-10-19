import { useEffect, useState } from "react";

export function scalerPercent(value: number) {
  const percent = Math.round(value * 100);
  if (!Number.isFinite(percent)) return 100;
  return Math.min(100, Math.max(95, percent));
}

export function scalerFactor(percent: number) {
  return Math.min(100, Math.max(95, Math.round(percent))) / 100;
}

export function scalerSupported(value: number) {
  const percent = value * 100;
  return (
    Number.isFinite(percent) &&
    percent >= 95 &&
    percent <= 100 &&
    Math.abs(percent - Math.round(percent)) < 1e-9
  );
}

export type ScalerGroups = {
  coaches: boolean;
  cheerleaders: boolean;
  arenaPersonnel: boolean;
};

const GROUP_ROWS: { key: keyof ScalerGroups; label: string; note?: string }[] = [
  { key: "coaches", label: "Coaches" },
  { key: "cheerleaders", label: "Cheerleaders" },
  {
    key: "arenaPersonnel",
    label: "Arena personnel & crowd",
    note: "Announcers, security and individually loaded crowd actors",
  },
];

// WPF index.html input ids for each group switch.
const GROUP_IDS: Record<keyof ScalerGroups, string> = {
  coaches: "scalerCoaches",
  cheerleaders: "scalerCheerleaders",
  arenaPersonnel: "scalerArenaPersonnel",
};

export function ScalerSettings({
  value,
  onSave,
  groups,
  onGroupsChange,
}: {
  value: number;
  onSave: (value: number) => void;
  groups: ScalerGroups;
  onGroupsChange: (groups: ScalerGroups) => void;
}) {
  const saved = scalerPercent(value);
  const [draft, setDraft] = useState(saved);
  useEffect(() => setDraft(saved), [saved]);
  const storedIsSupported = scalerSupported(value);
  const changed = !storedIsSupported || draft !== saved;
  return (
    <div className="scaler-layout">
      <article className="card scaler-groups-card">
        <div className="scaler-card-heading">
          <div>
            <strong>Character groups</strong>
            <small>Choose who uses the saved scale</small>
          </div>
          <span className="settings-badge">Session setting</span>
        </div>
        <div className="scaler-groups" aria-label="Scaled character groups">
          <label className="scaler-enable scaler-group-primary">
            <span>
              <strong>Players &amp; referees</strong>
              <small>Main scale switch · always included in Version 2</small>
            </span>
            <input type="checkbox" id="scalerEnabled" checked disabled readOnly />
          </label>
          {GROUP_ROWS.map(({ key, label, note }) => (
            <label className="scaler-enable" key={key}>
              {note ? (
                <span>
                  <strong>{label}</strong>
                  <small>{note}</small>
                </span>
              ) : (
                <span>{label}</span>
              )}
              <input
                type="checkbox"
                id={GROUP_IDS[key]}
                aria-label={`Scale ${label.toLowerCase()}`}
                checked={groups[key]}
                onChange={(event) =>
                  onGroupsChange({ ...groups, [key]: event.target.checked })
                }
              />
            </label>
          ))}
        </div>
      </article>
      <article className="card scaler-size-card">
        <div className="scaler-card-heading">
          <div>
            <strong>Character size</strong>
            <small>Relative to the original model</small>
          </div>
          <output id="scalerValue" className="scaler-value" htmlFor="scalerPercent">
            {draft}%
          </output>
        </div>
        <div className="scaler-control">
          <input
            id="scalerPercent"
            aria-label="Character size relative to original"
            aria-describedby="scalerHelp"
            type="range"
            min="95"
            max="100"
            step="1"
            value={draft}
            onChange={(event) => setDraft(Number(event.target.value))}
          />
          <div className="scaler-limits">
            <small>95% · Smaller</small>
            <small>100% · Original</small>
          </div>
        </div>
        <div className="scaler-scale-marks" aria-hidden="true">
          {[95, 96, 97, 98, 99, 100].map((mark) => (
            <span key={mark}>{mark}</span>
          ))}
        </div>
        {!storedIsSupported && (
          <p className="settings-description" role="alert">
            The older saved scale is outside the Version 2 range. Save this
            supported value before Play.
          </p>
        )}
      </article>
      <section className="card scaler-command-bar">
        <div className="scaler-status-wrap">
          <span className="status-dot ready" aria-hidden="true" />
          <p id="scalerStatus" role="status" aria-live="polite">
            {changed
              ? `${draft}% is staged for the next Play launch.`
              : `${saved}% is saved for the next Play launch.`}
          </p>
        </div>
        <div className="scaler-actions">
          <button
            id="scalerSave"
            className="primary"
            type="button"
            disabled={!changed}
            onClick={() => onSave(scalerFactor(draft))}
          >
            Save size
          </button>
          <button
            id="scalerReset"
            className="secondary"
            type="button"
            disabled={storedIsSupported && draft === 100 && saved === 100}
            onClick={() => {
              setDraft(100);
              if (!storedIsSupported || saved !== 100) onSave(1);
            }}
          >
            Reset to 100%
          </button>
        </div>
      </section>
      <small id="scalerHelp" className="scaler-help">
        Players and referees remain paired because they share one native scale
        input. Roster height, wingspan and weight stay unchanged. Selected
        coaches, cheerleaders and arena personnel join that same game input
        while they are loaded. Group choices apply at the next Play launch.
      </small>
    </div>
  );
}
