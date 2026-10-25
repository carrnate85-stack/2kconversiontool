use super::*;
use crate::{
    blacktop_arena::Region,
    rules_v2::tests::{Fake as RulesFake, fixture as rules_fixture},
};
use std::cell::Cell;
#[test]
#[cfg(feature = "dev-model-preview")]
fn custom_blacktop_catalog_import_provider_aliases_plan_and_restore() {
    use crate::{backend, custom_blacktop as custom, provider_assets::Service};
    use std::{fs, sync::{Arc, atomic::AtomicBool}};
    let root = std::env::temp_dir().join(format!("c27-custom-arena-{}", backend::unique_id()));
    let mods = root.join("Mods");
    fs::create_dir_all(mods.join("levels/Arena pack")).unwrap();
    let original = b"fixture arena source bytes";
    fs::write(mods.join("levels/Arena pack/arena_test.iff"), original).unwrap();
    fs::write(mods.join("levels/Arena pack/floor_test.iff"), b"fixture court").unwrap();
    fs::create_dir_all(mods.join("players")).unwrap();
    fs::write(mods.join("players/png0123.iff"), b"player fixture").unwrap();
    fs::write(mods.join("arena_outside.iff"), b"outside levels fixture").unwrap();
    let sources = custom::request(&root, &mods, &serde_json::json!({"action":"sources"})).unwrap();
    assert_eq!(sources["files"], serde_json::json!(["levels/Arena pack/arena_test.iff", "levels/Arena pack/floor_test.iff"]));
    assert_eq!(sources["levelsRoot"], serde_json::json!(mods.join("levels")));
    let empty_mods = root.join("Empty mods");
    fs::create_dir(&empty_mods).unwrap();
    assert_eq!(custom::request(&root, &empty_mods, &serde_json::json!({"action":"sources"})).unwrap()["files"], serde_json::json!([]));
    let request = serde_json::json!({"action":"add","name":"My custom arena","baseId":"blacktop-stock","arenaSource":"levels/Arena pack/arena_test.iff","floorSource":"levels/Arena pack/floor_test.iff"});
    let mut outside = request.clone(); outside["arenaSource"] = serde_json::json!("arena_outside.iff");
    assert!(custom::request(&root, &mods, &outside).is_err(), "manual requests must not bypass the levels restriction");
    let added = custom::request(&root, &mods, &request).unwrap();
    let id = added["id"].as_str().unwrap();
    let selected = arena::choice(id).unwrap();
    let mut settings = backend::Settings::default();
    settings.arena_id = id.into(); settings.arena = "My custom arena".into();
    assert_eq!(crate::feature_plan::Plan::freeze(&settings, &crate::library::Library::default()).unwrap().arena.as_deref(), Some(id));
    assert!(custom::request(&root, &mods, &request).is_err(), "duplicate names must not create ambiguous selections");
    assert_eq!(selected.arena_id, Some(700));
    assert!(prepare(&fixture(), id, &STOCK, None).is_err(), "unpublished sources cannot authorize native changes");
    let assets = Service::open(&mods, &[], &[], Arc::new(AtomicBool::new(false))).unwrap();
    custom::publish(&root, &mods, &assets, Some(id)).unwrap();
    let alias = format!("levels/{}.iff", selected.file.as_deref().unwrap());
    let (_, bytes) = assets.read(&alias, 0, original.len() as u32).unwrap();
    assert_eq!(bytes, original);
    let (_, bytes) = assets.read(alias.strip_prefix("levels/").unwrap(), 0, original.len() as u32).unwrap();
    assert_eq!(bytes, original, "filename-only game requests must reach the same mod");
    let mut f = fixture();
    let p = prepare(&f, id, &STOCK, None).unwrap();
    assert!(arena::valid_template_fields(p.snapshot.record.as_ref().unwrap(), p.template.as_deref().unwrap()));
    let mut forged = selected.clone(); forged.file = Some("arena_other_int".into());
    assert!(capture(&f, &forged, &STOCK).is_err());
    let edits = p.edits().unwrap();
    assert_eq!(edits.len(), 2);
    for (i, edit) in edits.iter().enumerate() { p.verify_step(&f, i).unwrap(); apply(&mut f, edit); p.verify_step(&f, i + 1).unwrap(); }
    // Replacing/removing a mod disables future application but cannot prevent
    // restoring the already-owned in-memory names and mapping.
    fs::remove_file(mods.join("levels/Arena pack/floor_test.iff")).unwrap();
    assert!(custom::verify_assets(&selected).is_err());
    assert_eq!(custom::rows(&root, &mods).unwrap()[0]["disabled"], true);
    for edit in p.restore(&f).unwrap() { apply(&mut f, &edit); }
    p.verify_step(&f, 0).unwrap();
    assert_eq!(read(&f, f.base() + MAPPING, 20).unwrap(), STOCK);
    assert_eq!(fs::read(mods.join("levels/Arena pack/arena_test.iff")).unwrap(), original);
    assert!(custom::source(&mods, "../outside.iff").is_err());
    assert!(custom::source(&mods, "C:/outside.iff").is_err());
    assert!(custom::request(&root, &mods, &request).is_err());
    // Court-only imports retain a chosen stock arena shell, and persistence
    // uses the same catalog ID when reopened.
    fs::write(mods.join("levels/Arena pack/floor_test.iff"), b"fixture court").unwrap();
    let court = custom::request(&root, &mods, &serde_json::json!({"action":"add","name":"Court only","baseId":"arena_594_int","arenaSource":null,"floorSource":"levels/Arena pack/floor_test.iff"})).unwrap();
    let court_id = court["id"].as_str().unwrap();
    custom::load(&root).unwrap();
    custom::publish(&root, &mods, &assets, Some(court_id)).unwrap();
    let court_choice = arena::choice(court_id).unwrap();
    assert_eq!(court_choice.file.as_deref(), Some("arena_594_int"));
    assert!(prepare(&f, court_id, &STOCK, None).is_ok());
    assert!(custom::publish(&root, &root.join("Other mods"), &assets, Some(court_id)).is_err());
    assert_ne!(selected.file, court_choice.floor_file.as_ref().map(|s| s.trim_end_matches("_floor").to_string()), "rapid imports must have distinct aliases");
    // Editing replaces the visible card but retains the exact identity/fields
    // of an active session, including after reloading the persistent store.
    let mut edit = request.clone();
    edit["action"] = serde_json::json!("edit"); edit["id"] = serde_json::json!(id);
    edit["name"] = serde_json::json!("Renamed court"); edit["arenaSource"] = serde_json::Value::Null;
    let edited = custom::request(&root, &mods, &edit).unwrap();
    let edited_id = edited["id"].as_str().unwrap();
    assert_ne!(edited_id, id);
    assert_eq!(edited["previousId"], id);
    assert_eq!(custom::rows(&root, &mods).unwrap().len(), 2);
    assert!(!custom::rows(&root, &mods).unwrap().iter().any(|e| e["id"] == id));
    custom::load(&root).unwrap();
    assert_eq!(custom::resolve(id).unwrap(), selected);
    assert!(custom::known_fields(&arena::choice_fields(&selected).unwrap()));
    assert!(custom::request(&root, &mods, &edit).is_err(), "a stale editor cannot overwrite a later edit");
    let saved = custom::request(&root, &mods, &serde_json::json!({"action":"get","id":edited_id})).unwrap();
    assert_eq!(saved["name"], "Renamed court"); assert!(saved["arenaSource"].is_null());
    custom::publish(&root, &mods, &assets, Some(edited_id)).unwrap();
    assert!(prepare(&f, edited_id, &STOCK, None).is_ok());
    let mut invalid_edit = edit.clone(); invalid_edit["id"] = serde_json::json!(edited_id);
    invalid_edit["arenaSource"] = serde_json::json!("arena_outside.iff");
    assert!(custom::request(&root, &mods, &invalid_edit).is_err());
    invalid_edit["arenaSource"] = serde_json::Value::Null; invalid_edit["name"] = serde_json::json!("Court only");
    assert!(custom::request(&root, &mods, &invalid_edit).is_err(), "edits enforce unique names");
    drop(assets);
    fs::remove_dir_all(root).unwrap();
}
const ROOT: u64 = 0x50000000000;
const ALLOCATION: u64 = 0x60000000000;
const OWNER: u64 = ALLOCATION + 0x50;
const TABLE: u64 = ALLOCATION + 0x1000;
pub(crate) struct Fake {
    inner: RulesFake,
    private_kind: u32,
    private_protection: u32,
    image_protection: u32,
    foreign_allocation: bool,
    torn_fields: bool,
    fields_reads: Cell<usize>,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
        let mut bytes = self.inner.read(address, len)?;
        if self.torn_fields && address == TABLE + STRIDE + 0x7E {
            let n = self.fields_reads.get();
            self.fields_reads.set(n + 1);
            if n > 0 {
                bytes[0] ^= 1;
            }
        }
        Ok(bytes)
    }
}
impl Inspect for Fake {
    fn region(&self, address: u64) -> Result<Region> {
        if (self.base() + MAPPING..self.base() + MAPPING + 20).contains(&address) {
            return Ok(Region {
                start: self.base() + MAPPING,
                len: 20,
                allocation: self.base(),
                committed: true,
                protection: self.image_protection,
                kind: 0x1000000,
            });
        }
        ensure!(
            (ALLOCATION..ALLOCATION + 0x800000).contains(&address),
            "Unknown arena region"
        );
        Ok(Region {
            start: ALLOCATION,
            len: 0x800000,
            allocation: ALLOCATION + if self.foreign_allocation { 0x1000 } else { 0 },
            committed: true,
            protection: self.private_protection,
            kind: self.private_kind,
        })
    }
}
fn row(f: &mut Fake, index: u64, id: u16, file: &str, floor: &str) {
    let mut bytes = vec![0; 0x794];
    bytes[0x7E..0x7E + file.len()].copy_from_slice(file.as_bytes());
    bytes[0xA4..0xA4 + floor.len()].copy_from_slice(floor.as_bytes());
    bytes[0x792..0x794].copy_from_slice(&id.to_le_bytes());
    f.inner.put(TABLE + index * STRIDE, &bytes);
}
pub(crate) fn fixture() -> Fake {
    let mut inner = rules_fixture();
    let base = inner.base();
    for &(rva, prefix, target) in roster::RIP_ANCHORS {
        let mut b = prefix.to_vec();
        b.extend_from_slice(
            &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
        );
        inner.put(base + rva, &b);
    }
    for &(rva, b) in roster::FIELD_ANCHORS {
        inner.put(base + rva, b);
    }
    inner.q(roster::ROOT_CLASS, base + 0x024E66A0);
    inner.q(roster::ROOT_CLASS + 8, base + 0x024E6710);
    for anchor in anchors().unwrap()["anchors"].as_array().unwrap() {
        inner.put(
            base + anchor["rva"].as_u64().unwrap(),
            &hex(anchor["hex"].as_str().unwrap()).unwrap(),
        );
    }
    for (rva, text) in [
        (0x07369224, "arena_701_int"),
        (0x07369240, "arena_701_int_floor"),
    ] {
        inner.put(
            base + rva,
            &text
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    inner.q(roster::ROOT, ROOT);
    inner.put(ROOT, &(base + roster::ROOT_CLASS).to_le_bytes());
    inner.put(ROOT + 0xA8, &OWNER.to_le_bytes());
    inner.put(
        OWNER + 0x60,
        &[3u64.to_le_bytes(), TABLE.to_le_bytes()].concat(),
    );
    inner.put(base + MAPPING, &STOCK);
    let mut f = Fake {
        inner,
        private_kind: 0x20000,
        private_protection: 4,
        image_protection: 2,
        foreign_allocation: false,
        torn_fields: false,
        fields_reads: Cell::new(0),
    };
    row(&mut f, 0, 700, "arena_700_int", "arena_700_int_floor");
    row(&mut f, 1, 594, "arena_594_int", "arena_594_int_floor");
    row(&mut f, 2, 0, "arena_000_int", "arena_000_int_floor");
    f
}
impl Fake {
    pub(crate) fn patch(&mut self, at: u64, bytes: &[u8]) {
        self.inner.put(at, bytes);
    }
}
fn apply(f: &mut Fake, edit: &Edit) {
    assert_eq!(read(f, edit.at, edit.before.len()).unwrap(), edit.before);
    f.inner.put(edit.at, &edit.after);
}
fn template_manifest() -> Vec<u8> {
    b"levels/arena_brickleyfacility_int.iff,0A,1,2\nlevels/arena_brickleyfacility_int_floor.iff,0A,3,4\n".to_vec()
}
#[test]
fn arena_v2_all_native_mapping_field_and_typed_literal_proofs_are_required() {
    let mut f = fixture();
    verify_profile(&f).unwrap();
    for anchor in anchors().unwrap()["anchors"].as_array().unwrap() {
        let address = f.base() + anchor["rva"].as_u64().unwrap();
        let bytes = hex(anchor["hex"].as_str().unwrap()).unwrap();
        f.inner.put(address, &[bytes[0] ^ 1]);
        assert!(verify_profile(&f).is_err(), "Accepted {}", anchor["label"]);
        f.inner.put(address, &bytes);
    }
    f.inner.put(f.base() + 0x07369224, &[b'x']);
    assert!(verify_profile(&f).is_err());
}
#[test]
fn arena_v2_normal_arena_changes_only_first_two_mapping_ids_and_restores() {
    let mut f = fixture();
    let p = prepare(&f, "arena_594_int", &STOCK, None).unwrap();
    p.verify_step(&f, 0).unwrap();
    let edits = p.edits().unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].at, f.base() + MAPPING);
    assert_eq!(&edits[0].after[8..], &STOCK[8..]);
    assert_eq!(&edits[0].after[..8], &[594u32.to_le_bytes(); 2].concat());
    assert!(p.template.is_none());
    apply(&mut f, &edits[0]);
    p.verify_step(&f, 1).unwrap();
    let restore = p.restore(&f).unwrap();
    assert_eq!(restore.len(), 1);
    apply(&mut f, &restore[0]);
    p.verify_step(&f, 0).unwrap();
    assert_eq!(read(&f, f.base() + MAPPING, 20).unwrap(), STOCK);
}
#[test]
fn arena_v2_template_requires_both_assets_and_orders_names_before_mapping() {
    let mut f = fixture();
    for manifest in [
        None,
        Some(&b"levels/arena_brickleyfacility_int.iff\n"[..]),
        Some(&b"foreign"[..]),
    ] {
        assert!(prepare(&f, "arena_brickleyfacility_int", &STOCK, manifest).is_err());
    }
    let p = prepare(
        &f,
        "arena_brickleyfacility_int",
        &STOCK,
        Some(&template_manifest()),
    )
    .unwrap();
    let edits = p.edits().unwrap();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits[0].at, TABLE + 0x7E);
    assert_eq!(edits[0].after.len(), 78);
    assert_eq!(edits[1].at, f.base() + MAPPING);
    for (i, edit) in edits.iter().enumerate() {
        p.verify_step(&f, i).unwrap();
        apply(&mut f, edit);
        p.verify_step(&f, i + 1).unwrap();
    }
    let restore = p.restore(&f).unwrap();
    assert_eq!(restore[0].at, f.base() + MAPPING);
    assert_eq!(restore[1].at, TABLE + 0x7E);
    for (i, edit) in restore.iter().enumerate() {
        apply(&mut f, edit);
        p.verify_step(&f, edits.len() - i - 1).unwrap();
    }
}
#[test]
fn arena_v2_unknown_disabled_mismatched_and_duplicate_records_fail_closed() {
    let mut f = fixture();
    for id in [
        "../../arena_594_int",
        "arena_reccenter_lobbyint",
        "arena_allstarteamup_lobbyint",
    ] {
        assert!(prepare(&f, id, &STOCK, None).is_err());
    }
    let mut choice = arena::choice("arena_594_int").unwrap().clone();
    choice.arena_id = Some(595);
    assert!(capture(&f, &choice, &STOCK).is_err());
    row(&mut f, 2, 594, "arena_594_int", "arena_594_int_floor");
    assert!(prepare(&f, "arena_594_int", &STOCK, None).is_err());
    let mut f = fixture();
    row(&mut f, 0, 700, "arena_700_int", "foreign_floor");
    assert!(
        prepare(
            &f,
            "arena_brickleyfacility_int",
            &STOCK,
            Some(&template_manifest())
        )
        .is_err()
    );
}
#[test]
fn arena_v2_foreign_mapping_root_class_fields_or_protection_are_not_adopted() {
    for mutation in 0..8 {
        let mut f = fixture();
        match mutation {
            0 => f.private_kind = 0x1000000,
            1 => f.private_protection = 2,
            2 => f.image_protection = 4,
            3 => f.foreign_allocation = true,
            4 => f
                .inner
                .put(ROOT, &(f.base() + roster::ROOT_CLASS + 8).to_le_bytes()),
            5 => f.inner.put(f.base() + MAPPING + 8, &[1]),
            6 => f.inner.put(OWNER + 0x60, &2049u64.to_le_bytes()),
            _ => f.inner.put(TABLE + STRIDE + 0xA4, b"foreign_floor\0"),
        }
        assert!(
            prepare(&f, "arena_594_int", &STOCK, None).is_err(),
            "Accepted mutation {mutation}"
        );
    }
    let mut f = fixture();
    let p = prepare(&f, "arena_594_int", &STOCK, None).unwrap();
    f.inner.put(TABLE + STRIDE + 0x7E, b"foreign");
    assert!(p.verify_step(&f, 0).is_err());
    let mut torn = fixture();
    torn.torn_fields = true;
    assert!(prepare(&torn, "arena_594_int", &STOCK, None).is_err());
}
#[test]
fn arena_v2_loading_match_main_and_quick_play_never_get_arena_edits_but_rearm_at_selection() {
    let mut f = fixture();
    let p = prepare(&f, "arena_594_int", &STOCK, None).unwrap();
    apply(&mut f, &p.edits().unwrap()[0]);
    for (mode, stage, active) in [
        (0x92DF06DE, 2, 0),
        (0xB4D37B22, 1, 1),
        (0x6509837B, 1, 0),
        (0x079964C0, 1, 0),
    ] {
        f.inner.d(0xAD07408 + 64, mode);
        f.inner.d(0xAF46870 + 0xEA8, stage);
        f.inner.put(f.base() + 0xAB5D790, &[active]);
        assert!(p.restore(&f).is_err());
        assert!(prepare(&f, "arena_594_int", &p.target_mapping, None).is_err());
        f.inner.d(0xAD07408 + 64, 0x92DF06DE);
        f.inner.d(0xAF46870 + 0xEA8, 3);
        f.inner.put(f.base() + 0xAB5D790, &[0]);
        p.verify_step(&f, 1).unwrap();
        let again = prepare(&f, "arena_594_int", &p.target_mapping, None).unwrap();
        assert!(again.edits().unwrap().is_empty());
    }
}
#[test]
fn arena_v2_tampered_or_foreign_partial_transactions_do_not_restore_blindly() {
    let mut f = fixture();
    let p = prepare(
        &f,
        "arena_brickleyfacility_int",
        &STOCK,
        Some(&template_manifest()),
    )
    .unwrap();
    for mutation in 0..4 {
        let mut altered = p.clone();
        match mutation {
            0 => altered.target_mapping[12] ^= 1,
            1 => altered.template.as_mut().unwrap()[0] = b'x',
            2 => altered.selected = "arena_594_int".into(),
            _ => altered.snapshot.record.as_mut().unwrap().address += STRIDE,
        }
        assert!(altered.verify_step(&f, 0).is_err());
    }
    apply(&mut f, &p.edits().unwrap()[0]);
    assert!(p.verify_step(&f, 0).is_err());
    p.verify_step(&f, 1).unwrap();
    f.inner.put(TABLE + 0x7E, b"foreign");
    assert!(p.verify_step(&f, 1).is_err());
    assert!(p.restore(&f).is_err());
}
#[test]
#[ignore = "explicit pinned Version 2 arena disk audit; no game process access or file changes"]
fn arena_v2_actual_installed_disk_profile() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let path = std::env::var_os("C27_RULES_V2_DISK").expect("Explicit game EXE path required");
    let mut f = std::fs::File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = f.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    assert_eq!(format!("{:X}", digest.finalize()), SHA);
    struct Disk(std::cell::RefCell<std::fs::File>);
    impl Memory for Disk {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
            let rva = address - self.base();
            let offset = if rva == 0 {
                0
            } else if rva >= 0x898C000 {
                144222720 + rva - 0x898C000
            } else if rva >= 97464320 {
                97460224 + rva - 97464320
            } else {
                1536 + rva - 4096
            };
            let mut f = self.0.borrow_mut();
            f.seek(SeekFrom::Start(offset))?;
            let mut b = vec![0; len];
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    let disk = Disk(std::cell::RefCell::new(f));
    verify_profile(&disk).unwrap();
    assert_eq!(read(&disk, disk.base() + MAPPING, 20).unwrap(), STOCK);
    println!(
        "Verified exact Version 2 arena mapping, four native consumers, root/class/directory, arenaID and typed filename/floor capacity proofs; disk only, no process access or game writes"
    );
}
