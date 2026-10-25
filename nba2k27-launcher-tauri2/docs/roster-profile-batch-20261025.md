# Roster profile batch update — dev only

The installed game is `B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B`
(PE timestamp `6AD2E48C`, image size `36532000`). The earlier player-loader
update left the separately gated write, dictionary, arena, appearance, and
staff/season profiles on the preceding build. This caused the successive
serializer and `Version 2 roster write anchor differs` failures.

## Verification and changes

- Inventoried 138 code/data fingerprints across the roster runtime and proof
  assets. Initially 123 failed; the updated inventory has zero mismatches.
- Recovered the previous pinned method bodies from saved audit evidence and
  fixtures, checking their original SHA-256 hashes before comparison. Compared
  every instruction, record offset, mask, constant, and internal branch;
  allowed only the documented PC-relative relocations. The retained report
  records each old/new call or data target. Runtime still requires exact bytes
  and hashes; this offline comparison does not relax a runtime check.
- Independently re-extracted all 52 writable uniform controls, 81 display
  fields, five team text capacities, and the 32-slot uniform directory.
- Independently verified 308 staff schema/control comparisons, editable masks,
  enum tables, 32 season getter/setter pairs, and the player season directory.
- Updated appearance getters/setters and named unit references, dictionary and
  pool methods, team metadata, arena evidence, and the structural helper's exact
  game-image hash. Preserved historical journal tags as historical only.
- Updated the stale research serializer fingerprint as well as its disk test.
- Fixed ratings capture initialization: the host dispatches `player-editor:open`
  before showing the modal. The button now refreshes after that event and on
  URL input. Visibility, destination identity, cancellation, source validation,
  review/staging, and explicit Save & verify remain enforced.

## Repeatable whole-editor check

The later [expanded native audit](roster-expanded-audit-20261025.md) adds
initialized metadata, real PE relocation replay, build routing, and the native
measurement DLL's production validators to the command below.

Run `scripts/Test-RosterProfiles.ps1` (optional `-GameExe <absolute path>`).
It compiles the dev test target and reads the installed executable only. It
does not open the game, install the launcher, or sync/build Exporter.

The disk test verifies the full image hash and evaluates all ten groups before
asserting: owner/research, player/team loader, Save & verify, staff/seasons/
rotations, arena catalog, origins, name/audio, appearance, appearance pool,
and structural season/arena dependencies (including the current Rules adapter).
The old-image hash in `rules_v2.rs` remains the identity of its embedded legacy
getter catalog; its existing current-build relocation adapter passed the actual
profile check. It is not an unconverted live-image gate.

`tools/audit_roster_current.py` provides the complementary inventory report.
The `roster_profile_evidence`, `compare_roster_relocations`,
`prove_roster_current_layouts`, and `apply_roster_current_evidence` tools and
their artifacts record this specific migration; do not repin a future build
merely by replacing hashes. The migration writer is intentionally one-time.

## Results and delivery

- 170 roster regression tests passed; 8 explicit-environment tests remained
  ignored in the ordinary run. Two initial sandbox parent-directory handle
  failures passed when rerun without that restriction.
- All ten real profile routines passed against the installed executable.
- 32 browser import/staging tests and five relocation-corruption tests passed.
- Dev build succeeded; installed executable SHA-256:
  `4230EDADFF993D9DC86B47937034A6B1B0E7B744C57042AAD74E4F92A7DBC913`.
- `Start-Dev.ps1 -CheckOnly` passed. Installed renderer pointer, preview worker,
  and release executable hashes were unchanged. No Exporter sync was run.
- Source backup: `Backups/before-roster-batch-profile-20261025`.
  Previous dev executable: `Backups/before-roster-batch-install-20261025`.

No game process was opened and no game/mod/roster files were changed. Disk and
synthetic tests establish profile and transaction behavior; an actual in-game
Save and real website capture still require user testing. Open `Start Dev.vbs`
and refresh the roster before testing Import ratings and Save & verify.
