# 0.51: WNBA teams in Freestyle (experimental), implementation note

**What it is:** an opt-in route that makes NBA 2K27 Version 2 Freestyle offer NBA and WNBA teams, as WPF did. It rebuilds the WPF-era 403-row mixed roster locally and then applies WPF's Freestyle category write.

**Background:** `artifacts\wnba-merge-20261017.md`. **WPF reference:** `docs\wpf-reference\unlocks-wnba.md`. **Live checklist:** `docs\qa051-wnba-freestyle-live-checklist.md`.

## Flow

1. **Settings** (`backend.rs`): `wnbaFreestyleMixed` (default **false**) and `wnbaFreestyleVariant` (`wpf-era` default, `national`, `clubs`). They are new keys, so a WPF-migrated `unlockWnbaFreestyle` can never turn this on.
2. **Plan** (`feature_plan.rs`): `wnba_mixed: Option<Variant>`. It counts as a Freestyle feature, and it adds `roster/learn2k_roster.iff` as the forced `OVERRIDE` slot (camera + music + cover + roster ≤ 4). It also suppresses the withdrawn `wnba_freestyle` loader request.
3. **Pre-launch** (`provider_launch.rs`): publish a cached build that still matches its pinned hash. If there is no cached build but a key was saved, rebuild and publish. A failed build retries in-session. If the user's profile already has that path, the user's mod is kept and the feature is held.
4. **In-session** (`feature_runtime.rs`):
   - Read the roster key read-only, from the same manager as the probe (`0xBB9A8E8+0xD8/+0xE0`).
   - Build in a background thread (`wnba_roster_v2::build`). The provider then publishes the result as a virtual asset (`provider_assets::publish_wnba_roster`).
   - Until then the proxy's forced override reports "missing" and reads stock.
5. **Categories:** the existing `freestyle_tick` (4 DWORDs, journaled, guarded) runs once the roster is published. It uses the **strict** mixed verifier (`freestyle_v2::capture`): 15 WNBA clubs, national {69, 361}, 30 NBA, kind 1 and kind 27 present.
   - The `clubs` variant never writes categories.
   - `wnba_loader_v2::RUNTIME_ENABLED` stays `false`.
6. **Build integrity** (`wnba_roster_v2.rs`, `rdat_roster.rs`, `vcblowfish.rs`):
   - **Stock inputs:** both stock entries are read through the archive index `0A` (key = `archive_key(path)`, packed size = u32@+12 >> 1, position = u64@+16 >> 23). They are pinned by the SHA-256 of their packed bytes.
   - **Decryption:** VCBLOWFISH (pure Rust; pi constants computed and checked against Schneier vectors), then Oodle (installed DLL), then the authored length and CRC.
   - **Salt and CRCs:** the CRC salt is read from the exe and checked by its own CRC. Every bundle CRC is verified.
   - **Merge:** a byte-exact port of `tools\roster_merge_wnba.py`.
   - **Re-parse checks:** CRCs, every reference in range, 30 NBA / 15 WNBA, 403 teams for wpf-era, and national {69, 361}.
   - **Output pin:** the output must equal the offline-verified SHA-256 for its variant, or it is not served.
7. **Key handling:** the key is saved to `Portable\Data\wnba-roster-v2\roster-key.bin` only after it decrypts both pinned rosters. It is never logged and never shipped. The support bundle collects only `Portable\Logs`.

## Divergences from WPF

1. **The roster is rebuilt.** WPF's old-build stock `learn2k_roster.iff` was already mixed (403 rows), and WPF never touched files. V2 ships the NBA and WNBA Learn2K rosters separately, so the launcher rebuilds the WPF-era roster locally and serves it through the version.dll provider. WPF's version.dll had no WNBA role.
   - The team table matches the WPF snapshot row for row (ID, type, name, order).
   - Players: 5,563 vs 5,590 (27 unidentified rows).
   - 53 copy players and 6 staff got new unique IDs, and the copy players are not in the player hash index.
2. **A roster key is needed.** On the first session the key is read in-session, so the mixed roster is ready only after boot. Freestyle entered earlier still shows stock; re-enter it. WPF had no such step.
3. **Default off and experimental.** WPF defaults `unlockWnbaFreestyle` to on.
4. **Category write.** It is the same 4-DWORD WPF `category-*` route and allowlist, and it is still armed only during Play. It uses the strict mixed verifier, as WPF does: 15 WNBA, 2 national, 30 NBA, Classic, All-Time.
5. **Payload.** The provider serves the decoded IFF (stored zip), as the camera route does. If live QA shows the stock roster is still read, serving the re-wrapped 0x84 file is the follow-up.
6. **Variant picker.** WPF had none. It exists for crash bisection.

## Tests

- **Full Rust suite:** 1115 passed, 0 failed (before the final index-reader change). The focused groups re-run afterwards all pass: `wnba_roster_v2`, `rdat_roster`, `vcblowfish`, `feature_plan`, `feature_runtime` (17), `freestyle_v2`, `provider_assets`.
- **New tests:**
  - Blowfish vectors.
  - Bit packing and the chunk padding rule.
  - Key capture on fake images: wrong build, wrong pointer, zero key.
  - Key storage.
  - Variants.
  - Plan opt-in and the forced slot. A WPF-migrated flag alone does nothing.
  - Runtime route transitions.
  - Two env-gated stock tests (`--include-ignored` with `C27_WNBA_ROSTER_DECODED`, `C27_GAME_EXE`, `C27_WNBA_ROSTER_KEY`), both passing on this machine:
    - the Rust merge equals the offline SHA-256 for all three variants;
    - the full build from the installed archives with the captured key works, plus cache reload and rejection of a wrong key.
- **Frontend:** vitest 193/193; `tsc --noEmit` clean. The parity smoke expectation is updated (2 guarded unlocks, plus Freestyle enabled and unchecked by default).

## Changed / new files

New:
- `src\wnba_roster_v2.rs`
- `src\wnba_roster_v2_tests.rs`
- `src\rdat_roster.rs`
- `src\vcblowfish.rs`
- `docs\qa051-wnba-freestyle-*.md`

Edited:
- `src\main.rs` (modules)
- `src\backend.rs` (settings)
- `src\feature_plan.rs` (plan, override, test)
- `src\feature_runtime.rs` (mixed route, category gating)
- `src\provider_assets.rs` (publication)
- `src\provider_launch.rs` (pre-launch and in-session publication)
- `frontend\UnlocksPanel.tsx`, `frontend\UnlocksPanel.test.tsx`
- `frontend\App.tsx`, `frontend\types.ts`
- `frontend\parity-smoke.ts`
- `frontend\wpf\renderer\unlocks-polish.css`

Also changed:
- `tools\roster_merge_wnba.py` (staff ID offset fix)
- `tools\test_roster_merge_wnba.py`

Backups of the edited files are in `Backups\qa051-before-wnba-mixed-roster\`.
