import { useEffect, useRef, useState } from "react";
import { call } from "./api";
import type { Mod } from "./types";
import { ModalDialog } from "./wpf/WpfShell";

type PlayerPreview = {
  defaultConfig?: string;
  selectedConfig?: string;
  configurations?: string[];
};
type PreviewTexture = { name?: string };
export type PreviewPick = {
  name: string;
  target: string;
  model: string;
  primitive: string;
  material?: {
    name: string;
    shader: string;
    source: string;
    textures: { name: string; channel: string; applied: boolean; status: string }[];
  } | null;
};
type LoadReply = {
  session: string;
  rendererRevision?: string;
  payload: {
    meshes?: unknown[];
    textures?: PreviewTexture[];
    skipped?: { name?: string; reason?: string }[];
    arena?: boolean;
    materialTextureTotal?: number;
    player?: PlayerPreview;
    [key: string]: unknown;
  };
};
type ArenaBatch = {
  textures: unknown[];
  end: number;
  total: number;
  bytes: number;
  skipped?: unknown[];
};
type ArenaState = {
  cursor: number;
  total: number;
  bytes: number;
  loaded: number;
  busy: boolean;
  budget: boolean;
};
type Pending = { resolve: () => void; reject: (error: Error) => void };
type PreviewStatus = { rendererRevision?: string; progress?: { phase?: string; percent?: number } | null };

const EMPTY_ARENA: ArenaState = { cursor: 0, total: 0, bytes: 0, loaded: 0, busy: false, budget: false };
const ARENA_STAGED_MEMORY_LIMIT = 128 * 1024 * 1024;

export function arenaTextureLimitReached(loadAll: boolean, loadedBytes: number, batchBytes: number) {
  return !loadAll && loadedBytes + batchBytes > ARENA_STAGED_MEMORY_LIMIT;
}

export function PreviewObjectInspector({ pick }: { pick: PreviewPick | null }) {
  return (
    <aside className={`model-preview-object-panel${pick ? " has-selection" : ""}`} aria-live="polite">
      <p className="eyebrow">Arena object inspector</p>
      {!pick ? (
        <span>Click arena geometry to inspect its SCNE object, target model, primitive, material, and textures.</span>
      ) : (
        <>
          <span>SCNE object</span><strong title={pick.name}>{pick.name}</strong>
          <span>Target model</span><code title={pick.target}>{pick.target}</code>
          <span>Geometry model</span><code title={pick.model}>{pick.model}</code>
          <span>Primitive</span><code title={pick.primitive}>{pick.primitive || "Not described"}</code>
          <span>Material</span><strong>{pick.material?.name || "Unassigned"}</strong>
          <span>Shader</span><code>{pick.material?.shader || "Not described"}</code>
          {!!pick.material?.textures?.length && (
            <div className="model-preview-dependencies">
              <b>Texture dependencies</b>
              {pick.material.textures.map((item, index) => (
                <div key={`${item.name}-${index}`}>
                  <strong>{item.name}</strong>
                  <small>{item.channel} · {item.status}{item.applied ? " · applied" : ""}</small>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </aside>
  );
}

export function ModelPreviewDialog({ mod, title, onClose }: { mod: Mod; title: string; onClose: () => void }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const ready = useRef(false);
  const alive = useRef(true);
  const payload = useRef<LoadReply | null>(null);
  const preserveCamera = useRef(false);
  const sequence = useRef(0);
  const pending = useRef(new Map<number, Pending>());
  const arenaRef = useRef<ArenaState>(EMPTY_ARENA);
  const [arena, setArenaState] = useState<ArenaState>(EMPTY_ARENA);
  const [player, setPlayer] = useState<PlayerPreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState("Reading the selected mod…");
  const [detail, setDetail] = useState("Decoding IFF and shared dependencies. Mod and game files remain unchanged.");
  const [error, setError] = useState("");
  const [details, setDetails] = useState("");
  const [texture, setTexture] = useState("");
  const [wireframe, setWireframe] = useState(false);
  const [lashes, setLashes] = useState(false);
  const [pick, setPick] = useState<PreviewPick | null>(null);

  const setArena = (next: ArenaState) => {
    arenaRef.current = next;
    setArenaState(next);
  };
  const postLoad = () => {
    if (!ready.current || !payload.current || !frame.current?.contentWindow) return;
    frame.current.contentWindow.postMessage(
      {
        type: "model-preview:load",
        requestId: payload.current.session,
        payload: payload.current.payload,
        preserveCamera: preserveCamera.current,
      },
      location.origin,
    );
    setStatus("Building interactive 3D preview…");
  };
  const postArenaTextures = (textures: unknown[], append: boolean) =>
    new Promise<void>((resolve, reject) => {
      const requestId = ++sequence.current;
      pending.current.set(requestId, { resolve, reject });
      frame.current?.contentWindow?.postMessage(
        { type: "model-preview:arena-textures", requestId, textures, append },
        location.origin,
      );
    });
  const command = (name: string, value?: unknown) => {
    frame.current?.contentWindow?.postMessage({ type: "model-preview:command", command: name, value }, location.origin);
  };

  async function requestModel(config?: string, keepCamera = false) {
    setLoading(true);
    setError("");
    setStatus(config === undefined ? "Reading the selected mod…" : "Loading player configuration…");
    setDetail(config === undefined
      ? "Decoding IFF and shared dependencies. Mod and game files remain unchanged."
      : "Loading the player configuration, hair, and facial-hair assets.");
    const args: { path: string; config?: string } = { path: mod.path };
    if (config !== undefined) args.config = config;
    const reply = await call<LoadReply>("model-preview-load", args);
    if (!alive.current) return;
    if (!reply.payload.meshes?.length)
      throw new Error("This IFF does not contain supported triangle meshes to preview.");
    payload.current = reply;
    preserveCamera.current = keepCamera;
    setPlayer(reply.payload.player ?? null);
    setTexture("");
    setWireframe(false);
    setLashes(false);
    setPick(null);
    setArena({ ...EMPTY_ARENA, total: reply.payload.materialTextureTotal ?? 0 });
    postLoad();
  }

  async function loadArena(allBatches: boolean) {
    const current = payload.current;
    if (!current?.payload.arena || arenaRef.current.busy) return;
    setLoading(true);
    setDetail("Completed texture batches remain loaded. Geometry, mod files, and exports are unchanged.");
    let next = { ...arenaRef.current, busy: true };
    setArena(next);
    try {
      do {
        if (!alive.current || payload.current?.session !== current.session) return;
        setStatus(allBatches ? `Loading all arena textures ${next.cursor}/${next.total}…` : `Loading arena textures ${next.cursor}/${next.total}…`);
        const batch = await call<ArenaBatch>("model-preview-arena-textures", { session: current.session, cursor: next.cursor });
        if (batch.end < next.cursor || batch.end > batch.total || (batch.end === next.cursor && batch.end < batch.total))
          throw new Error("The arena renderer returned an invalid texture batch.");
        if (arenaTextureLimitReached(allBatches, next.bytes, batch.bytes)) {
          next = { ...next, total: batch.total, busy: false, budget: true };
          setArena(next);
          setStatus(`Preview ready · ${next.loaded}/${batch.total} linked arena textures loaded (128 MiB test limit)`);
          return;
        }
        if (batch.textures.length) await postArenaTextures(batch.textures, next.cursor > 0);
        next = {
          cursor: batch.end,
          total: batch.total,
          bytes: next.bytes + batch.bytes,
          loaded: next.loaded + batch.textures.length,
          busy: true,
          budget: false,
        };
        setArena(next);
      } while (allBatches && next.cursor < next.total);
      next = { ...next, busy: false };
      setArena(next);
      setStatus(`Preview ready · ${next.loaded}/${next.total} linked arena textures loaded`);
    } catch (reason) {
      setArena({ ...next, busy: false });
      throw reason;
    } finally {
      if (alive.current) setLoading(false);
    }
  }

  function changePlayerConfig(config: string) {
    void requestModel(config, true).catch((reason) => {
      if (!alive.current) return;
      setLoading(false);
      setError(String(reason?.message ?? reason).replace(/^Error: /, ""));
    });
  }

  useEffect(() => {
    alive.current = true;
    function receive(event: MessageEvent) {
      if (event.source !== frame.current?.contentWindow || event.origin !== location.origin) return;
      const message = event.data;
      if (message?.type === "model-preview:ready") {
        ready.current = true;
        postLoad();
      } else if (message?.type === "model-preview:displayed" && message.requestId === payload.current?.session) {
        const current = payload.current;
        if (!current) return;
        preserveCamera.current = false;
        setLoading(false);
        const meshes = current.payload.meshes?.length ?? 0;
        const skipped = current.payload.skipped?.length ?? 0;
        const revision = current.rendererRevision?.slice(0, 12);
        setDetails(`${meshes.toLocaleString()} mesh${meshes === 1 ? "" : "es"}${skipped ? ` · ${skipped} unsupported item${skipped === 1 ? "" : "s"}` : ""}${revision ? ` · shared ${revision}` : ""}`);
        if (current.payload.arena) {
          const total = current.payload.materialTextureTotal ?? 0;
          setArena({ ...EMPTY_ARENA, total });
          setStatus(total ? `Preview ready · geometry loaded · ${total} linked textures available` : "Preview ready · geometry loaded");
        } else if (current.payload.player?.selectedConfig) {
          setStatus(`Preview ready · ${current.payload.player.selectedConfig.replaceAll("_", " ")} player configuration loaded`);
        } else if (current.payload.player?.configurations?.length) {
          setStatus("Preview ready · enable Configs to load hair and facial-hair geometry");
        } else setStatus("Preview ready");
      } else if (message?.type === "model-preview:progress" && message.requestId === payload.current?.session) {
        setStatus(`${message.phase} ${message.completed}/${message.total}…`);
        setDetail("The 0.1.27 viewer stays responsive while the scene is assembled.");
      } else if (message?.type === "model-preview:arena-progress") {
        setStatus(`Uploading textures ${message.completed}/${message.total}…`);
        setDetail("Completed texture batches remain loaded. Geometry, mod files, and exports are unchanged.");
      } else if (message?.type === "model-preview:pick") {
        setPick(message.detail ?? null);
      } else if (message?.type === "model-preview:arena-textures-ready") {
        const item = pending.current.get(message.requestId);
        pending.current.delete(message.requestId);
        item?.resolve();
      } else if (message?.type === "model-preview:error") {
        const item = pending.current.get(message.requestId);
        pending.current.delete(message.requestId);
        if (item) item.reject(new Error(message.message));
        else {
          setLoading(false);
          setError(message.message || "The renderer could not display this model.");
        }
      }
    }
    addEventListener("message", receive);
    void requestModel().catch((reason) => {
      if (!alive.current) return;
      setLoading(false);
      setError(String(reason?.message ?? reason).replace(/^Error: /, ""));
    });
    return () => {
      alive.current = false;
      removeEventListener("message", receive);
      frame.current?.contentWindow?.postMessage({ type: "model-preview:clear" }, location.origin);
      for (const item of pending.current.values()) item.reject(new Error("Preview closed."));
      pending.current.clear();
      void call("model-preview-cancel").catch(() => undefined);
    };
  }, [mod.path]);

  useEffect(() => {
    if (!loading) return;
    let stopped = false;
    const refresh = async () => {
      try {
        const value = await call<PreviewStatus>("model-preview-status");
        const progress = value.progress;
        if (!stopped && progress?.phase) {
          setStatus(`${progress.phase}${typeof progress.percent === "number" ? ` · ${progress.percent}%` : ""}`);
          setDetail("Native Exporter 0.1.27 preview work. Mod and game files remain unchanged.");
        }
      } catch { /* The visible load call owns any actionable error. */ }
    };
    void refresh();
    const timer = window.setInterval(refresh, 120);
    return () => { stopped = true; window.clearInterval(timer); };
  }, [loading]);

  const configurations = player?.configurations ?? [];
  const selectedConfig = player?.selectedConfig ?? "";
  const arenaDone = arena.cursor >= arena.total;

  return (
    <ModalDialog className="model-preview-dialog" labelledBy="modelPreviewTitle" onCancel={onClose}>
      <header className="model-preview-header">
        <div className="model-preview-identity">
          <p className="eyebrow">2K EXPLORER RENDERER · TEST BUILD</p>
          <h2 id="modelPreviewTitle">{title}</h2>
          <small className="mono" title={mod.path}>{mod.path.replaceAll("\\", "/")}</small>
        </div>
        <div className="model-preview-actions">
          <label className="model-preview-texture-control">
            <span>Texture</span>
            <select
              aria-label="Preview texture"
              disabled={loading}
              value={texture}
              onChange={(event) => {
                setTexture(event.target.value);
                command("setTexture", event.target.value);
              }}
            >
              <option value="">Automatic material textures</option>
              <option value="-1">No color texture</option>
              {payload.current?.payload.textures?.map((item, index) => <option key={`${item.name ?? "texture"}-${index}`} value={index}>{item.name ?? `Texture ${index + 1}`}</option>)}
            </select>
          </label>
          <button type="button" className="secondary" disabled={loading} aria-pressed={wireframe} onClick={() => {
            const next = !wireframe;
            setWireframe(next);
            command("setWireframe", next);
          }}>Wireframe</button>
          {payload.current?.payload.arena && <button type="button" className="secondary" disabled={loading} onClick={() => command("setArenaCourtView")}>Court view</button>}
          <button type="button" className="secondary" disabled={loading} onClick={() => command("fitCamera")}>Fit scene</button>
        </div>
        <span className="model-preview-dev-badge">TEST BUILD</span>
        <button type="button" className="secondary" onClick={onClose}>Close</button>
      </header>
      <div className="model-preview-stage" data-error={error ? "true" : "false"}>
        <iframe ref={frame} title={`3D preview of ${title}`} src="/model-preview-dev/" />
        {player && (
          <div className="model-preview-player-controls">
            <label className="model-preview-overlay-toggle">
              <input
                type="checkbox"
                disabled={loading}
                checked={lashes}
                onChange={(event) => {
                  setLashes(event.target.checked);
                  command("setEyelashesVisible", event.target.checked);
                }}
              />
              Eyelashes
            </label>
            <label className="model-preview-overlay-toggle">
              <input
                aria-label="Load player configuration"
                type="checkbox"
                disabled={loading || configurations.length === 0}
                checked={!!selectedConfig}
                onChange={(event) => changePlayerConfig(event.target.checked ? (player.defaultConfig || configurations[0] || "") : "")}
              />
              Configs
            </label>
            {!!selectedConfig && (
              <label className="model-preview-config-field">
                <span>Player configuration</span>
                <select
                  className="model-preview-config-select"
                  aria-label="Player configuration"
                  disabled={loading}
                  value={selectedConfig}
                  onChange={(event) => changePlayerConfig(event.target.value)}
                >
                  {configurations.map((name) => <option key={name} value={name}>{name.replaceAll("_", " ")}</option>)}
                </select>
              </label>
            )}
          </div>
        )}
        {loading && !error && (
          <div className="model-preview-loading" role="status" aria-live="polite" aria-busy="true">
            <i aria-hidden="true" />
            <strong>{status}</strong>
            <span>{detail}</span>
            <button type="button" className="secondary model-preview-loading-cancel" onClick={onClose}>Cancel preview</button>
          </div>
        )}
        {error && <div className="model-preview-error"><strong>3D preview unavailable</strong><span>{error}</span></div>}
        {payload.current?.payload.arena && <PreviewObjectInspector pick={pick} />}
      </div>
      <footer className="model-preview-footer">
        <div className="model-preview-footer-copy">
          <span role="status">{error || status}</span>
          <span>{details || "Read-only preview · mod and game files stay unchanged"}</span>
        </div>
        <div className="model-preview-controls">
          {payload.current?.payload.arena && (
            <>
              <span className="model-preview-progress">{arena.loaded} DDS · {arena.cursor}/{arena.total}</span>
              <button type="button" className="secondary" disabled={loading || arena.busy || arena.budget || arenaDone} onClick={() => void loadArena(false).catch((reason) => alive.current && setError(String(reason?.message ?? reason)))}>Load more textures</button>
              <button type="button" className="secondary" disabled={loading || arena.busy || arenaDone} onClick={() => void loadArena(true).catch((reason) => alive.current && setError(String(reason?.message ?? reason)))}>Load all textures</button>
            </>
          )}
        </div>
      </footer>
    </ModalDialog>
  );
}
