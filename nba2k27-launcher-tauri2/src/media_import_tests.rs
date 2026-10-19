use super::*;
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    source: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "media-import-fixture-{}",
            crate::backend::unique_id()
        ));
        let root = base.join("Portable");
        let source = base.join("Inputs");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir(&source).unwrap();
        Self { base, root, source }
    }
    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.source.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
    fn no_stages(&self) {
        let path = self.root.join("Cache/Imports");
        if path.exists() {
            assert_eq!(fs::read_dir(path).unwrap().count(), 0);
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.base.parent(), Some(std::env::temp_dir().as_path()));
        assert!(
            self.base
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("media-import-fixture-")
        );
        fs::remove_dir_all(&self.base).unwrap();
    }
}
// Structural ISO-BMFF fixture with real sample-description layout. The service
// expressly does not call this a decoded/playback-verified video. A separate
// opt-in test below imports the user's known real MP4 into fixture storage.
fn bx(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend(kind);
    b.extend(payload);
    b
}
fn mp4() -> Vec<u8> {
    let mut sample = vec![0; 78];
    sample[24..26].copy_from_slice(&16u16.to_be_bytes());
    sample[26..28].copy_from_slice(&16u16.to_be_bytes());
    sample.extend(bx(
        b"avcC",
        vec![1, 66, 0, 10, 255, 225, 0, 1, 103, 1, 0, 1, 104],
    ));
    let mut stsd = vec![0; 4];
    stsd.extend(1u32.to_be_bytes());
    stsd.extend(bx(b"avc1", sample));
    let mut hdlr = vec![0; 12];
    hdlr[8..].copy_from_slice(b"vide");
    let mut mdia = bx(b"hdlr", hdlr);
    mdia.extend(bx(b"minf", bx(b"stbl", bx(b"stsd", stsd))));
    let track = bx(b"trak", bx(b"mdia", mdia));
    let mut mvhd = vec![0; 20];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&1000u32.to_be_bytes());
    let mut moov = bx(b"mvhd", mvhd);
    moov.extend(track);
    let mut b = bx(
        b"ftyp",
        [b"isom".to_vec(), vec![0; 4], b"mp42".to_vec()].concat(),
    );
    b.extend(bx(b"moov", moov));
    b.extend(bx(b"mdat", vec![0, 0, 0, 2, 0x65, 0x88]));
    b
}
#[test]
fn verified_audio_import_preserves_filename_data_library_and_source() {
    let f = Fixture::new();
    fs::create_dir_all(f.root.join("Data")).unwrap();
    fs::write(f.root.join("Data/keep.json"), b"existing settings").unwrap();
    fs::create_dir_all(f.root.join("Library/Music")).unwrap();
    fs::write(f.root.join("Library/Music/old.mp3"), b"old media").unwrap();
    let bytes = vec![42; BLOCK * 2 + 37];
    let source = f.file("My Song.MP3", &bytes);
    let result = import(&f.root, &source, "Music").unwrap();
    assert_eq!(result.name, "My Song.MP3");
    assert_eq!(result.bytes, bytes.len() as u64);
    assert_eq!(result.sha256, format!("{:X}", Sha256::digest(&bytes)));
    assert!(result.video.is_none());
    assert_eq!(fs::read(result.path).unwrap(), bytes);
    assert_eq!(fs::read(source).unwrap(), bytes);
    assert_eq!(
        fs::read(f.root.join("Data/keep.json")).unwrap(),
        b"existing settings"
    );
    assert_eq!(
        fs::read(f.root.join("Library/Music/old.mp3")).unwrap(),
        b"old media"
    );
    f.no_stages();
}
#[test]
fn video_import_is_validated_before_and_after_copy_with_ordered_progress() {
    let f = Fixture::new();
    let source = f.file("Intro.MP4", &mp4());
    let mut phases = vec![];
    let result = import_with(&f.root, &source, "Videos", |p| {
        assert!(p.bytes <= p.total || p.total == 0);
        phases.push(p.phase);
        Ok(())
    })
    .unwrap();
    let info = result.video.unwrap();
    assert_eq!(info.duration_ms, 1000);
    assert_eq!(info.codecs, vec!["H.264"]);
    assert!(info.playback_probe_required);
    assert_eq!(phases[0], Phase::Checking);
    assert_eq!(*phases.last().unwrap(), Phase::Done);
    assert!(phases.contains(&Phase::VerifyingSource) && phases.contains(&Phase::VerifyingCopy));
    f.no_stages();
}
#[test]
fn duplicate_and_racing_publications_never_overwrite_existing_media() {
    let f = Fixture::new();
    let source = f.file("song.mp3", b"source");
    let first = import(&f.root, &source, "Music").unwrap();
    assert!(import(&f.root, &source, "Music").is_err());
    assert_eq!(fs::read(first.path).unwrap(), b"source");
    f.no_stages();
    let source = f.file("race.mp3", b"ours");
    let destination = f.root.join("Library/Music/race.mp3");
    assert!(
        import_with(&f.root, &source, "Music", |p| {
            if p.phase == Phase::Publishing {
                fs::write(&destination, b"foreign")?;
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(destination).unwrap(), b"foreign");
    f.no_stages();
}
#[test]
fn cancellation_at_every_prepublication_phase_removes_only_owned_stage() {
    for stop in [
        Phase::Checking,
        Phase::Copying,
        Phase::VerifyingSource,
        Phase::VerifyingCopy,
        Phase::Publishing,
    ] {
        let f = Fixture::new();
        let source = f.file("cancel.mp3", &vec![4; BLOCK + 31]);
        let foreign = f.root.join("foreign.keep");
        fs::write(&foreign, b"preserve").unwrap();
        assert!(
            import_with(&f.root, &source, "Music", |p| {
                if p.phase == stop && !(stop == Phase::Copying && p.bytes == 0) {
                    anyhow::bail!("fixture cancellation");
                }
                Ok(())
            })
            .is_err()
        );
        assert!(!f.root.join("Library/Music/cancel.mp3").exists());
        assert_eq!(fs::read(&foreign).unwrap(), b"preserve");
        assert_eq!(fs::metadata(source).unwrap().len(), (BLOCK + 31) as u64);
        f.no_stages();
    }
}
#[test]
fn a_cancel_after_commit_cannot_remove_or_misreport_the_imported_file() {
    let f = Fixture::new();
    let source = f.file("commit.mp3", b"committed");
    let result = import_with(&f.root, &source, "Music", |p| {
        if p.phase == Phase::Done {
            anyhow::bail!("UI closed after publication");
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read(result.path).unwrap(), b"committed");
    f.no_stages();
}
#[test]
fn source_is_pinned_against_rewrite_delete_and_rename_during_import() {
    let f = Fixture::new();
    let source = f.file("pinned.mp3", &vec![9; BLOCK + 21]);
    let mut tested = false;
    import_with(&f.root, &source, "Music", |p| {
        if p.phase == Phase::VerifyingSource && !tested {
            assert!(fs::OpenOptions::new().write(true).open(&source).is_err());
            assert!(fs::remove_file(&source).is_err());
            assert!(fs::rename(&source, source.with_extension("moved")).is_err());
            tested = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(tested);
    f.no_stages();
}
#[test]
fn changed_source_identity_size_timestamp_and_links_are_not_accepted() {
    let before = Fingerprint {
        volume: 1,
        index: 2,
        size: 3,
        modified: 4,
        attributes: 0,
        links: 1,
    };
    stable_fingerprint(before, before).unwrap();
    for actual in [
        Fingerprint {
            volume: 2,
            ..before
        },
        Fingerprint { index: 3, ..before },
        Fingerprint { size: 4, ..before },
        Fingerprint {
            modified: 5,
            ..before
        },
        Fingerprint {
            attributes: 1,
            ..before
        },
        Fingerprint { links: 2, ..before },
    ] {
        assert!(stable_fingerprint(actual, before).is_err());
    }
    for n in [0, MAX_BYTES + 1, u64::MAX] {
        assert!(valid_size(n).is_err());
    }
    for n in [1, MAX_BYTES] {
        valid_size(n).unwrap();
    }
}
#[test]
fn stage_truncation_corruption_and_growth_are_not_published() {
    for fault in 0..3 {
        let f = Fixture::new();
        let source = f.file("badcopy.mp3", b"correct source content");
        let result = import_inner(&f.root, &source, "Music", &mut |_| Ok(()), |stage| {
            match fault {
                0 => stage.set_len(3)?,
                1 => {
                    stage.seek(SeekFrom::Start(0))?;
                    stage.write_all(b"changed")?;
                }
                _ => {
                    stage.seek(SeekFrom::End(0))?;
                    stage.write_all(b"grew")?;
                }
            }
            stage.sync_all()?;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!f.root.join("Library/Music/badcopy.mp3").exists());
        assert_eq!(fs::read(source).unwrap(), b"correct source content");
        f.no_stages();
    }
}
#[test]
fn invalid_codec_truncated_mp4_and_legacy_mov_do_not_publish() {
    for fault in 0..3 {
        let f = Fixture::new();
        let mut video = mp4();
        let name = if fault == 2 { "old.mov" } else { "bad.mp4" };
        if fault == 0 {
            let at = video.windows(4).position(|b| b == b"avc1").unwrap();
            video[at..at + 4].copy_from_slice(b"hvc1");
        } else if fault == 1 {
            video.truncate(video.len() - 1);
        }
        let source = f.file(name, &video);
        assert!(import(&f.root, &source, "Videos").is_err());
        assert!(!f.root.join("Library").exists());
        f.no_stages();
    }
}
#[test]
fn linked_source_ancestors_and_linked_library_cache_roots_are_refused() {
    for fault in 0..3 {
        let f = Fixture::new();
        let target = f.base.join("Other");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"foreign").unwrap();
        let source = f.file("clip.mp3", b"source");
        let link = match fault {
            0 => f.base.join("SourceLink"),
            1 => f.root.join("Library"),
            _ => {
                fs::create_dir(f.root.join("Cache")).unwrap();
                f.root.join("Cache/Imports")
            }
        };
        crate::mod_profile::fixture_junction(&link, if fault == 0 { &f.source } else { &target })
            .unwrap();
        let supplied = if fault == 0 {
            link.join("clip.mp3")
        } else {
            source
        };
        assert!(import(&f.root, &supplied, "Music").is_err());
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"foreign");
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
        fs::remove_dir(&link).unwrap();
        f.no_stages();
    }
}
#[test]
fn long_portable_paths_and_unicode_filenames_publish_without_legacy_maxpath_limit() {
    let mut f = Fixture::new();
    for index in 0..9 {
        f.root = f.root.join(format!("long-portable-component-{index:02}"));
        fs::create_dir(&f.root).unwrap();
    }
    assert!(f.root.as_os_str().encode_wide().count() > 260);
    let source = f.file("🎬 Compact Intro.MP4", &mp4());
    let imported = import(&f.root, &source, "Videos").unwrap();
    assert_eq!(imported.name, "🎬 Compact Intro.MP4");
    assert_eq!(fs::read(imported.path).unwrap(), mp4());
    f.no_stages();
}
#[test]
fn unsupported_filename_extensions_ads_device_paths_and_relative_paths_fail_closed() {
    for path in [
        r"C:\Input\CON.mp4",
        r"C:\Input\COM1.mp4",
        r"C:\Input\clip:alternate.mp4",
        r"C:\Input\clip.mp4.",
        r"C:\Input\clip.mov",
    ] {
        assert!(filename(Path::new(path), "Videos").is_err());
    }
    for path in [
        r"relative\clip.mp4",
        r"C:\Input\..\clip.mp4",
        r"\\.\pipe\clip.mp4",
    ] {
        assert!(normalized(Path::new(path)).is_err());
    }
}
#[test]
#[ignore = "explicit real MP4 source read; all imported files are confined to disposable fixture storage; no playback"]
fn actual_mp4_import_is_verified_end_to_end_in_fixture_storage_only() {
    let source = PathBuf::from(
        std::env::var_os("C27_MEDIA_IMPORT_MP4").expect("Explicit real MP4 source required"),
    );
    let f = Fixture::new();
    let result = import(&f.root, &source, "Videos").unwrap();
    assert_eq!(fs::metadata(&source).unwrap().len(), result.bytes);
    assert_eq!(fs::read(&source).unwrap(), fs::read(&result.path).unwrap());
    assert!(result.path.starts_with(&f.root));
    assert!(result.video.as_ref().unwrap().playback_probe_required);
    f.no_stages();
    println!(
        "{}",
        serde_json::json!({"schema":"nba2k27-real-mp4-fixture-import-v1","bytes":result.bytes,"sha256":result.sha256,"video":result.video,"userLibraryChanged":false,"gameAccess":false,"videoPlayback":false,"fixtureOnly":true})
    );
}
