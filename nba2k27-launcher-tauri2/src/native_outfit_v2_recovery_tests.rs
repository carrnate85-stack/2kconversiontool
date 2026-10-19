#[test]
fn orphaned_v2_loader_or_worker_receipts_block_launch_without_adoption() {
    for marker in [
        "loader-outfit-v2-session.json".to_owned(),
        format!("blacktop-outfit-v2-{}.journal", "a".repeat(48)),
        format!("blacktop-outfit-v2-{}.lease", "a".repeat(48)),
    ] {
        let root = fixture();
        let path = root.join("Data").join(marker);
        fs::write(&path, b"foreign").unwrap();
        assert!(mod_pending(&root).unwrap());
        let mut c = Controller::load(root.clone());
        assert!(c.clear_with_closed(Ok(vec![]), || Ok(())).is_err());
        let report = session_safety::smoke_fixture(&root).unwrap();
        assert_eq!(report["checksPassed"], false);
        assert_eq!(report["recoveryRequired"], true);
        assert!(
            c.launch(
                &game(&root),
                report,
                || Ok(vec![]),
                |_, _| panic!("Must not launch")
            )
            .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), b"foreign");
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn colliding_original_and_v2_loader_receipts_hold_both_before_any_cleanup() {
    let root = fixture();
    let mut c = modded_fixture(&root);
    let session = c.journal.as_ref().unwrap().session.clone();
    let mut old =
        crate::loader_files::Store::create(&root, &game(&root), &session, b"CONFIG\r\n", || Ok(()))
            .unwrap();
    old.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    drop(old);
    let marker = root.join("Data/loader-session.json");
    let before = fs::read(&marker).unwrap();
    let v2 = root.join("Data/loader-outfit-v2-session.json");
    fs::write(&v2, b"foreign").unwrap();
    assert!(
        c.clear_with_closed(Ok(vec![]), || Ok(()))
            .unwrap_err()
            .to_string()
            .contains("collide")
    );
    assert_eq!(fs::read(marker).unwrap(), before);
    assert_eq!(fs::read(v2).unwrap(), b"foreign");
    assert!(game(&root).join("version.dll").exists());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn version_two_loader_of_another_session_is_not_restored() {
    let root = fixture();
    let mut c = modded_fixture(&root);
    let foreign = "f".repeat(48);
    assert_ne!(c.journal.as_ref().unwrap().session, foreign);
    let mut s = crate::outfit_worker_v2_loader::Store::create(
        &root,
        &game(&root),
        &foreign,
        b"CONFIG\r\n",
        || Ok(()),
    )
    .unwrap();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    drop(s);
    let p = root.join("Data/loader-outfit-v2-session.json");
    let before = fs::read(&p).unwrap();
    assert!(c.clear_with_closed(Ok(vec![]), || Ok(())).is_err());
    assert_eq!(fs::read(p).unwrap(), before);
    assert!(game(&root).join("version.dll").exists());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn matching_v2_loader_restores_before_a_foreign_memory_only_worker_log_holds_final_recovery() {
    let root = fixture();
    let mut c = modded_fixture(&root);
    let session = c.journal.as_ref().unwrap().session.clone();
    let mut s = crate::outfit_worker_v2_loader::Store::create(
        &root,
        &game(&root),
        &session,
        b"CONFIG\r\n",
        || Ok(()),
    )
    .unwrap();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    drop(s);
    let foreign = root
        .join("Data")
        .join(format!("blacktop-outfit-v2-{}.journal", "f".repeat(48)));
    fs::write(&foreign, b"foreign").unwrap();
    assert!(c.clear_with_closed(Ok(vec![]), || Ok(())).is_err());
    assert!(!game(&root).join("version.dll").exists());
    assert!(!crate::outfit_worker_v2_loader::pending(&root).unwrap());
    assert_eq!(fs::read(foreign).unwrap(), b"foreign");
    assert_ne!(c.journal.as_ref().unwrap().phase, Phase::Closed);
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn matching_v2_loader_closed_recovery_never_requires_game_memory_access() {
    let root = fixture();
    let mut c = modded_fixture(&root);
    let session = c.journal.as_ref().unwrap().session.clone();
    let mut s = crate::outfit_worker_v2_loader::Store::create(
        &root,
        &game(&root),
        &session,
        b"CONFIG\r\n",
        || Ok(()),
    )
    .unwrap();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    s.trigger(|| Ok(())).unwrap();
    drop(s);
    c.clear_with_closed(Ok(vec![]), || Ok(())).unwrap();
    assert_eq!(c.journal.as_ref().unwrap().phase, Phase::Closed);
    assert!(!mod_pending(&root).unwrap());
    assert!(!game(&root).join("version.dll").exists());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
