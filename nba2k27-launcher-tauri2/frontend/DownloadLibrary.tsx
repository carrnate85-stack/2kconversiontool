/// <reference types="vite/client" />
import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import "./download-library.css";
type Item = {
  id: string;
  kind: string;
  name: string;
  description: string;
  artwork: string;
  order: number;
  bytes: number;
  supported: boolean;
  reason: string;
  installed: boolean;
  url?: string;
};
type Snapshot = {
  curator: boolean;
  items: Item[];
  status: {
    busy: boolean;
    id: string;
    phase: string;
    message: string;
    done: number;
    total: number;
    error?: string;
    warning?: string;
  };
};
type Editorial = {
  id: string;
  title: string;
  description: string;
  artwork: string;
  order: number;
  hidden: boolean;
  download?: { url: string; filename: string; bytes: number; sha256: string };
};
type Catalog = { schema: number; items: Editorial[] };
const collectionNames: Record<string,string> = {videos:"Startup videos",playlists:"Playlists",previews:"Blacktop Previews",cameras:"Cameras"};
const inCollection = (item: Editorial, tab: string) => tab === "previews" ? item.id === "previews-blacktop" : item.id.startsWith(`${tab}-`);
const art = import.meta.glob("../download-catalog/art/*", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;
export function artworkUrl(value: string) {
  return value.startsWith("https://raw.githubusercontent.com/")
    ? value
    : art["../download-catalog/" + value] || "";
}
export function downloadPercent(done: number, total: number) {
  return total > 0
    ? Math.min(100, Math.max(0, Math.floor((done / total) * 100)))
    : 0;
}
export function mergeEditorial(catalog: Catalog, items: Item[]): Catalog {
  return {
    ...catalog,
    items: [
      ...catalog.items,
      ...items
        .filter((e) => !catalog.items.some((i) => i.id === e.id))
        .map((e) => ({
          id: e.id,
          title: e.name,
          description: e.description,
          artwork: e.artwork,
          order: e.order,
          hidden: false,
        })),
    ],
  };
}
export function DownloadLibrary({initialTab = "playlists"}: {
  navigate: (page: string) => void;
  initialTab?: "playlists" | "videos" | "previews";
}) {
  const [view, setView] = useState<Snapshot | null>(null),
    [tab, setTab] = useState<string>(initialTab),
    [query, setQuery] = useState(""),
    [error, setError] = useState(""),
    [working, setWorking] = useState(true),
    [notice, setNotice] = useState("");
  const [catalog, setCatalog] = useState<Catalog | null>(null),
    [sha, setSha] = useState<string | null>(null),
    [editing, setEditing] = useState("");
  const [links, setLinks] = useState<Record<string,string>>({});
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    let disposed = false;
    let firstStatus = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const s = await call<Snapshot>("curated:status");
        if (disposed) return;
        setView(s);
        if (firstStatus) {
          firstStatus = false;
          if (!s.items.length && !s.status.busy) void act("curated:refresh");
          else setWorking(false);
        }
      } catch (e) {
        if (!disposed) { setError(String(e)); setWorking(false); firstStatus = false; }
      } finally {
        if (!disposed) timer = setTimeout(poll, 700);
      }
    }
    void poll();
    return () => {
      alive.current = false;
      disposed = true;
      clearTimeout(timer);
    };
  }, []);
  async function act(channel: string, args: unknown = null) {
    setWorking(true);
    setError("");
    setNotice("");
    try {
      const s = await call<Snapshot>(channel, args);
      if (alive.current) setView(s);
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setWorking(false);
    }
  }
  async function editor() {
    setWorking(true);
    setError("");
    try {
      const r = await call<{ catalog: Catalog; sha: string | null }>(
        "curated:editor",
      );
      const merged = mergeEditorial(r.catalog, view?.items || []);
      setCatalog(merged);
      setLinks({});
      setSha(r.sha);
      setEditing(merged.items.find(i => inCollection(i,tab))?.id || "");
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  }
  async function publish() {
    if (!catalog) return;
    setWorking(true);
    setError("");
    try {
      const r = await call<{ sha: string }>("curated:publish", {
        catalog,
        sha,
      });
      setSha(r.sha);
      setNotice(
        "Published to GitHub. Refresh the library to load the published catalog.",
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  }
  const curatorItems = catalog?.items.filter(i => inCollection(i,tab)) || [];
  const chosen = curatorItems.find(i => i.id === editing) || curatorItems[0];
  const linkFor = (i: Editorial) => links[i.id] ?? i.download?.url ?? view?.items.find(e => e.id === i.id)?.url ?? "";
  const uncheckedLinks = !!catalog?.items.some(i => links[i.id] !== undefined && links[i.id] !== i.download?.url);
  async function checkLink() {
    if (!chosen) return;
    const id=chosen.id, url=linkFor(chosen);
    setWorking(true);setError("");setNotice("");
    try {
      const download=await call<NonNullable<Editorial["download"]>>("curated:check-link",{id,url});
      setCatalog(c => c ? {...c,items:c.items.map(i => i.id===id ? {...i,download} : i)} : c);
      setLinks(v => ({...v,[id]:download.url}));
      setNotice(`Link checked: ${download.filename} · ${(download.bytes/1048576).toFixed(1)} MB. Publish to update the shared catalog.`);
    } catch(e) {setError(String(e));} finally {setWorking(false);}
  }
  function edit(patch: Partial<Editorial>) {
    setCatalog((c) =>
      c
        ? {
            ...c,
            items: c.items.map((i) =>
              i.id === chosen?.id ? { ...i, ...patch } : i,
            ),
          }
        : c,
    );
  }
  async function uploadArt() {
    setWorking(true);
    setError("");
    try {
      const r = await call<{ url: string } | null>("curated:artwork");
      if (r) {
        edit({ artwork: r.url });
        setNotice(
          "Cover uploaded to GitHub. Publish the catalog to use it for everyone.",
        );
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setWorking(false);
    }
  }
  const busy = working || !!view?.status.busy;
  const items = (view?.items || []).filter(
    (i) =>
      i.kind === tab &&
      `${i.name} ${i.description}`.toLowerCase().includes(query.toLowerCase()),
  );
  const status = view?.status;
  return (
    <section className="download-library">
      <div className="dl-toolbar">
        <div className="dl-tabs">
          <button
            disabled={busy}
            className={tab === "playlists" ? "selected" : ""}
            onClick={() => setTab("playlists")}
          >
            Playlists
          </button>
          <button
            disabled={busy}
            className={tab === "videos" ? "selected" : ""}
            onClick={() => setTab("videos")}
          >
            Startup videos
          </button>
          <button disabled={busy} className={tab === "previews" ? "selected" : ""} onClick={() => setTab("previews")}>Blacktop Previews</button>
          <button disabled title="No camera downloads available yet" aria-label="Cameras — no downloads available yet">Cameras</button>
        </div>
        <div className="dl-actions">
        <button title="Check for new downloads and catalog updates" disabled={busy} onClick={() => void act("curated:refresh")}>
          ↻ Refresh
        </button>
        {view?.curator && (
          <button
            disabled={busy}
            onClick={() => (catalog ? setCatalog(null) : void editor())}
          >
            {catalog ? "Close curator" : "Curator"}
          </button>
        )}
        </div>
      </div>
      <p className="dl-muted">
        Choose optional content for your launcher. Downloads are added to your
        library; video and playlist selections stay the same. Preview packs become available after import.
      </p>
      <input
        aria-label="Search downloads"
        className="dl-search"
        placeholder="Search the collection…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {(error || status?.error) && (
        <div className="dl-error" role="alert">
          {error || status?.error}
        </div>
      )}
      {notice && (
        <div className="dl-notice" role="status">
          {notice}
        </div>
      )}
      {working && !status?.busy && !view?.items.length && <p className="dl-muted" role="status">Loading collection…</p>}
      {status?.warning && <p className="dl-muted">{status.warning}</p>}
      {status?.busy && (
        <div className="dl-progress" role="status">
          <div>
            <strong>
              {status.phase === "refreshing"
                ? "Refreshing collection"
                : status.message || "Preparing download"}
            </strong>
            <button
              onClick={() => void act("curated:cancel", { id: status.id })}
            >
              Cancel
            </button>
          </div>
          <progress
            max={100}
            value={
              status.total
                ? downloadPercent(status.done, status.total)
                : undefined
            }
          />
          {status.total > 0 && status.phase === "downloading" && (
            <small>
              {(status.done / 1048576).toFixed(1)} /{" "}
              {(status.total / 1048576).toFixed(1)} MB
            </small>
          )}
        </div>
      )}
      {catalog && (
        <section className="dl-curator">
          <h3>{collectionNames[tab]} curator · dev only</h3>
          <p>
            Preview changes here, then publish titles, descriptions, order, and
            art and download links to GitHub. Video and playlist files stay in your MediaFire
            folders.
          </p>
          <select
            aria-label="Choose catalog item"
            value={chosen?.id || ""}
            disabled={busy || !curatorItems.length}
            onChange={(e) => setEditing(e.target.value)}
          >
            {!curatorItems.length && <option value="">No items in this collection</option>}
            {curatorItems.map((i) => (
              <option key={i.id} value={i.id}>
                {i.title}
              </option>
            ))}
          </select>
          {chosen && (
            <div className="dl-editor">
              <label>
                Display name
                <input
                  value={chosen.title}
                  maxLength={120}
                  onChange={(e) => edit({ title: e.target.value })}
                />
              </label>
              <label>
                Description
                <textarea
                  value={chosen.description}
                  maxLength={4000}
                  onChange={(e) => edit({ description: e.target.value })}
                />
              </label>
              <label>
                MediaFire download link
                <input type="url" disabled={busy} value={linkFor(chosen)} maxLength={2048}
                  placeholder="https://www.mediafire.com/file/…"
                  onChange={e => setLinks(v => ({...v,[chosen.id]:e.target.value.trim()}))}/>
              </label>
              <button disabled={busy || !linkFor(chosen)} onClick={() => void checkLink()}>Check download link</button>
              <small>Use a public MP4, .2kplaylist, or preview ZIP. Check a changed link before publishing. Downloads verify its checksum before import.</small>
              <label>
                Cover artwork
                <input
                  value={chosen.artwork}
                  onChange={(e) => edit({ artwork: e.target.value })}
                />
              </label>
              <button disabled={busy} onClick={() => void uploadArt()}>
                Upload cover to GitHub…
              </button>
              <label>
                Display order
                <input
                  type="number"
                  value={chosen.order}
                  onChange={(e) => edit({ order: Number(e.target.value) })}
                />
              </label>
              <label className="dl-check">
                <input
                  type="checkbox"
                  checked={chosen.hidden}
                  onChange={(e) => edit({ hidden: e.target.checked })}
                />
                Hide from the public collection
              </label>
            </div>
          )}
          <div className="dl-toolbar">
            <button
              disabled={busy || uncheckedLinks}
              onClick={() => void act("curated:preview", { catalog })}
            >
              Preview locally
            </button>
            <button
              className="dl-primary"
              disabled={busy || uncheckedLinks}
              onClick={() => void publish()}
            >
              Publish to GitHub
            </button>
          </div>
          <small>
            Publishes to carrnate85-stack/2kconversiontool · launcher-downloads.
            Uses your local GitHub CLI sign-in. Reload the curator if the
            catalog was edited elsewhere.
          </small>
        </section>
      )}
      {!view?.items.length && !busy && (
        <div className="dl-empty">
          <h3>Your curated collection</h3>
          <p>Refresh to check the current videos and playlists.</p>
          <button
            className="dl-primary"
            onClick={() => void act("curated:refresh")}
          >
            Browse downloads
          </button>
        </div>
      )}
      <div className="dl-grid">
        {items.map((i) => (
          <article className="dl-card" key={i.id}>
            <div className="dl-art">
              {artworkUrl(i.artwork) ? (
                <img
                  src={artworkUrl(i.artwork)}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  referrerPolicy="no-referrer"
                  onLoad={(e) => { e.currentTarget.style.display = "block"; }}
                  onError={(e) => {
                    e.currentTarget.style.display = "none";
                  }}
                />
              ) : (
                <span>{i.kind === "videos" ? "▶" : "♫"}</span>
              )}
            </div>
            <div className="dl-copy">
              <h3>{i.name}</h3>
              <p>
                {i.description ||
                  (i.kind === "videos" ? "Startup video" : i.kind === "previews" ? "Blacktop catalog previews" : "Jukebox playlist")}
              </p>
              <small>
                {(i.bytes / 1048576).toFixed(1)} MB ·{" "}
                {i.kind === "videos" ? "MP4" : i.kind === "previews" ? "Preview pack · ZIP" : "2K playlist"}
              </small>
              {!i.supported && <p className="dl-muted">{i.reason}</p>}
              <button
                className={i.installed ? "" : "dl-primary"}
                disabled={busy || !i.supported || i.installed}
                onClick={() => void act("curated:download", { id: i.id })}
              >
                {i.installed
                  ? "✓ Added to library"
                  : status?.busy && status.id === i.id
                    ? "Adding…"
                    : i.supported
                      ? (i.kind === "previews" ? "↓ Download & Import" : "↓ Download & Add")
                      : "MP4 needed"}
              </button>
            </div>
          </article>
        ))}
      </div>
      {!!view?.items.length && !items.length && (
        <p className="dl-empty">No matching downloads in this collection.</p>
      )}
    </section>
  );
}
