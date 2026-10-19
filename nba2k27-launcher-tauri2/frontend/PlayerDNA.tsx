import React, { useEffect, useRef, useState } from "react";
import { call } from "./api";
import {
  SeasonHistory,
  type SeasonRow,
  type SeasonSchema,
} from "./SeasonHistory";
import { SeasonImport } from "./StatsImport";
import { playerEditorLayout } from "./player-web-presentation";
import { PlayerFieldNumber, type PlayerField } from "./PlayerFieldNumber";
import { PlayerMeasurements } from "./PlayerMeasurements";
import { assertDraftInputs } from "./editor-validation";
import { PlayerCatalogImport } from "./PlayerCatalogImport";
export type DNA = {
  format: "2kplayer";
  version: 1 | 2;
  game: "NBA2K27";
  schemaId: string;
  name: string;
  info: Record<string, string | number>;
  fields: Record<string, number>;
  seasonHistory?: SeasonRow[];
};
type Schema = SeasonSchema & {
  schemaId: string;
  groups: string[];
  fields: PlayerField[];
};
const groupNames: Record<string, string> = {
  info: "Player info",
  appearance: "Appearance",
  audio: "Audio",
  attributes: "Attributes",
  development: "Development",
  tendencies: "Tendencies",
  playtypes: "Play types",
  personality: "Personality",
  badges: "Badges",
  hotzones: "Hot zones",
  signature: "Signature",
  equipment: "Equipment",
  contract: "Contract",
  stats: "Stats",
  seasons: "Season history",
  draft: "Draft",
  injuries: "Injuries",
};
const infoGroups: Record<string, string> = {
  cyberfaceId: "appearance",
  weightLbs: "appearance",
  heightCm: "appearance",
  wingspanCm: "appearance",
  fullNameId: "audio",
  potential: "attributes",
};
const infoLabels: Record<string, string> = {
  firstName: "First name",
  lastName: "Last name",
  fromName: "From",
  birthDate: "Birthday",
  yearsPro: "Years pro",
  positionId: "Position",
  secondaryPositionId: "Secondary position",
  jerseyNumberRaw: "Jersey number",
  potential: "Potential",
  cyberfaceId: "Face ID",
  heightCm: "Height (cm)",
  wingspanCm: "Wingspan (cm)",
  weightLbs: "Weight (lb)",
  fullNameId: "Full-name audio ID",
};
let session: { draft: DNA | null; saved: DNA | null } | null = null;
export function PlayerDNA({ status }: { status: (s: string) => void }) {
  const [schema, setSchema] = useState<Schema | null>(null),
    [draft, updateDraft] = useState<DNA | null>(session?.draft ?? null),
    [saved, setSaved] = useState<DNA | null>(session?.saved ?? null),
    [tab, setTab] = useState("info"),
    [filter, setFilter] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [review, setReview] = useState<DNA | null>(null),
    [groups, setGroups] = useState<string[]>([]),
    [fresh, setFresh] = useState(false);
  const [statsImport, setStatsImport] = useState(false);
  const [catalogImport, setCatalogImport] = useState(false);
  const [category, setCategory] = useState("All");
  const running = useRef(false);
  function setDraft(d: DNA | null) {
    session = { draft: d, saved };
    updateDraft(d);
  }
  function newDraft() {
    if (schema) {
      setDraft({
        format: "2kplayer",
        version: 1,
        game: "NBA2K27",
        schemaId: schema.schemaId,
        name: "New player",
        info: {},
        fields: {},
      });
      setTab("info");
    }
  }
  useEffect(() => {
    let alive = true;
    Promise.all([
      call<Schema>("player-dna-schema"),
      session
        ? Promise.resolve(session)
        : call<DNA | null>("player-dna").then((d) => ({ draft: d, saved: d })),
    ])
      .then(([s, entry]) => {
        if (alive) {
          setSchema(s);
          session = entry;
          updateDraft(entry.draft);
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
  async function stage() {
    if (!review) return;
    const selected = await call<DNA>("player-dna-subset", {
      document: review,
      groups,
    });
    const next: DNA = draft
      ? {
          ...draft,
          info: { ...draft.info, ...selected.info },
          fields: { ...draft.fields, ...selected.fields },
          ...(selected.seasonHistory
            ? { version: 2, seasonHistory: selected.seasonHistory }
            : {}),
        }
      : selected;
    setDraft(await call<DNA>("player-dna-validate", next));
    setReview(null);
    status("Player DNA staged in local editor · nothing written to game");
  }
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const tabs = [
    "info",
    "attributes",
    "tendencies",
    "badges",
    "hotzones",
    "signature",
    "contract",
    "equipment",
    "playtypes",
    "appearance",
    "development",
    "personality",
    "stats",
    "draft",
    "injuries",
    "audio",
    "seasons",
  ];
  const fields =
    schema?.fields
      .filter(
        (f) =>
          f.group === tab &&
          (category === "All" || playerEditorLayout.category(f) === category) &&
          `${f.label} ${f.key}`.toLowerCase().includes(filter.toLowerCase()),
      )
      .sort(
        (a, b) => playerEditorLayout.order(a) - playerEditorLayout.order(b),
      ) ?? [];
  const info = Object.keys(infoLabels).filter(
    (k) =>
      (infoGroups[k] ?? "info") === tab &&
      infoLabels[k].toLowerCase().includes(filter.toLowerCase()),
  );
  return (
    <section className="card player-dna-editor">
      <header>
        <h3>Player DNA · offline editor</h3>
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
              else newDraft();
            }}
          >
            New local player
          </button>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const d = await call<DNA | null>("import-player-dna");
                if (d) {
                  setReview(d);
                  setGroups(schema?.groups ?? []);
                }
              })
            }
          >
            Import .2kplayer
          </button>
          <button
            disabled={busy || !schema}
            onClick={() => setCatalogImport(true)}
          >
            Import from NBA 2K23
          </button>
          <button
            disabled={busy || !draft}
            onClick={() =>
              void run(async () => {
                const r = await call<{ path: string }>(
                  "export-player-dna",
                  draft,
                );
                status(`Player DNA exported: ${r.path}`);
              })
            }
          >
            Export .2kplayer
          </button>
          <button
            onClick={() =>
              void run(async () => {
                await call("open-library", { kind: "Players" });
              })
            }
          >
            Open player folder
          </button>
        </div>
        <p className="muted compact-note">
          Web-based local drafts with native schema validation. Live roster
          connection and game Save &amp; verify are still unported.
        </p>
        {error && (
          <div role="alert" className="notice">
            {error}
          </div>
        )}
        {draft && schema && (
          <>
            <div className="dna-name toolbar">
              <label>
                Player DNA name{" "}
                <input
                  aria-label="Player DNA name"
                  disabled={busy}
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                />
              </label>
            </div>
            <div className="dna-layout">
              <nav aria-label="Player DNA sections">
                {tabs.map((t) => (
                  <button
                    key={t}
                    className={t === tab ? "active" : ""}
                    onClick={() => {
                      setTab(t);
                      setFilter("");
                      setCategory("All");
                    }}
                  >
                    {groupNames[t] ?? t}
                  </button>
                ))}
              </nav>
              <div className="dna-fields">
                <input
                  className="wide-input"
                  aria-label="Filter player DNA fields"
                  placeholder="Search fields in this section"
                  value={filter}
                  onChange={(e) => {
                    setFilter(e.target.value);
                    setCategory("All");
                  }}
                />
                {!!playerEditorLayout.categories[tab] && (
                  <div
                    className="studio-category-tabs"
                    role="tablist"
                    aria-label={`${tab} categories`}
                  >
                    {["All", ...playerEditorLayout.categories[tab]].map((c) => (
                      <button
                        key={c}
                        type="button"
                        role="tab"
                        aria-selected={category === c}
                        onClick={() => {
                          setCategory(c);
                          setFilter("");
                        }}
                        onKeyDown={(e) => {
                          if (
                            ![
                              "ArrowRight",
                              "ArrowLeft",
                              "Home",
                              "End",
                            ].includes(e.key)
                          )
                            return;
                          e.preventDefault();
                          const buttons = [
                            ...e.currentTarget.parentElement!.querySelectorAll<HTMLButtonElement>(
                              "button",
                            ),
                          ];
                          const index = buttons.indexOf(e.currentTarget),
                            last = buttons.length - 1;
                          const next =
                            e.key === "Home"
                              ? 0
                              : e.key === "End"
                                ? last
                                : (index +
                                    (e.key === "ArrowRight" ? 1 : last)) %
                                  buttons.length;
                          buttons[next].click();
                          buttons[next].focus();
                        }}
                      >
                        {c}
                      </button>
                    ))}
                  </div>
                )}
                <div className="dna-field-grid">
                  {info.map((k) =>
                    k === "heightCm" || k === "wingspanCm" ? (
                      <PlayerMeasurements
                        key={k}
                        kind={k}
                        raw={draft.info[k] as number | undefined}
                        busy={busy}
                        error={setError}
                        commit={(cm) => {
                          const next = { ...draft.info };
                          if (cm === undefined) delete next[k];
                          else next[k] = cm;
                          setDraft({ ...draft, info: next });
                        }}
                      />
                    ) : (
                      <label key={k}>
                        <span>{infoLabels[k]}</span>
                        <input
                          aria-label={`DNA ${infoLabels[k]}`}
                          disabled={busy}
                          type={
                            k === "birthDate"
                              ? "date"
                              : ["firstName", "lastName", "fromName"].includes(
                                    k,
                                  )
                                ? "text"
                                : "number"
                          }
                          value={draft.info[k] ?? ""}
                          placeholder="Not included"
                          onChange={(e) => {
                            const values = { ...draft.info };
                            if (e.target.value === "") delete values[k];
                            else
                              values[k] = [
                                "firstName",
                                "lastName",
                                "fromName",
                                "birthDate",
                              ].includes(k)
                                ? e.target.value
                                : Number(e.target.value);
                            setDraft({ ...draft, info: values });
                          }}
                        />
                      </label>
                    ),
                  )}
                  {fields.map((f) => (
                    <label key={f.key}>
                      <span title={f.key}>
                        {f.label}
                        {f.maximum < 0 ? " · read-only" : ""}
                      </span>
                      {f.choices.length && f.maximum >= 0 ? (
                        <select
                          aria-label={`DNA ${f.label}`}
                          disabled={busy}
                          value={draft.fields[f.key] ?? ""}
                          onChange={(e) => {
                            const values = { ...draft.fields };
                            if (e.target.value === "") delete values[f.key];
                            else values[f.key] = Number(e.target.value);
                            setDraft({ ...draft, fields: values });
                          }}
                        >
                          <option value="">Not included</option>
                          {f.choices.slice(0, f.maximum + 1).map((c, i) => (
                            <option key={i} value={i}>
                              {c}
                            </option>
                          ))}
                        </select>
                      ) : (
                        <PlayerFieldNumber
                          field={f}
                          raw={draft.fields[f.key]}
                          busy={busy}
                          error={setError}
                          commit={(value) => {
                            const values = { ...draft.fields };
                            if (value === undefined) delete values[f.key];
                            else values[f.key] = value;
                            setDraft({ ...draft, fields: values });
                          }}
                        />
                      )}
                    </label>
                  ))}
                </div>
                {fields.some((f) => f.group === "attributes") && (
                  <p className="muted">
                    Attributes use the WPF 25–99 display scale. Unchanged
                    imported raw values are preserved exactly, including values
                    above the editable display range. Salary fields use
                    millions.
                  </p>
                )}
                {tab === "seasons" && (
                  <SeasonHistory
                    rows={draft.seasonHistory ?? []}
                    schema={schema}
                    busy={busy}
                    change={(seasonHistory) =>
                      setDraft({ ...draft, version: 2, seasonHistory })
                    }
                    importRows={() => setStatsImport(true)}
                  />
                )}
              </div>
            </div>
            <div className="toolbar dna-save">
              <button
                disabled={busy || !dirty}
                onClick={() => {
                  setDraft(saved);
                  setError("");
                  status(
                    "Local Player DNA edits reset · no game refresh required",
                  );
                }}
              >
                Reset local edits
              </button>
              <button
                className="primary"
                disabled={busy || !dirty}
                onClick={() =>
                  void run(async () => {
                    assertDraftInputs(".player-dna-editor");
                    const d = await call<DNA>("save-player-dna", draft);
                    session = { draft: d, saved: d };
                    updateDraft(d);
                    setSaved(d);
                    status(
                      "Player DNA draft saved · no game writes",
                    );
                  })
                }
              >
                Save local draft
              </button>
            </div>
          </>
        )}
      </div>
      {statsImport && draft && (
        <SeasonImport
          busy={busy}
          onClose={() => setStatsImport(false)}
          stageReference={async (token, selected) => {
            if (running.current)
              throw Error("An editor operation is already running.");
            running.current = true;
            setBusy(true);
            try {
              const next = await call<DNA>("reference-stats-stage", {
                document: draft,
                token,
                selected,
              });
              setDraft(next);
              setStatsImport(false);
              setTab("seasons");
              setFilter("");
              setError("");
              status(
                `Staged ${selected.length} Basketball-Reference seasons · review and Save local draft · no game writes`,
              );
            } finally {
              running.current = false;
              setBusy(false);
            }
          }}
          stage={async (id, selected, awards) => {
            if (running.current)
              throw Error("An editor operation is already running.");
            running.current = true;
            setBusy(true);
            try {
              const next = await call<DNA>("player-dna-stage-seasons", {
                document: draft,
                id,
                selected,
                awards,
              });
              setDraft(next);
              setStatsImport(false);
              setTab("seasons");
              setFilter("");
              setError("");
              status(
                `Staged ${selected.length} seasons in Player DNA · review and Save local draft · no game writes`,
              );
            } finally {
              running.current = false;
              setBusy(false);
            }
          }}
        />
      )}
      {catalogImport && schema && (
        <PlayerCatalogImport
          fields={schema.fields}
          destination={draft?.name ?? "New local player"}
          busy={busy}
          onClose={() => setCatalogImport(false)}
          onStage={async (document, selectedGroups) => {
            if (running.current)
              throw Error("An editor operation is already running.");
            running.current = true;
            setBusy(true);
            try {
              const selected = await call<DNA>("player-dna-subset", {
                document,
                groups: selectedGroups,
              });
              const next = draft
                ? {
                    ...draft,
                    info: { ...draft.info, ...selected.info },
                    fields: { ...draft.fields, ...selected.fields },
                  }
                : selected;
              setDraft(await call<DNA>("player-dna-validate", next));
              setCatalogImport(false);
              setTab(selectedGroups[0] ?? "info");
              setCategory("All");
              setFilter("");
              setError("");
              status(
                `Staged ${selectedGroups.length} NBA 2K23 categories from ${document.name} · review and Save local draft · no game writes`,
              );
            } finally {
              running.current = false;
              setBusy(false);
            }
          }}
        />
      )}
      {review && (
        <div className="modal-backdrop">
          <section className="modal dna-import-review">
            <header>
              <h2>Import Player DNA · {review.name}</h2>
              <button disabled={busy} onClick={() => setReview(null)}>
                Cancel
              </button>
            </header>
            <div className="modal-body">
              <p>
                Choose which categories to stage. Existing local fields outside
                these categories stay unchanged.
              </p>
              <div className="dna-sections">
                {schema?.groups.map((g) => (
                  <label key={g}>
                    <input
                      type="checkbox"
                      disabled={busy}
                      checked={groups.includes(g)}
                      onChange={(e) =>
                        setGroups(
                          e.target.checked
                            ? [...groups, g]
                            : groups.filter((x) => x !== g),
                        )
                      }
                    />
                    {groupNames[g] ?? g}
                  </label>
                ))}
              </div>
              <p className="muted">
                {Object.keys(review.fields).length} mapped fields ·{" "}
                {review.seasonHistory?.length ?? 0} seasons. Nothing is written
                to the game.
              </p>
              {error && (
                <div role="alert" className="notice">
                  {error}
                </div>
              )}
            </div>
            <footer>
              <button
                className="primary"
                disabled={busy || !groups.length}
                onClick={() => void run(stage)}
              >
                Stage selected values
              </button>
            </footer>
          </section>
        </div>
      )}
      {fresh && (
        <div className="modal-backdrop">
          <section className="modal">
            <header>
              <h2>Discard local player edits?</h2>
            </header>
            <div className="modal-body">Saved data stays unchanged.</div>
            <footer>
              <button onClick={() => setFresh(false)}>Keep editing</button>
              <button
                onClick={() => {
                  newDraft();
                  setFresh(false);
                }}
              >
                Discard and create player
              </button>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}
