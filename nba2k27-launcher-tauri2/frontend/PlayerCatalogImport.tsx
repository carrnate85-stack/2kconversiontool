import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import type { DNA } from "./PlayerDNA";
import type { PlayerField } from "./PlayerFieldNumber";
import {
  catalogCategoryName,
  catalogSectionCounts,
  catalogSections,
} from "./catalog-import";
type Player = {
  id: number;
  name: string;
  seasons: number;
  categories: string[];
  context: string[];
};
type Results = {
  total: number;
  offset: number;
  limit: number;
  categories: string[];
  items: Player[];
};
type Review = { document: DNA; sourceId: number; unsupported: string[] };
export function PlayerCatalogImport({
  fields,
  destination,
  busy,
  onClose,
  onStage,
}: {
  fields: PlayerField[];
  destination: string;
  busy: boolean;
  onClose: () => void;
  onStage: (document: DNA, groups: string[]) => Promise<void>;
}) {
  const [query, setQuery] = useState(""),
    [category, setCategory] = useState(""),
    [offset, setOffset] = useState(0),
    [result, setResult] = useState<Results | null>(null),
    [review, setReview] = useState<Review | null>(null),
    [groups, setGroups] = useState<string[]>([]),
    [loading, setLoading] = useState(true),
    [working, setWorking] = useState(false),
    [error, setError] = useState("");
  const dialog = useRef<HTMLDialogElement>(null),
    alive = useRef(true),
    revision = useRef(0),
    pending = useRef(false);
  useEffect(() => {
    alive.current = true;
    const el = dialog.current!;
    el.showModal();
    return () => {
      alive.current = false;
      ++revision.current;
      el.close();
    };
  }, []);
  const reviewing = review !== null;
  useEffect(() => {
    if (reviewing) return;
    const tag = ++revision.current;
    setLoading(true);
    setError("");
    const timer = setTimeout(() => {
      void call<Results>("players", { query, category, offset, limit: 40 })
        .then((data) => {
          if (alive.current && tag === revision.current) setResult(data);
        })
        .catch((failure) => {
          if (alive.current && tag === revision.current) {
            setResult(null);
            setError(String(failure));
          }
        })
        .finally(() => {
          if (alive.current && tag === revision.current) setLoading(false);
        });
    }, 150);
    return () => {
      clearTimeout(timer);
      if (tag === revision.current) ++revision.current;
    };
  }, [query, category, offset, reviewing]);
  async function choose(player: Player) {
    if (busy || pending.current || loading) return;
    pending.current = true;
    setWorking(true);
    setError("");
    ++revision.current;
    try {
      const data = await call<Review>("catalog-player-review", {
        id: player.id,
      });
      if (!alive.current) return;
      if (data.sourceId !== player.id)
        throw Error("Source player changed. Please select again.");
      setGroups([]);
      setReview(data);
      dialog.current?.querySelector<HTMLElement>(".modal-body")?.scrollTo(0, 0);
    } catch (failure) {
      if (alive.current) setError(String(failure));
    } finally {
      pending.current = false;
      if (alive.current) setWorking(false);
    }
  }
  async function stage() {
    if (!review || busy || pending.current || !groups.length) return;
    pending.current = true;
    setWorking(true);
    setError("");
    try {
      await onStage(review.document, groups);
    } catch (failure) {
      if (alive.current) setError(String(failure));
    } finally {
      pending.current = false;
      if (alive.current) setWorking(false);
    }
  }
  function back() {
    if (busy || pending.current) return;
    if (review) {
      setReview(null);
      setGroups([]);
      setError("");
    } else onClose();
  }
  const counts = review ? catalogSectionCounts(review.document, fields) : {};
  const selectedCount = groups.reduce((n, g) => n + (counts[g] ?? 0), 0);
  const locked = busy || working;
  return (
    <dialog
      ref={dialog}
      className="modal player-catalog-dialog"
      aria-labelledby="player-catalog-title"
      onCancel={(event) => {
        event.preventDefault();
        back();
      }}
    >
      <header>
        <div>
          <h2 id="player-catalog-title">
            {review ? "Choose import categories" : "NBA 2K23 roster"}
          </h2>
          <p>Import into: {destination || "New local player"}</p>
        </div>
        <button disabled={locked} onClick={back}>
          {review ? "Back to players" : "Back to player"}
        </button>
      </header>
      <div className="modal-body">
        {error && (
          <p className="notice" role="alert">
            {error}
          </p>
        )}
        {!review ? (
          <>
            <div className="player-catalog-search">
              <input
                type="search"
                autoFocus
                maxLength={100}
                aria-label="Search import players"
                placeholder="Search players or teams"
                disabled={locked}
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setOffset(0);
                  setLoading(true);
                }}
              />
              <select
                aria-label="Import player category"
                disabled={locked}
                value={category}
                onChange={(e) => {
                  setCategory(e.target.value);
                  setOffset(0);
                  setLoading(true);
                }}
              >
                <option value="">All source players</option>
                {result?.categories.map((c) => (
                  <option key={c} value={c}>
                    {catalogCategoryName(c)}
                  </option>
                ))}
              </select>
            </div>
            <p className="muted compact-note">
              The built-in NBA 2K23 catalog is included—no download needed.
              Choose a source player, then review categories.
            </p>
            <p role="status" className="player-catalog-status">
              {loading
                ? "Searching…"
                : `${result?.total ?? 0} matching players`}
            </p>
            <div
              className="player-catalog-results"
              aria-label="NBA 2K23 source players"
              aria-busy={loading}
            >
              {result?.items.map((p) => (
                <button
                  key={p.id}
                  disabled={loading || locked}
                  onClick={() => void choose(p)}
                >
                  <strong>{p.name}</strong>
                  <small>
                    #{p.id} · {p.categories.map(catalogCategoryName).join(", ")}{" "}
                    · {p.context.join(" / ") || "Unassigned"} · {p.seasons}{" "}
                    seasons
                  </small>
                  <span aria-hidden="true">›</span>
                </button>
              ))}
            </div>
            {!loading && !result?.items.length && !error && (
              <p className="muted">No matching players.</p>
            )}
          </>
        ) : (
          <>
            <div className="player-catalog-source">
              <small>NBA 2K23 SOURCE PLAYER · #{review.sourceId}</small>
              <strong>{review.document.name}</strong>
            </div>
            <div className="toolbar">
              <h3>Choose categories</h3>
              <button
                disabled={locked}
                onClick={() =>
                  setGroups(
                    catalogSections
                      .filter((s) => counts[s.key] > 0)
                      .map((s) => s.key),
                  )
                }
              >
                Select all supported
              </button>
              <button disabled={locked} onClick={() => setGroups([])}>
                Clear categories
              </button>
            </div>
            <div className="player-catalog-sections">
              {catalogSections.map((s) => (
                <label
                  key={s.key}
                  className={groups.includes(s.key) ? "selected" : ""}
                >
                  <input
                    type="checkbox"
                    aria-label={`Import category ${s.name}`}
                    disabled={locked || !counts[s.key]}
                    checked={groups.includes(s.key)}
                    onChange={(e) =>
                      setGroups(
                        e.target.checked
                          ? [...groups, s.key]
                          : groups.filter((g) => g !== s.key),
                      )
                    }
                  />
                  <span>
                    <strong>{s.name}</strong>
                    <span>{s.description}</span>
                    <small>{counts[s.key] ?? 0} mapped values</small>
                  </span>
                </label>
              ))}
            </div>
            <details className="player-catalog-kept">
              <summary>Fields kept unchanged</summary>
              <ul>
                {review.unsupported.map((s, i) => (
                  <li key={i}>{s}</li>
                ))}
              </ul>
            </details>
            <p className="muted">
              Season stats and awards stay unchanged here. Use Season history →
              Import stats to select individual years and destination teams.
            </p>
            <p role="status">
              {groups.length} categories · {selectedCount} mapped values
              selected. Stage, then review the player tabs and Save local draft.
              Nothing is written to the game.
            </p>
          </>
        )}
      </div>
      <footer>
        {!review ? (
          <>
            <button
              disabled={locked || loading || offset === 0}
              onClick={() => {
                setOffset(Math.max(0, offset - 40));
                setLoading(true);
              }}
            >
              Previous
            </button>
            <p>
              {result?.total
                ? `${offset + 1}–${Math.min(offset + 40, result.total)} of ${result.total}`
                : "0 results"}
            </p>
            <button
              disabled={
                locked || loading || !result || offset + 40 >= result.total
              }
              onClick={() => {
                setOffset(offset + 40);
                setLoading(true);
              }}
            >
              Next
            </button>
          </>
        ) : (
          <>
            <p>Draft name, unselected values and season history are retained.</p>
            <button
              className="primary"
              disabled={locked || selectedCount === 0}
              onClick={() => void stage()}
            >
              {working ? "Staging…" : "Stage selected categories"}
            </button>
          </>
        )}
      </footer>
    </dialog>
  );
}
