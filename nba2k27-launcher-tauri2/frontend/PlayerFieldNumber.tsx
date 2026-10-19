import { useEffect, useRef, useState } from "react";
import {
  playerDisplay,
  type PresentationField,
} from "./player-web-presentation";
import { playerFieldValue } from "./player-field-value";
export type PlayerField = PresentationField & {
  label: string;
  maximum: number;
  choices: string[];
};
// The web editor owns presentation only; the Rust DNA validator remains the
// authority for stored values. Visiting a rounded display must not rewrite it.
export function PlayerFieldNumber({
  field,
  raw,
  busy,
  commit,
  error,
}: {
  field: PlayerField;
  raw?: number;
  busy: boolean;
  commit: (value: number | undefined) => void;
  error: (message: string) => void;
}) {
  const formatted = raw === undefined ? "" : String(playerDisplay(field, raw));
  const [text, setText] = useState(formatted);
  const numberRef = useRef<HTMLInputElement>(null);
  const cancelled = useRef(false);
  useEffect(() => {
    setText(formatted);
    numberRef.current?.setCustomValidity("");
  }, [raw, formatted]);
  const attributes = field.group === "attributes",
    money = !!field.money;
  const disabled = busy || field.maximum < 0;
  function update(value: string) {
    if (disabled) return;
    const next = playerFieldValue(field, value, raw);
    if (next !== raw) commit(next);
  }
  const number = (
    <input
      ref={numberRef}
      data-local-field
      aria-label={`DNA ${field.label}`}
      type="number"
      min={attributes ? 25 : 0}
      max={
        money
          ? 200
          : attributes
            ? Math.max(99, Number(formatted) || 99)
            : field.maximum
      }
      step={money ? "0.000001" : 1}
      disabled={disabled}
      value={text}
      placeholder="Not included"
      title={raw === undefined ? "Not included" : `Stored value: ${raw}`}
      onFocus={() => {
        cancelled.current = false;
      }}
      onChange={(e) => {
        setText(e.target.value);
        e.currentTarget.setCustomValidity("");
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        }
        if (e.key === "Escape") {
          e.preventDefault();
          cancelled.current = true;
          setText(formatted);
          e.currentTarget.setCustomValidity("");
          error("");
          e.currentTarget.blur();
          queueMicrotask(() => {
            cancelled.current = false;
          });
        }
      }}
      onBlur={(e) => {
        if (cancelled.current) {
          cancelled.current = false;
          return;
        }
        if(e.currentTarget.validity.badInput) {
          const message=`Enter a valid ${field.label}.`;
          e.currentTarget.setCustomValidity(message);error(message);return;
        }
        if (text === formatted) {
          e.currentTarget.setCustomValidity("");
          error("");
          return;
        }
        try {
          update(text);
          error("");
        } catch (failure) {
          setText(formatted);
          e.currentTarget.setCustomValidity(String(failure));
          error(String(failure));
        }
      }}
    />
  );
  if (!attributes && field.group !== "tendencies") return number;
  return (
    <span className="studio-attribute-control">
      <input
        type="range"
        aria-label={`${field.label} slider`}
        min={attributes ? 25 : 0}
        max={attributes ? 99 : field.maximum}
        step={1}
        disabled={
          disabled ||
          raw === undefined ||
          Number(formatted) > (attributes ? 99 : field.maximum)
        }
        value={raw === undefined ? (attributes ? 25 : 0) : Number(formatted)}
        onChange={(e) => {
          try {
            update(e.target.value);
            error("");
          } catch (e) {
            error(String(e));
          }
        }}
      />
      {number}
    </span>
  );
}
