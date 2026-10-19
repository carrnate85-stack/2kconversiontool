include!("jukebox_test_paths.rs");
use super::*;
use std::sync::atomic::AtomicUsize;
static NEXT: AtomicUsize = AtomicUsize::new(0);
const NONCE: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = fixture_artifacts().join(format!(
            "video-library-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self) -> PathBuf {
        self.0.join("source.mp4")
    }
    fn doc(&self) -> PathBuf {
        self.0.join("Data/Rust/startup-videos.json")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("video-library-test-")
        );
        fs::remove_dir_all(&self.0).unwrap();
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
fn fresh_library_is_empty_without_creating_a_catalog_then_av1_import_persists_id_and_friendly_name()
{
    let f = Fixture::new();
    let source = mp4(b"av01");
    fs::write(f.source(), &source).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    assert!(store.view().items.is_empty());
    assert!(!f.doc().exists());
    assert!(store.rows().is_empty());
    let mut phases = vec![];
    let item = store
        .import_mp4(
            &f.source(),
            "Friendly AV1",
            NONCE,
            &AtomicBool::new(false),
            |p| {
                phases.push(p.phase);
                Ok(())
            },
        )
        .unwrap();
    assert!(
        phases.contains(&Phase::Copying)
            && phases.contains(&Phase::Verifying)
            && phases.contains(&Phase::Committing)
    );
    assert_eq!(phases.last(), Some(&Phase::Done));
    assert!(valid_id(&item.id));
    assert_eq!(item.codecs, vec!["AV1"]);
    assert_eq!(store.view().selected, None);
    assert_eq!(store.rows()[0].name, "Friendly AV1");
    assert_eq!(store.rows()[0].id, item.id);
    assert_eq!(fs::read(&store.rows()[0].path).unwrap(), source);
    assert_eq!(fs::read(f.source()).unwrap(), source);
    store.verify_current().unwrap();
    drop(store);
    let store = Store::open(&f.0).unwrap();
    let lease = store.resolve(&item.id, &AtomicBool::new(false)).unwrap();
    assert_eq!(lease.item.sha256, item.sha256);
    assert_eq!(lease.item.name, "Friendly AV1");
    assert!(!f.0.join("Data/settings.json").exists());
}
#[test]
fn duplicate_id_and_foreign_disk_collision_never_overwrite_or_delete_existing_media() {
    let f = Fixture::new();
    fs::write(f.source(), mp4(b"avc1")).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    let cancel = AtomicBool::new(false);
    let item = store
        .import_mp4(&f.source(), "First", NONCE, &cancel, |_| Ok(()))
        .unwrap();
    let before = fs::read(store.rows()[0].path.clone()).unwrap();
    assert!(
        store
            .import_mp4(&f.source(), "Duplicate", NONCE, &cancel, |_| Ok(()))
            .is_err()
    );
    assert_eq!(fs::read(store.rows()[0].path.clone()).unwrap(), before);
    let other = "ffffffffffffffffffffffffffffffffffffffffffffffff";
    let path =
        f.0.join(format!("Library/Videos/mp4/{}.mp4", uuid(other).unwrap()));
    fs::write(&path, b"foreign existing bytes").unwrap();
    assert!(
        store
            .import_mp4(&f.source(), "Foreign", other, &cancel, |_| Ok(()))
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), b"foreign existing bytes");
    assert_eq!(store.view().items[0].id, item.id);
}
#[test]
fn cancel_and_bad_mp4_remove_only_the_new_owned_copy_and_preserve_source() {
    let f = Fixture::new();
    let source = mp4(b"avc1");
    fs::write(f.source(), &source).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    let cancel = AtomicBool::new(false);
    assert!(
        store
            .import_mp4(&f.source(), "Cancel", NONCE, &cancel, |p| {
                if p.phase == Phase::Verifying {
                    cancel.store(true, Ordering::Release);
                }
                Ok(())
            })
            .is_err()
    );
    assert!(
        !f.0.join(format!("Library/Videos/mp4/{}.mp4", uuid(NONCE).unwrap()))
            .exists()
    );
    assert!(!f.doc().exists());
    assert_eq!(fs::read(f.source()).unwrap(), source);
    fs::write(f.source(), b"fake mp4 source").unwrap();
    assert!(
        store
            .import_mp4(&f.source(), "Bad", NONCE, &AtomicBool::new(false), |_| Ok(
                ()
            ))
            .is_err()
    );
    assert_eq!(fs::read(f.source()).unwrap(), b"fake mp4 source");
}
#[test]
fn stale_store_and_corrupt_catalog_hold_without_overwriting_preferences() {
    let f = Fixture::new();
    fs::write(f.source(), mp4(b"avc1")).unwrap();
    let mut one = Store::open(&f.0).unwrap();
    let mut two = Store::open(&f.0).unwrap();
    let item = one
        .import_mp4(&f.source(), "First", NONCE, &AtomicBool::new(false), |_| {
            Ok(())
        })
        .unwrap();
    let exact = fs::read(f.doc()).unwrap();
    assert!(
        two.import_mp4(
            &f.source(),
            "Stale",
            "ffffffffffffffffffffffffffffffffffffffffffffffff",
            &AtomicBool::new(false),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(two.verify_current().is_err());
    assert_eq!(fs::read(f.doc()).unwrap(), exact);
    one.rename(
        &item.id,
        "Renamed",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    assert_eq!(one.rows()[0].name, "Renamed");
    drop(one);
    drop(two);
    fs::write(f.doc(), b"{corrupt}").unwrap();
    assert!(Store::open(&f.0).is_err());
    assert_eq!(fs::read(f.doc()).unwrap(), b"{corrupt}");
}
#[test]
fn all_four_catalog_commit_faults_recover_without_deleting_a_referenced_mp4() {
    for point in 1..=4 {
        let f = Fixture::new();
        let source = mp4(b"av01");
        fs::write(f.source(), &source).unwrap();
        let mut store = Store::open(&f.0).unwrap();
        let cancel = AtomicBool::new(false);
        assert!(
            store
                .import_inner(
                    &f.source(),
                    "Retained",
                    NONCE,
                    &cancel,
                    |_| Ok(()),
                    |doc, revision, bytes, nonce| doc.test_commit(revision, bytes, nonce, |p| {
                        ensure!(p != point, "fixture phase");
                        Ok(())
                    })
                )
                .is_err()
        );
        assert!(store.verify_current().is_err());
        let file =
            f.0.join(format!("Library/Videos/mp4/{}.mp4", uuid(NONCE).unwrap()));
        assert_eq!(fs::read(&file).unwrap(), source);
        drop(store);
        let recovered = Store::open(&f.0).unwrap();
        assert_eq!(recovered.view().items.len(), if point <= 2 { 0 } else { 1 });
        if point > 2 {
            recovered.resolve(&uuid(NONCE).unwrap(), &cancel).unwrap();
        }
        assert_eq!(fs::read(f.source()).unwrap(), source);
        assert!(!f.0.join("Data/Rust/startup-videos-update.json").exists());
    }
}
#[test]
fn done_notification_failure_cannot_delete_or_hide_an_already_committed_import() {
    let f = Fixture::new();
    fs::write(f.source(), mp4(b"avc1")).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    let item = store
        .import_mp4(&f.source(), "Saved", NONCE, &AtomicBool::new(false), |p| {
            ensure!(p.phase != Phase::Done, "UI already closed");
            Ok(())
        })
        .unwrap();
    assert_eq!(store.view().items[0].id, item.id);
    assert!(store.rows()[0].path.is_file());
    store.verify_current().unwrap();
}
#[test]
fn supplied_path_or_escape_is_never_a_selected_id_and_changed_source_bytes_hold_playback() {
    let f = Fixture::new();
    fs::write(f.source(), mp4(b"avc1")).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    let cancel = AtomicBool::new(false);
    let item = store
        .import_mp4(&f.source(), "Saved", NONCE, &cancel, |_| Ok(()))
        .unwrap();
    assert!(store.resolve("../source.mp4", &cancel).is_err());
    let target = store.rows()[0].path.clone();
    fs::write(target, b"changed native bytes").unwrap();
    assert!(store.resolve(&item.id, &cancel).is_err());
}

#[test]
fn network_device_relative_stream_and_parent_sources_are_rejected_before_open_or_mutation() {
    let f = Fixture::new();
    let mut store = Store::open(&f.0).unwrap();
    for path in [
        r"\\unreachable.example\share\video.mp4",
        r"\\?\UNC\unreachable.example\share\video.mp4",
        r"\\.\pipe\video.mp4",
        r"C:\sample.mp4:stream.mp4",
        r"C:\folder\..\video.mp4",
        r"relative\video.mp4",
    ] {
        let error = store
            .import_mp4(
                Path::new(path),
                "Unsupported source",
                NONCE,
                &AtomicBool::new(false),
                |_| anyhow::bail!("No source progress should have started"),
            )
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("path"), "{path}: {error:#}");
        assert!(store.view().items.is_empty());
        assert!(!f.doc().exists());
        assert!(!f.0.join("Library").exists());
    }
}
#[test]
fn rename_and_remove_edit_only_the_catalog_and_owned_copy_never_the_original() {
    let f = Fixture::new();
    let source = mp4(b"avc1");
    fs::write(f.source(), &source).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    let item = store
        .import_mp4(&f.source(), "Intro", NONCE, &AtomicBool::new(false), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(store.rows()[0].duration_ms, 45000);
    assert!(store.rename(&item.id, "", NONCE).is_err());
    assert!(store.rename(&item.id, " padded", NONCE).is_err());
    store.rename(&item.id, "Season intro – 2K27", N2).unwrap();
    drop(store);
    let mut store = Store::open(&f.0).unwrap();
    assert_eq!(store.rows()[0].name, "Season intro – 2K27");
    let copy = store.rows()[0].path.clone();
    assert!(
        store
            .remove("00000000-0000-4000-8000-000000000000", NONCE)
            .is_err()
    );
    let outcome = store.remove(&item.id, N3).unwrap();
    assert!(outcome.file_removed);
    assert_eq!(outcome.item.id, item.id);
    assert!(store.rows().is_empty() && !copy.exists());
    assert_eq!(fs::read(f.source()).unwrap(), source);
    drop(store);
    assert!(Store::open(&f.0).unwrap().view().items.is_empty());
}
#[test]
fn removing_an_in_use_copy_unlists_it_but_retains_the_file() {
    let f = Fixture::new();
    fs::write(f.source(), mp4(b"avc1")).unwrap();
    let mut store = Store::open(&f.0).unwrap();
    let item = store
        .import_mp4(&f.source(), "Busy", NONCE, &AtomicBool::new(false), |_| {
            Ok(())
        })
        .unwrap();
    let copy = store.rows()[0].path.clone();
    let held = fs::File::open(&copy).unwrap();
    let outcome = store.remove(&item.id, N3).unwrap();
    assert!(!outcome.file_removed);
    assert!(store.rows().is_empty() && copy.exists());
    drop(held);
}
const N2: &str = "1123456789abcdef0123456789abcdef0123456789abcdef";
const N3: &str = "2123456789abcdef0123456789abcdef0123456789abcdef";
