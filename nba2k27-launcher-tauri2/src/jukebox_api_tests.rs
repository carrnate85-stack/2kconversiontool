include!("jukebox_test_paths.rs");
#[test]
fn curated_download_import_bypasses_picker_preserves_selection_and_rejects_bad_payload() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:100:100"]}),
    )
    .unwrap();
    call(&api, "jukebox:set-active", json!({"playlistId":id})).unwrap();
    call(&api, "jukebox:refresh-bundle", json!({"playlistId":id})).unwrap();
    assert_eq!(wait(&api).job.unwrap().phase, "completed");
    let file = f.0.join("Library/Jukebox/Playlists/My playlist.2kplaylist");
    assert!(h.playlist.lock().unwrap().is_none());
    let added = api
        .import_downloaded(&file, &AtomicBool::new(false), |_| {})
        .unwrap();
    let v = wait(&api);
    assert_ne!(added, id);
    assert_eq!(v.state.active_playlist_id.as_deref(), Some(id.as_str()));
    assert_eq!(v.state.playlists.len(), 2);
    assert_eq!(v.state.playlists[1].track_keys, vec!["1:100:100"]);
    let bad = f.0.join("bad.2kplaylist");
    fs::write(&bad, b"not a playlist archive").unwrap();
    assert!(
        api.import_downloaded(&bad, &AtomicBool::new(false), |_| {})
            .is_err()
    );
    assert_eq!(api.view().unwrap().state.playlists.len(), 2);
}
use super::*;
use serde_json::Map;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fixture_artifacts();
        let path = root.join(format!(
            "2k-jukebox-api-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("2k-jukebox-api-")
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn stock() -> catalog::Music {
    catalog::Music {
        tracks: [1, 100]
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
#[derive(Default)]
struct TestHost {
    audio: Mutex<Option<Vec<PathBuf>>>,
    playlist: Mutex<Option<PathBuf>>,
    export: Mutex<Option<PathBuf>>,
    decode_calls: AtomicUsize,
    wait_decode: AtomicBool,
    decoder_started: AtomicBool,
    fail_decode: AtomicBool,
    pause_progress: AtomicBool,
    progress_paused: AtomicBool,
    native_only: AtomicBool,
    admission_error: AtomicBool,
    change_admission_on_choose: Mutex<Option<bool>>,
}
impl Host for TestHost {
    fn decode_admission(&self) -> Result<admission::Admission> {
        ensure!(
            !self.admission_error.load(Ordering::Acquire),
            "Trusted authority unavailable"
        );
        Ok(if self.native_only.load(Ordering::Acquire) {
            admission::Admission::NativeOnly
        } else {
            admission::Admission::DecoderAllowed
        })
    }
    fn choose_audio(&self, cancel: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
        cancelled(cancel)?;
        if let Some(native_only) = self.change_admission_on_choose.lock().unwrap().take() {
            self.native_only.store(native_only, Ordering::Release);
        }
        Ok(self.audio.lock().unwrap().take())
    }
    fn choose_playlist(&self, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        cancelled(cancel)?;
        Ok(self.playlist.lock().unwrap().take())
    }
    fn choose_export(&self, _: &str, cancel: &AtomicBool) -> Result<Option<PathBuf>> {
        cancelled(cancel)?;
        Ok(self.export.lock().unwrap().take())
    }
    fn decode(
        &self,
        _: &[u8],
        _: &str,
        cancel: Arc<AtomicBool>,
        progress: &mut dyn FnMut(u32) -> Result<()>,
    ) -> Result<Vec<u8>> {
        self.decode_calls.fetch_add(1, Ordering::SeqCst);
        self.decoder_started.store(true, Ordering::Release);
        let start = Instant::now();
        while self.wait_decode.load(Ordering::Acquire) {
            cancelled(&cancel)?;
            ensure!(
                start.elapsed() < Duration::from_secs(3),
                "test decoder timeout"
            );
            if self.pause_progress.load(Ordering::Acquire) {
                self.progress_paused.store(true, Ordering::Release);
            } else {
                progress(480)?;
            }
            thread::sleep(Duration::from_millis(5));
        }
        cancelled(&cancel)?;
        ensure!(
            !self.fail_decode.load(Ordering::Acquire),
            "unsupported test codec; original retained"
        );
        Ok(vec![0; 1920])
    }
}
fn open(f: &Fixture, h: Arc<TestHost>) -> Api {
    Api::open(&f.0, stock(), None, h).unwrap()
}
fn create(api: &Api) -> String {
    let v = api.view().unwrap();
    api.dispatch(
        "jukebox:create-playlist",
        json!({"expectedRevision":v.revision,"name":"My playlist"}),
    )
    .unwrap();
    api.view().unwrap().state.playlists[0].id.clone()
}
fn call(api: &Api, channel: &str, mut args: Value) -> Result<Value> {
    args["expectedRevision"] = json!(api.view()?.revision);
    api.dispatch(channel, args)
}
fn wait(api: &Api) -> View {
    let start = Instant::now();
    loop {
        let v = api.view().unwrap();
        if v.job.as_ref().is_some_and(|j| !j.active()) {
            while lock(&api.worker)
                .unwrap()
                .as_ref()
                .is_some_and(|t| !t.is_finished())
            {
                thread::sleep(Duration::from_millis(1));
            }
            return v;
        }
        assert!(start.elapsed() < Duration::from_secs(4));
        thread::sleep(Duration::from_millis(5));
    }
}
fn wait_decoder(h: &TestHost) {
    let start = Instant::now();
    while !h.decoder_started.load(Ordering::Acquire) {
        assert!(start.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(3));
    }
}

#[test]
fn command_view_matches_ui_and_preserves_v2_stock_identity() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let view = api.dispatch("jukebox:get", Value::Null).unwrap();
    assert_eq!(view["schema"], "nba2k27-jukebox-view-v1");
    assert_eq!(view["stockKeys"][1], "1:100:100");
    assert_eq!(view["capabilities"]["nativePlayback"], false);
    assert_eq!(view["capabilities"]["customPreview"], false);
    assert!(view["revision"].as_str().unwrap().len() == 64);
    assert_eq!(view["playback"]["phase"], "default");
}
#[test]
fn metadata_commands_use_cas_and_never_claim_game_activation() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let old = api.view().unwrap().revision;
    let id = create(&api);
    assert!(
        api.dispatch(
            "jukebox:rename-playlist",
            json!({"expectedRevision":old,"playlistId":id,"name":"stale"})
        )
        .is_err()
    );
    call(
        &api,
        "jukebox:rename-playlist",
        json!({"playlistId":id,"name":"Renamed"}),
    )
    .unwrap();
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:100:100","1:1:1"]}),
    )
    .unwrap();
    call(&api, "jukebox:set-active", json!({"playlistId":id})).unwrap();
    let v = api.view().unwrap();
    assert_eq!(v.state.playlists[0].name, "Renamed");
    assert_eq!(v.state.playlists[0].track_keys[0], "1:100:100");
    assert_eq!(v.playback.phase, "selected");
    assert!(call(&api, "jukebox:set-active", json!({})).is_err());
    call(&api, "jukebox:set-active", json!({"playlistId":null})).unwrap();
    assert_eq!(api.view().unwrap().playback.phase, "default");
    call(&api, "jukebox:delete-playlist", json!({"playlistId":id})).unwrap();
    assert!(api.view().unwrap().state.playlists.is_empty());
}
#[test]
fn actual_wav_import_runs_off_thread_and_persists_portably() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("A song.wav");
    let wave = audio::pcm_wave(&[0; 1920]).unwrap();
    fs::write(&path, &wave).unwrap();
    *h.audio.lock().unwrap() = Some(vec![path.clone()]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    let v = wait(&api);
    assert_eq!(v.job.unwrap().phase, "completed");
    assert_eq!(v.state.playlists[0].track_keys.len(), 1);
    assert_eq!(h.decode_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fs::read(&path).unwrap(), wave);
    assert!(f.0.join("Data/Jukebox/library.json").exists());
    api.shutdown(Duration::from_secs(1)).unwrap();
    drop(api);
    let reopened = open(&f, h);
    assert_eq!(
        reopened.view().unwrap().state.playlists[0].track_keys.len(),
        1
    );
}
#[test]
fn codec_decode_progress_remains_responsive_and_cancel_commits_nothing() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    h.wait_decode.store(true, Ordering::Release);
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("Encoded.mp3");
    fs::write(&path, b"encoded fixture").unwrap();
    *h.audio.lock().unwrap() = Some(vec![path.clone()]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    wait_decoder(&h);
    let start = Instant::now();
    let v = api.view().unwrap();
    assert!(start.elapsed() < Duration::from_millis(100));
    assert_eq!(v.job.as_ref().unwrap().phase, "decoding");
    assert!(call(&api, "jukebox:create-playlist", json!({"name":"blocked"})).is_err());
    let response = api
        .dispatch("jukebox:cancel-import", json!({"jobId":v.job.unwrap().id}))
        .unwrap();
    assert!(
        response["message"]
            .as_str()
            .unwrap()
            .contains("not yet confirmed")
    );
    let done = wait(&api);
    assert_eq!(done.job.unwrap().phase, "cancelled");
    assert!(done.state.playlists[0].track_keys.is_empty());
    assert_eq!(fs::read(&path).unwrap(), b"encoded fixture");
}
#[test]
fn successful_encoded_import_uses_guardian_result_and_truthful_library_status() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("Encoded.flac");
    fs::write(&path, b"fixture").unwrap();
    *h.audio.lock().unwrap() = Some(vec![path]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    let v = wait(&api);
    assert_eq!(v.job.unwrap().phase, "completed");
    assert_eq!(h.decode_calls.load(Ordering::SeqCst), 1);
    assert_eq!(v.playback.phase, "default");
    assert_eq!(v.state.audio.len(), 1);
}
#[test]
fn later_batch_failure_preserves_prior_completed_song_and_reports_partial_result() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    h.fail_decode.store(true, Ordering::Release);
    let api = open(&f, h.clone());
    let id = create(&api);
    let first = f.0.join("First.wav");
    fs::write(&first, audio::pcm_wave(&[0; 1920]).unwrap()).unwrap();
    let second = f.0.join("Bad.mp3");
    fs::write(&second, b"bad fixture").unwrap();
    *h.audio.lock().unwrap() = Some(vec![first, second]);
    let _ = call(&api, "jukebox:import-audio", json!({"playlistId":id}));
    let v = wait(&api);
    let j = v.job.unwrap();
    // WPF batch add keeps going past a bad file and reports "Added X of Y".
    assert_eq!(j.phase, "completed");
    assert!(
        j.message.contains("Added 1 of 2 songs. Bad.mp3:"),
        "{}",
        j.message
    );
    assert!(j.message.contains("unsupported test codec"));
    assert_eq!(v.state.playlists[0].track_keys.len(), 1);
}
#[test]
fn filename_names_follow_wpf_infer_from_filename() {
    use super::names_from_filename as n;
    assert_eq!(
        n("Drake - Hotline_Bling"),
        ("Drake".into(), "Hotline Bling".into())
    );
    assert_eq!(n("Artist \u{2013} Song"), ("Artist".into(), "Song".into()));
    assert_eq!(n("  my__song  "), ("Custom".into(), "my song".into()));
    assert_eq!(n("___"), ("Custom".into(), "Custom Song".into()));
    assert_eq!(n("-Song"), ("Custom".into(), "-Song".into()));
}
#[test]
fn picker_cancel_never_claims_a_save() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let before = api.view().unwrap().revision;
    let reply = call(&api, "jukebox:import-playlist", json!({})).unwrap();
    assert!(["started", "cancelled"].contains(&reply["kind"].as_str().unwrap()));
    let v = wait(&api);
    assert_eq!(v.job.unwrap().phase, "cancelled");
    assert_eq!(v.revision, before);
}
#[test]
fn shutdown_cancels_owned_decode_and_returns_bounded_without_new_mutations() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    h.wait_decode.store(true, Ordering::Release);
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("Slow.m4a");
    fs::write(&path, b"fixture").unwrap();
    *h.audio.lock().unwrap() = Some(vec![path]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    wait_decoder(&h);
    let start = Instant::now();
    api.shutdown(Duration::from_secs(1)).unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(call(&api, "jukebox:create-playlist", json!({"name":"late"})).is_err());
}
#[test]
fn immutable_preview_leases_support_only_bounded_token_ranges_and_revoke() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("Preview.wav");
    fs::write(&path, audio::pcm_wave(&[0; 1920]).unwrap()).unwrap();
    *h.audio.lock().unwrap() = Some(vec![path]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    let v = wait(&api);
    let key = &v.state.playlists[0].track_keys[0];
    assert!(call(&api, "jukebox:preview", json!({"trackKey":key})).is_err());
    let p = api.prepare_preview(key, &v.revision).unwrap();
    assert_eq!(p.mime_type, "audio/wav");
    let chunk = api.read_preview(&p.token, 0, 12).unwrap();
    assert_eq!(&chunk.bytes[..4], b"RIFF");
    assert_eq!(chunk.total, p.size);
    assert!(api.read_preview("../../secret", 0, 10).is_err());
    assert!(api.read_preview(&p.token, 0, 4 * 1024 * 1024 + 1).is_err());
    assert!(api.read_preview(&p.token, p.size + 1, 1).is_err());
    api.release_preview(&p.token).unwrap();
    api.release_preview(&p.token).unwrap();
    assert!(api.read_preview(&p.token, 0, 1).is_err());
}
#[test]
fn actual_playlist_export_and_import_roundtrip_without_overwrite() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:100:100","1:1:1"]}),
    )
    .unwrap();
    let path = f.0.join("Shared.2kplaylist");
    *h.export.lock().unwrap() = Some(path.clone());
    call(&api, "jukebox:export-playlist", json!({"playlistId":id})).unwrap();
    assert_eq!(wait(&api).job.unwrap().phase, "completed");
    let original = fs::read(&path).unwrap();
    *h.export.lock().unwrap() = Some(path.clone());
    let _ = call(&api, "jukebox:export-playlist", json!({"playlistId":id}));
    assert_eq!(wait(&api).job.unwrap().phase, "failed");
    assert_eq!(fs::read(&path).unwrap(), original);
    *h.playlist.lock().unwrap() = Some(path);
    call(&api, "jukebox:import-playlist", json!({})).unwrap();
    let v = wait(&api);
    assert_eq!(v.job.unwrap().phase, "completed");
    assert_eq!(v.state.playlists.len(), 2);
    assert_eq!(v.state.playlists[1].track_keys, vec!["1:100:100", "1:1:1"]);
    assert!(!fs::read_dir(&f.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".jukebox-export-")
    }));
}
#[test]
fn launch_seal_rejects_changed_library_before_any_game_path_access() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let seal = api.seal_launch().unwrap();
    create(&api);
    assert!(
        api.prepare_session(
            &seal,
            &f.0.join("absent-game"),
            &f.0.join("absent-mods"),
            &"a".repeat(48),
            || anyhow::bail!("must not access game")
        )
        .err()
        .unwrap()
        .to_string()
        .contains("changed")
    );
}
#[test]
fn source_and_export_refuse_linked_paths_and_preserve_foreign_files() {
    let f = Fixture::new();
    let original = f.0.join("Original.wav");
    fs::write(&original, b"foreign").unwrap();
    let link = f.0.join("Link.wav");
    match fs::hard_link(&original, &link) {
        Ok(()) => assert!(Source::open(&link, 100).is_err()),
        Err(error) if matches!(error.raw_os_error(), Some(5 | 1314)) => eprintln!(
            "Hard-link fixture unavailable under this Windows sandbox; the independent owned-file no-overwrite test still runs."
        ),
        Err(error) => panic!("Unexpected hard-link fixture error: {error}"),
    }
    assert_eq!(fs::read(&original).unwrap(), b"foreign");
    let target = f.0.join("Foreign.2kplaylist");
    fs::write(&target, b"foreign").unwrap();
    assert!(Export::new(&target, &"a".repeat(48)).is_err());
    assert_eq!(fs::read(target).unwrap(), b"foreign");
}
#[test]
fn export_race_preserves_foreign_destination_and_removes_only_owned_stage() {
    let f = Fixture::new();
    let target = f.0.join("Race.2kplaylist");
    let id = "f".repeat(48);
    let mut output = Export::new(&target, &id).unwrap();
    output.file.write_all(b"our staged playlist").unwrap();
    fs::write(&target, b"foreign race winner").unwrap();
    assert!(output.publish(&target).is_err());
    drop(output);
    assert_eq!(fs::read(&target).unwrap(), b"foreign race winner");
    assert!(!f.0.join(format!(".jukebox-export-{id}.tmp")).exists());
}
#[test]
fn retained_source_handle_denies_concurrent_write_and_delete() {
    let f = Fixture::new();
    let path = f.0.join("Source.wav");
    fs::write(&path, b"original").unwrap();
    let source = Source::open(&path, 100).unwrap();
    assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::remove_file(&path).is_err());
    source.verify().unwrap();
    drop(source);
    assert_eq!(fs::read(&path).unwrap(), b"original");
}
#[test]
fn malformed_package_and_unknown_commands_cannot_mutate_metadata() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let before = api.view().unwrap().revision;
    let bad = f.0.join("Invalid.2kplaylist");
    fs::write(&bad, b"not-a-valid-playlist-container").unwrap();
    *h.playlist.lock().unwrap() = Some(bad);
    let _ = call(&api, "jukebox:import-playlist", json!({}));
    let v = wait(&api);
    assert_eq!(v.job.unwrap().phase, "failed");
    assert_eq!(v.revision, before);
    assert!(v.state.playlists.is_empty());
    assert!(call(&api, "jukebox:write-arbitrary-file", json!({})).is_err());
    assert!(
        call(
            &api,
            "jukebox:create-playlist",
            json!({"name":"Valid","unrecognizedPath":"C:/Windows"})
        )
        .is_err()
    );
}
#[test]
fn foreign_job_and_saving_boundary_cannot_acknowledge_cancellation() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    h.wait_decode.store(true, Ordering::Release);
    h.pause_progress.store(true, Ordering::Release);
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("Slow.mp3");
    fs::write(&path, b"encoded fixture").unwrap();
    *h.audio.lock().unwrap() = Some(vec![path]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    wait_decoder(&h);
    let start = Instant::now();
    while !h.progress_paused.load(Ordering::Acquire) {
        assert!(start.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        api.dispatch("jukebox:cancel-import", json!({"jobId":"foreign"}))
            .is_err()
    );
    let actual = api.view().unwrap().job.unwrap().id;
    let cancel = lock(&api.shared.status).unwrap().cancel.clone().unwrap();
    api.shared
        .commit_boundary(&cancel, "Atomic fixture commit")
        .unwrap();
    assert!(
        api.dispatch("jukebox:cancel-import", json!({"jobId":actual}))
            .is_err()
    );
    api.shutdown(Duration::from_secs(1)).unwrap();
    assert!(lock(&api.worker).unwrap().is_none());
}
#[test]
fn shutdown_waits_for_terminal_worker_to_release_retained_directory_handles() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let pins = Parents::pin(&f.0).unwrap();
    let released = Arc::new(AtomicBool::new(false));
    let flag = released.clone();
    *lock(&api.worker).unwrap() = Some(thread::spawn(move || {
        thread::sleep(Duration::from_millis(35));
        drop(pins);
        flag.store(true, Ordering::Release);
    }));
    // The status is already idle, but the owned thread still retains handles.
    assert!(!lock(&api.shared.status).unwrap().busy);
    api.shutdown(Duration::from_secs(1)).unwrap();
    assert!(released.load(Ordering::Acquire));
    assert!(lock(&api.worker).unwrap().is_none());
}
#[test]
fn shutdown_budget_never_detaches_an_unfinished_owned_worker() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let release = Arc::new(AtomicBool::new(false));
    let flag = release.clone();
    *lock(&api.worker).unwrap() = Some(thread::spawn(move || {
        while !flag.load(Ordering::Acquire) {
            thread::sleep(Duration::from_millis(1));
        }
    }));
    assert!(api.shutdown(Duration::from_millis(10)).is_err());
    assert!(lock(&api.worker).unwrap().is_some());
    release.store(true, Ordering::Release);
    api.shutdown(Duration::from_secs(1)).unwrap();
    assert!(lock(&api.worker).unwrap().is_none());
}
#[test]
fn job_wire_shape_omits_optional_values_instead_of_invalid_nulls() {
    let value = serde_json::to_value(Job {
        id: "test-job".into(),
        kind: "audio-import".into(),
        phase: "choosing".into(),
        message: "Native picker".into(),
        done: None,
        total: None,
        frames: None,
        cancellable: true,
        error: None,
    })
    .unwrap();
    for key in ["done", "total", "frames", "error"] {
        assert!(
            value.get(key).is_none(),
            "{key} must be omitted for the typed frontend contract"
        );
    }
    assert_eq!(value["cancellable"], true);
}
#[test]
fn invalid_next_play_selection_is_rejected_by_backend_not_only_ui() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let id = create(&api);
    let before = api.view().unwrap().revision;
    assert!(call(&api, "jukebox:set-active", json!({"playlistId":id})).is_err());
    assert_eq!(api.view().unwrap().revision, before);
    assert!(api.view().unwrap().state.active_playlist_id.is_none());
}
#[test]
fn authoritative_active_playlist_stays_separate_from_saved_next_play_selection() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let id = create(&api);
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:1:1"]}),
    )
    .unwrap();
    call(&api, "jukebox:set-active", json!({"playlistId":id})).unwrap();
    let game_playlist = "e".repeat(48);
    let session = "f".repeat(48);
    let native = json!({"phase":"game-running","session":session,"nativeProvider":{"schema":"nba2k27-native-provider-session-v1","session":session,"jukebox":{"phase":"active","message":"Native playlist files verified; listening verification pending.","playlistId":game_playlist,"gameplayVerified":false}}});
    api.sync_native_status(&native, false).unwrap();
    let view = api.view().unwrap();
    assert_eq!(
        view.playback.playlist_id.as_deref(),
        Some(game_playlist.as_str())
    );
    assert_eq!(view.state.active_playlist_id.as_deref(), Some(id.as_str()));
    assert!(!view.playback.gameplay_verified);
    assert!(!view.capabilities.native_playback);
    let sequence = view.sequence;
    api.sync_native_status(&native, false).unwrap();
    assert_eq!(api.view().unwrap().sequence, sequence);
    api.sync_native_status(
        &json!({"phase":"closed","session":session,"nativeProvider":native["nativeProvider"]}),
        true,
    )
    .unwrap();
    let view = api.view().unwrap();
    assert_eq!(view.playback.phase, "selected");
    assert_eq!(view.playback.playlist_id, Some(id));
    assert!(view.capabilities.native_playback);
}
#[test]
fn mismatched_provider_status_cannot_claim_active_playback_or_adopt_paths() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let native = json!({"phase":"game-running","session":"a".repeat(48),"nativeProvider":{"schema":"nba2k27-native-provider-session-v1","session":"b".repeat(48),"jukebox":{"phase":"active","message":"foreign file C:/untrusted.json","playlistId":"c".repeat(48),"path":"C:/untrusted.json"}}});
    api.sync_native_status(&native, false).unwrap();
    let view = api.view().unwrap();
    assert_eq!(view.playback.phase, "held");
    assert!(!view.playback.message.contains("untrusted.json"));
    assert!(view.playback.session.is_none());
    api.sync_native_status(&json!({"phase":"idle","nativeProvider":null}), false)
        .unwrap();
    assert_eq!(api.view().unwrap().playback.phase, "default");
}
#[test]
fn running_default_soundtrack_is_not_confused_with_a_different_next_play_choice() {
    let f = Fixture::new();
    let api = open(&f, Arc::new(TestHost::default()));
    let id = create(&api);
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:1:1"]}),
    )
    .unwrap();
    let session = "e".repeat(48);
    let native = json!({"phase":"game-running","session":session,"nativeProvider":{"schema":"nba2k27-native-provider-session-v1","session":session,"jukebox":{"phase":"default","message":"This game uses its original soundtrack.","gameplayVerified":false}}});
    api.sync_native_status(&native, true).unwrap();
    call(&api, "jukebox:set-active", json!({"playlistId":id})).unwrap();
    let view = api.view().unwrap();
    assert_eq!(view.playback.phase, "default");
    assert_eq!(view.playback.session, Some(session));
    assert_eq!(view.state.active_playlist_id, Some(id));
}
#[test]
fn another_service_disk_edit_causes_cas_failure_not_silent_lost_update() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    h.wait_decode.store(true, Ordering::Release);
    let api = open(&f, h.clone());
    let id = create(&api);
    let path = f.0.join("Waiting.mp3");
    fs::write(&path, b"fixture").unwrap();
    *h.audio.lock().unwrap() = Some(vec![path]);
    call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    wait_decoder(&h);
    let mut other = Service::open(&f.0, stock(), &"f".repeat(48)).unwrap();
    other
        .rename_playlist(&id, "External retained edit", &"e".repeat(48))
        .unwrap();
    h.wait_decode.store(false, Ordering::Release);
    let done = wait(&api);
    assert_eq!(done.job.unwrap().phase, "failed");
    assert!(done.state.playlists[0].track_keys.is_empty());
    drop(other);
    drop(api);
    let reloaded = open(&f, h);
    assert_eq!(
        reloaded.view().unwrap().state.playlists[0].name,
        "External retained edit"
    );
}

#[test]
fn active_or_unknown_game_admission_preserves_native_pcm_opus_and_metadata_without_child() {
    for unavailable in [false, true] {
        let f = Fixture::new();
        let h = Arc::new(TestHost::default());
        h.native_only.store(true, Ordering::Release);
        h.admission_error.store(unavailable, Ordering::Release);
        let api = open(&f, h.clone());
        let id = create(&api);
        let pcm = f.0.join("Native.wav");
        let opus = f.0.join("Native.opus");
        fs::write(&pcm, audio::pcm_wave(&[0; 1920]).unwrap()).unwrap();
        fs::write(&opus, audio::build_opus(&[vec![0x9c, 1, 2]], 960).unwrap()).unwrap();
        *h.audio.lock().unwrap() = Some(vec![pcm, opus]);
        call(&api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
        let done = wait(&api);
        assert_eq!(done.job.unwrap().phase, "completed");
        assert_eq!(done.state.playlists[0].track_keys.len(), 2);
        assert_eq!(h.decode_calls.load(Ordering::Acquire), 0);
        call(
            &api,
            "jukebox:rename-playlist",
            json!({"playlistId":id,"name":"Next Play choice"}),
        )
        .unwrap();
        call(&api, "jukebox:set-active", json!({"playlistId":id})).unwrap();
        assert_eq!(api.view().unwrap().playback.phase, "selected");
        let key = api.view().unwrap().state.playlists[0].track_keys[0].clone();
        api.prepare_preview(&key, &api.view().unwrap().revision)
            .unwrap();
    }
}

#[test]
fn native_only_jobs_never_spawn_decoder_even_if_game_ends_while_picker_is_open() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    h.native_only.store(true, Ordering::Release);
    *h.change_admission_on_choose.lock().unwrap() = Some(false);
    let api = open(&f, h.clone());
    let id = create(&api);
    let source = f.0.join("Encoded.mp3");
    fs::write(&source, b"encoded unchanged source").unwrap();
    *h.audio.lock().unwrap() = Some(vec![source.clone()]);
    let _ = call(&api, "jukebox:import-audio", json!({"playlistId":id}));
    let done = wait(&api);
    let job = done.job.unwrap();
    assert_eq!(job.phase, "failed");
    assert!(job.error.unwrap().contains("Close NBA 2K27"));
    assert_eq!(h.decode_calls.load(Ordering::Acquire), 0);
    assert!(done.state.audio.is_empty());
    assert_eq!(fs::read(source).unwrap(), b"encoded unchanged source");
}

#[test]
fn fresh_active_game_before_conversion_refuses_old_admission_without_changing_library() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    *h.change_admission_on_choose.lock().unwrap() = Some(true);
    let api = open(&f, h.clone());
    let id = create(&api);
    let source = f.0.join("Encoded.mp3");
    fs::write(&source, b"encoded unchanged source").unwrap();
    *h.audio.lock().unwrap() = Some(vec![source]);
    let _ = call(&api, "jukebox:import-audio", json!({"playlistId":id}));
    let done = wait(&api);
    assert_eq!(done.job.unwrap().phase, "failed");
    assert_eq!(h.decode_calls.load(Ordering::Acquire), 0);
    assert!(done.state.audio.is_empty());
}
fn import_two(f: &Fixture, h: &Arc<TestHost>, api: &Api, id: &str) -> Vec<String> {
    let mut paths = vec![];
    for (name, fill) in [("Keep.wav", 0u8), ("Lost.wav", 7u8)] {
        let path = f.0.join(name);
        fs::write(&path, audio::pcm_wave(&[fill; 1920]).unwrap()).unwrap();
        paths.push(path);
    }
    *h.audio.lock().unwrap() = Some(paths);
    call(api, "jukebox:import-audio", json!({"playlistId":id})).unwrap();
    assert_eq!(wait(api).job.unwrap().phase, "completed");
    api.view().unwrap().state.playlists[0].track_keys.clone()
}
#[test]
fn repair_missing_removes_only_confirmed_absent_custom_songs() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    let keys = import_two(&f, &h, &api, &id);
    let none = api.dispatch("jukebox:find-missing", json!({})).unwrap();
    assert!(none["missing"].as_array().unwrap().is_empty());
    let lost = api.view().unwrap().state.audio[&keys[1]]
        .native
        .sha256
        .clone();
    fs::remove_file(f.0.join(format!("Library/Jukebox/Audio/{lost}.wav"))).unwrap();
    let found = api.dispatch("jukebox:find-missing", json!({})).unwrap();
    assert_eq!(found["missing"].as_array().unwrap().len(), 1);
    assert_eq!(found["missing"][0]["key"], keys[1]);
    assert_eq!(found["missing"][0]["title"], "Lost");
    // A song whose audio is still present cannot be dropped through repair.
    assert!(
        call(
            &api,
            "jukebox:repair-missing",
            json!({"trackKeys":[keys[0].clone()]})
        )
        .is_err()
    );
    call(
        &api,
        "jukebox:repair-missing",
        json!({"trackKeys":[keys[1].clone()]}),
    )
    .unwrap();
    let v = api.view().unwrap();
    assert_eq!(v.state.playlists[0].track_keys, vec![keys[0].clone()]);
    assert!(!v.state.audio.contains_key(&keys[1]));
    assert!(v.state.music.tracks.iter().all(|t| t.key() != keys[1]));
    let found = api.dispatch("jukebox:find-missing", json!({})).unwrap();
    assert!(found["missing"].as_array().unwrap().is_empty());
}
#[test]
fn repair_clears_a_play_choice_left_without_songs() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    let keys = import_two(&f, &h, &api, &id);
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":[keys[1].clone()]}),
    )
    .unwrap();
    call(&api, "jukebox:set-active", json!({"playlistId":id})).unwrap();
    let lost = api.view().unwrap().state.audio[&keys[1]]
        .native
        .sha256
        .clone();
    fs::remove_file(f.0.join(format!("Library/Jukebox/Audio/{lost}.wav"))).unwrap();
    call(
        &api,
        "jukebox:repair-missing",
        json!({"trackKeys":[keys[1].clone()]}),
    )
    .unwrap();
    let v = api.view().unwrap();
    assert!(v.state.playlists[0].track_keys.is_empty());
    assert_eq!(v.state.active_playlist_id, None);
    assert_eq!(v.playback.phase, "default");
}
#[test]
fn refresh_bundle_writes_and_replaces_only_the_owned_share_file() {
    let f = Fixture::new();
    let h = Arc::new(TestHost::default());
    let api = open(&f, h.clone());
    let id = create(&api);
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:100:100"]}),
    )
    .unwrap();
    call(&api, "jukebox:refresh-bundle", json!({"playlistId":id})).unwrap();
    let v = wait(&api);
    assert_eq!(v.job.unwrap().phase, "completed");
    let bundle = f.0.join("Library/Jukebox/Playlists/My playlist.2kplaylist");
    let first = fs::read(&bundle).unwrap();
    call(
        &api,
        "jukebox:set-order",
        json!({"playlistId":id,"trackKeys":["1:100:100","1:1:1"]}),
    )
    .unwrap();
    call(&api, "jukebox:refresh-bundle", json!({"playlistId":id})).unwrap();
    assert_eq!(wait(&api).job.unwrap().phase, "completed");
    let second = fs::read(&bundle).unwrap();
    assert_ne!(first, second);
    // The refreshed bundle is a normal share file that imports cleanly.
    *h.playlist.lock().unwrap() = Some(bundle.clone());
    call(&api, "jukebox:import-playlist", json!({})).unwrap();
    let v = wait(&api);
    assert_eq!(v.state.playlists[1].track_keys, vec!["1:100:100", "1:1:1"]);
    assert!(!fs::read_dir(bundle.parent().unwrap()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".jukebox-export-")
    }));
    // A non-file at the bundle path is never replaced.
    fs::remove_file(&bundle).unwrap();
    fs::create_dir(&bundle).unwrap();
    let _ = call(&api, "jukebox:refresh-bundle", json!({"playlistId":id}));
    assert_eq!(wait(&api).job.unwrap().phase, "failed");
    assert!(bundle.is_dir());
    assert_eq!(bundle_name("a/b:c. "), "abc.2kplaylist");
    assert_eq!(bundle_name("..."), "Playlist.2kplaylist");
}
