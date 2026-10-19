// This screen uses the owned native command service. There is no filename-only
// playlist fallback or optimistic claim of native/game playback.
export type JukeboxTrack = {
  Artist?: string;
  Collection: number;
  "Cover Name"?: string;
  Name?: string;
  "Bank Index": number;
  "Unique Id": number;
};
export type JukeboxPlaylist = { id: string; name: string; trackKeys: string[] };
export type JukeboxState = {
  schema: string;
  stockSha256: string;
  music: { Tracks: JukeboxTrack[] };
  playlists: JukeboxPlaylist[];
  activePlaylistId: string | null;
  audio: Record<
    string,
    {
      native: { sha256: string; size: number };
      artwork?: unknown;
      preview?: unknown;
    }
  >;
  artworkOverrides?: Record<string, { native: unknown; preview: unknown }>;
};
export type JukeboxJob = {
  id: string;
  kind:
    | "audio-import"
    | "playlist-import"
    | "playlist-export"
    | "playlist-bundle";
  phase:
    | "choosing"
    | "reading"
    | "decoding"
    | "saving"
    | "exporting"
    | "completed"
    | "cancelled"
    | "failed";
  message: string;
  done?: number;
  total?: number;
  frames?: number;
  cancellable: boolean;
  error?: string;
};
export type JukeboxView = {
  schema: "nba2k27-jukebox-view-v1";
  sequence: number;
  revision: string;
  state: JukeboxState | null;
  // Authoritative stock identities, NOT bank<=71: Version2 stock includes79..100.
  stockKeys: string[];
  capabilities: {
    importAudio: boolean;
    importPlaylists: boolean;
    exportPlaylists: boolean;
    customPreview: boolean;
    stockPreview: boolean;
    nativePlayback: boolean;
    maxSessionSlots: number;
    artworkEditing?: boolean;
    artworkLookup?: boolean;
    artworkPreview?: boolean;
    playlistReordering?: boolean;
    nativeArtworkPublication?: boolean;
  };
  playback: {
    phase:
      | "unavailable"
      | "default"
      | "selected"
      | "preparing"
      | "prepared"
      | "active"
      | "restoring"
      | "held";
    message: string;
    playlistId?: string;
    session?: string;
    gameplayVerified?: boolean;
  };
  job?: JukeboxJob | null;
  warnings: string[];
};
export type JukeboxReply = {
  kind: "state" | "started" | "cancelled" | "exported";
  view?: JukeboxView;
  message?: string;
};
export type JukeboxRequest = <T>(channel: string, args?: unknown) => Promise<T>;
export type JukeboxPreview = {
  token: string;
  url: string;
  mimeType: "audio/wav" | "audio/ogg";
};
export type JukeboxArtwork = {
  token: string;
  url: string;
  mimeType: "image/png";
  matchedTitle?: string;
  matchedArtist?: string;
  source?: string;
};
export type JukeboxArtworkReply = { artwork: JukeboxArtwork | null };

// Images are served only from a bounded native lease, never a renderer URL,
// remote image, arbitrary file path, or HTML supplied by an artwork service.
export function validJukeboxArtwork(artwork: JukeboxArtwork) {
  return (
    artwork?.mimeType === "image/png" &&
    validJukeboxPreview({
      token: artwork.token,
      url: artwork.url,
      mimeType: "audio/wav",
    })
  );
}
export async function releaseJukeboxArtwork(
  request: JukeboxRequest,
  artwork: JukeboxArtwork | null,
) {
  if (artwork?.token)
    await request("jukebox:release-artwork", { token: artwork.token }).catch(
      () => undefined,
    );
}

export function defaultJukeboxKeys(view: JukeboxView) {
  const stock = new Set(view.stockKeys);
  return (view.state?.music.Tracks ?? [])
    .filter((track) => track.Collection === 1 && stock.has(jukeboxKey(track)))
    .map(jukeboxKey);
}

// A serial, explicitly-started operation. Existing artwork is checked again
// before lookup and every prepared lease is released on failure/cancellation.
export async function findMissingJukeboxArtwork(options: {
  keys: string[];
  request: JukeboxRequest;
  client: JukeboxNativeClient;
  cancelled: () => boolean;
  progress: (
    done: number,
    total: number,
    saved: number,
    failures: string[],
  ) => void;
}) {
  const { keys, request, client, cancelled, progress } = options;
  let done = 0,
    saved = 0;
  const failures: string[] = [];
  for (const key of [...new Set(keys)]) {
    if (cancelled()) break;
    let prepared: JukeboxArtwork | null = null;
    try {
      const view = client.snapshot().view;
      const track = view?.state?.music.Tracks.find(
        (t) => jukeboxKey(t) === key,
      );
      if (!view || !track) throw new Error("Song is no longer in the library");
      const args = { trackKey: key, expectedRevision: view.revision };
      const current = await request<JukeboxArtworkReply>(
        "jukebox:get-artwork",
        args,
      );
      prepared = current.artwork;
      if (prepared && !validJukeboxArtwork(prepared))
        throw new Error("Unsupported artwork address");
      if (!prepared && !cancelled()) {
        const result = await request<JukeboxArtworkReply>(
          "jukebox:lookup-artwork",
          {
            ...args,
            title: jukeboxTitle(track),
            artist: track.Artist ?? "",
          },
        );
        prepared = result.artwork;
        if (prepared && !validJukeboxArtwork(prepared))
          throw new Error("Unsupported artwork address");
        if (!prepared) throw new Error("No matching artwork found");
        if (!cancelled()) {
          const ok = await client.perform(
            "jukebox:edit-track",
            {
              trackKey: key,
              title: jukeboxTitle(track),
              artist: track.Artist ?? "",
              artworkToken: prepared.token,
            },
            "Saving artwork",
          );
          if (!ok)
            throw new Error(client.snapshot().error || "Artwork was not saved");
          saved++;
        }
      }
    } catch (error) {
      failures.push(`${key}: ${String(error)}`);
    } finally {
      await releaseJukeboxArtwork(request, prepared);
      done++;
      progress(done, keys.length, saved, [...failures]);
    }
  }
  return { done, saved, failures, cancelled: cancelled() };
}
export const jukeboxKey = (track: JukeboxTrack) =>
  `${track.Collection}:${track["Bank Index"]}:${track["Unique Id"]}`;
export const jukeboxTitle = (track: JukeboxTrack) =>
  track.Name?.trim() || track.Artist?.trim() || `Track ${track["Bank Index"]}`;
export const jukeboxJobActive = (job?: JukeboxJob | null) =>
  !!job && !["completed", "cancelled", "failed"].includes(job.phase);

export function validateJukeboxView(value: JukeboxView): JukeboxView {
  const phases = [
    "unavailable",
    "default",
    "selected",
    "preparing",
    "prepared",
    "active",
    "restoring",
    "held",
  ];
  const text = (input: unknown) =>
    typeof input === "string" && input.length <= 4096;
  const identity = (input: unknown) =>
    typeof input === "string" && /^\d{1,5}:\d{1,5}:\d{1,5}$/.test(input);
  if (
    !value ||
    value.schema !== "nba2k27-jukebox-view-v1" ||
    !Number.isSafeInteger(value.sequence) ||
    value.sequence < 0 ||
    !value.capabilities ||
    !Number.isInteger(value.capabilities.maxSessionSlots) ||
    value.capabilities.maxSessionSlots < 1 ||
    value.capabilities.maxSessionSlots > 999 ||
    ![
      value.capabilities.importAudio,
      value.capabilities.importPlaylists,
      value.capabilities.exportPlaylists,
      value.capabilities.customPreview,
      value.capabilities.stockPreview,
      value.capabilities.nativePlayback,
    ].every((v) => typeof v === "boolean") ||
    [
      value.capabilities.artworkEditing,
      value.capabilities.artworkLookup,
      value.capabilities.artworkPreview,
      value.capabilities.playlistReordering,
      value.capabilities.nativeArtworkPublication,
    ].some((v) => v !== undefined && typeof v !== "boolean") ||
    !Array.isArray(value.stockKeys) ||
    value.stockKeys.length > 4096 ||
    !value.stockKeys.every(identity) ||
    !Array.isArray(value.warnings) ||
    !value.warnings.every(text) ||
    !value.playback ||
    !phases.includes(value.playback.phase) ||
    !text(value.playback.message)
  ) {
    throw new Error(
      "The native Jukebox returned an unsupported status. No changes were confirmed.",
    );
  }
  if (
    value.state &&
    (!/^[a-f0-9]{64}$/i.test(value.revision) ||
      !Array.isArray(value.state.music?.Tracks) ||
      value.state.music.Tracks.length > 4096 ||
      !Array.isArray(value.state.playlists) ||
      value.state.playlists.length > 100 ||
      !value.state.audio ||
      Array.isArray(value.state.audio) ||
      !value.state.music.Tracks.every(
        (t) =>
          t &&
          [t.Collection, t["Bank Index"], t["Unique Id"]].every(
            (n) => Number.isInteger(n) && n >= 0 && n <= 65535,
          ) &&
          [t.Name, t.Artist, t["Cover Name"]].every(
            (s) => s === undefined || text(s),
          ),
      ) ||
      !value.state.playlists.every(
        (p) =>
          p &&
          text(p.id) &&
          text(p.name) &&
          Array.isArray(p.trackKeys) &&
          p.trackKeys.length <= 4096 &&
          p.trackKeys.every(identity),
      ) ||
      !Object.values(value.state.audio).every(
        (a) =>
          a?.native &&
          /^[a-f0-9]{64}$/i.test(a.native.sha256) &&
          Number.isSafeInteger(a.native.size) &&
          a.native.size > 0,
      ))
  ) {
    throw new Error(
      "The native Jukebox library response is incomplete. Refresh before editing.",
    );
  }
  if (
    value.job &&
    (!text(value.job.id) ||
      !text(value.job.message) ||
      ![
        "audio-import",
        "playlist-import",
        "playlist-export",
        "playlist-bundle",
      ].includes(
        value.job.kind,
      ) ||
      ![
        "choosing",
        "reading",
        "decoding",
        "saving",
        "exporting",
        "completed",
        "cancelled",
        "failed",
      ].includes(value.job.phase) ||
      typeof value.job.cancellable !== "boolean" ||
      (value.job.frames !== undefined &&
        (!Number.isSafeInteger(value.job.frames) || value.job.frames < 0)) ||
      (value.job.error !== undefined && !text(value.job.error)))
  ) {
    throw new Error(
      "The native Jukebox returned an unsupported operation status.",
    );
  }
  return value;
}
export function mergeJukeboxView(
  previous: JukeboxView | null,
  incoming: JukeboxView,
) {
  validateJukeboxView(incoming);
  return previous && incoming.sequence < previous.sequence
    ? previous
    : incoming;
}
export function jukeboxProgress(job: JukeboxJob): number | undefined {
  if (job.phase === "completed") return 100;
  if (
    typeof job.done !== "number" ||
    typeof job.total !== "number" ||
    !Number.isFinite(job.done) ||
    !Number.isFinite(job.total) ||
    job.done < 0 ||
    job.total <= 0
  )
    return undefined;
  // Reading/decoding100% does not mean the metadata commit has completed.
  return Math.max(0, Math.min(99, Math.floor((job.done / job.total) * 100)));
}
export function moveJukeboxTrack(
  keys: string[],
  index: number,
  step: -1 | 1,
): string[] {
  const next = [...keys],
    target = index + step;
  if (index < 0 || index >= keys.length || target < 0 || target >= keys.length)
    return next;
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}
export function findJukeboxTracks(
  view: JukeboxView,
  query: string,
  source: "all" | "stock" | "custom",
  collection: "main" | "all",
) {
  const stock = new Set(view.stockKeys),
    needle = query.trim().toLocaleLowerCase();
  return (view.state?.music.Tracks ?? []).filter((track) => {
    const isStock = stock.has(jukeboxKey(track));
    return (
      (source === "all" || (source === "stock" ? isStock : !isStock)) &&
      (collection === "all" || track.Collection === 1) &&
      (!needle ||
        `${jukeboxTitle(track)} ${track.Artist ?? ""} ${jukeboxKey(track)}`
          .toLocaleLowerCase()
          .includes(needle))
    );
  });
}
export function validJukeboxPreview(preview: JukeboxPreview) {
  if (
    !preview ||
    !/^[a-z0-9_-]{16,128}$/i.test(preview.token) ||
    !["audio/wav", "audio/ogg"].includes(preview.mimeType)
  )
    return false;
  try {
    const url = new URL(preview.url);
    return (
      url.protocol === "http:" &&
      ["127.0.0.1", "[::1]"].includes(url.hostname) &&
      !!url.port &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash &&
      url.pathname === `/api/jukebox/preview/${preview.token}`
    );
  } catch {
    return false;
  }
}

type ClientState = {
  view: JukeboxView | null;
  working: string;
  error: string;
  refreshError: string;
  notice: string;
  cancelRequested: string | null;
};
// The independently tested command controller owns request generations. Late
// polls/replies cannot undo a newer mutation or report a cancelled picker saved.
export class JukeboxNativeClient {
  private value: ClientState;
  private listeners = new Set<() => void>();
  private epoch = 0;
  private alive = true;
  constructor(
    private request: JukeboxRequest,
    initial?: JukeboxView,
  ) {
    this.value = {
      view: initial ? validateJukeboxView(initial) : null,
      working: "",
      error: "",
      refreshError: "",
      notice: "",
      cancelRequested: null,
    };
  }
  snapshot = () => this.value;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  activate() {
    this.alive = true;
  }
  dispose() {
    this.alive = false;
    this.epoch++;
  }
  private publish(next: Partial<ClientState>) {
    if (this.alive) {
      this.value = { ...this.value, ...next };
      this.listeners.forEach((listener) => listener());
    }
  }
  async refresh() {
    if (!this.alive || this.value.working) return;
    const epoch = this.epoch;
    try {
      const incoming = validateJukeboxView(
        await this.request<JukeboxView>("jukebox:get"),
      );
      if (!this.alive || epoch !== this.epoch || this.value.working) return;
      const view = mergeJukeboxView(this.value.view, incoming);
      this.publish({
        view,
        refreshError: "",
        cancelRequested: jukeboxJobActive(view.job)
          ? this.value.cancelRequested
          : null,
      });
    } catch (error) {
      if (this.alive && epoch === this.epoch)
        this.publish({ refreshError: String(error) });
    }
  }
  async perform(
    channel: string,
    args: Record<string, unknown>,
    working: string,
  ): Promise<boolean> {
    if (!this.alive || this.value.working) return false;
    const view = this.value.view;
    if (!view?.state) {
      this.publish({
        error: "Load the native music library before editing it.",
      });
      return false;
    }
    const cancel = channel === "jukebox:cancel-import";
    if (!cancel && jukeboxJobActive(view.job)) {
      this.publish({
        error: "Wait for the current import/export or cancel it first.",
      });
      return false;
    }
    const epoch = ++this.epoch;
    this.publish({ working, error: "", refreshError: "", notice: "" });
    try {
      const reply = await this.request<JukeboxReply>(
        channel,
        cancel ? args : { ...args, expectedRevision: view.revision },
      );
      if (!this.alive || epoch !== this.epoch) return false;
      if (
        !reply ||
        !["state", "started", "cancelled", "exported"].includes(reply.kind) ||
        (reply.kind !== "cancelled" && !reply.view)
      ) {
        throw new Error(
          "The native action returned no confirmation. Nothing is shown as saved or installed.",
        );
      }
      if (reply.view && reply.view.sequence < (this.value.view?.sequence ?? 0))
        throw new Error(
          "The native action returned an older status. Refresh to confirm its result.",
        );
      const next = reply.view
        ? mergeJukeboxView(this.value.view, reply.view)
        : this.value.view;
      if (reply.kind === "started" && !jukeboxJobActive(next?.job))
        throw new Error("No active native import/export job was returned.");
      this.publish({
        view: next,
        refreshError: "",
        notice:
          reply.message ??
          (reply.kind === "cancelled"
            ? cancel
              ? "Native operation cancelled; any uncommitted import was discarded."
              : "File selection cancelled; library unchanged."
            : reply.kind === "started"
              ? "Native import/export started."
              : reply.kind === "exported"
                ? "Playlist export completed."
                : cancel
                  ? "Cancellation requested; waiting for the native helper to stop."
                  : "Saved to the launcher library. Changes apply on the next Play launch."),
        cancelRequested:
          cancel && jukeboxJobActive(next?.job) ? String(args.jobId) : null,
      });
      return reply.kind !== "cancelled";
    } catch (error) {
      if (this.alive && epoch === this.epoch)
        this.publish({ error: String(error) });
      return false;
    } finally {
      if (this.alive && epoch === this.epoch) this.publish({ working: "" });
    }
  }
}
