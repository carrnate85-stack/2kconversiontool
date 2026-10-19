export type OverlayCatalogResult<T> = {
  items: T[];
  total: number;
  groups: string[];
};

export type OverlayCatalogRequest = {
  kind: string;
  query: string;
  group: string;
  offset: number;
  slot?: string;
  pool?: string;
};

// Catalog pages are immutable for the lifetime of one overlay window. Keep the
// key explicit so changing slot, pool, filter, search or page can never reuse a
// visually similar but incorrect response.
export function overlayCatalogKey(request: OverlayCatalogRequest): string {
  return JSON.stringify([
    request.kind,
    request.slot ?? "",
    request.pool ?? "",
    request.query,
    request.group,
    request.offset,
  ]);
}

export function cachedOverlayCatalog<T>(
  cache: ReadonlyMap<string, OverlayCatalogResult<T>>,
  request: OverlayCatalogRequest,
): OverlayCatalogResult<T> | null {
  return cache.get(overlayCatalogKey(request)) ?? null;
}

export function storeOverlayCatalog<T>(
  cache: Map<string, OverlayCatalogResult<T>>,
  request: OverlayCatalogRequest,
  result: OverlayCatalogResult<T>,
): void {
  // Responses are treated as immutable snapshots. Copy the arrays so a caller
  // cannot later mutate the view that a reopened catalog restores instantly.
  cache.set(overlayCatalogKey(request), {
    items: [...result.items],
    total: result.total,
    groups: [...result.groups],
  });
}

// WPF category order: arena-browser.js groups, scoreboard-picker.js groups,
// ball catalog order (Installed first).
const catalogGroupOrder: Record<string, string[]> = {
  arena: ["NBA", "WNBA", "Historic", "Gyms", "Outdoor", "Other"],
  scoreboard: ["Classic", "Broadcast", "Eras", "Experimental"],
  ball: ["Installed"],
};
export function orderCatalogGroups(kind: string, groups: string[]) {
  const order = catalogGroupOrder[kind] ?? [];
  const rank = (g: string) => {
    const i = order.indexOf(g);
    return i < 0 ? order.length : i;
  };
  return [...groups].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}
// Load one page, or (all=true) the first page and then every remaining page
// at once (not one request after another), keeping catalog order.
export type CatalogPage<T> = { items: T[]; total: number; groups: string[] };
export async function loadCatalogPages<T>(
  fetchPage: (offset: number) => Promise<CatalogPage<T>>,
  all: boolean,
  offset = 0,
  cap = 2000,
): Promise<CatalogPage<T>> {
  const first = await fetchPage(all ? 0 : offset);
  if (!all || !first.items.length || first.items.length >= first.total)
    return first;
  const step = first.items.length;
  const offsets: number[] = [];
  for (let at = step; at < Math.min(first.total, cap); at += step)
    offsets.push(at);
  const rest = await Promise.all(offsets.map((at) => fetchPage(at)));
  return { ...first, items: [...first.items, ...rest.flatMap((p) => p.items)] };
}
