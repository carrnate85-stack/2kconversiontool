import { UiIcon } from "./wpf/WpfShell";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import { call } from "./api";
import {
  JukeboxNativeClient,
  defaultJukeboxKeys,
  findJukeboxTracks,
  findMissingJukeboxArtwork,
  jukeboxJobActive,
  jukeboxKey,
  jukeboxProgress,
  jukeboxTitle,
  moveJukeboxTrack,
  releaseJukeboxArtwork,
  validJukeboxArtwork,
  validJukeboxPreview,
  type JukeboxArtwork,
  type JukeboxArtworkReply,
  type JukeboxPreview,
  type JukeboxRequest,
  type JukeboxTrack,
  type JukeboxView,
} from "./JukeboxClient";
import "./JukeboxWorkspace.css";

function Modal({
  title,
  children,
  close,
  locked = false,
}: {
  title: string;
  children: ReactNode;
  close: () => void;
  locked?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("input,button,select")?.focus();
    return () => {
      if (before?.isConnected) before.focus();
    };
  }, []);
  return (
    <div
      className="modal-backdrop juke-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !locked) close();
      }}
    >
      <div
        ref={ref}
        className="modal juke-modal"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onKeyDown={(e) => {
          if (e.key === "Escape" && !locked) {
            e.stopPropagation();
            close();
          }
          if (e.key !== "Tab") return;
          const items = [
            ...(ref.current?.querySelectorAll<HTMLElement>(
              'button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex="0"]',
            ) ?? []),
          ];
          if (!items.length) return;
          const first = items[0],
            last = items.at(-1)!;
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last.focus();
          }
          if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }}
      >
        <header>
          <h2>{title}</h2>
          <button
            aria-label={`Close ${title}`}
            disabled={locked}
            onClick={close}
          >
            ×
          </button>
        </header>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}

function Cover({
  track,
  revision,
  request,
  enabled,
}: {
  track: JukeboxTrack;
  revision: string;
  request: JukeboxRequest;
  enabled: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null),
    [visible, setVisible] = useState(false),
    [art, setArt] = useState<JukeboxArtwork | null>(null);
  const key = jukeboxKey(track);
  useEffect(() => {
    if (!enabled || !ref.current) return;
    if (typeof IntersectionObserver === "undefined") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => setVisible(entries.some((e) => e.isIntersecting)),
      { rootMargin: "60px" },
    );
    observer.observe(ref.current);
    return () => observer.disconnect();
  }, [enabled]);
  useEffect(() => {
    if (!enabled || !visible) return;
    let live = true,
      lease: JukeboxArtwork | null = null;
    void request<JukeboxArtworkReply>("jukebox:get-artwork", {
      trackKey: key,
      expectedRevision: revision,
    })
      .then(async (reply) => {
        lease = reply.artwork;
        if (!live) {
          await releaseJukeboxArtwork(request, lease);
          lease = null;
          return;
        }
        if (lease && validJukeboxArtwork(lease)) setArt(lease);
        else {
          await releaseJukeboxArtwork(request, lease);
          lease = null;
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
      setArt(null);
      void releaseJukeboxArtwork(request, lease);
      lease = null;
    };
  }, [key, revision, request, enabled, visible]);
  return (
    <div
      className={art ? "jukebox-cover" : "jukebox-art-placeholder"}
      ref={ref}
    >
      {art ? (
        <img
          className="jukebox-row-art"
          src={art.url}
          alt={`${jukeboxTitle(track)} artwork`}
          onError={() => setArt(null)}
        />
      ) : (
        <span data-jukebox-art aria-label="No saved artwork preview">
          ♫
        </span>
      )}
    </div>
  );
}

function SongEditor({
  track,
  view,
  request,
  client,
  close,
  saved,
}: {
  track: JukeboxTrack;
  view: JukeboxView;
  request: JukeboxRequest;
  client: JukeboxNativeClient;
  close: () => void;
  saved: () => void;
}) {
  const [title, setTitle] = useState(jukeboxTitle(track)),
    [artist, setArtist] = useState(track.Artist ?? "");
  const [art, setArt] = useState<JukeboxArtwork | null>(null),
    [prepared, setPrepared] = useState(false),
    [working, setWorking] = useState(""),
    [error, setError] = useState("");
  const key = jukeboxKey(track),
    revision = useRef(view.revision).current,
    artRef = useRef<JukeboxArtwork | null>(null),
    epoch = useRef(0),
    mounted = useRef(true);
  const stale = revision !== view.revision;
  async function artwork(channel: string) {
    const generation = ++epoch.current;
    setWorking(
      channel === "jukebox:lookup-artwork"
        ? "Searching for artwork…"
        : channel === "jukebox:choose-artwork"
          ? "Choosing artwork…"
          : "Loading artwork…",
    );
    setError("");
    try {
      const reply = await request<JukeboxArtworkReply>(channel, {
        trackKey: key,
        expectedRevision: revision,
        ...(channel === "jukebox:lookup-artwork"
          ? { title: title.trim(), artist: artist.trim() }
          : {}),
      });
      const next = reply.artwork;
      if (!mounted.current || generation !== epoch.current) {
        await releaseJukeboxArtwork(request, next);
        return;
      }
      if (next && !validJukeboxArtwork(next)) {
        await releaseJukeboxArtwork(request, next);
        throw new Error("Unsupported artwork preview address");
      }
      if (!next) {
        if (channel === "jukebox:lookup-artwork")
          setError(
            "No matching artwork found. Try another artist/title or choose a file.",
          );
        return;
      }
      const previous = artRef.current;
      artRef.current = next;
      setArt(next);
      setPrepared(channel !== "jukebox:get-artwork");
      await releaseJukeboxArtwork(request, previous);
    } catch (e) {
      if (mounted.current && generation === epoch.current) setError(String(e));
    } finally {
      if (mounted.current && generation === epoch.current) setWorking("");
    }
  }
  useEffect(() => {
    mounted.current = true;
    if (view.capabilities.artworkPreview) void artwork("jukebox:get-artwork");
    return () => {
      mounted.current = false;
      epoch.current++;
      void releaseJukeboxArtwork(request, artRef.current);
      artRef.current = null;
    };
  }, []);
  async function save() {
    if (stale || working) return;
    setWorking("Saving song…");
    setError("");
    const ok = await client.perform(
      "jukebox:edit-track",
      {
        trackKey: key,
        title: title.trim(),
        artist: artist.trim(),
        ...(prepared && art ? { artworkToken: art.token } : {}),
      },
      "Saving song",
    );
    if (!mounted.current) return;
    if (ok) {
      saved();
      close();
    } else {
      setError(client.snapshot().error || "Song was not saved");
      setWorking("");
    }
  }
  return (
    <Modal title="Edit song" close={close} locked={working === "Saving song…"}>
      <div className="juke-song-form">
        <label>
          Song title
          <input
            aria-label="Song title"
            maxLength={120}
            value={title}
            disabled={!!working}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label>
          Artist
          <input
            aria-label="Song artist"
            maxLength={120}
            value={artist}
            disabled={!!working}
            onChange={(e) => setArtist(e.target.value)}
          />
        </label>
        <div className="juke-art-editor">
          <div className="juke-art-large">
            {art ? (
              <img src={art.url} alt="Song artwork preview" />
            ) : (
              <span>♫</span>
            )}
          </div>
          <div>
            <strong>Artwork</strong>
            <p className="muted">
              Search using the artist and song title, or choose your own image.
            </p>
            <div className="juke-actions">
              <button
                disabled={
                  !!working ||
                  stale ||
                  !view.capabilities.artworkLookup ||
                  !artist.trim() ||
                  !title.trim()
                }
                onClick={() => void artwork("jukebox:lookup-artwork")}
              >
                Find online
              </button>
              <button
                disabled={
                  !!working || stale || !view.capabilities.artworkEditing
                }
                onClick={() => void artwork("jukebox:choose-artwork")}
              >
                Choose file
              </button>
            </div>
            {prepared && (
              <small className="muted">
                New artwork selected · saved only with Save song.
              </small>
            )}
            {art?.source && (
              <small className="muted">
                {art.source}
                {art.matchedArtist ? ` · ${art.matchedArtist}` : ""}
                {art.matchedTitle ? ` — ${art.matchedTitle}` : ""}
              </small>
            )}
          </div>
        </div>
        {stale && (
          <p className="error" role="alert">
            The library changed while this editor was open. Close and reopen
            this song before saving; your original is unchanged.
          </p>
        )}
        {working && (
          <p className="muted" role="status">
            {working}
          </p>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <div className="juke-modal-actions">
          <button disabled={working === "Saving song…"} onClick={close}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={!!working || stale || !title.trim()}
            onClick={() => void save()}
          >
            Save song
          </button>
        </div>
      </div>
    </Modal>
  );
}

export function JukeboxNative({
  status,
  onBusyChange,
  request = call,
  initialView,
  freestylePlaylist = true,
  onFreestylePlaylist,
}: {
  status?: (message: string) => void;
  onBusyChange?: (busy: boolean) => void;
  request?: JukeboxRequest;
  initialView?: JukeboxView;
  /** WPF usePlaylistInFreestyle (default on), saved in launcher settings. */
  freestylePlaylist?: boolean;
  onFreestylePlaylist?: (on: boolean) => void;
}) {
  const client = useMemo(
    () => new JukeboxNativeClient(request, initialView),
    [request],
  );
  const current = useSyncExternalStore(
    client.subscribe,
    client.snapshot,
    client.snapshot,
  );
  const view = current.view,
    state = view?.state,
    job = view?.job;
  const [selected, setSelected] = useState(
      initialView?.state?.activePlaylistId ?? "",
    ),
    initialized = useRef(!!initialView?.state);
  const [query, setQuery] = useState(""),
    [modal, setModal] = useState<
      | "new"
      | "rename"
      | "delete"
      | "reorder"
      | "library"
      | "add"
      | "repair"
      | null
    >(null);
  const [missing, setMissing] = useState<
    { key: string; title: string; artist: string }[]
  >([]);
  const [name, setName] = useState(""),
    // WPF: a new playlist starts empty unless the box is ticked.
    [defaults, setDefaults] = useState(false),
    [order, setOrder] = useState<string[]>([]),
    [libraryQuery, setLibraryQuery] = useState("");
  const [addTitle, setAddTitle] = useState(""),
    [addArtist, setAddArtist] = useState("");
  const [editKey, setEditKey] = useState<string | null>(null),
    [localError, setLocalError] = useState(""),
    [notice, setNotice] = useState("");
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [batch, setBatch] = useState<{
      done: number;
      total: number;
      saved: number;
      failures: string[];
      active: boolean;
    } | null>(null),
    cancelBatch = useRef(false);
  const [preview, setPreview] = useState<
      (JukeboxPreview & { key: string }) | null
    >(null),
    [previewLoading, setPreviewLoading] = useState<string | null>(null),
    [playing, setPlaying] = useState(false);
  const audio = useRef<HTMLAudioElement>(null),
    audioLease = useRef<JukeboxPreview | null>(null),
    audioEpoch = useRef(0),
    mounted = useRef(true);
  const playlist = state?.playlists.find((p) => p.id === selected),
    stock = useMemo(() => new Set(view?.stockKeys ?? []), [view?.stockKeys]);
  const byKey = useMemo(
    () => new Map((state?.music.Tracks ?? []).map((t) => [jukeboxKey(t), t])),
    [state?.music.Tracks],
  );
  const keys = playlist?.trackKeys ?? (view ? defaultJukeboxKeys(view) : []),
    keysSignature = keys.join(",");
  const includedKeys = keys.filter((k) => !excluded.has(k));
  const rows = keys
    .map((k) => byKey.get(k))
    .filter((t): t is JukeboxTrack => !!t)
    .filter(
      (t) =>
        !query.trim() ||
        `${jukeboxTitle(t)} ${t.Artist ?? ""} ${jukeboxKey(t)}`
          .toLocaleLowerCase()
          .includes(query.trim().toLocaleLowerCase()),
    );
  const busy = !!current.working || jukeboxJobActive(job) || !!batch?.active;
  const canUse =
    !!view?.capabilities.nativePlayback &&
    !!playlist &&
    includedKeys.length > 0 &&
    includedKeys.length <= view.capabilities.maxSessionSlots &&
    includedKeys.every((k) => byKey.get(k)?.Collection === 1);
  const artEnabled = !!view?.capabilities.artworkPreview;
  const editing = editKey ? byKey.get(editKey) : undefined;
  useEffect(() => {
    client.activate();
    let ended = false,
      timer: ReturnType<typeof setTimeout>;
    const tick = async () => {
      await client.refresh();
      if (!ended)
        timer = setTimeout(
          tick,
          jukeboxJobActive(client.snapshot().view?.job) ? 750 : 2500,
        );
    };
    void tick();
    return () => {
      ended = true;
      clearTimeout(timer);
      client.dispose();
    };
  }, [client]);
  useEffect(() => {
    onBusyChange?.(busy);
    return () => onBusyChange?.(false);
  }, [busy, onBusyChange]);
  useEffect(() => {
    if (state && !initialized.current) {
      initialized.current = true;
      setSelected(state.activePlaylistId ?? "");
    }
    if (state && selected && !state.playlists.some((p) => p.id === selected))
      setSelected("");
  }, [state, selected]);
  useEffect(() => {
    setExcluded(new Set());
  }, [selected, keysSignature]);
  const releaseAudio = async (lease: JukeboxPreview | null) => {
    if (lease)
      await request("jukebox:release-preview", { token: lease.token }).catch(
        () => undefined,
      );
  };
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      cancelBatch.current = true;
      audioEpoch.current++;
      audio.current?.pause();
      void releaseAudio(audioLease.current);
      audioLease.current = null;
    };
  }, [request]);
  async function action(
    channel: string,
    args: Record<string, unknown>,
    message: string,
  ) {
    const ok = await client.perform(channel, args, message);
    if (ok) status?.(client.snapshot().notice);
    return ok;
  }
  async function saveChoices() {
    if (!playlist) return false;
    return (
      !excluded.size ||
      action(
        "jukebox:set-order",
        { playlistId: selected, trackKeys: includedKeys },
        "Saving selected songs",
      )
    );
  }
  async function exportPlaylist() {
    if (await saveChoices())
      await action(
        "jukebox:export-playlist",
        { playlistId: selected },
        "Choosing playlist export",
      );
  }
  // WPF jukebox:repair-missing-tracks: list custom songs whose saved audio is
  // gone, then drop only those after confirmation.
  async function findMissing() {
    setLocalError("");
    try {
      const reply = await request<{
        missing: { key: string; title: string; artist: string }[];
      }>("jukebox:find-missing", {});
      if (!reply.missing.length) {
        setNotice("No missing songs: every custom song's audio is saved.");
        return;
      }
      setMissing(reply.missing);
      setModal("repair");
    } catch (e) {
      setLocalError(String(e));
    }
  }
  async function refreshBundle() {
    if (await saveChoices())
      await action(
        "jukebox:refresh-bundle",
        { playlistId: selected },
        "Refreshing playlist share bundle",
      );
  }
  async function choosePlaylist(id: string) {
    if (excluded.size) {
      setLocalError(
        "Save your song selection, or restore the unchecked songs, before switching playlists.",
      );
      return;
    }
    audio.current?.pause();
    setSelected(id);
    setQuery("");
    setLocalError("");
  }
  async function playPreview(track: JukeboxTrack) {
    const key = jukeboxKey(track);
    if (preview?.key === key && audio.current) {
      if (audio.current.paused)
        await audio.current.play().catch((e) => setLocalError(String(e)));
      else audio.current.pause();
      return;
    }
    const generation = ++audioEpoch.current;
    audio.current?.pause();
    setPlaying(false);
    setPreview(null);
    setPreviewLoading(key);
    setLocalError("");
    const old = audioLease.current;
    audioLease.current = null;
    await releaseAudio(old);
    try {
      const lease = await request<JukeboxPreview>("jukebox:preview", {
        trackKey: key,
        expectedRevision: view?.revision,
      });
      if (!mounted.current || generation !== audioEpoch.current) {
        await releaseAudio(lease);
        return;
      }
      if (!validJukeboxPreview(lease)) {
        await releaseAudio(lease);
        throw new Error("Unsupported native audio preview address");
      }
      audioLease.current = lease;
      setPreview({ ...lease, key });
    } catch (e) {
      if (mounted.current && generation === audioEpoch.current)
        setLocalError(String(e));
    } finally {
      if (mounted.current && generation === audioEpoch.current)
        setPreviewLoading(null);
    }
  }
  async function batchArtwork() {
    if (!playlist || busy) return;
    cancelBatch.current = false;
    setBatch({
      done: 0,
      total: keys.length,
      saved: 0,
      failures: [],
      active: true,
    });
    const result = await findMissingJukeboxArtwork({
      keys,
      request,
      client,
      cancelled: () => cancelBatch.current || !mounted.current,
      progress: (done, total, saved, failures) => {
        if (mounted.current)
          setBatch({ done, total, saved, failures, active: true });
      },
    });
    if (mounted.current) {
      setBatch({ ...result, total: keys.length, active: false });
      setNotice(
        `${result.cancelled ? "Artwork search stopped" : "Artwork search complete"}: ${result.saved} saved, ${result.failures.length} not found or failed. Existing covers were kept.`,
      );
    }
  }
  const previewAllowed = (track: JukeboxTrack) =>
    !!view &&
    (stock.has(jukeboxKey(track))
      ? view.capabilities.stockPreview && track.Collection === 1
      : view.capabilities.customPreview && !!state?.audio[jukeboxKey(track)]);
  const playbackHeading = view
    ? {
        unavailable: "Game playback unavailable",
        default: "Original soundtrack",
        selected: "Selected for next Play",
        preparing: "Preparing game playlist",
        prepared: "Game playlist prepared",
        active: "Game playlist active",
        restoring: "Restoring original soundtrack",
        held: "Playlist session needs attention",
      }[view.playback.phase]
    : "";
  const modalClose = () => {
    if (!current.working) {
      setModal(null);
      setLocalError("");
    }
  };
  return (
    <div className="juke-workspace" aria-busy={busy || !view}>
      <section className="card jukebox-controls">
        <div className="jukebox-playlist-row">
          <label>
            Playlist
            <select
              id="jukeboxPlaylistSelect"
              aria-label="Music playlist"
              disabled={!state || busy}
              value={selected}
              onChange={(e) => void choosePlaylist(e.target.value)}
            >
              <option value="">Default Playlist (Locked)</option>
              {state?.playlists.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.trackKeys.length})
                </option>
              ))}
            </select>
          </label>
          <div className="jukebox-playlist-actions jukebox-playlist-actions-primary">
            <button
              className="secondary small"
              id="saveJukeboxPlaylistBundle"
              disabled={
                busy ||
                !playlist ||
                !view?.capabilities.exportPlaylists ||
                !includedKeys.length
              }
              title="Save a shareable .2kplaylist"
              onClick={() => void exportPlaylist()}
            >
              {excluded.size ? "Save •" : "Save"}
            </button>
          </div>
          <div className="jukebox-playlist-actions jukebox-playlist-actions-manage">
            <button
              className="secondary small"
              id="newJukeboxPlaylist"
              title="Create a new playlist"
              disabled={busy || !state || !!excluded.size}
              onClick={() => {
                setName("");
                setDefaults(false);
                setModal("new");
              }}
            >
              New
            </button>
            <button
              className="secondary small"
              id="editJukeboxPlaylist"
              title="Rename the selected playlist"
              disabled={busy || !playlist}
              onClick={() => {
                setName(playlist?.name ?? "");
                setModal("rename");
              }}
            >
              Rename
            </button>
            <button
              className="secondary small"
              id="orderJukeboxPlaylists"
              title="Drag playlists into your preferred order"
              disabled={
                busy ||
                !view?.capabilities.playlistReordering ||
                (state?.playlists.length ?? 0) < 2
              }
              onClick={() => {
                setOrder(state!.playlists.map((p) => p.id));
                setModal("reorder");
              }}
            >
              Reorder
            </button>
            <button
              className="secondary small"
              id="deleteJukeboxPlaylist"
              title="Delete the selected playlist"
              disabled={busy || !playlist || !!excluded.size}
              onClick={() => setModal("delete")}
            >
              Delete
            </button>
          </div>
          <div className="jukebox-playlist-actions jukebox-playlist-actions-files">
            <button
              className="secondary small"
              id="importJukeboxPlaylist"
              disabled={
                busy ||
                !state ||
                !!excluded.size ||
                !view?.capabilities.importPlaylists
              }
              title="Import a .2kplaylist share file"
              onClick={() =>
                void action(
                  "jukebox:import-playlist",
                  {},
                  "Choosing .2kplaylist",
                )
              }
            >
              Import
            </button>
            <button
              className="secondary small"
              id="openJukeboxPlaylistFolder"
              title="Open the playlist share folder"
              disabled={!state}
              onClick={() =>
                void request("jukebox:open-folder").catch((e) =>
                  setLocalError(String(e)),
                )
              }
            >
              Folder
            </button>
            {/* Rust-only bundle tools, in WPF's "More" menu pattern. */}
            <details className="outfit-actions jukebox-more-actions">
              <summary aria-label="More playlist actions">
                More <span aria-hidden="true">▾</span>
              </summary>
              <div className="outfit-actions-menu">
                <button
                  className="secondary small"
                  disabled={
                    busy ||
                    !playlist ||
                    !view?.capabilities.exportPlaylists ||
                    !includedKeys.length
                  }
                  title="Rebuild this playlist's share bundle in Library/Jukebox/Playlists"
                  onClick={() => void refreshBundle()}
                >
                  Refresh bundle
                </button>
                <button
                  className="secondary small"
                  disabled={busy || !state || !!excluded.size}
                  title="Find songs whose saved audio file is missing"
                  onClick={() => void findMissing()}
                >
                  Repair missing tracks
                </button>
              </div>
            </details>
          </div>
        </div>
        <div className="jukebox-session-row">
          <label
            className="jukebox-freestyle-option"
            title="Freestyle plays your imported playlist instead of its stock music"
          >
            <input
              type="checkbox"
              id="usePlaylistInFreestyle"
              aria-label="Use playlist in Freestyle"
              checked={freestylePlaylist}
              disabled={!onFreestylePlaylist}
              onChange={(e) => onFreestylePlaylist?.(e.target.checked)}
            />
            <span>Use playlist in Freestyle</span>
            <small>
              Applies next launch · imported songs only · samples 58 if your
              playlist is larger
            </small>
          </label>
          <div className="jukebox-next-play">
            <div
              role={view?.playback.phase === "held" ? "alert" : "status"}
              className={view?.playback.phase === "held" ? "error" : "muted"}
            >
              <strong>
                {playbackHeading || "Loading the native music library…"}
              </strong>
              {view && <span> · {view.playback.message}</span>}
            </div>
            <button
              className="primary"
              disabled={
                busy ||
                !view?.capabilities.nativePlayback ||
                (!!selected && !canUse)
              }
              onClick={() =>
                void saveChoices().then(async (ok) => {
                  if (ok || !selected)
                    await action(
                      "jukebox:set-active",
                      { playlistId: selected || null },
                      "Saving next Play selection",
                    );
                })
              }
            >
              {selected ? "Use for next Play" : "Use default next Play"}
            </button>
          </div>
        </div>
        {view?.playback.phase === "active" &&
          view.playback.gameplayVerified === false && (
            <small className="muted">
              Native session active; in-game audio still needs a listening
              check.
            </small>
          )}
        {view?.warnings.map((warning, i) => (
          <p key={i} className="muted compact-note">
            {warning}
          </p>
        ))}
        {!!excluded.size && (
          <p className="muted compact-note">
            {excluded.size} unchecked · Save or Use for next Play to apply.{" "}
            <button disabled={busy} onClick={() => void saveChoices()}>
              Save selection
            </button>{" "}
            <button disabled={busy} onClick={() => setExcluded(new Set())}>
              Restore selection
            </button>
          </p>
        )}
      </section>
      {(current.error || current.refreshError || localError) && (
        <p className="error" role="alert">
          {localError || current.error || current.refreshError}
        </p>
      )}
      {(current.notice || notice || current.working) && (
        <p className="muted compact-note" role="status">
          {current.working ? `${current.working}…` : notice || current.notice}
        </p>
      )}
      {job && (
        <div
          className="import-progress"
          aria-live="polite"
          aria-label="Native Jukebox operation"
        >
          <div>
            <strong>
              {job.phase === "failed"
                ? "Operation failed"
                : job.phase === "cancelled"
                  ? "Operation cancelled"
                  : job.phase === "completed"
                    ? "Operation complete"
                    : "Native music preparation"}
            </strong>
            <p className="muted">
              {job.message}
              {job.frames !== undefined && job.phase === "decoding"
                ? ` · ${(job.frames / 48000).toFixed(1)} sec decoded`
                : ""}
            </p>
            {jukeboxJobActive(job) && (
              <progress
                aria-label="Jukebox import/export progress"
                max={100}
                value={jukeboxProgress(job)}
              />
            )}
            {job.error && (
              <p className="error" role="alert">
                {job.error}
              </p>
            )}
          </div>
          {jukeboxJobActive(job) && (
            <button
              disabled={
                !job.cancellable ||
                !!current.working ||
                current.cancelRequested === job.id
              }
              onClick={() =>
                void action(
                  "jukebox:cancel-import",
                  { jobId: job.id },
                  "Requesting cancellation",
                )
              }
            >
              {current.cancelRequested === job.id ? "Cancelling…" : "Cancel"}
            </button>
          )}
        </div>
      )}
      {batch && (
        <div className="juke-batch" role="status">
          <span>
            Artwork: {batch.done}/{batch.total} checked · {batch.saved} saved ·{" "}
            {batch.failures.length} not found or failed
          </span>
          {batch.active && (
            <button
              onClick={() => {
                cancelBatch.current = true;
                setNotice("Stopping after the current artwork request…");
              }}
            >
              Stop search
            </button>
          )}
          {batch.failures.length > 0 && (
            <details>
              <summary>View artwork results</summary>
              {batch.failures.map((failure, i) => (
                <p key={i}>{failure}</p>
              ))}
            </details>
          )}
        </div>
      )}
      <div className="jukebox-table-search">
        <div className="jukebox-song-tools">
          <button
            className="primary small"
            id="importJukeboxWav"
            disabled={
              busy ||
              !playlist ||
              !!excluded.size ||
              !view?.capabilities.importAudio
            }
            onClick={() => {
              setAddTitle("");
              setAddArtist("");
              setModal("add");
            }}
          >
            + Add song
          </button>
          <button
            className="secondary small"
            id="batchImportJukeboxWav"
            disabled={
              busy ||
              !playlist ||
              !!excluded.size ||
              !view?.capabilities.importAudio
            }
            onClick={() =>
              void action(
                "jukebox:import-audio",
                { playlistId: selected },
                "Choosing audio",
              )
            }
          >
            Batch add
          </button>
          <button
            className="secondary small"
            id="findAllJukeboxArtwork"
            disabled={
              busy ||
              !playlist ||
              !view?.capabilities.artworkLookup ||
              !keys.length
            }
            onClick={() => void batchArtwork()}
          >
            Find missing artwork
          </button>
          <button
            className="secondary small"
            disabled={busy || !playlist || !!excluded.size}
            onClick={() => {
              setLibraryQuery("");
              setModal("library");
            }}
          >
            Add from library
          </button>
        </div>
        <label className="jukebox-search-field">
          <span data-ui-icon="search" aria-hidden="true">
            <UiIcon name="search" />
          </span>
          <input
            id="jukeboxSearch"
            aria-label="Search music"
            placeholder="Search by song or artist…"
            autoComplete="off"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <p id="jukeboxResultSummary">
          {rows.length} of {keys.length} {playlist ? "playlist" : "default"}{" "}
          songs shown{!playlist ? " · Default playlist locked." : ""}
        </p>
      </div>
      <section
        className="card jukebox-table-card"
        aria-label="Selected playlist songs"
      >
        <div className="jukebox-table-head" aria-hidden="true">
          <span>Use</span>
          <span>Art</span>
          <span>Song</span>
          <span>Artist</span>
          <span>Actions</span>
        </div>
        <div
          id="jukeboxRows"
          className="jukebox-table-body"
          role="list"
          aria-label="Playlist song list"
        >
          {rows.map((track) => {
            const key = jukeboxKey(track),
              index = keys.indexOf(key),
              loading = previewLoading === key,
              isPlaying = preview?.key === key && playing;
            return (
              <div
                className="jukebox-row"
                role="listitem"
                key={key}
                data-track-key={key}
              >
                <input
                  type="checkbox"
                  aria-label={`Include ${jukeboxTitle(track)}`}
                  checked={!excluded.has(key)}
                  disabled={busy || !playlist}
                  onChange={(e) =>
                    setExcluded((previous) => {
                      const next = new Set(previous);
                      if (e.target.checked) next.delete(key);
                      else next.add(key);
                      return next;
                    })
                  }
                />
                <Cover
                  track={track}
                  revision={view!.revision}
                  request={request}
                  enabled={artEnabled}
                />
                <div>
                  <strong className="jukebox-title" title={jukeboxTitle(track)}>
                    {jukeboxTitle(track)}
                  </strong>
                </div>
                <div>
                  <strong className="jukebox-artist" title={track.Artist}>
                    {track.Artist || "—"}
                  </strong>
                </div>
                <div className="jukebox-row-actions">
                  <button
                    className="jukebox-play-button"
                    data-jukebox-preview
                    aria-label={`${isPlaying ? "Pause" : "Preview"} ${jukeboxTitle(track)}`}
                    disabled={loading || !previewAllowed(track)}
                    onClick={() => void playPreview(track)}
                  >
                    {loading ? "Preparing…" : isPlaying ? "Pause" : "Play"}
                  </button>
                  <button
                    className="jukebox-edit-button"
                    data-jukebox-edit
                    disabled={busy || !playlist}
                    onClick={() => setEditKey(key)}
                  >
                    Edit
                  </button>
                  <button
                    disabled={busy || !playlist || !!excluded.size}
                    className="jukebox-remove-button"
                    data-jukebox-remove
                    title="Remove from this playlist; keep the audio in your library"
                    onClick={() =>
                      void action(
                        "jukebox:set-order",
                        {
                          playlistId: selected,
                          trackKeys: keys.filter((k) => k !== key),
                        },
                        "Removing song from playlist",
                      )
                    }
                  >
                    Remove
                  </button>
                  <button
                    className="jukebox-order-button"
                    aria-label={`Move song ${index + 1} up`}
                    disabled={busy || !playlist || !index || !!excluded.size}
                    onClick={() =>
                      void action(
                        "jukebox:set-order",
                        {
                          playlistId: selected,
                          trackKeys: moveJukeboxTrack(keys, index, -1),
                        },
                        "Saving song order",
                      )
                    }
                  >
                    ↑
                  </button>
                  <button
                    className="jukebox-order-button"
                    aria-label={`Move song ${index + 1} down`}
                    disabled={
                      busy ||
                      !playlist ||
                      index === keys.length - 1 ||
                      !!excluded.size
                    }
                    onClick={() =>
                      void action(
                        "jukebox:set-order",
                        {
                          playlistId: selected,
                          trackKeys: moveJukeboxTrack(keys, index, 1),
                        },
                        "Saving song order",
                      )
                    }
                  >
                    ↓
                  </button>
                </div>
              </div>
            );
          })}
          {!rows.length && (
            <div className="empty">
              {!state
                ? "The native music catalog is not loaded."
                : query
                  ? "No songs in this playlist match the search."
                  : "This playlist is empty. Use + Add song or Add from library."}
            </div>
          )}
        </div>
      </section>
      {!!playlist &&
        keys.length > (view?.capabilities.maxSessionSlots ?? 71) && (
          <p className="error">
            {keys.length} songs exceeds the{" "}
            {view?.capabilities.maxSessionSlots ?? 71} positions per game
            session. Split this playlist before Play. Nothing will be silently
            truncated.
          </p>
        )}
      {preview && (
        <div className="jukebox-audio">
          <small className="muted">
            Launcher preview ·{" "}
            {byKey.get(preview.key)
              ? jukeboxTitle(byKey.get(preview.key)!)
              : "Song"}
          </small>
          <audio
            ref={audio}
            key={preview.token}
            src={preview.url}
            controls
            autoPlay
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
            onEnded={() => setPlaying(false)}
            onError={() => {
              setPlaying(false);
              setLocalError(
                "This native audio preview could not be played; original audio is unchanged.",
              );
            }}
          />
        </div>
      )}
      <section className="notice player-note jukebox-share-note">
        <span className="jukebox-share-icon" aria-hidden="true">
          <UiIcon name="jukebox" />
        </span>
        <div>
          <strong>Save to share</strong>
          <p>
            Save exports your playlist, audio and saved artwork as a
            .2kplaylist. Original files stay in your library.
          </p>
          <small>
            Library edits do not change a running game. Select Use for next Play
            when you’re ready.
          </small>
        </div>
      </section>
      {editing && view && (
        <SongEditor
          key={editKey}
          track={editing}
          view={view}
          request={request}
          client={client}
          close={() => setEditKey(null)}
          saved={() => status?.("Song saved to the library.")}
        />
      )}
      {(modal === "new" || modal === "rename") && (
        <Modal
          title={modal === "new" ? "New playlist" : "Rename playlist"}
          close={modalClose}
          locked={!!current.working}
        >
          <label className="juke-field">
            Playlist name
            <input
              aria-label="Playlist name"
              maxLength={60}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          {modal === "new" && (
            <label className="juke-check">
              <input
                type="checkbox"
                checked={defaults}
                onChange={(e) => setDefaults(e.target.checked)}
              />
              Add all default songs
            </label>
          )}
          <div className="juke-modal-actions">
            <button onClick={modalClose} disabled={busy}>
              Cancel
            </button>
            <button
              className="primary"
              disabled={busy || !name.trim()}
              onClick={() => {
                const before = new Set(state?.playlists.map((p) => p.id));
                void action(
                  modal === "new"
                    ? "jukebox:create-playlist"
                    : "jukebox:rename-playlist",
                  modal === "new"
                    ? { name: name.trim(), addDefaultSongs: defaults }
                    : { playlistId: selected, name: name.trim() },
                  "Saving playlist",
                ).then((ok) => {
                  if (ok) {
                    if (modal === "new") {
                      const added = client
                        .snapshot()
                        .view?.state?.playlists.find((p) => !before.has(p.id));
                      if (added) setSelected(added.id);
                    }
                    setModal(null);
                  }
                });
              }}
            >
              {modal === "new" ? "Create playlist" : "Save name"}
            </button>
          </div>
          {current.error && (
            <p className="error" role="alert">
              {current.error}
            </p>
          )}
        </Modal>
      )}
      {modal === "delete" && playlist && (
        <Modal title="Delete playlist" close={modalClose} locked={busy}>
          <p>
            Delete “{playlist.name}”? Songs and artwork stay in your library.
          </p>
          <div className="juke-modal-actions">
            <button onClick={modalClose}>Keep playlist</button>
            <button
              className="juke-remove"
              disabled={busy}
              onClick={() =>
                void action(
                  "jukebox:delete-playlist",
                  { playlistId: selected },
                  "Deleting playlist",
                ).then((ok) => {
                  if (ok) setModal(null);
                })
              }
            >
              Confirm delete
            </button>
          </div>
        </Modal>
      )}
      {modal === "repair" && (
        <Modal title="Repair missing tracks" close={modalClose} locked={busy}>
          <p>
            {missing.length === 1
              ? "1 song's"
              : `${missing.length} songs'`}{" "}
            saved audio is missing from Library/Jukebox/Audio. Remove{" "}
            {missing.length === 1 ? "it" : "them"} from the library and every
            playlist? Nothing else is deleted.
          </p>
          <ul className="juke-missing-list">
            {missing.map((m) => (
              <li key={m.key}>
                <strong>{m.title || m.key}</strong>
                {m.artist && <span> · {m.artist}</span>}
              </li>
            ))}
          </ul>
          <div className="juke-modal-actions">
            <button onClick={modalClose}>Keep songs</button>
            <button
              className="juke-remove"
              disabled={busy}
              onClick={() =>
                void action(
                  "jukebox:repair-missing",
                  { trackKeys: missing.map((m) => m.key) },
                  "Repairing missing songs",
                ).then((ok) => {
                  if (ok) setModal(null);
                })
              }
            >
              Remove missing songs
            </button>
          </div>
        </Modal>
      )}
      {modal === "reorder" && (
        <Modal title="Reorder playlists" close={modalClose} locked={busy}>
          <div className="juke-reorder">
            {order.map((id, index) => (
              <div key={id}>
                <strong>
                  {state?.playlists.find((p) => p.id === id)?.name}
                </strong>
                <button
                  aria-label={`Move playlist ${index + 1} up`}
                  disabled={!index || busy}
                  onClick={() => setOrder(moveJukeboxTrack(order, index, -1))}
                >
                  ↑
                </button>
                <button
                  aria-label={`Move playlist ${index + 1} down`}
                  disabled={index === order.length - 1 || busy}
                  onClick={() => setOrder(moveJukeboxTrack(order, index, 1))}
                >
                  ↓
                </button>
              </div>
            ))}
          </div>
          <div className="juke-modal-actions">
            <button onClick={modalClose}>Cancel</button>
            <button
              className="primary"
              disabled={busy}
              onClick={() =>
                void action(
                  "jukebox:reorder-playlists",
                  { playlistIds: order },
                  "Saving playlist order",
                ).then((ok) => {
                  if (ok) setModal(null);
                })
              }
            >
              Save order
            </button>
          </div>
        </Modal>
      )}
      {modal === "add" && (
        <Modal title="Add song" close={modalClose} locked={busy}>
          <div className="juke-song-form">
            <label>
              Song title (optional)
              <input
                aria-label="New song title"
                maxLength={120}
                value={addTitle}
                onChange={(e) => setAddTitle(e.target.value)}
              />
            </label>
            <label>
              Artist (optional)
              <input
                aria-label="New song artist"
                maxLength={120}
                value={addArtist}
                onChange={(e) => setAddArtist(e.target.value)}
              />
            </label>
            <p className="muted">
              Choose an audio file to add to {playlist?.name}. Then use Edit to
              find or choose artwork. Leave the title or artist blank to use
              the filename (“Artist - Title”; otherwise the artist is Custom).
            </p>
            <div className="juke-modal-actions">
              <button onClick={modalClose}>Cancel</button>
              <button
                className="primary"
                disabled={busy}
                onClick={() =>
                  void action(
                    "jukebox:import-audio",
                    {
                      playlistId: selected,
                      single: true,
                      ...(addTitle.trim() ? { title: addTitle.trim() } : {}),
                      ...(addArtist.trim() ? { artist: addArtist.trim() } : {}),
                    },
                    "Choosing audio",
                  ).then((ok) => {
                    if (ok) setModal(null);
                  })
                }
              >
                Choose audio & add
              </button>
            </div>
            {current.error && (
              <p className="error" role="alert">
                {current.error}
              </p>
            )}
          </div>
        </Modal>
      )}
      {modal === "library" && view && (
        <Modal
          title="Add from library"
          close={modalClose}
          locked={!!current.working}
        >
          <input
            className="juke-library-search"
            type="search"
            aria-label="Search library"
            placeholder="Search all songs in your library"
            value={libraryQuery}
            onChange={(e) => setLibraryQuery(e.target.value)}
          />
          <div className="juke-library-list" aria-label="Music catalog">
            {findJukeboxTracks(view, libraryQuery, "all", "main").map(
              (track) => {
                const key = jukeboxKey(track),
                  added = keys.includes(key);
                return (
                  <div key={key}>
                    <Cover
                      track={track}
                      revision={view.revision}
                      request={request}
                      enabled={artEnabled}
                    />
                    <div>
                      <strong>{jukeboxTitle(track)}</strong>
                      <small className="muted">
                        {track.Artist || "—"} ·{" "}
                        {stock.has(key) ? "Stock" : "Imported"}
                      </small>
                    </div>
                    <button
                      disabled={busy || added}
                      onClick={() =>
                        void action(
                          "jukebox:set-order",
                          { playlistId: selected, trackKeys: [...keys, key] },
                          "Adding song to playlist",
                        )
                      }
                    >
                      {added ? "Added" : "Add"}
                    </button>
                  </div>
                );
              },
            )}
          </div>
          <div className="juke-modal-actions">
            <button onClick={modalClose}>Done</button>
          </div>
        </Modal>
      )}
    </div>
  );
}
