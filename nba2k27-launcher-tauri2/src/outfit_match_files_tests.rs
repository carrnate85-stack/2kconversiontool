use super::*;
use crate::blacktop_outfits::tests::{Fixture, game};
use crate::outfit_journal::tests as fixture;
const SESSION: &str = "abcdef0123456789abcdef0123456789abcdef0123456789";
fn setup() -> (PathBuf, Store, Fixture, Player, Composed) {
    let root = fixture::root();
    drop(loader_files::prepare_reports(&root).unwrap());
    let (mut memory, player, composed) = fixture::fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    fixture::committed(&mut s, &mut memory, &player, &composed);
    write_report(&root, 1, "ARMED", 0);
    (root, s, memory, player, composed)
}
fn text(state: &str, n: usize) -> String {
    format!(
        "SCHEMA=blacktop-saved-outfits-v1\r\nPID={}\r\nSTATE={state}\r\nREASON=Fixture report\r\nPLAYERS_REBUILT={n}\r\nGAME_CODE_HOOKS=0\r\nOBJECT_HOOKS=0\r\n",
        game().pid
    )
}
fn write_report(root: &Path, number: u32, state: &str, n: usize) {
    fs::write(
        path(&directory(root), &game(), number, "log").unwrap(),
        text(state, n),
    )
    .unwrap();
}
fn refresh_choice(s: &mut Store, memory: &Fixture, c: &Composed) {
    let p = outfits::capture(memory, &game(), s.pages())
        .unwrap()
        .players
        .remove(0);
    let r = s.reserve(&p, c).unwrap();
    assert_eq!(r.intent().kind, Kind::Existing);
    let r = s.existing(r).unwrap();
    let id = r.intent().id.clone();
    drop(r);
    s.verified(&id).unwrap();
}
fn native_read(path: &Path) -> Vec<u8> {
    // Match the unchanged DLL's GENERIC_READ + FILE_SHARE_READ contract.
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(path)
        .unwrap();
    read_at(&f, 0, f.metadata().unwrap().len() as usize).unwrap()
}
#[test]
fn consumed_choices_precede_native_files_and_handle_sharing_allows_native_read_delete() {
    let (root, mut s, memory, _, c) = setup();
    let choices = s.ledger().choices.clone();
    let frozen = s.reserve_match(&choices, || Ok(())).unwrap();
    assert!(s.ledger().fresh(&[choices[0].roster]).unwrap().is_empty());
    assert_eq!(s.shared.as_ref().unwrap().read().unwrap().1, s.state);
    assert!(!exists(&path(&directory(&root), &game(), 1, "plan").unwrap()).unwrap());
    assert!(s.reserve_match(&choices, || Ok(())).is_err());
    let mut p = s.publish_match(frozen, || Ok(())).unwrap();
    let plan = path(&directory(&root), &game(), 1, "plan").unwrap();
    let trigger = path(&directory(&root), &game(), 1, "trigger").unwrap();
    assert_eq!(
        native_read(&plan),
        policy::encode(&game(), &choices).unwrap()
    );
    assert_eq!(native_read(&trigger), policy::TRIGGER);
    assert!(fs::OpenOptions::new().write(true).open(&plan).is_err());
    fs::remove_file(&trigger).unwrap(); // Native worker can delete it immediately.
    write_report(&root, 1, "RUNNING", 0);
    assert_eq!(
        s.poll_match(&mut p, || Ok(())).unwrap().unwrap().phase,
        Phase::Running
    );
    assert!(s.next_match().is_err());
    write_report(&root, 1, "REBUILT", 1);
    assert_eq!(
        s.poll_match(&mut p, || Ok(())).unwrap().unwrap().phase,
        Phase::Rebuilt
    );
    s.poll_match(&mut p, || Ok(())).unwrap(); // No duplicate completion event.
    assert_eq!(s.next_match().unwrap(), 2);
    assert!(s.finish(|| Ok(())).is_err()); // Pending native read lease is retained.
    drop(p);
    refresh_choice(&mut s, &memory, &c);
    let choices = s.ledger().choices.clone();
    assert!(s.reserve_match(&choices, || Ok(())).is_err()); // No match2 ARMED report yet.
    write_report(&root, 2, "ARMED", 0);
    let f = s.reserve_match(&choices, || Ok(())).unwrap();
    let p = s.publish_match(f, || Ok(())).unwrap();
    assert!(
        path(&directory(&root), &game(), 2, "plan")
            .unwrap()
            .is_file()
    );
    drop(p);
    s.finish(|| Ok(())).unwrap();
    for number in [1, 2] {
        for ext in ["plan", "trigger"] {
            assert!(!exists(&path(&directory(&root), &game(), number, ext).unwrap()).unwrap());
        }
    }
    assert!(path(&directory(&root), &game(), 1, "log").unwrap().exists());
    assert!(!pending(&root).unwrap());
}
#[test]
fn existing_foreign_files_never_consume_a_choice_or_get_overwritten() {
    for ext in ["plan", "trigger"] {
        let (root, mut s, _, _, _) = setup();
        let file = path(&directory(&root), &game(), 1, ext).unwrap();
        fs::write(&file, b"foreign").unwrap();
        let choices = s.ledger().choices.clone();
        assert!(s.reserve_match(&choices, || Ok(())).is_err());
        assert!(s.ledger().consumed.is_empty());
        assert_eq!(fs::read(&file).unwrap(), b"foreign");
        s.finish(|| Ok(())).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"foreign");
    }
}
#[test]
fn a_foreign_trigger_race_holds_without_replay_or_adopting_it_during_recovery() {
    let (root, mut s, _, _, _) = setup();
    let choices = s.ledger().choices.clone();
    let frozen = s.reserve_match(&choices, || Ok(())).unwrap();
    let trigger = path(&directory(&root), &game(), 1, "trigger").unwrap();
    fs::write(&trigger, b"foreign").unwrap();
    assert!(s.publish_match(frozen, || Ok(())).is_err());
    assert!(s.halted().is_some());
    assert!(s.reserve_match(&choices, || Ok(())).is_err());
    assert!(s.finish(|| Ok(())).is_err());
    assert!(pending(&root).unwrap());
    assert_eq!(fs::read(&trigger).unwrap(), b"foreign");
    fs::remove_file(&trigger).unwrap(); // Explicit fixture-owner review, not recovery adoption.
    s.finish(|| Ok(())).unwrap();
    assert!(!pending(&root).unwrap());
}
#[test]
fn failed_plan_ownership_commit_never_releases_a_trigger_to_the_worker() {
    let (root, mut s, _, _, _) = setup();
    let frozen = s
        .reserve_match(&s.ledger().choices.clone(), || Ok(()))
        .unwrap();
    s.fail_append_after = Some(12);
    assert!(s.publish_match(frozen, || Ok(())).is_err());
    for ext in ["plan", "trigger"] {
        assert!(!exists(&path(&directory(&root), &game(), 1, ext).unwrap()).unwrap());
    }
    assert!(s.halted().is_some());
    drop(s);
    assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
    assert!(pending(&root).unwrap());
}
#[test]
fn reports_require_exact_pid_schema_count_and_zero_hooks_without_accepting_stale_files() {
    let good = text("REBUILT", 1);
    parse_report(good.as_bytes(), &game(), Some(1)).unwrap();
    for bad in [
        good.replace("PID=311", "PID=312"),
        good.replace("OBJECT_HOOKS=0", "OBJECT_HOOKS=1"),
        good.replace("STATE=REBUILT", "STATE=UNKNOWN"),
        format!("{good}PID=311\n"),
        good.replace("PLAYERS_REBUILT=1", "PLAYERS_REBUILT=01"),
        good.replace("PLAYERS_REBUILT=1", "PLAYERS_REBUILT=2"),
        text("ARMED", 1),
        good.replace("SCHEMA=blacktop-saved-outfits-v1", "SCHEMA=other"),
    ] {
        assert!(parse_report(bad.as_bytes(), &game(), Some(1)).is_err());
    }
    let (root, mut s, _, _, _) = setup();
    let mut future = game();
    future.started = "199000000000000000".into();
    assert!(report(&directory(&root), &future, 1, None).is_err());
    fs::write(path(&directory(&root), &game(), 1, "log").unwrap(), []).unwrap();
    assert!(s.worker_report(1).unwrap().is_none());
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn failed_inconclusive_incomplete_or_missing_trigger_results_never_rearm_or_retry() {
    for state in ["FAILED", "INCONCLUSIVE", "REBUILT"] {
        let (root, mut s, _, _, _) = setup();
        let f = s
            .reserve_match(&s.ledger().choices.clone(), || Ok(()))
            .unwrap();
        let mut p = s.publish_match(f, || Ok(())).unwrap();
        write_report(&root, 1, state, 0);
        assert!(s.poll_match(&mut p, || Ok(())).is_err());
        assert!(s.halted().is_some());
        assert!(s.next_match().is_err());
        assert_eq!(s.ledger().consumed.len(), 1);
        drop(p);
        s.finish(|| Ok(())).unwrap();
    }
    let (root, mut s, _, _, _) = setup();
    let f = s
        .reserve_match(&s.ledger().choices.clone(), || Ok(()))
        .unwrap();
    let mut p = s.publish_match(f, || Ok(())).unwrap();
    write_report(&root, 1, "REBUILT", 1);
    assert!(s.poll_match(&mut p, || Ok(())).is_err()); // A success report without trigger consumption is not accepted.
    drop(p);
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn ended_recovery_removes_only_exact_owned_files_and_keeps_native_diagnostics() {
    let (root, mut s, _, _, _) = setup();
    let f = s
        .reserve_match(&s.ledger().choices.clone(), || Ok(()))
        .unwrap();
    let p = s.publish_match(f, || Ok(())).unwrap();
    drop(p);
    drop(s);
    assert!(resolve_ended(&root, SESSION, &game(), || anyhow::bail!("game alive")).is_err());
    let plan = path(&directory(&root), &game(), 1, "plan").unwrap();
    let bytes = fs::read(&plan).unwrap();
    let moved = directory(&root).join("foreign-moved-plan");
    fs::rename(&plan, &moved).unwrap();
    fs::write(&plan, &bytes).unwrap();
    assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
    assert_eq!(fs::read(&plan).unwrap(), bytes);
    fs::remove_file(&plan).unwrap();
    fs::rename(&moved, &plan).unwrap();
    resolve_ended(&root, SESSION, &game(), || Ok(())).unwrap();
    assert!(!pending(&root).unwrap());
    assert!(!plan.exists());
    assert!(path(&directory(&root), &game(), 1, "log").unwrap().exists());
}
#[test]
fn changed_report_directory_or_cross_journal_capability_never_publishes_files() {
    let (root, mut s, _, _, _) = setup();
    let frozen = s
        .reserve_match(&s.ledger().choices.clone(), || Ok(()))
        .unwrap();
    let dir = directory(&root);
    let old = root.join("Logs/old-game-session");
    fs::rename(&dir, &old).unwrap();
    fs::create_dir(&dir).unwrap();
    assert!(s.publish_match(frozen, || Ok(())).is_err());
    assert!(s.finish(|| Ok(())).is_err());
    fs::remove_dir(&dir).unwrap();
    fs::rename(&old, &dir).unwrap();
    s.finish(|| Ok(())).unwrap();
    let (_, mut s, _, _, _) = setup();
    let f = s
        .reserve_match(&s.ledger().choices.clone(), || Ok(()))
        .unwrap();
    let (_, mut other, _, _, _) = setup();
    assert!(other.publish_match(f, || Ok(())).is_err());
    s.finish(|| Ok(())).unwrap();
    other.finish(|| Ok(())).unwrap();
}
#[test]
fn trigger_ownership_or_submission_commit_failures_never_release_a_readable_trigger() {
    for skip in [1, 2] {
        let (root, mut s, _, _, _) = setup();
        let f = s
            .reserve_match(&s.ledger().choices.clone(), || Ok(()))
            .unwrap();
        s.fail_append_after = Some(0);
        s.fail_append_skip = skip;
        assert!(s.publish_match(f, || Ok(())).is_err());
        assert!(!exists(&path(&directory(&root), &game(), 1, "trigger").unwrap()).unwrap());
        assert!(s.halted().is_some());
        assert_eq!(s.ledger().consumed.len(), 1);
        if s.finish(|| Ok(())).is_err() {
            // A complete first frame of the pair is a valid disk prefix, but
            // in-memory state deliberately stays unchanged after uncertainty.
            drop(s);
            resolve_ended(&root, SESSION, &game(), || Ok(())).unwrap();
        }
        assert!(!pending(&root).unwrap());
    }
}
#[test]
fn final_liveness_gate_runs_while_native_open_and_delete_are_still_blocked() {
    use std::cell::Cell;
    let (root, mut s, _, _, _) = setup();
    let f = s
        .reserve_match(&s.ledger().choices.clone(), || Ok(()))
        .unwrap();
    let trigger = path(&directory(&root), &game(), 1, "trigger").unwrap();
    let calls = Cell::new(0);
    assert!(
        s.publish_match(f, || {
            let n = calls.get() + 1;
            calls.set(n);
            if n == 3 {
                assert!(exists(&trigger)?);
                ensure!(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(&trigger)
                        .is_err(),
                    "Native reader opened before submission commit"
                );
                ensure!(
                    fs::remove_file(&trigger).is_err(),
                    "Foreign delete was allowed before submission commit"
                );
                anyhow::bail!("game changed before native handoff");
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls.get(), 3);
    assert!(!exists(&trigger).unwrap());
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn a_shorter_in_place_rewrite_read_mid_truncate_waits_instead_of_halting() {
    // The worker seeks to 0, writes, then truncates: ARMED (longer reason)
    // to RUNNING briefly reads as RUNNING plus the old report's tail.
    let native = |state: &str, reason: &str| {
        text(state, 0)
            .replace("\r\n", "\n")
            .replace("Fixture report", reason)
    };
    let armed = native("ARMED", "Waiting for the match to load players");
    let running = native("RUNNING", "Rebuilding");
    assert!(armed.len() > running.len());
    let mut torn = running.clone().into_bytes();
    torn.extend_from_slice(&armed.as_bytes()[running.len()..]);
    assert!(parse_report(&torn, &game(), Some(1)).is_err());
    assert!(torn_rewrite(&torn, &game(), Some(1)));
    // A long tail (shorter than the new report) is still a tear.
    let mut long = running.clone().into_bytes();
    long.extend_from_slice(b"=0\nGAME_CODE_HOOKS=0\nOBJECT_HOOKS=0\n");
    assert!(torn_rewrite(&long, &game(), Some(1)));
    // Anything else malformed after a valid report still fails.
    for bad in [
        format!("{running}PID=311\n"),
        format!("{running}STATE=ARMED\n"),
        format!("{running}{running}"),
    ] {
        assert!(!torn_rewrite(bad.as_bytes(), &game(), Some(1)));
    }
    let (root, mut s, _, _, _) = setup();
    fs::write(path(&directory(&root), &game(), 1, "log").unwrap(), &torn).unwrap();
    assert!(s.worker_report(1).unwrap().is_none());
    assert!(s.halted().is_none());
    s.finish(|| Ok(())).unwrap();
}

#[test]
fn paired_submission_one_flush_pins_both_frames_before_final_check_and_unlock() {
    SUBMISSION_TRACE.with(|trace| trace.borrow_mut().clear());
    let (root, mut store, _, _, _) = setup();
    let choices = store.ledger().choices.clone();
    let frozen = store.reserve_match(&choices, || Ok(())).unwrap();
    let trigger = path(&directory(&root), &game(), 1, "trigger").unwrap();
    let calls = std::cell::Cell::new(0);
    let pending = store
        .publish_match(frozen, || {
            calls.set(calls.get() + 1);
            if calls.get() == 3 {
                assert!(
                    fs::OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .open(&trigger)
                        .is_err()
                );
                assert!(fs::remove_file(&trigger).is_err());
                assert_eq!(
                    SUBMISSION_TRACE.with(|trace| trace.borrow().clone()),
                    vec!["pair-write", "pair-sync", "pair-readback", "pair-pin"]
                );
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(
        SUBMISSION_TRACE.with(|trace| trace.borrow().clone()),
        vec![
            "pair-write",
            "pair-sync",
            "pair-readback",
            "pair-pin",
            "final-check",
            "trigger-unlock"
        ]
    );
    assert_eq!(
        store.shared.as_ref().unwrap().read().unwrap().1,
        store.state
    );
    assert!(store.state.matches.records.last().unwrap().submitted);
    assert_eq!(native_read(&trigger), policy::TRIGGER);
    drop(pending);
    store.finish(|| Ok(())).unwrap();
}

#[test]
fn every_two_frame_physical_prefix_is_old_parser_compatible_and_never_replayed() {
    let (root, mut store, _, _, _) = setup();
    let choices = store.ledger().choices.clone();
    let frozen = store.reserve_match(&choices, || Ok(())).unwrap();
    let dir = directory(&root);
    let bytes = policy::encode(&game(), &choices).unwrap();
    let plan = create(&path(&dir, &game(), 1, "plan").unwrap(), &bytes).unwrap();
    let plan_id = file_id(&plan).unwrap();
    drop(plan);
    drop(
        store
            .append(super::super::Event::Match(Event::Owned {
                number: 1,
                trigger: false,
                file: plan_id,
            }))
            .unwrap(),
    );
    drop(frozen);
    let trigger = store
        .create_locked_trigger(
            &path(&dir, &game(), 1, "trigger").unwrap(),
            1,
            vec![Directories::pin(&dir).unwrap()],
        )
        .unwrap();
    let owned = super::super::Event::Match(Event::Owned {
        number: 1,
        trigger: true,
        file: trigger.id.as_ref().unwrap().clone(),
    });
    let submitted = super::super::Event::Match(Event::Submitted { number: 1 });
    let first = super::super::frame(&owned).unwrap();
    let pair = [
        first.as_slice(),
        super::super::frame(&submitted).unwrap().as_slice(),
    ]
    .concat();
    let shared = store.shared.as_ref().unwrap().clone();
    let mut file = shared.file.lock().unwrap();
    let base = read_at(&file, 0, store.end as usize).unwrap();
    for cut in 0..=pair.len() {
        file.set_len(base.len() as u64).unwrap();
        file.seek(SeekFrom::Start(base.len() as u64)).unwrap();
        file.write_all(&pair[..cut]).unwrap();
        file.sync_all().unwrap();
        let physical = read_at(&file, 0, base.len() + cut).unwrap();
        let parsed = super::super::parse(&physical);
        if cut == 0 || cut == first.len() || cut == pair.len() {
            let (_, state) = parsed.unwrap();
            assert_eq!(state.ledger.consumed.len(), choices.len());
            assert_eq!(
                state.matches.records.last().unwrap().submitted,
                cut == pair.len()
            );
            if cut == pair.len() {
                let mut old = base.clone();
                old.extend(super::super::frame(&owned).unwrap());
                old.extend(super::super::frame(&submitted).unwrap());
                assert_eq!(old, physical);
            }
        } else {
            assert!(parsed.is_err(), "torn prefix {cut}");
        }
        assert!(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(path(&dir, &game(), 1, "trigger").unwrap())
                .is_err()
        );
    }
    file.set_len(base.len() as u64).unwrap();
    file.sync_all().unwrap();
    drop(file);
    drop(shared);
    assert!(store.reserve_match(&choices, || Ok(())).is_err());
    store.abort_locked_trigger(trigger).unwrap();
    store.finish(|| Ok(())).unwrap();
}

#[test]
fn submission_pair_stage_failures_hold_without_unlock_or_replay() {
    // Before-write identity, sync uncertainty, readback uncertainty, late file ID,
    // trigger creation sync, and final liveness/plan verification are independent.
    for stage in [1u8, 2, 4, 16, 32] {
        let (root, mut store, _, _, _) = setup();
        let choices = store.ledger().choices.clone();
        let frozen = store.reserve_match(&choices, || Ok(())).unwrap();
        SUBMISSION_FAULT.with(|fault| fault.set(stage));
        assert!(
            store.publish_match(frozen, || Ok(())).is_err(),
            "stage {stage}"
        );
        assert!(store.halted().is_some());
        assert_eq!(store.ledger().consumed.len(), choices.len());
        assert!(store.reserve_match(&choices, || Ok(())).is_err());
        assert!(
            !path(&directory(&root), &game(), 1, "trigger")
                .unwrap()
                .exists()
        );
        if store.finish(|| Ok(())).is_err() {
            drop(store);
            resolve_ended(&root, SESSION, &game(), || Ok(())).unwrap();
        }
    }
}

#[test]
fn unlink_failure_retains_exclusive_lock_until_positive_closed_proof() {
    let (root, mut store, _, _, _) = setup();
    let frozen = store
        .reserve_match(&store.ledger().choices.clone(), || Ok(()))
        .unwrap();
    SUBMISSION_FAULT.with(|fault| fault.set(2 | 8));
    assert!(store.publish_match(frozen, || Ok(())).is_err());
    assert!(store.held_trigger.is_some());
    let target = path(&directory(&root), &game(), 1, "trigger").unwrap();
    assert!(
        fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .is_err()
    );
    assert!(Store::create(&root, SESSION, &game()).is_err());
    assert!(store.finish(|| anyhow::bail!("game alive")).is_err());
    assert!(store.held_trigger.is_some());
    assert!(
        fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .is_err()
    );
    // Fake fixture game is positively closed; unknown disk state still requires
    // ended-only recovery, but closing its quarantine is now safe.
    let _ = store.finish(|| Ok(()));
    assert!(store.held_trigger.is_none());
    drop(store);
    resolve_ended(&root, SESSION, &game(), || Ok(())).unwrap();
}

#[test]
fn early_store_drop_and_unknown_creation_id_cannot_unlock_uncertain_trigger() {
    use std::os::windows::io::AsRawHandle;
    for stage in [2u8 | 8, 16 | 8, 64] {
        let (root, mut store, _, _, _) = setup();
        let frozen = store
            .reserve_match(&store.ledger().choices.clone(), || Ok(()))
            .unwrap();
        SUBMISSION_FAULT.with(|fault| fault.set(stage));
        assert!(store.publish_match(frozen, || Ok(())).is_err());
        let raw = store
            .held_trigger
            .as_ref()
            .unwrap()
            .file()
            .unwrap()
            .as_raw_handle();
        let target = path(&directory(&root), &game(), 1, "trigger").unwrap();
        drop(store);
        assert!(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&target)
                .is_err()
        );
        assert!(fs::remove_file(&target).is_err());
        assert!(Store::create(&root, SESSION, &game()).is_err());
        // Explicit test-owner game-closed cleanup of this synthetic file only.
        // Production never recovers a leaked handle or performs this close.
        unsafe {
            windows::Win32::Foundation::CloseHandle(HANDLE(raw)).unwrap();
        }
        if stage != (2 | 8) {
            // Creation failed before a durable Owned trigger frame. Recovery
            // must refuse adoption; only this fixture owner removes its file.
            assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
            assert!(target.exists());
            fs::remove_file(&target).unwrap();
        }
        resolve_ended(&root, SESSION, &game(), || Ok(())).unwrap();
    }
}

#[test]
fn mismatching_trigger_identity_refuses_unlink_and_quarantines_exact_original_handle() {
    let (root, mut store, _, _, _) = setup();
    let target = path(&directory(&root), &game(), 1, "trigger").unwrap();
    let mut trigger = store
        .create_locked_trigger(
            &target,
            1,
            vec![Directories::pin(&directory(&root)).unwrap()],
        )
        .unwrap();
    let original = trigger.id.as_ref().unwrap().clone();
    trigger.id.as_mut().unwrap().index ^= 1;
    assert!(store.abort_locked_trigger(trigger).is_err());
    assert!(target.exists());
    assert_eq!(
        file_id(store.held_trigger.as_ref().unwrap().file().unwrap()).unwrap(),
        original
    );
    assert!(
        fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .is_err()
    );
    assert!(
        store
            .finish(|| anyhow::bail!("synthetic game alive"))
            .is_err()
    );
    assert!(store.held_trigger.is_some());
    // Positive closed proof permits handle release, but not path adoption.
    assert!(store.finish(|| Ok(())).is_ok());
    assert!(target.exists());
    fs::remove_file(&target).unwrap(); // Fixture-owner cleanup, never recovery.
}

#[test]
fn final_handoff_failure_after_durable_pair_preserves_no_replay_and_conditional_cleanup() {
    let (root, mut store, _, _, _) = setup();
    let choices = store.ledger().choices.clone();
    let frozen = store.reserve_match(&choices, || Ok(())).unwrap();
    let target = path(&directory(&root), &game(), 1, "trigger").unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(
        store
            .publish_match(frozen, || {
                calls.set(calls.get() + 1);
                if calls.get() == 3 {
                    assert!(
                        fs::OpenOptions::new()
                            .read(true)
                            .share_mode(1)
                            .open(&target)
                            .is_err()
                    );
                    anyhow::bail!("fresh actor/parent/context changed after durable commit");
                }
                Ok(())
            })
            .is_err()
    );
    assert_eq!(calls.get(), 3);
    assert!(store.state.matches.records.last().unwrap().submitted);
    assert_eq!(
        store.shared.as_ref().unwrap().read().unwrap().1,
        store.state
    );
    assert!(store.halted().is_some());
    assert!(!target.exists());
    assert!(
        store
            .ledger()
            .fresh(&[choices[0].roster])
            .unwrap()
            .is_empty()
    );
    assert!(store.reserve_match(&choices, || Ok(())).is_err());
    store.finish(|| Ok(())).unwrap();
}
