// Status text for the read-only game hang capture (button and Ctrl+Shift+F12).
export type GameCaptureResult = {
  file: string;
  snapshot: number;
  bytes: number;
  windowHung: boolean;
};
export type GameCaptureEvent =
  | { phase: "capturing" }
  | { phase: "captured"; result: GameCaptureResult }
  | { phase: "error"; message: string };
export function gameCaptureMessage(event: GameCaptureEvent) {
  if (event.phase === "capturing")
    return "Capturing game snapshot · the game may pause briefly";
  if (event.phase === "error") return `Game snapshot: ${event.message}`;
  const { result } = event;
  return `Game snapshot ${result.snapshot}/3 saved locally · ${(result.bytes / 1048576).toFixed(1)} MB${result.windowHung ? " · game window not responding" : ""} · ${result.file}`;
}
