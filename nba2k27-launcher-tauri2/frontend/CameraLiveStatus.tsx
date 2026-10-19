import { useEffect, useRef, useState } from "react";
import { call } from "./api";

// WPF renderCameraStatus parity: live phase/message from the provider's
// v2Features "cameras" entry, Retry when a hold is restorable or after Stop,
// and Stop & restore while cameras are waiting or active.
export type CameraFeature = {
  phase: string;
  message: string;
  retryable?: boolean;
};
type Session = {
  session?: string;
  nativeProvider?: {
    v2Features?: { features?: Record<string, CameraFeature> };
  } | null;
};
const running = new Set(["waiting", "active", "reload", "reset"]);
export function cameraActions(feature?: CameraFeature | null) {
  return {
    retry:
      !!feature &&
      (feature.phase === "stopped" ||
        (feature.phase === "held" && feature.retryable === true)),
    stop: !!feature && running.has(feature.phase),
  };
}
export function cameraDot(phase?: string) {
  return phase === "active"
    ? "live"
    : phase && running.has(phase)
      ? "waiting"
      : phase === "held"
        ? "error"
        : "";
}
export function CameraLiveStatus({ gameRoot }: { gameRoot: string }) {
  const [session, setSession] = useState<Session | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    let polling = false;
    async function poll() {
      if (polling) return;
      polling = true;
      try {
        const next = await call<Session>("native-session-status");
        if (alive.current) setSession(next);
      } catch {
        // Status is advisory here; Play shows session errors.
      } finally {
        polling = false;
      }
    }
    void poll();
    const timer = setInterval(() => void poll(), 1500);
    return () => {
      alive.current = false;
      clearInterval(timer);
    };
  }, []);
  const feature =
    session?.nativeProvider?.v2Features?.features?.cameras ?? null;
  const actions = cameraActions(feature);
  async function send(command: "retry" | "stop") {
    if (!session?.session || busy) return;
    setBusy(true);
    setError("");
    try {
      await call("native-camera-command", {
        acknowledgement: "control-cameras-for-current-game",
        gameRoot,
        session: session.session,
        command,
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const phase = feature?.phase ?? "stopped";
  return (
    // WPF keeps .camera-status hidden until a session reports camera state.
    <div
      className={feature ? "camera-status" : "camera-status hidden"}
      aria-live="polite"
    >
      <span className={`status-dot ${cameraDot(feature?.phase)}`} />
      <div>
        <strong>
          {feature ? phase.replaceAll("-", " ").toUpperCase() : "NOT RUNNING"}
        </strong>
        <p>
          {feature?.message ??
            "Cameras apply when Play starts a modded game with cameras enabled."}
        </p>
        {(actions.retry || actions.stop) && (
          <div className="toolbar">
            {actions.retry && (
              <button disabled={busy} onClick={() => void send("retry")}>
                {busy ? "Retrying…" : "Retry cameras"}
              </button>
            )}
            {actions.stop && (
              <button disabled={busy} onClick={() => void send("stop")}>
                Stop &amp; restore
              </button>
            )}
          </div>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}
