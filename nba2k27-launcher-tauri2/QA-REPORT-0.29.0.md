# 0.29.0 manual Blacktop outfit QA

Offline package verified October 16; not installed over Main. Main, Data and Library are preserved. QA0.28 remains frozen for rollback. The full feature goal remains active.

**Game Version 2 is unsupported:** the installed October 16 executable has PE timestamp 6AC53765, image size 35BDE000 and SHA-256 29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1. Steam build ID is 25761995. Neither the native provider nor Blacktop memory guards accepts this build. The provider check precedes mod/profile/loader publication. Passing offline tests do not establish gameplay compatibility; no live test was performed. Keep build guards intact while mappings and native components are revalidated.

## Manual outfit controls

F8 Edit outfit now reads actual selection-screen players and connects explicit Apply outfit and Restore original to the native provider. A dedicated worker keeps native scans, refresh confirmation and match handoff off the UI, asset-service and arena/ball polling threads. The offline preview shows the same player and action controls with a clearly labeled dummy player.

Reading players uses a read-only game handle. Writes require explicit per-game enablement and a fresh native player binding. The worker revalidates exact process birth, supported build, model/description/roster identity, clothing pool and current source. New private pages are non-executable, sealed read-only and journaled before exposure; only the mapped source/count and refresh byte are changed. The original uniform reference is not written. Pages exposed to the game remain allocated until exit.

Only confirmed selection refresh adds a match choice. The unchanged proven native DLL receives exact original V3 plan bytes after durable ownership and single-use consumption. Stable match samples precede submission. Confirmation checks the native report and expected rebuilt count. Main Menu and Quick Play clear pending choices without resetting native match numbers; rearming requires stable selection and the next native armed report. Uncertain outcomes hold instead of replaying.

Match carry-over requires 3–7 pieces and at most three accessories. Two-piece outfits can refresh at selection but are not eligible for native match carry-over. Uniform pieces require the selected player's verified context. Shirtless is manual only. Automatic fitting, random player/all gameplay modes, scoreboards and presentation executors remain unfinished; draft randomization does not apply to the game.

## Regression coverage

The final pipeline passes 371 native tests, zero failures, with three opt-in disk audits excluded; the full run took 43.21 seconds. All 36 frontend tests, strict TypeScript and Vite pass. New coverage includes strict authenticated outfit requests, coalescing, read-only versus write capabilities, stable player binding, selection confirmation, owned pages, file sharing, plan publication, native report consumption, repeated-match rearming and cleanup failures.

Preserved C# and PowerShell oracles check 3,582 layout cases, twelve exact private-page cases and fifteen original match-plan cases. Automated memory tests use only their own process, not NBA 2K27. The packaged EXE passes 100 UI checks plus two normal-affinity fresh-profile repeats, 100 each. The live-overlay fixture has no invented player and cannot enable Apply/Restore without an owned provider; smoke refuses live outfit access. Preview and native-fixture screenshots were inspected. Browser authentication, no-game-save rejection, WPF sharing validators and provider shutdown pass. The worker closed in 82.496 ms; repeat UI-ready times were 377 and 348 ms, not cold-start guarantees. Original C# oracle compilation emits unused-field warnings; the Rust optimized build passes without warnings.

Final evidence: artifacts/build-v0290-outfit-fixture-fix-20261016.log; artifacts/qa029-final-release-verification.log; Builds/0.29.0/QA-VERIFICATION.json. Pipeline profile: artifacts/smoke-78119f300ac048629169c85fccb51ef9. The first packaging attempt caught a stale disabled-player test expectation; the corrected fixture now verifies the new player selector and its write gates. Its failed log is retained separately.

## Live verification still required

First revalidate Version 2 mappings and the native loader/worker; do not launch this milestone for Version 2 gameplay. Once a compatible QA is built and a live test agreed, use the mod-profile launch. At Blacktop selection, enable Native F8, open Edit outfit, refresh players, choose a player, review a compatible 3–7-piece draft, enable changes and Apply. Wait for confirmation, then inspect selection and match appearance. Repeat after Quick Play and Main Menu re-entry, test Restore original, and exit normally to inspect cleanup. Do not repeatedly retry held errors.

Full selected-profile mod appearance, generated selectors, additional arena/ball compatibility, current F8 focus, Game Rules writes and browser editor writes also retain their existing acceptance requirements. A functioning catalog or saved preference is not verified gameplay.

## Rollback

Backups/v0.28.0-before-blacktop-outfit-port-20261016 is the hash-verified 414-file source/release checkpoint, excluding user Data and Library. Incremental checkpoints include Backups/v029-before-outfit-provider-ui-20261016-132525 and Backups/v029-before-qa-packaging-20261016-134357.

Frozen QA0.28 EXE SHA-256: 46D2592AE6C138E0E6CED50FA456ABE5B956724E744FB08438307F4917DC8321. Expected Main EXE SHA-256: 5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E. The unchanged native DLL remains ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF.

Offline deliverable: Builds/0.29.0/Portable/App/2k Launcher Rust Prototype.exe; 28,278,784 bytes; SHA-256 320C9C93D498764F9C3355CB7F0D41E8A8498640078F95493368819B106A1A67. No installation or promotion is authorized by this QA milestone.
