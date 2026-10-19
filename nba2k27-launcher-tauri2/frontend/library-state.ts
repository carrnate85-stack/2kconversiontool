import type { Mod } from "./types";
export function move<T>(items: T[], index: number, by: number) {
  const next = [...items],
    target = index + by;
  if (index < 0 || index >= next.length || target < 0 || target >= next.length)
    return next;
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}
export function uniqueName(base: string, names: string[]) {
  let next = base,
    n = 2;
  while (names.some((name) => name.toLowerCase() === next.toLowerCase()))
    next = `${base} ${n++}`;
  return next;
}
export type TreeRow = {
  path: string;
  depth: number;
  folder: boolean;
  name: string;
  bytes: number;
};
export function modRows(
  mods: Mod[],
  expanded: Set<string>,
  query: string,
  tags: Record<string, string>,
  root: string,
): TreeRow[] {
  const q = query.trim().toLowerCase();
  const rows = new Map<string, TreeRow>();
  for (const mod of mods) {
    if (
      q &&
      !`${mod.path} ${tags[`${root}|${mod.path}`] ?? ""}`
        .toLowerCase()
        .includes(q)
    )
      continue;
    const parts = mod.path.replaceAll("\\", "/").split("/");
    let parent = "";
    let visible = true;
    parts.forEach((name, i) => {
      const path = parent ? `${parent}/${name}` : name;
      const folder = i < parts.length - 1;
      if (visible || q) {
        const row = rows.get(path);
        if (row) row.bytes += mod.bytes;
        else rows.set(path, { path, depth: i, folder, name, bytes: mod.bytes });
      }
      visible = visible && (expanded.has(path) || !!q);
      parent = path;
    });
  }
  return [...rows.values()].sort((a, b) => {
    const ap = a.path.split("/"),
      bp = b.path.split("/");
    for (let i = 0; i < Math.min(ap.length, bp.length); i++) {
      if (ap[i] === bp[i]) continue;
      const af = i < ap.length - 1 || a.folder,
        bf = i < bp.length - 1 || b.folder;
      return (
        Number(bf) - Number(af) ||
        ap[i].localeCompare(bp[i], undefined, { numeric: true })
      );
    }
    return ap.length - bp.length;
  });
}
export type ModCategory =
  | "Player"
  | "Arena"
  | "Uniform"
  | "Localization"
  | "IFF"
  | "Texture"
  | "Audio"
  | "Other";
// Same rules as the WPF catalog (core/catalog.js classifyAsset).
export function classifyMod(path: string): ModCategory {
  const name = (path.split(/[\\/]/).pop() ?? "").toLowerCase();
  if (/^png\d+/.test(name)) return "Player";
  if (/^(arena|s|floor|stadium)_/.test(name)) return "Arena";
  if (/^(clothing|uniform|jersey|shorts)_/.test(name)) return "Uniform";
  if (/^(english|spanish|french|german|italian)\.iff$/.test(name))
    return "Localization";
  if (/\.(iff|iff\.bin)$/.test(name)) return "IFF";
  if (/\.(dds|png|jpg|jpeg)$/.test(name)) return "Texture";
  if (/\.(wav|mp3|flac|ogg)$/.test(name)) return "Audio";
  return "Other";
}
export function parseModTags(value: string | undefined) {
  const seen = new Set<string>(),
    tags: string[] = [];
  for (const item of String(value ?? "").split(",")) {
    const tag = item.trim().replace(/\s+/g, " ");
    if (!tag || seen.has(tag.toLowerCase())) continue;
    seen.add(tag.toLowerCase());
    tags.push(tag);
  }
  return tags;
}
// Validates the comma-separated editor value and returns the stored form.
export function normalizeModTags(value: string) {
  const tags = parseModTags(value);
  if (tags.some((t) => t.length > 40 || /[\u0000-\u001f]/.test(t)))
    throw Error("Each mod tag must contain at most 40 valid characters.");
  // WPF mod-tags.js limits: 12 tags × 40 characters (no other total).
  if (tags.length > 12) throw Error("A mod file can have at most 12 tags.");
  return tags.join(", ");
}
export type ModGroup = {
  id: string;
  title: string;
  stem: string;
  category: ModCategory;
  files: Mod[];
  base: Mod;
  tags: string[];
  bytes: number;
};
export type ModSortKey = "title" | "category" | "tags" | "size";
export const modTagKey = (root: string, mod: Mod) => `${root}|${mod.path}`;
// Player files that share a pngNNNN stem in one folder form a dependency
// bundle (WPF mod library); every other file is its own entry.
export function modGroups(
  mods: Mod[],
  tags: Record<string, string>,
  root: string,
): ModGroup[] {
  const byId = new Map<
    string,
    { id: string; stem: string; folder: string; files: Mod[] }
  >();
  for (const mod of mods) {
    const parts = mod.path.replaceAll("\\", "/").split("/");
    const leaf = parts.at(-1) ?? "",
      folder = parts.slice(0, -1).join("/");
    const stem = /^(png\d{4,6})(?=[_.]|$)/i.exec(leaf)?.[1] ?? "";
    const id = stem
      ? `player:${folder.toLowerCase()}:${stem.toLowerCase()}`
      : `file:${mod.path}`;
    const group = byId.get(id) ?? { id, stem, folder, files: [] };
    group.files.push(mod);
    byId.set(id, group);
  }
  return [...byId.values()].map((g) => {
    const base =
      (g.stem &&
        g.files.find(
          (f) => f.name.toLowerCase() === `${g.stem.toLowerCase()}.iff`,
        )) ||
      g.files[0];
    const seen = new Set<string>(),
      groupTags: string[] = [];
    for (const f of [base, ...g.files.filter((f) => f !== base)])
      for (const t of parseModTags(tags[modTagKey(root, f)]))
        if (!seen.has(t.toLowerCase())) {
          seen.add(t.toLowerCase());
          groupTags.push(t);
        }
    return {
      id: g.id,
      stem: g.stem,
      title: g.stem
        ? [g.folder, g.stem].filter(Boolean).join("/")
        : base.path.replaceAll("\\", "/"),
      category: classifyMod(base.path),
      files: g.files,
      base,
      tags: groupTags,
      bytes: g.files.reduce((sum, f) => sum + f.bytes, 0),
    };
  });
}
export function sortModGroups(
  groups: ModGroup[],
  key: ModSortKey,
  direction: 1 | -1,
) {
  const text = (g: ModGroup) =>
    key === "tags"
      ? g.tags.join(", ")
      : key === "category"
        ? g.category
        : g.title;
  return [...groups].sort(
    (a, b) =>
      ((key === "size"
        ? a.bytes - b.bytes
        : text(a).localeCompare(text(b), undefined, { numeric: true })) ||
        a.title.localeCompare(b.title, undefined, { numeric: true })) *
      direction,
  );
}
export function filterModGroups(groups: ModGroup[], query: string) {
  const q = query.trim().toLowerCase();
  if (!q) return groups;
  return groups.filter((g) =>
    `${g.title} ${g.category} ${g.tags.join(" ")} ${g.files.map((f) => f.path.replaceAll("\\", "/")).join(" ")}`
      .toLowerCase()
      .includes(q),
  );
}
// Tag edits apply to the whole bundle: stored on the base file, cleared on
// dependencies so removed tags do not linger.
export function withGroupTags(
  tags: Record<string, string>,
  root: string,
  group: ModGroup,
  value: string,
) {
  const next = { ...tags };
  for (const f of group.files) delete next[modTagKey(root, f)];
  if (value) next[modTagKey(root, group.base)] = value;
  return next;
}
