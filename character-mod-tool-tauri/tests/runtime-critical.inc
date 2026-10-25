use super::*;

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let fixture = Self(std::env::temp_dir().join(format!("cmt-runtime-critical-{}", stamp())));
        fixture.seed(&fixture.engine());
        fixture
    }
    fn engine(&self) -> PathBuf {
        self.0
            .join("Runtime")
            .join(format!("engine-{}", &HASH[..16]))
    }
    fn seed(&self, destination: &Path) {
        for relative in BOOTSTRAP_FILES {
            let path = destination.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"verified fixture").unwrap();
        }
        fs::write(destination.join("ready.sha256"), HASH).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cmt-runtime-critical-")
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn every_bootstrap_file_must_be_present_nonempty_and_regular() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    assert!(usable(&engine, HASH));
    for relative in BOOTSTRAP_FILES {
        let path = engine.join(relative);
        fs::remove_file(&path).unwrap();
        assert!(!usable(&engine, HASH), "Missing {relative} was accepted");
        assert!(cached_runtime(&fixture.0.join("Runtime"), HASH).is_none());
        fs::write(&path, b"").unwrap();
        assert!(!usable(&engine, HASH), "Empty {relative} was accepted");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(!usable(&engine, HASH), "Directory {relative} was accepted");
        fs::remove_dir(&path).unwrap();
        fs::write(&path, b"verified fixture").unwrap();
        assert!(usable(&engine, HASH));
    }
}

#[test]
fn readiness_marker_is_exact_and_bounded() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    let marker = engine.join("ready.sha256");
    for invalid in [
        Vec::new(),
        b"b".repeat(64),
        b"a".repeat(65),
        b"a".repeat(1024 * 1024),
    ] {
        fs::write(&marker, invalid).unwrap();
        assert!(!usable(&engine, HASH));
    }
    fs::remove_file(&marker).unwrap();
    assert!(!usable(&engine, HASH));
    fs::create_dir(&marker).unwrap();
    assert!(!usable(&engine, HASH));
    fs::remove_dir(&marker).unwrap();
    fs::write(&marker, HASH).unwrap();
    assert!(usable(&engine, HASH));
    assert!(!usable(&engine, &"a".repeat(63)));
}

#[test]
fn readiness_does_not_scan_inventory_or_hash_bootstrap_contents() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    // Full integrity is explicit/repair-only, not part of cached startup.
    fs::write(
        engine.join("Backend/prototype_services.py"),
        b"other nonempty bytes",
    )
    .unwrap();
    fs::write(engine.join("inventory.json"), b"not parsed at startup").unwrap();
    let unrelated = engine.join("Backend/non-bootstrap.bin");
    fs::write(&unrelated, b"not read at startup").unwrap();
    let _locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(unrelated)
        .unwrap();
    assert!(usable(&engine, HASH));
}

#[test]
fn readiness_rejects_bootstrap_and_marker_file_links() {
    use std::os::windows::fs::symlink_file;
    let fixture = Fixture::new();
    let engine = fixture.engine();
    let target = fixture.0.join("link-target");
    fs::write(&target, HASH).unwrap();
    for relative in BOOTSTRAP_FILES
        .iter()
        .copied()
        .chain(std::iter::once("ready.sha256"))
    {
        let path = engine.join(relative);
        let original = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        match symlink_file(&target, &path) {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(1314) => {
                fs::write(&path, original).unwrap();
                eprintln!(
                    "File symlink test requires Developer Mode or symlink privilege; junction coverage remains active"
                );
                return;
            }
            Err(error) => panic!("Could not create fixture file link: {error}"),
        }
        assert!(!usable(&engine, HASH), "Linked {relative} was accepted");
        fs::remove_file(&path).unwrap();
        assert_eq!(fs::read(&target).unwrap(), HASH.as_bytes());
        fs::write(&path, original).unwrap();
    }
}

#[test]
fn readiness_rejects_bootstrap_ancestor_junction() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    let original = engine.join("Backend/python_runtime");
    let moved = fixture.0.join("python-target");
    fs::rename(&original, &moved).unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(original.to_string_lossy().replace('/', "\\"))
        .arg(moved.to_string_lossy().replace('/', "\\"))
        .status()
        .unwrap();
    assert!(status.success(), "Could not create fixture junction");
    let accepted = usable(&engine, HASH);
    fs::remove_dir(&original).unwrap();
    assert!(moved.join("python.exe").is_file());
    fs::rename(&moved, &original).unwrap();
    assert!(!accepted);
    assert!(usable(&engine, HASH));
}

#[test]
fn verified_replacement_restores_new_bootstrap_modules_without_touching_user_files() {
    let fixture = Fixture::new();
    let engine = fixture.engine();
    let inventory: std::collections::BTreeMap<_, _> = BOOTSTRAP_FILES
        .iter()
        .map(|relative| {
            (
                (*relative).to_owned(),
                cache::FileRecord {
                    bytes: 16,
                    sha256: format!("{:x}", Sha256::digest(b"verified fixture")),
                },
            )
        })
        .collect();
    cache::write_inventory(&engine, HASH, inventory.clone()).unwrap();
    for name in [
        "prototype_services.py",
        "prototype_accessories.py",
        "prototype_blender_snapshot.py",
        "prototype_blender_paths.py",
        "prototype_seams.py",
        "prototype_staging.py",
        "prototype_player_writes.py",
    ] {
        fs::remove_file(engine.join("Backend").join(name)).unwrap();
    }
    assert!(!usable(&engine, HASH));
    let settings = fixture.0.join("settings.json");
    let tools = fixture.0.join("user-tools/keep.py");
    fs::write(&settings, b"user settings").unwrap();
    fs::create_dir_all(tools.parent().unwrap()).unwrap();
    fs::write(&tools, b"user tool").unwrap();
    let lock = preparation_lock(&fixture.0.join("Runtime")).unwrap();
    let engine = lock.cache.join(format!("engine-{}", &HASH[..16]));
    let plan = cache::ScratchPlan::new(HASH, inventory.clone(), Default::default()).unwrap();
    let temporary = cache::create_scratch(&lock, plan).unwrap();
    fixture.seed(&temporary);
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for relative in BOOTSTRAP_FILES {
        archive
            .start_file(*relative, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"verified fixture").unwrap();
    }
    let bytes = archive.finish().unwrap().into_inner();
    assert_eq!(
        verify_members(std::io::Cursor::new(bytes), &temporary).unwrap(),
        BOOTSTRAP_FILES.len()
    );
    cache::write_inventory(&temporary, HASH, inventory).unwrap();
    install_prepared(&lock, &temporary, &engine).unwrap();
    assert!(usable(&engine, HASH));
    for relative in BOOTSTRAP_FILES {
        assert_eq!(
            fs::read(engine.join(relative)).unwrap(),
            b"verified fixture"
        );
    }
    assert_eq!(fs::read(settings).unwrap(), b"user settings");
    assert_eq!(fs::read(tools).unwrap(), b"user tool");
}
