use super::*;
fn fixture() -> (PathBuf, Binding) {
    let root = std::env::temp_dir().join(format!(
        "scoreboard-v2-journal-{}",
        crate::backend::unique_id()
    ));
    fs::create_dir_all(root.join("Data")).unwrap();
    let b = Binding::from_launched(
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
        root.clone(),
        "a".repeat(48),
    )
    .unwrap();
    (root, b)
}
fn snapshot() -> (Snapshot, Plan) {
    let f = crate::scoreboard_v2::tests::fixture();
    let s = crate::scoreboard_v2::capture(&f, "era-1990", true).unwrap();
    let p = s.plan().unwrap();
    (s, p)
}
#[test]
fn real_journal_exclusive_intent_is_pinned_and_exact_permit_bound() {
    let (root, b) = fixture();
    let (mut s, p) = snapshot();
    let mut j = Journal::create(&b, "era-1990").unwrap();
    assert!(Journal::create(&b, "simple").is_err());
    assert!(fs::write(root.join("Data").join(MARKER), b"foreign").is_err());
    assert!(fs::remove_file(root.join("Data").join(MARKER)).is_err());
    let permit = Permit::fixture(&s, &p, 0, 0, false);
    assert!(j.verify(&permit).is_err());
    j.intent(&s, &p).unwrap();
    j.verify(&permit).unwrap();
    j.progress(1, 0).unwrap();
    j.verify(&Permit::fixture(&s, &p, 1, 0, false)).unwrap();
    assert!(j.verify(&Permit::fixture(&s, &p, 3, 0, false)).is_err());
    assert!(j.verify(&Permit::fixture(&s, &p, 1, 0, true)).is_err());
    j.restore_intent(&s, &p, 2, 0).unwrap();
    j.verify(&Permit::fixture(&s, &p, 2, 0, true)).unwrap();
    assert!(j.verify(&Permit::fixture(&s, &p, 3, 0, true)).is_err());
    j.proof.as_mut().unwrap().bytes[0] ^= 1;
    assert!(j.verify(&Permit::fixture(&s, &p, 2, 0, true)).is_err());
    drop(j);
    remove_ended(&root, &b.game, &b.session, &[]).unwrap();
    assert!(!pending(&root).unwrap());
    fs::remove_dir_all(root).unwrap();
    let _ = &mut s;
}
#[test]
fn recovery_never_replays_and_preserves_foreign_truncated_or_live_logs() {
    let (root, b) = fixture();
    let j = Journal::create(&b, "simple").unwrap();
    drop(j);
    let rows = [session_safety::Process {
        name: "NBA2K27.exe".into(),
        pid: b.game.pid,
        parent: b.parent.pid,
        identity: Some(b.game.clone()),
    }];
    assert!(remove_ended(&root, &b.game, &b.session, &rows).is_err());
    assert!(remove_ended(&root, &b.game, &"b".repeat(48), &[]).is_err());
    let path = root.join("Data").join(MARKER);
    let mut bytes = fs::read(&path).unwrap();
    bytes.pop();
    fs::write(&path, &bytes).unwrap();
    assert!(remove_ended(&root, &b.game, &b.session, &[]).is_err());
    assert!(pending(&root).unwrap());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn journal_limits_hold_before_any_additional_effect_and_keep_existing_proof() {
    let (root, b) = fixture();
    let (s, p) = snapshot();
    let mut j = Journal::create(&b, "era-1990").unwrap();
    j.intent(&s, &p).unwrap();
    j.events = 2048;
    assert!(j.progress(1, 0).is_err());
    j.verify(&Permit::fixture(&s, &p, 0, 0, false)).unwrap();
    drop(j);
    remove_ended(&root, &b.game, &b.session, &[]).unwrap();
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn runner_never_acquires_capability_without_parent_or_exact_verified_owner() {
    let (root, b) = fixture();
    let mut runner = Runner::new(b, "simple", true).unwrap();
    runner.tick(false).unwrap();
    assert!(!pending(&root).unwrap());
    runner.tick(true).unwrap();
    assert!(!pending(&root).unwrap());
    assert!(
        runner.status()["message"]
            .as_str()
            .unwrap()
            .starts_with("Waiting for exact Version2 scoreboard owner:")
    );
    assert!(runner.native.is_none());
    assert!(runner.journal.is_none());
    assert!(!runner.service.owns_mutation());
    assert_eq!(runner.status()["gameplayVerified"], false);
    assert!(Runner::new(runner.binding.clone(), "flashback-openrun", true).is_err());
    assert!(Runner::new(runner.binding.clone(), "w-scrimmage", false).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn copied_receipt_bytes_never_authorize_removing_a_replacement_file() {
    let (root, b) = fixture();
    let j = Journal::create(&b, "simple").unwrap();
    drop(j);
    let path = root.join("Data").join(MARKER);
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, root.join("original.jsonl")).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(remove_ended(&root, &b.game, &b.session, &[]).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn exhausted_valid_journal_cannot_strand_closed_session_cleanup() {
    let (root, b) = fixture();
    let mut runner = Runner::new(b.clone(), "simple", true).unwrap();
    let mut j = Journal::create(&b, "simple").unwrap();
    j.events = 2048;
    runner.journal = Some(j);
    runner.finish_ended(&[]).unwrap();
    assert!(!pending(&root).unwrap());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn latched_transaction_fault_stays_visible_while_waiting_for_safe_restoration() {
    let (root, b) = fixture();
    let mut runner = Runner::new(b, "simple", true).unwrap();
    runner.held = true;
    runner.status = Status::Held("Scoreboard apply is incomplete".into());
    runner.observe_status(Status::Waiting);
    let status = runner.status();
    assert_eq!(status["phase"], "held");
    assert_eq!(status["message"], "Scoreboard apply is incomplete");
    assert_eq!(status["held"], true);
    assert!(runner.choose("broadcast", true).is_err());
    // An actual completed restoration may update the message, but never
    // reauthorize another forward transaction in this held game session.
    runner.observe_status(Status::Restored);
    assert_eq!(runner.status()["phase"], "restored-held");
    assert_eq!(
        runner.status()["failureReason"],
        "Scoreboard apply is incomplete"
    );
    runner.observe_status(Status::Held("Later restoration observation".into()));
    runner.observe_status(Status::Restored);
    assert_eq!(
        runner.status()["failureReason"],
        "Scoreboard apply is incomplete"
    );
    assert!(runner.choose("broadcast", true).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn preflight_fault_without_owned_transaction_can_resume_normal_waiting() {
    let (root, b) = fixture();
    let mut runner = Runner::new(b, "simple", true).unwrap();
    runner.observe_status(Status::Held("Descriptor not loaded yet".into()));
    runner.observe_status(Status::Waiting);
    assert_eq!(runner.status()["phase"], "waiting");
    assert_eq!(runner.status()["held"], false);
    assert_eq!(runner.status()["failureReason"], serde_json::Value::Null);
    runner.choose("broadcast", true).unwrap();
    fs::remove_dir_all(root).unwrap();
}
