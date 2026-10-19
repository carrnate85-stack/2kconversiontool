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
