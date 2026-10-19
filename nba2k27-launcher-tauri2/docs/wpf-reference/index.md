# WPF behavior reference — index

Spec source: frozen WPF build `nba2k27-launcher-wpf\release\quality-pass-qa-20261010` plus version.dll (`nba2k27-version-proxy\src`) and helper sources. Researched 2026-10-17.
**Read [HOW-TO-USE.md](HOW-TO-USE.md) first.** Every file uses the same layout for each sub-feature: 1 behavior · 2 timing/gating · 3 mechanism · 4 build profiles · 5 "Rust must match" checklist.

## Files

| Area | File | Covers |
|---|---|---|
| Cross-cutting | [architecture.md](architecture.md) | C# WebView2 host ↔ Node backend ↔ renderers; boot order; version.dll cfg/trigger/ARMED/LIVE protocol; build detection; restore on game exit, launcher close and host loss; storage paths |
| Play / launch | [launch-play.md](launch-play.md) | Play button and progress; ordered prepare and rollback; what is armed before Steam vs after attach; Steam launch; profile junction; single-launcher lock; close dialog; game volume |
| Blacktop overlay (F8) | [blacktop-overlay.md](blacktop-overlay.md) | Blacktop page; F8 window (create, topmost, focus, Alt-Tab); F8 watcher; layout and sizes; player dropdown; Random player / Random all; auto-open/hide; auto-random; match carry |
| Outfits | [outfits.md](outfits.md) | Apply at selection; Random / Random all / auto-random write paths; match carry (version.dll saved-outfits worker); outfit creator, presets, `.2koutfit` share, mannequin |
| Arena / ball / scoreboard | [arena-ball-scoreboard.md](arena-ball-scoreboard.md) | Pickers, defaults, favorites, saved-choice semantics, selection-screen gate, re-apply across matches and restarts, restore; V2 ball mechanism differs |
| Presentation removals | [presentation.md](presentation.md) | Blacktop drone, DJ, MC and crowd removals: write boundary, re-apply each selection, crowd restore at main menu |
| Game rules | [rules.md](rules.md) | F8 "Game rules" pane: 39 rules, full-snapshot writes, whitelist; abandoned rules unlock (do not port) |
| Startup video | [startup-video.md](startup-video.md) | External fullscreen MP4 overlay as shipped; window-ready wait; volume and mute; skip input. The native movie route is dead code |
| Scaler | [scaler.md](scaler.md) | Player/referee global float, coaches and arena actors, groups, 95–100%, mode gate, polling, conflict and reset phases |
| Cameras | [cameras.md](cameras.md) | Camera install: no menu gate, all-or-nothing with rollback, retry window; order and names persistence; height/zoom lock |
| Unlocks / WNBA | [unlocks-wnba.md](unlocks-wnba.md) | WNBA in Freestyle (category-order route), WNBA in Blacktop (`blacktop-watch`), Debug Menus (never saved), restore on exit |
| Jukebox | [jukebox.md](jukebox.md) | Library and archives; session publish and journaled restore; version.dll audio policy; Freestyle playlist; covers; prepared-playlist cache; audio readiness |
| Mods / profiles | [mods-profiles.md](mods-profiles.md) | Mod library; install from ZIP/RAR/folder; tags; profiles via `mods` junction; provider host indexing; selector bundle; player-folder hard links |
| Roster / team editors | [roster-team-editors.md](roster-team-editors.md) | 26-service inventory; attach and readiness; save transaction (expected-old-values, read-back, rollback); seasons with game pause; staff, rotations, uniforms, DNA, 2KDB |
| Settings / support / updates | [settings-support-updates.md](settings-support-updates.md) | `settings.json` keys and defaults; theme; diagnostics; support bundle and redaction; logs; offline updater and rollback; migrations |

## Corrections to common assumptions (found during this research)

- **F8 is neither a keyboard hook nor RegisterHotKey.** `NBA2K27F8Watcher.exe` polls the key every 20 ms and only counts presses while the game is in the foreground ([blacktop-overlay.md](blacktop-overlay.md)).
- **Auto-random is always on in WPF**, with no setting. It runs in the backend, and manual choices always win ([blacktop-overlay.md](blacktop-overlay.md), [outfits.md](outfits.md)).
- **There is no camera menu gate.** Cameras install all-or-nothing with rollback. Retries run every 500 ms within a 20 s transient window, with at most 2 restarts ([cameras.md](cameras.md)).
- **"Presentation" in the WPF source mostly means the startup video.** Only `blacktop-presentation.js` and `blacktop-crowd.js` are court removals ([presentation.md](presentation.md)).
- **WNBA Freestyle uses the 4-DWORD category-order route.** The mask 0x68400033 / category 5 route was withdrawn (`ABANDONED_FEATURES.md`) ([unlocks-wnba.md](unlocks-wnba.md)).
- **The WPF ball swap does not move the court ball on V2.** V2 needs the GameData owner+0xDA4 write ([arena-ball-scoreboard.md](arena-ball-scoreboard.md)).
- **WPF never saved the arena choice.** The ball choice resets to "Current installed" at every launcher start. The scoreboard choice is saved and auto-applies ([arena-ball-scoreboard.md](arena-ball-scoreboard.md)).
- **`selector_override.hpp` is the player-selector `.iff` redirect**, not scoreboard or overlay logic.
- **WPF refuses to close while the game runs** ("Close NBA 2K27 first.", `main.js:461,2315`) and restores only after proving the game has exited ([architecture.md](architecture.md)).
- **WPF has no per-mod enable/disable or load order.** Profiles are the only way to switch mods ([mods-profiles.md](mods-profiles.md)).

## Known Rust divergences flagged by the research (verify before acting)

These come from read-only checks of the current port; each file's "Known divergences" section has the details.
- **Outfit carry:** the V2 rebuild must use the full manager object (interface pointer − 0x78). The Rust port missed this, which is why carry never fired ([outfits.md](outfits.md)).
- **WNBA Blacktop:** the V2 delegate slot is +0xD30, not +0xD10. The settings names and defaults also differ from WPF (`unlockWnbaFreestyle` on, `unlockWnbaBlacktop` off) ([unlocks-wnba.md](unlocks-wnba.md)).
- **Overlay:** Rust shows the overlay on entering selection and allows it during matches. WPF requires 2 complete-lineup reads at selection, and hides after 2 reads off selection ([blacktop-overlay.md](blacktop-overlay.md)).
- **Random all:** WPF stops at the first unconfirmed player; Rust continues ([outfits.md](outfits.md)).
- **Launch arming:** WPF arms the scaler, unlocks and other features before Steam starts, each with its own deadline. Rust creates its runners after ARMED ([launch-play.md](launch-play.md)).
- **Jukebox readiness:** WPF uses `NBA2K27AudioReadiness.exe` when a custom playlist is on; Rust uses a fixed 20 s settle ([jukebox.md](jukebox.md)).
- **ZIP extraction:** WPF uses Windows `tar.exe`, so it accepts more compression methods than the Rust `zip` crate ([mods-profiles.md](mods-profiles.md)).
- **Updater and rollback** are missing in Rust ([settings-support-updates.md](settings-support-updates.md)).
