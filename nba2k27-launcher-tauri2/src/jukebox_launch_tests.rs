include!("jukebox_test_paths.rs");
use super::*;
use crate::jukebox_audio;
use serde_json::{Map, json};
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) mods: PathBuf,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let root = fixture_artifacts().join(format!(
            "2k-jukebox-launch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&root).unwrap();
        let mods = root.join("mods");
        fs::create_dir(&mods).unwrap();
        Self { root, mods }
    }
    fn service(&self) -> Service {
        Service::open(&self.root, stock(), &"a".repeat(48)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("2k-jukebox-launch-")
        );
        fs::remove_dir_all(&self.root).unwrap();
    }
}
pub(crate) fn stock() -> Music {
    Music {
        tracks: [1, 2, 100]
            .into_iter()
            .map(|bank| catalog::Track {
                name: format!("Stock {bank}"),
                artist: "Artist".into(),
                cover: "001_helicopter".into(),
                collection: 1,
                bank,
                unique: bank,
                extra: Map::new(),
            })
            .collect(),
        extra: Map::new(),
    }
}
pub(crate) fn selected(f: &Fixture, custom: bool) -> (Service, Plan) {
    let mut service = f.service();
    let id = "b".repeat(48);
    service.create_playlist("Playlist", &id).unwrap();
    if custom {
        service
            .import_audio(
                &id,
                "Custom",
                "Artist",
                &jukebox_audio::pcm_wave(&[0; 1920]).unwrap(),
                &"c".repeat(48),
            )
            .unwrap();
    } else {
        service
            .set_order(&id, vec!["1:2:2".into(), "1:1:1".into()], &"c".repeat(48))
            .unwrap();
    }
    service.set_active(Some(&id), &"d".repeat(48)).unwrap();
    let plan = Plan::from_state(service.view(), &service.fingerprint().unwrap()).unwrap();
    (service, plan)
}
pub(crate) fn prepare_fixture(f: &Fixture, plan: &Plan, session: &str) -> Result<Option<Session>> {
    prepare_with_stock(
        &f.root,
        &f.root,
        &f.mods,
        session,
        plan,
        &stock(),
        Checks {
            prepare: &|| Ok(()),
            poll: &|| Ok(()),
            cleanup: &|| Ok(()),
        },
    )
}

#[test]
fn default_plan_has_no_files_no_loader_fields_and_no_game_access() {
    let f = Fixture::new();
    let plan = Plan::default();
    plan.validate().unwrap();
    assert!(!plan.enabled());
    assert!(plan.stock_overrides().is_empty());
    assert!(
        prepare(
            &f.root,
            &f.root.join("missing-game"),
            &f.mods,
            &"e".repeat(48),
            &plan,
            Checks {
                prepare: &|| Ok(()),
                poll: &|| Ok(()),
                cleanup: &|| Ok(())
            }
        )
        .unwrap()
        .is_none()
    );
    assert!(!f.root.join("Data").exists());
    assert_eq!(
        append_loader_config(b"PIPE=owned\r\n", None).unwrap(),
        b"PIPE=owned\r\n"
    );
}
#[test]
fn saved_launch_freeze_needs_no_api_http_dialog_or_new_files() {
    let fresh = Fixture::new();
    assert!(!freeze_saved(&fresh.root).unwrap().enabled());
    assert!(!fresh.root.join("Data").exists());
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    assert_eq!(freeze_saved(&f.root).unwrap(), plan);
    fs::remove_file(f.root.join("Data/Jukebox/library.json")).unwrap();
    assert!(freeze_saved(&f.root).is_err());
    assert!(!f.root.join("Data/Jukebox/library.json").exists());
}
#[test]
fn lightweight_selection_summary_never_initializes_the_jukebox_or_stock_catalog() {
    let fresh = Fixture::new();
    assert_eq!(
        selection_summary(&fresh.root).unwrap(),
        SelectionSummary {
            library_ready: false,
            playlist_count: 0,
            playlist_id: None,
            playlist_name: None,
            song_count: 0,
            revision: None,
            playlists: vec![],
        }
    );
    assert!(!fresh.root.join("Data").exists());

    let f = Fixture::new();
    let (service, _) = selected(&f, false);
    assert_eq!(
        selection_summary(&f.root).unwrap(),
        SelectionSummary {
            library_ready: true,
            playlist_count: 1,
            playlist_id: Some("b".repeat(48)),
            playlist_name: Some("Playlist".into()),
            song_count: 2,
            revision: Some(service.fingerprint().unwrap()),
            playlists: vec![PlaylistChoice {
                id: "b".repeat(48),
                name: "Playlist".into(),
                song_count: 2
            }],
        }
    );
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(f.root.join("Data/Jukebox/library.json")).unwrap())
            .unwrap();
    value["activePlaylistId"] = json!("e".repeat(48));
    fs::write(
        f.root.join("Data/Jukebox/library.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    assert!(selection_summary(&f.root).is_err());
}

#[test]
fn play_selection_is_revision_checked_persists_and_never_initializes_music() {
    let f = Fixture::new();
    let (_, original_plan) = selected(&f, false);
    let selected = selection_summary(&f.root).unwrap();
    let before = fs::read(f.root.join("Data/Jukebox/library.json")).unwrap();
    assert!(
        select_saved(
            &f.root,
            Some(&"f".repeat(48)),
            selected.revision.as_deref().unwrap()
        )
        .is_err()
    );
    assert_eq!(
        fs::read(f.root.join("Data/Jukebox/library.json")).unwrap(),
        before
    );
    let off = select_saved(&f.root, None, selected.revision.as_deref().unwrap()).unwrap();
    assert_eq!(off.playlists, selected.playlists);
    assert!(off.playlist_id.is_none());
    assert!(!freeze_saved(&f.root).unwrap().enabled());
    assert!(
        select_saved(
            &f.root,
            selected.playlist_id.as_deref(),
            selected.revision.as_deref().unwrap()
        )
        .is_err()
    );
    let enabled = select_saved(
        &f.root,
        selected.playlist_id.as_deref(),
        off.revision.as_deref().unwrap(),
    )
    .unwrap();
    assert_eq!(enabled, selected);
    assert_eq!(freeze_saved(&f.root).unwrap(), original_plan);
    let runtime = crate::jukebox_runtime::Runtime::new(f.root.clone());
    assert!(
        runtime
            .select_for_play(json!({"revision":enabled.revision}))
            .is_err()
    );
    assert!(
        runtime
            .select_for_play(
                json!({"playlistId":null,"revision":enabled.revision,"path":"outside"})
            )
            .is_err()
    );
    let result = runtime
        .select_for_play(json!({"playlistId":null,"revision":enabled.revision}))
        .unwrap();
    assert!(result["playlistId"].is_null());
    assert!(!f.root.join("Cache/Jukebox").exists());
}
#[test]
fn plan_is_bounded_path_free_and_preserves_exact_song_order() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, false);
    assert_eq!(
        plan.selection.as_ref().unwrap().track_keys,
        vec!["1:2:2", "1:1:1"]
    );
    assert_eq!(plan.stock_overrides(), vec!["music.iff"]);
    let bytes = serde_json::to_vec(&plan).unwrap();
    assert!(bytes.len() < 1024);
    assert!(!String::from_utf8_lossy(&bytes).contains("mods"));
    let round: Plan = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(round, plan);
    let mut foreign = serde_json::to_value(&plan).unwrap();
    foreign["selection"]["path"] = json!("C:/Windows");
    assert!(serde_json::from_value::<Plan>(foreign).is_err());
    for bad in ["../music.iff", "5:1:1", "1:100000:1", "1:+1:1"] {
        let mut p = plan.clone();
        p.selection.as_mut().unwrap().track_keys = vec![bad.into()];
        assert!(p.validate().is_err());
    }
    let mut p = plan.clone();
    p.selection.as_mut().unwrap().track_keys = vec!["1:1:1".into(); 72];
    assert!(p.validate().is_err());
}
#[test]
fn stale_revision_stock_choice_and_metadata_fail_before_publication() {
    let f = Fixture::new();
    let (mut service, plan) = selected(&f, false);
    service
        .rename_playlist(&"b".repeat(48), "Changed after Play", &"e".repeat(48))
        .unwrap();
    assert!(prepare_fixture(&f, &plan, &"f".repeat(48)).is_err());
    assert!(!pending(&f.root).unwrap());
    assert!(!f.mods.join("music.iff").exists());
    let fresh = Plan::from_state(service.view(), &service.fingerprint().unwrap()).unwrap();
    let mut wrong = stock();
    wrong.tracks[0].name = "Different stock".into();
    assert!(
        prepare_with_stock(
            &f.root,
            &f.root,
            &f.mods,
            &"f".repeat(48),
            &fresh,
            &wrong,
            Checks {
                prepare: &|| Ok(()),
                poll: &|| Ok(()),
                cleanup: &|| Ok(())
            }
        )
        .is_err()
    );
    assert!(!pending(&f.root).unwrap());
    let mut forged = fresh.clone();
    forged.selection.as_mut().unwrap().track_keys.reverse();
    assert!(prepare_fixture(&f, &forged, &"f".repeat(48)).is_err());
}
#[test]
fn missing_sealed_library_never_creates_default_document_or_receipt() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, false);
    let missing = Fixture::new();
    assert!(
        prepare_with_stock(
            &missing.root,
            &missing.root,
            &missing.mods,
            &"e".repeat(48),
            &plan,
            &stock(),
            Checks {
                prepare: &|| Ok(()),
                poll: &|| Ok(()),
                cleanup: &|| Ok(())
            }
        )
        .is_err()
    );
    assert!(!missing.root.join("Data").exists());
}
#[test]
fn actual_native_publication_verify_and_ended_recovery_are_session_owned() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    let id = "e".repeat(48);
    let session = prepare_fixture(&f, &plan, &id).unwrap().unwrap();
    assert!(pending(&f.root).unwrap());
    session.verify_before_launch(|| Ok(())).unwrap();
    let entries = session.publication().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].logical, "music.iff");
    assert!(
        entries[1]
            .logical
            .starts_with("newaudio/jukeboxmusic/000001-")
    );
    assert!(entries[1].size > 54);
    assert_eq!(
        entries[1].sha256,
        catalog::digest(&fs::read(&entries[1].path).unwrap())
    );
    assert!(recover_ended(&f.root, &f.mods, &"f".repeat(48), || Ok(())).is_err());
    assert!(pending(&f.root).unwrap());
    drop(session);
    assert!(recover_ended(&f.root, &f.mods, &id, || Ok(())).unwrap());
    assert!(!pending(&f.root).unwrap());
    assert!(!f.mods.join("music.iff").exists());
    assert!(!recover_ended(&f.root, &f.mods, &id, || Ok(())).unwrap());
}
#[test]
fn original_music_mod_is_restored_exactly_after_custom_session() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    let original = crate::jukebox_package::build_music(&stock()).unwrap();
    fs::write(f.mods.join("music.iff"), &original).unwrap();
    let session = prepare_fixture(&f, &plan, &"e".repeat(48))
        .unwrap()
        .unwrap();
    assert_ne!(fs::read(f.mods.join("music.iff")).unwrap(), original);
    session.restore(|| Ok(())).unwrap();
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), original);
}
#[test]
fn existing_audio_collision_prevents_all_publication_without_overwriting() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    fs::create_dir_all(f.mods.join("newaudio/jukeboxmusic")).unwrap();
    let target = f.mods.join(catalog::audio_path(1));
    fs::write(&target, b"foreign existing audio mod").unwrap();
    assert!(prepare_fixture(&f, &plan, &"e".repeat(48)).is_err());
    assert_eq!(fs::read(target).unwrap(), b"foreign existing audio mod");
    assert!(!pending(&f.root).unwrap());
    assert!(!f.mods.join("music.iff").exists());
}
#[test]
fn interrupted_activation_rolls_back_using_independent_closed_not_cancel_authority() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, false);
    let original = crate::jukebox_package::build_music(&stock()).unwrap();
    fs::write(f.mods.join("music.iff"), &original).unwrap();
    let id = "e".repeat(48);
    let backup = f
        .mods
        .join(format!(".2k-launcher-jukebox-{id}-original.iff"));
    let result = prepare_with_stock(
        &f.root,
        &f.root,
        &f.mods,
        &id,
        &plan,
        &stock(),
        Checks {
            prepare: &|| {
                ensure!(
                    !backup.exists(),
                    "launch was cancelled after moving the original"
                );
                Ok(())
            },
            poll: &|| Ok(()),
            cleanup: &|| Ok(()),
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), original);
    assert!(!backup.exists());
    assert!(!pending(&f.root).unwrap());
}
#[test]
fn protected_game_blocks_prepare_and_recovery_without_touching_files() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    assert!(
        prepare_with_stock(
            &f.root,
            &f.root,
            &f.mods,
            &"e".repeat(48),
            &plan,
            &stock(),
            Checks {
                prepare: &|| anyhow::bail!("protected game is running"),
                poll: &|| Ok(()),
                cleanup: &|| Ok(())
            }
        )
        .is_err()
    );
    assert!(!pending(&f.root).unwrap());
    let id = "f".repeat(48);
    let session = prepare_fixture(&f, &plan, &id).unwrap().unwrap();
    let bytes = fs::read(f.mods.join("music.iff")).unwrap();
    assert!(
        recover_ended(&f.root, &f.mods, &id, || anyhow::bail!(
            "game remains running"
        ))
        .is_err()
    );
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), bytes);
    session.restore(|| Ok(())).unwrap();
}
#[test]
fn foreign_modified_published_song_holds_all_cleanup_and_original_backup() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    let original = crate::jukebox_package::build_music(&stock()).unwrap();
    fs::write(f.mods.join("music.iff"), original).unwrap();
    let session = prepare_fixture(&f, &plan, &"e".repeat(48))
        .unwrap()
        .unwrap();
    let entries = session.publication().unwrap();
    fs::write(&entries[1].path, b"foreign replacement").unwrap();
    let music = fs::read(f.mods.join("music.iff")).unwrap();
    assert!(session.restore(|| Ok(())).is_err());
    assert!(pending(&f.root).unwrap());
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), music);
    assert_eq!(fs::read(&entries[1].path).unwrap(), b"foreign replacement");
}
#[test]
fn changed_library_after_activation_blocks_steam_boundary_but_does_not_block_owned_restore() {
    let f = Fixture::new();
    let (mut service, plan) = selected(&f, true);
    let session = prepare_fixture(&f, &plan, &"e".repeat(48))
        .unwrap()
        .unwrap();
    service
        .rename_playlist(&"b".repeat(48), "Changed before Steam", &"f".repeat(48))
        .unwrap();
    assert!(session.verify_before_launch(|| Ok(())).is_err());
    session.restore(|| Ok(())).unwrap();
    assert!(!pending(&f.root).unwrap());
    assert_eq!(service.view().playlists[0].name, "Changed before Steam");
}
#[test]
fn independent_next_game_sessions_rearm_after_complete_restore() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    for id in ["e".repeat(48), "f".repeat(48)] {
        let session = prepare_fixture(&f, &plan, &id).unwrap().unwrap();
        session.verify_before_launch(|| Ok(())).unwrap();
        session.restore(|| Ok(())).unwrap();
        assert!(!pending(&f.root).unwrap());
    }
}
#[test]
fn loader_fields_are_fixed_bounded_and_forbid_old_freestyle_or_index_experiments() {
    let policy = AudioPolicy {
        main: format!("1{}", "0".repeat(70)),
        park: "0".repeat(54),
        custom: format!("1{}", "0".repeat(70)),
    };
    let config =
        append_loader_config(b"PIPE=owned\r\nSCOREBOARD_TEXT_TEST=1\r\n", Some(&policy)).unwrap();
    let text = String::from_utf8(config).unwrap();
    assert!(text.contains("AUDIO_SLOT_COUNT=71\r\n"));
    assert_eq!(text.matches("AUDIO_").count(), 4);
    assert!(!text.contains("AUDIO_INDEX_OVERRIDE"));
    assert!(!text.contains("FREESTYLE"));
    for prior in [
        "AUDIO_SLOT_COUNT=71\r\n",
        "AUDIO_INDEX_OVERRIDE=x\r\n",
        "FREESTYLE_PLAYLIST=992\r\n",
        "audio_allowed_main=x\r\n",
    ] {
        assert!(append_loader_config(prior.as_bytes(), Some(&policy)).is_err());
    }
    assert!(
        append_loader_config(
            format!("{}\r\n", "X".repeat(1900)).as_bytes(),
            Some(&policy)
        )
        .is_err()
    );
    let mut invalid = policy.clone();
    invalid.main = "0".repeat(71);
    assert!(append_loader_config(b"PIPE=owned\r\n", Some(&invalid)).is_err());
}
#[test]
fn loader_source_proves_exact_mask_parser_and_no_freestyle_dependency() {
    let source = include_str!("../../nba2k27-version-proxy/src/version_proxy.cpp");
    for proof in [
        "valid_audio_mask(main_mask,71)",
        "valid_audio_mask(park_mask,54)",
        "valid_audio_mask(custom_mask,71)",
        "std::strcmp(audio_slots,\"71\")!=0",
        "custom_mask[i]=='1'&&main_mask[i]!='1'",
        "kMaxProviderReadBytes = 1024u * 1024u",
    ] {
        assert!(
            source.contains(proof),
            "Native loader policy anchor changed: {proof}"
        );
    }
    let header = include_str!("../../nba2k27-version-proxy/src/audio_index_override.hpp");
    assert!(header.contains("slot>=1&&slot<=71"));
    assert!(header.contains("newaudio/jukeboxmusic/"));
}
#[test]
fn pending_receipt_blocks_even_default_plan_instead_of_hiding_older_session() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    let session = prepare_fixture(&f, &plan, &"e".repeat(48))
        .unwrap()
        .unwrap();
    let result = prepare_fixture(&f, &Plan::default(), &"f".repeat(48));
    assert!(result.is_err());
    assert!(pending(&f.root).unwrap());
    session.restore(|| Ok(())).unwrap();
}
#[test]
fn publication_cached_handle_must_be_released_before_ended_restore() {
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    let session = prepare_fixture(&f, &plan, &"e".repeat(48))
        .unwrap()
        .unwrap();
    let entry = session.publication().unwrap().remove(1);
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&entry.path)
        .unwrap();
    assert!(session.restore(|| Ok(())).is_err());
    assert!(pending(&f.root).unwrap());
    drop(handle);
    session.restore(|| Ok(())).unwrap();
    assert!(!pending(&f.root).unwrap());
}

fn prepare_known(
    f: &Fixture,
    plan: &Plan,
    session: &str,
    known: &crate::jukebox_prepared::Verified,
) -> Result<Option<Session>> {
    prepare_inner(
        &f.root,
        &f.root,
        &f.mods,
        session,
        plan,
        &stock(),
        Checks {
            prepare: &|| Ok(()),
            poll: &|| Ok(()),
            cleanup: &|| Ok(()),
        },
        known,
    )
}
fn fixture_proof(f: &Fixture) -> jukebox_v2_resources::StockProof {
    let seen = jukebox_v2_resources::Seen::read(
        &File::open(f.root.join("Data/Jukebox/library.json")).unwrap(),
    )
    .unwrap();
    jukebox_v2_resources::StockProof {
        image: seen.clone(),
        manifest: seen,
        resource: jukebox_v2_resources::Resource {
            archive: "0B".into(),
            offset: 0,
            size: 24,
        },
    }
}
fn library_audio(f: &Fixture) -> PathBuf {
    let mut files = fs::read_dir(f.root.join("Library/Jukebox/Audio"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    files.pop().unwrap()
}
fn owned_files(f: &Fixture) -> usize {
    fs::read_dir(f.mods.join("newaudio/jukeboxmusic"))
        .map(|d| d.count())
        .unwrap_or(0)
        + usize::from(f.mods.join("music.iff").exists())
}
#[test]
fn prepared_cache_reuses_only_validated_sources_and_rehashes_on_any_change() {
    use crate::jukebox_prepared::Cache;
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    // Cold: one full hash, then the in-launch pre-intent check reuses it.
    let cache = Cache::load(&f.root);
    assert!(cache.stock().is_none());
    let s = prepare_known(&f, &plan, &"1".repeat(48), &cache.verified)
        .unwrap()
        .unwrap();
    assert_eq!(cache.report(false, true).sources_hashed, 1);
    cache
        .save(&f.root, fixture_proof(&f), &"1".repeat(48))
        .unwrap();
    s.restore(|| Ok(())).unwrap();
    assert_eq!(owned_files(&f), 0);
    // Warm, unchanged playlist: no source is re-hashed before staging.
    let cache = Cache::load(&f.root);
    assert_eq!(cache.stock(), Some(&fixture_proof(&f)));
    let s = prepare_known(&f, &plan, &"2".repeat(48), &cache.verified)
        .unwrap()
        .unwrap();
    let warm = cache.report(true, true);
    assert_eq!((warm.sources_reused, warm.sources_hashed), (2, 0));
    s.verify_before_launch(|| Ok(())).unwrap();
    s.publication().unwrap();
    cache
        .save(&f.root, fixture_proof(&f), &"2".repeat(48))
        .unwrap();
    s.restore(|| Ok(())).unwrap();
    // Same bytes rewritten (new last-write time): identity no longer proven.
    let audio = library_audio(&f);
    let original = fs::read(&audio).unwrap();
    let written = fs::metadata(&audio).unwrap().modified().unwrap();
    {
        let file = fs::OpenOptions::new().write(true).open(&audio).unwrap();
        file.set_modified(written + std::time::Duration::from_secs(5))
            .unwrap();
    }
    let cache = Cache::load(&f.root);
    let s = prepare_known(&f, &plan, &"3".repeat(48), &cache.verified)
        .unwrap()
        .unwrap();
    assert_eq!(cache.report(true, true).sources_hashed, 1);
    cache
        .save(&f.root, fixture_proof(&f), &"3".repeat(48))
        .unwrap();
    s.restore(|| Ok(())).unwrap();
    // Tampered bytes with identity, size and time forged back: the cache may
    // skip the early check, but staging hashes every byte and refuses.
    let restamp = fs::metadata(&audio).unwrap().modified().unwrap();
    let mut tampered = original.clone();
    *tampered.last_mut().unwrap() ^= 0x55;
    {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().write(true).open(&audio).unwrap();
        file.write_all(&tampered).unwrap();
        file.set_modified(restamp).unwrap();
    }
    let cache = Cache::load(&f.root);
    let error = prepare_known(&f, &plan, &"4".repeat(48), &cache.verified)
        .err()
        .expect("tampered source must not publish");
    assert!(format!("{error:#}").contains("changed before staging"));
    assert_eq!(owned_files(&f), 0);
    assert!(!pending(&f.root).unwrap());
    fs::write(&audio, &original).unwrap();
}
#[test]
fn prepared_cache_ignores_foreign_invalid_or_linked_records() {
    use crate::jukebox_prepared::{Cache, clear};
    let f = Fixture::new();
    let (_, plan) = selected(&f, true);
    let cache = Cache::load(&f.root);
    let s = prepare_known(&f, &plan, &"5".repeat(48), &cache.verified)
        .unwrap()
        .unwrap();
    cache
        .save(&f.root, fixture_proof(&f), &"5".repeat(48))
        .unwrap();
    s.restore(|| Ok(())).unwrap();
    let record = f.root.join("Cache/Jukebox/Prepared/prepared-v1.json");
    let good = fs::read(&record).unwrap();
    assert!(Cache::load(&f.root).stock().is_some());
    let mut other_build: serde_json::Value = serde_json::from_slice(&good).unwrap();
    other_build["imageSha256"] = json!("0".repeat(64));
    let mut other_schema: serde_json::Value = serde_json::from_slice(&good).unwrap();
    other_schema["schema"] = json!("nba2k27-rust-jukebox-prepared-v0");
    let mut extra: serde_json::Value = serde_json::from_slice(&good).unwrap();
    extra["publishPath"] = json!("C:/Windows");
    let mut bad_hash: serde_json::Value = serde_json::from_slice(&good).unwrap();
    bad_hash["sources"][0]["sha256"] = json!("Z".repeat(64));
    for bytes in [
        b"not json".to_vec(),
        serde_json::to_vec(&other_build).unwrap(),
        serde_json::to_vec(&other_schema).unwrap(),
        serde_json::to_vec(&extra).unwrap(),
        serde_json::to_vec(&bad_hash).unwrap(),
    ] {
        fs::write(&record, bytes).unwrap();
        let cache = Cache::load(&f.root);
        assert!(cache.stock().is_none());
        let s = prepare_known(&f, &plan, &"6".repeat(48), &cache.verified)
            .unwrap()
            .unwrap();
        assert_eq!(
            cache.report(false, false).sources_reused,
            1,
            "only in-launch reuse"
        );
        s.restore(|| Ok(())).unwrap();
    }
    assert!(clear(&f.root).unwrap());
    assert!(!record.exists());
    assert!(
        library_audio(&f).exists(),
        "clearing never removes library audio"
    );
    assert!(!clear(&f.root).unwrap());
}
fn freestyle_policy(custom_slots: usize) -> AudioPolicy {
    let custom = format!("{}{}", "1".repeat(custom_slots), "0".repeat(71 - custom_slots));
    AudioPolicy {
        main: custom.clone(),
        park: "0".repeat(54),
        custom,
    }
}
#[test]
fn freestyle_selection_matches_wpf_imported_only_mask() {
    // 1..=58 imported songs: every slot, no randomness consumed.
    for n in [1, 3, 58] {
        let mask = freestyle_selection(&freestyle_policy(n), |_| panic!("no sample")).unwrap();
        assert_eq!(mask, freestyle_policy(n).custom);
    }
    // >58: exactly 58 distinct imported slots via Fisher-Yates.
    let policy = freestyle_policy(71);
    let mut calls = Vec::new();
    let mask = freestyle_selection(&policy, |bound| {
        calls.push(bound);
        Ok(bound - 1)
    })
    .unwrap();
    assert_eq!(calls, (2..=71).rev().collect::<Vec<_>>());
    assert_eq!(mask.len(), 71);
    assert_eq!(mask.bytes().filter(|&b| b == b'1').count(), 58);
    let random = freestyle_selection(&policy, os_random).unwrap();
    assert_eq!(random.bytes().filter(|&b| b == b'1').count(), 58);
    // Sparse custom slots stay inside custom.
    let mut sparse = freestyle_policy(0);
    sparse.custom = format!("{}{}", "01".repeat(35), "1");
    sparse.main = sparse.custom.clone();
    let mask = freestyle_selection(&sparse, |_| unreachable!()).unwrap();
    assert_eq!(mask, sparse.custom);
    assert!(freestyle_selection(&policy, |b| Ok(b)).is_err());
}
#[test]
fn freestyle_rejects_mixed_or_empty_playlists_with_wpf_message() {
    let mut mixed = freestyle_policy(3);
    mixed.main = format!("1111{}", "0".repeat(67));
    let error = freestyle_eligible(&mixed).unwrap_err().to_string();
    assert_eq!(error, FREESTYLE_MIXED);
    assert!(freestyle_eligible(&freestyle_policy(0)).is_err());
    assert!(freestyle_eligible(&freestyle_policy(71)).is_ok());
}
#[test]
fn freestyle_lines_follow_audio_fields_and_stay_bounded() {
    let policy = freestyle_policy(2);
    let base = append_loader_config(b"PIPE=owned\r\n", Some(&policy)).unwrap();
    let config = append_freestyle_with(&base, &policy, |_| unreachable!()).unwrap();
    let text = String::from_utf8(config).unwrap();
    assert!(text.starts_with(std::str::from_utf8(&base).unwrap()));
    assert!(text.ends_with(&format!(
        "AUDIO_CUSTOM_MAIN={0}\r\nFREESTYLE_PLAYLIST=992\r\nFREESTYLE_SELECTION={0}\r\n",
        policy.custom
    )));
    // Needs the prepared AUDIO fields, refuses a second Freestyle block.
    assert!(append_freestyle_with(b"PIPE=owned\r\n", &policy, |_| unreachable!()).is_err());
    assert!(append_freestyle_with(text.as_bytes(), &policy, |_| unreachable!()).is_err());
    let mut other = policy.clone();
    other.custom = freestyle_policy(1).custom;
    other.main = other.custom.clone();
    assert!(append_freestyle_with(&base, &other, |_| unreachable!()).is_err());
    let long = format!("{}\r\n", "X".repeat(1650));
    let near = append_loader_config(long.as_bytes(), Some(&policy)).unwrap();
    assert!(near.len() <= 2047);
    assert!(append_freestyle_with(&near, &policy, |_| unreachable!()).is_err());
}
#[test]
fn v2_proxy_source_uses_rust_owned_freestyle_profile() {
    let source = include_str!("../native/outfit-match-v2-proxy/version_proxy_v2.cpp");
    assert!(source.contains("#include \"freestyle_build_profile_v2.hpp\""));
    assert!(!source.contains("#include \"freestyle_build_profile.hpp\""));
    for proof in [
        "std::strcmp(custom_mask,main_mask)!=0",
        "if(count<1||count>58)return false;",
        "key!=0xFAB3211A",
    ] {
        assert!(source.contains(proof), "V2 Freestyle parser anchor changed: {proof}");
    }
    let header = include_str!("../native/freestyle_build_profile_v2.hpp");
    assert!(header.contains("timestamp==0x6AC53765 && image_size==0x35BDE000"));
    assert!(header.contains("{0x25C4A7,0xA31E7D0,0x677F930,"));
    assert!(header.contains("{0x48,0x8d,0x0d,0x22,0x23,0x0c,0x0a}"));
}
