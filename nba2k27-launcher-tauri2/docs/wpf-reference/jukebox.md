# Jukebox — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `W/` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src/`
- `VP/` = `nba2k27-version-proxy/src/`
- `ML/` = `nba2k27-mod-launcher/`
- `ART/` = `nba2k27-launcher-tauri2/artifacts/`
- `NOTES/` = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633\`

## Summary
The jukebox has four parts:
- **Library.** An app-owned song library with playlists. Imported songs, 256×256 covers and PNG previews live inside per-profile `.2kplaylist` archives under `<music>/archive-library/<profile16>/`. Recoverable drafts sit next to them.
- **Session publisher.** On Play, it writes temporary game-facing files into the mods profile, journals them, and restores them on exit: `music.iff`, wrapped native-PCM/Opus WAVs at `newaudio/jukeboxmusic/NNNNNN-…wav` slots 1–71, and borrowed cover TLDs.
- **version.dll audio policy.** The `AUDIO_*` config lines block unselected stock songs and force custom slots to the provider, with no stock fallback.
- **Freestyle redirect.** A native redirect, gated on build identity (`FREESTYLE_PLAYLIST=992`), sends Freestyle's 58 event-992 tracks to the custom slots.

The logic lives in `W/core/archive-jukebox.js`, `jukebox-manager.js`, `jukebox-session.js`, `jukebox-artwork-session.js`, `freestyle-playlist.js`, `version-proxy-session.js`, `W/main.js:1507-1680,1740-1760`, `VP/version_proxy.cpp:425-475,754-826,1087-1200`, `VP/audio_index_override.hpp`, `VP/freestyle_audio.hpp` and `VP/audio_readiness.cpp`.

The three things most likely to break in a port:
1. **The `planPlaylist` slot/ID remap.** Custom songs are stored at bank ≥72. They must be renumbered into free slots 1–71, with the Unique Id rule and the 71-cap error. The masks must also be exact: main 71, park 54 all zeros, and custom ⊆ main.
2. **Custom slots must never fall back to stock.** Unselected main/park paths must return NotFound before either lookup.
3. **Freestyle eligibility.** `custom === main`, 1–71 songs, a cryptographic Fisher–Yates sample of 58, and a native re-validation. It is only active when the per-build profile matches.

Also fragile: journaled restore with hash checks, and the readiness gate that replaces the 20 s delay.

## Key source map
| Concern | file:line |
|---|---|
| Jukebox page markup (playlist row, buttons, Freestyle checkbox, table, share note) | `W/renderer/index.html:215-253` |
| Song editor / Add-song / New-playlist modals | `W/renderer/index.html:576-635` |
| Home "Jukebox playlist" toggle + select | `W/renderer/index.html:100`; logic `W/renderer/app.js:303-320,2563-2582` |
| Cache-prepared-playlists setting | `W/renderer/index.html:481`; `W/renderer/app.js:547,2767-2772`; `W/main.js:956-964` |
| Freestyle setting default `usePlaylistInFreestyle: true` | `W/core/paths.js:22`; UI `W/renderer/app.js:546,2757-2766` |
| Renderer: rows, preview, save/dirty, activate, delete, batch add, find artwork | `W/renderer/app.js:1469-1665,1676-1761,1873-1953,1971-2333` |
| Renderer audio decode → 48k stereo PCM16 WAV | `W/renderer/app.js:1763-1815` |
| Renderer artwork → 256×256 PNG center-crop | `W/renderer/app.js:1817-1837` |
| Artwork batch (3 concurrent prepare, serial save) | `W/renderer/artwork-batch.js:1-36` |
| Playlist order (localStorage) | `W/renderer/playlist-order.js:1-63` |
| IPC handlers `jukebox:*` | `W/main.js:1507-1680` |
| Editability lock | `W/main.js:500-502` |
| Import limits (128 MB, 100 files, extensions) | `W/main.js:523-546` |
| Manager construction / library-root election | `W/main.js:1956-1992` |
| Play: jukebox prepare + readiness wiring | `W/main.js:1716,1743-1757,1795`; failure cleanup `W/main.js:1829-1851` |
| Session cleanup order | `W/main.js:2073-2095` |
| ZIP/music.iff codec, TLD codec, base manager | `W/core/jukebox-manager.js:13-448,450-1406` |
| Archive storage, migration, drafts, launchSource | `W/core/archive-jukebox.js:38-458` |
| Segmented `.2kplaylist` container | `W/core/playlist-container.js:1-62` |
| FLAC storage codec | `W/core/flac-audio.js:1-53` |
| Native Opus container / Ogg preview | `W/core/native-opus.js:1-42` |
| Native PCM16 wrapper (game-facing) | `W/core/native-pcm.js:1-44` |
| Opus encoder (hidden BrowserWindow, WebCodecs) | `W/core/opus-encoder.js:1-36`, `W/core/opus-encoder-page.js:1-37`, `W/core/opus-encoder.html:1` |
| Tag inference (ID3v1/v2, RIFF INFO, FLAC) | `W/core/audio-metadata.js:1-134` |
| Online artwork (MusicBrainz + CAA) | `W/core/artwork-lookup.js:1-230` |
| Session publish/restore journal + planPlaylist | `W/core/jukebox-session.js:1-172` |
| Session cover-slot borrowing | `W/core/jukebox-artwork-session.js:1-73` |
| Prepared playlist cache | `W/core/playlist-launch-cache.js:1-49`; use `W/core/archive-jukebox.js:409-438` |
| Freestyle eligibility + 58 sample | `W/core/freestyle-playlist.js:1-20` |
| Loader config lines, validation, readiness/delay, diagnostics loop | `W/core/version-proxy-session.js:160-300,333-400` |
| Readiness helper wrapper | `W/core/audio-readiness.js:1-15` |
| Route diagnostics JSON | `W/core/audio-route-diagnostics.js:1-44` |
| Native config parse for AUDIO_* / FREESTYLE_* | `VP/version_proxy.cpp:432-471` |
| Native path policy + Freestyle redirect | `VP/version_proxy.cpp:754-826` |
| Hook use (FileInfo / ActualOpen) | `VP/version_proxy.cpp:1087-1200` |
| Native report lines | `VP/version_proxy.cpp:2308-2326` |
| Mask helpers / slot path parser | `VP/audio_index_override.hpp:1-55` |
| Freestyle 58-track table + slot pick | `VP/freestyle_audio.hpp:1-30` |
| Freestyle build profiles | `VP/freestyle_build_profile.hpp:1-20` |
| Read-only readiness EXE | `VP/audio_readiness.cpp:1-81` |
| music-helper | `ML/music-helper/` is **empty**. Not found in WPF sources. |

---

### 1. Library (playlists, songs, storage, sharing)

#### 1. User-visible behavior
- **Navigation and header.** The nav item is "Jukebox" (`W/renderer/index.html:44`). The page title is "Jukebox" (`W/renderer/app.js:1959`). The intro text is "Build, manage, and preview your custom NBA 2K27 soundtrack." (`index.html:69`).
- **Playlist row** (`index.html:222-236`):
  - **Select.** `Playlist` select `#jukeboxPlaylistSelect`. The first option is always `Default Playlist (Locked)` with value `""`. Each other option is labelled `${name} (${trackCount})` (`app.js:1615-1616`).
  - **Buttons.** `Save`, `New`, `Rename`, `Reorder`, `Delete`, `Import`, `Folder`.
  - **Save button title:** "Save a compressed Opus playlist archive (requires launcher 0.36.42 or newer)".
  - **Dirty state.** When the selected playlist has unsaved changes, the Save label becomes `Save •` and switches to the primary style (`app.js:1567-1577`).
  - **Disabled without a selected playlist:** Delete, Rename, Batch add and Find missing artwork are disabled when the selection is Default (`app.js:1588-1596`).
- **Freestyle checkbox.** `Use playlist in Freestyle` with the small text "Applies next launch · samples 58 if your imported playlist is larger". It is checked by default (`index.html:238`).
- **Song tools** (`index.html:240-247`):
  - Buttons: `+ Add song`, `Batch add`, `Find missing artwork`.
  - Search placeholder: "Search by song or artist…".
  - The summary starts as "No soundtrack loaded."
- **Table columns** are `Use | Art | Song | Artist | Actions` (`index.html:250`). The empty state is "Install or generate music.iff in the active profile to begin." (`index.html:251`).
- **Row rendering** (`app.js:1483-1565`):
  - **Use checkbox.** Enabled only when an editable playlist is selected.
  - **Art.** Shows a ♫ placeholder until the preview is loaded. Loading uses `previewJukeboxArtwork` with 3 tries and 120 ms / 240 ms backoff (`app.js:1469-1481`).
  - **Play button text:**
    - `Missing` (disabled) when a custom file is missing.
    - `Play` / `Pause` for custom songs.
    - `In game` (disabled) for stock songs, with the title "Stock songs remain inside the game audio bank and can only be heard in NBA 2K27." (`app.js:1542-1552`).
  - **Remove** is disabled on Default (title "The Default Playlist is locked.").
  - **Empty-table messages:**
    - "No songs in this playlist match the search."
    - "This playlist is empty. Use + Add song to add its first track."
    - "No songs match this search."
    - "No jukebox tracks are available." (`app.js:1498-1501`)
  - **Summary line:**
    - Editable playlist: `N of M playlist songs shown.`
    - Default: `N of M default songs shown · Default playlist locked.` (`app.js:1562-1564`)
- **Default view.** With Default selected, the shown set is every track that is not custom: Collection ≠ 1, or Bank Index ≤ 71 (`W/core/jukebox-manager.js:868`, `isCustomTrack` at `:292-294`).
- **Selecting a playlist in the dropdown** activates it immediately (`app.js:2261-2264,2300-2318`):
  - If the playlist is dirty, it is first saved with `deferBundle:true`.
  - Then `activateJukeboxPlaylist(id|null)` runs and shows the toast "Playlist selected for your next game launch."
  - An empty playlist cannot be activated: "Add a song before this playlist can be used." (`app.js:2304`).
- **Home tab** (`app.js:303-320,2563-2582`):
  - **Toggle.** The checkbox reflects `activePlaylistId != null`.
  - **Toggle on** picks `lastPlaylistId`, falling back to the first playlist.
  - **Toggle off** activates `null`, which is Default.
  - **Dirty playlist.** Selecting a dirty playlist is refused with "Save this playlist in the Jukebox tab before selecting it here." (toggle variant: "Save this playlist in Jukebox before enabling it.").
  - **Session lock.** Both controls are locked while a session is active. The status text reads "Selections are locked while the game session is active.", or "Reading your jukebox…" while loading.
- **New / Rename modal** (`index.html:627-635`, `app.js:1701-1761`):
  - New: eyebrow `NEW PLAYLIST`, title "Name your playlist", help "It starts empty unless you choose to include the default soundtrack.", checkbox "Add all default songs", button `Create playlist`. Toast: `New empty playlist created. Save it when you finish editing.` (or `New playlist with default songs created. …`).
  - Rename: eyebrow `EDIT PLAYLIST`, title "Rename playlist", button `Save name`. Toast: "Playlist renamed. Save the playlist when you finish editing."
  - The name is limited to 1–60 printable characters (`jukebox-manager.js:1218`, `cleanText` `:300-304`). The error text is "Playlist name must be 1–60 printable characters."
  - Both actions save with `deferBundle:true`, which updates metadata only, and mark the playlist dirty.
- **Add song modal** (`index.html:598-625`, `app.js:1686-1699,2129-2205`):
  - **Fields.** Title, Artist (default `Custom`), and "Add to playlist" (defaults to the selected playlist).
  - **Song file.** Prompt text: "Choose MP3, WAV, M4A, AAC, FLAC, OGG, or Opus." After picking, Title and Artist are prefilled from tags or the filename (`W/core/audio-metadata.js:111-132`). The filename pattern `Artist - Title` wins over embedded tags.
  - **Artwork.** "Optional. PNG, JPG, WebP, or BMP; it will be center-cropped to 256 × 256."
  - **Convert & add song.**
    1. The renderer decodes the file with WebAudio and resamples it to 48 kHz stereo PCM16. The song must be at most 20 minutes and the result at most 256 MB (`app.js:1794-1815`).
    2. Artwork goes through a 256×256 PNG canvas.
    3. `importJukeboxAudioData` is called.
    4. Toast: `Added <title>[ with custom artwork] as custom bank <N>. Restart NBA 2K27 to load it.`
- **Batch add** (`app.js:1974-2034`):
  - Requires a selected playlist. Otherwise: "Create or select an editable playlist before batch importing songs."
  - First it runs `repairMissingJukeboxTracks`.
  - The file picker allows at most 100 files. Over the limit: "Choose no more than 100 songs at once."
  - Songs are processed serially. The overlay shows eyebrow `BUILDING PLAYLIST`, title "Adding your songs…", stages `Converting audio` / `Installing song`, and a count `i / n`.
  - Each song is imported with `deferBundle:true`.
  - Toasts: `Added N songs to <name>. Click Save playlist when finished.` On partial failure: `Added X of N songs. <first failure>`.
- **Find missing artwork** (`app.js:2035-2109`):
  - Only songs in the draft without `artworkPreviewAvailable` are processed. Help text: "Preparing up to 3 covers at a time. Only missing artwork is searched; existing covers are kept."
  - Concurrency is 3 for prepare; saves stay ordered and serial (`artwork-batch.js:3-33`).
  - Completion toast: `Added artwork to all N missing songs. Click Save playlist when finished.`
- **Edit song modal** (`index.html:576-596`, `app.js:1881-1953`):
  - Title and Artist (max 120 each), plus artwork "Find online" / "Choose file".
  - Artwork status text: "Custom artwork is active. Choose a replacement, or save to keep it." or "Using game artwork. Choose an image to replace it for this profile."
  - Save toast: `Song saved[ with updated artwork]. Restart the game to reload it.` Every playlist containing the song is marked dirty.
- **Remove** (`app.js:2227-2251`) saves the playlist with `deferBundle:true` right away. Toast: `<title> removed from <name>. Save the playlist when finished.` Audio is never deleted. Button title: "Remove this song from the selected playlist without deleting its audio file."
- **Save** (`app.js:2265-2299`):
  - Overlay: `SAVING PLAYLIST`, "Packaging your playlist…", "Building share files", with help "Gameplay changes are already local. This step creates the compressed files used for backup and sharing."
  - Toast: "Playlist saved and its .2kplaylist share file is ready." If there were several parts: `Playlist saved as N numbered .2kplaylist files. Keep them together when sharing.`
- **Delete** (`app.js:2319-2330`). Toast: "Playlist deleted. No audio files were removed."
- **Import** (`app.js:2110-2124`, `W/main.js:1664-1674`):
  - Dialog title "Import a shared NBA 2K27 playlist", filter `.2kplaylist`.
  - Toast: `Imported <name> with N songs[ (W unavailable stock references skipped)].`
  - Name collisions get the suffix ` (2)`, ` (3)` and so on (`jukebox-manager.js:1296-1300`).
- **Folder** opens `<music>/playlists` (organized storage) or the profile's playlist folder (`W/main.js:1675-1680`).
- **Reorder** (`W/renderer/playlist-order.js`):
  - Dialog "Playlist order" with the help text "Drag to reorder, or use the arrows. Changes save automatically." and a `Done` button.
  - Supports drag and ↑/↓ buttons.
  - The order is stored only in `localStorage['2k-launcher-playlist-order']` as an id array. It applies to `#jukeboxPlaylistSelect`, `#homeJukeboxPlaylist` and `#jukeboxImportPlaylist` through a MutationObserver.
  - It is UI-only and does not affect in-game order.
- **Share note** (`index.html:253`): "Save builds a .2kplaylist with 192 kbps Opus audio and your artwork. Stock songs remain references. Keep your original audio for lossless quality." / "Playback changes apply on the next launch."
- **Editing lock.** Every mutating IPC call throws "Close NBA 2K27 before editing or changing the active playlist. Preview playback remains available." while a launch is in progress or a jukebox session is active or busy (`W/main.js:500-502`). Preview and status calls are not locked.
- **Recovery playlist.** A playlist named `Recovered songs` with `recoveryPlaylist:true` is never shown in any menu. It cannot be activated: "This is a recovery backup, not a selectable playlist." (`W/core/archive-jukebox.js:62-70,272-276`; `ML/JUKEBOX-V47.md:3-5`).

#### 2. Timing and gating
- **First open.** `refreshJukebox()` runs the first time the Jukebox page opens, or when the jukebox is unavailable. Later visits only re-render (`app.js:1965-1966`).
- **Serialization.** All manager calls run through one promise tail (`ArchiveJukeboxManager.run`, `W/core/archive-jukebox.js:444`).
- **Settle.** Every non-preview, non-view operation is followed by `settle()`, which runs ensureRecovered, exports the recovery archive if needed, and prunes verified drafts (`archive-jukebox.js:450-457,363-368`).
- **First access (migration).**
  - Requires the game to be closed. Error: "Close NBA 2K27 once so the launcher can safely migrate your playlists into app storage." (`archive-jukebox.js:146`).
  - Progress text: "Moving the jukebox into app-managed playlist storage…".
  - If no seed `music.iff` exists, it decodes stock `music.iff` from the manifest using the vcz decoder and `data/oodle/oo2core_9_win64.dll` (`archive-jukebox.js:153-165`). Missing game root: "Select the installed NBA 2K27 game folder to initialize the jukebox."
- **Library root.** Chosen once at startup and persisted as `settings.jukeboxLibraryRoot`. When unset, the profile whose archive library has the most non-recovery playlists wins (`W/main.js:1966-1980`).
- **External tool timeouts.**
  - Decoder exec: 120 s (`jukebox-manager.js:466-472`).
  - texconv: 120 s (`:473-479`).
  - FLAC child process: 120 s (`W/core/flac-audio.js:15`).
  - Opus encode: 180 s, run serially through a module-level tail (`W/core/opus-encoder.js:5,31,34`).

#### 3. Mechanism
- **Storage roots.** `musicPath = <root>/Library/Music` in organized storage, otherwise `userData/jukebox` (`W/main.js:51`, `W/core/launcher-storage.js:20`).
- **Per-profile directory.** `<music>/archive-library/<sha256(lower(modsRoot))[0:16]>/` (`archive-jukebox.js:39-43`) contains:
  - `library.json` (schema `nba2k27-jukebox-library-v1`, `archiveStorageVersion:1`, `archiveAssets`, `artworkOverrides`, `playlists`, `activePlaylistId`, `lastPlaylistId`, `migrationFiles`, `migrationMusic`)
  - `drafts/music.iff`
  - `drafts/newaudio/jukeboxmusic/<bank6>-000000-000000-000000-000000-000000.wav`
  - `playlists/<SafeName80>-<id8>.2kplaylist`
  - `launch-ready-v1/`
  - `default-music.iff`
  - `legacy-*.{json,iff}`
- **Track identity.** `trackKey = "Collection:Bank Index:Unique Id"` (`jukebox-manager.js:288-290`). A custom track is Collection 1 with Bank > 71.
- **New custom track numbering.** The bank is the first free value ≥ 72 and < 999. The Unique Id is the first free value ≥ 130 and < 65535 (`jukebox-manager.js:969-977`).
- **New track metadata.** The cover defaults to `001_helicopter`. The `Collection:1` record is pushed to `state.music.Tracks` and appended to the target playlist's `trackKeys` (`:1002-1011`).
- **Accepted WAV.** PCM, 2 channels, 48000 Hz, 16-bit. Otherwise: "This first importer accepts PCM WAV: 48 kHz, stereo, 16-bit." Native Opus is also accepted (`jukebox-manager.js:306-337`).
- **music.iff codec.** It is a ZIP with a single `music.json`, method 8 (deflate level 9), UTF-8 flag 0x0800, max 16 MiB, at most 4096 tracks (`jukebox-manager.js:40-112`). Reading tolerates a BOM.
- **Draft music.iff.** `writeActiveMusic` writes the active playlist's tracks in **library order** (`state.music.Tracks.filter`, not `trackKeys` order), or all non-custom tracks for Default (`:819-840`). In archive mode this targets only `drafts/music.iff`.
- **Share format.**
  - **Small playlists.** A plain ZIP with `playlist.json` plus `audio/NNNN.wav|.flac`, `artwork/NNNN.tld` and `artwork/NNNN.png`.
  - **Schemas.** `nba2k27-2kplaylist-v1` (wav), `-v2` (flac), `-v3` (native-opus) (`jukebox-manager.js:16-19,563-688`).
  - **Track records.** Each is `{kind:'custom'|'stock-reference', sourceKey, metadata:{Artist,Collection,Cover Name,Name,Bank Index,Unique Id}, audioFile, artworkFile, artworkPreviewFile, audioCodec, audioNativeSha256 | audioWaveSha256+audioPcmBytes}`.
  - **Large playlists.** Parts are capped at `bundlePartBytes` (default 320 MiB, at most 384 MiB − 1 MiB) and 512 entries. Multi-part playlists become one container file: magic `2KPLAYLIST2\n`, ZIP segments, a JSON index `{schema:'nba2k27-playlist-container-v2', manifest, parts:[{offset,length,sha256}]}`, and a 24-byte footer `2KPLIDX2` + u64 offset + u64 length (`W/core/playlist-container.js:5-31`).
  - **Container limits.** Segments ≤ 384 MiB, ≤ 4096 parts, index ≤ 4 MiB.
  - **Legacy imports.** `.partNNN.2kplaylist` split sets are still importable (`jukebox-manager.js:1264-1290`).
- **Archive Save (`ArchiveProfile.exportPlaylistBundle`, `archive-jukebox.js:289-332`):**
  1. Materialize every asset whose old archive is about to be replaced.
  2. Opus-encode each custom WAV that is not already native-Opus. Progress text: `Compressing <name>: i/n — <title>`.
  3. Write the archive atomically.
  4. Read back each segment and compare SHA256 of the decoded member against the draft. On mismatch: "Saved playlist differs from its draft. No drafts were removed."
  5. Commit `archiveAssets[bank][kind] = {bundle, segment, member, hash, ...}`.
  6. Set `archiveMetadataHash`.
  7. `prune()` deletes drafts whose bytes equal the committed hash.
- **Opus encoder** (`opus-encoder.js`, `opus-encoder-page.js`):
  - Runs in a hidden sandboxed BrowserWindow using WebCodecs `AudioEncoder` with `{codec:'opus', 48000, 2ch, bitrate 192000, frameDuration 20000}`.
  - PCM is streamed in as base64 1 MiB chunks.
  - Tail packets past the declared sample count are trimmed.
  - The output is built into the native container: magic `0xBBCFA17E`, codec `0x758195F9` (both written as LE u32), a seek table, and 1275-byte max packets (`native-opus.js:2-18`).
  - The result is verified by decoding it back as Ogg in an `OfflineAudioContext`. The length must equal the frame count.
- **FLAC (v2 legacy).** `flac.exe --silent --verify -8 --no-padding --no-seektable`. Encoding is used only for canonical 44-byte-header WAVs. Each encode is round-trip verified (`flac-audio.js:25-33`).
- **Preview.**
  - `previewTrack` materializes the audio draft. A native-Opus file is converted to a sibling `.ogg` via `toOgg`. The renderer plays `file://` in an `Audio` element with `preload='metadata'` (`jukebox-manager.js:1137-1158`, `W/main.js:1624-1628`, `app.js:1640-1665`).
  - Stock songs: "Stock soundtrack preview is not available because its audio is stored inside the game bank."
- **Repair (archive mode).** It never deletes records. It returns `{removed:0, missing:N}` (`archive-jukebox.js:403-408`). The legacy non-archive `repairMissingCustomTracks` deleted records (`jukebox-manager.js:936-962`), but it is overridden.
- **Restore default (archive mode).** Same as activating `null`. Report: `{source:'Unmodified stock game soundtrack'}` (`archive-jukebox.js:398-402`).
- **Delete (archive mode).**
  1. Remove the playlist record.
  2. Move orphaned songs into the hidden recovery playlist.
  3. Re-export every remaining playlist.
  4. Unlink the deleted playlist's archive only if no asset still references it (`archive-jukebox.js:385-397`).
- **IPC limits** (`W/main.js:523-546,1527-1617`):
  - Audio file: max 128 MB. Errors: "Choose an audio file smaller than 128 MB." / `<name> must be smaller than 128 MB.`
  - Converted WAV: ≤ 256 MiB. Error: "The converted WAV is outside the safe size limit."
  - Artwork image: ≤ 20 MB. Error: "Choose an artwork image smaller than 20 MB."
  - Batch tokens are single-use and size/mtime-pinned. Errors: "This batch audio selection expired. Choose the songs again." / `<name> changed after it was selected.`

#### 4. Build profiles
The library is build-independent. Stock `music.iff` is decoded from the manifest of whatever build is installed. Version 2 has 93 Collection-1 tracks (`ART/jukebox-covers-v2-route-20261017.md:41`); see the Rust note in `src/jukebox_catalog.rs:241-242`.

#### 5. Rust must match
- [ ] Default playlist is shown as `Default Playlist (Locked)`. It is never editable, and it shows only non-custom tracks.
- [ ] A new custom track gets the first free bank ≥ 72 and the first free Unique Id ≥ 130. Its Collection is 1 and its cover defaults to `001_helicopter`.
- [ ] Import accepts only 48 kHz / stereo / 16-bit PCM (or native Opus) after conversion. Any other WAV fails with the exact WAV error.
- [ ] Remove, edit, rename and batch add mark the playlist dirty (`Save •`). Only Save writes the `.2kplaylist`.
- [ ] Selecting a dirty playlist from Home is refused with the exact text. Selecting it in Jukebox auto-saves its metadata first.
- [ ] Deleting a playlist never deletes song audio. Orphans go to a hidden recovery playlist that never appears in either dropdown and cannot be activated.
- [ ] Every mutating jukebox call during a launch or active session fails with "Close NBA 2K27 before editing or changing the active playlist. Preview playback remains available." Preview still works.
- [ ] A stock-song Play button reads `In game` (disabled). A missing custom file reads `Missing`.
- [ ] Batch add rejects more than 100 files. A partial failure still keeps the successful songs and reports `Added X of N songs.`
- [ ] A `.2kplaylist` from WPF (v1/v2/v3, ZIP or `2KPLAYLIST2` container, or legacy `.partNNN`) imports. Stock references that are not in the library become warnings, not errors.

---

### 2. In-game playback (session publish, audio policy, readiness, diagnostics)

#### 1. User-visible behavior
- **Launch stage.** Play shows `Preparing playlist audio` at progress 32 (`app.js:44`), with the message "Preparing the selected playlist for in-game playback…" (`W/main.js:1744`).
- **Per-song progress.** `Preparing song i/n: <title>` is sent when a mapping needs conversion (`W/core/jukebox-session.js:127`).
- **Loader messages.** "Waiting for verified file-manager readiness (read-only startup test)…" (`W/core/version-proxy-session.js:349`). When active: "Ready. Your selected playlist is active; unselected soundtrack songs are blocked." (`:388-390`). With Default, the text is the generic unique-assets message instead.
- **Errors (exact strings)** (`jukebox-session.js:16-36,111,129-146`, `jukebox-artwork-session.js:31`):
  - Empty playlist: "Choose a nonempty playlist before Play."
  - Non-main collection: "Custom in-game playlists currently support main soundtrack songs only. Remove tracks from other collections, or use Default Playlist unchanged."
  - More than 71 positions: "This game build currently supports 71 main soundtrack positions per active playlist. Split this playlist; your library and shared playlists are unchanged."
  - Occupied slot: "An existing audio mod occupies soundtrack position N; it was left untouched."
  - Disk: "Not enough disk space for the temporary playlist files."
  - "Close NBA 2K27 before preparing the playlist."
  - "Jukebox preparation is already running."
  - Covers: `This playlist needs N cover slots, but only M are available without replacing unrelated artwork mods. All artwork remains saved in the library.`
  - Cache tampering: "Prepared playlist cache changed. Relaunch to rebuild it."
- **Playlist order.** There is no song order or "play first" control. A startup star was requested but never enabled: there is no verified first-play mechanism (`ML/JUKEBOX-V39.md:41-50`). The published `music.iff` keeps library order (`archive-jukebox.js:420,436`).

#### 2. Timing and gating
- **Order inside `game:launch-without-eac`** (`W/main.js:1705-1795`):
  1. Refuse if `NBA2K27.exe` is running.
  2. `jukeboxSession.restore()`, together with the other restores (`:1716`).
  3. Build check and selector scan run in parallel. The jukebox step only runs **after `layout.status==='ready'`**.
  4. `launchSource` → `jukeboxSession.prepare` → `useFreestylePlaylist` (`:1746-1750`).
  5. Set `startupReadinessCheck` if `audioPolicy` exists (`:1753-1756`).
  6. Mod-profile activation (`:1759-1762`), then session monitor, camera names, scoreboards, ball.
  7. `versionProxySession.start({...audio})` (`:1784-1795`), which writes the config and launches.
- **Readiness gate** (`W/core/version-proxy-session.js:333-359,370-376`):
  - After `nba2k27_version_proxy_armed.log` appears (90 s timeout), WPF runs `NBA2K27AudioReadiness.exe <gameRoot>\NBA2K27.exe --armed-report <armed>` **instead of** the fixed 20 s delay. The default is `activationDelayMs = 20000` (`:125`).
  - Without a custom playlist (`audioPolicy` null), the 20 s delay is used. The delay polls once per second and fails if the game exits.
  - Wrapper settings (`W/core/audio-readiness.js:5-14`): execFile timeout 65000 ms, maxBuffer 32 KiB. Pass condition: `ready===true && memoryWrites===0 && gameCalls===0 && Number.isInteger(pid)`. Otherwise "Audio startup readiness did not pass."
  - The optional `--stable-ms` value must be 50–2000. Production omits it, so it defaults to 1000.
  - After readiness, WPF writes the trigger file, then waits up to 180 s for the report and requires `LIVE_SESSION_ACTIVE=YES` (`:379-386`).
- **Readiness EXE internals** (`VP/audio_readiness.cpp:118-180`):
  - Reads PID from the armed log. Requires the line `AUDIT_STATE=ARMED`.
  - Opens the process with `QUERY_LIMITED_INFORMATION|VM_READ` only. Requires the image path to match.
  - Checks a 512-byte header match and the anchor `48 8B 05 disp32` resolving to `manager`.
  - Polls every 50 ms for at most 60000 ms.
  - Ready means the manager pointer is non-null, `[manager+0x1E0] == 0xC7E442DC`, and both values are unchanged for `stable_ms` (default 1000).
  - Output: JSON `{"ready":true,"pid":..,"waitMs":..,"stableMs":..,"memoryWrites":0,"gameCalls":0}`.
- **Route diagnostics.** Sampled once at provider start, then every 2000 ms inside the 500 ms monitor loop while the game runs (`version-proxy-session.js:236-240,391-400`).
- **Restore triggers.** Restore runs at app start (`W/main.js:1958`), before every Play (`:1716`), on any launch failure (`:1838`), and in session cleanup after game exit. Cleanup runs in this order (`:2073-2095`):
  1. Video window
  2. Outfit helper
  3. Ball
  4. **Jukebox**
  5. Startup video
  6. Camera names
  7. Scoreboards
  8. Front menu
  9. Digits
  10. Ball scale
  11. **Jukebox cache**
  12. **Playlist cache**
  13. **Audio conversion cache**
  14. Mod profile
  15. Player folders
- **Restore preconditions.** Restore refuses while the game runs: "Close NBA 2K27 before restoring its playlist session." (`jukebox-session.js:63`).

#### 3. Mechanism
- **launchSource** (`archive-jukebox.js:409-438`):
  1. `settle()`.
  2. If there is no active playlist, return `{enabled:false}`. That means no session and no `AUDIO_*` lines.
  3. If `needsSave`, auto-export the playlist.
  4. Materialize artwork drafts.
  5. Get each custom song's audio through `PlaylistLaunchCache` (section 5) with concurrency 2 (`boundedWork`, `W/core/bounded-work.js:3-9`).
  6. Return `{enabled:true, playlistBytes:music.iff of selected tracks, sourceRoot:drafts, libraryPath:library.json, preparedAudio, launchCacheTimings}`.
- **planPlaylist** (`jukebox-session.js:14-41`). This is the core remap:
  1. Start with `main = 71×'0'`, `park = 54×'0'`, `custom = 71×'0'`.
  2. Every track must have Collection 1.
  3. A stock main track (Bank ≤ 71) keeps its bank and sets `main[i-1]='1'`.
  4. Each distinct custom source (Bank > 71) takes the lowest unused slot 1..71 and sets `main` and `custom` to `'1'`. A mapping `{slot, source: audioLogicalPath(bank), target: audioLogicalPath(slot), title}` is recorded.
  5. The track is rewritten with `Bank Index = slot`. `Unique Id = slot-1`, unless that ID is already used by a stock selected track; then it takes the first free ID in 0..124.
  6. Result: `{music, mappings, policy:{main, park, custom}}`. **Park is always all zeros**, which blocks every park jukebox song while a custom playlist is active.
- **Audio path format.** `newaudio/jukeboxmusic/%06d-000000-000000-000000-000000-000000.wav` (`jukebox-manager.js:296-298`).
- **Game-facing audio.** `wrapPcmWave` converts RIFF to native format (`W/core/native-pcm.js:8-43`):
  - 8-byte header: `u32 0xBBCFA17E`, `u32 46`.
  - 46-byte descriptor: `PCM16=0x063543BD`, channels, rate, `pcm.length/2` (scalar samples), 18, `pcm.length`, align, `fmt[0..16]`, `u16 0`.
  - Then the raw PCM.
  - Limit: PCM must be under 20 minutes (1200 s).
  - Native Opus passes through unchanged (`:9`).
- **prepare() write order** (`jukebox-session.js:106-170`):
  1. `restore()`.
  2. Plan.
  3. `prepareArtwork` builds private copies (section 4).
  4. Remap Cover Names onto the plan.
  5. For each mapping, refuse if the target already exists. Use the verified `preparedAudio` path, or wrap the WAV into `game-session/native-pcm-v1/<sha>.wav` (temp file, then rename; a hit touches the mtime).
  6. Check free disk ≥ total + 16 MiB.
  7. Re-hash the live `music.iff`. Refuse if it changed: "Playlist changed while preparing audio."
  8. Back up `original-music.iff` if present.
  9. **Journal `session.json`** (`schema 'nba2k27-jukebox-session-v1', modsRoot, originalHash|null, testHash, mappings[{slot,source,target,title,hash,cache}], artworks[]`). Written as tmp, then rename.
  10. Publish artwork, checking each slot hash first.
  11. Copy audio with `COPYFILE_EXCL` and verify its hash.
  12. Write `music.iff` last.
  13. On any publish error, run `restore()` and rethrow.
- **Result.** `{audioSlotCount:71, audioPolicy, preparationTimings}`. When disabled: `{audioSlotCount:0, audioPolicy:null}`.
- **Where files go.** Everything is written under `modsRoot` (the selected profile). `ModProfileManager.activate` makes that profile the game's live `<game>/mods` (`W/core/mod-profile-manager.js:73-98`).
- **Session root.** `<music>/game-session/` (`W/main.js:1956`).
- **restore()** (`jukebox-session.js:59-85`):
  1. Validate the journal strictly: at most 71 mappings, target equals the slot path, hex hashes. Otherwise "Invalid jukebox recovery journal; no files changed."
  2. The original backup hash must match.
  3. The current `music.iff` hash must equal the original or the test hash. Otherwise "Playlist changed outside the session; backup retained, no overwrite."
  4. Each temporary song must match its hash. Otherwise "Temporary song changed outside this session; it was left untouched."
  5. Put back the original `music.iff`, or delete it if `originalHash` is null.
  6. Restore artwork.
  7. Unlink the owned audio.
  8. Delete the journal.
  9. `clearConversionCache()`.
- **Link refusal.** All paths go through `plain()`, which rejects symlinks and junctions in any component: "Jukebox session refuses linked files or folders." (`:50-58`).
- **Loader config lines** (`W/core/version-proxy-session.js:290-293`):
  - `AUDIO_SLOT_COUNT=71`
  - `AUDIO_ALLOWED_MAIN=<71>`
  - `AUDIO_ALLOWED_PARK=<54>`
  - `AUDIO_CUSTOM_MAIN=<71>`
  - CRLF line endings. The total config must be ≤ 2047 ASCII bytes (`:296`).
  - JS-side validation (`:181-184`): `audioSlotCount` is 71 whenever a policy exists, and custom ⊆ main. It cannot be combined with `NBA2K27_AUDIO_INDEX_TEST`.
- **Native parse** (`VP/version_proxy.cpp:440-444,458-471`). If any mask is present:
  - All three masks must be valid ('0'/'1', exact length).
  - `AUDIO_SLOT_COUNT` must be exactly "71", with no `AUDIO_INDEX_OVERRIDE`, and custom ⊆ main.
  - Otherwise the config is rejected and the session fails closed.
- **Native path policy** (`VP/audio_index_override.hpp:19-29,41-54`; `VP/version_proxy.cpp:754-758,1092-1095,1149-1151,1114,1183`):
  - **Blocked.** A `jukeboxmusic` slot that is > 71 or not in main, or a `jukeboxmusic_park` slot that is > 54 or not in park, returns `kNotFound` immediately from both `live_file_info` and `live_actual_open`, before stock or provider. This is only active when `main_mask[0] != 0` (a policy exists).
  - **Forced to provider.** `matches_audio_slot_override`: jukeboxmusic slots 1..`AUDIO_SLOT_COUNT` are forced overrides, so the provider is asked first (`version_proxy.cpp:809`).
  - **Custom slots.** Paths in the custom mask **skip the stock fallback**: a failed custom song does not play the stock song at that position.
  - **Selected stock slots** (main=1, custom=0): asked of the provider first, then stock as fallback.
- **Native report.** Writes `LIVE_AUDIO_SLOT_COUNT=%u` and `LIVE_PLAYLIST_FILTER=ON|OFF` (`VP/version_proxy.cpp:2320-2322`).
- **Diagnostics file** `logs/jukebox-audio-route.json` (`W/core/audio-route-diagnostics.js:9-31`):
  - Schema `nba2k27-audio-route-diagnostics-v1`.
  - Keeps provider `recentActivity` rows whose path matches `/(newaudio|jukebox|\.wav$)/i`.
  - Keeps the last 120 events and sets `historyMayBeIncomplete` when the sequence gap shows the provider overflowed its 200-event history.
  - Also tracks accessory events.
  - When status is unreachable it writes `providerReachable:false`.
- **Index override (test only).** `AUDIO_INDEX_OVERRIDE=newaudio/jukeboxmusic/directory_listing.adb2` is used only when env `NBA2K27_AUDIO_INDEX_TEST=1`. The provider size must be 4312 (`version-proxy-session.js:231-232,245-248`; `VP/audio_index_override.hpp:32-40`). It is never used in production.
- **Abandoned-feature note.** The Hide What's New feature was correlated with "intermittent missing music/audio and a Freestyle loading freeze" (`ML/ABANDONED_FEATURES.md:35-42`).

#### 4. Build profiles
- **Path policy.** Path-only, so it works on any build the DLL accepts.
- **Readiness EXE.** Only two profiles:
  - Original: `6A974D13/368C3000`, anchor `0x4B451E3`, manager `0xBCE8FC8`
  - Sep 11: `6AA2013D/36558000`, anchor `0x4B47AE3`, manager `0xBCED2C8` (`VP/audio_readiness.cpp:14-17`)
  - Any other build gives "Unsupported game build". Readiness then fails and launch fails. WPF had no fallback to the delay once `audioPolicy` existed.
- **V2.** The Rust port uses a fixed 20 s settle and no AudioReadiness port. The audit calls this "accepted substitute" (`ART/wpf-version-dll-audit-20261017.md:63,163`).
- **V2 music.iff.** V2 archives `music.iff` in `0B`. Rust therefore spends one `OVERRIDE` on `music.iff` (`ART/jukebox-v2-freestyle-cover-research-20261017.md:119`).

#### 5. Rust must match
- [ ] With Default selected, Play emits **no** `AUDIO_*` lines and does not use the readiness gate. The stock jukebox plays.
- [ ] With a custom playlist: `AUDIO_SLOT_COUNT=71` plus three masks. Park is all `0`. Custom songs are renumbered to the lowest free slots 1..71. Selected stock main songs keep their own slot.
- [ ] A playlist containing any Collection ≠ 1 song stops Play with the exact "main soundtrack songs only" message. More than 71 positions stops Play with the exact 71 message, and nothing is truncated.
- [ ] An unselected stock jukebox song (main or park) is never heard in the main menu: its request returns NotFound.
- [ ] If a custom song's file is missing, the stock song at that slot does **not** play.
- [ ] After the game exits, `music.iff` in the profile is byte-identical to before. If none existed before, it is deleted. All temporary slot WAVs and artwork are removed after hash verification.
- [ ] If `music.iff` or a temporary file was changed externally during the session, restore refuses and leaves everything in place.
- [ ] Restore runs at launcher start, before Play, on launch failure, and after game exit.
- [ ] Session files are written only after the journal exists. `music.iff` is written last.
- [ ] The published `music.iff` keeps library order. There is no claim of in-game order control.

---

### 3. Freestyle playlist ("Use playlist in Freestyle")

#### 1. User-visible behavior
- **Control.** Checkbox `Use playlist in Freestyle` on the Jukebox page (`W/renderer/index.html:238`).
- **Default.** ON: `usePlaylistInFreestyle: true` (`W/core/paths.js:22`). An explicitly saved `false` is kept (`ML/JUKEBOX-V35.md:3`).
- **Change.** Saved immediately. Toasts: "Playlist in Freestyle enabled for next launch." / "Freestyle will use its stock music next launch." (`app.js:2757-2766`).
- **Mixed playlist.** If the checkbox is on and the active playlist mixes stock songs, **Play is blocked** with: "Use playlist in Freestyle currently supports imported songs without stock songs mixed in. Uncheck it in Jukebox to play a mixed playlist in the main menu only." (`W/core/freestyle-playlist.js:7-8`, thrown from `W/main.js:1750`).
- **Default playlist.** Locked Default has no policy, so the checkbox is ignored and Freestyle stays stock.

#### 2. Timing and gating
- **Decision point.** Decided once per launch, right after `jukeboxSession.prepare` (`W/main.js:1750`).
- **Sampling.** The 58-of-N sample is drawn when the config is written (`W/core/version-proxy-session.js:293`). It is fixed for the session; there is no mid-session reshuffle (`ML/JUKEBOX-V35.md:7`).
- **Lifetime.** Native lifetime is `GAME_SESSION`. The ten-minute cutoff applies only to `FREESTYLE_EVENT_TEST` (`VP/version_proxy.cpp:783-784,2325`).

#### 3. Mechanism
- **JS eligibility** (`freestyle-playlist.js:3-10`):
  - Returns false if the setting is `false` or there is no `audioPolicy`.
  - Otherwise requires `custom` to match `/^[01]{71}$/`, `custom === main` (imported songs only), and 1–71 set bits.
- **Sample** (`freestyle-playlist.js:11-19`):
  - The slot list is the custom bits.
  - If there are more than 58, a Fisher–Yates shuffle using `crypto.randomInt` runs, then the first 58 are kept.
  - The output is a 71-character mask.
- **Config lines.** `FREESTYLE_PLAYLIST=992` and `FREESTYLE_SELECTION=<mask>` (`version-proxy-session.js:293`). JS also requires `audioSlotCount===71` and that `freestyleTest` is not set (`:178-179`).
- **Native validation** (`VP/version_proxy.cpp:449-457`):
  - Value must be "992". `FREESTYLE_EVENT_TEST` must be absent.
  - `custom_mask` must be non-empty and equal to `main_mask`.
  - The selection must be a valid 71-character mask, a subset of custom, with 1–58 bits.
  - A `FREESTYLE_SELECTION` without `FREESTYLE_PLAYLIST` is rejected.
- **Redirect** (`VP/version_proxy.cpp:762-786`, `VP/freestyle_audio.hpp:9-25`):
  1. The build profile must match the live timestamp and SizeOfImage.
  2. The request must be a `newaudio/eventmusic/NNNNNN-…wav` path.
  3. In-process `ReadProcessMemory` (no calls, no patches) reads 7 anchor bytes, `event[0x150]` and `keys[992]`. It requires `keys[992]==0xFAB3211A`, `event+0x44 == 1` (enabled), `event+0x40 == 992` (index) and `event+0x14C == 34` (bank).
  4. The path's ID must be in the 58-entry `kFreestyleTracks` table. Its table position `p` selects slot `selected[p % count]`.
  5. The target path is `newaudio/jukeboxmusic/%06u-…wav`. It is served by the provider as a forced override, from the same published session audio.
- **Hook behavior.** In both FileInfo and Open, a redirected request skips the stock fallback (`VP/version_proxy.cpp:1097-1101,1114,1153-1157,1179-1183`). Opens are traced as `FREESTYLE_REDIRECT_OPEN|FAILED source= target=` and counted (`:787-795,1175-1178`).
- **Report lines.** `FREESTYLE_PLAYLIST=ON|OFF`, `FREESTYLE_PLAYLIST_LIFETIME=GAME_SESSION|DISABLED`, `FREESTYLE_SELECTION=` (`:2325-2326`).
- **Known caveats.**
  - Event 992 is not proven to be exclusive to Freestyle.
  - Event metadata and durations stay stock.
  - One silent load was reported and remains undiagnosed (`ML/JUKEBOX-V35.md:9`).

#### 4. Build profiles
- **WPF profiles** (`VP/freestyle_build_profile.hpp:9-15`):
  - Original `6A974D13/368C3000`: anchor `0x25984E`, event `0xA2908F0`, keys `0x67184A0`, bytes `48 8d 0d 9b 70 03 0a`.
  - Sep 11 `6AA2013D/36558000`: anchor `0x25994E`, event `0xA2948F0`, keys `0x671B4C0`, bytes `48 8d 0d 9b af 03 0a`.
  - An unknown build silently does not redirect.
- **V2** `6AC53765/35BDE000`, disk-proven 19/19 (`ART/jukebox-v2-freestyle-cover-research-20261017.md:33-49`): anchor `0x25C4A7`, bytes `48 8d 0d 22 23 0c 0a`, event `0xA31E7D0`, keys `0x677F930`. Live state and track membership are unproven. All 400 eventmusic IDs 4300–4699 exist, so membership may have drifted (`:51-55`).
- **Rust QA 0.50 (side session)** (`NOTES/qa050-freestyle-playlist.md`):
  - Added `native/freestyle_build_profile_v2.hpp` with the V2 tuple.
  - New DLL SHA `ABD5EA9A…7B16`.
  - `use_playlist_in_freestyle` defaults to true.
  - Emits the lines only when the V2 outfit proxy is in use.
  - Live check is still pending.

#### 5. Rust must match
- [ ] The setting defaults to ON when it is absent. An explicit false is persisted and respected.
- [ ] Setting on + mixed stock/custom playlist: Play stops with the exact WPF message. Setting off + mixed playlist: Play proceeds with no `FREESTYLE_*` lines.
- [ ] Setting on + Default playlist: no Freestyle lines and no error.
- [ ] 1–58 imported songs: the selection mask equals the custom mask. 59–71 songs: exactly 58 distinct bits, all a subset of custom, from an OS CSPRNG Fisher–Yates. The sample is new each launch, and main-menu masks are unchanged.
- [ ] The config ends with `FREESTYLE_PLAYLIST=992` and `FREESTYLE_SELECTION=`, after the `AUDIO_*` lines, and stays ≤ 2047 bytes.
- [ ] On an unprofiled build the game plays stock Freestyle music, with no crash and no error.

---

### 4. Covers / artwork

#### 1. User-visible behavior
- **Find online.**
  - Searches MusicBrainz and Cover Art Archive. The preview label reads `<artist> · <album> · MusicBrainz / Cover Art Archive`. The toast is `Found artwork for <a> - <t>. Review the preview, then save.` (`app.js:1839-1871`).
  - With artist `Custom`: "Name the artist before searching for artwork." (`W/core/artwork-lookup.js:179`).
  - No match: `No online music match was found for <a> - <t>.`
  - Match but no image: `A song match was found, but no online cover artwork is available for <a> - <t>.` (`:193,226`).
  - Busy service: "The online artwork service is busy. Wait a moment and try Find online again."
- **Choose file.** Dialog "Choose square album artwork". Accepts png/jpg/jpeg/webp/bmp up to 20 MB (`W/main.js:1583-1599`).
- **Conversion.** The image is always center-cropped (cover-fit) to 256×256 PNG on a canvas with high-quality smoothing (`app.js:1817-1837`).
- **In game.** No WPF live acceptance of in-game covers was found (`ART/jukebox-v2-freestyle-cover-research-20261017.md:115`).

#### 2. Timing and gating
- **Lookup rate.** MusicBrainz requests are serialized with 1100 ms spacing and 3 attempts. A 429/503 backs off 1200×(n+1) ms. Image fetches retry 429/503 three times with 700×(n+1) ms.
- **Network limits.** HTTPS only. Hosts are restricted to musicbrainz.org, coverartarchive.org and `*.archive.org`. Up to 5 redirects, 15 s timeout. JSON ≤ 2 MiB, images ≤ 20 MiB, content type jpeg/png/webp (`artwork-lookup.js:19-101,194-212`).
- **Caches.** Match and image LRU caches are session-only: 32 MiB / 64 entries / 10 min each. In-flight requests are shared and errors are not cached (`:130-174`; `ML/JUKEBOX-V39.md:9-14`).
- **Batch.** 3 concurrent prepares, serial saves (`W/renderer/artwork-batch.js`).
- **Session covers.** Prepared only at Play, inside `jukeboxSession.prepare` before audio (`W/core/jukebox-session.js:118`). Restored with the session.

#### 3. Mechanism
- **Library storage.**
  - PNG → `texconv -nologo -y -w 256 -h 256 -f BC1_UNORM -m 1` → DDS (must be DXT1, 256×256, 1 mip, 32768-byte payload).
  - `buildTldFromDds` writes a 32800-byte TLD: `'TLD '`, bytes 8–11 = `05 21 47 01`, u32 31 at 12, u16 256/256/1/1, u32 32768 raw, u32 32768 stored, then the interleaved BC1 (`W/core/jukebox-manager.js:391-428,1043-1093`).
  - Archive mode always stores the cover privately at `artwork-previews/<profile16>/<bank>.tld` and the preview at `.png`, with `storage:'library-v2'` and logical path `library-artwork/<bank>.tld` (`W/core/archive-jukebox.js:55-57,71-75`).
- **Session borrowing (`prepareArtwork`, `W/core/jukebox-artwork-session.js:9-51`):**
  - **Skip condition.** Skipped unless the library path, game root, and at least one selected track with a `library-v2` override all exist.
  - **Candidate slots.** Manifest rows `shared/<2hex>/<cover>.<16hex>.tld` whose `<cover>` is a stock Collection-1 / Bank 1–71 Cover Name in the library.
  - **Excluded slots.** A slot is skipped if a mod file already occupies it and it is not a launcher-managed record ("Never borrow an unrelated mod's slot"). It is also skipped if its cover name is still used by a selected track that has no new art.
  - **Copies.** All sources are read before publishing. Each is copied to `game-session/artwork-cache/<sha>.tld`. Each original slot's bytes are backed up there too.
  - **Publish.** The TLD is written as a loose file at the exact stock logical path in `modsRoot`, and the published track's `Cover Name` is set to the borrowed slot's name.
  - **Restore.** Each slot hash must equal either the new hash or the original. Otherwise "Session artwork changed outside the launcher; it was left untouched." Then the backup is restored, or the slot file is deleted if no original existed (`:52-72`).
- **Native.** There is nothing cover-specific in the DLL. WPF relied on mods files at stock paths being picked up (`ART/jukebox-covers-v2-route-20261017.md:29-35`).

#### 4. Build profiles
- **Old builds.** Code-proven only.
- **V2.**
  - **Format.** WPF's TLD format (0x21 with stored == raw, interleaved BC1) has **no stock precedent**. Stock V2 covers are planar-split BC1 (4096 endpoint dwords, then 4096 index dwords) compressed with Oodle Leviathan, with the real stored size. Oodle refuses comp == raw.
  - **Resolution chain.** `music.json` "Default Cover Art Context":"jukebox" → `alb_cvr_cont_fcb780e2.iff`, a STORED zip of 103 TXTR → `Binary` leaf `<cover>.<crc64xz(tld)>.tld` → `shared/%02x/<leaf>`.
  - **Recommended route B.** Retarget the TXTR hash, publish the TLD at a new path (a stock miss, so the provider serves it), and spend one OVERRIDE on the container (`ART/jukebox-covers-v2-route-20261017.md:5-19,37-95`).

#### 5. Rust must match
- [ ] Find online refuses artist `Custom` with the exact text, and respects the 1.1 s MusicBrainz spacing.
- [ ] Any input image becomes a 256×256 center-crop PNG preview. The stored cover is a 256×256 BC1 texture.
- [ ] Find missing artwork skips songs that already have art. One failure does not lose the other successes.
- [ ] At Play, a cover slot already occupied by an unrelated mod is never overwritten. When slots run out, Play fails with the exact `This playlist needs N cover slots…` text. (This is WPF behaviour; on V2 the route differs.)
- [ ] After exit, borrowed cover slots are byte-identical to before.

---

### 5. Prepared playlist cache ("Cache prepared playlists")

#### 1. User-visible behavior
- **Settings checkbox.** `Cache prepared playlists`, default checked, with the help text "Keep the most recently played playlist ready for faster launches. Turning this off clears only the reusable cache after the game closes." (`W/renderer/index.html:481`).
- **Toasts.** "Faster-launch playlist cache enabled." / "Playlist cache disabled. Cleanup waits until the game is closed." (`app.js:2767-2772`).
- **Validation.** A non-boolean value is rejected with "Invalid playlist cache preference." (`W/main.js:956`).

#### 2. Timing and gating
- **Clear on disable.** When the setting is saved as `false` and the game is not running, both `clearLaunchCache` and `clearConversionCache(0)` run immediately (`W/main.js:961-964`). They also run at launcher start (`:1962,1992`) and in post-game cleanup when disabled (`:2085-2091`).
- **Game running.** `clearLaunchCache` returns false and does nothing while the game runs (`W/core/archive-jukebox.js:447`).

#### 3. Mechanism
- **Launch cache** (`PlaylistLaunchCache`, `W/core/playlist-launch-cache.js`):
  - Root: `archive-library/<profile>/launch-ready-v1/`.
  - Key: the decoded-source SHA256 (`archiveAssets[bank].audio.hash`).
  - Files: `<key>.wav` holds the **already wrapped native** audio. `<key>.json` holds `{schema:1, sourceHash, hash, size}` and must be ≤ 4096 bytes.
  - Read: valid only if the schema, sizes (54 B .. 256 MiB) and full-file SHA match. Any mismatch is treated as a miss.
  - Miss: `readRef` decodes from the archive. The source SHA must equal the key ("Playlist source changed while caching.") before the file is wrapped and written.
  - After each launch, `retain(keys)` deletes verified entries that are not in the current playlist, so only the most recently played playlist stays.
  - Linked directories are refused.
- **Cache disabled.** `launchSource` calls `retain([])` and materializes drafts instead. `JukeboxSession` then wraps the audio into `game-session/native-pcm-v1/` (`archive-jukebox.js:417-422`).
- **Hit validation.** `JukeboxSession.prepare` re-validates every `preparedAudio` entry (absolute path, hex hash, 54..256 MiB size, stat, SHA) before copying (`jukebox-session.js:130-135`).
- **Conversion cache sweep.** `clearConversionCache(max=256 MiB)` sweeps only `native-pcm-v1` and `artwork-cache`, and only files named `<sha64>.(wav|tld)` whose content still matches the name. Audio keeps the newest 256 MiB; artwork keeps nothing (`jukebox-session.js:86-105`).
- **Timings.** `launchCacheTimings {hits, misses, bytes, totalMs}` and `preparationTimings {cacheHits, cacheMisses, artworkMs, audioPreparationMs, publishMs, totalMs}` are recorded into `userData/startup-timings.json` (`W/main.js:1747-1752,1827`).

#### 4. Build profiles
- **WPF.** Build-independent.
- **Rust.**
  - QA 0.43 had no equivalent (`ART/prepared-playlist-cache-gap-20261017.md:5`).
  - QA 0.47 added `jukebox_prepared.rs`. The bench measured warm 13.0 s against 16 s, unverified in game (`ART/feature-gap-inventory-20261017.md:25`).
  - Guidance: use a separate `Cache/Jukebox/Prepared` store, bind the cache key to the game build, stock hashes, playlist revision and source SHAs, and never treat `Library/Jukebox/Audio` as a cache (`ART/prepared-playlist-cache-gap-20261017.md:7-9`).

#### 5. Rust must match
- [ ] The setting defaults to ON. Turning it off while the game runs defers cleanup. Turning it off with the game closed clears only verified cache entries.
- [ ] A second launch of the same unchanged playlist reuses the wrapped audio: no decode or wrap, and hits = song count.
- [ ] A tampered cache file is detected and rebuilt, or rejected with "Prepared playlist cache changed. Relaunch to rebuild it." It is never published.
- [ ] Launching a different playlist evicts the previous playlist's cache entries.
- [ ] Library audio is never deleted by any cache clear.

---

## Known divergences in current Rust port (brief, checked read-only)
- **Readiness.** Rust uses a fixed 20 s settle (`nba2k27-launcher-tauri2/src/provider_launch.rs:969-970`). There is no AudioReadiness port.
- **Overrides.** Rust always spends an `OVERRIDE` on `music.iff` when the jukebox is enabled (`src/jukebox_launch.rs:300-306`), because V2 archives it.
- **Track count check.** Rust validates 1–71 distinct playlist keys at Play (`src/jukebox_launch.rs:283-286`). WPF validated mapped positions (stock ≤ 71 plus customs) inside `planPlaylist`. The library playlist itself was unbounded.
- **Covers.** Rust swaps custom songs to stock cover slots, with the warning "…uses stock cover slots until native artwork publication is integrated." (`src/jukebox_service.rs:871`). In-game covers are not ported.
- **Freestyle.** Wired in QA 0.50 for V2 (side session). It is live-unverified and silently off if the V2 outfit proxy is not in use (`NOTES/qa050-freestyle-playlist.md`).

## Open questions / UNVERIFIED
- **UNVERIFIED: how the profile `music.iff` and cover TLDs reached the game on old builds.** `music.iff` exists in the stock manifest even on old builds (`restoreDefault` decodes it, `W/core/jukebox-manager.js:1167`), yet WPF published a loose `music.iff` with no `OVERRIDE`. The inference is that the game's native `mods` folder loading did this, via the `ModProfileManager` link. The precedence mechanism is not documented in WPF sources.
- **In-game playback order and shuffle.** Not found in WPF sources. Only library order is written.
- **Startup "play first" star.** Requested, never implemented (`ML/JUKEBOX-V39.md:41-50`).
- **Event 992 exclusivity.** Not proven to be Freestyle-only. One silent Freestyle load remains undiagnosed (`ML/JUKEBOX-V35.md:9`).
- **music-helper.** `ML/music-helper/` is empty. If a helper existed, it is not in the sources.
- **No live proof of in-game covers on any build.** WPF's TLD format is believed invalid on V2 (`ART/jukebox-covers-v2-route-20261017.md:51-58`).
- **Readiness on an unsupported build.** If `audioPolicy` exists, WPF would fail the launch rather than fall back to the 20 s delay. This is inferred from `audio-readiness.js:12` plus "Unsupported game build" at `audio_readiness.cpp:154`.
