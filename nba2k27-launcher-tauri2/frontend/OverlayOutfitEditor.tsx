import { useEffect, useRef, useState } from "react";
import { call, garmentImage } from "./api";
import { canApplyOutfit, useNativeOutfits } from "./NativeOutfits";
import { label } from "./state";
import type { Garment, Outfit } from "./types";
import "./OverlayOutfitEditor.css";

// WPF F8 "Edit outfit" (renderer/blacktop.js + accessory-controls.js): a
// players column, piece cards and accessory slot tiles, each opening the
// clothing picker. Game access goes only through the outfit hook (`control`);
// picking pieces edits the local draft (`outfit`).
type SlotDef = [slot: string, name: string, icon: string];
export const outfitPieceSlots: SlotDef[] = [
  ["top", "Top", "M8 3l4 2 4-2 5 5-4 3-1-2v12H8V9l-1 2-4-3z"],
  ["bottom", "Bottom", "M6 3h12l2 17h-7l-1-9-1 9H4z"],
  ["socks", "Socks", "M8 3h8v12l-6 6H4v-5l4-3z"],
  ["shoes", "Shoes", "M5 7l5 5 5 1 5 4v3H3v-8z"],
];
export const outfitAccessorySlots: SlotDef[] = [
  ["headband", "Headband", "M4 10c4-3 12-3 16 0v5c-4-3-12-3-16 0z"],
  ["eyewear", "Eyewear", "M3 10h7v5H3z M14 10h7v5h-7z M10 11h4 M1 9l2 2 M23 9l-2 2"],
  ["faceMask", "Face mask", "M5 8l7-3 7 3v8l-7 5-7-5z M7 10h3 M14 10h3 M10 15h4 M5 8L2 6 M19 8l3-2"],
  ["backpack", "Backpack", "M8 7V5a4 4 0 0 1 8 0v2 M6 7h12v14H6z M9 13h6v5H9z"],
  ["leftArm", "Left arm", "M10 3l5 2-2 8 4 6-4 3-6-8z"],
  ["rightArm", "Right arm", "M10 3l5 2-2 8 4 6-4 3-6-8z"],
  ["leftLeg", "Left leg", "M8 2h8l-2 11 3 8H9l-3-8z"],
  ["rightLeg", "Right leg", "M8 2h8l-2 11 3 8H9l-3-8z"],
  ["headwear", "Hat", "M3 14h18 M6 14V8c0-5 12-5 12 0v6 M9 6v8"],
  ["headphones", "Headphones", "M4 14V9a8 8 0 0 1 16 0v5 M3 12h4v8H3z M17 12h4v8h-4z"],
  ["leftPiercing", "Left piercing", "M8 4c8-5 12 4 6 9 M9 15a3 3 0 1 0 6 0"],
  ["rightPiercing", "Right piercing", "M8 4c8-5 12 4 6 9 M9 15a3 3 0 1 0 6 0"],
  ["neckwear", "Neckwear", "M4 5c4 7 12 7 16 0 M11 12h2v6h-2z"],
  ["bodywear", "Shoulder bag", "M7 3l10 12 M6 12h13v9H6z"],
  ["waist", "Waist", "M3 10h18v5H3z M9 9h6v7H9z"],
  ["leftWrist", "Left wrist", "M7 2h10v7H7z M7 15h10v7H7z M5 9h14v6H5z"],
  ["rightWrist", "Right wrist", "M7 2h10v7H7z M7 15h10v7H7z M5 9h14v6H5z"],
  ["leftHand", "Left hand", "M7 11V4 M10 11V2 M13 11V3 M16 11V5 M7 9l-3 2 3 10h10l3-10-4-1"],
  ["rightHand", "Right hand", "M7 11V4 M10 11V2 M13 11V3 M16 11V5 M7 9l-3 2 3 10h10l3-10-4-1"],
  ["other", "Other", "M5 5h14v14H5z M9 12h6"],
];
const PAGE = 18;
function ItemIcon({ d }: { d: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className="outfit-item-icon">
      <path d={d} fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
    </svg>
  );
}
type Piece = Garment & { category: string };
type Picker = { slot: string; name: string };
export function OverlayOutfitEditor({
  live,
  control,
  outfit,
  setOutfit,
  message,
  setMessage,
  restore,
}: {
  live: boolean;
  control: ReturnType<typeof useNativeOutfits>;
  outfit: Outfit;
  setOutfit: (o: Outfit) => void;
  message: string;
  setMessage: (m: string) => void;
  restore: () => void;
}) {
  const [pieces, setPieces] = useState<Piece[]>([]);
  const [saved, setSaved] = useState<Outfit | null>(null);
  const [picker, setPicker] = useState<Picker | null>(null);
  const pool = outfit.pool ?? "male";
  useEffect(() => {
    let alive = true;
    call<Piece[]>("outfit-pieces", { outfit })
      .then((p) => alive && setPieces(Array.isArray(p) ? p : []))
      .catch(() => alive && setPieces([]));
    return () => {
      alive = false;
    };
  }, [outfit]);
  useEffect(() => {
    call<Outfit | null>("outfit")
      .then(setSaved)
      .catch(() => setSaved(null));
  }, []);
  const selection = control.status?.selection;
  const rows = [
    ...(selection?.players ?? []).map((p) => ({ slot: p.slot, player: p, reason: "" })),
    ...(selection?.unavailable ?? []).map((slot) => ({
      slot,
      player: null,
      reason:
        selection?.unavailableReasons?.find((r) => r.slot === slot)?.reason ||
        "Player could not be read",
    })),
  ].sort((a, b) => a.slot - b.slot);
  const name = control.player?.name ?? (live ? "Select a player" : "Stephen Curry");
  const pieceFor = (slot: string) =>
    slot === "top" || slot === "bottom" || slot === "shoes" || slot === "socks"
      ? pieces.find((p) => p.asset === outfit[slot as "top"] && p.category === slot)
      : pieces.find((p) => p.category === slot && outfit.accessories.includes(p.asset));
  const assetFor = (slot: string) =>
    slot === "top" || slot === "bottom" || slot === "shoes" || slot === "socks"
      ? outfit[slot as "top"] || ""
      : (pieceFor(slot)?.asset ?? "");
  const canApply = canApplyOutfit(
    live,
    control.eligible,
    control.busy,
    control.status,
    control.player,
    outfit,
  );
  const savedFits = saved && (saved.pool ?? "male") === pool ? saved : null;
  const removeAccessory = (asset: string) =>
    setOutfit({ ...outfit, accessories: outfit.accessories.filter((a) => a !== asset) });
  const statusMessage = control.error || control.unavailable ||
    (control.status?.message === "Native Blacktop players refreshed."
      ? ""
      : control.status?.message || (live ? "" : message));
  return (
    <div className="wpf-outfit blacktop-overlay">
      {statusMessage && <p className="wpf-status" role="status">{statusMessage}</p>}
      <main className="wpf-main">
        <div className="blacktop-workspace">
          <nav className="wpf-player-picker" aria-label="Selected Blacktop players">
            {rows.map(({ slot, player: p, reason }) =>
              p ? (
                <button
                  key={p.binding}
                  type="button"
                  className="blacktop-player-button"
                  aria-pressed={p.binding === control.binding}
                  disabled={control.busy || !p.pool}
                  title={p.pool ? `Player ${p.slot + 1} · ${p.name}` : "Clothing pool could not be verified"}
                  onClick={() => control.select(p.binding)}
                >
                  <small>Player {p.slot + 1}</small>
                  <span>{p.name || `Player ${p.slot + 1}`}</span>
                </button>
              ) : (
                <button
                  key={`slot-${slot}`}
                  type="button"
                  className="blacktop-player-button"
                  aria-pressed={false}
                  disabled
                  title={reason}
                >
                  <small>Player {slot + 1}</small>
                  <span>{reason}</span>
                </button>
              ),
            )}
          </nav>
          <div className="wardrobe-editor">
            <div className="wpf-players">
              <article className="card blacktop-outfit-row" aria-label={`Outfit for ${name}`}>
                <div className="blacktop-player-heading">
                  <div className="blacktop-player-identity">
                    <strong>{name}</strong>
                  </div>
                  <div className="blacktop-saved-outfit">
                    <select
                      aria-label={`Saved outfit for ${name}`}
                      disabled={!savedFits}
                      value={savedFits ? "saved" : ""}
                      onChange={() => {}}
                    >
                      {savedFits ? (
                        <option value="saved">{savedFits.name || "Saved outfit"}</option>
                      ) : (
                        <option value="">No saved outfits for this clothing fit</option>
                      )}
                    </select>
                    <button
                      className="secondary small"
                      type="button"
                      disabled={!savedFits}
                      onClick={() => savedFits && setOutfit({ ...savedFits, pool })}
                    >
                      Apply preset
                    </button>
                  </div>
                </div>
                <div className="native-outfit-controls wpf-outfit-actions">
                  <button
                    type="button"
                    className="secondary small"
                    onClick={() =>
                      call("save-outfit", outfit)
                        .then(() => {
                          setSaved(outfit);
                          setMessage("Outfit saved.");
                        })
                        .catch((e) => setMessage(String(e)))
                    }
                  >
                    Save outfit
                  </button>
                  <button
                    type="button"
                    className="primary"
                    disabled={!canApply}
                    onClick={() => void control.apply(outfit)}
                  >
                    Apply outfit
                  </button>
                  <button
                    type="button"
                    disabled={
                      !control.player ||
                      control.busy ||
                      (live &&
                        (!control.eligible ||
                          control.status?.boundary !== "selection"))
                    }
                    onClick={restore}
                  >
                    Restore original
                  </button>
                </div>
                {outfitPieceSlots.map(([slot, title, icon]) => {
                  const asset = assetFor(slot);
                  const piece = pieceFor(slot);
                  const image = piece ? garmentImage(piece) : undefined;
                  const text = asset ? label(asset) : `Choose ${title.toLowerCase()}`;
                  return (
                    <label className="blacktop-piece" key={slot}>
                      <button
                        className="blacktop-browse blacktop-piece-tile"
                        type="button"
                        aria-label={`Change ${title.toLowerCase()}`}
                        onClick={() => setPicker({ slot, name: title })}
                      >
                        <span className="blacktop-piece-category">{title}</span>
                        <span className="blacktop-piece-preview">
                          {image ? <img src={image} alt="" /> : <ItemIcon d={icon} />}
                        </span>
                        <span className="blacktop-piece-name" title={text}>
                          {text}
                        </span>
                        <span className="blacktop-piece-change">
                          <span>Change</span>
                          <span>↗</span>
                        </span>
                      </button>
                    </label>
                  );
                })}
                <section className="accessory-cards accessory-slots">
                  <div className="accessory-slots-heading">
                    <strong>Accessories</strong>
                    <span role="status" title="Up to three accessories per outfit.">
                      {outfit.accessories.length} / 3 equipped
                    </span>
                    <button
                      type="button"
                      disabled={!outfit.accessories.length}
                      onClick={() => setOutfit({ ...outfit, accessories: [] })}
                    >
                      Remove all
                    </button>
                  </div>
                  <div className="accessory-slots-grid accessory-primary-grid">
                    {outfitAccessorySlots.map(([slot, title, icon]) => {
                      const piece = pieceFor(slot);
                      const image = piece ? garmentImage(piece) : undefined;
                      return (
                        <div className="accessory-slot" data-slot={slot} key={slot}>
                          <button
                            type="button"
                            className="accessory-tile"
                            aria-label={`${piece ? "Change" : "Add"} ${title.toLowerCase()}`}
                            title={piece ? label(piece.asset) : "Add item"}
                            onClick={() => setPicker({ slot, name: title })}
                          >
                            <span className="accessory-tile-preview">
                              {image ? <img src={image} alt="" /> : <ItemIcon d={icon} />}
                            </span>
                            <span className="accessory-tile-category">{title}</span>
                            <span className="accessory-tile-name">
                              {piece ? label(piece.asset) : "Add item"}
                            </span>
                          </button>
                          {piece && (
                            <button
                              type="button"
                              className="accessory-tile-remove"
                              aria-label={`Remove ${title.toLowerCase()}`}
                              onClick={() => removeAccessory(piece.asset)}
                            >
                              ×
                            </button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </section>
              </article>
            </div>
          </div>
        </div>
      </main>
      <footer className="wpf-footer">
        <span>F8 to hide · Esc back to game</span>
        <span>Edit on player selection · Session-only outfits</span>
      </footer>
      {picker && (
        <ClothingPicker
          picker={picker}
          playerName={name}
          pool={pool}
          selected={assetFor(picker.slot)}
          onClose={() => setPicker(null)}
          onChoose={(asset) =>
            call<Outfit>("select-outfit-piece", { outfit, slot: picker.slot, asset })
              .then((o) => {
                setOutfit(o);
                setPicker(null);
              })
              .catch((e) => setMessage(String(e)))
          }
        />
      )}
    </div>
  );
}
// WPF renderer/clothing-picker.js in the overlay: a full-window dialog with
// search, category chips, 18 tiles per page and a page counter.
function ClothingPicker({
  picker,
  playerName,
  pool,
  selected,
  onClose,
  onChoose,
}: {
  picker: Picker;
  playerName: string;
  pool: string;
  selected: string;
  onClose: () => void;
  onChoose: (asset: string) => void;
}) {
  const [query, setQuery] = useState(""),
    [group, setGroup] = useState("All"),
    [offset, setOffset] = useState(0),
    [items, setItems] = useState<Garment[]>([]),
    [total, setTotal] = useState(0),
    [groups, setGroups] = useState<string[]>([]);
  const request = useRef(0);
  useEffect(() => {
    const id = ++request.current;
    const timer = setTimeout(
      () =>
        void call<{ items: Garment[]; total: number; groups: string[] }>("wardrobe", {
          slot: picker.slot,
          query,
          group,
          offset,
          limit: PAGE,
          pool,
        })
          .then((r) => {
            if (id !== request.current) return;
            setItems(r.items);
            setTotal(r.total);
            setGroups(r.groups);
          })
          .catch(() => {}),
      query ? 100 : 0,
    );
    return () => clearTimeout(timer);
  }, [picker.slot, query, group, offset, pool]);
  useEffect(() => {
    // Escape returns to the editor before the overlay's own Escape handling.
    function key(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopImmediatePropagation();
      onClose();
    }
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [onClose]);
  const pages = Math.max(1, Math.ceil(total / PAGE));
  const page = Math.floor(offset / PAGE);
  return (
    <>
      <div className="clothing-picker-backdrop" onClick={onClose} />
      <dialog open className="clothing-picker" aria-labelledby="clothing-picker-title">
        <div className="clothing-picker-heading">
          <h2 id="clothing-picker-title">
            {picker.name} · {playerName}
          </h2>
          <button type="button" onClick={onClose}>
            Back
          </button>
        </div>
        <div className="clothing-picker-filters">
          <input
            type="search"
            placeholder="Search clothing or brand"
            aria-label="Search clothing"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setOffset(0);
            }}
          />
          <button
            type="button"
            disabled={!query}
            onClick={() => {
              setQuery("");
              setOffset(0);
            }}
          >
            Clear search
          </button>
          <div className="clothing-category-filters" aria-label="Clothing categories">
            {["All", ...groups].map((g) => (
              <button
                key={g}
                type="button"
                data-category={g}
                aria-pressed={g === group}
                onClick={() => {
                  setGroup(g);
                  setOffset(0);
                }}
              >
                {g}
              </button>
            ))}
          </div>
        </div>
        <div className="clothing-picker-grid garment-grid">
          {items.map((g) => {
            const image = garmentImage(g);
            const isSelected = g.asset === selected;
            return (
              <button
                key={g.asset}
                className="clothing-tile"
                type="button"
                aria-pressed={isSelected}
                title={label(g.asset)}
                onClick={() => onChoose(g.asset)}
              >
                <span className="clothing-picture">
                  {image ? <img src={image} alt="" /> : "No preview"}
                </span>
                <strong className="clothing-tile-name">{label(g.asset)}</strong>
                {isSelected && <small className="clothing-selected">Selected</small>}
              </button>
            );
          })}
        </div>
        <div className="clothing-picker-pager">
          <button type="button" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
            Previous
          </button>
          <span role="status">
            {total
              ? `${offset + 1}–${Math.min(offset + PAGE, total)} of ${total} · Page ${page + 1} / ${pages}`
              : "No results"}
          </span>
          <button type="button" disabled={offset + PAGE >= total} onClick={() => setOffset(offset + PAGE)}>
            Next
          </button>
        </div>
      </dialog>
    </>
  );
}
