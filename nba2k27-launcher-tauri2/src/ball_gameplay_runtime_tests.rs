use super::*;
use crate::ball_gameplay_v2::tests::{chosen, fixture, manifest};
#[test]
fn prepared_selection_backout_restores_before_any_match() {
    assert!(restore_boundary("main-menu", false));
    assert!(restore_boundary("quick-play", false));
    assert!(!restore_boundary("selection", false));
    assert!(restore_boundary("selection", true));
    assert!(!restore_boundary("match", true));
}
fn binding(root: &Path) -> Binding {
    Binding::from_launched(
        Identity {
            pid: 11,
            started: "100".into(),
            path: r"C:\Fixture\NBA2K27.exe".into(),
        },
        Identity {
            pid: 22,
            started: "200".into(),
            path: r"C:\Portable\2k Launcher.exe".into(),
        },
        r"C:\Fixture".into(),
        root.into(),
        "a".repeat(48),
    )
    .unwrap()
}
#[test]
fn ordinary_native_reset_rearms_but_uncertain_publication_stays_held() {
    let root = FixtureRoot::new();
    for uncertain in [false, true] {
        let mut runner = Runner::new(binding(&root.0), &chosen()).unwrap();
        runner.owned = Some(plan());
        runner.held = uncertain;
        runner.seen_match = true;
        runner.restore_requested = true;
        runner.gameplay_model_confirmed = true;
        assert!(runner.owned.as_ref().unwrap().restored(&fixture()).is_ok());
        runner.acknowledge_restored();
        assert_eq!(runner.held, uncertain);
        assert!(runner.owned.is_none());
        assert!(
            !runner.seen_match && !runner.restore_requested && !runner.gameplay_model_confirmed
        );
        assert_eq!(
            runner.phase,
            if uncertain {
                "restored-held"
            } else {
                "restored"
            }
        );
        if !uncertain {
            runner.choose(&chosen()).unwrap();
        }
    }
}
#[test]
fn leaving_confirmed_match_clears_current_confirmation_while_loaded_restore_waits() {
    let root = FixtureRoot::new();
    let mut runner = Runner::new(binding(&root.0), &chosen()).unwrap();
    let mut f = fixture();
    let p = plan();
    f.patch(p.address(), &p.after_bytes().to_vec());
    let setting = p.after_bytes().to_vec();
    runner.owned = Some(p);
    runner.seen_match = true;
    runner.gameplay_model_confirmed = true;
    runner.phase = "gameplay-model-confirmed";
    // Selected slot is still referenced after reaching Quick Play. Its native
    // resource and the exact-owned setting must not be changed at this point.
    f.patch(
        f.base() + crate::ball_v2::REFERENCES + 0x18 + 8,
        &1u32.to_le_bytes(),
    );
    runner.restore_requested |= restore_boundary("quick-play", runner.seen_match);
    observe_status_boundary(
        "quick-play",
        runner.restore_requested,
        &mut runner.gameplay_model_confirmed,
        &mut runner.phase,
        &mut runner.restore_samples,
    );
    assert!(!runner.gameplay_model_confirmed);
    assert_eq!(runner.phase, "waiting-for-release");
    assert!(runner.restore_requested && runner.owned.is_some());
    let p = runner.owned.as_ref().unwrap();
    assert!(
        p.restore_stable(&f, &mut runner.restore_samples, 0)
            .is_err()
    );
    assert_eq!(f.read(p.address(), setting.len()).unwrap(), setting);
    f.patch(
        f.base() + crate::ball_v2::REFERENCES + 0x18 + 8,
        &0u32.to_le_bytes(),
    );
    for t in [100, 200, 300] {
        assert!(
            p.restore_stable(&f, &mut runner.restore_samples, t)
                .unwrap()
                .is_none()
        );
    }
    let permit = p
        .restore_stable(&f, &mut runner.restore_samples, 400)
        .unwrap()
        .unwrap();
    f.patch(permit.address(), permit.after());
    permit.verify_after(&f).unwrap();
    runner.acknowledge_restored();
    assert_eq!(runner.phase, "restored");
    assert!(!runner.held && !runner.gameplay_model_confirmed && runner.owned.is_none());
}
#[test]
fn unknown_transition_discards_previous_restore_stability_and_confirmation() {
    let root = FixtureRoot::new();
    let mut runner = Runner::new(binding(&root.0), &chosen()).unwrap();
    let mut f = fixture();
    let p = plan();
    f.patch(p.address(), &p.after_bytes().to_vec());
    for t in [0, 100, 200] {
        assert!(
            p.restore_stable(&f, &mut runner.restore_samples, t)
                .unwrap()
                .is_none()
        );
    }
    runner.gameplay_model_confirmed = true;
    runner.phase = "gameplay-model-confirmed";
    observe_status_boundary(
        "transition-or-other-mode",
        true,
        &mut runner.gameplay_model_confirmed,
        &mut runner.phase,
        &mut runner.restore_samples,
    );
    assert!(!runner.gameplay_model_confirmed);
    assert_eq!(runner.phase, "waiting-for-release");
    for t in [300, 400, 500] {
        assert!(
            p.restore_stable(&f, &mut runner.restore_samples, t)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        p.restore_stable(&f, &mut runner.restore_samples, 600)
            .unwrap()
            .is_some()
    );
}
#[test]
fn owned_prepared_context_failure_recovers_with_fresh_read_only_selection_check() {
    let root = FixtureRoot::new();
    let mut runner = Runner::new(binding(&root.0), &chosen()).unwrap();
    let mut f = fixture();
    let p = plan();
    // Simulate the one previously completed publication, not another write on
    // recovery. The status path receives only immutable read access afterward.
    f.patch(p.address(), &p.after_bytes().to_vec());
    runner.owned = Some(p);
    runner.phase = "prepared";
    let before = policy::capture(&f, &chosen()).unwrap();
    context_unavailable_status(
        &anyhow::anyhow!("transient fixture context read failure"),
        &mut runner.gameplay_model_confirmed,
        &mut runner.phase,
        &mut runner.restore_samples,
        &mut runner.message,
    );
    assert_eq!(runner.phase, "waiting-for-context");
    assert!(!runner.gameplay_model_confirmed);
    let p = runner.owned.as_ref().unwrap();
    prepared_selection_status(
        p.prepared(&f),
        &mut runner.gameplay_model_confirmed,
        &mut runner.phase,
        &mut runner.message,
    );
    assert_eq!(runner.phase, "prepared");
    assert!(!runner.gameplay_model_confirmed);
    assert!(!runner.restore_requested && !runner.held && !runner.seen_match);
    assert!(runner.owned.is_some());
    assert!(runner.native.is_none() && runner.journal.is_none());
    assert_eq!(policy::capture(&f, &chosen()).unwrap(), before);
    assert!(!pending(&root.0).unwrap());
    assert_eq!(runner.status()["visualAppearanceProven"], false);
}
#[test]
fn prepared_recovery_rejects_changed_field_and_loaded_resource_without_stale_confirmation() {
    for changed_setting in [true, false] {
        let root = FixtureRoot::new();
        let mut runner = Runner::new(binding(&root.0), &chosen()).unwrap();
        let mut f = fixture();
        let p = plan();
        f.patch(p.address(), &p.after_bytes().to_vec());
        if changed_setting {
            f.patch(p.address(), &p.before_bytes().to_vec());
        } else {
            f.patch(
                f.base() + crate::ball_v2::REFERENCES + 0x18 + 8,
                &1u32.to_le_bytes(),
            );
        }
        let before = policy::capture(&f, &chosen()).unwrap();
        runner.owned = Some(p);
        runner.gameplay_model_confirmed = true;
        runner.phase = "gameplay-model-confirmed";
        prepared_selection_status(
            runner.owned.as_ref().unwrap().prepared(&f),
            &mut runner.gameplay_model_confirmed,
            &mut runner.phase,
            &mut runner.message,
        );
        assert_eq!(runner.phase, "prepared-unverified");
        assert!(!runner.gameplay_model_confirmed);
        assert!(runner.owned.is_some());
        assert!(runner.native.is_none() && runner.journal.is_none());
        assert_eq!(policy::capture(&f, &chosen()).unwrap(), before);
        assert!(!pending(&root.0).unwrap());
    }
}
struct FixtureRoot(PathBuf);
impl FixtureRoot {
    fn new() -> Self {
        let parent =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/gameplay-ball-fixtures");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(crate::backend::unique_id());
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Data")).unwrap();
        Self(root)
    }
}
impl Drop for FixtureRoot {
    fn drop(&mut self) {
        for name in [MARKER, "saved-owned-receipt"] {
            let _ = fs::remove_file(self.0.join("Data").join(name));
        }
        let _ = fs::remove_dir(self.0.join("Data"));
        let _ = fs::remove_dir(&self.0);
    }
}
fn plan() -> policy::Plan {
    let f = fixture();
    let mut s = policy::Samples::new();
    for t in [0, 100, 200] {
        assert!(
            s.push(policy::capture(&f, &chosen()).unwrap(), t)
                .unwrap()
                .is_none()
        );
    }
    s.push(policy::capture(&f, &chosen()).unwrap(), 300)
        .unwrap()
        .unwrap()
}
#[test]
fn durable_intent_is_exact_direction_plan_and_full_readback() {
    let root = FixtureRoot::new();
    let b = binding(&root.0);
    let f = fixture();
    let p = plan();
    let permit = p.forward(&f, &manifest()).unwrap();
    let mut j = Journal::create(&b, &chosen()).unwrap();
    assert!(j.verify(&permit).is_err());
    j.intent(&permit).unwrap();
    j.verify(&permit).unwrap();
    j.file.set_len(j.size - 1).unwrap();
    assert!(j.verify(&permit).is_err());
    drop(j);
    assert!(remove_ended(&root.0, &b.game, &b.session, &[]).is_err());
    assert!(pending(&root.0).unwrap());
}
#[test]
fn existing_receipt_is_never_overwritten_and_running_game_prevents_recovery() {
    let root = FixtureRoot::new();
    let b = binding(&root.0);
    let j = Journal::create(&b, &chosen()).unwrap();
    assert!(Journal::create(&b, &chosen()).is_err());
    drop(j);
    let rows = [session_safety::Process {
        name: "NBA2K27.exe".into(),
        pid: 55,
        parent: 0,
        identity: None,
    }];
    assert!(remove_ended(&root.0, &b.game, &b.session, &rows).is_err());
    assert!(pending(&root.0).unwrap());
    remove_ended(&root.0, &b.game, &b.session, &[]).unwrap();
    assert!(!pending(&root.0).unwrap());
}
#[test]
fn copied_foreign_file_id_and_wrong_session_are_preserved() {
    let root = FixtureRoot::new();
    let b = binding(&root.0);
    let j = Journal::create(&b, &chosen()).unwrap();
    drop(j);
    assert!(remove_ended(&root.0, &b.game, &"b".repeat(48), &[]).is_err());
    let marker = root.0.join("Data").join(MARKER);
    let saved = root.0.join("Data/saved-owned-receipt");
    fs::rename(&marker, &saved).unwrap();
    fs::copy(&saved, &marker).unwrap();
    assert!(remove_ended(&root.0, &b.game, &b.session, &[]).is_err());
    assert!(pending(&root.0).unwrap());
}
#[test]
fn ended_cleanup_does_not_replay_any_saved_address_or_need_another_log_event() {
    let root = FixtureRoot::new();
    let b = binding(&root.0);
    let f = fixture();
    let p = plan();
    let permit = p.forward(&f, &manifest()).unwrap();
    let mut j = Journal::create(&b, &chosen()).unwrap();
    j.intent(&permit).unwrap();
    j.events = 2048;
    assert!(j.note("restored").is_err());
    drop(j);
    remove_ended(&root.0, &b.game, &b.session, &[]).unwrap();
    assert!(!pending(&root.0).unwrap());
}
#[test]
fn arbitrary_or_truncated_receipts_do_not_authorize_cleanup() {
    let root = FixtureRoot::new();
    let b = binding(&root.0);
    let marker = root.0.join("Data").join(MARKER);
    for content in [b"{}\n".as_slice(), b"{\"event\":\"header\"}".as_slice()] {
        fs::write(&marker, content).unwrap();
        assert!(remove_ended(&root.0, &b.game, &b.session, &[]).is_err());
        assert_eq!(fs::read(&marker).unwrap(), content);
        fs::remove_file(&marker).unwrap();
    }
}
#[test]
fn binding_rejects_raw_saved_or_mismatched_game_identity() {
    let root = FixtureRoot::new();
    let valid = binding(&root.0);
    assert!(
        Binding::from_launched(
            valid.game.clone(),
            valid.game.clone(),
            valid.game_root.clone(),
            root.0.clone(),
            valid.session.clone()
        )
        .is_err()
    );
    let mut game = valid.game.clone();
    game.path = r"C:\Other\NBA2K27.exe".into();
    assert!(
        Binding::from_launched(
            game,
            valid.parent.clone(),
            valid.game_root.clone(),
            root.0.clone(),
            valid.session.clone()
        )
        .is_err()
    );
    let mut game = valid.game.clone();
    game.started = "0".into();
    assert!(
        Binding::from_launched(
            game,
            valid.parent.clone(),
            valid.game_root.clone(),
            root.0.clone(),
            valid.session.clone()
        )
        .is_err()
    );
}
/// A shared fake clock that only the burst's pauses advance.
fn burst_clock() -> (std::rc::Rc<std::cell::Cell<u64>>, impl Fn() -> u64) {
    let now = std::rc::Rc::new(std::cell::Cell::new(1000u64));
    let read = now.clone();
    (now, move || read.get())
}
#[test]
fn first_ready_selection_takes_four_spaced_samples_in_one_poll() {
    // WPF AssetStable: 4 snapshots 100 ms apart, then the write. The provider
    // poll is >=250 ms plus every other feature, so one sample per poll took
    // several polls before Ready.
    let f = fixture();
    let (now, elapsed) = burst_clock();
    let captures = std::cell::Cell::new(0);
    let mut pauses = 0;
    let mut samples = policy::Samples::new();
    let plan = sample_burst(
        &mut samples,
        || {
            captures.set(captures.get() + 1);
            policy::capture(&f, &chosen())
        },
        elapsed,
        &mut || {
            pauses += 1;
            now.set(now.get() + 100);
        },
    );
    let plan = match plan {
        Ok(Some(plan)) => plan,
        _ => panic!("A stable ready selection must prepare within one poll"),
    };
    assert_eq!((captures.get(), pauses), (4, 3));
    plan.forward(&f, &manifest()).unwrap();
}
#[test]
fn burst_never_shortens_spacing_or_exceeds_its_bound() {
    let f = fixture();
    // A pause that does not advance the clock (sub-100 ms) never authorizes.
    let (_now, elapsed) = burst_clock();
    let captures = std::cell::Cell::new(0);
    let mut pauses = 0;
    let mut samples = policy::Samples::new();
    let result = sample_burst(
        &mut samples,
        || {
            captures.set(captures.get() + 1);
            policy::capture(&f, &chosen())
        },
        elapsed,
        &mut || pauses += 1,
    );
    assert!(matches!(result, Ok(None)));
    assert_eq!(
        (captures.get(), pauses),
        (BURST_ATTEMPTS, BURST_ATTEMPTS - 1)
    );
    // The started sequence is kept for the next poll, never shortened.
    assert!(samples.started());
}
#[test]
fn unsafe_or_failed_capture_resets_without_pausing() {
    let mut f = fixture();
    // Loaded court ball resource: not ready, no burst, samples reset.
    f.patch(
        f.base() + crate::ball_v2::RENDERER + 0x28 + 2 * 48,
        &[1; 48],
    );
    let (_now, elapsed) = burst_clock();
    let mut pauses = 0;
    let mut samples = policy::Samples::new();
    let result = sample_burst(
        &mut samples,
        || policy::capture(&f, &chosen()),
        elapsed,
        &mut || pauses += 1,
    );
    assert!(matches!(result, Err(Burst::Unsafe(_))));
    assert_eq!(pauses, 0);
    assert!(!samples.started());
    let (_now, elapsed) = burst_clock();
    let result = sample_burst(&mut samples, || anyhow::bail!("torn"), elapsed, &mut || {
        pauses += 1
    });
    assert!(matches!(result, Err(Burst::Capture(_))));
    assert_eq!(pauses, 0);
}
#[test]
fn mid_burst_change_restarts_the_four_sample_proof() {
    let f = fixture();
    let mut other = fixture();
    // A different (still ready) snapshot: unrelated record bytes changed.
    let at = 0x60000000000 + 2 * 416 + 400;
    other.patch(at, &[9]);
    let (now, elapsed) = burst_clock();
    let calls = std::cell::Cell::new(0);
    let mut samples = policy::Samples::new();
    let result = sample_burst(
        &mut samples,
        || {
            calls.set(calls.get() + 1);
            // Second observation differs; the proof restarts from it.
            policy::capture(if calls.get() == 2 { &other } else { &f }, &chosen())
        },
        elapsed,
        &mut || now.set(now.get() + 100),
    );
    // f, other (restart), f (restart), then f three more times: the plan
    // needs four equal spaced samples after the last change, within bound.
    assert!(matches!(result, Ok(Some(_))));
    assert_eq!(calls.get(), 6);
}
