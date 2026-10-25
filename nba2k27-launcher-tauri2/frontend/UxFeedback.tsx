import { useEffect, useRef, useState } from "react";

export function useRetainedDraft<T>(key: string, saved: T) {
  const baseline = JSON.stringify(saved);
  const previous = useRef(baseline);
  const [draft, setDraft] = useState<T>(() => {
    try {
      const item = JSON.parse(sessionStorage.getItem(key) || "null");
      return item?.baseline === baseline ? item.draft : saved;
    } catch { return saved; }
  });
  useEffect(() => {
    if (previous.current !== baseline) {
      previous.current = baseline;
      setDraft(saved);
      try { sessionStorage.removeItem(key); } catch {}
      return;
    }
    try {
      if (JSON.stringify(draft) === baseline) sessionStorage.removeItem(key);
      else sessionStorage.setItem(key, JSON.stringify({ baseline, draft }));
    } catch {}
  }, [baseline, draft, key]);
  return [draft, setDraft] as const;
}

export function friendlyError(text: string): string {
  if (/scoreboard.*(ownership|stopped|held|restart)/i.test(text)) return "Scoreboard switching is paused. Close the game normally, then start a new session before choosing another scoreboard.";
  if (/unsupported game build/i.test(text)) return "This game update needs a compatible launcher. Open Settings to check game-build compatibility.";
  if (/access.*denied|permission denied/i.test(text)) return "The launcher could not access a required file. Close any app using it and check the file permissions.";
  if (text.length > 240 || /0x[0-9a-f]+|descriptor|ownership|Some\(/i.test(text)) return "This action could not finish. Review the details below before trying again.";
  return text.replace(/^Error:\s*/, "");
}

export function ActionError({ text }: { text?: string | null }) {
  const [copy, setCopy] = useState("");
  if (!text) return null;
  return <div className="action-error" role="alert">
    <p>{friendlyError(text)}</p>
    <details><summary>Details</summary><pre>{text}</pre>
      <button type="button" className="secondary small" onClick={() => {
        void navigator.clipboard.writeText(text).then(() => setCopy("Copied"), () => setCopy("Copy failed. Select the details above to copy manually."));
      }}>Copy report</button><span role="status">{copy}</span>
    </details>
  </div>;
}
