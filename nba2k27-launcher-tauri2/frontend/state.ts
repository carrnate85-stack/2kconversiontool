import type { Outfit } from "./types";
import { shoeLabel } from "./shoe-labels";
export const initialOutfit: Outfit = {
  name: "My outfit",
  top: "Shirtless",
  bottom: "basketball shorts solid blk",
  shoes: "Currently installed",
  accessories: [],
};
export const togglePanel = (active: string | null, next: string) =>
  active === next ? null : next;
export function toggleAccessory(outfit: Outfit, asset: string): Outfit {
  if (outfit.accessories.includes(asset))
    return {
      ...outfit,
      accessories: outfit.accessories.filter((a) => a !== asset),
    };
  if (outfit.accessories.length === 3) return outfit;
  return { ...outfit, accessories: [...outfit.accessories, asset] };
}
export function stagedSeasons(ids: Set<number>, records: { id: number }[]) {
  return records.filter((r) => ids.has(r.id));
}
export const formatBytes = (n: number) =>
  n >= 1048576
    ? `${(n / 1048576).toFixed(1)} MB`
    : `${(n / 1024).toFixed(1)} KB`;
export const label = (id: string) =>
    shoeLabel(id) ?? id
    .replace(/\.iff$/, "")
    .split("/")
    .pop()!
    .replaceAll("_", " ")
    .replace(/([a-z])([A-Z])/g, "$1 $2");
export function formatVideoLength(ms?: number) {
  if (!ms || !Number.isFinite(ms)) return "—";
  const total = Math.round(ms / 1000);
  return total < 60
    ? `${total} sec`
    : `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}
