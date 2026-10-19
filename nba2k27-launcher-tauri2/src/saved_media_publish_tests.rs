use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(1);
const VIDEO: &str = "12345678-1234-1234-1234-123456789abc";
struct Fixture {
    base: PathBuf,
    review: Review,
    before: BTreeMap<String, Owned>,
}
fn write(root: &Path, rel: &str, data: &[u8]) -> Result<()> {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, data)?;
    Ok(())
}
impl Fixture {
    fn new() -> Result<Self> {
        let base = std::env::current_dir()?.join("artifacts").join(format!(
            "saved-media-publish-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let stage = base.join("Stage");
        let target = base.join("MainFixture");
        fs::create_dir_all(&stage)?;
        for dir in ["Data/Rust", "Library", "Recovery"] {
            fs::create_dir_all(target.join(dir))?;
        }
        let original = BTreeMap::from([
            (
                "Data/settings.json".to_string(),
                b"original WPF settings".to_vec(),
            ),
            (
                "Library/Music/library.json".to_string(),
                b"original music catalog".to_vec(),
            ),
            (
                "Library/Videos/mp4-library.json".to_string(),
                b"original video catalog".to_vec(),
            ),
        ]);
        let mut originals = BTreeMap::new();
        let mut before = BTreeMap::new();
        for (rel, data) in original {
            write(&target, &rel, &data)?;
            originals.insert(rel.clone(), io::hash(&data));
            before.insert(rel.clone(), Source::open(&target.join(&rel))?.owned);
        }
        write(
            &target,
            "Data/Rust/settings.json",
            b"preserved native Rust settings",
        )?;
        before.insert(
            "Data/Rust/settings.json".into(),
            Source::open(&target.join("Data/Rust/settings.json"))?.owned,
        );
        write(
            &stage,
            "Library/Jukebox/Audio/native-sha.wav",
            b"small native Opus fixture bytes",
        )?;
        write(
            &stage,
            "Library/Jukebox/Artwork/art-sha.png",
            b"small artwork fixture",
        )?;
        write(
            &stage,
            "Data/Jukebox/library.json",
            br#"{"schema":"nba2k27-rust-jukebox-library-v1","activePlaylistId":null}"#,
        )?;
        write(
            &stage,
            "Data/Rust/LegacyImport/legacy-jukebox-library.json",
            b"preserved legacy music provenance",
        )?;
        write(
            &stage,
            "Data/Rust/LegacyImport/legacy-mp4-library.json",
            b"preserved legacy video provenance",
        )?;
        write(&stage,"Data/Rust/legacy-media-import-complete.json",br#"{"schema":"nba2k27-legacy-media-staged-v1","activePlaylistId":null,"startupVideoEnabled":false}"#)?;
        let video = b"small opaque prevalidated video fixture";
        let video_rel = format!("Library/Videos/mp4/{VIDEO}.mp4");
        write(&stage, &video_rel, video)?;
        write(&target, &video_rel, video)?;
        before.insert(
            video_rel.clone(),
            Source::open(&target.join(&video_rel))?.owned,
        );
        write(
            &stage,
            "Data/Rust/startup-videos.json",
            &serde_json::to_vec(
                &json!({"schema":"nba2k27-rust-startup-video-library-v1","selected":VIDEO,
            "items":[{"id":VIDEO,"relativePath":video_rel,"sha256":io::hash(video),"bytes":video.len()}]}),
            )?,
        )?;
        let inventory = Tree::open(&stage)?.digest();
        Ok(Self {
            base,
            review: Review {
                stage,
                target,
                inventory,
                originals,
            },
            before,
        })
    }
    fn unchanged(&self) -> Result<()> {
        for (rel, owned) in &self.before {
            ensure!(
                Source::open(&self.review.target.join(rel))?.owned == *owned,
                "Original changed: {rel}"
            );
        }
        Ok(())
    }
    fn absent_targets(&self) -> Result<()> {
        for (rel, _) in TARGETS {
            ensure!(
                io::missing(&self.review.target.join(rel))?,
                "Target remained: {rel}"
            );
        }
        Ok(())
    }
}
fn success() -> Result<()> {
    let f = Fixture::new()?;
    let report = publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    assert_eq!(report.phase, "complete");
    assert_eq!(report.copied_files, 7);
    assert_eq!(report.reused_videos, 1);
    f.unchanged()?;
    assert_eq!(Tree::open(&f.review.stage)?.digest(), f.review.inventory);
    assert_eq!(
        recover(f.review.clone(), false, &|| Ok(()))?.phase,
        "complete-verified"
    );
    assert!(publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(())).is_err());
    f.unchanged()?;
    let report = recover(f.review.clone(), true, &|| Ok(()))?;
    assert!(report.phase.starts_with("rolled-back"));
    f.absent_targets()?;
    f.unchanged()?;
    assert!(io::missing(&f.review.target.join(RECEIPT))?);
    // Explicit rollback archived only its exact receipt and kept the copies in
    // Recovery. A fresh attempt has a fresh nonce and can succeed without merge.
    publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    f.unchanged()?;
    let mut fresh = Fixture::new()?;
    fs::remove_file(fresh.review.target.join("Data/Rust/settings.json"))?;
    fs::remove_dir(fresh.review.target.join("Data/Rust"))?;
    fresh.before.remove("Data/Rust/settings.json");
    publish(fresh.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    // The coordinator may create its own settings after publication. They
    // are not ours to roll back even though this tool created the directory.
    write(
        &fresh.review.target,
        "Data/Rust/settings.json",
        b"later coordinator settings",
    )?;
    fresh.before.insert(
        "Data/Rust/settings.json".into(),
        Source::open(&fresh.review.target.join("Data/Rust/settings.json"))?.owned,
    );
    recover(fresh.review.clone(), true, &|| Ok(()))?;
    fresh.absent_targets()?;
    fresh.unchanged()?;
    println!(
        "PASS five atomic no-clobber units, raw native byte copies, reused MP4 identity, preserved WPF/Rust settings and source inventory; exact rollback and fresh retry"
    );
    Ok(())
}
fn faults() -> Result<()> {
    let phases = [
        ("copied-file", 0),
        ("prepared", 0),
        ("intent", 0),
        ("renamed", 0),
        ("published", 0),
        ("intent", 4),
        ("renamed", 4),
        ("published", 4),
    ];
    for (phase, index) in phases {
        let f = Fixture::new()?;
        assert!(
            publish(f.review.clone(), &|| Ok(()), &|p, i| {
                ensure!(!(p == phase && i == index), "injected {phase}/{index}");
                Ok(())
            })
            .is_err()
        );
        f.unchanged()?;
        assert!(
            recover(f.review.clone(), false, &|| Ok(()))?
                .phase
                .starts_with("held-partial")
        );
        recover(f.review.clone(), true, &|| Ok(()))?;
        f.absent_targets()?;
        f.unchanged()?;
    }
    println!(
        "PASS eight partial-copy/prepared/intent/rename/acknowledgement faults: receipt holds and explicit exact-owned rollback never deletes originals"
    );
    Ok(())
}
fn collisions() -> Result<()> {
    for index in 0..5 {
        let f = Fixture::new()?;
        let (rel, dir) = TARGETS[index];
        let path = f.review.target.join(rel);
        if dir {
            fs::create_dir_all(&path)?;
            fs::write(path.join("foreign"), b"keep")?;
        } else {
            fs::write(&path, b"foreign")?;
        }
        assert!(publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(())).is_err());
        assert!(io::missing(&f.review.target.join(RECEIPT))?);
        f.unchanged()?;
    }
    let f = Fixture::new()?;
    let foreign = f.review.target.join("Data/Jukebox");
    assert!(
        publish(f.review.clone(), &|| Ok(()), &|p, i| {
            if p == "intent" && i == 1 {
                fs::create_dir(&foreign)?;
                fs::write(foreign.join("keep"), b"foreign race")?;
            }
            Ok(())
        })
        .is_err()
    );
    assert!(recover(f.review.clone(), true, &|| Ok(())).is_err());
    assert_eq!(fs::read(foreign.join("keep"))?, b"foreign race");
    assert!(f.review.target.join("Library/Jukebox").exists());
    f.unchanged()?;
    println!(
        "PASS all five existing-target conflicts before receipt creation and publication-time target race; foreign content holds rollback before any unit moves"
    );
    Ok(())
}
fn changed_evidence() -> Result<()> {
    let f = Fixture::new()?;
    write(
        &f.review.stage,
        "Library/Jukebox/Audio/native-sha.wav",
        b"tampered",
    )?;
    assert!(publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(())).is_err());
    assert!(io::missing(&f.review.target.join(RECEIPT))?);
    f.unchanged()?;
    let f = Fixture::new()?;
    let path = f
        .review
        .target
        .join(format!("Library/Videos/mp4/{VIDEO}.mp4"));
    fs::write(path, b"changed original video")?;
    assert!(publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(())).is_err());
    assert!(io::missing(&f.review.target.join(RECEIPT))?);
    let f = Fixture::new()?;
    publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    let file = f.review.target.join("Library/Jukebox/Audio/native-sha.wav");
    let data = fs::read(&file)?;
    fs::rename(&file, file.with_extension("held"))?;
    fs::write(&file, data)?;
    assert!(recover(f.review.clone(), true, &|| Ok(())).is_err());
    f.unchanged()?;
    let f = Fixture::new()?;
    publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    let library = f.review.target.join("Library");
    let held = f.review.target.join("HeldLibrary");
    fs::rename(&library, &held)?;
    fs::create_dir(&library)?;
    for child in ["Music", "Videos", "Jukebox"] {
        fs::rename(held.join(child), library.join(child))?;
    }
    // Original file IDs and every published child ID stayed identical, but
    // the parent namespace is not the durable one and must not be adopted.
    f.unchanged()?;
    assert!(recover(f.review.clone(), true, &|| Ok(())).is_err());
    let f = Fixture::new()?;
    publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    fs::create_dir(
        f.review
            .target
            .join("Library/Jukebox/foreign-empty-directory"),
    )?;
    assert!(recover(f.review.clone(), true, &|| Ok(())).is_err());
    f.unchanged()?;
    let f = Fixture::new()?;
    publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    let receipt = f.review.target.join(RECEIPT);
    let data = fs::read(&receipt)?;
    fs::rename(&receipt, receipt.with_extension("held"))?;
    fs::write(&receipt, data)?;
    assert!(recover(f.review.clone(), true, &|| Ok(())).is_err());
    f.unchanged()?;
    println!(
        "PASS frozen inventory/original video mismatch rejection; same-byte foreign dataset, added empty directory, replaced receipt and changed parent identities hold"
    );
    Ok(())
}
fn cancellation_and_source_pins() -> Result<()> {
    let f = Fixture::new()?;
    assert!(
        publish(
            f.review.clone(),
            &|| anyhow::bail!("closed guard denied"),
            &|_, _| Ok(())
        )
        .is_err()
    );
    assert!(io::missing(&f.review.target.join(RECEIPT))?);
    f.absent_targets()?;
    f.unchanged()?;
    let f = Fixture::new()?;
    let prepared = Prepared::open(f.review.clone())?;
    assert!(fs::rename(&f.review.stage, f.base.join("MovedStage")).is_err());
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(f.review.stage.join("Library/Jukebox/Audio/native-sha.wav"))
            .is_err()
    );
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(f.review.target.join("Data/settings.json"))
            .is_err()
    );
    drop(prepared);
    f.unchanged()?;
    println!(
        "PASS repeated closed-guard admission and retained source/root/MP4/settings pins; production Main entrypoints never invoked by fixtures"
    );
    Ok(())
}
fn interrupted_rollback() -> Result<()> {
    for point in 0..3 {
        let f = Fixture::new()?;
        publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
        {
            let plan = Prepared::open(f.review.clone())?;
            let (mut journal, events) = Journal::load(&f.review.target.join(RECEIPT), true)?;
            let recovered = recovery(&plan, &journal, &events)?;
            let unit = &recovered.units[&4];
            let spec = &plan.specs[4];
            let work = workspace(&plan.review.target, &recovered.header.nonce);
            journal.append(&Event::RollbackIntent { index: 4 })?;
            if point >= 1 {
                let source = f.review.target.join(&spec.target);
                validate_unit(&plan, unit, &source)?;
                let node = io::open_node(&source, false, true)?;
                io::rename(&node, &temporary(&work, 4))?;
                drop(node);
            }
            if point == 2 {
                journal.append(&Event::RolledBackUnit { index: 4 })?;
            }
        }
        assert!(
            recover(f.review.clone(), false, &|| Ok(()))?
                .phase
                .starts_with("held-partial")
        );
        recover(f.review.clone(), true, &|| Ok(()))?;
        f.absent_targets()?;
        f.unchanged()?;
    }
    let f = Fixture::new()?;
    publish(f.review.clone(), &|| Ok(()), &|_, _| Ok(()))?;
    let receipt = f.review.target.join(RECEIPT);
    let mut bytes = fs::read(&receipt)?;
    bytes.pop();
    fs::write(&receipt, bytes)?;
    assert!(recover(f.review.clone(), true, &|| Ok(())).is_err());
    f.unchanged()?;
    assert!(f.review.target.join("Data/Jukebox/library.json").is_file());
    println!(
        "PASS rollback restart before/after owned rename/acknowledgement; incomplete receipt tails hold without guessing or changing published data"
    );
    Ok(())
}
pub(crate) fn run() -> Result<()> {
    success()?;
    faults()?;
    collisions()?;
    changed_evidence()?;
    cancellation_and_source_pins()?;
    interrupted_rollback()?;
    Ok(())
}
