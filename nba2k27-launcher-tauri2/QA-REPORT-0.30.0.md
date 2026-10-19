# 0.30.0 Version 2 mod loading QA

This isolated QA targets mod loading first on the October 16 Version 2 game update. It does not replace Main or migrate user Data and Library. The full feature goal remains active.

## Version 2 scope

The installed executable is 870,548,760 bytes, PE build 6AC53765 / 35BDE000, Steam build 25761995, SHA-256 29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1.

The unchanged native provider's structural resolver identifies one valid manager, device-base, path-builder and reference-release layout. Its 47 virtual-method targets match the disk image. The Rust gate additionally verifies eight ABI code sites, all 47 table entries, section bounds/permissions and preferred image base before permitting publication. A retained read-only file handle pins the image against replacement. A PE timestamp alone is insufficient. Native runtime checks still precede activation.

The read-only Blacktop observer has a separate Version 2 profile with eight RIP-relative references, four field signatures and three primary class tables. Native F8 and the session observer use that read-only factory. The strict legacy editor reader is retained for arena and other native writers.

Version 2 arena, ball and outfit requests are refused before queuing or invoking a game gate. Their UI controls require an explicit mapped-write capability. The Rust outfit worker is not started for Version 2. Roster, Game Rules and other native memory editors retain their legacy build guards. No old memory address is authorized by the new provider or observer profiles.

## Mod test

Close NBA 2K27 and other launcher versions normally, open Builds/0.30.0/Portable/App/2k Launcher Rust Prototype.exe, and select the installed game and complete existing Mods folder. Use Launch with mod profile (QA). Inspect a known player such as Amir Johnson and other representative mods. No F8 memory edits are needed. Exit the game normally and wait for provider cleanup; report any held/error state without repeated retries.

The existing full-folder route, shared-data links for named player packages, coordinated selector validation/generation and exact-owned restoration remain. This does not guarantee every third-party mod is valid. Game appearance and normal cleanup on Version 2 require live acceptance.

## Verification

All 378 native tests pass with zero failures; five opt-in disk audits are excluded from the ordinary suite. The full native run took 41.78 seconds. The frontend passes strict TypeScript, production bundling and 37 tests. The packaged EXE passes 100 UI checks plus two fresh-profile repeats, 100 each. The repeated normal-affinity UI-ready times were 349 and 346 ms, not cold-start guarantees. Browser authentication, no-game-save rejection and WPF sharing validators pass. The owned provider-worker fixture exits in 82.616 ms. Automated fixtures never open or modify NBA 2K27; explicit compatibility audits read only the installed EXE on disk.

Final package evidence is Builds/0.30.0/QA-VERIFICATION.json, artifacts/build-v0300-version2-mod-first-20261016.log and artifacts/qa030-final-release-verification.log. The packaged confirmation and native outfit-catalog screenshots were inspected. The isolated EXE is 28,288,000 bytes, SHA-256 6AEC7F3A16541A12FB773B7348BDD18E75519EA231F9D7932E0C51D593BEF50F. No installation or promotion was performed.

Disk evidence includes artifacts/game-v2-native-provider-resolver-20261016.log, artifacts/game-v2-provider-abi-disassembly-20261016.txt and artifacts/qa030-provider-actual-disk-gate-final.log. Observer disk evidence is artifacts/qa030-v2-observer-actual-disk-audit.log. Failed intermediate compile/test logs are retained rather than presented as passes.

## Additional offline work

Cameras, scaler and WNBA team availability in Freestyle were added to the requested follow-up scope. The camera registry and three framing records have independent matching disk candidates; Freestyle category cycles and masks also have matching candidates and code references. The mutable scaler input and complete feature lifecycles still need verification. These are not enabled or ready for live testing in 0.30. Their evidence and remaining checks are recorded in artifacts/camera-scaler-freestyle-v2-progress.json. The first gameplay acceptance remains mod loading and normal cleanup.

## Rollback

Backups/v0.29.0-before-v2-readonly-profile-20261016 contains 439 hash-verified source/release files before this work, excluding user Data and Library. Frozen QA0.28 and QA0.29 remain unchanged; neither is a Version 2 fallback. Main was not replaced.

Main EXE SHA-256 remains 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E. The unchanged native DLL is ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF. The goal remains incomplete pending Version 2 mod-loading acceptance and the remaining feature ports.
