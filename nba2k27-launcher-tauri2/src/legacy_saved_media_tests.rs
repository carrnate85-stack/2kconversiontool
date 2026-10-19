include!("jukebox_test_paths.rs");
use super::*;
use serde_json::json;
use std::sync::atomic::AtomicUsize;
static NEXT: AtomicUsize = AtomicUsize::new(0);
const PROFILE: &str = "0123456789abcdef";
const ID1: &str = "12345678-1234-1234-1234-123456789abc";
const ID2: &str = "abcdef12-1234-1234-1234-123456789abc";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = fixture_artifacts().join(format!(
            "legacy-media-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self) -> PathBuf {
        self.0.join("Source")
    }
    fn target(&self) -> PathBuf {
        self.0.join("Target")
    }
    fn catalog(&self) -> PathBuf {
        self.source()
            .join("Library/Music/archive-library")
            .join(PROFILE)
            .join("library.json")
    }
    fn bundle(&self) -> PathBuf {
        self.catalog()
            .parent()
            .unwrap()
            .join("playlists/one.2kplaylist")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("legacy-media-test-")
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn track(bank: u16, unique: u16, name: &str) -> catalog::Track {
    catalog::Track {
        collection: 1,
        bank,
        unique,
        name: name.into(),
        artist: "Fixture artist".into(),
        cover: "001_helicopter".into(),
        extra: Default::default(),
    }
}
fn stock() -> catalog::Music {
    catalog::Music {
        tracks: vec![
            track(1, 1, "Stock1"),
            track(79, 79, "New V2 stock79"),
            track(100, 100, "New V2 stock100"),
        ],
        extra: Default::default(),
    }
}
fn native() -> Vec<u8> {
    audio::build_opus(&[vec![0xFC, 0xAA], vec![0xFC, 0xBB]], 1500).unwrap()
}
fn seed(f: &Fixture, shared: bool, videos: bool) {
    fs::create_dir_all(f.bundle().parent().unwrap()).unwrap();
    let song = track(79, 200, "Saved custom song");
    let music = catalog::Music {
        tracks: vec![song.clone()],
        extra: Default::default(),
    };
    let (_, bytes) = package::export_segment(
        "First",
        &music,
        &catalog::Music {
            tracks: vec![],
            extra: Default::default(),
        },
        &BTreeMap::from([(song.key(), native())]),
        None,
    )
    .unwrap();
    fs::write(f.bundle(), bytes).unwrap();
    let mut lists = vec![
        json!({"id":"old-first","name":"First","trackKeys":[song.key()],"bundlePath":f.bundle(),"bundlePaths":[f.bundle()]}),
    ];
    if shared {
        lists.push(json!({"id":"old-shared","name":"Shared","trackKeys":[song.key()],"bundlePath":f.bundle(),"bundlePaths":[f.bundle()]}));
    }
    lists.push(json!({"id":"hidden","name":"Recovery","recoveryPlaylist":true,"trackKeys":["1:72:130"],"bundlePath":"C:/Missing/never-read.2kplaylist"}));
    fs::write(f.catalog(),serde_json::to_vec(&json!({"schema":"nba2k27-jukebox-library-v1","archiveStorageVersion":1,"music":music,"playlists":lists,"activePlaylistId":null,"lastPlaylistId":"old-first","unknownLegacyMetadata":{"preserve":true}})).unwrap()).unwrap();
    fs::create_dir_all(f.source().join("Data")).unwrap();
    fs::write(
        f.source().join("Data/settings.json"),
        b"original WPF settings retained byte-for-byte",
    )
    .unwrap();
    if videos {
        let root = f.source().join("Library/Videos/mp4");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(format!("{ID1}.mp4")), mp4(b"av01")).unwrap();
        fs::write(root.join(format!("{ID2}.mp4")), mp4(b"avc1")).unwrap();
        fs::write(f.source().join("Library/Videos/mp4-library.json"),serde_json::to_vec(&json!({"schema":1,"selected":ID1,"items":[{"id":ID1,"name":"Saved AV1"},{"id":ID2,"name":"Saved H264"}]})).unwrap()).unwrap();
    }
}
fn bx(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend(kind);
    b.extend(payload);
    b
}
fn mp4(codec: &[u8; 4]) -> Vec<u8> {
    let mut sample = vec![0; 78];
    sample[24..26].copy_from_slice(&1280u16.to_be_bytes());
    sample[26..28].copy_from_slice(&720u16.to_be_bytes());
    sample.extend(bx(
        if codec == b"av01" { b"av1C" } else { b"avcC" },
        vec![1; 8],
    ));
    let mut stsd = vec![0; 4];
    stsd.extend(1u32.to_be_bytes());
    stsd.extend(bx(codec, sample));
    let mut hdlr = vec![0; 12];
    hdlr[8..12].copy_from_slice(b"vide");
    let track = bx(
        b"trak",
        bx(
            b"mdia",
            [
                bx(b"hdlr", hdlr),
                bx(b"minf", bx(b"stbl", bx(b"stsd", stsd))),
            ]
            .concat(),
        ),
    );
    let mut mvhd = vec![0; 20];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&45000u32.to_be_bytes());
    [
        bx(
            b"ftyp",
            [b"isom".to_vec(), vec![0; 4], b"mp42".to_vec()].concat(),
        ),
        bx(b"moov", [bx(b"mvhd", mvhd), track].concat()),
        bx(b"mdat", vec![1; 64]),
    ]
    .concat()
}
#[test]
fn actual_v3_import_remaps_new_v2_stock_collision_without_decode_or_source_changes() {
    let f = Fixture::new();
    seed(&f, false, false);
    let before = fs::read(f.catalog()).unwrap();
    let package_before = fs::read(f.bundle()).unwrap();
    let cancel = AtomicBool::new(false);
    let mut plan = inspect(&f.source(), PROFILE, stock(), &cancel).unwrap();
    assert_eq!(plan.report().visible_playlists, 1);
    assert_eq!(plan.report().hidden_recovery_playlists, 1);
    assert_eq!(plan.report().opus_tracks, 1);
    let staged = plan.stage_new(&f.target(), &cancel, |_, _| Ok(())).unwrap();
    assert_eq!(staged.active_playlist_id, None);
    assert!(!staged.startup_video_enabled);
    {
        let service = Service::open(&f.target(), stock(), &"a".repeat(48)).unwrap();
        let key = &staged.track_keys["1:79:200"];
        assert_ne!(key, "1:79:200");
        assert_eq!(
            service
                .view()
                .music
                .tracks
                .iter()
                .find(|t| t.bank == 79)
                .unwrap(),
            &stock().tracks[1]
        );
        assert_eq!(service.source(key).unwrap().sha256, digest(&native()));
        assert_eq!(service.view().playlists[0].track_keys, vec![key.clone()]);
    }
    assert_eq!(fs::read(f.catalog()).unwrap(), before);
    assert_eq!(fs::read(f.bundle()).unwrap(), package_before);
    assert_eq!(
        fs::read(f.source().join("Data/settings.json")).unwrap(),
        b"original WPF settings retained byte-for-byte"
    );
}
#[test]
fn shared_song_has_one_new_identity_and_content_addressed_blob_across_playlists() {
    let f = Fixture::new();
    seed(&f, true, false);
    let cancel = AtomicBool::new(false);
    let mut plan = inspect(&f.source(), PROFILE, stock(), &cancel).unwrap();
    assert_eq!(plan.report().ordered_tracks, 2);
    assert_eq!(plan.report().unique_source_tracks, 1);
    assert_eq!(plan.report().unique_audio_blobs, 1);
    assert_eq!(
        plan.report().duplicate_audio_bytes_avoided,
        native().len() as u64
    );
    let staged = plan.stage_new(&f.target(), &cancel, |_, _| Ok(())).unwrap();
    assert_eq!(staged.track_keys.len(), 1);
    let service = Service::open(&f.target(), stock(), &"b".repeat(48)).unwrap();
    assert_eq!(service.view().audio.len(), 1);
    assert_eq!(service.view().playlists.len(), 2);
    assert_eq!(
        service.view().playlists[0].track_keys,
        service.view().playlists[1].track_keys
    );
}
#[test]
fn corrupt_package_or_outside_managed_reference_fails_before_destination_exists() {
    let f = Fixture::new();
    seed(&f, false, false);
    let cancel = AtomicBool::new(false);
    let mut bytes = fs::read(f.bundle()).unwrap();
    bytes[14] ^= 1;
    fs::write(f.bundle(), bytes).unwrap();
    assert!(inspect(&f.source(), PROFILE, stock(), &cancel).is_err());
    assert!(!f.target().exists());
    seed(&f, false, false);
    let mut doc: serde_json::Value =
        serde_json::from_slice(&fs::read(f.catalog()).unwrap()).unwrap();
    doc["playlists"][0]["bundlePath"] = json!(f.0.join("foreign.2kplaylist"));
    fs::write(f.catalog(), serde_json::to_vec(&doc).unwrap()).unwrap();
    assert!(inspect(&f.source(), PROFILE, stock(), &cancel).is_err());
}
#[test]
fn cancellation_existing_destination_and_source_nested_target_never_overwrite() {
    let f = Fixture::new();
    seed(&f, false, false);
    let cancel = AtomicBool::new(false);
    let mut plan = inspect(&f.source(), PROFILE, stock(), &cancel).unwrap();
    assert!(
        plan.stage_new(&f.source().join("DoNotCreate"), &cancel, |_, _| Ok(()))
            .is_err()
    );
    assert!(!f.source().join("DoNotCreate").exists());
    fs::create_dir(f.target()).unwrap();
    fs::write(f.target().join("foreign"), b"keep").unwrap();
    assert!(plan.stage_new(&f.target(), &cancel, |_, _| Ok(())).is_err());
    assert_eq!(fs::read(f.target().join("foreign")).unwrap(), b"keep");
    cancel.store(true, Ordering::Release);
    assert!(inspect(&f.source(), PROFILE, stock(), &cancel).is_err());
}
#[test]
fn retained_source_denies_concurrent_mutation_and_never_adopts_a_new_bundle() {
    let f = Fixture::new();
    seed(&f, false, false);
    let cancel = AtomicBool::new(false);
    let _plan = inspect(&f.source(), PROFILE, stock(), &cancel).unwrap();
    assert!(fs::OpenOptions::new().write(true).open(f.bundle()).is_err());
    assert!(fs::remove_file(f.bundle()).is_err());
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(f.catalog())
            .is_err()
    );
}
#[test]
fn saved_av1_and_h264_keep_ids_selection_names_and_resolve_only_fixed_paths() {
    let f = Fixture::new();
    seed(&f, false, true);
    let cancel = AtomicBool::new(false);
    let mut plan = inspect(&f.source(), PROFILE, stock(), &cancel).unwrap();
    assert_eq!(plan.report().videos, 2);
    assert_eq!(plan.report().video_codecs["AV1"], 1);
    assert_eq!(plan.videos().selected.as_deref(), Some(ID1));
    plan.stage_new(&f.target(), &cancel, |_, _| Ok(())).unwrap();
    {
        let lease = resolve_video(&f.target(), ID1, &cancel).unwrap();
        assert_eq!(lease.item.name, "Saved AV1");
        assert_eq!(lease.item.codecs, vec!["AV1"]);
        assert!(lease.path().ends_with(format!("mp4/{ID1}.mp4")));
        assert!(fs::remove_file(lease.path()).is_err());
    }
    let path = f.target().join("Data/Rust/startup-videos.json");
    let mut bad: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    bad["items"][0]["relativePath"] = json!("../outside.mp4");
    fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
    assert!(resolve_video(&f.target(), ID1, &cancel).is_err());
}
#[test]
fn incomplete_stage_is_not_promotable_and_is_never_silently_reused() {
    let f = Fixture::new();
    seed(&f, true, false);
    let cancel = AtomicBool::new(false);
    let mut plan = inspect(&f.source(), PROFILE, stock(), &cancel).unwrap();
    assert!(
        plan.stage_new(&f.target(), &cancel, |_, _| anyhow::bail!("stop fixture"))
            .is_err()
    );
    assert!(
        !f.target()
            .join("Data/Rust/legacy-media-import-complete.json")
            .exists()
    );
    assert!(plan.stage_new(&f.target(), &cancel, |_, _| Ok(())).is_err());
}
