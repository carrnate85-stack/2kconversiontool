import { useEffect, useState } from "react";
import { UiIcon } from "./wpf/WpfShell";
import { call } from "./api";

export type PlayJukeboxPlaylist = {
  id: string;
  name: string;
  songCount: number;
};

export type PlayJukeboxSummary = {
  libraryReady: boolean;
  revision: string | null;
  playlistCount: number;
  playlists: PlayJukeboxPlaylist[];
  playlistId: string | null;
  playlistName: string | null;
  songCount: number;
};

const playlistId = /^[a-f0-9]{48}$/i;
const revision = /^[a-f0-9]{64}$/i;

export function validatePlayJukeboxSummary(value: PlayJukeboxSummary) {
  if (
    !value ||
    typeof value.libraryReady !== "boolean" ||
    !(value.revision === null || revision.test(value.revision)) ||
    !Number.isSafeInteger(value.playlistCount) ||
    value.playlistCount < 0 ||
    value.playlistCount > 100 ||
    !Array.isArray(value.playlists) ||
    value.playlists.length !== value.playlistCount ||
    !Number.isSafeInteger(value.songCount) ||
    value.songCount < 0 ||
    value.songCount > 4096 ||
    !(value.playlistId === null || playlistId.test(value.playlistId)) ||
    !(
      value.playlistName === null ||
      (typeof value.playlistName === "string" &&
        value.playlistName.trim() === value.playlistName &&
        value.playlistName.length > 0 &&
        [...value.playlistName].length <= 80)
    ) ||
    (value.playlistId === null) !== (value.playlistName === null) ||
    (value.playlistId === null && value.songCount !== 0) ||
    (!value.libraryReady &&
      (value.revision !== null || value.playlists.length !== 0))
  )
    throw new Error("The saved Jukebox selection summary is invalid.");
  const ids = new Set<string>();
  for (const playlist of value.playlists) {
    if (
      !playlist ||
      !playlistId.test(playlist.id) ||
      ids.has(playlist.id) ||
      typeof playlist.name !== "string" ||
      playlist.name.trim() !== playlist.name ||
      playlist.name.length === 0 ||
      [...playlist.name].length > 80 ||
      !Number.isSafeInteger(playlist.songCount) ||
      playlist.songCount < 0 ||
      playlist.songCount > 4096
    )
      throw new Error("The saved Jukebox selection summary is invalid.");
    ids.add(playlist.id);
  }
  if (value.playlistId) {
    const active = value.playlists.find(
      (playlist) => playlist.id === value.playlistId,
    );
    if (
      !active ||
      active.name !== value.playlistName ||
      active.songCount !== value.songCount
    )
      throw new Error("The saved Jukebox selection summary is invalid.");
  }
  return value;
}

export function playJukeboxCopy(summary: PlayJukeboxSummary | null) {
  if (!summary) return "Reading saved playlists…";
  if (!summary.libraryReady) return "Open Jukebox to create a playlist.";
  if (!summary.playlists.length) return "No saved playlists";
  if (!summary.playlistId) return "Jukebox is off for the next Play launch.";
  return `${summary.playlistName} · ${summary.songCount} song${summary.songCount === 1 ? "" : "s"}`;
}

export function PlayJukeboxChoice({
  rememberedId,
  onRemember,
  request = call,
  initialSummary,
}: {
  rememberedId: string;
  onRemember: (id: string) => void;
  request?: <T>(channel: string, args?: unknown) => Promise<T>;
  initialSummary?: PlayJukeboxSummary;
}) {
  const [summary, setSummary] = useState<PlayJukeboxSummary | null>(
      initialSummary ? validatePlayJukeboxSummary(initialSummary) : null,
    ),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  useEffect(() => {
    if (initialSummary) return;
    let live = true;
    void request<PlayJukeboxSummary>("jukebox:selection-summary")
      .then(validatePlayJukeboxSummary)
      .then((value) => {
        if (live) setSummary(value);
      })
      .catch((reason) => {
        if (live) setError(String(reason));
      });
    return () => {
      live = false;
    };
  }, [initialSummary, request]);

  const remembered = summary?.playlists.some(
    (playlist) => playlist.id === rememberedId,
  )
    ? rememberedId
    : null;
  const selectedId =
    summary?.playlistId ?? remembered ?? summary?.playlists[0]?.id ?? "";
  const enabled = !!summary?.playlistId;
  const available = !!summary?.revision && !!summary.playlists.length;

  async function setActive(next: string | null) {
    if (!summary?.revision || busy) return;
    setBusy(true);
    setError("");
    try {
      const updated = validatePlayJukeboxSummary(
        await request<PlayJukeboxSummary>("jukebox:play-set-active", {
          playlistId: next,
          revision: summary.revision,
        }),
      );
      setSummary(updated);
      if (next) onRemember(next);
    } catch (reason) {
      const message = String(reason);
      try {
        setSummary(
          validatePlayJukeboxSummary(
            await request<PlayJukeboxSummary>("jukebox:selection-summary"),
          ),
        );
      } catch {
        // Keep the original CAS/save failure as the actionable message.
      }
      setError(message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="home-startup-video play-jukebox-choice">
      <label className="home-startup-toggle">
        <span className="home-field-label" data-ui-icon="jukebox">
          <UiIcon name="jukebox" />
          Jukebox playlist
        </span>
        <input
          id="homeJukeboxEnabled"
          aria-label="Enable Jukebox playlist"
          type="checkbox"
          checked={enabled}
          disabled={!available || busy}
          onChange={(event) =>
            void setActive(event.target.checked ? selectedId : null)
          }
        />
      </label>
      <select
        id="homeJukeboxPlaylist"
        aria-label="Jukebox playlist"
        value={selectedId}
        disabled={!available || busy}
        onChange={(event) => {
          const next = event.target.value;
          onRemember(next);
          if (enabled) void setActive(next);
        }}
      >
        {!summary?.playlists.length && (
          <option value="">
            {summary ? "No saved playlists" : "Reading playlists…"}
          </option>
        )}
        {summary?.playlists.map((playlist) => (
          <option key={playlist.id} value={playlist.id}>
            {playlist.name}
          </option>
        ))}
      </select>
      {(error || busy) && (
        <small id="homePicksStatus" role="status">
          {error ? "Playlist selection needs review." : "Saving playlist…"}
        </small>
      )}
    </div>
  );
}
