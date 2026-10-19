import { useEffect, useRef, useState } from "react";
import {
  playerMeasurements as units,
  type MeasurementInput,
} from "./editor-units";
const empty = (): MeasurementInput => ({ value: "", dataset: {} });
function model(raw?: number) {
  const feet = empty(),
    inches = empty(),
    wing = empty();
  units.writeHeight(feet, inches, raw);
  units.writeWingspan(wing, raw);
  return { feet, inches, wing };
}
export function PlayerMeasurements({
  kind,
  raw,
  busy,
  commit,
  error,
}: {
  kind: "heightCm" | "wingspanCm";
  raw?: number;
  busy: boolean;
  commit: (cm: number | undefined) => void;
  error: (message: string) => void;
}) {
  const [values, setValues] = useState(() => model(raw)),
    cancelled = useRef(false);
  const refs = useRef<
    Partial<Record<"feet" | "inches" | "wing", HTMLInputElement>>
  >({});
  useEffect(() => {
    setValues(model(raw));
    Object.values(refs.current).forEach((input) =>
      input?.setCustomValidity(""),
    );
  }, [raw, kind]);
  function apply() {
    if (cancelled.current) {
      cancelled.current = false;
      return;
    }
    try {
      if(Object.values(refs.current).some(input=>input?.validity.badInput))throw Error("Enter valid measurement numbers.");
      const missing =
        kind === "heightCm"
          ? !values.feet.value && !values.inches.value
          : !values.wing.value;
      const next = missing
        ? undefined
        : kind === "heightCm"
          ? units.readHeight(values.feet, values.inches)
          : units.readWingspan(values.wing);
      if (next !== raw) commit(next);
      error("");
    } catch (e) {
      Object.values(refs.current).forEach((input) =>
        input?.setCustomValidity(String(e)),
      );
      error(String(e));
    }
  }
  const input = (key: "feet" | "inches" | "wing", label: string) => (
    <input
      data-local-field
      ref={(input) => {
        if (input) refs.current[key] = input;
        else delete refs.current[key];
      }}
      aria-label={`DNA ${label}`}
      type="number"
      disabled={busy}
      value={values[key].value}
      step={key === "feet" ? 1 : 0.001}
      min={0}
      max={key === "feet" ? 8 : key === "inches" ? 11.999 : 126}
      placeholder="Not included"
      title={raw === undefined ? "Not included" : `Stored centimetres: ${raw}`}
      onFocus={() => {
        cancelled.current = false;
      }}
      onChange={(e) => {
        Object.values(refs.current).forEach((input) =>
          input?.setCustomValidity(""),
        );
        const value = e.target.value;
        setValues((previous) => ({
          ...previous,
          [key]: { ...previous[key], value },
        }));
      }}
      onBlur={apply}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          apply();
          cancelled.current = true;
          e.currentTarget.blur();
          queueMicrotask(() => {
            cancelled.current = false;
          });
        }
        if (e.key === "Escape") {
          e.preventDefault();
          cancelled.current = true;
          setValues(model(raw));
          Object.values(refs.current).forEach((input) =>
            input?.setCustomValidity(""),
          );
          error("");
          e.currentTarget.blur();
          queueMicrotask(() => {
            cancelled.current = false;
          });
        }
      }}
    />
  );
  return (
    <div className="measurement-field">
      <span>{kind === "heightCm" ? "Height" : "Wingspan"}</span>
      <div className="measurement-control">
        {kind === "heightCm" ? (
          <>
            <label>
              {input("feet", "Height feet")}
              <small>ft</small>
            </label>
            <label>
              {input("inches", "Height inches")}
              <small>in</small>
            </label>
          </>
        ) : (
          <label>
            {input("wing", "Wingspan inches")}
            <small>in</small>
          </label>
        )}
        <button
          type="button"
          disabled={busy || raw === undefined}
          aria-label={`Clear ${kind === "heightCm" ? "height" : "wingspan"}`}
          onClick={() => {
            setValues(model());
            Object.values(refs.current).forEach((input) =>
              input?.setCustomValidity(""),
            );
            commit(undefined);
            error("");
          }}
        >
          Clear
        </button>
      </div>
    </div>
  );
}
