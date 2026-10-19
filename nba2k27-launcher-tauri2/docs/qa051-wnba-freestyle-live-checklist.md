# 0.51 live test: WNBA teams in Freestyle (experimental)

This feature is off by default. Turn it on under **Unlocks → WNBA Teams in Freestyle (experimental)**. The roster picker defaults to **WPF-era mixed roster (403 teams)**.

**What happens on Play**
1. If a verified build is already cached in `Portable\Data\wnba-roster-v2\`, or a key was saved there by an earlier session, the launcher builds and publishes the mixed roster before Steam starts.
2. **First session only:** the launcher reads the roster key from the running game (read-only). It then builds the roster from your installed files and publishes it.
   - The feature status moves through `preparing` → `building` → `publishing` → `roster-ready`.
   - Stay out of Freestyle until it shows **roster-ready**.
   - If you entered Freestyle earlier, return to the main menu and enter Freestyle again.
3. In the Freestyle team picker, the launcher deep-checks the loaded roster. It needs 15 WNBA clubs (300–311, 315–317), USA 69 and 361, and 30 NBA teams. It then writes WPF's 4 category-order DWORDs. Status: **active**.

**Probe:** `tools\Read-WnbaRouteState.ps1 -Label <name>`. It is read-only (QUERY_INFORMATION|VM_READ) and saves JSON under `artifacts\`.

## Checklist

1. **Home**
   - [ ] `rosterHooksStock: true`
   - [ ] Roster count 370 (the default `roster.iff`)
2. **Wait for the launcher.** The feature status shows `roster-ready` ("Mixed roster ready (403 teams: 30 NBA, 15 WNBA)").
3. **Freestyle team select**
   - [ ] The probe shows `TKU_DELEGATE`, count **403**, nbaCount **30**, wnbaCount **15**, `mixed: true`, nationalIds [69, 361]. The WNBA rows are at indices 161–175.
   - [ ] The launcher status is **active**.
   - [ ] Cycling categories reaches the WNBA category.
   - If the count is **334**, the stock roster was read. Go back to the main menu and re-enter Freestyle. If it is still 334 after the status shows roster-ready, see "If it fails" below.
4. **NBA vs NBA first**
   - [ ] Pick two NBA teams, play through court entry and tip-off, then quit to the menu. This is where 0.41 crashed.
5. **WNBA vs WNBA**
   - [ ] For example Aces vs Liberty: play through court entry, tip-off and about a minute of play.
6. **Mixed**
   - [ ] One NBA team vs one WNBA team, through court entry.
7. **Back at Home**
   - [ ] The probe shows `roster.iff` restored (370).
   - [ ] Re-enter Freestyle: 403 again.
   - [ ] The category stays enabled (status stays active, with no rewrite).
8. **Restart Play**
   - [ ] The second session publishes before Steam (cached build): status is `roster-ready` from the start.
9. **Toggle off and Play**
   - [ ] Freestyle is stock again (334 teams, no WNBA category).
   - [ ] Nothing is written.

## If it fails

| Symptom | Switch to | What it isolates |
|---|---|---|
| Crash at court entry with any WNBA team | Roster: **Clubs + WNBA USA (350)** | The WPF-era extras: All-Star/Extras teams, 53 appended players not in the player hash index, and the 403-row insert |
| Still crashes with national | Roster: **Clubs only (349)**; categories stay stock | No players are added and the stock player hash index stays exact. Shows whether the crash comes from the roster file or from the category write |
| Crash even with NBA vs NBA on the mixed roster | Turn the feature off; report the probe JSON | The team table insert or the header (fingerprint, size) |
| Count stays 334 after roster-ready | Report the loader log `LIVE_FORCED_OVERRIDE_4` line | The decoded file was not taken through the override (a payload-format question; follow-up: serve the re-wrapped 0x84 file instead) |
| Status held: "selected mod profile already provides roster/learn2k_roster.iff" | Remove that mod from the profile | A user roster mod is never replaced |
| Status held: "did not decrypt" / "differ from the verified Version 2 files" | — | Game update or different files; nothing is served |

Keep every probe JSON and `Portable\Logs` from a failing run. **Never** attach `Portable\Data\wnba-roster-v2\roster-key.bin`.
