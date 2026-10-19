import { describe, expect, it } from "vitest";
import {
  cachedOverlayCatalog,
  orderCatalogGroups,
  loadCatalogPages,
  overlayCatalogKey,
  storeOverlayCatalog,
} from "./overlay-catalog-cache";

describe("overlay catalog cache", () => {
  const request = {
    kind: "outfit",
    slot: "top",
    pool: "male",
    query: "nike",
    group: "T-shirts",
    offset: 24,
  };

  it("keeps every selection and filter dimension in the cache identity", () => {
    const key = overlayCatalogKey(request);
    for (const changed of [
      { ...request, kind: "ball" },
      { ...request, slot: "bottom" },
      { ...request, pool: "female" },
      { ...request, query: "jordan" },
      { ...request, group: "Jersey" },
      { ...request, offset: 48 },
    ]) {
      expect(overlayCatalogKey(changed)).not.toBe(key);
    }
  });

  it("restores an exact immutable page and never substitutes a near match", () => {
    const cache = new Map();
    const result = {
      items: [{ id: "top-1" }],
      total: 30,
      groups: ["T-shirts"],
    };
    storeOverlayCatalog(cache, request, result);
    result.items.push({ id: "top-2" });
    result.groups.push("Jersey");

    expect(cachedOverlayCatalog(cache, request)).toEqual({
      items: [{ id: "top-1" }],
      total: 30,
      groups: ["T-shirts"],
    });
    expect(cachedOverlayCatalog(cache, { ...request, offset: 0 })).toBeNull();
  });
});
it("catalog categories follow the WPF order", () => {
  expect(
    orderCatalogGroups("arena", ["Gyms", "Historic", "NBA", "Other", "Outdoor", "WNBA"]),
  ).toEqual(["NBA", "WNBA", "Historic", "Gyms", "Outdoor", "Other"]);
  expect(
    orderCatalogGroups("scoreboard", ["Broadcast", "Classic", "Eras", "Experimental"]),
  ).toEqual(["Classic", "Broadcast", "Eras", "Experimental"]);
  expect(orderCatalogGroups("ball", ["All-Star", "Animals", "Installed"])).toEqual([
    "Installed",
    "All-Star",
    "Animals",
  ]);
});
it("loads every catalog page at once and keeps catalog order", async () => {
  const rows = Array.from({ length: 154 }, (_, i) => i);
  let inFlight = 0,
    peak = 0;
  const fetchPage = async (offset: number) => {
    inFlight++;
    peak = Math.max(peak, inFlight);
    await new Promise((r) => setTimeout(r, 5));
    inFlight--;
    return { items: rows.slice(offset, offset + 24), total: rows.length, groups: ["NBA"] };
  };
  const all = await loadCatalogPages(fetchPage, true);
  expect(all.items).toEqual(rows);
  expect(peak).toBeGreaterThan(1);
  const one = await loadCatalogPages(fetchPage, false, 48);
  expect(one.items).toEqual(rows.slice(48, 72));
});
