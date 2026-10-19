# Code review: nba2k27-launcher-tauri2 (QA084)

- **Reviewer:** Claude Code (Opus 5.5), 2026-10-19
- **Commit reviewed:** `0fad871` ("Add complete NBA 2K27 Tauri launcher QA084 source checkpoint")
- **Scope:** the whole `nba2k27-launcher-tauri2/` folder. That commit added all 1,129 files at once, so there is no smaller diff to review. The review was split into six risk areas (listed below).
- **Method:** a read-only review that looked only for real bugs, not style. Each finding has a severity, a location, a concrete scenario and a minimal fix. See [Not verified](#not-verified) for what this review could not check.

## Instructions for Codex (read first)

Do not take these findings as correct by default. For each one:

1. Open the cited code and trace the path yourself.
2. Mark it **Agree**, **Disagree** (say why) or **Needs info**, in the `Codex verdict` line under the finding.
3. Implement only the findings you agree with, using the smallest change that fixes them. Do not rewrite code that works.
4. Several findings touch fail-closed safety code (journals, receipts, ownership checks). Keep those guarantees: a fix must never let the launcher overwrite or delete something it cannot prove it owns.
5. Where a fix changes behaviour a test asserts (noted per finding), update that test and say so.

**Status legend:**
- **Confirmed:** the defect was traced in the code a second time by the lead reviewer, beyond the area reviewer's own pass.
- **Reported:** traced by the area reviewer only.

Neither status is proof that the bug happens at runtime. The Rust test suite was run (results under [Not verified](#not-verified)), but no finding was reproduced against the live game.

## Summary

| ID | Sev | Area | Finding | Status |
|----|-----|------|---------|--------|
| H1 | High | Jukebox | Crafted `.2kplaylist` can exhaust memory and abort the launcher | Confirmed |
| M1 | Med-High | File writes | One missing player source blocks all session cleanup, incl. the `game\mods` junction | Confirmed |
| M2 | Med-High | Roster | Rollback can't run after cancel/12 s budget, leaving half-applied record edits | Confirmed |
| M3 | Medium | File writes | Mod-install commit isn't crash-safe; original mod can be stranded as a hidden `.bak` | Confirmed |
| M4 | Medium | Security | Oodle DLLs loaded in-process from user-set game folder with no hash/signature check | Confirmed |
| M5 | Medium | Security | Launcher update verifies integrity against its own manifest only; no authenticity, allows downgrade | Confirmed |
| M6 | Medium | File writes | Stage file left on in-process error makes camera/selector recovery hold forever | Reported |
| M7 | Medium | File writes | Failed self-update rollback deletes the recovery journal | Reported |
| M8 | Medium | Frontend | Failed settings save is never rolled back; UI and backend disagree on profile/mods root | Confirmed |
| M9 | Medium | Frontend | Successful mod install reported as failure when the follow-up rescan fails | Confirmed |
| M10 | Medium | Memory | Transaction watchdog deadline isn't aligned with the helper's rollback window; a legal rollback can be killed, closing the game | Reported |
| M11 | Medium | Memory | `GAME_WRITE_GATE` is per-process; the provider child process isn't serialized against launcher transactions | Reported (gate scope confirmed) |
| M12 | Med-Low | Memory | One transient `observe` error permanently holds the native session until launcher restart | Confirmed |
| M13 | Medium | Build | No `.gitattributes`: default Git-for-Windows clone rewrites LF→CRLF, breaking hash-pinned embedded assets (27 test failures, broken arena features in a release built from it) | Confirmed by test run |
| M14 | Medium | Memory | Owned-transaction freeze is pinned to one exact `ntdll.dll` SHA-256; routine Windows updates disable it | Confirmed by test run |
| L1–L25 | Low | various | See [Low severity](#low-severity) | mixed |

---

## High

### H1. Crafted `.2kplaylist` can exhaust memory and abort the launcher
- **Status:** Confirmed
- **Where:** `src/jukebox_package.rs:289-306` (`reference`), `:343-524` (`import_entries`, especially `:440-443`, `:453`, `:483-493`); the size limit is enforced only afterwards in `src/jukebox_api.rs:1361-1364` (container) and `:1373-1382` (single ZIP).
- **Defect:**
  - `reference()` never records which archive members are already used, so any number of custom tracks can point at the same `audio/...` member.
  - Each track makes its own copy (`audio::wrap(data)`, `data.to_vec()`, or a FLAC decode).
  - `MAX_PACKAGE_NATIVE` (320 MiB) is checked only after `import_entries` has built every song (in the container path, after each whole segment).
- **How it happens:**
  - A share file under 1 MB contains one highly compressible WAV of about 230 MB and up to `MAX_TRACKS` (4096) custom tracks that all reference it. Each track has a unique `source_key`, which is all the check requires.
  - Import tries to allocate hundreds of GB, and Rust aborts the process.
  - With FLAC it is slower but just as bad: each reference also starts a decode worker that can run for up to 120 s.
  - `.2kplaylist` is a sharing format, so importing a file someone else made is the normal use, not an edge case.
- **Minimal fix:**
  1. In `reference()` (or its callers), keep a `HashSet` of members already used and reject a second reference to the same audio member.
  2. Pass a running byte budget into `import_entries`. Check it **before** `wrap`, `to_vec` or `decode_flac`, and check the declared `audio_pcm_bytes` before starting a FLAC worker.
- **Tests:** add one with N tracks that share one member, and one that goes over the budget partway through.
- **Codex verdict:**

---

## Medium

### M1. One missing player source blocks all session cleanup, including the `game\mods` junction
- **Status:** Confirmed
- **Where:** `src/player_folders.rs:501-507` (`Directories::pin(source.parent().unwrap())?`, `fs::symlink_metadata(&source)?`). The sequential `?` recovery chains are in `src/native_session.rs:843-894` and `src/provider_launch.rs:1576-1631`.
- **Defect:**
  - If a player's source file or folder no longer exists, `restore` returns an error at the first such entry.
  - The `?` chain then skips everything after it: the remaining player links, Jukebox recovery and the mod-profile junction restore (`native_session.rs:892-894`).
  - Holding that one entry for review is deliberate (see the comment at `:499-500`). Blocking every unrelated cleanup is not.
- **How it happens:**
  1. The user deletes `mods/players/<Name>/` while the game is running, or between a crash and the next start.
  2. Every recovery attempt now fails at the same entry.
  3. "Player-folder recovery is pending" blocks every modded launch.
  4. `game\mods` still points at the mod profile, so even a plain Steam launch starts modded.
- **Minimal fix:**
  - Treat a missing or changed source as a per-entry hold: keep that entry's receipt and continue with the others, never deleting the last copy.
  - Restructure the chain so that a held player store doesn't stop the independent loader, Jukebox and profile-junction restores. Collect the errors and report them together.
- **Codex verdict:**

### M2. Roster rollback can't run after cancellation or the 12 s budget, leaving half-applied edits
- **Status:** Confirmed
- **Where:**
  - Rollback loop: `src/roster_v2_write.rs:1800-1818`.
  - `WriteLease::owner`/`capture` (`src/roster_v2_access.rs:1060-1137`) read through `Access::read`, which calls `Access::checkpoint` (`:471`, `:474-484`).
  - The contract this breaks is stated at `roster_v2_write.rs:1479-1481`.
  - Same mechanism in `src/season_structure_v2_native.rs:1140-1198`.
- **Defect:** `checkpoint()` fails once `stop` is set or `started.elapsed() >= BUDGET`. Rollback calls `m.owner()` and `m.capture()` first, so it fails immediately.
- **How it happens:**
  1. A multi-patch player save is under way.
  2. The launcher window closes (`stop` is set), or a slow save passes 12 s.
  3. `apply` fails at the next `m.checkpoint()`, and the rollback fails at once on the same checks.
  4. Patches already written stay in game memory, and the journal stays `pending`.
  5. All roster writes are blocked until the game exits. This part is the designed safe-hold.
- **Minimal fix:**
  - Give `Access` a recovery mode (for example a `Cell<bool>`) that `apply` sets before rolling back.
  - In that mode, `checkpoint()` skips the stop and budget checks but **keeps** `handle_matches`.
  - Do the same for the season-structure rollback.
- **Tests:** `roster_v2_write_tests.rs:742-747` passes only because the fake's `owner`/`capture`/`write` ignore cancellation. Make the fake honour it, then add a cancel-mid-apply test.
- **Codex verdict:**

### M3. Mod-install commit isn't crash-safe
- **Status:** Confirmed
- **Where:** `src/mod_install.rs:1109-1117`
- **Defect:**
  - The existing mod is renamed to `.mod-install-<id>.bak` (`:1113`) **before** the staged file is renamed into place (`:1117`).
  - In-process errors are rolled back (`:1121-1130`), but nothing journals the commit.
  - No code outside the tests ever looks for `.mod-install-*` files.
- **How it happens:**
  - A crash, kill or power loss between the two renames leaves the destination missing. The user's original mod survives only under a hidden random name.
  - Staged temp copies (up to 2 GB each) are also left in the mods folder.
- **Minimal fix:**
  - Preferred: replace in one step with `MoveFileExW(temp, dest, MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` or `ReplaceFileW` with a backup name. Keep the existing unchanged-metadata check just before it.
  - Alternatively, journal the commit in `Data/` and sweep or restore `.mod-install-*` at startup.
- **Codex verdict:**

### M4. Oodle DLLs are loaded in-process from the user-set game folder without verification
- **Status:** Confirmed
- **Where:**
  - `src/fit_assets.rs:224-243` (`load_game_dll`, `Oodle::load`).
  - The same pattern in `src/stock_selectors.rs:258-270`, `src/jukebox_cover_route.rs:232-248`, `src/jukebox_v2_resources.rs:329-336` and `src/wnba_roster_v2.rs:275-282`.
  - `game_root` comes from `save-settings` (`src/backend.rs:611-629`, `:1444-1462`).
- **Defect:**
  - `oo2core_9_win64.dll` and `oo2texrt_9_win64.dll` are loaded with `LoadLibraryEx` from `<game_root>/data/oodle/`.
  - There is no hash pin and no Authenticode check.
  - `game_root` only needs `NBA2K27.exe` and a manifest file to exist.
- **How it happens:**
  - The user points Settings at a downloaded "pre-modded game folder" that contains a planted Oodle DLL.
  - Opening outfit previews or the Jukebox then runs that DLL inside the launcher.
  - The launcher holds a write handle to the game process, and it is a separate path from Steam launch.
- **Minimal fix:**
  - Before loading, check the SHA-256 against the known Oodle builds for the supported game builds, or check the Authenticode signer. Refuse otherwise.
  - Optionally, require `game_root` to match the Steam-resolved root that `session_safety` already computes.
- **Codex verdict:**

### M5. Launcher update has integrity checks but no authenticity check, and allows downgrades
- **Status:** Confirmed
- **Where:** `src/launcher_update.rs:120-154` (`validate_manifest`), `:237-277` (`manifest_from_build_info`), `:279-301` (`package`) and `:421-483`.
- **Defect:**
  - The expected SHA-256 values come from `release.json` or `BUILD-INFO.json` inside the same folder being verified.
  - So any self-consistent package is accepted.
  - The version is never compared with the running version.
- **How it happens:**
  - A re-hosted or tampered update ZIP carries a manifest that matches its own modified exe. It gets installed and relaunched.
  - An older build with a known bug is also accepted.
- **Minimal fix:**
  - Sign `release.json`, for example with Ed25519 using a public key compiled into the binary, and verify the signature before `verify()`.
  - Reject versions lower than the current one unless it's an explicit rollback from `PreviousApp`.
  - The `BUILD-INFO.json` fallback would need the same signature, or should be removed.
- **Codex verdict:**

### M6. A stage file left after an in-process error makes camera/selector recovery hold forever
- **Status:** Reported
- **Where:**
  - `src/camera_name_files.rs:383-403`; restore refuses at `:499-502` ("Unrecorded interrupted camera stage").
  - `src/selector_files.rs:262-269`; a partial file fails the hash check at `:369-370`.
- **Defect:** both create a stage file with `create_new`, write to it, and on error return without deleting it, even though they hold the only handle.
- **How it happens:**
  - Cancel, a failure in a parallel prep lane, or a full disk hits between creating the stage and recording the receipt.
  - Recovery then holds permanently, which also triggers the M1 chain.
- **Minimal fix:**
  - Delete the stage on the error path, as `src/loader_files.rs:374-383` and `src/jukebox_files.rs:772-789` already do.
  - `camera_name_files_tests.rs:315-343` currently asserts the held behaviour and needs updating.
- **Codex verdict:**

### M7. A failed self-update rollback can delete the recovery journal
- **Status:** Reported
- **Where:** `src/launcher_update.rs:576-590`
- **Defect:** undo renames are `let _ = rename(...)`, and the journal is then removed whether or not the undo worked.
- **How it happens:**
  1. Antivirus holds the new exe, so `rename(staged → App)` fails after its retries.
  2. The undo `rename(previous → App)` fails for the same reason.
  3. The journal is deleted anyway.
  4. `App` is now missing, and "Recover launcher" reports nothing to recover.
- **Minimal fix:** remove the journal only after confirming `App` exists and passes `verify_dir`. Otherwise keep it.
- **Codex verdict:**

### M8. A failed settings save is never rolled back in the UI
- **Status:** Confirmed
- **Where:** `frontend/App.tsx:1911-1926` (`change`). For comparison, `saveLibrary` at `:1789-1807` does roll back.
- **Defect:**
  - `settingsRef` and state are updated optimistically.
  - When `save-settings` rejects, the catch only sets the error and status.
- **How it happens:**
  1. A profile or `modsRoot` change fails to save.
  2. The UI keeps showing the new profile.
  3. `scan()` waits for the queue, then compares `root` against `settingsRef`. Both hold the new value, so the check passes.
  4. The backend scans its saved **old** folder, and the old folder's mods are listed under the new profile.
  5. Play uses the backend's old settings.
- **Minimal fix:** in the catch, reload the saved settings (`bootstrap`/settings call), then reset `settingsRef.current` and `setSettings` to them, the same way `saveLibrary` does.
- **Codex verdict:**

### M9. A successful mod install is reported as a failure when the rescan fails
- **Status:** Confirmed
- **Where:** `frontend/Workflows.tsx:662-674` (`commitInstall`). `p.scan` is `() => scan(true)` (`frontend/App.tsx:2419`), and that call rethrows (`App.tsx:1974`).
- **Defect:** after `mod-install-commit` succeeds, a failed rescan throws before `p.status(modInstallSummary(result))` runs.
- **How it happens:** the files are installed, a transient error hits the rescan, the UI says "Action not completed", and the user installs again.
- **Minimal fix:** show the summary first, then run the rescan in its own `try`. On failure, report "Installed; library refresh failed: …".
- **Codex verdict:**

### M10. The transaction watchdog's deadline isn't aligned with the helper's rollback window
- **Status:** Reported. The constants and the watchdog condition were checked; the timing scenario was not run.
- **Where:**
  - Watchdog: `src/owned_transaction_win.rs:1131-1132` (`now - wr.started >= WATCHDOG_MS`).
  - Constants: `src/owned_transaction.rs:17-19` (`FORWARD_MS` 5000, `REVERSE_MS` 8000, `WATCHDOG_MS` 10000).
  - Helper lifecycle: `src/owned_transaction.rs:197-221`.
- **Defect:**
  - The watchdog counts from the parent's bootstrap tick.
  - The helper's 5 s forward and 8 s reverse windows count from the moment it freezes the game, which comes after the helper's bootstrap: a full SHA-256 of the game exe, a thread snapshot, a capture, an intent fsync and a round trip.
- **How it happens:**
  1. The freeze lands 3 s after `wr.started`.
  2. A forward step fails at freeze + 4.9 s, and a legal rollback starts.
  3. The watchdog kills the helper at started + 10 s, which is freeze + 7 s and still inside the 8 s rollback window.
  4. The verifier sees a partial state, and `emergency_close` terminates the game without saving.
- **Minimal fix:**
  - Start the watchdog's effect deadline at the freeze acknowledgement: freeze + `REVERSE_MS` + margin. Keep a separate bootstrap deadline.
  - Or pass `started` to the helper and have it refuse to freeze if `started + WATCHDOG_MS - REVERSE_MS - margin` has already passed.
- **Related:** the watchdog's 2000 ms bootstrap check (`:1016-1019`) also hashes the full exe while the helper is hashing it, so a cold disk cache can cause spurious "Watchdog bootstrap expired" refusals.
- **Codex verdict:**

### M11. The game write gate only serializes within one process
- **Status:** Reported. That the gate is per-process is confirmed. That a real overlapping provider write happens has not been traced end to end.
- **Where:**
  - The gate: `src/session_safety.rs:21-28`, a `static Mutex<()>` documented as serializing "across the independent browser and F8 services".
  - It's the only lock taken by `src/owned_transaction.rs:120`.
  - The provider is a separate child process (`src/provider_worker.rs:228`, `--native-mod-provider`) that runs the arena, ball, scoreboard, presentation and outfit writers. Each takes its own copy of the gate.
- **Defect:** writes from the provider process aren't serialized against frozen structural or arena-clone transactions in the launcher process.
- **How it happens:**
  - During a launcher arena-clone transaction, the provider publishes an arena template (`src/arena_v2.rs:364-370`, 78 bytes at `record+0x7E`).
  - `entire_catalog` (`src/roster_v2_arena_clone_access.rs:604-624`) hashes every arena row except the target, so the post-effect check fails and the game is emergency-closed.
  - The area reviewer flagged one assumption as unverified: that those provider records are in the same directory the clone hashes.
- **Minimal fix:** use a named mutex per session (for example `Local\NBA2K27Write-{session}`), taken by both processes wherever `write_gate()` is called now. Alternatively, pause the provider over its control pipe before a freeze.
- **Codex verdict:**

### M12. One transient error permanently holds the native session
- **Status:** Confirmed
- **Where:** `src/native_session.rs:1206-1214` sets `c.error` on any `observe` error. The only place `error` is reset is `load` (`:187`). `observe` propagates via `self.provider_view()?` at `:694`.
- **Defect:** these ordinary failures all become a permanent "needs review" state for the rest of the launcher run:
  - a `provider-state.json` open failing with something other than NotFound (for example access denied during the provider's atomic replace)
  - a `worker.exited()` error
  - a commit rename that fails after retries
  - a `mod_pending` I/O error
- **How it happens:** after one failed read while in game, `observer_binding` refuses Rules, the roster binding and the F8 overlay. `clear_with_closed` also refuses while `error.is_some()`. Nothing works again until the launcher restarts.
- **Minimal fix:**
  - Classify `provider_view`, `exited` and the I/O check failures as transient: set the status message, return `Ok`, and retry on the next tick.
  - Keep `c.error` for journal corruption or ownership conflicts only.
- **Codex verdict:**

### M13. No `.gitattributes`, so a default Windows clone corrupts hash-pinned embedded assets
- **Status:** Confirmed by the test run.
- **Where:**
  - The repo root has no `.gitattributes`.
  - Example: `src/roster_v2_arena.rs:10` (`PROOF_HASH`) and `:39-46` (`proof()`: `include_bytes!("../assets/roster-v2-arena-proof.json")`, then a SHA-256 compare).
- **Defect:**
  - Git for Windows installs with `core.autocrlf=true` in its system gitconfig, so checkout converts LF to CRLF in text files.
  - `assets/roster-v2-arena-proof.json` grows from 3,923 bytes to 3,997 bytes (74 CRs added), and its hash no longer matches.
- **How it happens:**
  - A fresh `git clone` followed by `cargo test` made 27 `roster_v2_arena`/`roster_v2_readonly`/`roster_v2_write` tests fail with "Arena proof unavailable: Arena proof content changed".
  - Because the file is embedded with `include_bytes!` in non-test code, a **release** built from that clone fails the same check at runtime. Every Version 2 arena feature would refuse to run.
  - After `git config core.autocrlf false` and a re-checkout, all 164 `roster_v2_` tests passed.
- **Minimal fix:**
  - Add `nba2k27-launcher-tauri2/.gitattributes` (or one at the repo root) that keeps hashed inputs byte-exact, for example:
    ```
    assets/** -text
    browser-overrides/** -text
    ```
    Add any other folder whose files are hashed or `include_bytes!`'d. Or use `* text=auto eol=lf`, plus `-text` for the hashed assets.
  - Then run `git add --renormalize .`.
- **Codex verdict:**

### M14. Owned-transaction freeze is pinned to one exact `ntdll.dll` build
- **Status:** Confirmed by the test run.
- **Where:** `src/owned_transaction_win.rs:229-241` hashes the loaded `ntdll.dll` and requires it to equal `NTDLL_SHA`.
- **Defect:** `ntdll.dll` changes in most monthly Windows cumulative updates. The supported set is a single hash.
- **How it happens:**
  - On this review machine (Windows 11 Home 10.0.26200), `custom_arena_profiles::launch_tests::enabling_requires_explicit_native_consent_and_remains_future_launch_only` fails with "Owned freeze is unsupported on this ntdll build".
  - For users, every feature that goes through `owned_transaction` (arena clone, structural season edits) silently becomes unavailable after an ordinary Windows update, until a new launcher build ships.
  - It fails closed, which is the safe direction, but this is a predictable outage.
- **Minimal fix:**
  - Pick one:
    - Check the specific property the code relies on, for example the exported function signature or bytes of the routine being used, rather than the whole-file hash.
    - Keep an allowlist of known hashes, plus a clear UI message that names the reason.
  - Separately, make the test independent of the host: inject the ntdll check, or mark the test `#[ignore]` when the hash doesn't match.
- **Codex verdict:**

---

## Low severity

Most are reported by the area reviewers and not independently re-checked. Verify before fixing.

**Security / IPC**
- **L1.**
  - **Issue:** overlay windows (`overlay`, `live-overlay`) get the whole `allow-launcher` command surface (`capabilities/main.json`). In `src/main.rs:1627-1641`, unmatched channels fall through to `backend.dispatch` without checking which window sent them.
  - **Fix:** add a per-window channel allowlist, or give the overlays their own capability. This is least-privilege hardening; the CSP and React escaping already make injection unlikely.
- **L2.**
  - **Issue:** both loopback servers can be stalled by any local web page that holds sockets open:
    - `src/jukebox_http.rs:31-35` serves one connection at a time on the accept thread.
    - `src/browser_workspace.rs:796-798` allows at most 8 workers of 3 s each.
  - **Effect:** availability only. No data is exposed.
  - **Fix:** use a capped worker per connection, and drop connections that stay idle for more than about 200 ms.
- **L3.**
  - **Issue:** the browser-editor token lasts for the whole session (`src/browser_workspace.rs:696-743`). It can remain in browser history through the opened URL fragment, and an extension with `127.0.0.1` access can read it.
  - **Fix:** rotate it or expire it on idle.
- **L4.**
  - **Issue:** network and user images are decoded by WIC inside the launcher process (`src/jukebox_artwork.rs:57-138`, `src/roster_artwork_import.rs:180`). There is no timeout during a WIC call. Input is bounded to 4 MiB and 8192 px.
  - **Fix:** decode in the existing decode-worker process.

**Jukebox / media**
- **L5.**
  - **Issue:** at import, custom-song preview PNGs only need the magic bytes and ≤ 4 MiB (`src/jukebox_package.rs:504-510`). The 256×256 check happens only at commit (`src/jukebox_local.rs:525-533`), by which time audio already staged by `stage_song` is left orphaned in `Library/Jukebox/Audio`.
  - **Fix:** use the same `png()` validator at import time.
- **L6.**
  - **Issue:** a stage temp file is left behind when `write_all`, `sync_all` or the rename fails, for example on a full disk (`src/jukebox_local.rs:413-425`, `:559-565`). Each can be up to 256 MB.
  - **Fix:** delete on the error path.
- **L7.**
  - **Issue:** `src/animation_codec.rs:599-604` and `:1125-1130` check `member.size() <= MAX_MEMBER`, then call `read_to_end` without a `.take()`. These archives are hash-checked game assets, so this is hardening.
  - **Fix:** use `.take(MAX_MEMBER + 1)` and check the length.
- **L8.**
  - **Issue:** a single 256 MB audio import peaks at about 1.2 GB of memory (`src/jukebox_decode_worker.rs:470`, `src/jukebox_api.rs:1225-1275`), because the input and the PCM are copied several times.

**File writes**
- **L9.**
  - **Issue:** `src/loader_files.rs:406-419`: `restore` checks the game-side `version.dll` hash even when `entry.published == false`, and a mismatch is a hard error. If a crash happens before install and the user then installs a different `version.dll`, every recovery fails. That also triggers the M1 chain, because the loader is restored first.
  - **Fix:** skip a mismatching target when the entry was never published.
- **L10.**
  - **Issue:** temporary files are never swept after a crash:
    - `.preparing-*` folders (`src/animation_preview.rs:265`; the prune at `:378-391` skips them)
    - `Cache/Imports/*.importing` (`src/backend.rs:1228`, `src/previews.rs:263`), up to 2 GB each
    - `.write-*.tmp` next to receipts (`src/backend.rs:222`)
  - **Fix:** sweep the launcher's own prefixes at startup.
- **L11.**
  - **Issue:** the reserved-name check in `src/mod_install.rs:102-112` misses `COM¹-³`, `LPT¹-³`, `CONIN$` and `CONOUT$`.
  - **Effect:** the likely result is an extraction error, not data loss.
- **L12.**
  - **Issue:** `src/saved_media_publish.rs:33-56` is dead code with hard-coded `CARGO_MANIFEST_DIR` and Steam paths, plus an `unwrap` at `:217-260`.
  - **Fix:** remove it or put it behind a cfg flag.

**Roster**
- **L13.**
  - **Issue:** one malformed name aborts the whole Version 2 roster capture:
    - `src/roster_v2_readonly.rs:268-284` decodes names with strict UTF-16 plus a control-character check, and the error propagates at `:631`, `:647` and `:663`.
    - `src/roster_v2_services.rs:538-539` (`staff_name`) also rejects 20-unit names and U+FFFD, which blocks rotation editing for that team.
  - **Fix:** decode leniently and mark the row read-only.
- **L14.**
  - **Issue:** legacy player, team and uniform saves (`src/roster_native.rs:1002-1021`, `:1155-1162`, `:1335-1342`) don't call `writer::any_pending(root)`, so pending Version 2 arena, appearance or season journals don't block them.
- **L15.**
  - **Issue:** text shown in the UI is garbled (`â€”`, `â€¦`, `â€™`) because these literals are double-encoded UTF-8:
    - `src/roster_v2_access.rs:493`, `1699`, `1801`, `1806`, `1990`, `2528`
    - `src/browser_workspace.rs:34`
    - `README-GITHUB.md`
- **L16.**
  - **Issue:** when the Version 2 tab cap is reached, it evicts `tabs.keys().next()`, the smallest ID, rather than the least recently used (`src/roster_v2_access.rs:2437-2444`). The tab being actively edited can be evicted. It fails safe, but with a confusing "snapshot expired" message.

**Frontend**
- **L17.**
  - **Issue:** a single poll error stays on screen after polling recovers (`frontend/LaunchReadiness.tsx:134-135`, `frontend/NativeArena.tsx:175-176`, `frontend/NativeOverlay.tsx:34-35`).
  - **Fix:** keep a separate poll error that is cleared on success, as `NativeOutfits` does.
- **L18.**
  - **Issue:** about 13–25 `native-session-status` IPC calls per second while in game:
    - the F8 overlay outfits poll every 100 ms
    - three arena hooks poll every 250 ms to 1 s each
    - five or so other pollers
  - **Fix:** use one shared poller.
- **L19.**
  - **Issue:** outfit piece picks have no pending or stale-reply guard (`frontend/OverlayOutfitEditor.tsx:317-324`). A late reply can undo a later "Remove". `App.tsx:1019` `pickAsset` has the same pattern but is dead code.
- **L20.**
  - **Issue:** a wardrobe load error is swallowed by `.catch(() => {})` (`frontend/OverlayOutfitEditor.tsx:372`). The picker shows as empty with no error.
- **L21.**
  - **Issue:** if `refresh()` fails after any update action, `LauncherUpdatesCard` disappears entirely (`frontend/SettingsTools.tsx:68-95`).
  - **Fix:** keep the last info and show the error.
- **L22.**
  - **Issue:** a cancelled 2KDB import leaves "Opening the ratings source…" showing (`browser-overrides/player-dna.js:101`).
- **L23.**
  - **Issue:** `frontend/SeasonHistory.tsx:309,343`: clearing a number field stores 0, and year 0 renders as "-1–0". The backend rejects it on save.

**Process memory / transactions**
- **L24.**
  - **Issue:** every successful structural season edit leaves a `.pending` intent until the game exits. `inventory()` fails beyond 128 of them (`src/season_structure_v2_native.rs:950-983`, `:1014-1024`).
  - **Effect:** that check is part of `roster_v2_write::any_pending`, so after 128 edits in one game run all Version 2 roster writes fail. After a launcher restart with the game still running, every earlier successful edit counts as pending.
  - **Fix:** archive or mark verified intents durably once independent verification succeeds.
- **L25.**
  - **Issue:** the helper's `Context::drop` loop (`src/owned_transaction_win.rs:637-645`) can retry forever if the watchdog has died and `same()` keeps failing. The game stays frozen, and the parent blocks in `child.wait()` while holding the write gate.
  - **Fix:** bound the loop, and terminate after N identity failures on the handle that was authenticated at bootstrap.
- **L26.**
  - **Issue:** two minor problems:
    - `assert_unowned` (`src/owned_transaction_win.rs:315`) refuses the whole transaction if any game thread exits between the snapshot and `OpenThread`. It fails safe, but causes occasional spurious refusals.
    - `:1239-1247` reports "full consistent state independently verified" even when the helper ended before freezing and nothing was verified.

---

## Not verified

- **Build and tests:**
  - **Rust:** toolchain rustc 1.99.1 (MSVC) with VS 2022 Build Tools 17.14 on Windows 11 10.0.26200.
    - `cargo test --locked --no-default-features --bin nba2k27-launcher-tauri2 --no-run` compiled in 2.5 min with 34 warnings and no errors.
    - The README's three test groups, run with `--test-threads=1`, gave **1,496 passed, 1 failed, 54 ignored**: `roster_v2_` 164 passed / 4 ignored, `player_catalog` 13 passed, the rest 1,319 passed / 1 failed / 50 ignored.
    - This matches the README's 1,497 passed / 54 ignored. The single failure is M14 (host ntdll differs).
    - The first run, on a default CRLF checkout, also had 27 extra failures; see M13.
  - **Release build** (`cargo build --release` with `custom-protocol`): not run.
  - **Frontend:** not run, because Node and pnpm aren't installed on the review machine (`pnpm test`, 224 tests per the README). The browser regression scripts and native fixtures weren't run either.
  - **Warnings:** the 34 compiler warnings were not triaged.
- **Live game behaviour:** none of this was run against NBA 2K27. Memory addresses, menu-boundary timing, overlay behaviour, camera, ball, arena and Jukebox playback, and real cleanup after the game exits are all outside what a code read can show.
- **Runtime-dependent claims:**
  - H1's exact peak memory depends on the allocator and how fast the worker runs. The missing dedupe and late budget check are certain from the code.
  - M4 and M5 depend on the user choosing a malicious folder or package.
  - L2's stall duration depends on the browser's connection behaviour.
- **Library behaviour not checked against source:**
  - whether `zip` 4.x limits `read_to_end` to the declared size (L7)
  - exact `MoveFileEx` semantics on network or FAT volumes
- **Not reviewed in depth:**
  - `native/` C/C++ loader sources and prebuilt payloads
  - `tools/` and `scripts/` (PowerShell/Node build helpers)
  - the vendored WPF browser bundle, which was only checked for `innerHTML` and eval sinks
  - the many `*_tests.rs` files, except where a finding cites one
  - the 90 QA `.ps1` files at the root

## What looked solid

The codebase is unusually defensive. Each reviewer called out these as well built:

- **Tauri surface:** only 4 commands. Window and URL `trusted()` checks. `devtools(false)` and navigation lock on every window. No fs, shell or http plugins. A strict production CSP. The asset-protocol scope starts empty and is set at runtime.
- **Local browser server:** a 256-bit token from `BCryptGenRandom`, compared in constant time. An exact `Host` check (blocks DNS rebinding). `Origin`, Content-Type and custom-header requirements (forces a CORS preflight). Bounded `httparse`. An allowlisted, hash-checked static file set.
- **Outbound HTTP:** WinHTTP over TLS only, with manual redirects re-checked against a host allowlist, deadlines and size caps. The ratings WebView is InPrivate and locked down.
- **Archives:** the ZIP, tar and `.2kplaylist` parsers reject traversal, ADS, links, ZIP64, encrypted and duplicate entries. Reads are bounded and CRC-checked.
- **Game-folder publishing:** handle-based no-replace renames, deletes on the exact verified handle, `OPEN_REPARSE_POINT` throughout, and write-through receipts. The last copy of a player package is never deleted.
- **Roster encoding:** bit-packing reads and writes round-trip exactly. Pointer arithmetic on game memory is checked. Writes are confined to the leased record, and every patch is verified before and after.
- **Decode worker:** a Job object with kill-on-close, a memory cap and a process limit. Pipes are drained on separate threads, there is a deadline, and output is bounded.
- **Frontend:** launch double-submit guards, generation counters against stale replies, every interval and `listen()` cleaned up, and epoch checks in the `browser-overrides` scripts.
- **Process memory:**
  - Identity is PID + creation FILETIME + image path, re-checked on retained handles before and after every write.
  - Every write checks `done == len` and reads the bytes back.
  - Page protection is always restored and confirmed with `VirtualQueryEx`.
  - Atomic JSON is written as temp file, `sync_all`, then `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)`.
  - Native struct layouts are correct, there's no `unwrap` on data read from the game, and the lock order is consistent.
