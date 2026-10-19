import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import samples from "./samples.json";
export type Rule = {
  id: string;
  label: string;
  value: number;
  supported: boolean;
  choices: { value: number; label: string }[];
};
export type RulesSnapshot = {
  token: string;
  boundary: string;
  expiresInSeconds: number;
  rules: Rule[];
};
export function previewRules(): RulesSnapshot {
  return {
    token: "offline-preview",
    boundary: "offline preview",
    expiresInSeconds: 30,
    rules: samples.rules.map((r, i) => ({
      id: `preview-${i}`,
      label: r.name,
      value: 0,
      supported: true,
      choices: r.options.map((label, value) => ({ label, value })),
    })),
  };
}
// As in the WPF overlay, choosing a value is the change: no separate
// confirmation or Apply step. Only a different, mapped value is sent.
export function canApplyRule(rule: Rule, chosen: number, busy: boolean) {
  return (
    !busy &&
    rule.supported &&
    chosen !== rule.value &&
    rule.choices.some((c) => c.value === chosen)
  );
}
// WPF draws Off/On rules as two buttons and every other mapped rule as a list.
export function ruleUsesToggle(rule: Rule) {
  return (
    rule.choices.length === 2 &&
    rule.choices[0].label === "Off" &&
    rule.choices[1].label === "On"
  );
}
export function NativeRules({
  gameRoot,
  running,
  compact = false,
  autoRead = false,
  preview = false,
}: {
  gameRoot: string;
  running: boolean;
  compact?: boolean;
  autoRead?: boolean;
  preview?: boolean;
}) {
  const [snapshot, setSnapshot] = useState<RulesSnapshot | null>(null),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState(""),
    [expired, setExpired] = useState(false);
  const pending = useRef(false),
    generation = useRef(0),
    fixtureMode = useRef(false);
  const previewValues = useRef<RulesSnapshot>(previewRules());
  useEffect(() => {
    ++generation.current;
    setSnapshot(preview ? previewValues.current : null);
    setMessage("");
    fixtureMode.current = false;
    return () => {
      ++generation.current;
    };
  }, [gameRoot, running, preview]);
  useEffect(() => {
    setExpired(false);
    if (!snapshot) return;
    const timer = setTimeout(
      () => setExpired(true),
      snapshot.expiresInSeconds * 1000,
    );
    return () => clearTimeout(timer);
  }, [snapshot]);
  async function fetchSnapshot(fixture: boolean) {
    return preview
      ? { ...previewValues.current }
      : fixture
        ? (await call<{ sample: RulesSnapshot }>("smoke-game-rules")).sample
        : await call<RulesSnapshot>("native-rules-read", {
            acknowledgement: "read-only-rules",
            gameRoot,
          });
  }
  async function read(fixture = false) {
    if (pending.current) return;
    pending.current = true;
    fixtureMode.current = fixture;
    setBusy(true);
    setSnapshot(null);
    setMessage("Reading live rule values…");
    const g = generation.current;
    try {
      const next = await fetchSnapshot(fixture);
      if (g === generation.current) {
        setSnapshot(next);
        setMessage(
          preview
            ? "Offline preview — no game access; values are examples."
            : fixture
              ? "Isolated fixture — no live game access."
              : `${next.rules.length} live rules loaded`,
        );
      }
    } catch (e) {
      if (g === generation.current) setMessage(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  async function choose(rule: Rule, value: number) {
    if (!snapshot || pending.current || !canApplyRule(rule, value, busy))
      return;
    pending.current = true;
    setBusy(true);
    setMessage("Applying rule…");
    const g = generation.current;
    let current = snapshot;
    setSnapshot(null);
    try {
      if (preview) {
        previewValues.current = {
          ...previewValues.current,
          rules: previewValues.current.rules.map((r) =>
            r.id === rule.id ? { ...r, value } : r,
          ),
        };
        if (g === generation.current) {
          setSnapshot({ ...previewValues.current });
          setMessage(
            "Preview value changed locally. Other values were left unchanged; no game accessed.",
          );
        }
        return;
      }
      // A one-use snapshot that timed out is re-read first, so the chosen
      // value is still checked against the game's current values.
      if (expired) current = await fetchSnapshot(fixtureMode.current);
      const fresh = current.rules.find((r) => r.id === rule.id);
      if (!fresh || !canApplyRule(fresh, value, false)) {
        if (g === generation.current) {
          setSnapshot(current);
          setMessage("Rule value already current. Other values were left unchanged.");
        }
        return;
      }
      const result = await call<{ message: string }>("native-rules-apply", {
        acknowledgement: "apply-one-rule",
        gameRoot,
        token: current.token,
        field: rule.id,
        value,
      });
      if (g !== generation.current) return;
      const next = await fetchSnapshot(fixtureMode.current).catch(() => null);
      if (g !== generation.current) return;
      setSnapshot(next);
      setMessage(
        result.message || "Rule updated. Other values were left unchanged.",
      );
    } catch (e) {
      // Like WPF: drop the snapshot and ask for a refresh before retrying.
      if (g === generation.current)
        setMessage(
          `${String(e)} · Refresh values before trying again.`,
        );
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  useEffect(() => {
    if (autoRead && running && !preview && !window.__nativeOverlayProbe)
      void read();
  }, [autoRead, gameRoot, running, preview]);
  return (
    <div className={`native-rules ${compact ? "compact" : ""}`}>
      {!compact && <h4>Native Game Rules · Blacktop QA</h4>}
      {!compact && (
        <p className="muted">
          Reads the current Blacktop values; never forces rules On. Choosing a
          value changes game memory for this session, not game files. Changes
          are not automatically undone on menu navigation or launcher exit. To
          restore a value, choose its original setting again.
        </p>
      )}
      <div className="native-rules-heading">
        <p>
          Live values for this game session. Nothing changes until you choose a
          value.
        </p>
        <div className="toolbar">
          <button disabled={busy || !running} onClick={() => void read()}>
            {busy ? "Working…" : "Refresh values"}
          </button>
          {(window.__launcherSmoke || window.__nativeOverlayProbe) && (
            <button disabled={busy} onClick={() => void read(true)}>
              Load isolated rules fixture
            </button>
          )}
        </div>
      </div>
      <p role="status" className="native-rules-status">
        {message ||
          (running
            ? "Open Game rules to read the current values."
            : "Launch the game from this launcher first.")}
      </p>
      {snapshot && (
        <div className="native-rules-grid">
          {snapshot.rules.map((rule) => (
            <div
              className="native-rule-row"
              key={rule.id}
              data-rule={rule.id}
            >
              <span className="native-rule-name" id={`rule-${rule.id}-name`}>
                {rule.label}
              </span>
              <div
                className="native-rule-controls"
                role="group"
                aria-labelledby={`rule-${rule.id}-name`}
              >
                {!rule.supported ? (
                  <span
                    className="native-rule-number"
                    title="Numeric field; value meaning not yet established"
                  >
                    Value {rule.value}
                  </span>
                ) : ruleUsesToggle(rule) ? (
                  rule.choices.map((c) => (
                    <button
                      key={c.value}
                      type="button"
                      aria-pressed={rule.value === c.value}
                      disabled={busy || rule.value === c.value}
                      onClick={() => void choose(rule, c.value)}
                    >
                      {c.label}
                    </button>
                  ))
                ) : (
                  <select
                    aria-label={rule.label}
                    value={rule.value}
                    disabled={busy}
                    onChange={(e) => void choose(rule, Number(e.target.value))}
                  >
                    {rule.choices.map((c) => (
                      <option key={c.value} value={c.value}>
                        {c.label}
                      </option>
                    ))}
                  </select>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
      <p className="native-rules-note">
        Changes apply immediately to the current game state. The game may reload
        its own values when you change modes.
        {snapshot && !preview && expired
          ? " Values are re-read before the next change."
          : ""}
      </p>
    </div>
  );
}
