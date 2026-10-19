use super::*;
use std::cell::Cell;

#[test]
fn successive_sessions_publish_the_same_target_without_creation_time_tunneling_failures() {
    let f = Fixture::new();
    let mut first = f.store();
    first.activate(&f.prepared, || Ok(())).unwrap();
    first.restore(|| Ok(())).unwrap();
    drop(first);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let session = backend::unique_id();
    let mut second = Store::create(&f.portable, &f.mods, &session, &f.prepared, || Ok(())).unwrap();
    second.activate(&f.prepared, || Ok(())).unwrap();
    second.verify_current().unwrap();
    second.restore(|| Ok(())).unwrap();
    f.preserved();
}

#[test]
fn stable_identity_survives_metadata_changes_and_old_receipts_stay_strict() {
    let f = Fixture::new();
    let mut store = f.store();
    store.activate(&f.prepared, || Ok(())).unwrap();
    let actual = identity(&open(&f.target(), false).unwrap()).unwrap();
    let mut changed_metadata = actual.clone();
    changed_metadata.birth ^= 1;
    assert!(actual.matches(&changed_metadata));
    let mut legacy = actual.clone();
    legacy.stable = None;
    assert!(legacy.matches(&actual));
    assert!(!legacy.matches(&changed_metadata));
    assert!(!actual.matches(&legacy));
    let mut foreign = legacy.clone();
    foreign.index ^= 1;
    assert!(!legacy.matches(&foreign));
    foreign = legacy.clone();
    foreign.volume ^= 1;
    assert!(!legacy.matches(&foreign));

    // The original two-record journal format remains readable. Simulate an
    // old receipt without the new optional field; retain strict old IDs.
    let mut records: Vec<Record> =
        serde_json::Deserializer::from_slice(&fs::read(f.marker()).unwrap())
            .into_iter::<Record>()
            .map(Result::unwrap)
            .collect();
    assert_eq!(records.len(), 2);
    for record in &mut records {
        record.portable_identity.stable = None;
        record.data_identity.stable = None;
        record.root_identity.stable = None;
        record.loc_identity.stable = None;
        if let Some(id) = &mut record.file_identity {
            *id = legacy.clone();
        }
    }
    let serialized = records
        .iter()
        .map(|r| serde_json::to_string(r).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!serialized.contains("stable"));
    drop(store);
    fs::write(f.marker(), serialized).unwrap();
    let mut recovered = Store::recover(&f.portable, &f.mods, &f.session)
        .unwrap()
        .unwrap();
    recovered.restore(|| Ok(())).unwrap();
    assert!(!f.target().exists() && !f.marker().exists());
    f.preserved();
}

struct Fixture {
    base: PathBuf,
    portable: PathBuf,
    mods: PathBuf,
    session: String,
    prepared: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "camera-name-files-fixture-{}",
            backend::unique_id()
        ));
        let portable = base.join("Portable");
        let mods = base.join("SelectedMods");
        fs::create_dir_all(portable.join("Data")).unwrap();
        fs::create_dir_all(portable.join("Library")).unwrap();
        fs::create_dir(&mods).unwrap();
        fs::write(portable.join("Data/settings.json"), b"preserve settings").unwrap();
        fs::write(portable.join("Library/keep.mp4"), b"preserve library").unwrap();
        fs::write(mods.join("other.iff"), b"preserve other mod").unwrap();
        Self {
            base,
            portable,
            mods,
            session: backend::unique_id(),
            prepared: (0..4096).map(|n| (n % 253) as u8).collect(),
        }
    }
    fn store(&self) -> Store {
        Store::create(
            &self.portable,
            &self.mods,
            &self.session,
            &self.prepared,
            || Ok(()),
        )
        .unwrap()
    }
    fn marker(&self) -> PathBuf {
        self.portable.join("Data").join(MARKER)
    }
    fn target(&self) -> PathBuf {
        self.mods.join(TARGET)
    }
    fn preserved(&self) {
        assert_eq!(
            fs::read(self.portable.join("Data/settings.json")).unwrap(),
            b"preserve settings"
        );
        assert_eq!(
            fs::read(self.portable.join("Library/keep.mp4")).unwrap(),
            b"preserve library"
        );
        assert_eq!(
            fs::read(self.mods.join("other.iff")).unwrap(),
            b"preserve other mod"
        );
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
                .starts_with("camera-name-files-fixture-")
        );
        fs::remove_dir_all(&self.base).unwrap();
    }
}
fn persisted_stage(store: &mut Store, data: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(store.stage())
        .unwrap();
    file.write_all(data).unwrap();
    file.sync_all().unwrap();
    let mut next = store.record.clone();
    next.file_identity = Some(identity(&file).unwrap());
    next.publishing = true;
    let mut receipt = store.receipt(true, false).unwrap();
    store.append_publication(&mut receipt, next).unwrap();
}

#[test]
fn intent_publication_verified_cleanup_preserve_data_library_and_other_mods() {
    let f = Fixture::new();
    let mut store = f.store();
    assert!(!f.target().exists());
    let record: Record = serde_json::from_slice(&fs::read(f.marker()).unwrap()).unwrap();
    assert!(!record.publishing && record.file_identity.is_none());
    assert!(crate::native_session::same_path(
        &store.root().to_string_lossy(),
        &f.mods
    ));
    store.activate(&f.prepared, || Ok(())).unwrap();
    assert_eq!(fs::read(f.target()).unwrap(), f.prepared);
    assert!(!store.stage().exists());
    store.verify_prepared(&f.prepared).unwrap();
    assert!(store.verify_prepared(b"different").is_err());
    assert!(store.restore(|| anyhow::bail!("game active")).is_err());
    assert!(f.target().exists() && f.marker().exists());
    store.restore(|| Ok(())).unwrap();
    assert!(!f.target().exists() && !f.marker().exists());
    assert!(
        Store::recover(&f.portable, &f.mods, &f.session)
            .unwrap()
            .is_none()
    );
    assert!(f.mods.join("loc").is_dir()); // never remove a directory the user may reuse
    f.preserved();
}

#[test]
fn custom_file_receipt_and_racing_destinations_are_never_adopted() {
    let f = Fixture::new();
    fs::create_dir(f.mods.join("loc")).unwrap();
    fs::write(f.target(), &f.prepared).unwrap(); // exact bytes still not owned
    assert!(Store::create(&f.portable, &f.mods, &f.session, &f.prepared, || Ok(())).is_err());
    assert_eq!(fs::read(f.target()).unwrap(), f.prepared);
    assert!(!f.marker().exists());
    let f = Fixture::new();
    fs::write(f.marker(), b"foreign receipt").unwrap();
    assert!(Store::create(&f.portable, &f.mods, &f.session, &f.prepared, || Ok(())).is_err());
    assert_eq!(fs::read(f.marker()).unwrap(), b"foreign receipt");
    let f = Fixture::new();
    let mut store = f.store();
    fs::write(f.target(), b"appeared later").unwrap();
    assert!(store.activate(&f.prepared, || Ok(())).is_err());
    store.restore(|| Ok(())).unwrap();
    assert_eq!(fs::read(f.target()).unwrap(), b"appeared later");
    f.preserved();
}

#[test]
fn publication_uses_no_replace_handle_even_after_final_absence_check() {
    let f = Fixture::new();
    let mut store = f.store();
    let count = Cell::new(0);
    let result = store.activate(&f.prepared, || {
        count.set(count.get() + 1);
        // Seventh check is immediately after the final target-absent check.
        if count.get() == 7 {
            fs::write(f.target(), b"racing foreign mod")?;
        }
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(fs::read(f.target()).unwrap(), b"racing foreign mod");
    assert!(store.restore(|| Ok(())).is_err()); // stage + conflicting target
    assert!(store.stage().exists() && f.marker().exists());
    f.preserved();
}

#[test]
fn retained_receipt_cannot_be_rewritten_renamed_or_deleted_during_publication() {
    let f = Fixture::new();
    let mut store = f.store();
    let count = Cell::new(0);
    store
        .activate(&f.prepared, || {
            count.set(count.get() + 1);
            if count.get() == 2 {
                assert!(fs::write(f.marker(), b"foreign receipt").is_err());
                assert!(fs::rename(f.marker(), f.portable.join("Data/moved.json")).is_err());
                assert!(fs::remove_file(f.marker()).is_err());
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(count.get(), 7);
    let mut receipt = open(&f.marker(), false).unwrap();
    let record = read_receipt(&mut receipt).unwrap();
    assert!(record.publishing && record.file_identity.is_some());
    drop(receipt);
    store.restore(|| Ok(())).unwrap();
    f.preserved();
}

#[test]
fn torn_duplicate_or_mismatched_publication_records_hold_the_exact_owned_target() {
    for mode in 0..4 {
        let f = Fixture::new();
        let mut store = f.store();
        store.activate(&f.prepared, || Ok(())).unwrap();
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(f.marker())
            .unwrap();
        if mode == 0 {
            file.write_all(b"\n{torn publication").unwrap();
        }
        if mode == 1 {
            file.write_all(&serde_json::to_vec(&store.record).unwrap())
                .unwrap();
        }
        drop(file);
        if mode >= 2 {
            let mut intent = store.record.clone();
            intent.publishing = false;
            intent.file_identity = None;
            let mut publication = store.record.clone();
            if mode == 2 {
                publication.sha256 = "0".repeat(64);
            } else {
                publication.publishing = false;
                publication.file_identity = None;
            }
            let data = format!(
                "{}\n{}\n",
                serde_json::to_string(&intent).unwrap(),
                serde_json::to_string(&publication).unwrap()
            );
            fs::write(f.marker(), data).unwrap();
        }
        let before = fs::read(f.marker()).unwrap();
        assert!(Store::recover(&f.portable, &f.mods, &f.session).is_err());
        assert!(store.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(f.marker()).unwrap(), before);
        assert_eq!(fs::read(f.target()).unwrap(), f.prepared);
    }
}

#[test]
fn closed_guard_failure_at_every_mutation_boundary_prevents_later_publication() {
    let f = Fixture::new();
    assert!(
        Store::create(
            &f.portable,
            &f.mods,
            &f.session,
            &f.prepared,
            || anyhow::bail!("game active")
        )
        .is_err()
    );
    assert!(!f.marker().exists() && !f.mods.join("loc").exists());
    for fail_at in 1..=7 {
        let f = Fixture::new();
        let mut store = f.store();
        let count = Cell::new(0);
        assert!(
            store
                .activate(&f.prepared, || {
                    count.set(count.get() + 1);
                    ensure!(count.get() != fail_at, "game appeared");
                    Ok(())
                })
                .is_err()
        );
        assert!(!f.target().exists(), "guard {fail_at} published a file");
        let stage = store.stage();
        let known = store.record.file_identity.is_some();
        drop(store);
        let mut recovered = Store::recover(&f.portable, &f.mods, &f.session)
            .unwrap()
            .unwrap();
        if stage.exists() && !known {
            assert!(recovered.restore(|| Ok(())).is_err());
            assert!(stage.exists() && f.marker().exists());
        } else {
            recovered.restore(|| Ok(())).unwrap();
            assert!(!stage.exists() && !f.marker().exists());
        }
        f.preserved();
    }
}

#[test]
fn crashes_after_durable_stage_and_after_rename_recover_exact_owned_file() {
    for renamed in [false, true] {
        let f = Fixture::new();
        let mut store = f.store();
        persisted_stage(&mut store, &f.prepared);
        if renamed {
            let file = open(&store.stage(), true).unwrap();
            let target = f
                .mods
                .join("loc")
                .canonicalize()
                .unwrap()
                .join("english.iff");
            loader_files::publish(&file, &target).unwrap();
        }
        drop(store);
        let mut recovered = Store::recover(&f.portable, &f.mods, &f.session)
            .unwrap()
            .unwrap();
        recovered.restore(|| Ok(())).unwrap();
        assert!(!f.target().exists() && !recovered.stage().exists() && !f.marker().exists());
        f.preserved();
    }
    let f = Fixture::new();
    let store = f.store();
    drop(store); // intent only, before stage existed
    Store::recover(&f.portable, &f.mods, &f.session)
        .unwrap()
        .unwrap()
        .restore(|| Ok(()))
        .unwrap();
    assert!(!f.marker().exists());
}

#[test]
fn unrecorded_partial_or_altered_stages_are_preserved_not_hash_adopted() {
    for mode in 0..3 {
        let f = Fixture::new();
        let mut store = f.store();
        if mode == 0 {
            fs::write(store.stage(), &f.prepared).unwrap(); // no durable identity
        } else {
            persisted_stage(&mut store, &f.prepared);
            fs::write(
                store.stage(),
                if mode == 1 {
                    &b"partial"[..]
                } else {
                    &b"different bytes"[..]
                },
            )
            .unwrap();
        }
        let stage = store.stage();
        let before = fs::read(&stage).unwrap();
        drop(store);
        let mut recovered = Store::recover(&f.portable, &f.mods, &f.session)
            .unwrap()
            .unwrap();
        assert!(recovered.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(stage).unwrap(), before);
        assert!(f.marker().exists());
    }
}

#[test]
fn exact_bytes_on_replacement_inode_and_hardlinks_do_not_authorize_removal() {
    for hardlink in [false, true] {
        let f = Fixture::new();
        let mut store = f.store();
        store.activate(&f.prepared, || Ok(())).unwrap();
        if hardlink {
            fs::hard_link(f.target(), f.mods.join("user-copy.iff")).unwrap();
        } else {
            fs::rename(f.target(), f.mods.join("saved-original.iff")).unwrap();
            fs::write(f.target(), &f.prepared).unwrap();
        }
        assert!(store.verify_prepared(&f.prepared).is_err());
        assert!(store.restore(|| Ok(())).is_err());
        assert_eq!(fs::read(f.target()).unwrap(), f.prepared);
        assert!(f.marker().exists());
        f.preserved();
    }
}

#[test]
fn foreign_or_truncated_receipts_wrong_session_and_wrong_root_hold_files() {
    for mode in 0..6 {
        let f = Fixture::new();
        let mut store = f.store();
        store.activate(&f.prepared, || Ok(())).unwrap();
        if mode == 0 {
            fs::write(f.marker(), b"{truncated").unwrap();
        }
        if mode == 1 {
            let mut record = store.record.clone();
            record.staged = "../other.iff".into();
            backend::atomic_json(&f.marker(), &record).unwrap();
        }
        if mode == 2 {
            let mut record = store.record.clone();
            record.target = "char/other.iff".into();
            backend::atomic_json(&f.marker(), &record).unwrap();
        }
        if mode == 3 {
            let mut record = store.record.clone();
            record.data_identity.index ^= 1;
            backend::atomic_json(&f.marker(), &record).unwrap();
        }
        let original = fs::read(f.marker()).unwrap();
        let other = f.base.join("OtherProfile");
        fs::create_dir(&other).unwrap();
        let result = Store::recover(
            &f.portable,
            if mode == 4 { &other } else { &f.mods },
            if mode == 5 {
                "ffffffffffffffffffffffffffffffffffffffffffffffff"
            } else {
                &f.session
            },
        );
        assert!(result.is_err());
        assert!(store.restore(|| Ok(())).is_err() || mode >= 4); // original store still owns modes 4/5
        if mode < 4 {
            assert_eq!(fs::read(f.marker()).unwrap(), original);
            assert_eq!(fs::read(f.target()).unwrap(), f.prepared);
        }
        f.preserved();
    }
}

#[test]
fn unknown_json_fields_and_unbounded_receipts_are_never_rewritten() {
    for oversized in [false, true] {
        let f = Fixture::new();
        let store = f.store();
        if oversized {
            fs::write(f.marker(), vec![b' '; RECEIPT_LIMIT + 1]).unwrap();
        } else {
            let mut value = serde_json::to_value(&store.record).unwrap();
            value["foreignOwner"] = serde_json::json!(true);
            fs::write(f.marker(), serde_json::to_vec(&value).unwrap()).unwrap();
        }
        let before = fs::read(f.marker()).unwrap();
        drop(store);
        assert!(Store::recover(&f.portable, &f.mods, &f.session).is_err());
        assert_eq!(fs::read(f.marker()).unwrap(), before);
    }
}

#[test]
fn pinned_root_and_loc_cannot_be_replaced_and_foreign_junctions_are_rejected() {
    let f = Fixture::new();
    let store = f.store();
    assert!(fs::rename(&f.mods, f.base.join("MovedMods")).is_err());
    assert!(fs::rename(f.mods.join("loc"), f.mods.join("moved-loc")).is_err());
    drop(store);
    for component in ["loc", "Data"] {
        let f = Fixture::new();
        let foreign = f.base.join("Foreign");
        fs::create_dir(&foreign).unwrap();
        fs::write(foreign.join("keep.txt"), b"foreign content").unwrap();
        let path = if component == "loc" {
            f.mods.join(component)
        } else {
            fs::rename(f.portable.join("Data"), f.portable.join("SavedData")).unwrap();
            f.portable.join(component)
        };
        crate::mod_profile::fixture_junction(&path, &foreign).unwrap();
        assert!(Store::create(&f.portable, &f.mods, &f.session, &f.prepared, || Ok(())).is_err());
        assert_eq!(
            fs::read(foreign.join("keep.txt")).unwrap(),
            b"foreign content"
        );
        assert!(!foreign.join("english.iff").exists() && !foreign.join(MARKER).exists());
        fs::remove_dir(path).unwrap(); // exact fixture junction, not its target
    }
}

#[test]
fn long_mod_and_portable_paths_publish_and_restore_without_max_path_failure() {
    let mut f = Fixture::new();
    let mut nested = f.base.clone();
    for n in 0..6 {
        nested.push(format!(
            "long-fixture-component-{n}-abcdefghijklmnopqrstuvwxyz"
        ));
    }
    f.portable = nested.join("Portable");
    f.mods = nested.join("SelectedMods");
    fs::create_dir_all(f.portable.join("Data")).unwrap();
    fs::create_dir_all(&f.mods).unwrap();
    assert!(f.portable.as_os_str().len() > 260);
    let mut store = f.store();
    store.activate(&f.prepared, || Ok(())).unwrap();
    drop(store);
    let mut recovered = Store::recover(&f.portable, &f.mods, &f.session)
        .unwrap()
        .unwrap();
    recovered.verify_prepared(&f.prepared).unwrap();
    recovered.restore(|| Ok(())).unwrap();
    assert!(!f.target().exists() && !f.marker().exists());
}

#[test]
fn invalid_session_bounds_and_relative_roots_fail_before_receipt_or_stage() {
    let f = Fixture::new();
    for session in ["../outside", "01234", &"z".repeat(48)] {
        assert!(Store::create(&f.portable, &f.mods, session, &f.prepared, || Ok(())).is_err());
    }
    for data in [vec![], vec![1; 63], vec![1; MAX_BYTES + 1]] {
        assert!(Store::create(&f.portable, &f.mods, &f.session, &data, || Ok(())).is_err());
    }
    assert!(
        Store::create(
            Path::new("relative"),
            &f.mods,
            &f.session,
            &f.prepared,
            || Ok(())
        )
        .is_err()
    );
    assert!(
        Store::create(
            &f.portable,
            Path::new("relative"),
            &f.session,
            &f.prepared,
            || Ok(())
        )
        .is_err()
    );
    assert!(!f.marker().exists() && !f.mods.join("loc").exists());
}

#[test]
fn absent_receipt_never_requires_an_already_restored_or_deleted_selected_root() {
    let f = Fixture::new();
    let no_longer_existing = f.base.join("Game/mods");
    assert!(!no_longer_existing.exists());
    assert!(
        Store::recover(&f.portable, &no_longer_existing, &f.session)
            .unwrap()
            .is_none()
    );
    // A real receipt still requires its exact selected root and never treats
    // the absence check as permission to recover against a different folder.
    let store = f.store();
    assert!(Store::recover(&f.portable, &no_longer_existing, &f.session).is_err());
    drop(store);
    assert!(f.marker().exists());
    f.preserved();
}
