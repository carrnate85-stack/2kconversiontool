use super::*;
#[test]
fn freeze_once_and_irreversible_effect_marker() {
    let mut l = Lifecycle::new();
    assert!(l.before_effect().is_err());
    l.freeze(100).unwrap();
    assert!(l.freeze(101).is_err());
    l.check(101, false, true).unwrap();
    l.before_effect().unwrap();
    assert!(l.attempted);
    l.consistent().unwrap();
    assert!(l.attempted);
    assert!(l.may_release());
    assert!(l.before_effect().is_err());
}
#[test]
fn exact_deadlines_and_rollback_budget() {
    for elapsed in [0, 1, 4999] {
        let mut l = Lifecycle::new();
        l.freeze(1).unwrap();
        assert!(l.check(1 + elapsed, false, true).is_ok());
    }
    for elapsed in [5000, 5001, 7999] {
        let mut l = Lifecycle::new();
        l.freeze(1).unwrap();
        assert!(l.check(1 + elapsed, false, true).is_err());
        assert!(l.check(1 + elapsed, true, true).is_ok());
    }
    for elapsed in [8000, 8001, u64::MAX - 1] {
        let mut l = Lifecycle::new();
        l.freeze(1).unwrap();
        assert!(l.check(1 + elapsed, true, true).is_err());
    }
}
#[test]
fn companion_loss_allows_only_bounded_reverse() {
    let mut l = Lifecycle::new();
    l.freeze(10).unwrap();
    l.before_effect().unwrap();
    assert!(l.check(11, false, false).is_err());
    assert!(l.check(11, true, false).is_ok());
    assert!(!l.may_release());
    l.consistent().unwrap();
    assert!(l.may_release());
}
#[test]
fn backwards_clock_permanently_holds() {
    let mut l = Lifecycle::new();
    l.freeze(100).unwrap();
    l.check(110, false, true).unwrap();
    assert!(l.check(109, false, true).is_err());
    assert_eq!(l.phase, Phase::Held);
    assert!(l.check(111, true, true).is_err());
    assert!(!l.may_release());
    assert!(l.consistent().is_err());
}
#[test]
fn zero_clock_is_valid_and_no_deadline_renewal() {
    let mut l = Lifecycle::new();
    l.freeze(0).unwrap();
    for now in 0..5000 {
        l.check(now, false, true).unwrap();
    }
    assert!(l.check(5000, false, true).is_err());
    assert_eq!(l.start, Some(0));
    assert!(l.freeze(5000).is_err());
}
#[test]
fn unknown_partial_is_not_release_authority() {
    let mut l = Lifecycle::new();
    l.freeze(1).unwrap();
    assert!(l.may_release());
    l.before_effect().unwrap();
    assert!(!l.may_release());
    assert!(l.attempted);
    l.phase = Phase::Held;
    assert!(!l.may_release());
    assert!(l.consistent().is_err());
}
#[test]
fn operation_bounds_and_shape() {
    assert!(
        Operation::Season(serde_json::json!({"action":"addOlder","playerId":12}))
            .validate()
            .is_ok()
    );
    assert!(Operation::Season(Value::Null).validate().is_err());
    assert!(
        Operation::ArenaClone(serde_json::json!({"name":"x".repeat(MAX_OPERATION)}))
            .validate()
            .is_err()
    );
}
#[test]
fn request_requires_consent_and_identity_capabilities() {
    let id = Identity {
        pid: 1,
        started: "10".into(),
        path: "C:\\a.exe".into(),
    };
    let game = Identity {
        pid: 2,
        started: "11".into(),
        path: "C:\\NBA.exe".into(),
    };
    let mut r = Request {
        root: "C:\\root".into(),
        game,
        parent: id,
        session: "0".repeat(48),
        operation: Operation::Season(serde_json::json!({"action":"addOlder"})),
        emergency_close_consent: true,
        game_handle: 8,
        parent_handle: 12,
        prior_verified: Vec::new(),
    };
    assert!(r.validate().is_ok());
    r.emergency_close_consent = false;
    assert!(r.validate().is_err());
    r.emergency_close_consent = true;
    r.parent_handle = 8;
    assert!(r.validate().is_err());
    r.parent_handle = 12;
    r.session = "../other".into();
    assert!(r.validate().is_err());
}
#[test]
fn retained_intent_cng_sha_matches_known_vectors_and_fresh_changed_input() {
    for (bytes, expected) in [
        (
            Vec::new(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc".to_vec(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            vec![b'a'; 1000000],
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        ),
    ] {
        assert_eq!(platform::intent_sha256(&bytes).unwrap(), expected);
    }
    use sha2::{Digest, Sha256};
    let mut bytes = vec![0x7a; 8192];
    let old = platform::intent_sha256(&bytes).unwrap();
    for i in 0..16 {
        bytes[i] ^= 1;
        let digest = platform::intent_sha256(&bytes).unwrap();
        assert_eq!(digest, format!("{:x}", Sha256::digest(&bytes)));
        assert_ne!(digest, old);
    }
}
#[test]
fn retained_intent_cng_sha_parallel_and_nested_have_no_shared_mutable_state() {
    use sha2::{Digest, Sha256};
    let workers: Vec<_> = (0..8u8)
        .map(|worker| {
            std::thread::spawn(move || {
                for case in 0..100u8 {
                    let mut bytes = vec![worker; 257];
                    bytes[0] = case;
                    let expected = format!("{:x}", Sha256::digest(&bytes));
                    assert_eq!(platform::intent_sha256(&bytes).unwrap(), expected);
                    assert_eq!(
                        platform::intent_sha256(expected.as_bytes()).unwrap(),
                        format!("{:x}", Sha256::digest(expected.as_bytes()))
                    );
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
#[test]
fn teardown_identity_failure_only_accepts_signaled_exact_retained_writer() {
    assert!(
        !platform::helper_stopped_on_identity_error(Ok(()), |_| panic!(
            "no wait required for valididentity"
        ))
        .unwrap()
    );
    assert!(
        platform::helper_stopped_on_identity_error(
            Err(anyhow::anyhow!("image unavailable")),
            |ms| {
                assert_eq!(ms, 250);
                0
            }
        )
        .unwrap()
    );
    for result in [258, u32::MAX, 1] {
        let error = platform::helper_stopped_on_identity_error(
            Err(anyhow::anyhow!("original identity failure")),
            |ms| {
                assert_eq!(ms, 250);
                result
            },
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "original identity failure");
    }
}

#[test]
fn watchdog_deadline_starts_at_ack_not_bootstrap_and_never_renews() {
    let mut d = WatchdogDeadline::new(100);
    d.check(3500).unwrap();
    d.freeze_ack(3500).unwrap();
    d.check(11499).unwrap();
    assert!(d.freeze_ack(11500).is_err());
    assert!(d.check(11501).is_err());
    let mut d = WatchdogDeadline::new(100);
    d.freeze_ack(3500).unwrap();
    d.check(13499).unwrap();
    assert!(d.check(13500).is_err());
    let mut d = WatchdogDeadline::new(100);
    d.check(30099).unwrap();
    assert!(d.check(30100).is_err());
    let mut d = WatchdogDeadline::new(100);
    assert!(d.check(99).is_err());
    assert!(d.freeze_ack(101).is_err());
}
#[test]
fn authenticated_shutdown_is_bounded_and_stops_on_exact_handle_exit() {
    use std::cell::Cell;
    let n = Cell::new(0);
    let waits = std::cell::RefCell::new(Vec::new());
    assert!(
        platform::bounded_retained_shutdown(
            || {
                n.set(n.get() + 1);
                false
            },
            |ms| {
                waits.borrow_mut().push(ms);
                258
            }
        )
        .is_err()
    );
    assert_eq!(n.get(), 3);
    assert_eq!(*waits.borrow(), vec![0, 1000, 1000, 1000]);
    platform::bounded_retained_shutdown(|| panic!("already stopped"), |_| 0).unwrap();
    let n = Cell::new(0);
    platform::bounded_retained_shutdown(
        || {
            n.set(n.get() + 1);
            true
        },
        |ms| if ms == 0 { 258 } else { 0 },
    )
    .unwrap();
    assert_eq!(n.get(), 1);
}
#[test]
fn exited_thread_retries_whole_admission_once_unknown_failure_never_retries() {
    let mut n = 0;
    platform::bounded_thread_snapshot(|| {
        n += 1;
        Ok(n == 2)
    })
    .unwrap();
    assert_eq!(n, 2);
    let mut n = 0;
    assert!(
        platform::bounded_thread_snapshot(|| {
            n += 1;
            Ok(false)
        })
        .is_err()
    );
    assert_eq!(n, 2);
    let mut n = 0;
    assert!(
        platform::bounded_thread_snapshot(|| {
            n += 1;
            anyhow::bail!("unknown")
        })
        .is_err()
    );
    assert_eq!(n, 1);
}

#[test]
fn exact_handle_archive_never_appends_filename_bytes_at_alignment_boundaries() {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        os::windows::fs::OpenOptionsExt,
    };
    let dir = std::env::temp_dir().join(format!(
        "C27-rename-boundaries-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    for n in 0..32 {
        let source = dir.join(format!("{n}.pending"));
        let target = dir.join(format!("{}.completed", "a".repeat(64 + n)));
        let mut f = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .share_mode(1)
            .open(&source)
            .unwrap();
        f.write_all(b"immutable evidence").unwrap();
        f.sync_all().unwrap();
        drop(f);
        let f = OpenOptions::new()
            .read(true)
            .access_mode(0x80010000)
            .share_mode(1)
            .open(&source)
            .unwrap();
        platform::archive_intent(&f, &target).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"immutable evidence");
    }
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 32);
    fs::remove_dir_all(&dir).unwrap();
}
