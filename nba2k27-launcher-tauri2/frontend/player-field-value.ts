import {
  playerDisplay,
  playerRaw,
  type PresentationField,
} from "./player-web-presentation";
export type NumericPlayerField = PresentationField & {
  label: string;
  maximum: number;
};
export function playerFieldValue(
  field: NumericPlayerField,
  text: string,
  raw?: number,
): number | undefined {
  const n = Number(text);
  // An import can contain packed values that display above the editable range.
  // WPF preserves them until the user actually changes the displayed value.
  if (text.trim() && raw !== undefined && n === playerDisplay(field, raw))
    return raw;
  if (field.maximum < 0) throw Error(`${field.label} is read-only.`);
  if (text === "") return undefined;
  const attributes = field.group === "attributes",
    money = !!field.money;
  if (
    !text.trim() ||
    !Number.isFinite(n) ||
    (money ? !/^\d+(?:\.\d{1,6})?$/.test(text) : !Number.isInteger(n)) ||
    n < (attributes ? 25 : 0) ||
    n > (money ? 200 : attributes ? 99 : field.maximum)
  )
    throw Error(
      `Enter a valid ${field.label}${money ? " (0–200 million; up to six decimal places)" : attributes ? " (25–99)" : ""}.`,
    );
  const next = playerRaw(field, n);
  if (!Number.isInteger(next) || next < 0 || next > field.maximum)
    throw Error(`Converted ${field.label} exceeds its stored range.`);
  return next;
}
