import { useEffect, useRef, useState } from "react";
import { ActionError } from "./UxFeedback";

// WPF launchOverlay parity (renderer index.html launchOverlay, app.js
// LAUNCH_PHASES / showLaunchOverlay / updateLaunchOverlay). WPF stage names,
// progress and wording, mapped onto the Rust provider's phase order so the
// bar only moves forward. The message is WPF's user-facing text, not the
// provider's technical log line.
import { launchProgress, forwardProgress } from "./launch-progress";
export { launchProgress, forwardProgress } from "./launch-progress";
export type { LaunchProgress } from "./launch-progress";
export function elapsedLabel(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${seconds} second${seconds === 1 ? "" : "s"}`;
}
export function LaunchOverlay({
  startedAt,
  phase,
  error,
  onDone,
}: {
  startedAt: number;
  phase: string | null | undefined;
  error?: string | null;
  onDone: () => void;
}) {
  const [now, setNow] = useState(Date.now);
  const [bar, setBar] = useState(8);
  const value = launchProgress(phase, error);
  const done = useRef(onDone);
  const stageSince = useRef({ phase, time: Date.now() });
  if (stageSince.current.phase !== phase) stageSince.current = { phase, time: Date.now() };
  done.current = onDone;
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    setBar((old) => forwardProgress(old, value));
    if (value.state !== "ready") return;
    // Success clears automatically. Failures stay until the user dismisses them.
    const timer = setTimeout(
      () => done.current(),
      value.state === "ready" ? 900 : 2600,
    );
    return () => clearTimeout(timer);
  }, [value.state, value.progress]);
  return (
    <div
      className="launch-overlay"
      role="status"
      aria-live="polite"
      aria-label="NBA 2K27 startup progress"
    >
      <section className="launch-dialog">
        <div className="launch-spinner" aria-hidden="true">
          <span />
        </div>
        <p className="eyebrow">STARTING GAME</p>
        <h2>{value.title}</h2>
        {value.state === "failed" ? <ActionError text={value.message} /> : <p>{value.message}</p>}
        <div className="launch-progress" aria-hidden="true">
          <span style={{ width: `${bar}%` }} />
        </div>
        <div className="launch-meta">
          <span>{value.step}</span>
          <span>{elapsedLabel(now - startedAt)}</span>
        </div>
        <small>
          {value.state === "failed" ? "Read the issue above, then dismiss to return to the launcher." : "Keep this launcher open. This window clears automatically when the game is ready."}
        </small>
        {value.state === "running" && now - stageSince.current.time > 15000 && <p role="status">This step is taking longer. Keep the launcher open while it waits for the game. Open details to check the current stage.</p>}
        <details className="launch-details"><summary>View details</summary><p>Current stage: {phase || "queued"}. Elapsed: {elapsedLabel(now - startedAt)}. Progress reflects stages, not a time estimate.</p></details>
        {value.state === "failed" && <button className="secondary" onClick={onDone}>Dismiss</button>}
      </section>
    </div>
  );
}
