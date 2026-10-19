import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import { SeasonCatalogImport } from "./SeasonHistory";
export type ReferenceSource = {
  name: string;
  url: string;
  token: string;
  warnings: string[];
  awardsSupported: false;
  seasons: {
    year: number;
    label: string;
    team: string;
    abbreviation: string;
    aggregate: boolean;
    totals: Record<string, number>;
    missingFields: string[];
  }[];
};
type Team = { id: number; name: string; category: string };
export function referenceMapping(source: ReferenceSource, teams: Team[]) {
  return Object.fromEntries(
    source.seasons.map((r) => {
      const matches = teams.filter(
        (t) =>
          t.category === "current" &&
          t.name.trim().toLowerCase() === r.team.trim().toLowerCase(),
      );
      return [
        r.year,
        !r.aggregate && matches.length === 1 ? String(matches[0].id) : "",
      ];
    }),
  );
}
export function SeasonImport({
  busy,
  onClose,
  stage,
  stageReference,
}: {
  busy: boolean;
  onClose: () => void;
  stage: (
    id: number,
    selected: { id: number; teamId: number; secondaryTeamId?: number }[],
    awards: boolean,
  ) => Promise<void>;
  stageReference: (
    token: string,
    selected: { year: number; teamId: number }[],
  ) => Promise<void>;
}) {
  const [source, setSource] = useState<"catalog" | "reference" | null>(null);
  if (source === "catalog")
    return (
      <SeasonCatalogImport
        busy={busy}
        onClose={() => setSource(null)}
        stage={stage}
      />
    );
  if (source === "reference")
    return (
      <ReferenceImport
        busy={busy}
        onClose={() => setSource(null)}
        stage={stageReference}
      />
    );
  return (
    <div className="modal-backdrop">
      <section
        className="modal stats-source-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Import season stats"
      >
        <header>
          <h2>Import season stats</h2>
          <button disabled={busy} onClick={onClose}>
            Back to player
          </button>
        </header>
        <div className="modal-body">
          <p className="muted">
            Choose a source, review seasons and destination teams, then stage in
            the local editor. Nothing is written to the game.
          </p>
          <div className="stats-sources">
            <button onClick={() => setSource("catalog")}>
              Import from NBA 2K23<span>Search the bundled player catalog</span>
            </button>
            <button onClick={() => setSource("reference")}>
              Paste Basketball-Reference URL
              <span>Download one public NBA player page</span>
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}
function ReferenceImport({
  busy,
  onClose,
  stage,
}: {
  busy: boolean;
  onClose: () => void;
  stage: (
    token: string,
    selected: { year: number; teamId: number }[],
  ) => Promise<void>;
}) {
  const [url, setUrl] = useState(""),
    [source, setSource] = useState<ReferenceSource | null>(null),
    [teams, setTeams] = useState<Team[]>([]),
    [mapping, setMapping] = useState<Record<number, string>>({}),
    [selected, setSelected] = useState<number[]>([]),
    [loading, setLoading] = useState(false),
    [applying, setApplying] = useState(false),
    [error, setError] = useState("");
  const generation = useRef(0),
    pending = useRef(false),
    alive = useRef(true);
  useEffect(
    () => () => {
      alive.current = false;
      ++generation.current;
    },
    [],
  );
  const locked = busy || loading || applying;
  async function load() {
    if (pending.current || busy) return;
    pending.current = true;
    setLoading(true);
    setError("");
    setSource(null);
    setSelected([]);
    const tag = ++generation.current;
    try {
      const [data, available] = await Promise.all([
        call<ReferenceSource>(
          window.__launcherSmoke
            ? "smoke-reference-load"
            : "reference-stats-load",
          { url, acknowledgement: "fetch-public-player-page" },
        ),
        call<Team[]>("season-teams"),
      ]);
      if (!alive.current || tag !== generation.current) return;
      const normal = available
        .filter((t) => t.category === "current")
        .sort((a, b) => a.name.localeCompare(b.name));
      if (!normal.length)
        throw Error("No normal destination teams are available.");
      setTeams(normal);
      setMapping(referenceMapping(data, normal));
      setSource(data);
    } catch (e) {
      if (alive.current && tag === generation.current) setError(String(e));
    } finally {
      if (alive.current && tag === generation.current) {
        pending.current = false;
        setLoading(false);
      }
    }
  }
  async function importSelected() {
    if (!source || pending.current || busy || !selected.length) return;
    pending.current = true;
    setApplying(true);
    setError("");
    const tag = ++generation.current;
    try {
      const choices = selected.map((year) => {
        if (!mapping[year])
          throw Error(
            "Choose a normal destination team for every checked season.",
          );
        return { year, teamId: Number(mapping[year]) };
      });
      await stage(source.token, choices);
    } catch (e) {
      if (alive.current && tag === generation.current) setError(String(e));
    } finally {
      if (alive.current && tag === generation.current) {
        pending.current = false;
        setApplying(false);
      }
    }
  }
  const avg = (n: number, gp: number) => (gp ? (n / gp).toFixed(1) : "—");
  return (
    <div className="modal-backdrop">
      <section
        className="modal reference-import-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Basketball-Reference season stats"
      >
        <header>
          <h2>Basketball-Reference season stats</h2>
          <button disabled={busy || applying} onClick={onClose}>
            Back to sources
          </button>
        </header>
        <div className="modal-body">
          <p className="muted">
            Paste a public NBA player URL. Only regular-season totals are
            imported. Player info, ratings and awards stay unchanged. Review and
            save locally afterward.
          </p>
          <form
            className="toolbar reference-url"
            onSubmit={(e) => {
              e.preventDefault();
              void load();
            }}
          >
            <input
              type="url"
              required
              aria-label="Basketball-Reference NBA player URL"
              placeholder="https://www.basketball-reference.com/players/m/maxeyty01.html"
              value={url}
              disabled={locked}
              onChange={(e) => setUrl(e.target.value)}
            />
            <button type="submit" disabled={locked || !url.trim()}>
              Load seasons
            </button>
          </form>
          <p role="status" className="muted compact-note">
            {loading
              ? "Loading regular-season totals…"
              : source
                ? `${source.name} · ${source.seasons.length} seasons loaded · ${selected.length} selected`
                : "A page is downloaded only when you click Load seasons."}
          </p>
          {error && (
            <p className="notice" role="alert">
              {error}
            </p>
          )}
          {source && (
            <>
              <div className="toolbar">
                <h3>{source.name}</h3>
                <button
                  disabled={locked}
                  onClick={() =>
                    setSelected(source.seasons.slice(0, 25).map((s) => s.year))
                  }
                >
                  Select up to 25 seasons
                </button>
                <button disabled={locked} onClick={() => setSelected([])}>
                  Clear selection
                </button>
              </div>
              <div className="table-scroll season-history-grid">
                <table>
                  <thead>
                    <tr>
                      {[
                        "Use",
                        "Team",
                        "Season",
                        "GP",
                        "PPG",
                        "RPG",
                        "APG",
                        "PTS",
                      ].map((h) => (
                        <th key={h}>{h}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {source.seasons.map((r) => (
                      <tr
                        key={r.year}
                        className={selected.includes(r.year) ? "selected" : ""}
                      >
                        <td>
                          <input
                            type="checkbox"
                            aria-label={`Choose reference season ${r.year}`}
                            disabled={
                              locked ||
                              (!selected.includes(r.year) &&
                                selected.length >= 25)
                            }
                            checked={selected.includes(r.year)}
                            onChange={(e) =>
                              setSelected(
                                e.target.checked
                                  ? [...selected, r.year]
                                  : selected.filter((y) => y !== r.year),
                              )
                            }
                          />
                        </td>
                        <td>
                          <select
                            aria-label={`Reference ${r.year} destination team`}
                            value={mapping[r.year] ?? ""}
                            disabled={locked}
                            onChange={(e) =>
                              setMapping({
                                ...mapping,
                                [r.year]: e.target.value,
                              })
                            }
                          >
                            <option value="">
                              {r.aggregate
                                ? "Choose team (traded season)…"
                                : "Choose team…"}
                            </option>
                            {teams.map((t) => (
                              <option key={t.id} value={t.id}>
                                {t.name}
                              </option>
                            ))}
                          </select>
                        </td>
                        <td>{r.label}</td>
                        <td>{r.totals.gp}</td>
                        <td>{avg(r.totals.points, r.totals.gp)}</td>
                        <td>
                          {avg(r.totals.oreb + r.totals.dreb, r.totals.gp)}
                        </td>
                        <td>{avg(r.totals.assists, r.totals.gp)}</td>
                        <td>{r.totals.points}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p className="muted compact-note">
                Traded years use one aggregate row, not duplicated team totals.
                Unavailable totals (including DD, TD and +/−) keep existing
                values; new seasons use zero. Existing awards are preserved.{" "}
                {source.warnings.join(" ")}
              </p>
            </>
          )}
        </div>
        {source && (
          <footer>
            <button
              className="primary"
              disabled={
                locked || !selected.length || selected.some((y) => !mapping[y])
              }
              onClick={() => void importSelected()}
            >
              Import selected stats
            </button>
          </footer>
        )}
      </section>
    </div>
  );
}
