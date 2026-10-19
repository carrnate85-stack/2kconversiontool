include!("jukebox_test_paths.rs");
use super::*;
use crate::jukebox_catalog::{Music, Track};
use serde_json::Map;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    portable: PathBuf,
    mods: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = fixture_artifacts().join(format!(
            "2k-jukebox-offline-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&root).unwrap();
        let portable = root.join("Portable");
        fs::create_dir(&portable).unwrap();
        fs::create_dir(portable.join("Data")).unwrap();
        let mods = root.join("Mods");
        fs::create_dir(&mods).unwrap();
        let source = root.join("song.native");
        fs::write(
            &source,
            jukebox_audio::wrap(&jukebox_audio::pcm_wave(&[0; 16]).unwrap()).unwrap(),
        )
        .unwrap();
        Self {
            root,
            portable,
            mods,
            source,
        }
    }
    fn music() -> Music {
        Music {
            tracks: vec![Track {
                artist: "Artist".into(),
                collection: 1,
                cover: "001_helicopter".into(),
                name: "Song".into(),
                bank: 72,
                unique: 130,
                extra: Map::new(),
            }],
            extra: Map::new(),
        }
    }
    fn create(&self) -> Store {
        let p = catalog::plan(&Self::music()).unwrap();
        let raw = fs::read(&self.source).unwrap();
        let sources = BTreeMap::from([(
            catalog::audio_path(72),
            NativeSource {
                path: self.source.clone(),
                size: raw.len(),
                sha256: catalog::digest(&raw),
            },
        )]);
        Store::create(
            &self.portable,
            &self.mods,
            &"a".repeat(48),
            &p,
            &sources,
            || Ok(()),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let resolved = self.root.canonicalize().unwrap();
        assert!(resolved.to_string_lossy().contains("2k-jukebox-offline-"));
        fs::remove_dir_all(resolved).unwrap();
    }
}
#[test]
fn session_publish_restore_preserves_exact_original_file_identity() {
    let f = Fixture::new();
    let original = jukebox_package::build_music(&Fixture::music()).unwrap();
    fs::write(f.mods.join("music.iff"), &original).unwrap();
    let id = identity(&open(&f.mods.join("music.iff"), false, false).unwrap()).unwrap();
    let s = f.create();
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), original);
    s.activate(|| Ok(())).unwrap();
    assert!(f.mods.join(catalog::audio_path(1)).exists());
    s.verify_current().unwrap();
    s.restore(|| Ok(())).unwrap();
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), original);
    assert_eq!(
        identity(&open(&f.mods.join("music.iff"), false, false).unwrap()).unwrap(),
        id
    );
    assert!(!f.mods.join(catalog::audio_path(1)).exists());
    assert!(!f.portable.join("Data").join(MARKER).exists());
}
#[test]
fn session_absent_original_and_crash_recovery() {
    let f = Fixture::new();
    let s = f.create();
    s.activate(|| Ok(())).unwrap();
    drop(s);
    let s = Store::recover(&f.portable, &f.mods, &"a".repeat(48))
        .unwrap()
        .unwrap();
    s.restore(|| Ok(())).unwrap();
    assert!(!f.mods.join("music.iff").exists());
    assert!(
        Store::recover(&f.portable, &f.mods, &"a".repeat(48))
            .unwrap()
            .is_none()
    );
}
#[test]
fn restore_staged_only_before_publication() {
    let f = Fixture::new();
    let s = f.create();
    s.restore(|| Ok(())).unwrap();
    assert!(!f.mods.join("music.iff").exists());
    assert!(!f.portable.join("Data").join(MARKER).exists());
}
#[test]
fn existing_audio_is_not_overwritten() {
    let f = Fixture::new();
    fs::create_dir_all(f.mods.join("newaudio/jukeboxmusic")).unwrap();
    let target = f.mods.join(catalog::audio_path(1));
    fs::write(&target, b"user mod").unwrap();
    let p = catalog::plan(&Fixture::music()).unwrap();
    let raw = fs::read(&f.source).unwrap();
    let sources = BTreeMap::from([(
        catalog::audio_path(72),
        NativeSource {
            path: f.source.clone(),
            size: raw.len(),
            sha256: catalog::digest(&raw),
        },
    )]);
    assert!(
        Store::create(&f.portable, &f.mods, &"a".repeat(48), &p, &sources, || Ok(
            ()
        ))
        .is_err()
    );
    assert_eq!(fs::read(target).unwrap(), b"user mod");
    assert!(!f.portable.join("Data").join(MARKER).exists());
}
#[test]
fn foreign_song_blocks_all_restore_deletions() {
    let f = Fixture::new();
    let s = f.create();
    s.activate(|| Ok(())).unwrap();
    let music = fs::read(f.mods.join("music.iff")).unwrap();
    fs::write(f.mods.join(catalog::audio_path(1)), b"foreign").unwrap();
    assert!(s.restore(|| Ok(())).is_err());
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), music);
    assert_eq!(
        fs::read(f.mods.join(catalog::audio_path(1))).unwrap(),
        b"foreign"
    );
    assert!(f.portable.join("Data").join(MARKER).exists());
}
#[test]
fn changed_file_identity_even_same_bytes_is_not_adopted() {
    let f = Fixture::new();
    let s = f.create();
    s.activate(|| Ok(())).unwrap();
    let target = f.mods.join(catalog::audio_path(1));
    let content = fs::read(&target).unwrap();
    fs::rename(&target, f.root.join("original-owned.native")).unwrap();
    fs::write(&target, &content).unwrap();
    assert!(s.restore(|| Ok(())).is_err());
    assert!(f.mods.join("music.iff").exists());
}
#[test]
fn protected_or_running_check_prevents_each_mutation() {
    let f = Fixture::new();
    let s = f.create();
    let original = fs::read(f.portable.join("Data").join(MARKER)).unwrap();
    assert!(s.activate(|| anyhow::bail!("Game is running")).is_err());
    assert!(!f.mods.join("music.iff").exists());
    assert_eq!(
        fs::read(f.portable.join("Data").join(MARKER)).unwrap(),
        original
    );
    s.activate(|| Ok(())).unwrap();
    assert!(
        s.restore(|| anyhow::bail!("Protected process present"))
            .is_err()
    );
    assert!(f.mods.join("music.iff").exists());
    s.restore(|| Ok(())).unwrap();
}
#[test]
fn interrupted_publication_after_original_move_is_recoverable() {
    let f = Fixture::new();
    let original = jukebox_package::build_music(&Fixture::music()).unwrap();
    fs::write(f.mods.join("music.iff"), &original).unwrap();
    let s = f.create();
    let calls = AtomicUsize::new(0);
    assert!(
        s.activate(|| {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            if n >= 2 {
                anyhow::bail!("Stopped after original backup move")
            }
            Ok(())
        })
        .is_err()
    );
    assert!(!f.mods.join("music.iff").exists());
    assert!(s.backup().exists());
    s.restore(|| Ok(())).unwrap();
    assert_eq!(fs::read(f.mods.join("music.iff")).unwrap(), original);
}
#[test]
fn receipt_lock_blocks_concurrent_launcher_and_foreign_replacement() {
    let f = Fixture::new();
    let s = f.create();
    let held = s.receipt(false, true).unwrap();
    assert!(fs::write(s.marker(), b"replacement").is_err());
    assert!(fs::remove_file(s.marker()).is_err());
    assert!(Store::recover(&f.portable, &f.mods, &"b".repeat(48)).is_err());
    drop(held);
    s.restore(|| Ok(())).unwrap();
}
#[test]
fn exact_handle_rename_no_replace_primitive() {
    let f = Fixture::new();
    let stage = f.root.join("stage.tmp");
    let target = f.root.join("target.tmp");
    let mut handle = fresh(&stage).unwrap();
    handle.write_all(b"owned").unwrap();
    handle.sync_all().unwrap();
    rename(&handle, &target).unwrap();
    drop(handle);
    let second = f.root.join("second.tmp");
    let handle = open(&target, false, true).unwrap();
    rename(&handle, &second).unwrap();
}
#[test]
fn missing_last_native_source_fails_before_any_receipt_or_stage() {
    let f = Fixture::new();
    let mut music = Fixture::music();
    let mut second = music.tracks[0].clone();
    second.bank = 73;
    second.unique = 131;
    music.tracks.push(second);
    let plan = catalog::plan(&music).unwrap();
    let bytes = fs::read(&f.source).unwrap();
    let source = NativeSource {
        path: f.source.clone(),
        size: bytes.len(),
        sha256: catalog::digest(&bytes),
    };
    let mut missing = source.clone();
    missing.path = f.root.join("missing.native");
    let sources = BTreeMap::from([
        (catalog::audio_path(72), source),
        (catalog::audio_path(73), missing),
    ]);
    assert!(
        Store::create(
            &f.portable,
            &f.mods,
            &"a".repeat(48),
            &plan,
            &sources,
            || Ok(())
        )
        .is_err()
    );
    assert!(!f.portable.join("Data").join(MARKER).exists());
    assert!(!f.mods.join("music.iff").exists());
    assert!(
        !f.mods
            .join(".2k-launcher-jukebox-".to_owned() + &"a".repeat(48) + "-0.tmp")
            .exists()
    );
}
#[test]
fn cover_route_files_publish_and_restore_with_created_folders() {
    let f = Fixture::new();
    let covers = crate::jukebox_cover_route::fixture_files();
    let texture = covers[1].0.clone();
    let p = catalog::plan(&Fixture::music()).unwrap();
    let raw = fs::read(&f.source).unwrap();
    let sources = BTreeMap::from([(
        catalog::audio_path(72),
        NativeSource {
            path: f.source.clone(),
            size: raw.len(),
            sha256: catalog::digest(&raw),
        },
    )]);
    let session = "a".repeat(48);
    let s = Store::create_with_covers(
        &f.portable,
        &f.mods,
        &session,
        &p,
        &sources,
        &covers,
        || Ok(()),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        s.cover_targets(),
        vec![crate::jukebox_cover_route::CONTAINER.to_string(), texture.clone()]
    );
    s.activate(|| Ok(())).unwrap();
    for (logical, bytes) in &covers {
        assert_eq!(&fs::read(f.mods.join(logical)).unwrap(), bytes);
    }
    drop(s);
    // Crash: recovery adopts the receipt and removes files AND new folders.
    let s = Store::recover(&f.portable, &f.mods, &session).unwrap().unwrap();
    s.restore(|| Ok(())).unwrap();
    assert!(!f.mods.join(crate::jukebox_cover_route::CONTAINER).exists());
    assert!(!f.mods.join(&texture).exists());
    assert!(!f.mods.join("shared").exists());
    assert!(!f.portable.join("Data").join(MARKER).exists());
}
#[test]
fn cover_route_keeps_a_users_existing_shared_folder_and_refuses_foreign_files() {
    let f = Fixture::new();
    fs::create_dir(f.mods.join("shared")).unwrap();
    fs::write(f.mods.join("shared/keep.txt"), b"user").unwrap();
    let covers = crate::jukebox_cover_route::fixture_files();
    let p = catalog::plan(&Fixture::music()).unwrap();
    let raw = fs::read(&f.source).unwrap();
    let sources = BTreeMap::from([(
        catalog::audio_path(72),
        NativeSource {
            path: f.source.clone(),
            size: raw.len(),
            sha256: catalog::digest(&raw),
        },
    )]);
    let session = "a".repeat(48);
    let s = Store::create_with_covers(
        &f.portable, &f.mods, &session, &p, &sources, &covers, || Ok(()), &Default::default(),
    )
    .unwrap();
    s.activate(|| Ok(())).unwrap();
    s.restore(|| Ok(())).unwrap();
    assert_eq!(fs::read(f.mods.join("shared/keep.txt")).unwrap(), b"user");
    assert!(!f.mods.join(&covers[1].0).parent().unwrap().exists());
    // A container that is not a valid cover container is refused before intent.
    let mut bad = covers.clone();
    bad[0].1 = b"not a zip".to_vec();
    assert!(Store::create_with_covers(
        &f.portable, &f.mods, &session, &p, &sources, &bad, || Ok(()), &Default::default(),
    )
    .is_err());
    assert!(!f.portable.join("Data").join(MARKER).exists());
}
#[test]
fn any_staging_failure_removes_its_receipt_and_stages_so_launch_is_not_blocked() {
    // Every closed() checkpoint inside create() fails once in turn: a
    // protected process starting mid-staging must never strand an intent-only
    // receipt whose uncommitted stages restore() can never prove or clear.
    fn files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut failures = 0;
    for fail_at in 2..64usize {
        let f = Fixture::new();
        let p = catalog::plan(&Fixture::music()).unwrap();
        let raw = fs::read(&f.source).unwrap();
        let sources = BTreeMap::from([(
            catalog::audio_path(72),
            NativeSource {
                path: f.source.clone(),
                size: raw.len(),
                sha256: catalog::digest(&raw),
            },
        )]);
        let calls = std::cell::Cell::new(0usize);
        let result = Store::create(&f.portable, &f.mods, &"a".repeat(48), &p, &sources, || {
            calls.set(calls.get() + 1);
            ensure!(calls.get() != fail_at, "Game started during staging");
            Ok(())
        });
        let Err(error) = result else {
            // fail_at is past the last checkpoint: create() completed.
            assert!(failures >= 3, "too few staging checkpoints exercised");
            return;
        };
        failures += 1;
        assert!(format!("{error:#}").contains("Game started during staging"));
        assert!(
            !f.portable.join("Data").join(MARKER).exists(),
            "receipt stranded at checkpoint {fail_at}"
        );
        let mut left = Vec::new();
        files(&f.mods, &mut left);
        assert!(left.is_empty(), "stage stranded at checkpoint {fail_at}: {left:?}");
        assert!(
            Store::recover(&f.portable, &f.mods, &"a".repeat(48))
                .unwrap()
                .is_none()
        );
    }
    panic!("create() never completed");
}
