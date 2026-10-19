// WPF renderer/clothing-cache.js ("Prepare all previews") and render-on-demand
// game thumbnails for garment cards (core/clothing-thumbnails.js via Rust).
import React, { useEffect, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { call, garmentImage } from "./api";
import { useGameThumbnail } from "./FitPreview";
import { fitPreview } from "./fit-preview-host";
import type { Garment } from "./types";

export type ClothingCacheState = {
  phase: "idle" | "indexing" | "running" | "cancelling" | "cancelled" | "failed" | "complete";
  total: number;
  completed: number;
  ready: number;
  missing: number;
  failed: number;
  batch?: number;
  batches?: number;
  current?: string;
  error?: string;
};
export function clothingCacheText(s: ClothingCacheState) {
  const counts = `${s.batch ? `Batch ${s.batch} / ${s.batches} · ` : ""}${s.completed} / ${s.total} unique previews checked · ${s.ready} ready · ${s.missing} without previews · ${s.failed} failed`;
  return s.phase === "idle"
    ? "Previews also save as you browse."
    : s.phase === "indexing"
      ? "Finding available clothing previews…"
      : s.phase === "cancelling"
        ? "Finishing active pictures, then stopping…"
        : s.phase === "cancelled"
          ? `Stopped. ${counts}. Saved pictures will be reused next time.`
          : s.phase === "failed"
            ? `Could not prepare previews: ${s.error}`
            : s.phase === "complete"
              ? `Finished. ${counts}.`
              : counts;
}
export function useClothingCache() {
  // WPF fit-preview-ui.js: warm the mannequin worker when the Blacktop page
  // opens and cancel it when the page is left.
  useEffect(() => {
    void fitPreview.warm().catch(() => {});
    return () => fitPreview.cancel();
  }, []);
  const [state, setState] = useState<ClothingCacheState | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    listen<ClothingCacheState>("clothing-cache-status", (e) => setState(e.payload)).then((fn) => {
      if (disposed) fn();
      else off = fn;
    });
    call<ClothingCacheState>("previews:cache-status")
      .then((s) => !disposed && setState(s))
      .catch(() => {});
    return () => {
      disposed = true;
      off?.();
    };
  }, []);
  const invoke = (channel: string) => {
    setError("");
    call<ClothingCacheState>(channel)
      .then(setState)
      .catch((e) => setError(String(e)));
  };
  const busy = !!state && ["indexing", "running", "cancelling"].includes(state.phase);
  return {
    state,
    busy,
    text: error || (state ? clothingCacheText(state) : "Previews save as you browse."),
    start: () => invoke("previews:cache-start"),
    cancel: () => invoke("previews:cache-cancel"),
  };
}
/** Pack image when imported, otherwise the game's own store thumbnail. */
export function GarmentImg({ asset, garment, alt = "", fallback }: { asset?: string; garment?: Garment; alt?: string; fallback: ReactNode }) {
  const url = useGameThumbnail(asset, garment ? garmentImage(garment) : undefined);
  return url ? <img src={url} alt={alt} /> : <>{fallback}</>;
}
