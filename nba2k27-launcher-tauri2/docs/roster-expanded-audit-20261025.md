# Native roster editor version audit

The current target is game SHA-256
`B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B`,
timestamp `6AD2E48C`, image size `36532000`.

The earlier disk-only audit missed two independently gated dependencies:
the in-process appearance DLL and the initialized ClassicYear descriptor.
Those were fixed in the installed dev build before this follow-up audit.
This follow-up expands regression coverage rather than changing runtime guards.

## Scope

`scripts/Test-RosterProfiles.ps1` now runs all of these and collects failures
across groups, with separate logs under `artifacts/roster-profile-audit`:

- Actual native-editor build routing and appearance launch admission.
- Ten production Rust profile groups: owner/research, player/team loading,
  Save & verify (including uniform and team metadata), staff/seasons/rotations,
  arenas, origins, name/audio, appearance, appearance pools, and structural
  season/arena dependencies (including the Rules adapter).
- Each group at the preferred image address, at another address using the
  executable's actual DIR64 relocation table, and at a third address with the
  proven ClassicYear reflection initialization replayed.
- Actual provider disk admission, including the embedded native DLL identity.
- The complementary roster Rust/JSON fingerprint inventory.
- Native measurement C++ method/root/vtable/unit parity with its Rust evidence.
- Both actual C++ runtime profile functions from `outfit_match_v2_win.hpp` and
  `roster_appearance_v2_win.hpp` against an isolated SEC_IMAGE mapping. The
  fixture applies actual relocations, preserves section protections, and tests
  changed-method rejection and restoration for each validator.

The native fixture maps the file as data in its own process. It does not use
LoadLibrary, execute game code, resolve game imports, open a game process, or
write the executable. Its deliberate mutations are private copy-on-write pages.
The Rust replay likewise reads the executable and modifies returned fixture
buffers only. No game, mod, roster, renderer, or installed launcher is changed.

## Legacy checks reviewed

The old provider build branch is intentional multi-build recognition; the
current build takes its separate Version 2 route. Legacy roster saves remain
isolated from the Version 2 backend. The historical hash in `rules_v2.rs`
identifies its embedded getter catalog; current-image admission uses the
relocation adapter exercised by the structural profile test. Historical proxy
hashes are recovery-only; new publication requires the current DLL hash.
Unrelated legacy feature profiles were not repinned simply because they contain
an older timestamp or image size.

## Limits and delivery

Completed results: 140/140 inventoried fingerprints match; all ten Rust profile
groups pass in each of the three replay states; build routing, provider/DLL
identity, C++/Rust measurement parity, and both native validators pass. The
native fixture relocated 1,093,717 pointers at base `0x20000000000`. Targeted
ClassicYear mutation, V2 read/save routing, and current-DLL rejection tests also
pass. `Start-Dev.ps1 -CheckOnly` reports the installed dev build ready.

This is executable/profile and fixture validation. It does not reproduce every
possible runtime object state or perform a live in-game Save. The installed dev
build already contains the prior metadata and measurement fixes; this audit
adds no production changes requiring another installation. Release is unchanged.

Run the full audit with:

```powershell
./scripts/Test-RosterProfiles.ps1
```

Optional arguments are `-GameExe <absolute path>` and `-Python <python.exe>`.
Unknown game images fail validation; the audit never automatically updates
fingerprints or enables writes for an unproven build.
