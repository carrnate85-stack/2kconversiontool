// WPF renderer/blacktop-setup-picker.js: the launcher Blacktop page's own
// catalog dialog (not the F8 overlay). Picking a card is the choice: it is
// saved, applied live when this launcher's game is eligible, and the dialog
// closes. Arena cards carry WPF's favorite star; scoreboard cards show the
// offline preview and caption; ball cards show the preview pack thumbnail.
import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { call } from "./api";
import { useNativeArena, type CatalogFeature } from "./NativeArena";
import { orderCatalogGroups } from "./overlay-catalog-cache";
import type { Settings } from "./types";
import "./BlacktopCatalog.css";
import { BallThumbnail } from "./BallThumbnail";

export type CatalogRow = {
  id: string;
  name: string;
  group: string;
  image?: string;
  imagePath?: string;
  caption?: string;
  note?: string;
  disabled?: boolean;
};
type Page = { items: CatalogRow[]; total: number; groups: string[] };
const PAGE = 24;

export function catalogFilters(kind: CatalogFeature, groups: string[]) {
  return [
    "All",
    ...(kind === "arena" ? ["Favorites"] : []),
    ...orderCatalogGroups(kind, groups),
  ];
}
export function catalogPaging(offset: number, total: number) {
  return total
    ? `${offset + 1}–${Math.min(offset + PAGE, total)} of ${total}`
    : "No results";
}
export function catalogEmptyText(group: string) {
  return group === "Favorites"
    ? "Star an arena to find it here."
    : "No matching choices.";
}
/** Settings patch for a pick: the id disambiguates repeated names. */
export function catalogChoicePatch(
  kind: CatalogFeature,
  row: Pick<CatalogRow, "id" | "name">,
): Partial<Settings> {
  return kind === "arena"
    ? { arena: row.name, arenaId: row.id }
    : kind === "scoreboard"
      ? { scoreboard: row.name, scoreboardId: row.id }
      : { ball: row.name };
}
export function catalogSelected(
  kind: CatalogFeature,
  settings: Settings,
  row: Pick<CatalogRow, "id" | "name">,
) {
  if (kind === "arena")
    return settings.arenaId
      ? settings.arenaId === row.id
      : settings.arena === row.name;
  if (kind === "scoreboard")
    return settings.scoreboardId
      ? settings.scoreboardId === row.id
      : settings.scoreboard === row.name;
  return settings.ball === row.name;
}
/** WPF idle/saved messages (arena-manager.js:9, scoreboard-manager.js:23,37,
 * ball-live-selector.js:10). A saved arena is the Rust 0.48 extension. */
export function catalogSavedMessage(kind: CatalogFeature, settings: Settings) {
  if (kind === "ball")
    return settings.ball === BALL_CURRENT
      ? "Current installed ball will be used."
      : "Saved for the next Play session. Choose a ball at Blacktop player selection and wait for Ready before starting.";
  if (kind === "scoreboard")
    return !settings.scoreboard || settings.scoreboard === "Default"
      ? "Uses the stock Blacktop scoreboard."
      : "Saved — waiting for Blacktop.";
  return !settings.arena || settings.arena === ARENA_DEFAULT
    ? "Uses the game’s Day / Night choice."
    : "Saved — applies at Blacktop player selection on each mod-profile launch.";
}
const BALL_CURRENT = "Currently installed",
  ARENA_DEFAULT = "Default · Day / Night";
/** WPF's label for the stock ball; the data keeps the saved-settings name. */
export const catalogLabel = (name: string) =>
  name === BALL_CURRENT ? "Current installed" : name;

export function BlacktopCatalog({
  kind,
  settings,
  change,
  onClose,
}: {
  kind: CatalogFeature;
  settings: Settings;
  change: (p: Partial<Settings>) => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const control = useNativeArena(true, settings.gameRoot, kind, true);
  const [query, setQuery] = useState(""),
    [group, setGroup] = useState("All"),
    [offset, setOffset] = useState(0),
    [page, setPage] = useState<Page>({ items: [], total: 0, groups: [] }),
    [favorites, setFavorites] = useState<string[] | null>(null),
    [favoriteBusy, setFavoriteBusy] = useState(false),
    [message, setMessage] = useState("");
  useEffect(() => {
    const el = dialog.current;
    if (el && !el.open) el.showModal?.();
  }, []);
  useEffect(() => {
    if (kind !== "arena") return;
    let alive = true;
    call<string[]>("arena-favorites")
      .then((ids) => alive && setFavorites(ids))
      .catch((e) => alive && setMessage(String(e)));
    return () => {
      alive = false;
    };
  }, [kind]);
  useEffect(() => {
    let alive = true;
    const timer = setTimeout(
      () =>
        call<Page>("choices", { kind, query, group, offset })
          .then((r) => alive && setPage(r))
          .catch((e) => alive && setMessage(String(e))),
      query ? 100 : 0,
    );
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [kind, query, group, offset, group === "Favorites" && favorites]);
  async function toggleFavorite(id: string) {
    if (favorites === null || favoriteBusy) return;
    setFavoriteBusy(true);
    try {
      setFavorites(
        await call<string[]>("arena-favorite", {
          id,
          favorite: !favorites.includes(id),
        }),
      );
    } catch (e) {
      setMessage(String(e));
    } finally {
      setFavoriteBusy(false);
    }
  }
  async function choose(row: CatalogRow) {
    if (control.busy || row.disabled) return;
    setMessage(`Saving ${catalogLabel(row.name)}…`);
    // WPF: the same select applies live while this launcher's game runs.
    if (control.eligible && !(await control.select(row.id))) {
      setMessage("Could not save selection.");
      return;
    }
    change(catalogChoicePatch(kind, row));
    onClose();
  }
  const title = `${kind[0].toUpperCase()}${kind.slice(1)} catalog`;
  const footer =
    control.error ||
    control.unavailable ||
    message ||
    (control.eligible ? control.status?.message : "") ||
    catalogSavedMessage(kind, settings);
  return (
    <dialog
      ref={dialog}
      className="blacktop-catalog"
      data-catalog-kind={kind}
      aria-label={title}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <header className="blacktop-catalog-header">
        <div>
          <small>BLACKTOP PRESENTATION</small>
          <h2>{title}</h2>
        </div>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </header>
      <section className="blacktop-catalog-controls">
        <input
          type="search"
          autoFocus
          placeholder={`Search ${kind} catalog`}
          aria-label={`Search ${kind} catalog`}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setOffset(0);
          }}
        />
        <div className="blacktop-catalog-filters">
          {catalogFilters(kind, page.groups).map((g) => (
            <button
              key={g}
              type="button"
              aria-pressed={g === group}
              onClick={() => {
                setGroup(g);
                setOffset(0);
              }}
            >
              {g === "Favorites" ? "★ Favorites" : g}
            </button>
          ))}
        </div>
      </section>
      <section className="blacktop-catalog-grid">
        {page.items.map((row) => {
          const starred = favorites?.includes(row.id) ?? false;
          const image = row.imagePath
            ? convertFileSrc(row.imagePath)
            : row.image;
          return (
            <div className="blacktop-catalog-entry" key={row.id}>
              <button
                type="button"
                className="blacktop-catalog-item"
                aria-pressed={catalogSelected(kind, settings, row)}
                disabled={control.busy || row.disabled === true}
                title={kind === "ball" ? row.note : undefined}
                onClick={() => void choose(row)}
              >
                {kind === "scoreboard" && (
                  <span className="blacktop-scoreboard-preview">
                    {image ? (
                      <img src={image} alt="" loading="lazy" decoding="async" />
                    ) : (
                      <span className="blacktop-scoreboard-preview-empty">
                        PREVIEW UNAVAILABLE
                      </span>
                    )}
                  </span>
                )}
                {kind === "ball" && <BallThumbnail id={row.id} gameRoot={settings.gameRoot} image={image} />}
                <strong>{catalogLabel(row.name)}</strong>
                <small>{row.group}</small>
                {kind === "scoreboard" && (
                  <small className="blacktop-scoreboard-caption">
                    {row.caption || "Preview unavailable"}
                  </small>
                )}
              </button>
              {kind === "arena" && (
                <button
                  type="button"
                  className="blacktop-catalog-star"
                  aria-pressed={starred}
                  aria-label={`Toggle ${row.name} favorite`}
                  disabled={favorites === null || favoriteBusy}
                  onClick={(e) => {
                    e.stopPropagation();
                    void toggleFavorite(row.id);
                  }}
                >
                  {starred ? "★" : "☆"}
                </button>
              )}
            </div>
          );
        })}
        {page.total === 0 && (
          <div className="blacktop-catalog-empty">{catalogEmptyText(group)}</div>
        )}
      </section>
      <footer className="blacktop-catalog-footer">
        <span role="status">{footer}</span>
        <div>
          <button
            type="button"
            disabled={offset === 0}
            onClick={() => setOffset(Math.max(0, offset - PAGE))}
          >
            Previous
          </button>
          <span>{catalogPaging(offset, page.total)}</span>
          <button
            type="button"
            disabled={offset + PAGE >= page.total}
            onClick={() => setOffset(offset + PAGE)}
          >
            Next
          </button>
        </div>
      </footer>
    </dialog>
  );
}
