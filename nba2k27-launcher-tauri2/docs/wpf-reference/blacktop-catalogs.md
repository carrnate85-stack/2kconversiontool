# Blacktop arena, ball and scoreboard catalogs (WPF reference)

Source: frozen WPF `release\quality-pass-qa-20261010\App\Reference\src` (paths below are relative to it). Researched 2026-10-17.
This file covers the catalogs: data, grouping, search, favorites, thumbnails and the launcher-page dialog.
Live apply mechanics belong to the Blacktop session.

## 0. Dead code
`renderer\arena-launcher-picker.js` (a `<select>` with a ★ Favorites optgroup) is **not loaded** by any HTML. Do not port it.
- The live launcher UI is `renderer\blacktop-setup-picker.js` (`index.html:876`).
- F8 loads `arena-picker.js`, `scoreboard-picker.js` and `ball-picker.js` (`blacktop-panel.html:41`).

## 1. Arena catalog
**Data.** `data\blacktop-arenas.json` has 165 choices.
- Fields: id, label, tested, arenaId, file, disabled, caution, plus optional floorFile, group and templateArenaId.
- `data\blacktop-arena-expansion.txt` is build input only; no JS reads it.

**Group** (`shared\arena-browser.js:3-12`) is derived from the id; the JSON `group` field is ignored. Checked in order, with n from `arena_(\d+)_int`:
- `default` → Outdoor
- n 300–317, `arena_wnba*` or `arena_mycourtw_int` → WNBA
- n 551–649, 800, 924 or `arena_era*` → Historic
- n 0–31 or `arena_clippers2023_int` → NBA
- `park_*` or `*_ext` → Outdoor
- n 729, 906 or the regex `gym|facility|mycourt|scrimmage|hsmycareer|rechighschool|privatehome|animationlab` → Gyms
- everything else → Other

Tab order: NBA, WNBA, Historic, Gyms, Outdoor, Other.

**Search** (`arena-browser.js:14-18`)
- Normalise: NFKD, strip diacritics, lowercase, non-alphanumerics become spaces.
- Every term must appear in label, id, file, floorFile or group.
- Sort: default first, then `label.localeCompare(numeric)`.

**Template arenas.** 6 entries carry `templateArenaId:700` + floorFile (e.g. Brickley's Gym, Oracle Arena `arena_637_int`). Only the native helper reads the field; the UI does not.

**Disabled** (11: `*_lobbyint`, `main_menu_int`, `park_streetball_a/b/c`, `arena_provinggroundsa/b_int`)
- F8: label + `' · Needs work'`, button disabled, `title = caution || file || 'Uses Day / Night'` (`arena-picker.js:18`).
- Launcher: the card is disabled, no suffix.
- Server: `'That arena is not available for testing yet.'`

**Not persisted.** The arena resets to default on every launcher run (`core\arena-manager.js`).
Rust divergence, by user request (0.48): Rust saves the arena and applies it automatically at launch.

**`arena:clone`** (`main.js:1500`) belongs to the Team editor ("Create, verify & link"), in a box that is never shown. It is not part of the Blacktop catalog.

**Arena thumbnails.** None in WPF; text only.

## 2. Arena favorites
**Storage** (`core\arena-favorites.js`): `userData\blacktop\arena-favorites.json`, a plain id array.
- Unknown ids and duplicates are dropped on read.
- A damaged file fails with `'Could not read arena favorites. Your saved file was left unchanged.'`
- Invalid input fails with `'Invalid arena favorite.'`

**F8**
- Tabs: `All`, `★ Favorites`, then the groups.
- Star per row (`★`/`☆`, aria-pressed). aria-label `'Add|Remove <label> to|from favorites'`.
- Empty state: `'Star an arena to find it here.'` / `'No matching arenas.'` + `'Show all arenas'`.

**Launcher** (`blacktop-setup-picker.js:85`)
- `.blacktop-catalog-star`, aria-label `'Toggle <label> favorite'`, absolute top-right, 29px, 19px font, `#f1cf67` when on.
- Disabled while favorites load or save.

## 3. Ball catalog
**List.** Built from the game manifest `balls/ball_*.iff`, minus `data\ball-live-exclusions.json` (43). The first entry is `{id:'current',label:'Current installed',group:'Installed'}`.
- Rust uses the preview catalog (468) minus the same exclusions, giving 425 + current, with the same ids.

**Thumbnails come from two sources**
1. Preview-pack PNG (`assets\blacktop-ball-previews\catalog.json` thumbnails, served from an imported `blacktop-preview-images.zip`).
2. Fallback IPC `ball:thumbnail`, which decodes `store/thumbnails/*.iff` from the game. The route is VCZ via the oo2core decoder → DDS → texconv PNG, cached as `<Cache>\Blacktop\ball-thumbnails\<sha256>.png`. See `core\ball-thumbnails.js`.
   - **Rust: not ported yet** (needs VCZ + BCn → PNG). Rust shows pack PNGs or the CSS ball `●`.

Tile tooltip: `renderNote`.

**requireExplicitBallSelection** (`main.js:221`): a saved ball is reset to `current` once per app run. Rust 0.50 does the same in `Backend::new` (in memory).

## 4. Scoreboard catalog
- `data\scoreboards.json`: 21 choices. The picker drops `mycareer-story` and `flashback-openrun`, leaving 19.
- Groups: tested false → Experimental, `era-*` → Eras, nativeController `SCOREBUG_MAIN_GAME` → Broadcast, otherwise Classic.
- Images and captions come from `renderer\scoreboard-preview-catalog.js`. Missing image: `'PREVIEW UNAVAILABLE'` / `'Preview unavailable'`.
- Persisted at `userData\blacktop\scoreboard\selection.json` (migrations mycareer-story→broadcast, flashback-openrun→default). On load it shows `'Saved — waiting for Blacktop.'`
- A launcher pick and an F8 pick are the same state.

## 5. Launcher Blacktop page (`index.html:107-130`, `blacktop-setup-picker.js`)
**Rows.** Three rows: Scoreboard / Arena / Ball.
- Layout: icon tile 36px | label 11px + value 12px/650 | `Catalog` button 86px. Grid `36px 1fr auto`, min-height 72px, padding 8px 14px.
- The short status is sr-only; the full message is in `title`. There is no notes text.

**Catalog dialog**
- `<dialog class="blacktop-catalog" data-catalog-kind>` opened with `showModal`. Size `min(1040px,100vw-40px)` × `min(720px,100vh-40px)`, radius 11, backdrop `#020806c7`.
- Header: eyebrow `BLACKTOP PRESENTATION`, title `<Kind> catalog`, button `Close`.
- Search placeholder: `Search <kind> catalog`. Filters as above.
- Grid: 4 columns (3 below 1000px, 2 below 760px); scoreboard uses 2 (1 below 600px). Cards are 78px, ball 102px, scoreboard 164px with a 103px preview frame.
- The selected card has an inset 3px court bar.
- Paging: 24 per page, footer `a–b of N` / `No results`, Previous / Next.
- Picking sets `Saving <label>…`, closes the dialog and applies live while the game runs. Errors show `'Could not save selection.'`
- Ball dark palette: `#101821` / `#182635` / `#14212c`.
- CSS: `session-panels.css:72-107,178-195`.

## 6. Rust status (0.50, catalog session)
**Done**
- Data: WPF groups, sort order, the 165 arenas including disabled ones, file/floorFile/caution fields, scoreboard captions and ball notes (`scripts\prepare-assets.mjs`).
- Backend search uses WPF terms.
- Launcher dialog: `frontend\BlacktopCatalog.tsx/.css`.
- Ball reset each run.
- F8 disabled rows (`· Needs work`, disabled, caution/file/'Uses Day / Night' title), added by the overlay session in App.tsx Overlay.

**Open**
- Game-decoded ball thumbnails.
- Launcher arena order: WPF's launcher used raw JSON order (unsorted); Rust uses the F8 sorted order for both.
