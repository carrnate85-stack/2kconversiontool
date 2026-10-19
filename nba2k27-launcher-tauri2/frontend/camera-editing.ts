import type { Library } from "./types";
type Camera = Library["cameras"][number];
// Same fixed English slots as the native localization validator. Stock Side
// and Nosebleeds names are intentionally locked; native validation is final.
const slots = [
  { id: 51, name: "Broadcast Low 2K18", max: "Pregame Shootaround".length },
  { id: 53, name: "Press 2K17", max: "Coach View".length },
  { id: 54, name: "Press 2K27", max: "DO NOT USE THIS CAM".length },
  { id: 52, name: "Beluba 2K17", max: "Full Court Corner".length },
  { id: 57, name: "Beluba 2K27", max: "Minigame Downtown".length },
  { id: 59, name: "Side", max: 4, locked: true },
  { id: 55, name: "Nosebleeds", max: 10, locked: true },
];
export function cameraNameSlot(camera: Camera) {
  return camera.id !== undefined
    ? slots.find((s) => s.id === camera.id)
    : slots.find((s) => s.name === camera.name);
}
export function renameCamera(camera: Camera, name: string): Camera {
  const slot = cameraNameSlot(camera);
  if (!slot || slot.locked) return camera;
  return { ...camera, id: slot.id, name };
}
export function resetCameraName(camera: Camera): Camera {
  const slot = cameraNameSlot(camera);
  return !slot || slot.locked
    ? camera
    : { ...camera, id: slot.id, name: slot.name };
}
export function cameraDraftError(cameras: Camera[]): string | null {
  const names = new Set<string>();
  for (const camera of cameras) {
    const slot = cameraNameSlot(camera),
      name = camera.name.trim();
    if (!slot)
      return "A camera has an unknown type; restore its original setup before saving.";
    if (!name || name.length > slot.max || !/^[\x20-\x7e]+$/.test(name))
      return `${slot.name}: use 1–${slot.max} printable English characters.`;
    if (slot.locked && name !== slot.name)
      return `${slot.name} retains its stock name.`;
    if (names.has(name.toLowerCase())) return "Camera names must be unique.";
    names.add(name.toLowerCase());
    if (
      ![camera.height, camera.zoom].every(
        (n) => Number.isInteger(n) && n >= -100 && n <= 100,
      )
    )
      return `${name}: height and zoom must be whole steps from -100 to 100.`;
  }
  return null;
}

export function cameraSetupError(
  cameras: Camera[],
  enabled: boolean,
): string | null {
  const draft = cameraDraftError(cameras);
  if (draft) return draft;
  if (enabled && !cameras.some((camera) => camera.enabled))
    return "Choose at least one camera or turn Cameras off.";
  return null;
}
