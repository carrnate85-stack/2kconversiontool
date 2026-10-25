import { useEffect, useState } from "react";
import { call } from "./api";

type Base = { id: string; name: string; file: string; floorFile: string };
type Sources = { files: string[]; bases: Base[]; modsRoot: string; levelsRoot: string };
type Entry = { id: string; name: string; baseId: string; arenaSource: string | null; floorSource: string | null };
export type SavedCustomArena = { id: string; name: string; previousId?: string | null };
export function suggestedCourt(files: string[], arena: string) {
  const stem = arena.replace(/\.iff$/i, "");
  const expected = [`${stem}_floor.iff`, `${stem.replace(/(^|\/)arena_/i, "$1floor_")}_floor.iff`].map(s => s.toLowerCase());
  return files.find(f => expected.includes(f.toLowerCase())) ?? "";
}
export function CustomBlacktopImport({ editId, onAdded, onCancel }: {
  editId?: string; onAdded: (entry: SavedCustomArena) => void; onCancel: () => void;
}) {
  const [sources, setSources] = useState<Sources | null>(null);
  const [name, setName] = useState(""), [arena, setArena] = useState(""), [court, setCourt] = useState("");
  const [base, setBase] = useState("blacktop-stock"), [filter, setFilter] = useState("");
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    Promise.all([call<Sources>("custom-blacktop", { action: "sources" }),
      editId ? call<Entry>("custom-blacktop", { action: "get", id: editId }) : Promise.resolve(null)]).then(([value, entry]) => {
      if (!active) return;
      setSources(value);
      setBase(entry?.baseId ?? value.bases.find(b => b.id === "blacktop-stock")?.id ?? value.bases[0]?.id ?? "");
      if (entry) { setName(entry.name); setArena(entry.arenaSource ?? ""); setCourt(entry.floorSource ?? ""); }
    }).catch(e => active && setError(String(e)));
    return () => { active = false; };
  }, [editId]);
  const matching = sources?.files.filter(f => f.toLowerCase().includes(filter.toLowerCase())) ?? [];
  const options = (selected: string) => [...new Set([...(selected ? [selected] : []), ...matching])];
  async function add() {
    if (busy || !name.trim() || (!arena && !court)) return;
    setBusy(true); setError("");
    try {
      const saved = await call<SavedCustomArena>("custom-blacktop", { action: editId ? "edit" : "add", ...(editId ? { id: editId } : {}), name: name.trim(), baseId: base,
        arenaSource: arena || null, floorSource: court || null });
      onAdded(saved);
    } catch (e) { setError(String(e)); setBusy(false); }
  }
  return <section className="custom-blacktop-import" aria-label={editId ? "Edit custom Blacktop arena" : "Import custom Blacktop arena"}>
    <h3>{editId ? "Edit custom arena" : "Add an arena or court from levels"}</h3>
    <p>Give it a name, choose your files, then select the saved card to use it in Blacktop.</p>
    {sources && <p className="custom-blacktop-path" title={sources.levelsRoot}>{sources.levelsRoot}</p>}
    <fieldset disabled={busy || !sources}>
      <label>Name<input value={name} maxLength={120} onChange={e => setName(e.target.value)} placeholder="My custom court" /></label>
      <label>Find mod files<input type="search" value={filter} onChange={e => setFilter(e.target.value)} placeholder="Search files in mods/levels" /></label>
      <label>Arena mod<select aria-label="Arena mod" value={arena} onChange={e => {
        const value = e.target.value; setArena(value);
        const matchingBase = sources?.bases.find(b => b.file.toLowerCase() === value.split("/").pop()?.replace(/\.iff$/i, "").toLowerCase());
        if (matchingBase) setBase(matchingBase.id);
        if (!court) setCourt(suggestedCourt(sources?.files ?? [], value));
        if (!name && value) setName(value.split("/").pop()!.replace(/\.iff$/i, ""));
      }}><option value="">Use the base arena</option>{options(arena).map(f => <option key={f} value={f}>{f}</option>)}</select></label>
      <label>Court / floor mod<select aria-label="Court / floor mod" value={court} onChange={e => setCourt(e.target.value)}>
        <option value="">Use the base court</option>{options(court).map(f => <option key={f} value={f}>{f}</option>)}
      </select></label>
      <label>Base arena<select aria-label="Base arena" value={base} onChange={e => setBase(e.target.value)}>
        {sources?.bases.map(b => <option key={b.id} value={b.id}>{b.name}</option>)}
      </select><small>Supplies the arena or court when you leave that mod field empty.</small></label>
    </fieldset>
    {!sources && !error && <p role="status">Reading levels folder…</p>}
    {sources?.files.length === 0 && <p>No IFF files found in levels. Place your arena or court mod in this folder first.</p>}
    {error && <p role="alert">{error}</p>}
    <div className="custom-blacktop-buttons">
      <button type="button" disabled={busy || !sources || !base || !name.trim() || (!arena && !court)} onClick={() => void add()}>{busy ? "Saving…" : editId ? "Save changes" : "Add to Custom"}</button>
      <button type="button" disabled={busy} onClick={onCancel}>Cancel</button>
    </div>
  </section>;
}
