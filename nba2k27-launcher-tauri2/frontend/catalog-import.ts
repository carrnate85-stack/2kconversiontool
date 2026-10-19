import type { PlayerField } from "./PlayerFieldNumber";
export type CatalogDocument = {
  info: Record<string, string | number>;
  fields: Record<string, number>;
};
// Only the six mappings supported by the original WPF NBA 2K23 import.
// Native validation of a file is not proof of a cross-game badge/signature mapping.
export const catalogSections = [
  {
    key: "info",
    name: "Player info",
    description: "Names, positions, jersey, birthday and years pro",
  },
  {
    key: "appearance",
    name: "Body & handedness",
    description: "Weight and preferred shooting/dunking hands",
  },
  {
    key: "attributes",
    name: "Ratings & potential",
    description: "Mapped attributes and potential",
  },
  {
    key: "tendencies",
    name: "Tendencies",
    description: "Mapped gameplay tendencies",
  },
  {
    key: "personality",
    name: "Personality",
    description: "Verified personality flags",
  },
  { key: "hotzones", name: "Hot zones", description: "Shooting zones" },
] as const;
export function catalogSectionCounts(
  document: CatalogDocument,
  fields: Pick<PlayerField, "key" | "group">[],
) {
  const counts: Record<string, number> = Object.fromEntries(
    catalogSections.map((s) => [s.key, 0]),
  );
  for (const key of Object.keys(document.info)) {
    const group = [
      "weightLbs",
      "cyberfaceId",
      "heightCm",
      "wingspanCm",
    ].includes(key)
      ? "appearance"
      : key === "potential"
        ? "attributes"
        : key === "fullNameId"
          ? "audio"
          : "info";
    if (group in counts) counts[group]++;
  }
  const byKey = new Map(fields.map((f) => [f.key, f.group]));
  for (const key of Object.keys(document.fields)) {
    const group = byKey.get(key);
    if (group && group in counts) counts[group]++;
  }
  return counts;
}
export function catalogCategoryName(value: string) {
  const names: Record<string, string> = {
    current: "Current NBA",
    historic: "Historic",
    all_time: "All-time",
    free_agent: "Free agents",
    wnba: "WNBA",
  };
  return names[value] ?? value.replaceAll("_", " ");
}
