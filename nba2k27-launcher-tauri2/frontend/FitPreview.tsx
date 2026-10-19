// WPF renderer/fit-preview-ui.js ("Clothing mannequin" dialog) for the Rust
// launcher. The 3D view is WPF's own fit-preview/index.html + fast-viewer.js
// in an iframe; the payload comes from the reused WPF worker (fit-preview-host).
import React, { useEffect, useRef, useState } from "react";
import { call, garmentImage } from "./api";
import { fitPreview, type FitPayload } from "./fit-preview-host";
import type { Garment, Outfit } from "./types";

// clothing-catalog-ui.js label + fit-preview-ui.js readableName.
export function readableName(asset: string) {
  const label = asset
    .replace(/^female_/, "")
    .replace(/^acc_sock_/, "")
    .replace(/^acc_/, "")
    .replaceAll("_", " ")
    .replace(/\bgry\b/gi, "gray")
    .replace(/\bblk\b/gi, "black")
    .replace(/\bwht\b/gi, "white")
    .replace(/\b0000\b/g, "")
    .replace(/\s+/g, " ")
    .trim();
  if (/^tanktop solid (.+)$/i.test(label)) return label.replace(/^tanktop solid (.+)$/i, "$1 tank top");
  if (/^solid (.+) crew$/i.test(label)) return label.replace(/^solid (.+) crew$/i, "$1 crew socks");
  if (/^basketball shorts (.+) camo (.+)$/i.test(label))
    return label.replace(/^basketball shorts (.+) camo (.+)$/i, "$1 $2 camo shorts");
  return label.replace(/^shoe default$/i, "Default shoes");
}
const shapes: Record<string, string> = {
  Top: "M8 5 4 8l4 5v13h16V13l4-5-4-3-5 3h-6z",
  Bottom: "M7 5h18l2 21H17l-1-11-1 11H5z",
  Sock: "M11 4h10v14l5 3v5H10l-4-4 5-8z",
  Shoe: "M5 12l8 4 6-3 3 5 6 3v5H4v-8z",
};
type Thumb = { state: string; imagePath?: string; note?: string };
const thumbCache = new Map<string, string | null>();
/** Render-on-demand game thumbnail (WPF clothing-thumbnails.js) for one asset. */
export function useGameThumbnail(asset: string | undefined, preferred?: string) {
  const [url, setUrl] = useState<string | null>(preferred ?? (asset ? (thumbCache.get(asset) ?? null) : null));
  useEffect(() => {
    if (preferred) {
      setUrl(preferred);
      return;
    }
    if (!asset || asset === "Shirtless" || asset === "NO SHIRT") {
      setUrl(null);
      return;
    }
    if (thumbCache.has(asset)) {
      setUrl(thumbCache.get(asset) ?? null);
      return;
    }
    let alive = true;
    setUrl(null);
    call<Thumb>("previews:thumbnail", { asset })
      .then((r) => {
        const value = r.state === "ready" && r.imagePath ? garmentImage({ asset, category: "", imagePath: r.imagePath }) ?? null : null;
        if (r.state !== "busy") thumbCache.set(asset, value);
        if (alive) setUrl(value);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [asset, preferred]);
  return url;
}
function Photo({ label, asset, garment, onBrowse }: { label: string; asset?: string; garment?: Garment; onBrowse: () => void }) {
  const url = useGameThumbnail(asset, garment ? garmentImage(garment) : undefined);
  return (
    <button type="button" className="fit-preview-selected-photo" aria-label={`Browse ${label.toLowerCase()} catalog`} onClick={onBrowse}>
      <small>{label}</small>
      <div className={`fit-preview-selected-picture${url ? "" : " has-fallback"}`} data-asset={asset ?? ""}>
        {asset === "Shirtless" || asset === "NO SHIRT" ? (
          "No top"
        ) : url ? (
          <img src={url} alt="" />
        ) : (
          <svg viewBox="0 0 32 32" aria-hidden="true">
            <path d={shapes[label] ?? shapes.Top} />
          </svg>
        )}
      </div>
      <span className="fit-preview-selected-label">{asset ? readableName(asset) : `Choose ${label.toLowerCase()}`}</span>
    </button>
  );
}

export function FitPreviewDialog(p: {
  outfit: Outfit;
  setOutfit: (o: Outfit) => void;
  pictures: Garment[];
  /** Opens the outfit picker for a piece or accessory slot (WPF ClothingPicker / AccessoryControls). */
  onBrowse: (slot: string) => void;
  onClose: (message?: string) => void;
}) {
  const frame = useRef<HTMLIFrameElement>(null);
  const original = useRef(p.outfit);
  const state = useRef({ ready: false, generation: 0, mannequinShown: false, knownMeshes: [] as string[], knownTextures: [] as string[] });
  const [status, setStatus] = useState({ text: "Loading the mannequin…", state: "loading" });
  const [loading, setLoading] = useState(true);
  const [result, setResult] = useState<FitPayload | null>(null);
  const [readyTick, setReadyTick] = useState(0);
  const [busy, setBusy] = useState(false);
  const [items, setItems] = useState<string[]>([]);
  const post = (payload: FitPayload) => frame.current?.contentWindow?.postMessage({ type: "fit-preview:result", payload }, "*");
  useEffect(() => {
    function receive(event: MessageEvent) {
      if (event.source !== frame.current?.contentWindow) return;
      const s = state.current;
      if (event.data?.type === "fit-preview:ready") {
        Object.assign(s, { knownMeshes: [], knownTextures: [], mannequinShown: false, ready: true });
        setReadyTick((n) => n + 1);
      }
      if (event.data?.type === "fit-preview:displayed") s.mannequinShown = true;
      if (event.data?.type === "fit-preview:known") {
        s.knownMeshes = event.data.meshes || [];
        s.knownTextures = event.data.textures || [];
      }
      if (event.data?.type === "fit-preview:display-error") setStatus({ text: event.data.message, state: "error" });
      if (event.data?.type === "fit-preview:close") finish();
    }
    window.addEventListener("message", receive);
    return () => {
      window.removeEventListener("message", receive);
      fitPreview.cancel();
      // WPF: warm again while the Blacktop page stays open.
      void fitPreview.warm().catch(() => {});
    };
  }, []);
  // request(): debounced 35 ms after every selection change.
  useEffect(() => {
    const s = state.current;
    if (!s.ready) return;
    const token = ++s.generation;
    setStatus({ text: "Updating fit…", state: "loading" });
    setLoading(true);
    const timer = setTimeout(() => {
      call<{ items: string[] }>("previews:fit-items", p.outfit)
        .then((composed) => {
          if (token !== s.generation) return null;
          setItems(composed.items);
          return fitPreview.render(
            { items: composed.items, knownMeshes: s.knownMeshes, knownTextures: s.knownTextures, showMannequinEarly: !s.mannequinShown },
            (progress) => {
              if (token !== s.generation) return;
              post(progress);
              if (progress.parts.length) setLoading(false);
              setStatus({
                text: progress.stage === "mannequin" ? "Mannequin ready · loading outfit…" : `${progress.parts.length} garments · ${progress.stats.ms} ms base · loading artwork…`,
                state: "loading",
              });
            },
          );
        })
        .then((payload) => {
          if (!payload || token !== s.generation) return;
          post(payload);
          setLoading(false);
          setResult(payload);
          const unavailable = (payload.accessories || []).filter((a) => a.state === "unavailable");
          setStatus({
            text: unavailable.length ? `Preview ready · ${unavailable.length} accessory preview${unavailable.length === 1 ? "" : "s"} unavailable` : "Preview ready",
            state: "ready",
          });
        })
        .catch((error) => {
          if (token !== s.generation) return;
          setLoading(false);
          setStatus({ text: String(error?.message ?? error).replace(/^Error: /, ""), state: "error" });
        });
    }, 35);
    return () => clearTimeout(timer);
  }, [p.outfit, readyTick]);
  function finish(message?: string) {
    state.current.generation++;
    p.onClose(message);
  }
  async function randomize(mode: "clothes" | "accessories") {
    setBusy(true);
    try {
      p.setOutfit(await call<Outfit>("random-outfit", { mode, outfit: p.outfit }));
    } catch (error) {
      setStatus({ text: String(error), state: "error" });
    } finally {
      setBusy(false);
    }
  }
  const picture = (asset?: string) => p.pictures.find((g) => g.asset === asset);
  const slotOf = (asset: string) => picture(asset)?.category;
  // "Currently installed" previews the style's original shoe (see previews:fit-items).
  const shoe = p.outfit.shoes === "Currently installed" ? items.find((a) => /^shoe_/.test(a)) : p.outfit.shoes;
  const accessories = result?.accessories ?? [];
  const warnings = [
    ...(result?.warnings ?? []).map((w) => (/shoe color resources are unavailable/i.test(w) ? "Shoe colors unavailable. A neutral shoe shape is shown." : w)),
    ...accessories.filter((a) => a.state === "unavailable").map((a) => `${readableName(a.asset)}: ${a.reason}`),
  ];
  return (
    <div className="modal-backdrop fit-preview-backdrop">
      <dialog open className="fit-preview-dialog" aria-label="Clothing mannequin" style={{ position: "relative", margin: "auto" }}>
        <header className="fit-preview-header">
          <h2>Clothing mannequin</h2>
          <button type="button" className="primary" onClick={() => finish("Mannequin selections copied into the outfit draft. Save to keep them.")}>
            Use these choices
          </button>
          <button type="button" className="secondary" data-action="random-clothes" disabled={busy} title="Replace top, bottom and socks; keep shoes unchanged." onClick={() => void randomize("clothes")}>
            Random clothes
          </button>
          <button type="button" className="secondary" data-action="random-accessories" disabled={busy} title="Replace accessories with up to three items; keep clothes and shoes unchanged." onClick={() => void randomize("accessories")}>
            Random accessories
          </button>
          <button
            type="button"
            className="secondary"
            onClick={() => {
              p.setOutfit(original.current);
              finish();
            }}
          >
            Discard
          </button>
        </header>
        <div className="fit-preview-layout">
          <div className="fit-preview-view" data-loading={loading ? "true" : "false"}>
            <iframe ref={frame} title="3D clothing mannequin" src="./fit-preview/index.html" />
            <div className="fit-preview-loading">Updating fit…</div>
          </div>
          <aside className="fit-preview-sidebar">
            <p className="fit-preview-status" role="status" data-state={status.state}>
              {status.text}
            </p>
            <p className="fit-preview-count">
              {result ? `${result.parts.filter((x) => !x.accessorySlot).length} garments · ${p.outfit.accessories.length} accessories selected` : "4 garments selected"}
            </p>
            <div className="fit-preview-selected-photos">
              <Photo label="Top" asset={p.outfit.top} garment={picture(p.outfit.top)} onBrowse={() => p.onBrowse("top")} />
              <Photo label="Bottom" asset={p.outfit.bottom} garment={picture(p.outfit.bottom)} onBrowse={() => p.onBrowse("bottom")} />
              <Photo label="Sock" asset={p.outfit.socks} garment={picture(p.outfit.socks)} onBrowse={() => p.onBrowse("socks")} />
              <Photo label="Shoe" asset={shoe} garment={picture(shoe)} onBrowse={() => p.onBrowse("shoes")} />
            </div>
            {p.outfit.accessories.length > 0 && (
              <div className="mannequin-accessories">
                <div className="accessory-slots-heading">
                  <strong>Accessories</strong> <span>{p.outfit.accessories.length} selected</span>
                </div>
                {p.outfit.accessories.map((asset) => {
                  const r = accessories.find((a) => a.asset === asset);
                  return (
                    <div className="accessory-slot" key={asset}>
                      <button
                        type="button"
                        className="accessory-tile"
                        title={`Change ${readableName(asset)}`}
                        disabled={!slotOf(asset)}
                        onClick={() => p.onBrowse(slotOf(asset)!)}
                      >
                        {readableName(asset)}
                      </button>{" "}
                      <small className="accessory-preview-badge" data-state={r?.state || "checking"} title={r?.reason || ""}>
                        {r?.state === "shown" ? "3D preview" : r?.state === "unavailable" ? "3D unavailable" : "Checking preview"}
                      </small>
                      <button
                        type="button"
                        className="accessory-tile-remove"
                        aria-label={`Remove ${readableName(asset)}`}
                        onClick={() => p.setOutfit({ ...p.outfit, accessories: p.outfit.accessories.filter((x) => x !== asset) })}
                      >
                        ×
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
            {warnings.length > 0 && <p className="fit-preview-warning">{warnings.join(" ")}</p>}
            <p className="fit-preview-note">
              Choose a card to browse the full catalog. Shoe colors are an approximate render of the game’s saved colorway; team-dependent and unsupported shoes stay neutral. Save the outfit separately to keep your selection.
            </p>
            <details className="fit-preview-details">
              <summary>Details</summary>
              <p className="fit-preview-metrics">
                {result ? `${result.stats.ms} ms preparation · ${result.stats.cache.memory} memory / ${result.stats.cache.disk} disk / ${result.stats.cache.decoded} decoded` : ""}
              </p>
            </details>
          </aside>
        </div>
      </dialog>
    </div>
  );
}
