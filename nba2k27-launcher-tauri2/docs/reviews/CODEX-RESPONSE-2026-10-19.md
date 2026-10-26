# QA085 independent response to Claude review

Reviewed commit `859e27cdb6a7a22127eaa662ebea008d54ffeb9a`, whose review examined QA084 `0fad8711248b8983af3ce0da8aa72a8a4b5e210d`. Findings were checked against actual routes before editing. These are development changes only. No installed build, game launch, game memory operation, activation or promotion is part of this work.

## Findings

| ID | Verdict | Result |
|---|---|---|
| H1 | Agree | Deduplicate referenced audio members and enforce whole-package output budgets before decoding/copying; preflight all entries. |
| M1 | Agree | Restore independent player links/stores despite another failure, aggregate held errors, and recover the physical selected root from exact session/directory-bound receipts after profile cleanup. Last copies and conflicting receipts remain held. |
| M2 | Agree | Cancellation/forward timeout cannot disable reversal. Separate nonrenewable eight-second recovery access retains process identity, allocation, ownership and alias checks; the access cannot be reused for forward work. |
| M3 | Agree | Durable original copy precedes one atomic handle-based replacement. The old destination is not renamed away first. Error cleanup unlinks only exact owned handles; no-replace reverse preserves original backups when restoration is uncertain. Per-file atomicity is not an all-files power-loss transaction or cross-process FileID compare-and-swap. |
| M4 | Partly agree | Fit, selector and WNBA Oodle paths now authenticate exact compiled SHA256 allowlists before loading and retain deny-write file/ancestor pins. Cover decoding already authenticated its core runtime; it was not an unverified path. The texture pin was measured from the installed runtime, without loading the game. |
| M5 | Agree, mitigated | External update intake and runner application fail closed because no trusted publisher verification key is provisioned. Explicit independently revalidated PreviousApp rollback remains available. Version downgrade policy is implemented. Cryptographic signed-release infrastructure is still unprovisioned; an untrusted package/local key is not a trust anchor. |
| M6 | Agree | Camera/selector known in-process stage failures remove only retained CREATE_NEW handles. Unknown crash stages are not adopted by prefix or digest. |
| M7 | Agree | Update journals bind exact original manifests and directory FileIDs. Failed undo retains the journal until originals are independently verified. Legacy journals lacking witnesses remain held for manual review. Failed recovery cannot relaunch an unverified App. |
| M8 | Agree | Serialized settings saves recover persisted state on failure while preserving later queued patches and backend event merges. |
| M9 | Agree | Successful install remains successful if the refresh scan fails; success also reports preserved-backup cleanup warnings. |
| M10 | Agree | Separate bootstrap readiness from the authenticated freeze acknowledgement/effect deadline; reversal retains its own bounded lease. |
| M11 | Agree | Real typed WriteGuard retains local and named Windows mutex ownership across parent/child work, with bounded waits and an abandoned/poisoned gate hold. No fake loose MutexGuard conversion. |
| M12 | Partly agree | Snapshot enumeration errors were already transient. Only narrowly typed provider-status file I/O is now retryable; corrupted data, changed ownership and journal/persistence failures remain terminal. |
| M13 | Agree | Scoped Git attributes preserve raw bytes, including existing mixed endings, rather than normalizing hash-pinned inputs. Isolated autocrlf=true checkout proofs leave the real and publisher indexes unchanged. |
| M14 | Partly agree | Host-dependent enablement tests now inject an explicit availability seam. The whole-ntdll safety profile remains intentional; another Windows build is not enabled without a proven profile. Diagnostics identify the unsupported profile. |
| L1 | Agree | Dispatcher checks explicit per-window command authority before fallthrough. Overlays cannot invoke installers, launcher updates, arbitrary settings paths or unrelated writes. |
| L2 | Agree | Jukebox uses capped workers; both servers bound idle header acquisition. Existing authenticated framing/body limits remain. This mitigates availability loss, not unlimited hostile connection floods. |
| L3 | Partly disagree; idle policy deferred | The actual embedded bridge already moves the token to sessionStorage and strips the fragment using history.replaceState. Session-lifetime credentials and privileged extensions remain separate concerns; idle expiry/rotation needs an explicit active-tab policy. |
| L4 | Agree, deferred | Synchronous WIC decode lacks process containment. A worker contract and acceptance proposal are recorded; no thread timeout is falsely presented as cancellation of WIC. |
| L5 | Agree | Validate preview PNG dimensions during package preflight, before staging audio. |
| L6 | Agree | Local media failed publication cleans up only its retained owned temporary handle. |
| L7 | Agree | ZIP member reads enforce declared bounds with take(MAX+1) and exact actual-length checks. |
| L8 | Partly addressed | Remove proven duplicate decode buffers and bind FLAC worker/parent output allocation to validated limits. Unknown-total STREAMINFO remains supported with the manifest limit. No unmeasured universal peak-memory claim. |
| L9 | Agree with qualification | Preserve an unpublished foreign target while verifying/removing exact owned stages. An exact matching target may be a successful rename whose receipt update lagged, so that cleanup remains supported. |
| L10 | Disagree with suggested sweep; lifecycle gap deferred | A filename prefix alone does not prove ownership after a crash. Unknown files remain held; known in-process stage cleanup is fixed in M6/L6. Durable crash-stage ownership receipts are the safe follow-up. |
| L11 | Agree | Reject superscript COM/LPT device names plus CONIN$/CONOUT$. |
| L12 | Partly disagree | The file is absent from the launcher module tree. Separate explicit one-time fixed-plan publication tools use it intentionally; their hardcoded targets are authority restrictions, not reachable general launcher routes. No removal of recovery tooling. |
| L13 | Agree on malformed-display failure; safe fix deferred | A lenient decoder alone would re-enable writes/copy via downstream capability construction. The report specifies per-row markers and dependent write/copy/rotation guards. WPF also requires a terminator inside the 20-unit staff/rotation name field; that writer policy is preserved. |
| L14 | Agree | Legacy saves also check all pending version-two writer journals. |
| L15 | Agree for observed literals | Corrected malformed native/browser display literals. Current published README was independently checked and has no matching malformed literals. |
| L16 | Agree | Evict the least recently used roster tab, and update its use on valid status/detail/save access, rather than ordering by UUID. |
| L17 | Agree | Separate transient polling errors from user-action failures and clear polling errors after success. |
| L18 | Needs evidence | Multiple independent polls exist, but no measured shared-poller benefit justifies changing current F8 cadence during safety remediation. |
| L19 | Agree | Ignore stale outfit-pick replies after Remove or a newer request. |
| L20 | Agree | Surface wardrobe load errors. |
| L21 | Agree | Retain update-card success/state when a later refresh fails. |
| L22 | Agree | Canceled 2KDB operations cannot leave stale opening state. |
| L23 | Agree | Blank numeric inputs do not silently become zero; year validation remains meaningful. |
| L24 | Agree | Archive only current privately proven, exactly verified completed intents. Final testing exposed and fixed an unterminated native rename buffer; 32 filename alignment cases cover it. Restart receipts cannot authorize replay, writes or adoption of unknown records. |
| L25 | Partly addressed | Bound retries terminating the bootstrap-authenticated retained handle. If the OS refuses every attempt, preserve an explicit blocking safety hold rather than releasing a possibly frozen/partial graph. Universal release under kernel failure is not claimed. |
| L26 | Agree | Retry only verified thread-exit/recycled-ID races before freeze, with bounded scans; do not claim post-effect independent verification when no effect occurred. |

## Validation and qualifications

Final exact Rust partition coverage: **1,529 passed, 0 failed, 54 explicitly ignored**, covering all 1,583 enumerated tests on one pinned EXE/PDB without a debugger. Full frontend: **232 passed**; TypeScript and Vite build passed; **15** browser/source Node regression scripts passed. Separate production-module fixtures passed 15 unit cases, 14 lifecycle scenarios, actual cross-process gate checks, and 52 + 52 large-intent proof checks. Two additional read-only installed-disk audits passed, including approved core/texture loading and mannequin export; neither opened or launched the game process.

QA085 is staged only at `Dev/Pending/QA085/launcher.exe`, SHA256 `FF42FE0363AA996866A8B820B133297C7D030A142AA67F48CBF9B4DCBF07CBB0` (59,244,032 bytes). The starter hash check passed before its idle guard correctly refused because the existing launcher was running; no stop or activation was performed. Initial ordinary test failures and exact failing images/inputs are preserved separately; all final partitions pass after the documented fixes. The original parent Git HEAD, index and unrelated 137-file staged change set remain untouched.

The QA073 natural Rust test access violation and exact EXE/PDB evidence remain unresolved. QA078 Clang fixture failure remains unexplained. QA080 visible scaler acceptance remains pending. Passing offline checks do not establish live gameplay acceptance or full feature parity.

Detailed proposals, independent verdicts, source hashes and validation receipts are under `artifacts/qa085-claude-review-20261019`, `qa085-frontend-review`, `qa085-session-review`, `qa085-media-review`, `qa085-memory-review` and `qa085-git-line-endings`.
