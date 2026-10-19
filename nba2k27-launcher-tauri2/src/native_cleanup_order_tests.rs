// Included by native_session::tests so these fixtures exercise Controller's
// actual recovery orchestration, not a duplicate cleanup implementation.
#[test]
fn corrupt_memory_receipts_never_strand_exact_owned_loader_camera_or_profile_files() {
    for marker in [
        None,
        Some("game-rules-journal.json"),
        Some("roster-write-journal.json"),
        Some("team-write-journal.json"),
        Some("uniform-write-journal.json"),
        Some("blacktop-arena-session.json"),
        Some("blacktop-ball-session.json"),
        Some(crate::ball_gameplay_runtime::MARKER),
        Some(crate::outfit_journal::MARKER),
        Some(crate::feature_journal::MARKER),
        Some(crate::presentation_v2_runtime::MARKER),
        Some(crate::scoreboard_runtime::MARKER),
    ] {
        let root = fixture();
        fs::create_dir_all(root.join("App")).unwrap();
        fs::create_dir_all(root.join("Library")).unwrap();
        let selected = root.join("SelectedMods");
        fs::create_dir(&selected).unwrap();
        fs::write(root.join("Data/settings.json"), b"preserved settings").unwrap();
        fs::write(root.join("Library/keep.mp4"), b"preserved library").unwrap();
        fs::write(selected.join("user.iff"), b"preserved mod").unwrap();
        let mut c = modded_fixture(&root);
        let mut j = c.journal.clone().unwrap();
        j.game = Some(Identity {
            pid: 123456,
            started: "133000000000000000".into(),
            path: game(&root).join("NBA2K27.exe").to_string_lossy().into(),
        });
        j.phase = Phase::CleanupPending;
        c.commit(j.clone()).unwrap();
        let game_root = game(&root).canonicalize().unwrap();
        let mut profile =
            crate::mod_profile::Store::create(&root, &game_root, &selected, &j.session, || Ok(()))
                .unwrap()
                .unwrap();
        let prepared = vec![9; 128];
        let mut camera = crate::camera_name_files::Store::create(
            &root,
            &selected,
            &j.session,
            &prepared,
            || Ok(()),
        )
        .unwrap();
        camera.activate(&prepared, || Ok(())).unwrap();
        profile.activate(|| Ok(())).unwrap();
        let config = crate::loader_files::configuration(
            &format!(
                "{}{}",
                crate::provider_protocol::PIPE_PREFIX,
                "11".repeat(32)
            ),
            "__2k_launcher__/probe.bin",
            7,
            &crate::loader_files::digest(b"fixture"),
            &root,
            &["loc/english.iff".into()],
            false,
        )
        .unwrap();
        let mut loader =
            crate::loader_files::Store::create(&root, &game_root, &j.session, &config, || Ok(()))
                .unwrap();
        loader.install_loader(&config, || Ok(())).unwrap();
        drop(loader);
        drop(camera);
        drop(profile);
        if let Some(marker) = marker {
            fs::write(root.join("Data").join(marker), b"damaged receipt preserved").unwrap();
        }
        let result = c.clear_with_closed(Ok(vec![]), || Ok(()));
        assert_eq!(result.is_err(), marker.is_some(), "{marker:?}: {result:?}");
        assert!(!game_root.join("version.dll").exists(), "{marker:?}");
        assert!(!selected.join("loc/english.iff").exists(), "{marker:?}");
        assert!(
            !game_root.join("mods").exists(),
            "Originally absent mods folder was not restored"
        );
        for removed in [
            "loader-session.json",
            "mod-profile-session.json",
            crate::camera_name_files::MARKER,
        ] {
            assert!(
                !root.join("Data").join(removed).exists(),
                "{marker:?}: {removed}"
            );
        }
        if let Some(marker) = marker {
            assert_eq!(
                fs::read(root.join("Data").join(marker)).unwrap(),
                b"damaged receipt preserved"
            );
        }
        assert_eq!(
            fs::read(root.join("Data/settings.json")).unwrap(),
            b"preserved settings"
        );
        assert_eq!(
            fs::read(root.join("Library/keep.mp4")).unwrap(),
            b"preserved library"
        );
        assert_eq!(
            fs::read(selected.join("user.iff")).unwrap(),
            b"preserved mod"
        );
        drop(c);
        fs::remove_dir_all(root).unwrap();
    }
}
