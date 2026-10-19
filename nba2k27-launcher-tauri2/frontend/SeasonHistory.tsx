import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import { seasonMath } from "./season-ui-math";
import type { Player, PlayerDetail, Season } from "./types";

export type SeasonRow = {
  year: number;
  team: string;
  secondaryTeam?: string | null;
  totals: Record<string, number>;
  awards?: Record<string, number>;
};
export type SeasonSchema = {
  seasonLimits: Record<string, number>;
  awards: { key: string; label: string; maximum: number; choices: string[] }[];
};
type Team = PlayerDetail["teams"][number];
const totalLabels: Record<string, string> = {
  gp: "Games played",
  gs: "Games started",
  minutes: "Minutes",
  points: "Points",
  fgm: "FG made",
  fga: "FG attempted",
  threeMade: "3PT made",
  threeAttempted: "3PT attempted",
  ftm: "FT made",
  fta: "FT attempted",
  oreb: "Offensive rebounds",
  dreb: "Defensive rebounds",
  assists: "Assists",
  steals: "Steals",
  blocks: "Blocks",
  turnovers: "Turnovers",
  fouls: "Fouls",
  doubleDoubles: "Double-doubles",
  tripleDoubles: "Triple-doubles",
  plusMinus: "Plus/minus",
};
const average = (total: number, games: number) =>
  games ? (total / games).toFixed(1) : "—";
export const seasonName = (year: number) => Number.isInteger(year) && year >= 1900 && year <= 2155 ? `${year - 1}–${String(year).slice(-2)}` : "Invalid season year";
const derivedLabels: Record<string, string> = {
  mpg: "MPG",
  ppg: "PPG",
  orpg: "ORPG",
  rpg: "RPG",
  apg: "APG",
  spg: "SPG",
  bpg: "BPG",
  topg: "TOPG",
  fpg: "FPG",
  plusMinusAverage: "+/− per game",
  fgPercent: "FG%",
  threePercent: "3PT%",
  ftPercent: "FT%",
};
const compactColumns = [
  "gp",
  "gs",
  "mpg",
  "ppg",
  "orpg",
  "rpg",
  "apg",
  "spg",
  "bpg",
  "topg",
  "fgPercent",
  "threePercent",
  "ftPercent",
  "fpg",
  "plusMinusAverage",
  "points",
  "doubleDoubles",
  "tripleDoubles",
];
const compactLabels: Record<string, string> = {
  gp: "GP",
  gs: "GS",
  points: "PTS",
  doubleDoubles: "DD",
  tripleDoubles: "TD",
  plusMinusAverage: "+/-",
};
const normal = (teams: Team[]) =>
  teams
    .filter((t) => t.category === "current")
    .sort((a, b) => a.name.localeCompare(b.name));

export function SeasonHistory({
  rows,
  schema,
  busy,
  change,
  importRows,
}: {
  rows: SeasonRow[];
  schema: SeasonSchema;
  busy: boolean;
  change: (rows: SeasonRow[]) => void;
  importRows: () => void;
}) {
  const [selected, setSelected] = useState(0),
    [teams, setTeams] = useState<Team[]>([]),
    [error, setError] = useState("");
  useEffect(() => {
    let alive = true;
    call<Team[]>("season-teams")
      .then((t) => {
        if (alive) setTeams(normal(t));
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, []);
  const index = Math.min(selected, Math.max(0, rows.length - 1)),
    row = rows[index];
  function edit(next: SeasonRow) {
    change(rows.map((r, i) => (i === index ? next : r)));
  }
  function teamSelect(
    value: string,
    update: (team: string) => void,
    label: string,
    optional = false,
  ) {
    return (
      <select
        aria-label={label}
        value={value}
        disabled={busy}
        onChange={(e) => update(e.target.value)}
      >
        <option value="">{optional ? "None" : "Choose team…"}</option>
        {value && !teams.some((t) => t.name === value) && (
          <option value={value}>{value} · imported name</option>
        )}
        {teams.map((t) => (
          <option key={t.id} value={t.name}>
            {t.name}
          </option>
        ))}
      </select>
    );
  }
  return (
    <section className="season-history-editor">
      <div className="toolbar">
        <h3>Season history</h3>
        <button disabled={busy} onClick={importRows}>
          Import stats
        </button>
        <button
          disabled={busy || rows.length >= 25}
          onClick={() => {
            const year = rows.length
              ? Math.max(1900, Math.min(...rows.map((r) => r.year)) - 1)
              : new Date().getFullYear();
            change([
              ...rows,
              {
                year,
                team: "",
                totals: Object.fromEntries(
                  Object.keys(schema.seasonLimits).map((k) => [k, 0]),
                ),
              },
            ]);
            setSelected(rows.length);
          }}
        >
          Add older season
        </button>
        <button
          disabled={busy || !row}
          onClick={() => {
            change(rows.filter((_, i) => i !== index));
            setSelected(0);
          }}
        >
          Remove selected season
        </button>
        <span className="badge">{rows.length} / 25 seasons</span>
      </div>
      {error && (
        <p role="alert" className="notice">
          {error}
        </p>
      )}
      <p className="muted compact-note">
        Select a row to edit averages, percentages, totals and awards. Shooting
        edits keep attempts fixed and synchronize made shots and points; PPG
        uses the closest attainable scoring total. Invalid counts are rejected
        on Save. Changes remain local until saved.
      </p>
      <div className="table-scroll season-history-grid season-dense-grid">
        <table>
          <thead>
            <tr>
              {[
                "Season",
                "Team",
                ...compactColumns.map(
                  (k) => compactLabels[k] ?? derivedLabels[k],
                ),
                "Awards",
              ].map((h) => (
                <th key={h}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => {
              const display = seasonMath.display(r.totals);
              return (
                <tr key={i} className={i === index ? "selected" : ""}>
                  <td>
                    <button
                      aria-label={`Edit season ${r.year}`}
                      disabled={busy}
                      onClick={() => setSelected(i)}
                    >
                      {seasonName(r.year)}
                    </button>
                  </td>
                  <td>
                    {r.team || "Choose team"}
                    {r.secondaryTeam ? ` / ${r.secondaryTeam}` : ""}
                  </td>
                  {compactColumns.map((k) => (
                    <td key={k}>
                      {display[k] === null
                        ? "—"
                        : derivedLabels[k]
                          ? display[k]!.toFixed(1)
                          : display[k]}
                    </td>
                  ))}
                  <td>
                    {r.awards
                      ? schema.awards
                          .filter((a) => r.awards![a.key] > 0)
                          .map((a) => a.label)
                          .join(", ") || "None"
                      : "Not included"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {!row && (
        <p className="muted">
          No seasons in this Player DNA draft. Import stats or add a season.
        </p>
      )}
      {row && (
        <>
          <h4>Edit {seasonName(row.year)}</h4>
          <div className="dna-field-grid season-average-fields">
            {Object.entries(derivedLabels).map(([key, title]) => {
              const value = seasonMath.display(row.totals)[key];
              return (
                <label key={`${index}:${row.year}:${key}`}>
                  <span>{title}</span>
                  <SeasonNumber
                    label={`Season ${title}`}
                    value={value}
                    disabled={busy}
                    derived
                    clearError={() => setError("")}
                    commit={(number) => {
                      try {
                        edit({
                          ...row,
                          totals: seasonMath.change(row.totals, key, number),
                        });
                        setError("");
                      } catch (e) {
                        setError(String(e));
                        throw e;
                      }
                    }}
                  />
                </label>
              );
            })}
          </div>
          <p className="muted compact-note">
            Press Enter or leave a changed field to apply its value to this
            local draft. Unchanged displays never round stored totals. Review
            the updated totals, then Save local draft.
          </p>
          <div className="dna-field-grid">
            <label>
              Season end year
              <SeasonNumber label="Season end year" value={row.year} disabled={busy}
                clearError={() => setError("")} commit={(year) => {
                  if (!Number.isInteger(year) || year < 1900 || year > 2155) {
                    setError("Enter a season end year from 1900 to 2155.");
                    throw new Error("Invalid season end year");
                  }
                  edit({ ...row, year });
                }} />
            </label>
            <label>
              Team
              {teamSelect(
                row.team,
                (team) => edit({ ...row, team }),
                "Season team",
              )}
            </label>
            <label>
              Second team
              {teamSelect(
                row.secondaryTeam ?? "",
                (team) => edit({ ...row, secondaryTeam: team || null }),
                "Season second team",
                true,
              )}
            </label>
            {Object.entries(schema.seasonLimits).map(([k, max]) => (
              <label key={k}>
                {totalLabels[k] ?? k}
                <SeasonNumber label={`Season ${totalLabels[k] ?? k}`} value={row.totals[k]} disabled={busy}
                  clearError={() => setError("")} commit={(number) => {
                    if (!Number.isInteger(number) || number < (k === "plusMinus" ? -16384 : 0) || number > max) {
                      setError(`Enter a valid ${totalLabels[k] ?? k} total.`);
                      throw new Error("Invalid season total");
                    }
                    edit({ ...row, totals: { ...row.totals, [k]: number } });
                  }} />
              </label>
            ))}
          </div>
          <label className="season-awards-toggle">
            <input
              type="checkbox"
              aria-label="Include season awards"
              disabled={busy}
              checked={!!row.awards}
              onChange={(e) => {
                const next = { ...row };
                if (e.target.checked)
                  next.awards = Object.fromEntries(
                    schema.awards.map((a) => [a.key, 0]),
                  );
                else delete next.awards;
                edit(next);
              }}
            />{" "}
            Include season awards
          </label>
          {row.awards && (
            <div className="dna-field-grid season-awards-grid">
              {schema.awards.map((a) => (
                <label key={a.key}>
                  {a.label}
                  <select
                    aria-label={`Season award ${a.label}`}
                    disabled={busy}
                    value={row.awards![a.key]}
                    onChange={(e) =>
                      edit({
                        ...row,
                        awards: {
                          ...row.awards,
                          [a.key]: Number(e.target.value),
                        },
                      })
                    }
                  >
                    {a.choices.map((v, i) => (
                      <option key={i} value={i}>
                        {v}
                      </option>
                    ))}
                  </select>
                </label>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}

export function SeasonNumber({
  label,
  value,
  disabled,
  derived,
  commit,
  clearError,
}: {
  label: string;
  value: number | null;
  disabled: boolean;
  derived?: boolean;
  commit: (value: number) => void;
  clearError?: () => void;
}) {
  const formatted =
    value === null ? "" : derived ? value.toFixed(1) : String(value);
  const [text, setText] = useState(formatted),
    [invalid, setInvalid] = useState(false);
  // React state updates are asynchronous: Escape must suppress the blur
  // handler before it can read the previous, cancelled text.
  const cancelBlur = useRef(false);
  const numberRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    setText(formatted);
    setInvalid(false);
    numberRef.current?.setCustomValidity("");
  }, [formatted]);
  return (
    <input
      ref={numberRef}
      data-local-field
      aria-label={label}
      aria-invalid={invalid}
      type="number"
      step={derived ? "0.1" : "1"}
      value={text}
      disabled={disabled}
      placeholder={value === null ? "Set GP / attempts first" : undefined}
      onFocus={() => {
        cancelBlur.current = false;
      }}
      onChange={(e) => {
        setText(e.target.value);
        setInvalid(false);
        e.currentTarget.setCustomValidity("");
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        }
        if (e.key === "Escape") {
          e.preventDefault();
          cancelBlur.current = true;
          setText(formatted);
          setInvalid(false);
          e.currentTarget.setCustomValidity("");
          clearError?.();
          e.currentTarget.blur();
          // A disabled/hidden input may not dispatch blur. Do not let that
          // cancellation suppress the next, unrelated edit.
          queueMicrotask(() => {
            cancelBlur.current = false;
          });
        }
      }}
      onBlur={(event) => {
        if (cancelBlur.current) {
          cancelBlur.current = false;
          return;
        }
        if (text === formatted) {
          event.currentTarget.setCustomValidity("");
          setInvalid(false);
          clearError?.();
          return;
        }
        try {
          if (!text.trim() || !Number.isFinite(Number(text)))
            throw Error("Enter a valid number.");
          commit(Number(text));
          setText(formatted);
          setInvalid(false);
        } catch (failure) {
          setText(formatted);
          event.currentTarget.setCustomValidity(String(failure));
          setInvalid(true);
        }
      }}
    />
  );
}

export function SeasonCatalogImport({
  busy,
  onClose,
  stage,
}: {
  busy: boolean;
  onClose: () => void;
  stage: (
    id: number,
    selected: { id: number; teamId: number; secondaryTeamId?: number }[],
    awards: boolean,
  ) => Promise<void>;
}) {
  const [query, setQuery] = useState("Stephen Curry"),
    [players, setPlayers] = useState<Player[]>([]),
    [detail, setDetail] = useState<PlayerDetail | null>(null),
    [loading, setLoading] = useState(false),
    [error, setError] = useState(""),
    [chosen, setChosen] = useState<number[]>([]),
    [awards, setAwards] = useState(true),
    [mapping, setMapping] = useState<
      Record<number, { primary: string; secondary: string }>
    >({});
  const generation = useRef(0),
    pending = useRef(false);
  useEffect(() => {
    let alive = true;
    const g = ++generation.current;
    const timer = setTimeout(() => {
      setLoading(true);
      call<{ items: Player[] }>("players", { query })
        .then((r) => {
          if (alive && g === generation.current) setPlayers(r.items);
        })
        .catch((e) => {
          if (alive && g === generation.current) setError(String(e));
        })
        .finally(() => {
          if (alive && g === generation.current) setLoading(false);
        });
    }, 120);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [query]);
  const teams = detail ? normal(detail.teams) : [];
  async function choose(p: Player) {
    if (pending.current || busy) return;
    pending.current = true;
    ++generation.current;
    setLoading(true);
    setError("");
    try {
      const d = await call<PlayerDetail>("player", { id: p.id });
      setDetail(d);
      setChosen([]);
      const current = normal(d.teams),
        auto = (id: number | null | undefined) => {
          const source = d.teams.find((t) => t.id === id);
          if (!source) return "";
          const matches = current.filter(
            (t) =>
              t.name.trim().toLowerCase() === source.name.trim().toLowerCase(),
          );
          return matches.length === 1 ? String(matches[0].id) : "";
        };
      setMapping(
        Object.fromEntries(
          d.records.map((r) => [
            r.id,
            {
              primary: auto(r.primaryTeamId),
              secondary: auto(r.secondaryTeamId),
            },
          ]),
        ),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
      setLoading(false);
    }
  }
  const teamSelect = (r: Season, secondary = false) => (
    <select
      aria-label={`Import ${r.year} ${secondary ? "second" : "primary"} team`}
      disabled={busy || loading}
      value={mapping[r.id]?.[secondary ? "secondary" : "primary"] ?? ""}
      onChange={(e) =>
        setMapping({
          ...mapping,
          [r.id]: {
            ...mapping[r.id],
            [secondary ? "secondary" : "primary"]: e.target.value,
          },
        })
      }
    >
      <option value="">Choose team…</option>
      {teams.map((t) => (
        <option key={t.id} value={t.id}>
          {t.name}
        </option>
      ))}
    </select>
  );
  return (
    <div className="modal-backdrop">
      <section className="modal season-import-modal">
        <header>
          <h2>
            {detail
              ? `Import seasons · ${detail.player.firstName} ${detail.player.lastName}`
              : "Import stats · NBA 2K23"}
          </h2>
          <button
            disabled={busy || loading}
            onClick={detail ? () => setDetail(null) : onClose}
          >
            {detail ? "Back to players" : "Back to sources"}
          </button>
        </header>
        <div className="modal-body">
          {error && (
            <p role="alert" className="notice">
              {error}
            </p>
          )}
          {!detail ? (
            <>
              <input
                aria-label="Search stats players"
                className="wide-input"
                value={query}
                placeholder="Search player catalog"
                onChange={(e) => setQuery(e.target.value)}
              />
              {loading && <p className="muted">Searching…</p>}
              <div className="players-list">
                {players.map((p) => (
                  <button
                    key={p.id}
                    disabled={loading || busy}
                    onClick={() => void choose(p)}
                  >
                    <div>
                      <strong>{p.name}</strong>
                      <small>{p.seasons} seasons</small>
                    </div>
                    <span>Choose seasons →</span>
                  </button>
                ))}
              </div>
              {!loading && !players.length && (
                <p className="muted">No matching players.</p>
              )}
            </>
          ) : (
            <>
              <div className="toolbar">
                <span>{chosen.length} seasons selected</span>
                <label>
                  <input
                    type="checkbox"
                    aria-label="Import season awards"
                    disabled={busy || loading}
                    checked={awards}
                    onChange={(e) => setAwards(e.target.checked)}
                  />{" "}
                  Include season awards
                </label>
                <button
                  disabled={busy || loading}
                  onClick={() => setChosen([])}
                >
                  Clear selection
                </button>
              </div>
              <p className="muted">
                Choose normal teams for each checked row. Selected years replace
                matching draft years; other years and player fields stay
                unchanged. No game writes.
              </p>
              <div className="table-scroll season-history-grid">
                <table>
                  <thead>
                    <tr>
                      {["Use", "Team", "Season", "GP", "PPG", "RPG", "APG"].map(
                        (h) => (
                          <th key={h}>{h}</th>
                        ),
                      )}
                    </tr>
                  </thead>
                  <tbody>
                    {[...detail.records]
                      .sort((a, b) => b.year - a.year)
                      .map((r) => (
                        <tr key={r.id}>
                          <td>
                            <input
                              type="checkbox"
                              aria-label={`Choose stats season ${r.year}`}
                              disabled={
                                busy ||
                                loading ||
                                (!chosen.includes(r.id) && chosen.length >= 25)
                              }
                              checked={chosen.includes(r.id)}
                              onChange={(e) =>
                                setChosen(
                                  e.target.checked
                                    ? [...chosen, r.id]
                                    : chosen.filter((id) => id !== r.id),
                                )
                              }
                            />
                          </td>
                          <td>
                            {teamSelect(r)}
                            {r.secondaryTeamId != null && teamSelect(r, true)}
                          </td>
                          <td>{seasonName(r.year)}</td>
                          <td>{r.totals.gp}</td>
                          <td>{average(r.totals.points, r.totals.gp)}</td>
                          <td>
                            {average(
                              r.totals.oreb + r.totals.dreb,
                              r.totals.gp,
                            )}
                          </td>
                          <td>{average(r.totals.assists, r.totals.gp)}</td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </div>
        {detail && (
          <footer>
            <button
              className="primary"
              disabled={busy || loading || !chosen.length}
              onClick={() => {
                if (pending.current) return;
                const selected = [];
                for (const id of chosen) {
                  const m = mapping[id],
                    r = detail.records.find((r) => r.id === id)!;
                  if (
                    !m.primary ||
                    (r.secondaryTeamId != null && !m.secondary)
                  ) {
                    setError("Choose a normal team for every checked season.");
                    return;
                  }
                  selected.push({
                    id,
                    teamId: Number(m.primary),
                    ...(r.secondaryTeamId != null
                      ? { secondaryTeamId: Number(m.secondary) }
                      : {}),
                  });
                }
                pending.current = true;
                setLoading(true);
                setError("");
                void stage(detail.player.id, selected, awards)
                  .catch((e) => setError(String(e)))
                  .finally(() => {
                    pending.current = false;
                    setLoading(false);
                  });
              }}
            >
              Import selected seasons
            </button>
          </footer>
        )}
      </section>
    </div>
  );
}
