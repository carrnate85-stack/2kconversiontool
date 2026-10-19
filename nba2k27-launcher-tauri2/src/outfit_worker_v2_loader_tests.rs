use super::*;
struct Fixture {
    root: PathBuf,
    game: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts")
            .join(format!("outfit-v2-loader-{}", crate::backend::unique_id()));
        fs::create_dir_all(root.join("Data")).unwrap();
        let game = root.join("Game");
        fs::create_dir(&game).unwrap();
        Self { root, game }
    }
    fn store(&self) -> Store {
        Store::create(
            &self.root,
            &self.game,
            &"a".repeat(48),
            b"CONFIG\r\n",
            || Ok(()),
        )
        .unwrap()
    }
    fn marker(&self) -> PathBuf {
        self.root.join("Data").join(JOURNAL)
    }
    fn replace(&self, path: &Path, bytes: &[u8]) {
        fs::rename(
            path,
            path.with_extension(format!("saved-{}", crate::backend::unique_id())),
        )
        .unwrap();
        fs::write(path, bytes).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn same_byte_receipt_replacement_after_create_cannot_publish_or_be_adopted() {
    let f = Fixture::new();
    let mut s = f.store();
    let original = fs::read(f.marker()).unwrap();
    f.replace(&f.marker(), &original);
    assert!(s.install_loader(b"CONFIG\r\n", || Ok(())).is_err());
    assert!(!f.game.join("version.dll").exists());
    assert_eq!(fs::read(f.marker()).unwrap(), original);
    assert!(Store::recover(&f.root, &f.game).is_err());
}
#[test]
fn foreign_receipt_after_recover_never_removed_or_overwritten() {
    let f = Fixture::new();
    let mut s = f.store();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    drop(s);
    let mut s = Store::recover(&f.root, &f.game).unwrap().unwrap();
    f.replace(&f.marker(), b"foreign");
    assert!(s.restore(|| Ok(())).is_err());
    assert_eq!(fs::read(f.marker()).unwrap(), b"foreign");
    assert!(f.game.join("version.dll").exists());
}
#[test]
fn torn_or_corrupt_receipt_holds_physical_cleanup() {
    for tail in [b"\n{".as_slice(), b"\nnull".as_slice()] {
        let f = Fixture::new();
        let mut s = f.store();
        s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
        drop(s);
        fs::OpenOptions::new()
            .append(true)
            .open(f.marker())
            .unwrap()
            .write_all(tail)
            .unwrap();
        assert!(Store::recover(&f.root, &f.game).is_err());
        assert!(f.game.join("version.dll").exists());
    }
}
#[test]
fn same_byte_payload_replacement_is_foreign_even_with_owned_receipt() {
    let f = Fixture::new();
    let mut s = f.store();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    let target = f.game.join("version.dll");
    f.replace(&target, PROXY);
    assert!(s.restore(|| Ok(())).is_err());
    assert_eq!(fs::read(target).unwrap(), PROXY);
    assert!(pending(&f.root).unwrap());
}
#[test]
fn ordinary_repeated_sessions_have_durable_identity_and_bounded_append_history() {
    let f = Fixture::new();
    for _ in 0..3 {
        let mut s = f.store();
        s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
        s.trigger(|| Ok(())).unwrap();
        assert!(fs::metadata(f.marker()).unwrap().len() < RECEIPT_LIMIT);
        drop(s);
        let mut s = Store::recover(&f.root, &f.game).unwrap().unwrap();
        assert!(s.restore(|| anyhow::bail!("still running")).is_err());
        assert!(f.game.join("version.dll").exists());
        s.restore(|| Ok(())).unwrap();
        assert!(!pending(&f.root).unwrap());
        assert!(!f.game.join("version.dll").exists());
    }
}
#[test]
fn late_process_guard_holds_receipt_and_payloads() {
    let f = Fixture::new();
    let mut s = f.store();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(
        s.restore(|| {
            calls.set(calls.get() + 1);
            ensure!(calls.get() < 2, "late game");
            Ok(())
        })
        .is_err()
    );
    assert!(f.game.join("version.dll").exists());
    assert!(pending(&f.root).unwrap());
}
#[test]
fn original_loader_receipt_blocks_new_payload_creation() {
    let f = Fixture::new();
    fs::write(f.root.join("Data/loader-session.json"), b"foreign").unwrap();
    assert!(Store::create(&f.root, &f.game, &"a".repeat(48), b"CONFIG\r\n", || Ok(())).is_err());
    assert!(!f.marker().exists());
}

#[test]
fn valid_forward_slash_input_is_bound_to_the_same_canonical_game_as_outer_journal() {
    let f = Fixture::new();
    let spelling = PathBuf::from(f.game.to_string_lossy().replace('\\', "/"));
    let canonical = f.game.canonicalize().unwrap();
    let mut s = Store::create(
        &f.root,
        &spelling,
        &"a".repeat(48),
        b"CONFIG\r\n",
        || Ok(()),
    )
    .unwrap();
    assert_eq!(s.record.game_root, canonical);
    assert_eq!(s.game, canonical);
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    drop(s);
    let mut recovered = Store::recover(&f.root, &canonical).unwrap().unwrap();
    recovered.restore(|| Ok(())).unwrap();
    assert!(!f.game.join("version.dll").exists());
    let mut s = Store::create(&f.root, &canonical, &"a".repeat(48), b"CONFIG\r\n", || {
        Ok(())
    })
    .unwrap();
    s.install_loader(b"CONFIG\r\n", || Ok(())).unwrap();
    drop(s);
    let mut recovered = Store::recover(&f.root, &spelling).unwrap().unwrap();
    recovered.restore(|| Ok(())).unwrap();
    assert!(!pending(&f.root).unwrap());
}
