# Game rules (F8 live rules + retired Game Rules unlock) — WPF reference
Source build: WPF quality-pass-qa-20261010 (frozen). Last researched: 2026-10-17.

Path aliases used in citations (all relative to `C:\Users\carrn\CodexProjects\2kconversiontool`):
- `WR` = `nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference`
- `UH` = `nba2k27-mod-launcher/unlocks-helper` (C# source of `NBA2K27FreestyleTeams.exe`, shipped as `WR/runtime-resources/unlocks/NBA2K27FreestyleTeams.exe`)

## Summary
- The live feature is **"Game rules" inside the F8 Blacktop overlay**: a pane listing **39** live rule values read from game memory; the user flips one value at a time and it is written immediately to the running game (`WR/src/renderer/blacktop-panel.html:24`, `:31-37`; `WR/src/renderer/direct-game-rules.js`; `UH/GameRuleData.cs`).
- Logic: renderer `direct-game-rules.js` -> IPC `blacktop:rules-read` / `blacktop:rules-set` (`WR/src/main.js:284-293`) -> `DirectGameRules` (`WR/src/core/direct-game-rules.js`) -> helper `rules-read` / `rules-write` (`UH/GameRuleData.cs:74-142`). Pure data writes into one GameData owner object; no hooks, no code patch, no persistence, no launch-time arming.
- Nothing is saved: no settings key, no journal, no restore. The game itself may reload values when modes change (`WR/src/renderer/blacktop-panel.html:36`).
- `core/game-rules.js` + renderer `game-rules.js` + `UH/GameRules.cs` are the **retired "Game Rules unlock" menu hook** (code patch + trampoline). It is **ABANDONED** (hung Blacktop) and not wired in this build (`nba2k27-mod-launcher/ABANDONED_FEATURES.md:6-16`). Do not port it.
- Most likely to break in a port: (1) writing without the optimistic-concurrency check — WPF sends the whole 39-value snapshot and the helper refuses if any value changed ("Refresh before editing"); (2) the session gate — writes only allowed from the F8 window while the launcher's own offline session is `loader-active`; (3) value whitelisting by width (1-byte booleans vs 4-byte enums with fixed ranges).

## Key source map
| Concern | file:line |
|---|---|
| F8 bar button "Game rules" + pane markup/strings | `WR/src/renderer/blacktop-panel.html:24`, `:31-37` |
| Pane renderer (read, buttons, selects, setRule) | `WR/src/renderer/direct-game-rules.js:1-54` |
| Pane CSS (2-column grid, pressed state) | `WR/src/renderer/direct-game-rules.css:1-20` |
| Pane open/close + aria-pressed in overlay | `WR/src/renderer/blacktop.js:20`, `:36-41` |
| Option labels for enum fields | `WR/src/shared/game-rule-values.js:8-25` |
| Preload API | `WR/src/blacktop-panel-preload.js:17-18` |
| IPC handlers + gates | `WR/src/main.js:284-293` |
| DirectGameRules wiring (helper path) | `WR/src/main.js:195`, `:201-205` |
| Build compatibility gate (`blacktop:rules-set` -> rules) | `WR/src/main.js:17-26`, `WR/src/core/game-compatibility.js:6`, `:21` |
| Main-process validation + exec | `WR/src/core/direct-game-rules.js:5-33` |
| Native field table (labels, ids, offsets, widths) | `UH/GameRuleData.cs:13-53` |
| Native value whitelist | `UH/GameRuleData.cs:54-59` |
| Owner anchor + read/write procedure | `UH/GameRuleData.cs:65-142` |
| Launcher preview of the pane (disabled controls) | `WR/src/renderer/blacktop-overlay-preview-frame.js:109-127` |
| Retired unlock hook (do not port) | `WR/src/core/game-rules.js`, `WR/src/renderer/game-rules.js`, `UH/GameRules.cs` |

---

### Sub-feature A: F8 "Game rules" pane (live, direct GameData writes)

#### 1. User-visible behavior
- Entry: F8 overlay top bar segment labelled **"Rules"** with button `#openBlacktopRules` text **"Game rules"** (`WR/src/renderer/blacktop-panel.html:24`). Clicking toggles the expanded rules view (`WR/src/renderer/direct-game-rules.js:49-52`); opening calls `read()` every time.
- Pane (`WR/src/renderer/blacktop-panel.html:31-37`):
  - Heading text: "Live values for this game session. Nothing changes until you choose a value."
  - Button `#refreshBlacktopRules` **"Refresh values"**.
  - Status `#blacktopRulesStatus` initial: "Open Game rules to read the current values."
  - Footer note: "Changes apply immediately to the current game state. The game may reload its own values when you change modes."
  - List `#blacktopRulesList`, 2-column grid (1 column under 800 px) (`WR/src/renderer/direct-game-rules.css:10`, `:20`).
- Status strings (`WR/src/renderer/direct-game-rules.js`): reading "Reading live rule values…" (`:37`); success "`39` live rules loaded" (`${n} live rules loaded`, `:38`); applying "Applying rule…" (`:44`); after write "Rule updated. Other values were left unchanged." (`:45`); errors show the helper/IPC message, fallback "Game Rules are unavailable." (`:39`) or "Rule update failed. Refresh before trying again." (`:46`). After a failed write the snapshot is dropped, so the list empties until Refresh (`:46`).
- Row rendering (`:12-33`):
  - width 1: two buttons **"Off"** / **"On"**, `aria-pressed` on the current one; the current-value button is disabled (`:17-23`).
  - width 4 with a known enum: a `<select>` with the labels below (`:24-28`).
  - anything else (unknown value): text "Value N", tooltip "Numeric field; value meaning not yet established" (`:29-32`).
  - All controls disabled while busy (`:21`, `:27`).
- Rules (exact labels and order, from `UH/GameRuleData.cs:14-52`):

| # | Label | field id | offset | width | values |
|---|---|---|---|---|---|
| 1 | Shot Clock | ShotClock | 0x816 | 1 | 0 Off / 1 On |
| 2 | Out Of Bounds | OutOfBounds | 0x824 | 1 | Off/On |
| 3 | Backcourt Violation | BackcourtViolation | 0x825 | 1 | Off/On |
| 4 | 8 Sec Half Court | EightSecondsHalfCourt | 0x826 | 1 | Off/On |
| 5 | 5 Sec Inbound | FiveSecondsInbounding | 0xA75 | 1 | Off/On |
| 6 | 3 in the Key | OffensiveViolationInKey | 0x908 | 4 | 0 Off / 1 3 seconds / 2 5 seconds |
| 7 | Def 3 in the Key | DefensiveViolationInKey | 0x90C | 4 | 0 Off / 1 3 seconds / 2 5 seconds |
| 8 | 5 Sec Back To Basket | FiveSecondsBackToBasket | 0x827 | 1 | Off/On |
| 9 | 5 Sec Closely Guarded | CloselyGuarded | 0xA4E | 1 | Off/On |
| 10 | Over the Back Foul | OverTheBackRule | 0x830 | 1 | Off/On |
| 11 | Generic Personal Foul | GenericPersonalFoulRule | 0x831 | 1 | Off/On |
| 12 | Charging Foul | ChargingRule | 0x832 | 1 | Off/On |
| 13 | Offensive Offball Foul | OffensiveOffballFoulRule | 0x833 | 1 | Off/On |
| 14 | Defensive Offball Foul | DefensiveOffballFoulRule | 0x834 | 1 | Off/On |
| 15 | Blocking Foul | BlockingRule | 0x835 | 1 | Off/On |
| 16 | Reaching Foul | ReachingFoulRule | 0x836 | 1 | Off/On |
| 17 | Loose Ball Foul | LooseBallRule | 0x838 | 1 | Off/On |
| 18 | Shooting Foul | ShootingFoulRule | 0x837 | 1 | Off/On |
| 19 | Intentional Foul | IntentionalFoul | 0x839 | 1 | Off/On |
| 20 | Clear Path Foul | ClearPathRule | 0x840 | 1 | Off/On |
| 21 | Transition Take Foul | TransitionTakeFoulRule | 0x841 | 1 | Off/On |
| 22 | Kick Ball | KickBall | 0x83A | 1 | Off/On |
| 23 | Goaltending | Goaltending | 0x828 | 1 | Off/On |
| 24 | Illegal Pick | IllegalPick | 0x82B | 1 | Off/On |
| 25 | Traveling | Traveling | 0x829 | 1 | Off/On |
| 26 | Double Dribble | DoubleDribble | 0x82A | 1 | Off/On |
| 27 | Foul Out | FoulOut | 0x82C | 4 | 0 Off / 1..6 ("1".."6") |
| 28 | 3Pt Shots | ThreePointShotRestriction | 0xA98 | 4 | 0 Enabled / 1 Enabled after steal or block / 2 Disabled |
| 29 | Zone Defense Allowed | ZoneDefenseAllowed | 0x8D4 | 1 | Off/On |
| 30 | No Charge Zone | NoChargeZone | 0xA50 | 1 | Off/On |
| 31 | Buzzer Beater Review | BuzzerBeaterReview | 0x849 | 1 | Off/On |
| 32 | Clear Ball Required | ClearBallRequired | 0xA51 | 1 | Off/On |
| 33 | Timeouts | Timeouts | 0xA5D | 1 | Off/On |
| 34 | Screen Set Out of Bounds | ScreenOutOfBoundsViolation | 0x844 | 1 | Off/On |
| 35 | Offensive Player Out of Bounds | PlayerOutOfBoundsViolationRule | 0x846 | 1 | Off/On |
| 36 | Ball Entered Basket From Below | BallEnteredBasketFromBelow | 0xA74 | 1 | Off/On |
| 37 | Technical Foul | TechnicalFoulRule | 0x83B | 1 | Off/On |
| 38 | Flagrant Foul | FlagrantFoulRule | 0x83C | 1 | Off/On |
| 39 | Technical Foul Ejection | TechnicalFoulEjectionRule | 0x83D | 1 | Off/On |

  Enum labels: `WR/src/shared/game-rule-values.js:8-13`, `:21-23`. 35 width-1 + 4 width-4 fields, self-test enforced (`UH/GameRuleData.cs:75-78`). Enum meanings were paired against the stock debug Game Rules menu in an offline Blacktop match (`nba2k27-mod-launcher/README.md:219-231`).
- Defaults: none. WPF never sets a rule on its own; it shows whatever the game currently holds.
- Launcher (non-game) preview of the F8 overlay shows the same labels with disabled Off/On buttons and selects containing "Live value" + options; status "Preview only · current values appear when the F8 overlay is opened in game." (`WR/src/renderer/blacktop-overlay-preview-frame.js:109-127`).

#### 2. Timing and gating
- No automatic timing: reads happen on pane open and on "Refresh values"; writes only on user click/change (`WR/src/renderer/direct-game-rules.js:36-53`). No polling, no re-apply on later visits/menus/matches, no restore on exit.
- Renderer guard: one operation at a time (`busy`) (`:37`, `:43`). Main-process guard: `pending` -> "Another Game Rules operation is running." (`WR/src/core/direct-game-rules.js:14`). Native guard: mutex `Local\2kLauncherDirectRules`, `WaitOne(0)` -> `waiting` "Another Game Rules update is running." (`UH/GameRuleData.cs:83-87`).
- IPC sender gate: both channels only from the F8 panel window, else "Game Rules are available in the F8 overlay." (`WR/src/main.js:285`, `:289`).
- Write session gate: `versionProxySession.state.phase === 'loader-active'` && `background` && !`cancelled`, else "Launch an active offline session through this launcher before changing Game Rules." (`WR/src/main.js:290-291`). Reads have **no** session gate.
- Build gate for writes: every IPC passes `assertGameFeature`; channel `blacktop:rules-set` maps to feature `rules`, supported only on `6AA2013D:36558000`; else "Game rules: This game build is not supported by this feature. No new edit will be attempted." (`WR/src/main.js:17-26`, `WR/src/core/game-compatibility.js:6`, `:21`, `:31-34`).
- Helper exec: `timeout 8000` ms, `maxBuffer 65536` (`WR/src/core/direct-game-rules.js:6`).
- Native readiness: if the owner pointer is null/invalid -> `waiting` "Game Rules are not initialized yet." (surfaced as error text because only phase `ready` is accepted) (`UH/GameRuleData.cs:70`, `WR/src/core/direct-game-rules.js:19`).
- No game / more than one game: "Start one offline NBA 2K27 session first." (`UH/GameRuleData.cs:90`).
- EAC present: "Game Rules requires offline/non-EAC Play. No menu code was changed." (`UH/GameRules.cs:38`, called from `UH/GameRuleData.cs:94`, `:126`).

#### 3. Mechanism
Helper `NBA2K27FreestyleTeams.exe` (`WR/src/main.js:203`), dispatch prefix `rules-` (`UH/FreestyleTeams.cs:180`).

Read: `rules-read <gameRoot>` (`WR/src/core/direct-game-rules.js:24`; `UH/GameRuleData.cs:80-81`)
1. Exactly one `NBA2K27` process whose image path == `<gameRoot>\NBA2K27.exe` (`:90-93`).
2. Open with `0x410` (read) for read, `0x438` for write (`:98`); `Identity()` check (`:100`).
3. PE guard timestamp `0x6AA2013D` / image size `0x36558000` -> else `unsupported` "Game Rules support a different NBA 2K27 build." (`:101-103`).
4. Owner anchor (`RuleOwner`, `:65-73`): at `base+0x2610815` must be `48 8B 05 rel32` (mov rax,[rip+rel]) whose target == RVA `0xAACB9A0`; else `unsupported` "Game Rules owner anchor differs from this build.". `owner = *(u64*)(base+0xAACB9A0)`, must be 8-aligned in user range and region `[owner, owner+0xA9C)` private RW.
   (Same `0xAACB9A0` pointer is the "kind" pointer of the Blacktop context in presentation/crowd.)
5. Read each field: width 1 = byte, width 4 = u32 LE (`:60-63`). Owner re-checked before and after (`:134-136`).
6. Response: `{phase:"ready", pid, created, basis, owner, values:[{label,field,width,value}×39], writes}` (`:138`). Main requires `phase==='ready'` and exactly 39 values (`WR/src/core/direct-game-rules.js:18-20`).

Write: `rules-write <gameRoot> <pid> <created> <basis> <owner> <field> <before> <after> <v1,...,v39>` (`WR/src/core/direct-game-rules.js:32`; `UH/GameRuleData.cs:81`)
- Main-side validation: snapshot ids are decimal strings; field exists; `before` and `target` both in `optionsFor(field,width)`; `before != target`; all 39 values integers 0..0xFFFFFFFF (`WR/src/core/direct-game-rules.js:26-30`).
- Native (`UH/GameRuleData.cs:96-133`):
  1. pid/created/basis must match the live game, else `ended` "Game session changed. Refresh rules before editing." (`:96-97`).
  2. Owner must match, else `ended` "Game Rules owner changed. Refresh before editing." (`:106`).
  3. **All 39 current values must equal the snapshot**, else `conflict` "Game Rules changed or the game switched modes. Refresh before editing." (`:107-116`).
  4. Whitelist `RuleValueAllowed` (width1: 0|1; FoulOut 0..6; key/3pt enums 0..2) for before and after, before != after (`:54-59`, `:119-120`).
  5. Re-read target field == before; owner re-resolved == owner (`:122-123`).
  6. EAC + identity check, `WriteProcessMemory` width bytes (1 or 4 LE), read back == after; else `write-failed` "Rule update did not read back." (`:126-128`).
  7. On exception after attempt: if owner unchanged and value == after, write `before` back (best-effort rollback) (`:129-131`).
  8. Return a fresh full snapshot (same shape as read, `writes:1`) which becomes the renderer's new snapshot (`:138`; `WR/src/renderer/direct-game-rules.js:45`).
- Persistence: none. No settings key, no journal file, no restore at match end, menu, game exit or launcher exit.

#### 4. Build profiles
- WPF: `6AA2013D:36558000` only (helper PE guard `UH/GameRuleData.cs:102`, feature table `WR/src/core/game-compatibility.js:6`). The older `6A974D13:368C3000` is rejected.
- V2 (Rust): `src/game_rules.rs` ("Whitelisted GameRuleData mappings from the live-verified WPF implementation", `nba2k27-launcher-tauri2/src/game_rules.rs:1-2`), `src/rules_v2.rs`, `src/rules_v2_access.rs` (owner span `0xA9C`, `rules_v2_access.rs:35`). V2 status: UNV (`nba2k27-launcher-tauri2/artifacts/parity-score-20261017.md:71`). V2 audits: `artifacts/rules-v2-*-20261016.log`, `artifacts/qa034-rules-integrated-final-20261016.log`.

#### 5. Rust must match
- [ ] F8 bar shows "Rules" segment with a "Game rules" button; opening the pane always re-reads live values.
- [ ] Pane strings match: heading, "Refresh values", "Open Game rules to read the current values.", "Reading live rule values…", "39 live rules loaded", "Applying rule…", "Rule updated. Other values were left unchanged.", footer note.
- [ ] Exactly 39 rows in the table order above with the exact labels; width-1 rows render Off/On buttons (current one pressed+disabled); the 4 enum rows render selects with the exact labels; out-of-range values render "Value N" read-only.
- [ ] Reading requires exactly one game running from the configured folder; with no game shows "Start one offline NBA 2K27 session first."
- [ ] Writing from anywhere but the F8 overlay is refused; writing without an active launcher-managed offline session shows "Launch an active offline session through this launcher before changing Game Rules."
- [ ] A write sends the full snapshot; if any of the 39 values changed in game since the read, nothing is written and the user is told to refresh.
- [ ] Only whitelisted values are writable (booleans 0/1, Foul Out 0-6, key/3pt enums 0-2); same-value writes are refused.
- [ ] Each write is one field (1 or 4 bytes), read back, and the pane re-renders from the returned fresh snapshot.
- [ ] Nothing is persisted or auto-applied; no restore on exit; no polling.
- [ ] Refused while EasyAntiCheat/start_protected_game runs.
- [ ] One operation at a time (UI busy + process lock).

---

### Sub-feature B: Retired "Game Rules unlock" (ABANDONED — do not port)

#### 1. User-visible behavior
- Former switch `#enableGameRules` on the Unlocks page with status `#gameRulesStatus` (`WR/src/renderer/game-rules.js:3-7`). Messages: "Game Rules will unlock on the next Play launch." / "Game Rules is off." / "Game Rules is unlocked. Reopen Pause or Options to refresh." (`WR/src/core/game-rules.js:9`, `UH/GameRules.cs:74`).
- Not present in the frozen build: `index.html` has no `enableGameRules` and `main.js` never requires `core/game-rules` (only `direct-game-rules`, `WR/src/main.js:195`). Saved key `unlockGameRules` is deleted on settings load (`WR/src/core/paths.js:38`).

#### 2. Timing and gating
- Was armed at Play, 120 s bind deadline, 500 ms polling, 3000 ms observe after applied (`WR/src/core/game-rules.js:25-30`).

#### 3. Mechanism
- Code patch: 12 bytes at RVA `0x3913DA2` (`84 C0 75 AA 42 8B 84 3B 90 00 00 00`) replaced by `jmp` to a 32-byte trampoline allocated within ±2 GB that also includes menu item id `0xB79AF8BA` (`UH/GameRules.cs:8-27`, `:52-57`). Journal `game-rules-session.json`.

#### 4. Build profiles
- `6AA2013D:36558000` only (`UH/GameRules.cs:35`).

#### 5. Rust must match
- [ ] Do **not** implement. Reason: "The owned native hook caused NBA 2K27 to hang when Blacktop began." (`nba2k27-mod-launcher/ABANDONED_FEATURES.md:6-16`). Discard any imported `unlockGameRules` setting.

---

## Known divergences in current Rust port (brief, not re-verified)
- Rust has native rules modules (`game_rules.rs`, `rules_v2*.rs`); live status UNV. The sibling note flags that exact `rules_v2::context` equality could hit V2 selection blips; 0.48 made `rules_v2::context` the debounced read (`C:\Users\carrn\AppData\Roaming\Claude\side-session-notes\local_98bf0092-fe39-4ae3-abce-e88edad0a633\blacktop-presentation-overlay.md:50-57`). WPF rules have **no** context gate at all — they read/write whenever the owner pointer is valid.

## Open questions / UNVERIFIED
- Whether rule writes take effect mid-possession or only at next game state reload — WPF only states "The game may reload its own values when you change modes." (`WR/src/renderer/blacktop-panel.html:36`). Gameplay enforcement across a full possession or foul sequence: **UNVERIFIED** — WPF's own changelog says it "is still QA" (`nba2k27-mod-launcher/README.md:230-231`).
- Meaning of values outside the whitelist (shown as "Value N") — **Not found in WPF sources**.
