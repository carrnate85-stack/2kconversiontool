import { useEffect, useRef, useState } from "react";
import { call } from "./api";
export type OverlayStatus = {
  enabled: boolean;
  phase: string;
  boundary: string;
  visible: boolean;
  catalog?: boolean;
  message: string;
  playerBinding: false;
  gameWrites: false;
};
export function NativeOverlayControls({
  gameRoot,
  running,
}: {
  gameRoot: string;
  running: boolean;
}) {
  const [status, setStatus] = useState<OverlayStatus | null>(null),
    [ack, setAck] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const pending = useRef(false);
  useEffect(() => {
    let alive = true,
      polling = false;
    async function poll() {
      if (polling) return;
      polling = true;
      try {
        const s = await call<OverlayStatus>("native-overlay-status");
        if (alive) setStatus(s);
      } catch (e) {
        if (alive) setError(String(e));
      } finally {
        polling = false;
      }
    }
    void poll();
    const timer = setInterval(() => void poll(), 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    setAck(false);
  }, [gameRoot, running]);
  async function control(channel: string) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      setStatus(
        await call<OverlayStatus>(
          channel,
          channel === "native-overlay-enable"
            ? { acknowledgement: "native-overlay-explicit-qa", gameRoot }
            : undefined,
        ),
      );
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return (
    <div className="native-overlay-controls">
      <h4>Native F8 overlay</h4>
      <p className="muted">
        F8 stays bound only to the Blacktop game launched from this launcher. The bar
        uses the same catalog UI as the offline preview. Arena and ball choices
        can be explicitly prepared at player selection after the native provider
        activates on a mapped build. Game Rules supports explicit, verified
        one-rule changes. Outfit and
        scoreboard catalogs remain local previews in this QA.
      </p>
      <label>
        <input
          type="checkbox"
          checked={ack}
          disabled={busy || !!status?.enabled}
          onChange={(e) => setAck(e.target.checked)}
        />{" "}
        I understand these limits; enable the owned native F8 window.
      </label>
      <div className="toolbar">
        <button
          disabled={!running || !ack || busy || !!status?.enabled}
          onClick={() => void control("native-overlay-enable")}
        >
          Enable native F8
        </button>
        <button
          disabled={busy || !status?.enabled}
          onClick={() => void control("native-overlay-toggle")}
        >
          Show / hide F8
        </button>
        <button
          disabled={busy || !status?.enabled}
          onClick={() => void control("native-overlay-disable")}
        >
          Stop native F8
        </button>
      </div>
      <p role="status" className="muted">
        {error || status?.message}{" "}
        {status?.enabled ? `· ${status.boundary}` : ""}
      </p>
    </div>
  );
}
