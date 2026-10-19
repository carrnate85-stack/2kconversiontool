# 0.31.0 mod launch ownership QA

This isolated update addresses the pre-dispatch owner check that blocked the first Version 2 mod test. Main, Data and Library are preserved. It does not enable cameras, scaler, WNBA or the unported native editors.

## Owner scan and error reporting

The 0.30 provider report confirms Steam was never dispatched. Its error was Another launcher owns or may start a game. Subsequent metadata checks found only the expected QA parent, no other 2K launcher, and no legacy App/Backend/node.exe. That does not identify which process caused the earlier scan to fail.

The snapshot code had a confirmed race: a process that exited after Toolhelp enumeration but before identity lookup could be retained as unknown. An unknown Node process is deliberately treated as a potential launcher backend. The updated query distinguishes Windows-confirmed absence or exit from unreadable identity. It checks the same retained process handle around metadata queries, avoiding PID-reuse assumptions. If synchronization access is denied, it falls back to the original query-only rights and skips only a definitively ended process; unresolved identities still block. Public identity requests refuse confirmed-ended processes.

Liveness uses a zero-time process-handle wait, not the exit-time output of GetProcessTimes, which is undefined for active processes. A process that genuinely returns exit code 259 is covered separately, since that code alone is ambiguous. See Microsoft's [GetProcessTimes documentation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes) and [WaitForSingleObject documentation](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobject).

Competing-owner errors identify up to four process names, PIDs and identity states, with a count for additional blockers. They do not expose command lines, full paths or authentication tokens. A launch stopped before Steam dispatch is no longer reported as Game ended. No processes are terminated automatically and no ownership exception is granted merely by PID or executable name.

## Version 2 scope and test

The 0.30 file-provider profile and separate read-only Blacktop observer are retained. File-provider disk checks cover eight ABI sites, 47 virtual methods and section permissions. Version 2 memory editing remains disabled. The installed game build is 6AC53765 / 35BDE000, Steam build 25761995, SHA-256 29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1.

Close QA0.30 and any other launcher normally. Open Builds/0.31.0/Portable/App/2k Launcher Rust Prototype.exe, select the installed game and complete existing Mods folder, and choose Launch with mod profile (QA). Inspect known mods such as Amir Johnson. If held again, copy the full new message before retrying. Exit the game normally and let owned cleanup finish. Live mod appearance and cleanup remain unverified.

## Verification

All 382 native tests pass with zero failures; six opt-in audits are excluded from the ordinary suite. The native run took 39.78 seconds. All 37 frontend tests, strict TypeScript and production bundling pass. The packaged EXE passes 100 UI checks plus two fresh-profile repeats, 100 each. Browser authentication, no-game-save rejection, WPF sharing validators and provider shutdown pass. The worker exits in 81.816 ms. Normal-affinity repeat UI-ready times were 400 and 367 ms, not cold-start guarantees. The packaged launch-readiness screenshot was inspected.

Evidence is Builds/0.31.0/QA-VERIFICATION.json, artifacts/build-v0310-retained-liveness-final-20261016.log and artifacts/qa031-final-release-verification.log. The isolated EXE is 28,313,600 bytes, SHA-256 89A57F85B8ED344C9908D7B8936D4E82F37FB58F46766EE35B9E47BD4CD387CB. It was not installed over Main.

The explicit native owner audits query process metadata only; they never dispatch Steam, open game memory, change game files or stop user processes. The original parent check passed while QA0.30 was open. The final attempt using that old PID correctly refused an ended identity; a process inventory confirmed QA0.30 was closed. A fresh idle gate audit passed with an owned diagnostic parent and no competing launchers. These observations do not establish what caused the original live failure. The failed ended-parent audit and interrupted preliminary build are retained. Regression cases cover known-live, confirmed-exited and unresolved identities, real owned exits including code 259, failed liveness queries, competing-owner diagnostics, reused parent identities and accurate pre-dispatch messages.

## Rollback and remaining work

Backups/v0.30.0-before-camera-scaler-freestyle-port-20261016 contains 443 hash-verified source/release files before this change, excluding user Data, Library, caches and logs. Frozen QA0.30 is unchanged; use its notes for the previous package evidence, not as proof of this launch fix. Main was not replaced.

Cameras, scaler and WNBA team availability in Freestyle remain the additional offline workstream. Candidate mappings and remaining checks are recorded in artifacts/camera-scaler-freestyle-v2-progress.json. No candidate address grants runtime authority. The full feature goal remains active.
