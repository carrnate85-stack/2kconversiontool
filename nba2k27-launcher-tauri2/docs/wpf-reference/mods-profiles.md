# Mods library, install and profiles: WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path abbreviations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- **WR** = `nba2k27-launcher-wpf\release\quality-pass-qa-20261010\App\Reference`
- **VP** = `nba2k27-version-proxy\src` (version.dll source)
- **ML** = `nba2k27-mod-launcher` (dev tree. `ML\src\core\{mod-installer,mod-import-source,mod-profile-manager,mod-tags,multi-config-manager,provider-layout-resolver,production-provider}.js` are byte-identical to WR, checked with `cmp` on 2026-10-17)
- **AUD** = `nba2k27-launcher-tauri2\artifacts\wpf-version-dll-audit-20261017.md`
- **NOTES** = `C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633`

## Summary
- A **profile** is just a folder of mods. Settings hold `modsRoot` (the active folder) and `modsProfiles[] {name,path}`. The game's own `<game>\mods` folder is always profile #1 and can't be removed (`WR\src\core\paths.js:32-64`).
- **Mods are never copied into the game at launch.** Four mechanisms work together:
  1. If the active profile isn't `<game>\mods`, that folder is renamed to `.nba2k27-mod-launcher-default-mods` and replaced with an NTFS **junction** to the profile (`WR\src\core\mod-profile-manager.js:73-127`).
  2. Player sub-folders (`char\sig\<folder>\*.iff`, `players\<folder>\*.iff`) are **hard-linked** loose into `char\sig` (`WR\src\core\player-folder-session.js:39-70`).
  3. A launcher-owned `version.dll` plus a named-pipe **provider host** serve files that are missing from the stock manifest ("stock-first"). See `VP\version_proxy.cpp:1088-1135`.
  4. The game's native mods-folder loading covers stock-listed replacements. This is implied by the profile guide text (`WR\src\renderer\index.html:322`). The engine side is **UNVERIFIED** in these sources.
- The **only** launch-time write into the user's mods folder is the multi-config selector bundle: 3 files under `char\` (`WR\src\core\multi-config-manager.js:496-523`). They persist after the session ends.
- The library page is a read-only catalog: scan, search, sort, pngNNNN bundles, tags, Open/Folder, and an **Install mods** importer that routes files by the stock manifest.
- There is **no per-mod enable/disable and no load order** in WPF. A grep for `load order|disabledMods|enabledMods|toggleMod|priority` across main.js, app.js, index.html and catalog.js found nothing. Switching profiles is the only "disable".
- **Most likely to break in a port:**
  1. Profile junction and restore ordering, and crash recovery from the marker stages.
  2. The provider index is a **snapshot taken at provider start**: it must run after the player-folder links, it skips reparse points, and it matches paths exactly and case-insensitively.
  3. Install routing rules (package-path, then manifest leaf, then loose `pngNNNN*.iff`) and the all-or-nothing issue gate.

## Key source map
| Concern | file:line |
|---|---|
| Default settings, profile normalization, validation | `WR\src\core\paths.js:4, 13-30, 32-64, 66-80` |
| Settings read/write (atomic, serialized) | `WR\src\main.js:812-845` |
| `settings:save` IPC (`modsRoot` change guard) | `WR\src\main.js:954-967` |
| Mods IPC: scan / open-item / install / save-tags / open folder | `WR\src\main.js:1043-1080, 1854-1859`; preload `WR\src\preload.js:80, 88, 94-97, 156` |
| Catalog scan and category classifier | `WR\src\core\catalog.js:4-16, 22-67` |
| Tag store | `WR\src\core\mod-tags.js:1-75`; created at `WR\src\main.js:1955` |
| Install planner and committer | `WR\src\core\mod-installer.js:1-52` |
| Import sources (files / ZIP / RAR / folder) | `WR\src\core\mod-import-source.js:1-77` |
| Profile junction manager | `WR\src\core\mod-profile-manager.js:1-163`; created at `WR\src\main.js:2046-2048` |
| Player-folder hard links | `WR\src\core\player-folder-session.js:1-73`; `WR\src\main.js:395` |
| Multi-config selector generation | `WR\src\core\multi-config-manager.js:95-563`; created at `WR\src\main.js:2001-2004` |
| Provider list (UI only) | `WR\src\core\providers.js:1-26` |
| Provider layout resolver (pre-launch build gate) | `WR\src\core\provider-layout-resolver.js:1-139`; `WR\src\main.js:764-807` |
| Provider host process + pipe client | `WR\src\core\production-provider.js:34-113`; host source `ML\provider-host\Program.cs` |
| Session loader (version.dll install, cfg, handshake, cleanup) | `WR\src\core\version-proxy-session.js:160-476` |
| Launch orchestration | `WR\src\main.js:1688-1852` |
| Session-end restore list | `WR\src\main.js:2077-2099`; launcher-exit restore `:2238-2256`; startup recovery `:2204-2207` |
| version.dll stock-first route | `VP\version_proxy.cpp:725-752, 1088-1316`; full audit in AUD rows 5-11, 22 |
| Library UI | `WR\src\renderer\index.html:191-212, 565-575`; `WR\src\renderer\app.js:623-823, 2708-2814` |
| Profiles UI | `WR\src\renderer\index.html:306-341`; `WR\src\renderer\app.js:290-298, 526-622, 2538-2706` |
| Home "Mod profile" select | `WR\src\renderer\index.html:99`; `WR\src\renderer\app.js:302-318, 2551-2562` |
| List scroll/focus preservation | `WR\src\renderer\list-view-state.js:1-22` |
| Styles | `WR\src\renderer\mod-library-polish.css:1-75`, `WR\src\renderer\profiles-polish.css:1-83` |

---

## A. Settings and profile model

### 1. User-visible behavior
- Default game root is `C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27` (`WR\src\core\paths.js:4`). Default `modsRoot` is `<game>\mods`. The default profile list is `[{name:'Default mods', path:<game>\mods}]` (`paths.js:13-19`).
- On every read and write, `normalizeModProfiles` (`paths.js:32-64`) does this:
  - Rebuilds the list as **[default `<game>\mods` (keeps a saved custom name), ...saved profiles, active `modsRoot`]**.
  - Resolves every path, dedups case-insensitively, and drops entries with empty paths.
  - Trims names to **80 chars**. Missing names fall back to the folder basename, then `'Mods'`.
- So the active folder is **always** in the list, and the game's default folder is **always first**.
- `readSettings`/`writeSettings` force `provider:'local'` and `startCameras:true` (`WR\src\main.js:825, 835`). Writes are serialized through `settingsWriteTail` and go to temp file, then fsync, then rename (`main.js:833-845`).
- `validateSettings` returns `{gameRoot, gameExe, modsRoot}` kinds plus `ready` = gameRoot dir AND `NBA2K27.exe` file AND modsRoot dir (`paths.js:66-80`).

### 2. Timing and gating
- `settings:save` refuses `modsRoot`/`gameRoot` changes while a startup-video import is busy: "Finish or cancel the video import before changing its profile or playback setting." (`main.js:955`).
- `settings:save` does **not** check whether the game is running. Profile locking during a session is done **only in the renderer** (section D).

### 3. Mechanism
- Settings JSON lives at `settingsPath()` (`main.js:822`). Keys: `gameRoot`, `modsRoot`, `modsProfiles`.

### 4. Build profiles
- Build-independent.

### 5. Rust must match
- [ ] The settings profile list always starts with `<game>\mods` (named "Default mods" unless renamed) and always contains the active `modsRoot`, even if it was never saved.
- [ ] Profile paths are deduped case-insensitively and names are capped at 80 chars.
- [ ] Changing `gameRoot` changes the default profile path to `<newGame>\mods`.

---

## B. Mod library page

### 1. User-visible behavior
- **Toolbar** (`index.html:192-200`):
  - Label "Current mod folder" plus the path (`#modsRootLabel`, shows "Not scanned" first).
  - Buttons in order: **Open folder** · **Install mods** (title "Install individual files, a whole folder, ZIP, or RAR package") · **Scan mods** (primary).
  - Search box. Placeholder "Try png10000, arena_999, english, Player…", aria-label "Search the active mod library".
  - Status text `#modResultSummary`, initially "Scan the folder to search its files."
- **Stats strip** FILES / SIZE / PLAYERS / ARENAS is present but `hidden aria-hidden` (`index.html:202-207`). `renderCatalog` still fills it (`app.js:623-633`). Don't show it.
- **Table header** (`index.html:209`): sort buttons "Mod / asset tree", "Category", "Search tags", "Size", each followed by `↕`, then a plain "Actions" column.
  - Default sort is title ascending (`aria-pressed="true" data-direction="ascending"`).
  - Clicking the active column flips direction. Clicking another column sets ascending (`app.js:2795-2803`).
  - Sort uses `localeCompare(..., {numeric:true})`. Size sorts numerically. Tags sort by their joined `", "` string. Ties break on title (`app.js:687-691`).
- **Footer**: `#modShownSummary` ("No catalog loaded") plus the fixed text "Click tags to edit · expand grouped files" (`index.html:211`).
- **Grouping into bundles** (`app.js:657-686`):
  - A file whose leaf matches `/^(png\d{4,6})(?=[_.]|$)/i` is grouped by `player:<folder lowercased>:<stem lowercased>`. All other files are singletons.
  - The base file is `<stem>.iff` if present, otherwise the first file.
  - Group title is `folder/stem`, or the relative path for singletons.
  - Group tags are the union of member tags. Group size is the sum of member sizes.
- **Row** (`app.js:701-738`):
  - Toggle: `▶`/`▼` for bundles, `•` (disabled, hidden by CSS) for single files.
  - Icon by category: Player → players, Arena → blacktop, Uniform → uniform, title starting `balls/` → ball, otherwise file.
  - Name, plus sub-line "`N` bundled files" or "Single file".
  - Category, tag button, size (`formatBytes`).
  - Actions: **Open** and **Folder** buttons for the base file.
- **Tag button** text is the joined tags, or "Add tags" (class `untagged`).
- **Click behavior**:
  - Clicking a **bundle row** or its toggle expands or collapses it.
  - Clicking a **single-file row** opens the tag editor (`app.js:768-770`).
  - Expanding inserts dependency rows `└ <relativePath>`, typed "Base asset" or "Dependency", with an empty tags cell, size, and Open/Folder. Scroll position is kept and the list is not rebuilt (`app.js:739-783`).
- **Search** is a case-insensitive substring match over `title category tags… every member relativePath` (`app.js:692-695`). It re-renders on every `input` event (`app.js:2794`).
- **Render limits**:
  - At most **5000** groups are shown. Rendering yields every 250 files while grouping and every 40 rows while rendering, and aborts if a newer render starts (`app.js:645-700`).
  - Summary text: "`X` matching mod entries · `Y` files total", or "`X` matches · showing the first 5000". Footer: "`shown` of `matching` mod entries listed" (`app.js:788-791`).
- **Empty and loading states**:
  - No catalog: "Scan the folder to build the local mod catalog."
  - No match: "No mod files match that search."
  - Empty folder: "The selected folder is empty." (`app.js:651-655, 786`).
  - The page renders rows only while `#page-mods` is active (`app.js:647`).
- **Scan mods** (`app.js:2708-2727`):
  - The button becomes disabled with "Scanning…".
  - On success, a manual scan toasts "Active mod library refreshed." (no toast at startup).
  - On failure, the summary shows "Scan failed: `<msg>`", the home setup error is set, and a manual scan toasts the error.
  - Stale results are dropped by revision and by checking `modsRoot` is unchanged.
- **Open folder** → `shell:open-mods` → `shell.openPath(settings.modsRoot)`. A non-empty error string is thrown and toasted (`main.js:1854-1859`).
- **Open / Folder** per file → `mods:open-item {fileId, action:'file'|'folder'}` (`main.js:1049-1053`):
  - The file is resolved through `resolveCatalogFile`, which needs the cached catalog with the same root. Errors:
    - "Scan the active mod library again."
    - "That mod file is no longer in the scanned library."
    - "The mod file resolved outside the active library."
    - "That mod file is no longer a plain local file." (`mod-installer.js:49-51`).
  - `folder` → `shell.showItemInFolder`. `file` → `shell.openPath`. Any other action → "Unknown mod-library open action."
  - WPF has **no extension allowlist** for Open.
- **Scroll and focus preservation** (`list-view-state.js:1-22`):
  - Context key is `JSON.stringify([root, query, sort])` (`app.js:784`).
  - Same context: scroll is restored, and focus goes back to the same `data-view-key` row and control index.
  - New context (new search, sort or root): the list scrolls to the top.
- **Profile change** clears expanded groups, renders an empty catalog, and starts a startup scan (`app.js:600-606`).
- **Layout** (`mod-library-polish.css`):
  - Grid columns `minmax(180px,1fr) 110px minmax(130px,.52fr) 68px 161px`, gap 16 (`:21`).
  - At ≤1300px, Category and Size are hidden (`:49-58`).
  - At ≤800px, the search status wraps and asset icons are hidden (`:59-65`).
  - At ≤600px, tags drop to row 2 (`:66-75`).
  - Even rows are zebra-striped. Hover and focus-within add a 3px court-colored left inset (`:30-32`).
  - The descending arrow is rotated 180° (`:27`).

### 2. Timing and gating
- Scans happen:
  - at startup and after setup is fixed (`app.js:228, 241`);
  - when the active profile changes (`app.js:600-606`);
  - on the **Scan mods** button;
  - after install (the backend rescans and returns `catalog`) and after tag save.
- There is **no file watcher**.
- `mods:scan` has no game-running gate. It reads the folder through the profile path, not the junction.

### 3. Mechanism
- `scanMods(modsRoot)` (`catalog.js:22-67`):
  - Iterative DFS with entries sorted by name. `isSymbolicLink()` entries are **skipped** and listed in `skippedLinks` (junctions count as links).
  - Limit **100000** files (`catalog.js:4`). Over the limit: "Mod catalog exceeded its 100,000 file safety limit."
  - Missing root: "The selected mods folder does not exist."
  - Each file: `{id: relative lowercased with '/', relativePath, name, category, size, modifiedMs}`. Files sorted by `relativePath`. `categories` counts.
- `classifyAsset` (`catalog.js:6-16`), checked in this order:
  - `^png\d+` → Player
  - `^(arena|s|floor|stadium)_` → Arena
  - `^(clothing|uniform|jersey|shorts)_` → Uniform
  - `^(english|spanish|french|german|italian)\.iff$` → Localization
  - `.iff|.iff.bin` → IFF
  - `dds|png|jpg|jpeg` → Texture
  - `wav|mp3|flac|ogg` → Audio
  - anything else → Other
- The main process caches the catalog in the `catalog` variable (`main.js:487`). Tags are applied by `modTagStore.apply` on every scan (`main.js:1045`).

### 4. Build profiles
- Build-independent.

### 5. Rust must match
- [ ] Files `char/x/png1234.iff` and `char/x/png1234_hair.iff` show as one row "char/x/png1234" with "2 bundled files". Expanding shows "Base asset" and "Dependency" rows without changing scroll.
- [ ] A single-file row click opens the tag editor. A bundle row click toggles expansion.
- [ ] Search matches dependency filenames and tags. More than 5000 matches shows "… showing the first 5000".
- [ ] Junctions and symlinks inside the mods folder are not listed.
- [ ] FILES/SIZE/PLAYERS/ARENAS stats are not visible.
- [ ] Open folder opens the **active profile folder**, not `<game>\mods`.

---

## C. Mod tags

### 1. User-visible behavior
- **Modal** (`index.html:565-575`):
  - Eyebrow "MOD LIBRARY TAGS". Title is the stem or the file name. Identity is the group title in mono.
  - Input "Tags" (`maxlength=500`, placeholder "Player name, team, era, project…"), with help text "Separate tags with commas. Search matches these tags and every dependency filename in the bundle."
  - Bundle notice: "`N` files are bundled under this mod entry. The tags apply to the whole bundle." or "This mod entry contains one file." (`app.js:804-806`).
  - Buttons: **Cancel**, **Save tags**.
- Opening the modal focuses and selects the input (`app.js:799-810`).
- **Closing**: Esc (`app.js:2415`), backdrop click, × button, or Cancel (`app.js:2395-2398`).
- **Saving**: the button shows "Saving…". On success it toasts "Mod tags saved." On error it toasts the error (`app.js:812-823`).
- Tags are saved against the **base file id** (`tagFileId`).

### 2. Timing and gating
- `mods:save-tags` rescans first if the cached catalog root differs from `modsRoot` (`main.js:1070-1080`).
- An unknown fileId gives "That mod file is no longer present. Scan the profile and try again."
- There is no game-running gate.

### 3. Mechanism
- Store file: `%userData%\mod-library-tags.json` (`main.js:1955`). Format: `{schema:'nba2k27-mod-tags-v1', updatedAt, entries:[{root, fileId, tags}]}` (`mod-tags.js:68-70`).
- `root` = `path.resolve(root)` with `\`→`/`, lowercased (`mod-tags.js:7-9`). So tags are **per profile folder**.
- `normalizeTags` (`mod-tags.js:11-28`):
  - Accepts an array or a comma string. Trims and collapses whitespace. Dedups case-insensitively, keeping the first spelling.
  - Errors: tag longer than **40** chars or containing control chars → "Each mod tag must contain at most 40 valid characters." More than **12** tags → "A mod file can have at most 12 tags."
- `set` (`mod-tags.js:57-72`):
  - Validates fileId: non-empty, ≤500 chars, no leading `/`, no `../` → "The mod file identity is invalid."
  - Removes the old entry and adds the new one only if non-empty (empty input clears tags).
  - Sorts entries by `root|fileId` and writes pretty JSON (not atomic).
- `apply` attaches `tags` to every catalog file for the matching root (`mod-tags.js:45-55`).

### 4. Build profiles
- Build-independent.

### 5. Rust must match
- [ ] Limits are 12 tags × 40 chars. Duplicate tags (case-insensitive) collapse. An empty save removes the entry.
- [ ] Tags are keyed by (canonical profile root, lowercased relative path of the base file). Switching profiles shows that profile's tags.
- [ ] The "Mod tags saved." toast appears and the tag button updates without a manual rescan.

---

## D. Profiles page and home profile picker

### 1. User-visible behavior
- **Page** (`index.html:306-341`). Two cards:
  - **"Saved profiles"** / "Your mod collections", with count pill `#profileCount` showing "`N` PROFILE(S)" (`app.js:595`). Contains a radiogroup list and a **New profile** button. Status text: "Choose a profile to make it active. Your selection is saved for the next Play session."
  - **"How profiles work"** / "A quick guide to keep things clear.", with an amber pill "SESSION ONLY". It shows "Currently selected profile", name (`#activeProfileName`, fallback "Active profile") and path (fallback "No profile selected."). Four check items:
    - "No merging": "Each folder remains an independent mod collection."
    - "Native game support": "The selected folder is temporarily presented at the game's normal mods path."
    - "Automatic restoration": "Your original mods folder returns after NBA 2K27 closes."
    - "Safe removal": "Remove forgets the list entry and never deletes its files."
- **Row** (`app.js:550-590`):
  - Radio `name=activeModsProfile`, bold name, small path.
  - **Edit** button for every profile except `<game>\mods`.
  - **Remove** button only when the profile is **not active** and not `<game>\mods`.
  - aria-labels: "Edit `<name>` profile" and "Remove `<name>` profile".
- **Select by radio** → `saveSettings({modsRoot})`, then refresh the presentation and toast "`<name>` selected." On error the form is re-filled and the error toasted (`app.js:2538-2550`).
- **Home "Mod profile" select** (`index.html:99`) lists every profile by name. On change it calls `saveSettings({modsRoot})` and then waits for the triggered catalog scan (`app.js:2551-2562`).
- **New profile** (`app.js:2583-2592`):
  1. Opens the folder picker titled "Choose the live mods folder" (`main.js:1018-1028`).
  2. Opens the dialog in `create` mode. If the folder is already saved: toast "That folder is already saved. Select it from the list."
- **Dialog** (`index.html:328-340`; `app.js:2599-2626`):
  - Title "New profile" or "Edit profile". "Mod folder" path with a **Change folder** button.
  - "Profile name" input (maxlength 80), defaulting to the folder basename or `'Mods'`.
  - Buttons: **Cancel** and **Create profile** / **Save changes**.
  - Cancel and Esc are blocked while saving or picking (`app.js:2628-2633`).
  - The `<game>\mods` profile can't be opened in Edit mode (`app.js:2603`).
- **Submit validation** (`app.js:2650-2692`):
  - Locked: "Profiles are locked while your game session is active."
  - Name empty or longer than 80: "Enter a profile name of 1–80 characters."
  - Folder already used by another profile: "That folder is already saved in another profile. Choose a different folder."
  - Edit of a profile that disappeared: "This profile is no longer saved. Cancel and choose another profile."
  - **Create** appends the profile **and selects it**. Toast "`<name>` created and selected."
  - **Edit** renames and/or re-points the profile. Toast "Profile updated."
  - If the edited profile is the active one and its folder changed, `modsRoot` follows it.
  - `keepDefault` branch: re-pointing the default profile appends a new profile instead. Help text: "Choosing another folder adds a separate profile and keeps the game's default folder." This branch is unreachable because Edit is blocked for the default profile.
- **Remove** (`app.js:2693-2706`) filters the profile out of `modsProfiles` and toasts "Profile shortcut removed. No mod files were deleted." There is **no confirmation dialog**, and no files are touched.

### 2. Timing and gating
- `profilesLocked()` is true when `session.phase==='game-running'`, or when the loader phase is one of `preparing-loader, launching-game, waiting-for-game, loader-armed, activating-loader, loader-active, stopping-loader` (`app.js:2594-2597`).
- While locked, these are disabled: radios, Edit, Remove, New profile, dialog save and change-folder, and the home select. The status text reads "Profiles are locked while your game session is active." (`app.js:290-298, 314`).
- The home picks status reads "Selections are locked while the game session is active." (`app.js:317`).
- Note: the `preparing-*` launch phases (selectors, profile, jukebox…) are **not** in the lock list. Only the Play button is disabled then (`state.launchStartedAt` in the `startSession` disabled expression, `app.js:217`). This is **UNVERIFIED** as a deliberate choice.

### 3. Mechanism
- Selecting a profile only writes settings. **Nothing touches the disk** until Play (section G). The junction exists only between Play and game exit.

### 4. Build profiles
- Build-independent.

### 5. Rust must match
- [ ] The default `<game>\mods` profile has no Edit or Remove button. The active profile has no Remove button.
- [ ] Remove has no confirmation, shows "Profile shortcut removed. No mod files were deleted.", and deletes nothing.
- [ ] Creating a profile selects it immediately.
- [ ] The same folder can't be saved twice.
- [ ] While the game runs (or the loader is active), all profile controls are disabled and show the lock text.
- [ ] Selecting a profile writes settings only. No junction is created until Play.

---

## E. Install mods (files, ZIP, RAR, folder)

### 1. User-visible behavior
- The **Install mods** button shows "Installing…" while running (`app.js:2804-2812`).
- If NBA2K27.exe is running, the backend throws "Close NBA 2K27 before installing mods." (`main.js:1056`).
- **Chooser** message box (`main.js:1057`):
  - type `question`, title "Install mods", message "Choose what you want to install."
  - detail "ZIP and RAR archives are inspected before extraction. A folder is scanned recursively."
  - buttons **Files or archives** / **Folder** / **Cancel** (default 0, cancel 2).
- **File picker** (`main.js:1059`):
  - title "Choose mod files, ZIPs, or RARs", multi-select.
  - filters "Mod packages" (`zip,rar,iff,bin,dds`) and "All files" (`*`).
  - Folder mode: "Choose a mod folder" (single directory).
- Cancel at any step returns `{cancelled:true}` silently.
- **Any routing issue → nothing is installed.** Toast (error): "Nothing installed. `name`: `reason` · …". It lists up to 3 issues, then adds " · More files were rejected." (`app.js:2808`).
- **Replacements** need **one** all-or-nothing warning box (`main.js:1065`):
  - title "Replace existing mod files?"
  - message "`N` destination already contains a mod." or "`N` destinations already contain mods."
  - detail "The package was routed from its verified folder structure, the stock manifest, and safe player-file rules. Replaced files cannot be recovered by the launcher."
  - buttons **Cancel** (default) / **Replace and install**. Cancel returns `cancelled`.
  - There is **no per-file Keep option** in WPF.
- **Success toast**: "Installed `N` mod file(s) · replaced `R` · ignored `I` non-mod file(s)." The table re-renders from the returned catalog (`app.js:2809`).

### 2. Timing and gating
- The game-running check happens once, at the start (`main.js:1056`).
- The plan is computed after extraction. The commit re-verifies every source (size + mtime) and destination (existence, size, mtime) before renaming (`mod-installer.js:41-43`).
- tar listing and extraction timeout is **120000 ms** with maxBuffer 32 MB (`mod-import-source.js:12`).
- Staging is always cleaned up in `finally` (`main.js:1067`).

### 3. Mechanism
**Sources** (`mod-import-source.js:55-76`):
- Creates temp dir `%TEMP%\2k-mod-import-<uuid>-XXXXXX` (`:58`).
- **Folder mode**: exactly one folder ("Choose one mod folder at a time."). `collectFolder(..., {includeRootName:true})`, so package paths start with the folder's own name (`:61-63`).
- **Files mode**:
  - `.zip`/`.rar` → `inspectArchive`, then `tar --no-same-owner --no-same-permissions -xf <file> -C <temp>\archive-N`, then `collectFolder(output)` (`:66-69`).
  - Any other file → `{source, relativePath:null, package:false}`.
- No sources at all: "No files were found in this mod package."

**Archives** (`mod-import-source.js:22-34`):
- **Both ZIP and RAR** use Windows `%SystemRoot%\System32\tar.exe` (bsdtar/libarchive) (`:11`).
- Pre-checks:
  - plain local file ("The selected archive must be a plain local file.")
  - size 1 B–4 GB ("The selected archive exceeds the 4 GB safety limit.")
- Runs `-tf` and `-tvf` in parallel. Checks:
  - non-empty ("The selected archive is empty.")
  - ≤4096 entries ("The archive exceeds the 4,096 entry safety limit.")
  - every entry `safeEntry` (no absolute path, drive letter, `.`/`..`, `:` or control chars → "The archive contains an unsafe path.")
  - no case-insensitive duplicates ("The archive contains duplicate Windows paths.")
- The verbose listing must show only `-`/`d` types ("Archive links and special entries are not supported."), parse a size per line, and total ≤4 GB ("The archive expands beyond the 4 GB safety limit."). The record count must equal the `-tf` count (`:13-21, 32`).
- tar failure: "Could not read this archive: `<last stderr line>`" (`:12`).

**collectFolder** (`mod-import-source.js:35-54`):
- Rejects any symlink or junction ("Mod packages cannot contain linked files or folders.") and non-plain entries.
- **Skips 0-byte files**.
- Limits: ≤4096 files and ≤4 GB total.
- Each file becomes `{source, relativePath: packagePath, package:true}`.

**Planner** (`mod-installer.js:20-38`):
- Dedups sources. Needs 1–4096 ("Choose between 1 and 4096 mod files.").
- Mods root must be a plain directory with no link anywhere in its path chain:
  - "The active mods folder is not a plain local directory."
  - "Mod installation refuses linked folders." (`:22`, `plainChain` at `:11`)
- Loads a **leaf index of the stock `<game>\manifest`** (`:12-19`):
  - Each line is `logical,…`. Maps lowercased leaf → logical paths. `roots` = the set of first path segments.
  - Limits 512 MB / 2,000,000 entries. Missing manifest: "The selected game folder has no usable stock manifest."
- **Per file, routing in priority order** (`:29-30`):
  1. **package-path**: name ends `.iff|.bin|.dds` and its `relativePath` has a segment equal to a manifest root. The logical path is that segment onward (`rootedPackageLogical`, `:10`).
  2. **manifest**: leaf matches exactly one manifest path. With several matches, narrow by the source's parent folder name equal to the manifest's parent folder; this must leave exactly one (`manifestMatchForSource`, `:7`).
  3. **player-char-sig**: zero manifest matches and the name matches `/^png\d{4,6}(?=[_.]|$).*\.iff$/i` → `char/sig/<name>` (`:8`).
  4. Otherwise:
     - A package file that isn't `.iff` → **ignored** ("Package documentation or preview file.").
     - Everything else → **issue**: "Filename is ambiguous in the stock manifest." or "Filename is not listed in the stock manifest and is not a recognized PNG player file."
- Other issues:
  - "File no longer exists."
  - "Source must be a plain local file."
  - "File selection exceeds the installer size limit." (each 1 B–2 GB, total ≤4 GB)
  - "Resolved destination is outside the mods folder."
  - "Another selected file resolves to the same destination."
  - "Destination is not a plain file." (`:26-33`)
- Nothing installable at all → issue `{name:'Package', reason:'No installable mod files were found.'}` (`:36`).
- Result: `{root, items[{source,name,logicalPath,destination,routing,category,size,modifiedMs,existing}], issues, ignored, totalBytes, replacements}`.

**Commit** (`mod-installer.js:39-48`):
- Refuses if there are issues ("Resolve every mod routing issue before installing.") or unconfirmed replacements ("Existing mod files require replacement confirmation.").
- **Phase 1**: for each item, re-check the source (else "`name` changed after it was selected."), `mkdir -p`, re-check the destination parent chain ("Mod installation refuses linked destination folders."), then copy to `.mod-install-<uuid>.tmp` with `COPYFILE_EXCL`. On any failure, unlink all temp files.
- **Phase 2**: for each item, re-check the destination ("`logicalPath` changed while the install was being prepared."), rename the existing file to `.mod-install-<uuid>.bak`, then rename temp → destination. On any failure, roll back in reverse: unlink the new file and rename the `.bak` back.
- **Finally**: unlink leftover temp files. After success, unlink the `.bak` files.
- Returns `{installed, replaced, items}`. The IPC adds `ignored`, `archives`, `folders` and a fresh `catalog` (`main.js:1066`).

### 4. Build profiles
- Build-independent. It needs only `<game>\manifest`.

### 5. Rust must match
- [ ] With the game running, Install shows "Close NBA 2K27 before installing mods." and opens no dialog.
- [ ] A ZIP containing `MyMod\char\sig\png1234.iff` + `readme.txt` installs to `<profile>\char\sig\png1234.iff`, ignores the readme, and toasts "Installed 1 mod file · ignored 1 non-mod file."
- [ ] A loose `png99999_hair.iff` that isn't in the manifest routes to `char/sig/`.
- [ ] One unroutable file blocks the whole install with "Nothing installed. …".
- [ ] Existing destinations trigger exactly one "Replace existing mod files?" prompt. Cancel installs nothing.
- [ ] RAR and **any ZIP compression that bsdtar supports** extract. A missing `tar.exe` gives a clear error.
- [ ] Archives with `..`, absolute or drive paths, links, more than 4096 entries or over 4 GB are rejected before extraction.
- [ ] A failure mid-commit leaves every pre-existing destination byte-identical.

---

## F. Multi-config player selectors (runs at every Play)

### 1. User-visible behavior
- There is no page. The launch overlay shows step "Scanning selected profile" (22%) with message "Scanning the selected profile and preparing player selectors…" (`main.js:1728`; `app.js:42`).
- Status messages (emitted to `onStatus`; the WPF constructor passes none, so they are mostly invisible):
  - "Scanning unique player config sets…"
  - "Preparing local stock selector tables…"
  - "Building selectors for N unique player(s)…"
  - "Reused verified player config cache."
  - "Prepared selectors for N unique players and synchronized R replacement players."
  - "No eligible unique multi-config players were found." (`multi-config-manager.js:388-551`)
- **If a foreign selector mod is present, Play fails**: "A different `signatureconfigs_all.iff|signatureconfigs_editplayer.iff|fpplayer_global.iff` mod is installed; it was left untouched." (`:514-516`). The error propagates through `Promise.all` and the launch is aborted (`main.js:1739, 1829-1850`).

### 2. Timing and gating
- Runs in parallel with the provider-layout check, **before** profile activation (`main.js:1722-1743`). Always `enabled:true`.
- The cache is reused when all of these hold (`:345-367, 401-405`):
  - state `cacheVersion 5` with the same stockKey and the same mods inventory fingerprint
  - same `modsRoot` and a valid generationKey
  - every installed file still matches its sha256, or is an archive-equivalent of it
- **stockKey** = sha256 of `{exeSize, exeModifiedMs, manifestSize, manifestModifiedMs}`, first 24 hex chars (`:395-400`).
- **fingerprint** = sha256 of the inventory of `png*_config_*.iff` (path, size, mtime) **plus the sha256 of each base `pngNNNN.iff` that has configs** (`:95-147`).

### 3. Mechanism
- Scans the profile for:
  - base files `/^png(\d{4,5})\.iff$/i`
  - config files `/^png(\d{4,5})_config_([A-Za-z0-9_-]+)\.iff$/i` (`:117-124`)
- Reads the stock manifest (4-column lines) for:
  - `char/signatureconfigs_all.iff`, `char/signatureconfigs_editplayer.iff`, `char/fpplayer_global.iff`
  - stock base ids `char/sig/png(\d{5}).iff` and stock config groups (`:187-221`)
- Decodes those stock tables with the VCZ decoder `(archivePath, offset, size, oo2core_9_win64.dll, out)`, timeout 120 s. Then unzips with PowerShell `System.IO.Compression.ZipFile`, cached under `%userData%\multi-config\stock\<stockKey>` (`:223-237, 318-343`).
- Classification per modded id (`:427-449`):
  - Stock id with base, all-table record and both edit records → **replacement**. Its `appearance_info.json` accessory configs (≤9 names, all present as files, defaults declared) rewrite the config lists. Otherwise skip `replacement-layout-unverified`.
  - Other stock ids → skip `stock-player`.
  - No base → `base-png-missing`. Under 2 configs → `single-config`. Over 9 → `over-nine-config-limit`.
  - No stock donor with the **exact same suffix set** → `no-exact-stock-style-set`.
  - Otherwise → **eligible** with that donor.
- Builds the cloned tables (`buildTableWithClones`, `replaceConfigLists`). Headband partitioning: the first ordinary config is the base; the headband list = base + headband configs (`:149-158`). It zips them and patches `NNNNN_modified.json` in `fpplayer_global` (`:262-290`).
- Output goes to `%userData%\multi-config\generated\<generationKey>\`. Keeps the 2 newest generations (`pruneGeneratedConfigs`, `:248-260`).
- **Installs by copy** into `<modsRoot>\char\{signatureconfigs_all,signatureconfigs_editplayer,fpplayer_global}.iff` (`:496-523`). Records sha256 in `%userData%\multi-config\state.json` (schema `nba2k27-multi-config-v1`, `:529-542`).
- If nothing is eligible, `disable()` removes previously owned files whose hash still matches (`:457-464, 554-562`).
- The loader then gets `SELECTOR_OVERRIDE=1` (`main.js:1793`). The session verifies all 3 files are 1 B–16 MB and visible through the provider with equal size (`version-proxy-session.js:247-254`). The DLL forces those 3 paths to the provider (AUD row 11).

### 4. Build profiles
- Table formats are tied to the original and Sep11 builds. For V2, see `nba2k27-launcher-tauri2\src\selector_*.rs`. Parity is UNV (`nba2k27-launcher-tauri2\artifacts\parity-score-20261017.md:49`).

### 5. Rust must match
- [ ] A foreign selector bundle in the profile makes WPF **fail Play** with "A different … mod is installed; it was left untouched." Rust deliberately differs (see divergences). Record the decision.
- [ ] Generated selector files are written into `<profile>\char\` and survive game exit. A second Play with unchanged mods reuses them without regenerating.
- [ ] Editing a base `pngNNNN.iff` with configs (even with the same size and mtime) invalidates the cache through its sha256.

---

## G. What happens at Play (order matters)

### 1. User-visible behavior
- Overlay steps and percentages (`app.js:40-54`):

  | Phase | % | Step text |
  |---|---|---|
  | preparing-launch | 8 | "Checking game build" |
  | preparing-selectors | 22 | |
  | preparing-jukebox | 32 | |
  | preparing-profile | 38 | "Activating mod profile" |
  | preparing-session | 50 | |
  | preparing-names | 58 | |
  | preparing-loader | 66 | "Starting local asset loader" |
  | launching-game | 78 | |
  | waiting-for-game | 86 | |
  | loader-armed | 91 | |
  | activating-loader | 96 | "Finishing unique assets" |
  | loader-active | 100 | "Ready" |

- Final active message: "Unique assets are active. Stock files stay stock except explicitly enabled launcher assets such as camera names." (`version-proxy-session.js:387-389`).
- Complete message: "NBA 2K27 closed. The local provider and session files were restored cleanly." (`version-proxy-session.js:309`).

### 2. Timing and gating (exact order, `main.js:1688-1852`)
1. Guards:
   - native host present
   - no video import busy
   - no launch already in progress ("Game launch is already in progress.")
   - NBA2K27.exe not running ("Close NBA 2K27 before starting a new session.")
2. Restore leftovers from earlier sessions: presentation, **`playerFolders().prepare(modsRoot)`** (restores the old journal, then creates new hard links), jukebox, camera names, scoreboards, high-school probe, ball scale (`:1713-1720`).
3. **In parallel**: `refreshProviderLayout` (resolver exe, 30 s timeout, cached per exe path+size+mtime) and `multiConfigManager.prepare`. If the layout status isn't `ready`, the launch fails with "The current NBA 2K27 build is not safely resolved: `<msg>`" (`:1721-1743`).
4. Jukebox prepare (`:1745-1757`).
5. **`modProfileManager.activate({gameRoot, modsRoot})`**, timed as `profileActivationMs` (`:1759-1762`).
6. Session monitor, camera names, scoreboards, ball (`:1764-1780`).
7. `versionProxySession.start(...)` (`:1785-1824`). Inside it, in this order:
   - integrity-check the bundled DLL
   - refuse a foreign `version.dll` ("A different version.dll is installed. Nothing was overwritten.")
   - pick the **preflight asset**: `char/sig/png10000.iff` if it is 1 B–64 MB, else the first BFS file in that size range. If none: "The mods folder needs at least one file from 1 byte to 64 MB for the safe startup check." (`version-proxy-session.js:94-117`)
   - **start the provider host on `modsRoot`** (indexes now)
   - query the preflight, selectors, overrides and scoreboard layouts through the provider and compare sizes
   - write `nba2k27_provider_audit.cfg` (≤2047 bytes ASCII, `PIPE/PATH/SIZE/SHA256/...`)
   - copy `version.dll` into the game root
   - helper arms
   - Steam launch without EAC (`version-proxy-session.js:215-301`)
8. Background handshake: wait up to **90 s** for the armed log → readiness (audio-readiness exe when a playlist is set, else **20 s** delay) → write trigger → wait up to **180 s** for the report, which must contain `LIVE_SESSION_ACTIVE=YES` (`version-proxy-session.js:375-390, 335-360`).
9. **On any launch error**, everything is undone, including `modProfileManager.restoreIfSafe()` and `playerFolders().restore()` (`main.js:1828-1850`).

- **Session end** (`version-proxy-session.js:442-476`):
  1. Wait for confirmed exit. Exit is confirmed by the native presence handle, or by **3 fresh absent checks spanning ≥1 s** (`:405-433`).
  2. Archive logs, stop the provider (`STOP\n` on stdin, 5 s, then kill), delete session files and the DLL if it is a known hash.
  3. `onCleanup` steps in order (`main.js:2079-2094`): startup video, outfit helper, ball, jukebox, startup video, camera names, scoreboards, front menu, digits, ball scale, caches, **Mod profile**, **Player folders**.
  4. Each step first re-checks that the game is not running (`GAME_SESSION_ACTIVE` → retry every 500 ms).
- **Launcher startup** runs `modProfileManager.restoreIfSafe()` and `playerFolders().restore()` before init completes (`main.js:2204-2207`). A failure shows idle message "Mod-profile recovery needs attention: `<msg>`".
- **Launcher exit** runs the same restores with `gameStopped` asserted (`main.js:2238-2256`).

### 3. Mechanism: how mods reach the game without copying
**(a) Profile junction** (`mod-profile-manager.js`):
- Marker: `%userData%\active-mod-profile.json`, schema `nba2k27-session-mod-profile-v1` (`:6`; `main.js:2047`).
- Backup name: `<game>\.nba2k27-mod-launcher-default-mods` (`:7`).
- `activate` (`:73-127`):
  1. If a marker exists, run `restoreIfSafe` first. If the game is running: "Close NBA 2K27 before changing the active mod profile."
  2. **If the selected folder == `<game>\mods`, return `{active:false}`. No junction.**
  3. Game running → the same error.
  4. Selected folder missing → "The selected mod-profile folder does not exist."
  5. Selected folder inside `<game>\mods` → "A mod profile cannot be stored inside the live mods folder. Choose a sibling such as modsfiba."
  6. Live `mods` is a link or a non-folder → "The game mods path is already a link or non-folder and was left untouched."
  7. Backup already exists → "`.nba2k27-mod-launcher-default-mods` already exists. The launcher will not overwrite it."
  8. Write marker `stage:'preparing'` with `originalIdentity {dev, ino, birthtimeNs}` (atomic: tmp `wx` + fsync + rename, `:58-66`).
  9. Rename `mods` → backup. Marker `moved`.
  10. `fs.symlink(selected, liveMods, 'junction')`. Marker `active`.
  11. On error, roll back through `restoreIfSafe`. Messages: "The selected mod profile could not be activated safely: …" or "Profile activation stopped: … Automatic rollback also stopped safely: …".
- `restoreIfSafe` (`:129-160`):
  - No marker → `not-active`. Game running → `{restored:false, reason:'game-running'}` (not an error).
  - Live is a directory, the backup is missing and the live identity equals the original → just delete the marker (crash before the move or after the restore).
  - Backup missing or changed → throw "The preserved default mods folder is missing or changed; recovery left the live folder untouched."
  - Live is a link → its realpath must equal the selected folder, else "The live mods link no longer points to the recorded profile; it was left untouched." Then marker `restoring` and `rm` the link.
  - Live is something else (not missing) → "The live mods path changed during the session; it was left untouched."
  - Rename backup → `mods` and delete the marker.

**(b) Player-folder hard links** (`player-folder-session.js:39-70`):
- In the **real** profile root, every `.iff` in a sub-folder of `char\sig\*\` and `players\*\` is `fs.link`ed (hard link) into `char\sig\<name>`.
- A loose file with the same name already in `char\sig` wins (counted as `loosePriority`).
- The same name in two folders throws "Player-folder conflict: `X` is in both "`A`" and "`B`". Keep one copy or put your preferred copy loose in char/sig."
- Limit: 10000 files.
- Journal: `%userData%\player-folder-session.json` `{schema:1, root, links:[{name, identity dev:ino}]}`, written **before** linking.
- Restore unlinks only files whose identity still matches.

**(c) Provider host + version.dll** (stock-first missing-asset route):
- Host `NBA2K27ProviderHost.exe` is started with stdin JSON `{pipeName, token(32 random bytes hex), modsRoot, aliases:{}, mutableAliases:[]}`. It must print a `{"ready":true,...}` line within **30 s** (`production-provider.js:7, 34-73`).
- Index (`ML\provider-host\Program.cs:253-276`):
  - recursive `EnumerateFiles` with `AttributesToSkip = ReparsePoint`
  - key = exact relative path with `/`, **OrdinalIgnoreCase**
  - max 100000 assets, built **once at start**
- Pipe server: 4 workers, `PipeOptions.CurrentUserOnly` (`Program.cs:108-117`).
  - JSON ops (`status/query/open/read/close`) need the session token.
  - Binary frames (magic `2BP1` request / `2BR1` response, 24/32-byte headers, ≤1 MiB reads, `Program.cs:646-660`) are authorized by **client PID → NBA2K27.exe of an exact build** (`Program.cs:378-401, 404-408`).
- The DLL's `live_file_info` (`VP\version_proxy.cpp:1088-1135`), in order:
  1. playlist policy may force NotFound
  2. forced overrides (`OVERRIDE*`, selector, scoreboard, audio slots) ask the provider first
  3. otherwise the **stock device** is tried
  4. only if stock fails does it call `query_dynamic_provider`
- So **stock-listed paths stay stock**, and only manifest-missing paths come from the profile.
- Leaf-only requests that equal the preflight leaf map to the preflight path (`:741-745`). Device registration, vtable slots and the default-device swap at `+0x1E0` are in AUD rows 7-9.

### 4. Build profiles
- Profile junction, player links, install and tags are build-independent.
- Provider host binary authorization accepts only TimeDateStamp/SizeOfImage `0x6A974D13/0x368C3000` and `0x6AA2013D/0x36558000` (`Program.cs:404-408`). These are the two WPF-era builds. Which label each maps to is **UNVERIFIED** here.
- The provider-layout resolver is signature-based with no profiles (AUD row 5). V2 uses Rust's own provider host (`nba2k27-launcher-tauri2\src\provider_*.rs`). Mod loading on V2 is LIVE (`nba2k27-launcher-tauri2\artifacts\parity-score-20261017.md:38-39`).

### 5. Rust must match
- [ ] With the active profile == `<game>\mods`, Play creates no junction and no backup folder.
- [ ] With profile `D:\modsfiba`, after Play `<game>\mods` is a junction to `D:\modsfiba` and `<game>\.nba2k27-mod-launcher-default-mods` holds the original. After game exit both are back and the marker is gone.
- [ ] If the backup name already exists, Play fails with the exact WPF message and changes nothing.
- [ ] A profile inside `<game>\mods` is refused with the "Choose a sibling such as modsfiba." message.
- [ ] A crash at any marker stage (`preparing`/`moved`/`active`/`restoring`) recovers on the next launcher start without data loss. Recovery never runs while the game is running.
- [ ] A foreign or retargeted link is left untouched, with the matching message.
- [ ] Player sub-folder files are hard-linked (not copied) into `char\sig` **before** the provider indexes. They are removed at session end only if their identity is unchanged. Duplicate names across folders fail Play with the conflict message.
- [ ] The provider indexes the **real profile folder**, skips reparse points, and matches paths case-insensitively. A file added to the profile after Play is not served until the next Play.
- [ ] The provider serves only paths that stock can't resolve, plus the explicit override, selector, scoreboard and audio paths.
- [ ] An empty profile (no file between 1 B and 64 MB) fails Play with the preflight message.
- [ ] Any launch failure restores the profile junction and the player links.

---

## H. Provider diagnostics (Settings / developer surfaces)
- `resolver:status` / `resolver:rescan` (`main.js:1008-1009`) re-run the layout resolver (force bypasses the cache). The UI shows the status uppercased, the message and the raw JSON report (`app.js:258-268`).
- Resolver report contract (`provider-layout-resolver.js:24-55`):
  - schema `nba2k27-provider-layout-v1`
  - `timestamp`/`imageSize` as `0x%08X`
  - exactly **20** layout RVAs and a **47**-slot vtable
  - when `ok`, each candidate count is 1 and there are 47 executable targets
- The cache file is `<TS>-<SIZE>.json` in `provider-layouts`, validated against exe path, size and mtime (`:57-105`).
- `provider:diagnose` (`main.js:1082+`) does a full read through a fresh provider: hashes the asset, checks an EOF read and zero open handles. `provider-validation-cache.js` keeps up to 32 verified identities and warms the first 8 MB.
- These are diagnostics. They are not needed for parity of the mods UX.
- `PROVIDERS` (`providers.js:1-16`) lists `local` ("Local unique-asset loader", available) and `folder` ("Game mods folder", limited). Settings always force `local`.

---

## Known divergences in current Rust port (checked read-only 2026-10-17)
- **Install conflicts:** Rust asks for a per-file Replace or Keep decision (defaults Keep, with Replace all / Keep all) (`nba2k27-launcher-tauri2\src\mod_install.rs:961-983`; NOTES `mod-installer-port.md`). WPF uses one all-or-nothing "Replace and install" box. Rust is a superset, so the WPF wording should still be offered.
- **ZIP decoding:** Rust uses the `zip` crate, **Stored/Deflate only** (NOTES `mod-installer-port.md`). WPF passes ZIPs to `tar.exe` (libarchive), which also reads Deflate64, bzip2, LZMA and others. Some ZIPs that WPF installs will be rejected by Rust. RAR uses tar.exe in both.
- **Staging folder:** Rust stages in `<portable>\Data\mod-install\stage-<id>`. WPF uses `%TEMP%\2k-mod-import-*`.
- **Tags storage:** Rust stores tags in settings `library.tags` keyed `${root}|${path}`, with a backend limit of 200 chars total (NOTES `mods-page-wpf-parity.md`). WPF uses `%userData%\mod-library-tags.json` (`nba2k27-mod-tags-v1`). No migration from the WPF file was found (**UNVERIFIED**).
- **Open action:** Rust allows only data extensions for Open (NOTES `mods-page-wpf-parity.md`). WPF has no allowlist.
- **Selectors:** Rust lets "existing coordinated selector mods win" (`src\selector_runtime.rs:1-2`). WPF **fails Play** on a foreign selector bundle.
- **version.dll:** Rust blocks the launch on any existing `version.dll` (AUD row 21). WPF overwrote known launcher hashes.
- **Player folders:** Rust uses session-owned NTFS links with an intent journal and nonce-named staging (`src\player_folders.rs:1-4`), plus `provider_assets.rs:564 index_player_links`. Whether Rust also adds them to the provider index directly instead of relying on hard links being indexed is **UNVERIFIED**.

## Open questions / UNVERIFIED
- Whether the NBA 2K27 engine natively reads `<game>\mods` for **stock-listed** replacements, independent of the provider. The WPF guide text implies it ("presented at the game's normal mods path", `index.html:322`), and the DLL tries stock first. The engine-side mechanism isn't in these sources.
- Mapping of the two provider-host build stamps (`0x6A974D13/0x368C3000`, `0x6AA2013D/0x36558000`) to the "original" and "Sep11" labels.
- `ML\provider-host\Program.cs` is assumed to be the source of the shipped `WR\runtime-resources\provider-host\NBA2K27ProviderHost.exe`. The binary was not diffed.
- Whether leaving profile controls unlocked during the `preparing-*` phases (before `preparing-loader`) was intentional. A profile change mid-launch would not affect the already-read `settings`.
- Sibling notes: the Rust Mods page restyle passed in 0.47 step 2. Install is unverified pending a live 0.48 test with a real RAR (NOTES `mods-page-wpf-parity.md`, `mod-installer-port.md`; `nba2k27-launcher-tauri2\artifacts\feature-gap-inventory-20261017.md:35`).
