import display from "../assets/shoe-model-display.json";
const models: Record<string, { name: string; brand: string }> = display;
export function shoeLabel(asset: string): string | undefined {
  const match = /^shoe_(\d{4})_([a-zA-Z0-9_]+)$/.exec(asset);
  if (!match) return;
  const model = models[match[1]];
  if (match[1] === "0000" && match[2] === "default") return "Default shoes";
  const colorway = match[2].replaceAll("_", " ");
  return `${model?.name ?? `Unidentified shoe ${match[1]}`} · ${colorway}`;
}
export function previewWarning(text: string) {
  const missing = /^(shoe_\d{4}_[a-zA-Z0-9_]+): no verified shoe model mapping\.$/.exec(text);
  if (missing) return `${shoeLabel(missing[1])}: 3D preview unavailable. Other clothing is still shown.`;
  return text.replace(/\bshoe_\d{4}_[a-zA-Z0-9_]+\b/g, asset => shoeLabel(asset) ?? asset);
}
