import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import { argbToInput, colorHex, inputToArgb } from "./editor-units";
type Values = Record<string, string | number>;
type Team = {
  format: "2kteam";
  version: 1;
  game: "NBA2K27";
  name: string;
  info: Values;
  arena: { name: string; arenaFile: string; floorFile: string } | null;
  staff: { slot: number; values: Values }[];
  rotations: string[][] | null;
  uniforms: { match: Record<string, number>; values: Values }[];
};
type Field = {
  key: string;
  label: string;
  group: string;
  minimum?: number;
  maximum: number;
  text?: boolean;
  color?: boolean;
  choices: string[];
};
type Schema = {
  sections: string[];
  roles: string[];
  staff: Field[];
  uniforms: Field[];
};
const labels: Record<string, string> = {
  info: "Team info",
  arena: "Arena / floor",
  staff: "Staff & coaches",
  rotations: "Rotations",
  uniforms: "Uniforms",
};
const blank = (): Team => ({
  format: "2kteam",
  version: 1,
  game: "NBA2K27",
  name: "New team",
  info: {},
  arena: null,
  staff: [],
  rotations: null,
  uniforms: [],
});
// Staging survives navigation in this window; it is never an automatic disk save.
let session: { draft: Team | null; saved: Team | null } | null = null;
function MappedFields({
  defs,
  values,
  update,
  prefix,
  filter,
  busy,
}: {
  defs: Field[];
  values: Values;
  update: (v: Values) => void;
  prefix: string;
  filter: string;
  busy: boolean;
}) {
  function edit(f: Field, value: string) {
    const next = { ...values };
    if (value === "") delete next[f.key];
    else next[f.key] = f.text ? value : Number(value);
    update(next);
  }
  return (
    <div className="dna-field-grid">
      {defs
        .filter((f) =>
          `${f.label} ${f.key} ${f.group}`
            .toLowerCase()
            .includes(filter.toLowerCase()),
        )
        .map((f) => (
          <label key={f.key}>
            <span title={f.group}>{f.label}</span>
            {f.color ? (
              <span className="team-color-control">
                <input
                  type="color"
                  aria-label={`${prefix} ${f.label} color`}
                  disabled={
                    busy ||
                    (values[f.key] !== undefined &&
                      (!Number.isInteger(values[f.key]) ||
                        Number(values[f.key]) < 0 ||
                        Number(values[f.key]) > 0xffffffff))
                  }
                  value={argbToInput(values[f.key] as number | undefined)}
                  title="Existing alpha is preserved. New colors default to opaque."
                  onChange={(e) =>
                    edit(
                      f,
                      String(
                        inputToArgb(
                          e.target.value,
                          values[f.key] === undefined
                            ? 0xff000000
                            : Number(values[f.key]),
                        ),
                      ),
                    )
                  }
                />
                <input
                  aria-label={`${prefix} ${f.label}`}
                  type="number"
                  disabled={busy}
                  min={0}
                  max={0xffffffff}
                  step={1}
                  placeholder="Not included"
                  value={values[f.key] ?? ""}
                  onChange={(e) => edit(f, e.target.value)}
                />
                <code>
                  {values[f.key] === undefined
                    ? "Not included"
                    : colorHex(Number(values[f.key]))}
                </code>
              </span>
            ) : f.choices.length ? (
              <select
                aria-label={`${prefix} ${f.label}`}
                disabled={busy}
                value={values[f.key] ?? ""}
                onChange={(e) => edit(f, e.target.value)}
              >
                <option value="">Not included</option>
                {f.choices.map((c, i) => (
                  <option key={i} value={i}>
                    {c}
                  </option>
                ))}
                {f.key.endsWith("SystemId") && (
                  <option value={15}>None / unset</option>
                )}
              </select>
            ) : (
              <input
                aria-label={`${prefix} ${f.label}`}
                disabled={busy}
                type={f.text ? "text" : "number"}
                min={f.minimum ?? 0}
                max={f.maximum}
                step={1}
                maxLength={f.text ? f.maximum : undefined}
                placeholder="Not included"
                value={values[f.key] ?? ""}
                onChange={(e) => edit(f, e.target.value)}
              />
            )}
          </label>
        ))}
    </div>
  );
}
const infoFields: Field[] = [
  ...(
    [
      ["name", "Team name", 23],
      ["cityName", "City", 17],
      ["cityShort", "City abbreviation", 6],
      ["stateShort", "State abbreviation", 6],
      ["mainLogo", "Main logo", 35],
    ] as const
  ).map(([key, label, maximum]) => ({
    key,
    label,
    maximum,
    text: true,
    group: "Info",
    choices: [],
  })),
  {
    key: "rosterTypeId",
    label: "Roster type ID",
    maximum: 31,
    group: "Info",
    choices: [],
  },
  {
    key: "historicYearId",
    label: "Historic year ID",
    maximum: 127,
    group: "Info",
    choices: [],
  },
];
export function TeamDNA({ status }: { status: (s: string) => void }) {
  const [schema, setSchema] = useState<Schema | null>(null),
    [draft, setDraft] = useState<Team | null>(session?.draft ?? null),
    [saved, setSaved] = useState<Team | null>(session?.saved ?? null);
  const [tab, setTab] = useState("info"),
    [filter, setFilter] = useState(""),
    [slot, setSlot] = useState(0),
    [uniform, setUniform] = useState(0),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [review, setReview] = useState<Team | null>(null),
    [sections, setSections] = useState<string[]>([]),
    [fresh, setFresh] = useState(false);
  const running = useRef(false),
    dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  useEffect(() => {
    let alive = true;
    Promise.all([
      call<Schema>("team-dna-schema"),
      session
        ? Promise.resolve(session)
        : call<Team | null>("team-dna").then((d) => ({ draft: d, saved: d })),
    ])
      .then(([s, entry]) => {
        if (alive) {
          session = entry;
          setSchema(s);
          setDraft(entry.draft);
          setSaved(entry.saved);
        }
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, []);
  function change(d: Team | null) {
    session = { draft: d, saved };
    setDraft(d);
  }
  async function run(fn: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError("");
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  const staff = draft?.staff.find((s) => s.slot === slot),
    u = draft?.uniforms[uniform];
  const renderFields = (
    defs: Field[],
    values: Values,
    update: (v: Values) => void,
    prefix: string,
  ) => (
    <MappedFields
      defs={defs}
      values={values}
      update={update}
      prefix={prefix}
      filter={filter}
      busy={busy}
    />
  );
  return (
    <section className="card team-dna-editor">
      <header>
        <h3>Team DNA · offline editor</h3>
        <span className="badge">
          {dirty ? "Unsaved local edits" : "Local draft"}
        </span>
      </header>
      <div className="card-content">
        <div className="toolbar">
          <button
            disabled={busy || !schema}
            onClick={() => {
              if (dirty) setFresh(true);
              else change(blank());
            }}
          >
            New local team
          </button>
          <button
            disabled={busy || !schema}
            onClick={() =>
              void run(async () => {
                const r = await call<Team | null>("import-team-dna");
                if (r) {
                  if (!draft) change(blank());
                  setReview(r);
                  setSections(schema?.sections ?? []);
                }
              })
            }
          >
            Import .2kteam
          </button>
          <button
            disabled={busy || !draft}
            onClick={() =>
              void run(async () => {
                const r = await call<{ path: string }>(
                  "export-team-dna",
                  draft,
                );
                status(`Team DNA exported: ${r.path}`);
              })
            }
          >
            Export .2kteam
          </button>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await call("open-library", { kind: "Teams" });
              })
            }
          >
            Open team folder
          </button>
        </div>
        <p className="muted compact-note">
          Local draft only. Team IDs, staff assignment IDs and process metadata
          are never transferred. Live-team loading, destination-player matching
          and Save &amp; verify are still unported.
        </p>
        {error && (
          <p className="notice" role="alert">
            {error}
          </p>
        )}
        {draft && schema && (
          <>
            <div className="toolbar dna-name">
              <label>
                Team DNA name{" "}
                <input
                  aria-label="Team DNA name"
                  disabled={busy}
                  maxLength={80}
                  value={draft.name}
                  onChange={(e) => change({ ...draft, name: e.target.value })}
                />
              </label>
            </div>
            <div className="dna-layout">
              <nav aria-label="Team DNA sections">
                {schema.sections.map((s) => (
                  <button
                    key={s}
                    className={tab === s ? "active" : ""}
                    onClick={() => {
                      setTab(s);
                      setFilter("");
                    }}
                  >
                    {labels[s]}
                  </button>
                ))}
              </nav>
              <div className="dna-fields">
                {["info", "staff", "uniforms"].includes(tab) && (
                  <input
                    className="wide-input"
                    aria-label="Filter team DNA fields"
                    placeholder="Search fields in this section"
                    value={filter}
                    onChange={(e) => setFilter(e.target.value)}
                  />
                )}
                {tab === "info" &&
                  renderFields(
                    infoFields,
                    draft.info,
                    (info) => change({ ...draft, info }),
                    "Team",
                  )}
                {tab === "arena" && (
                  <>
                    <div className="toolbar">
                      <button
                        disabled={busy}
                        onClick={() =>
                          change({
                            ...draft,
                            arena: draft.arena
                              ? null
                              : { name: "", arenaFile: "", floorFile: "" },
                          })
                        }
                      >
                        {draft.arena
                          ? "Exclude arena / floor"
                          : "Include arena / floor"}
                      </button>
                    </div>
                    {draft.arena && (
                      <div className="dna-field-grid">
                        {(
                          [
                            ["name", "Arena name", 32],
                            ["arenaFile", "Arena asset", 37],
                            ["floorFile", "Floor asset", 39],
                          ] as const
                        ).map(([key, label, max]) => (
                          <label key={key}>
                            <span>{label}</span>
                            <input
                              disabled={busy}
                              aria-label={label}
                              maxLength={max}
                              value={draft.arena![key]}
                              onChange={(e) =>
                                change({
                                  ...draft,
                                  arena: {
                                    ...draft.arena!,
                                    [key]: e.target.value,
                                  },
                                })
                              }
                            />
                          </label>
                        ))}
                      </div>
                    )}
                    <p className="muted">
                      Arena and floor are separate asset stems, without paths or
                      extensions.
                    </p>
                  </>
                )}
                {tab === "staff" && (
                  <>
                    <div className="toolbar">
                      <select
                        aria-label="Staff slot"
                        value={slot}
                        onChange={(e) => setSlot(Number(e.target.value))}
                      >
                        {schema.roles.map((role, i) => (
                          <option value={i} key={i}>
                            {role.replaceAll("_", " ")}
                            {draft.staff.some((s) => s.slot === i)
                              ? " · included"
                              : ""}
                          </option>
                        ))}
                      </select>
                      <button
                        disabled={busy}
                        onClick={() =>
                          change({
                            ...draft,
                            staff: staff
                              ? draft.staff.filter((s) => s.slot !== slot)
                              : [...draft.staff, { slot, values: {} }],
                          })
                        }
                      >
                        {staff ? "Exclude staff slot" : "Include staff slot"}
                      </button>
                    </div>
                    {staff ? (
                      renderFields(
                        schema.staff,
                        staff.values,
                        (values) =>
                          change({
                            ...draft,
                            staff: draft.staff.map((s) =>
                              s.slot === slot ? { ...s, values } : s,
                            ),
                          }),
                        "Staff",
                      )
                    ) : (
                      <p className="muted">
                        Include this role to stage identity, tendencies,
                        attributes, systems, proficiency and badges.
                      </p>
                    )}
                    <p className="muted">
                      Raw mapped attribute and badge levels are shown without
                      tier conversion.
                    </p>
                  </>
                )}
                {tab === "rotations" && (
                  <>
                    <div className="toolbar">
                      <button
                        disabled={busy}
                        onClick={() =>
                          change({
                            ...draft,
                            rotations: draft.rotations
                              ? null
                              : Array.from({ length: 48 }, () => [
                                  "Player 1",
                                  "Player 2",
                                  "Player 3",
                                  "Player 4",
                                  "Player 5",
                                ]),
                          })
                        }
                      >
                        {draft.rotations
                          ? "Exclude rotations"
                          : "Create 48 segments"}
                      </button>
                    </div>
                    <p className="muted">
                      Each minute needs five distinct names. Names must be
                      matched to a destination roster before live use.
                    </p>
                    {draft.rotations && (
                      <div className="table-scroll team-rotation-grid">
                        <table>
                          <thead>
                            <tr>
                              <th>Minute</th>
                              {[1, 2, 3, 4, 5].map((n) => (
                                <th key={n}>Player {n}</th>
                              ))}
                            </tr>
                          </thead>
                          <tbody>
                            {draft.rotations.map((row, i) => (
                              <tr key={i}>
                                <td>{i + 1}</td>
                                {row.map((name, j) => (
                                  <td key={j}>
                                    <input
                                      disabled={busy}
                                      aria-label={`Rotation minute ${i + 1} player ${j + 1}`}
                                      maxLength={39}
                                      value={name}
                                      onChange={(e) =>
                                        change({
                                          ...draft,
                                          rotations: draft.rotations!.map(
                                            (r, k) =>
                                              k === i
                                                ? r.map((n, l) =>
                                                    l === j
                                                      ? e.target.value
                                                      : n,
                                                  )
                                                : r,
                                          ),
                                        })
                                      }
                                    />
                                  </td>
                                ))}
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    )}
                  </>
                )}
                {tab === "uniforms" && (
                  <>
                    <div className="toolbar">
                      <select
                        aria-label="Uniform row"
                        value={uniform}
                        onChange={(e) => setUniform(Number(e.target.value))}
                      >
                        {draft.uniforms.map((_, i) => (
                          <option key={i} value={i}>
                            Uniform {i + 1}
                          </option>
                        ))}
                      </select>
                      <button
                        disabled={busy || draft.uniforms.length >= 128}
                        onClick={() => {
                          const next = draft.uniforms.length;
                          change({
                            ...draft,
                            uniforms: [
                              ...draft.uniforms,
                              {
                                match: {
                                  typeId: 0,
                                  sideId: 0,
                                  designationId: 0,
                                },
                                values: {},
                              },
                            ],
                          });
                          setUniform(next);
                        }}
                      >
                        Add uniform row
                      </button>
                      {u && (
                        <button
                          disabled={busy}
                          onClick={() => {
                            change({
                              ...draft,
                              uniforms: draft.uniforms.filter(
                                (_, i) => i !== uniform,
                              ),
                            });
                            setUniform(0);
                          }}
                        >
                          Remove uniform row
                        </button>
                      )}
                    </div>
                    {u && (
                      <>
                        <h4>Destination slot match</h4>
                        {renderFields(
                          schema.uniforms.filter((f) =>
                            ["typeId", "sideId", "designationId"].includes(
                              f.key,
                            ),
                          ),
                          u.match,
                          (match) =>
                            change({
                              ...draft,
                              uniforms: draft.uniforms.map((r, i) =>
                                i === uniform
                                  ? {
                                      ...r,
                                      match: match as Record<string, number>,
                                    }
                                  : r,
                              ),
                            }),
                          "Uniform match",
                        )}
                        <h4>Selected uniform values</h4>
                        {renderFields(
                          schema.uniforms,
                          u.values,
                          (values) =>
                            change({
                              ...draft,
                              uniforms: draft.uniforms.map((r, i) =>
                                i === uniform ? { ...r, values } : r,
                              ),
                            }),
                          "Uniform",
                        )}
                        <p className="muted">
                          RGB pickers preserve existing alpha. Exact unsigned ARGB values, including
                          alpha. Matching uses type, side and designation, not
                          source uniform IDs.
                        </p>
                      </>
                    )}
                  </>
                )}
              </div>
            </div>
            <div className="toolbar dna-save">
              <button
                disabled={busy || !dirty}
                onClick={() => {
                  change(saved);
                  setError("");
                  status(
                    "Local Team DNA edits reset · no game refresh required",
                  );
                }}
              >
                Reset team edits
              </button>
              <button
                className="primary"
                disabled={busy || !dirty}
                onClick={() =>
                  void run(async () => {
                    const r = await call<Team>("save-team-dna", draft);
                    session = { draft: r, saved: r };
                    setSaved(r);
                    setDraft(r);
                    status("Team DNA draft saved locally · no game writes");
                  })
                }
              >
                Save team draft
              </button>
            </div>
          </>
        )}
      </div>
      {review && (
        <div className="modal-backdrop">
          <section className="modal dna-import-review">
            <header>
              <h2>Import Team DNA · {review.name}</h2>
              <button disabled={busy} onClick={() => setReview(null)}>
                Cancel
              </button>
            </header>
            <div className="modal-body">
              <p>
                Select categories to stage. Unselected categories and the
                destination draft name stay unchanged.
              </p>
              <div className="dna-sections">
                {schema?.sections.map((s) => (
                  <label key={s}>
                    <input
                      type="checkbox"
                      disabled={busy}
                      checked={sections.includes(s)}
                      onChange={(e) =>
                        setSections(
                          e.target.checked
                            ? [...sections, s]
                            : sections.filter((k) => k !== s),
                        )
                      }
                    />
                    {labels[s]}
                  </label>
                ))}
              </div>
              {error && <p role="alert">{error}</p>}
            </div>
            <footer>
              <button
                className="primary"
                disabled={busy || !sections.length}
                onClick={() =>
                  void run(async () => {
                    if (!draft) return;
                    change(
                      await call<Team>("team-dna-stage", {
                        destination: draft,
                        source: review,
                        sections,
                      }),
                    );
                    setReview(null);
                    setSlot(0);
                    setUniform(0);
                    status(
                      "Team DNA staged locally · destination identity retained · no game writes",
                    );
                  })
                }
              >
                Stage team categories
              </button>
            </footer>
          </section>
        </div>
      )}
      {fresh && (
        <div className="modal-backdrop">
          <section className="modal">
            <header>
              <h2>Discard local team edits?</h2>
            </header>
            <div className="modal-body">Saved data stays unchanged.</div>
            <footer>
              <button onClick={() => setFresh(false)}>Keep editing</button>
              <button
                onClick={() => {
                  change(blank());
                  setFresh(false);
                }}
              >
                Discard and create team
              </button>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}
