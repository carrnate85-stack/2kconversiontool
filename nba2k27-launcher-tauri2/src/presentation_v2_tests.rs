use super::*;
use crate::rules_v2::tests::{Fake, fixture as rules_fixture};
fn fixture() -> Fake {
    let mut f = rules_fixture();
    let value = catalog().unwrap();
    for anchor in value["anchors"].as_array().unwrap() {
        f.put(
            f.base() + anchor["rva"].as_u64().unwrap(),
            &hex(anchor["hex"].as_str().unwrap()).unwrap(),
        );
    }
    f.q(SETTINGS_OWNER, 0x40000000000);
    f.put(0x40000000000, &[0]);
    let mut window = vec![0; 16];
    window[1] = 1;
    f.put(0x40000000000 + WINDOW, &window);
    f.q(PRELOAD_TABLE, 0x50000000000);
    for index in 175u32..=179 {
        let row = 0x50000000000 + index as u64 * STRIDE + 8;
        f.put(row, &index.to_le_bytes());
        let mut front = vec![0; 24];
        front[1] = 0x4C;
        front[8] = 1;
        f.put(row + FRONT, &front);
        let mut ambient = vec![0; 24];
        ambient[1] = 0x4D;
        ambient[8] = 1;
        f.put(row + AMBIENT, &ambient);
    }
    f
}
fn apply(f: &mut Fake, transaction: &Transaction) {
    verify_expected(f, &transaction.before).unwrap();
    for change in &transaction.changes {
        assert_eq!(f.read(change.at, 1).unwrap(), [change.before]);
        f.put(change.at, &[change.after]);
    }
    verify_expected(f, &transaction.after).unwrap();
}
#[test]
fn presentation_v2_eight_masks_keep_mc_coupling_and_never_touch_game_data() {
    for mask in 0..8 {
        let mut f = fixture();
        let before = capture(&f).unwrap();
        let p = presentation(&before, mask).unwrap();
        assert_eq!(
            p.changes.len(),
            (mask & 1 != 0) as usize + (mask & 2 != 0) as usize + 2 * (mask & 4 != 0) as usize
        );
        assert!(p.changes.iter().all(|e| e.at >= before.owner + WINDOW + 6
            && e.at <= before.owner + WINDOW + 11
            && e.after == 1));
        apply(&mut f, &p);
        assert_eq!(p.after.window[1], 1);
        assert_eq!(p.after.sources, before.sources);
        for index in [6, 7, 8, 11] {
            let bit = if index == 6 {
                1
            } else if index == 7 {
                2
            } else {
                4
            };
            assert_eq!(p.after.window[index], (mask & bit != 0) as u8);
        }
    }
    assert!(presentation(&capture(&fixture()).unwrap(), 8).is_err());
}
#[test]
fn presentation_v2_six_crowd_writes_cover_all_five_match_sizes_and_restore_at_both_exit_menus() {
    for menu in [0x6509837Bu32, 0x079964C0] {
        let mut f = fixture();
        let original = capture(&f).unwrap();
        let p = crowd(&original).unwrap();
        assert_eq!(p.changes.len(), 6);
        assert_eq!(p.changes[0].kind, Kind::CrowdGlobal);
        for index in 175..=179 {
            assert!(
                p.changes
                    .iter()
                    .any(|c| c.kind == Kind::CrowdStanding(index))
            );
        }
        apply(&mut f, &p);
        assert!(restore_crowd(&capture(&f).unwrap(), &p).is_err());
        f.d(0xAD07408 + 64, menu);
        f.d(0xAF46870 + 0xEA8, 1);
        let current = capture(&f).unwrap();
        assert!(presentation(&current, 7).is_err());
        assert!(crowd(&current).is_err());
        let restore = restore_crowd(&current, &p).unwrap();
        assert_eq!(restore.changes.len(), 6);
        apply(&mut f, &restore);
        let after = capture(&f).unwrap();
        assert_eq!(after.window, original.window);
        assert_eq!(after.sources, original.sources);
        assert_eq!(restore_crowd(&after, &p).unwrap().changes.len(), 0);
    }
}
#[test]
fn presentation_v2_foreign_owners_tags_and_neighbor_changes_refuse_restore_without_edits() {
    let mut f = fixture();
    let original = capture(&f).unwrap();
    let p = crowd(&original).unwrap();
    apply(&mut f, &p);
    f.d(0xAD07408 + 64, 0x079964C0);
    f.d(0xAF46870 + 0xEA8, 1);
    let current = capture(&f).unwrap();
    let mut changed = current.clone();
    changed.owner += 8;
    assert!(restore_crowd(&changed, &p).is_err());
    let mut changed = current.clone();
    changed.preload_table += 8;
    assert!(restore_crowd(&changed, &p).is_err());
    let mut changed = current.clone();
    changed.window[3] ^= 1;
    assert!(restore_crowd(&changed, &p).is_err());
    let mut changed = current.clone();
    changed.sources[3].front[9] ^= 1;
    assert!(restore_crowd(&changed, &p).is_err());
    let mut changed = current.clone();
    changed.sources[3].ambient[8] ^= 1;
    assert!(restore_crowd(&changed, &p).is_err());
    let mut changed = current.clone();
    changed.sources[3].address += 8;
    assert!(restore_crowd(&changed, &p).is_err());
    f.put(current.sources[1].address, &999u32.to_le_bytes());
    assert!(capture(&f).is_err());
}
#[test]
fn presentation_v2_every_partial_crowd_preparation_can_restore_only_its_owned_values() {
    for prefix in 0..=6 {
        let mut f = fixture();
        let original = capture(&f).unwrap();
        let p = crowd(&original).unwrap();
        for change in &p.changes[..prefix] {
            f.put(change.at, &[change.after]);
        }
        f.d(0xAD07408 + 64, 0x6509837B);
        f.d(0xAF46870 + 0xEA8, 1);
        let restore = restore_crowd(&capture(&f).unwrap(), &p).unwrap();
        assert_eq!(restore.changes.len(), prefix);
        apply(&mut f, &restore);
        let after = capture(&f).unwrap();
        assert_eq!(after.window, original.window);
        assert_eq!(after.sources, original.sources);
    }
}
#[test]
fn presentation_v2_match_and_load_never_read_transient_preload_owners() {
    let mut f = fixture();
    f.d(0xAD07408 + 64, 0xB4D37B22);
    f.d(0xAF46870 + 0xEA8, 1);
    f.q(SETTINGS_OWNER, 0);
    f.q(PRELOAD_TABLE, 0);
    let error = capture(&f).unwrap_err().to_string();
    assert!(error.contains("waits for stable"));
    let mut f = fixture();
    f.d(0xAF46870 + 0xEA8, 2);
    f.q(SETTINGS_OWNER, 0);
    f.q(PRELOAD_TABLE, 0);
    assert!(
        capture(&f)
            .unwrap_err()
            .to_string()
            .contains("waits for stable")
    );
}
#[test]
fn presentation_v2_all_native_getter_table_and_source_proofs_are_required() {
    let mut f = fixture();
    verify_profile(&f).unwrap();
    for anchor in catalog().unwrap()["anchors"].as_array().unwrap() {
        let address = f.base() + anchor["rva"].as_u64().unwrap();
        let bytes = hex(anchor["hex"].as_str().unwrap()).unwrap();
        f.put(address, &[bytes[0] ^ 1]);
        assert!(
            verify_profile(&f).is_err(),
            "Accepted changed {}",
            anchor["label"]
        );
        f.put(address, &bytes);
    }
}
#[test]
fn presentation_v2_already_removed_crowd_does_not_claim_or_mutate_anything() {
    let mut f = fixture();
    let first = crowd(&capture(&f).unwrap()).unwrap();
    apply(&mut f, &first);
    let p = crowd(&capture(&f).unwrap()).unwrap();
    assert!(p.changes.is_empty());
    assert_eq!(p.before, p.after);
}
#[test]
fn presentation_v2_malformed_snapshots_fail_without_indexing_or_address_panics() {
    let original = capture(&fixture()).unwrap();
    for length in [0, 1, 6, 11, 15, 17] {
        let mut s = original.clone();
        s.window.resize(length, 0);
        assert!(presentation(&s, 7).is_err());
        assert!(crowd(&s).is_err());
    }
    for length in [0, 1, 23, 25] {
        let mut s = original.clone();
        s.sources[0].front.resize(length, 0);
        assert!(presentation(&s, 7).is_err());
        assert!(crowd(&s).is_err());
    }
    let mut s = original.clone();
    s.preload_table = u64::MAX;
    assert!(presentation(&s, 7).is_err());
    assert!(crowd(&s).is_err());
}
#[test]
fn presentation_v2_drone_dj_and_mc_do_not_depend_on_crowd_preload_initialization() {
    let mut f = fixture();
    f.q(PRELOAD_TABLE, 0);
    assert!(capture(&f).is_err());
    let actors = capture_actors(&f).unwrap();
    assert!(actors.sources.is_empty());
    assert_eq!(actors.preload_table, 0);
    let p = presentation(&actors, 7).unwrap();
    assert_eq!(p.changes.len(), 4);
    apply(&mut f, &p);
    assert!(crowd(&actors).is_err());
}
#[test]
#[ignore = "explicit pinned Version 2 presentation disk audit; no process access or game file changes"]
fn presentation_v2_actual_installed_disk_profile() {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    let path = std::env::var_os("C27_RULES_V2_DISK").expect("Explicit game EXE path required");
    let mut f = std::fs::File::open(path).unwrap();
    let mut h = Sha256::new();
    let mut b = vec![0; 1024 * 1024];
    loop {
        let n = f.read(&mut b).unwrap();
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    assert_eq!(format!("{:X}", h.finalize()), SHA);
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
    verify_profile(&Disk(std::cell::RefCell::new(f))).unwrap();
    println!(
        "Verified Version2 crowd/drone/DJ/MC getters, preset-table owner, C810 stride, all-five-size selection and standing crowd source consumption; disk-only, zero game changes"
    );
}
/// Fake whose Blacktop "active" byte blips to 1 on chosen reads, as observed
/// live at Version 2 selection (with kind/gameMode moving the same instant).
struct Blip {
    inner: Fake,
    reads: std::cell::Cell<usize>,
    blips: Vec<usize>,
}
impl Memory for Blip {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        let mut bytes = self.inner.read(at, len)?;
        if at == self.inner.base() + 0x0AB5D790 && len == 1 {
            let n = self.reads.get();
            self.reads.set(n + 1);
            if self.blips.contains(&n) {
                bytes[0] = 1;
            }
        }
        Ok(bytes)
    }
}
#[test]
fn presentation_v2_selection_context_blips_are_debounced_but_real_changes_are_not() {
    let stable = capture_actors(&fixture()).unwrap();
    // One blip in every capture sample group still yields the exact snapshot.
    let blip = Blip {
        inner: fixture(),
        reads: Default::default(),
        blips: (0..60).step_by(4).collect(),
    };
    assert_eq!(capture_actors(&blip).unwrap(), stable);
    verify_expected(&blip, &stable).unwrap();
    // A blip that wins the majority is treated as the real state: not selection.
    let mostly = Blip {
        inner: fixture(),
        reads: Default::default(),
        blips: vec![0, 2, 4],
    };
    assert_eq!(settled_context(&mostly).unwrap().active, 1);
    // A persistent change is a real boundary change.
    let gone = Blip {
        inner: fixture(),
        reads: Default::default(),
        blips: (0..200).collect(),
    };
    assert!(capture_actors(&gone).is_err());
}
