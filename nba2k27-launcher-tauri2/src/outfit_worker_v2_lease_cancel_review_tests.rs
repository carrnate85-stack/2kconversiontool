use super::*;
use std::cell::Cell;

#[test]
fn revocation_attempts_both_channels_and_remains_terminal_for_every_fault() {
    for (reset_fails, signal_fails) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let terminal = AtomicBool::new(false);
        let reset_calls = Cell::new(0);
        let signal_calls = Cell::new(0);
        let result = revoke(
            &terminal,
            || {
                assert!(terminal.load(Ordering::Acquire));
                reset_calls.set(reset_calls.get() + 1);
                ensure!(!reset_fails, "injected-reset-fault");
                Ok(())
            },
            || {
                assert!(terminal.load(Ordering::Acquire));
                signal_calls.set(signal_calls.get() + 1);
                ensure!(!signal_fails, "injected-signal-fault");
                Ok(())
            },
        );
        assert_eq!(reset_calls.get(), 1);
        assert_eq!(signal_calls.get(), 1);
        assert_eq!(result.is_err(), reset_fails || signal_fails);
        assert!(not_cancelled(&terminal).is_err());
        if reset_fails && signal_fails {
            let text = format!("{:#}", result.unwrap_err());
            assert!(
                text.contains("injected-reset-fault") && text.contains("injected-signal-fault")
            );
        }
        // A later successful OS retry does not grant local authority again.
        revoke(&terminal, || Ok(()), || Ok(())).unwrap();
        assert!(not_cancelled(&terminal).is_err());
    }
}

fn fixture() -> (PathBuf, String, Identity, Identity, Identity) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(format!(
            "lease-cancel-review-{}",
            crate::backend::unique_id()
        ));
    for name in ["App", "Data", "Library"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    let worker = Identity {
        pid: std::process::id(),
        started: "1".into(),
        path: root.join("App/provider.exe").to_string_lossy().into(),
    };
    let parent = Identity {
        pid: worker.pid + 1,
        started: "2".into(),
        path: root.join("App/launcher.exe").to_string_lossy().into(),
    };
    let game = Identity {
        pid: worker.pid + 2,
        started: "3".into(),
        path: root.join("Game/NBA2K27.exe").to_string_lossy().into(),
    };
    (root, crate::backend::unique_id(), worker, parent, game)
}

#[test]
fn actual_event_rights_failures_cannot_reallow_or_reuse_an_existing_admission() {
    for (reset_fails, signal_fails) in [(true, false), (false, true), (true, true)] {
        let (root, session, worker, parent, game) = fixture();
        let mut lease = Lease::prepare(
            &root,
            &session,
            &worker,
            &parent,
            Path::new(&game.path),
            || Ok(()),
        )
        .unwrap();
        lease.bind(&game, || Ok(())).unwrap();
        lease.allow(|| Ok(())).unwrap();
        let admission = lease.admission().unwrap();
        admission.check(&root, &session, &game, &parent).unwrap();
        // SYNCHRONIZE-only duplicate handles deterministically reject actual
        // ResetEvent/SetEvent on our own named fixture objects. No process or
        // game API is used and the original writer handles remain retained.
        let permit_writer = reset_fails.then(|| {
            let read_only = lease.permit.reader().unwrap();
            std::mem::replace(&mut lease.permit, read_only)
        });
        let cancel_writer = signal_fails.then(|| {
            let read_only = lease.cancel.reader().unwrap();
            std::mem::replace(&mut lease.cancel, read_only)
        });
        assert!(lease.cancel().is_err());
        assert_eq!(lease.event_state().unwrap(), (!signal_fails, reset_fails));
        assert!(
            lease
                .allow(|| panic!("Cancelled lease must not retry its live guard"))
                .is_err()
        );
        assert!(lease.admission().is_err());
        assert!(admission.check(&root, &session, &game, &parent).is_err());
        assert!(
            lease
                .bind(&game, || panic!("Cancelled lease must not bind again"))
                .is_err()
        );
        assert!(pending(&root).unwrap());
        drop(admission);
        if let Some(writer) = permit_writer {
            lease.permit = writer;
        }
        if let Some(writer) = cancel_writer {
            lease.cancel = writer;
        }
        // A closed-only cleanup may still revoke and remove exact owned files.
        lease.finish_ended(|| Ok(())).unwrap();
        drop(lease);
        assert!(!pending(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn bound_recovery_preserves_wrong_target_or_missing_changed_game_identity() {
    let (root, session, worker, parent, game) = fixture();
    let mut lease = Lease::prepare(
        &root,
        &session,
        &worker,
        &parent,
        Path::new(&game.path),
        || Ok(()),
    )
    .unwrap();
    lease.bind(&game, || Ok(())).unwrap();
    drop(lease);
    let (body, marker) = paths(&root, &session).unwrap();
    let before = (fs::read(&body).unwrap(), fs::read(&marker).unwrap());
    let mut changed = game.clone();
    changed.started = "4".into();
    for (target, expected) in [
        (root.join("Other/NBA2K27.exe"), Some(&game)),
        (PathBuf::from(&game.path), None),
        (PathBuf::from(&game.path), Some(&changed)),
    ] {
        assert!(resolve_ended_bound(&root, &session, &target, expected, || Ok(())).is_err());
        assert_eq!(fs::read(&body).unwrap(), before.0);
        assert_eq!(fs::read(&marker).unwrap(), before.1);
    }
    resolve_ended_bound(&root, &session, Path::new(&game.path), Some(&game), || {
        Ok(())
    })
    .unwrap();
    assert!(!pending(&root).unwrap());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bound_header_only_recovery_requires_the_exact_target_but_no_game() {
    let (root, session, worker, parent, game) = fixture();
    let lease = Lease::prepare(
        &root,
        &session,
        &worker,
        &parent,
        Path::new(&game.path),
        || Ok(()),
    )
    .unwrap();
    drop(lease);
    assert!(
        resolve_ended_bound(
            &root,
            &session,
            &root.join("Other/NBA2K27.exe"),
            None,
            || Ok(())
        )
        .is_err()
    );
    assert!(pending(&root).unwrap());
    resolve_ended_bound(&root, &session, Path::new(&game.path), None, || Ok(())).unwrap();
    assert!(!pending(&root).unwrap());
    fs::remove_dir_all(root).unwrap();
}
