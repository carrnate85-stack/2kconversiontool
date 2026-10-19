import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  JukeboxNative,
  JukeboxNativeClient,
  findJukeboxTracks,
  jukeboxJobActive,
  jukeboxKey,
  jukeboxProgress,
  jukeboxTitle,
  mergeJukeboxView,
  moveJukeboxTrack,
  validJukeboxPreview,
  validJukeboxArtwork,
  defaultJukeboxKeys,
  findMissingJukeboxArtwork,
  validateJukeboxView,
  type JukeboxJob,
  type JukeboxReply,
  type JukeboxRequest,
  type JukeboxView,
} from "./JukeboxNative";

const digest = "a".repeat(64);
const playlistId = "b".repeat(48);
function fixture(sequence = 1): JukeboxView {
  return {
    schema: "nba2k27-jukebox-view-v1",
    sequence,
    revision: digest,
    state: {
      schema: "nba2k27-rust-jukebox-library-v1",
      stockSha256: digest,
      music: {
        Tracks: [
          {
            Name: "Stock one",
            Artist: "First artist",
            Collection: 1,
            "Bank Index": 1,
            "Unique Id": 1,
          },
          {
            Name: "New Version 2 song",
            Artist: "V2 artist",
            Collection: 1,
            "Bank Index": 100,
            "Unique Id": 100,
          },
          {
            Name: "Imported song",
            Artist: "Local artist",
            Collection: 1,
            "Bank Index": 72,
            "Unique Id": 101,
          },
          {
            Artist: "Park artist",
            Collection: 5,
            "Bank Index": 2000,
            "Unique Id": 2000,
          },
        ],
      },
      playlists: [
        {
          id: playlistId,
          name: "My music",
          trackKeys: ["1:100:100", "1:72:101"],
        },
      ],
      activePlaylistId: playlistId,
      audio: { "1:72:101": { native: { sha256: digest, size: 192_044 } } },
    },
    stockKeys: ["1:1:1", "1:100:100", "5:2000:2000"],
    capabilities: {
      importAudio: true,
      importPlaylists: true,
      exportPlaylists: true,
      customPreview: true,
      stockPreview: true,
      nativePlayback: true,
      maxSessionSlots: 71,
    },
    playback: {
      phase: "selected",
      message: "My music is saved for the next Play launch.",
    },
    warnings: [],
  };
}
function job(phase: JukeboxJob["phase"] = "decoding"): JukeboxJob {
  return {
    id: "owned-job-1",
    kind: "audio-import",
    phase,
    message: "Decoding selected audio",
    done: 30,
    total: 100,
    cancellable: true,
  };
}
function requestFrom(
  fn: (channel: string, args: unknown) => unknown | Promise<unknown>,
): JukeboxRequest {
  return async <T,>(channel: string, args?: unknown) =>
    (await fn(channel, args)) as T;
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

describe("native Jukebox metadata and preview guards", () => {
  it("classifies Version 2 stock by exact identity, not the old 71-song boundary", () => {
    const view = fixture();
    expect(
      findJukeboxTracks(view, "", "stock", "main").map(jukeboxKey),
    ).toEqual(["1:1:1", "1:100:100"]);
    expect(
      findJukeboxTracks(view, "", "custom", "main").map(jukeboxKey),
    ).toEqual(["1:72:101"]);
    expect(
      findJukeboxTracks(view, "PARK", "stock", "all").map(jukeboxKey),
    ).toEqual(["5:2000:2000"]);
    expect(findJukeboxTracks(view, "1:100:100", "all", "main")).toHaveLength(1);
  });
  it("allows optional stock names and provides a readable fallback", () => {
    const track = { Collection: 5, "Bank Index": 2000, "Unique Id": 2000 };
    expect(jukeboxTitle(track)).toBe("Track 2000");
    expect(jukeboxTitle({ ...track, Artist: " Artist " })).toBe("Artist");
  });
  it("reorders by full playlist positions without mutating the original", () => {
    const keys = ["a", "b", "c"];
    expect(moveJukeboxTrack(keys, 1, -1)).toEqual(["b", "a", "c"]);
    expect(moveJukeboxTrack(keys, 1, 1)).toEqual(["a", "c", "b"]);
    expect(moveJukeboxTrack(keys, 0, -1)).toEqual(keys);
    expect(moveJukeboxTrack(keys, 2, 1)).toEqual(keys);
    expect(keys).toEqual(["a", "b", "c"]);
  });
  it("keeps progress indeterminate without a denominator and below 100 until committed", () => {
    expect(jukeboxProgress({ ...job(), done: undefined })).toBeUndefined();
    expect(jukeboxProgress({ ...job(), total: 0 })).toBeUndefined();
    expect(jukeboxProgress({ ...job(), done: NaN })).toBeUndefined();
    expect(jukeboxProgress(job())).toBe(30);
    expect(jukeboxProgress({ ...job(), done: 100 })).toBe(99);
    expect(jukeboxProgress({ ...job("completed"), done: 100 })).toBe(100);
    expect(jukeboxJobActive(job())).toBe(true);
    expect(jukeboxJobActive(job("failed"))).toBe(false);
    expect(jukeboxJobActive(job("cancelled"))).toBe(false);
  });
  it("accepts only a scoped loopback preview lease", () => {
    const token = "a1".repeat(24);
    const preview = {
      token,
      url: `http://127.0.0.1:43119/api/jukebox/preview/${token}`,
      mimeType: "audio/wav" as const,
    };
    expect(validJukeboxPreview(preview)).toBe(true);
    expect(
      validJukeboxPreview({
        ...preview,
        url: `http://[::1]:43119/api/jukebox/preview/${token}`,
        mimeType: "audio/ogg",
      }),
    ).toBe(true);
    for (const url of [
      `https://127.0.0.1:43119/api/jukebox/preview/${token}`,
      `http://localhost:43119/api/jukebox/preview/${token}`,
      `http://example.com:43119/api/jukebox/preview/${token}`,
      `http://user:password@127.0.0.1:43119/api/jukebox/preview/${token}`,
      `http://127.0.0.1:43119/api/jukebox/preview/${token}?path=secret`,
      `http://127.0.0.1:43119/api/jukebox/preview/${token}#fragment`,
      "file:///C:/Windows/secret.wav",
      "data:audio/wav;base64,AAAA",
      "javascript:alert(1)",
    ])
      expect(validJukeboxPreview({ ...preview, url })).toBe(false);
    expect(validJukeboxPreview({ ...preview, token: "short" })).toBe(false);
    expect(
      validJukeboxPreview({ ...preview, mimeType: "text/html" } as never),
    ).toBe(false);
  });
  it("rejects unsupported or malformed native snapshots before rendering", () => {
    expect(validateJukeboxView(fixture())).toBeDefined();
    const bad = fixture();
    bad.state!.music.Tracks[0].Name = {} as never;
    expect(() => validateJukeboxView(bad)).toThrow("incomplete");
    expect(() =>
      validateJukeboxView({
        ...fixture(),
        playback: { phase: "invented", message: "" },
      } as never),
    ).toThrow("unsupported status");
    expect(() =>
      validateJukeboxView({ ...fixture(), job: { ...job(), frames: -1 } }),
    ).toThrow("operation status");
    expect(() =>
      validateJukeboxView({ ...fixture(), stockKeys: ["../../song"] }),
    ).toThrow("unsupported status");
    expect(() =>
      validateJukeboxView({ ...fixture(), revision: "not-a-sha" }),
    ).toThrow("incomplete");
  });
  it("offers WPF repair and bundle refresh actions and accepts bundle jobs", () => {
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={fixture()} />,
    );
    expect(html).toContain("Repair missing tracks");
    expect(html).toContain("Refresh bundle");
    expect(
      validateJukeboxView({
        ...fixture(),
        job: { ...job(), kind: "playlist-bundle" },
      }),
    ).toBeDefined();
  });
  it("shows WPF's Use playlist in Freestyle toggle from saved settings", () => {
    const toggle = (html: string) =>
      html.match(/<input[^>]*aria-label="Use playlist in Freestyle"[^>]*>/)![0];
    const on = toggle(
      renderToStaticMarkup(
        <JukeboxNative initialView={fixture()} onFreestylePlaylist={() => {}} />,
      ),
    );
    expect(on).toContain("checked");
    expect(on).not.toContain("disabled");
    const off = toggle(
      renderToStaticMarkup(
        <JukeboxNative
          initialView={fixture()}
          freestylePlaylist={false}
          onFreestylePlaylist={() => {}}
        />,
      ),
    );
    expect(off).not.toContain("checked");
    expect(
      toggle(renderToStaticMarkup(<JukeboxNative initialView={fixture()} />)),
    ).toContain("disabled");
  });
  it("ignores out-of-order poll snapshots", () => {
    const newest = fixture(4);
    expect(mergeJukeboxView(newest, fixture(2))).toBe(newest);
    expect(mergeJukeboxView(newest, fixture(5)).sequence).toBe(5);
  });
});

describe("native Jukebox command ownership", () => {
  it("requires authoritative confirmation and never changes metadata optimistically", async () => {
    const pending = deferred<JukeboxReply>();
    let received: unknown;
    const client = new JukeboxNativeClient(
      requestFrom((channel, args) => {
        expect(channel).toBe("jukebox:rename-playlist");
        received = args;
        return pending.promise;
      }),
      fixture(),
    );
    const operation = client.perform(
      "jukebox:rename-playlist",
      { playlistId, name: "Renamed" },
      "Saving",
    );
    expect(client.snapshot().view!.state!.playlists[0].name).toBe("My music");
    expect(client.snapshot().working).toBe("Saving");
    expect(client.snapshot().notice).toBe("");
    expect(received).toEqual({
      playlistId,
      name: "Renamed",
      expectedRevision: digest,
    });
    const saved = fixture(2);
    saved.state!.playlists[0].name = "Renamed";
    pending.resolve({ kind: "state", view: saved });
    expect(await operation).toBe(true);
    expect(client.snapshot().view!.state!.playlists[0].name).toBe("Renamed");
    expect(client.snapshot().notice).toContain("launcher library");
    expect(client.snapshot().notice).not.toContain("active");
    expect(client.snapshot().working).toBe("");
  });
  it("preserves the library when the real backend rejects a codec or stale revision", async () => {
    const initial = fixture();
    const client = new JukeboxNativeClient(
      requestFrom(() => {
        throw new Error("Unsupported Windows codec; original retained");
      }),
      initial,
    );
    expect(
      await client.perform("jukebox:import-audio", { playlistId }, "Importing"),
    ).toBe(false);
    expect(client.snapshot().view).toBe(initial);
    expect(client.snapshot().error).toContain("Unsupported Windows codec");
    expect(client.snapshot().notice).toBe("");
    expect(client.snapshot().working).toBe("");
  });
  it("does not claim picker cancellation saved a playlist", async () => {
    const client = new JukeboxNativeClient(
      requestFrom(() => ({ kind: "cancelled" })),
      fixture(),
    );
    expect(
      await client.perform("jukebox:import-playlist", {}, "Choosing"),
    ).toBe(false);
    expect(client.snapshot().notice).toBe(
      "File selection cancelled; library unchanged.",
    );
    expect(client.snapshot().error).toBe("");
  });
  it("does not accept a missing mutation confirmation", async () => {
    const client = new JukeboxNativeClient(
      requestFrom(() => undefined),
      fixture(),
    );
    expect(
      await client.perform("jukebox:set-active", { playlistId }, "Saving"),
    ).toBe(false);
    expect(client.snapshot().error).toContain("no confirmation");
    expect(client.snapshot().notice).toBe("");
  });
  it("requires a running native job for an import-started response", async () => {
    const client = new JukeboxNativeClient(
      requestFrom(() => ({ kind: "started", view: fixture(2) })),
      fixture(),
    );
    expect(
      await client.perform("jukebox:import-audio", { playlistId }, "Importing"),
    ).toBe(false);
    expect(client.snapshot().error).toContain("No active native");
    expect(client.snapshot().notice).toBe("");
  });
  it("publishes actual native import progress and keeps mutations serialized", async () => {
    let calls = 0;
    const importing = fixture(2);
    importing.job = job();
    const client = new JukeboxNativeClient(
      requestFrom(() => {
        calls++;
        return { kind: "started", view: importing };
      }),
      fixture(),
    );
    expect(
      await client.perform("jukebox:import-audio", { playlistId }, "Importing"),
    ).toBe(true);
    expect(client.snapshot().view!.job!.phase).toBe("decoding");
    expect(
      await client.perform(
        "jukebox:create-playlist",
        { name: "Another" },
        "Creating",
      ),
    ).toBe(false);
    expect(calls).toBe(1);
    expect(client.snapshot().error).toContain("current import/export");
  });
  it("requests cancellation without claiming that the helper has already stopped", async () => {
    const importing = fixture();
    importing.job = job();
    const cancelling = fixture(2);
    cancelling.job = job();
    const seen: unknown[] = [];
    let next: unknown = { kind: "state", view: cancelling };
    const client = new JukeboxNativeClient(
      requestFrom((_, args) => {
        seen.push(args);
        return next;
      }),
      importing,
    );
    expect(
      await client.perform(
        "jukebox:cancel-import",
        { jobId: job().id },
        "Cancelling",
      ),
    ).toBe(true);
    expect(seen[0]).toEqual({ jobId: job().id });
    expect(client.snapshot().notice).toContain("waiting for the native helper");
    expect(client.snapshot().cancelRequested).toBe(job().id);
    const done = fixture(3);
    done.job = job("cancelled");
    next = done;
    await client.refresh();
    expect(client.snapshot().cancelRequested).toBeNull();
    expect(client.snapshot().view!.job!.phase).toBe("cancelled");
  });
  it("distinguishes a completed cancellation from file-picker cancellation", async () => {
    const importing = fixture();
    importing.job = job();
    const finished = fixture(2);
    finished.job = job("cancelled");
    const client = new JukeboxNativeClient(
      requestFrom(() => ({ kind: "cancelled", view: finished })),
      importing,
    );
    await client.perform(
      "jukebox:cancel-import",
      { jobId: job().id },
      "Cancelling",
    );
    expect(client.snapshot().notice).toContain("Native operation cancelled");
    expect(client.snapshot().notice).not.toContain("File selection");
  });
  it("rejects an older mutation reply instead of reporting a false save", async () => {
    const client = new JukeboxNativeClient(
      requestFrom(() => ({ kind: "state", view: fixture(1) })),
      fixture(4),
    );
    expect(
      await client.perform("jukebox:set-active", { playlistId }, "Saving"),
    ).toBe(false);
    expect(client.snapshot().view!.sequence).toBe(4);
    expect(client.snapshot().error).toContain("older status");
    expect(client.snapshot().notice).toBe("");
  });
  it("cannot let a slow poll revert a newly completed action", async () => {
    const oldPoll = deferred<JukeboxView>();
    const client = new JukeboxNativeClient(
      requestFrom((channel) =>
        channel === "jukebox:get"
          ? oldPoll.promise
          : { kind: "state", view: fixture(4) },
      ),
      fixture(1),
    );
    const refresh = client.refresh();
    await client.perform("jukebox:set-active", { playlistId }, "Saving");
    oldPoll.resolve(fixture(2));
    await refresh;
    expect(client.snapshot().view!.sequence).toBe(4);
  });
  it("clears a transient background refresh error after the backend recovers", async () => {
    let calls = 0;
    const client = new JukeboxNativeClient(
      requestFrom(() => {
        calls++;
        if (calls === 1) throw new Error("Temporary read failure");
        return fixture(2);
      }),
      fixture(),
    );
    await client.refresh();
    expect(client.snapshot().refreshError).toContain("Temporary read failure");
    expect(client.snapshot().error).toBe("");
    await client.refresh();
    expect(client.snapshot().refreshError).toBe("");
    expect(client.snapshot().view!.sequence).toBe(2);
  });
  it("does not erase an explicit action failure merely because polling succeeds", async () => {
    let action = true;
    const client = new JukeboxNativeClient(
      requestFrom(() => {
        if (action) throw new Error("Save was rejected");
        return fixture(2);
      }),
      fixture(),
    );
    await client.perform("jukebox:set-active", { playlistId }, "Saving");
    expect(client.snapshot().error).toContain("Save was rejected");
    action = false;
    await client.refresh();
    expect(client.snapshot().error).toContain("Save was rejected");
    expect(client.snapshot().refreshError).toBe("");
  });
  it("ignores a response after unmount/dispose", async () => {
    const late = deferred<JukeboxReply>();
    const client = new JukeboxNativeClient(
      requestFrom(() => late.promise),
      fixture(),
    );
    const operation = client.perform(
      "jukebox:set-active",
      { playlistId },
      "Saving",
    );
    client.dispose();
    late.resolve({ kind: "state", view: fixture(3) });
    expect(await operation).toBe(false);
    expect(client.snapshot().view!.sequence).toBe(1);
    expect(client.snapshot().notice).toBe("");
  });
  it("does not double-submit while a mutation is pending", async () => {
    const pending = deferred<JukeboxReply>();
    let calls = 0;
    const client = new JukeboxNativeClient(
      requestFrom(() => {
        calls++;
        return pending.promise;
      }),
      fixture(),
    );
    const operation = client.perform(
      "jukebox:rename-playlist",
      { playlistId, name: "First" },
      "Saving",
    );
    expect(
      await client.perform(
        "jukebox:rename-playlist",
        { playlistId, name: "Second" },
        "Saving",
      ),
    ).toBe(false);
    expect(calls).toBe(1);
    pending.resolve({ kind: "state", view: fixture(2) });
    await operation;
  });
  it("reports a final export only after native confirmation", async () => {
    const client = new JukeboxNativeClient(
      requestFrom(() => ({ kind: "exported", view: fixture(2) })),
      fixture(),
    );
    expect(
      await client.perform(
        "jukebox:export-playlist",
        { playlistId },
        "Exporting",
      ),
    ).toBe(true);
    expect(client.snapshot().notice).toBe("Playlist export completed.");
  });
  it("allows no library mutations before native state exists", async () => {
    let calls = 0;
    const client = new JukeboxNativeClient(
      requestFrom(() => {
        calls++;
        return fixture();
      }),
    );
    expect(
      await client.perform(
        "jukebox:create-playlist",
        { name: "New" },
        "Creating",
      ),
    ).toBe(false);
    expect(calls).toBe(0);
    expect(client.snapshot().error).toContain("Load the native music library");
  });
});

describe("native Jukebox screen", () => {
  it("uses WPF's single playlist-first song list, covers and compact actions", () => {
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={fixture()} />,
    );
    expect(html).toContain('class="card jukebox-controls"');
    expect(html).not.toContain('aria-label="Music catalog"');
    expect(html).toContain('aria-label="Selected playlist songs"');
    expect(html).toContain('aria-label="Search music"');
    expect(html).toContain("New Version 2 song");
    expect(html).toContain('class="jukebox-art-placeholder"');
    expect(html).toContain("+ Add song");
    expect(html).toContain("Batch add");
    expect(html).toContain("Find missing artwork");
    expect(html).toContain(">Edit</button>");
    expect(html).toContain(">Remove</button>");
    expect(html).toContain(">Reorder</button>");
    expect(html).toContain(">Folder</button>");
    expect(html).toContain("Import a .2kplaylist share file");
    expect(html).toContain("Save a shareable .2kplaylist");
    expect(html).toContain("My music (2)");
    expect(html).toContain("Default Playlist (Locked)");
    expect(html).toContain('aria-label="Include New Version 2 song"');
    expect(html).not.toContain("prototype session");
  });
  it("keeps next-Play selection separate from actual in-game activation", () => {
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={fixture()} />,
    );
    expect(html).toContain("Selected for next Play");
    expect(html).toContain("Library edits do not change a running game");
    expect(html).not.toContain("Game playlist active");
    const active = fixture();
    active.playback = {
      phase: "active",
      message: "Owned game files verified for this session.",
    };
    expect(
      renderToStaticMarkup(<JukeboxNative initialView={active} />),
    ).toContain("Game playlist active");
    active.playback.gameplayVerified = false;
    expect(
      renderToStaticMarkup(<JukeboxNative initialView={active} />),
    ).toContain("still needs a listening check");
  });
  it("shows held status as attention, never as a successful preparation", () => {
    const held = fixture();
    held.playback = {
      phase: "held",
      message: "Another music.iff mod is installed; left untouched.",
    };
    const html = renderToStaticMarkup(<JukeboxNative initialView={held} />);
    expect(html).toContain('role="alert"');
    expect(html).toContain("Playlist session needs attention");
    expect(html).toContain("left untouched");
    expect(html).not.toContain("Game playlist prepared");
  });
  it("has an honest loading state with no synthetic catalog or successful writes", () => {
    const html = renderToStaticMarkup(<JukeboxNative />);
    expect(html).toContain("Loading the native music library");
    expect(html).toContain("native music catalog is not loaded");
    expect(html).not.toContain("Stock one");
    expect(html).not.toContain("Game playlist active");
  });
  it("disables playback selection and preview when native capabilities are unavailable", () => {
    const unavailable = fixture();
    unavailable.capabilities.nativePlayback = false;
    unavailable.capabilities.stockPreview = false;
    unavailable.capabilities.customPreview = false;
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={unavailable} />,
    );
    expect(html).toMatch(
      /<button[^>]*disabled=""[^>]*>Use for next Play<\/button>/,
    );
    expect(html).toMatch(
      /<button[^>]*aria-label="Preview New Version 2 song"[^>]*disabled=""/,
    );
  });
  it("does not silently truncate oversized playlists or allow invalid game selections", () => {
    const oversized = fixture();
    oversized.state!.playlists[0].trackKeys = Array.from(
      { length: 72 },
      (_, i) => `1:${i + 1}:${i + 1}`,
    );
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={oversized} />,
    );
    expect(html).toContain("72 songs");
    expect(html).toContain("Nothing will be silently truncated");
    expect(html).toMatch(
      /<button[^>]*disabled=""[^>]*>Use for next Play<\/button>/,
    );
  });
  it("shows native progress and cancel without a false 100% claim during decoding", () => {
    const importing = fixture();
    importing.job = { ...job(), done: 100, frames: 480_000 };
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={importing} />,
    );
    expect(html).toContain('aria-busy="true"');
    expect(html).toContain('value="99"');
    expect(html).toContain("10.0 sec decoded");
    expect(html).toContain(">Cancel</button>");
    expect(html).not.toContain("Operation complete");
  });
  it("uses indeterminate progress when the helper has no total yet", () => {
    const importing = fixture();
    importing.job = { ...job(), done: undefined, total: undefined };
    const html = renderToStaticMarkup(
      <JukeboxNative initialView={importing} />,
    );
    expect(html).toMatch(/<progress[^>]*max="100"[^>]*>/);
    expect(html).not.toMatch(/<progress[^>]*value=/);
  });
  it("escapes user metadata and native error text", () => {
    const hostile = fixture();
    hostile.state!.playlists[0].name = "<img src=x onerror=alert(1)>";
    hostile.state!.music.Tracks[0].Name = "<script>bad()</script>";
    hostile.warnings = ["<svg onload=alert(1)>"];
    hostile.job = { ...job("failed"), error: "<script>failure</script>" };
    const html = renderToStaticMarkup(<JukeboxNative initialView={hostile} />);
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("<img src=x");
    expect(html).not.toContain("<svg onload=");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("Operation failed");
  });
});

describe("artwork lookup ownership and batch behavior", () => {
  const art = (suffix = "a") => {
    const token = suffix.repeat(48);
    return {
      token,
      url: `http://127.0.0.1:43119/api/jukebox/preview/${token}`,
      mimeType: "image/png" as const,
    };
  };
  it("never treats a remote image or arbitrary file as a trusted artwork lease", () => {
    expect(validJukeboxArtwork(art())).toBe(true);
    for (const url of [
      "https://coverartarchive.org/image.png",
      "file:///C:/cover.png",
      "data:image/png;base64,AA",
      "http://127.0.0.1:43119/arbitrary/path",
    ])
      expect(validJukeboxArtwork({ ...art(), url })).toBe(false);
    expect(
      validJukeboxArtwork({ ...art(), mimeType: "text/html" } as never),
    ).toBe(false);
  });
  it("populates the locked default list from exact stock identities only", () => {
    expect(defaultJukeboxKeys(fixture())).toEqual(["1:1:1", "1:100:100"]);
  });
  it("keeps existing covers, persists only missing covers, and releases every lease", async () => {
    const calls: { channel: string; args: any }[] = [];
    const request = requestFrom((channel, args: any) => {
      calls.push({ channel, args });
      if (channel === "jukebox:get-artwork")
        return { artwork: args.trackKey === "1:100:100" ? art("a") : null };
      if (channel === "jukebox:lookup-artwork") return { artwork: art("b") };
      if (channel === "jukebox:edit-track")
        return { kind: "state", view: fixture(2) };
      return {};
    });
    const client = new JukeboxNativeClient(request, fixture());
    const result = await findMissingJukeboxArtwork({
      keys: ["1:100:100", "1:72:101"],
      client,
      request,
      cancelled: () => false,
      progress: () => undefined,
    });
    expect(result.saved).toBe(1);
    expect(result.failures).toEqual([]);
    expect(
      calls
        .filter((c) => c.channel === "jukebox:lookup-artwork")
        .map((c) => c.args.trackKey),
    ).toEqual(["1:72:101"]);
    expect(
      calls
        .filter((c) => c.channel === "jukebox:release-artwork")
        .map((c) => c.args.token),
    ).toEqual([art("a").token, art("b").token]);
    expect(
      calls.find((c) => c.channel === "jukebox:edit-track")!.args,
    ).toMatchObject({
      trackKey: "1:72:101",
      title: "Imported song",
      artist: "Local artist",
      expectedRevision: digest,
      artworkToken: art("b").token,
    });
  });
  it("does not commit a late search after Stop and still retires its prepared image", async () => {
    let stopped = false;
    const lookup = deferred<unknown>(),
      seen: string[] = [];
    const request = requestFrom((channel) => {
      seen.push(channel);
      return channel === "jukebox:get-artwork"
        ? { artwork: null }
        : channel === "jukebox:lookup-artwork"
          ? lookup.promise
          : {};
    });
    const client = new JukeboxNativeClient(request, fixture());
    const pending = findMissingJukeboxArtwork({
      keys: ["1:72:101", "1:100:100"],
      request,
      client,
      cancelled: () => stopped,
      progress: () => undefined,
    });
    await Promise.resolve();
    await Promise.resolve();
    stopped = true;
    lookup.resolve({ artwork: art() });
    const result = await pending;
    expect(result.cancelled).toBe(true);
    expect(result.saved).toBe(0);
    expect(seen).not.toContain("jukebox:edit-track");
    expect(seen.filter((c) => c === "jukebox:get-artwork")).toHaveLength(1);
    expect(seen).toContain("jukebox:release-artwork");
  });
  it("reports partial failures, keeps progressing, and never claims a rejected save", async () => {
    let mutations = 0;
    const request = requestFrom((channel, args: any) => {
      if (channel === "jukebox:get-artwork") return { artwork: null };
      if (channel === "jukebox:lookup-artwork") {
        if (args.trackKey === "1:100:100")
          throw new Error("Rate limit; retry later");
        return { artwork: art() };
      }
      if (channel === "jukebox:edit-track") {
        mutations++;
        throw new Error("Library changed; refresh");
      }
      return {};
    });
    const client = new JukeboxNativeClient(request, fixture());
    const result = await findMissingJukeboxArtwork({
      keys: ["1:100:100", "1:72:101"],
      request,
      client,
      cancelled: () => false,
      progress: () => undefined,
    });
    expect(result.done).toBe(2);
    expect(result.saved).toBe(0);
    expect(result.failures).toHaveLength(2);
    expect(mutations).toBe(1);
  });
  it("rejects malformed artwork capabilities instead of enabling controls from truthy text", () => {
    const invalid = fixture();
    invalid.capabilities.artworkLookup = "yes" as never;
    expect(() => validateJukeboxView(invalid)).toThrow("unsupported");
  });
});
