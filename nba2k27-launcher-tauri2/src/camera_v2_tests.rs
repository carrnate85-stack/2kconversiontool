use super::*;
use std::{cell::Cell, collections::BTreeMap};

#[test]
fn live_header_accepts_only_preferred_or_exact_retained_base() {
    let base = 0x7ff7_4ea9_0000;
    let mut m = fixture(base);
    verify_profile(&m).unwrap();
    m.put(base + 0x80 + 48, &base.to_le_bytes());
    verify_profile(&m).unwrap();
    let loaded = read(&m, base, 4096).unwrap();
    assert!(
        header(&loaded, PREFERRED).is_err(),
        "Disk header must remain strict"
    );
    for other in [0, base + 0x10000, base - 0x10000, u64::MAX] {
        m.put(base + 0x80 + 48, &other.to_le_bytes());
        assert!(verify_profile(&m).is_err(), "Foreign header base {other:X}");
    }
    m.put(base + 0x80 + 48, &base.to_le_bytes());
    m.put(base + 0x80 + 8, &0x6aa2_013du32.to_le_bytes());
    assert!(
        verify_profile(&m).is_err(),
        "Relocation must not bypass build identity"
    );
}
struct Fake {
    base: u64,
    data: BTreeMap<u64, u8>,
    reads: Cell<usize>,
    torn: Cell<bool>,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        self.base
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(len <= 4096, "Unbounded camera read");
        let n = self.reads.get();
        self.reads.set(n + 1);
        let mut b: Vec<u8> = (0..len)
            .map(|i| {
                self.data
                    .get(&(at + i as u64))
                    .copied()
                    .context("Missing fixture address")
            })
            .collect::<Result<_>>()?;
        if self.torn.get() && at == self.base + MODES && n > 30 {
            b[0] ^= 1;
        }
        Ok(b)
    }
}
impl Fake {
    fn put(&mut self, at: u64, b: &[u8]) {
        for (i, value) in b.iter().enumerate() {
            self.data.insert(at + i as u64, *value);
        }
    }
    fn apply(&mut self, edits: &[Edit]) {
        for e in edits {
            assert_eq!(read(self, e.at, e.before.len()).unwrap(), e.before);
            self.put(e.at, &e.after);
        }
    }
    fn initialize(&mut self, plan: &Plan) {
        for init in &plan.initialization {
            self.put(init.at, &init.bytes);
        }
    }
}
fn fixture(base: u64) -> Fake {
    let mut fake = Fake {
        base,
        data: BTreeMap::new(),
        reads: Cell::new(0),
        torn: Cell::new(false),
    };
    let mut h = vec![0; 4096];
    h[..2].copy_from_slice(b"MZ");
    h[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    let pe = 0x80;
    h[pe..pe + 4].copy_from_slice(b"PE\0\0");
    h[pe + 4..pe + 6].copy_from_slice(&0x8664u16.to_le_bytes());
    h[pe + 6..pe + 8].copy_from_slice(&3u16.to_le_bytes());
    h[pe + 8..pe + 12].copy_from_slice(&0x6AD2E48Cu32.to_le_bytes());
    h[pe + 20..pe + 22].copy_from_slice(&240u16.to_le_bytes());
    h[pe + 24..pe + 26].copy_from_slice(&0x20Bu16.to_le_bytes());
    h[pe + 48..pe + 56].copy_from_slice(&PREFERRED.to_le_bytes());
    h[pe + 80..pe + 84].copy_from_slice(&(IMAGE_BYTES as u32).to_le_bytes());
    for (i, (rva, len, flags)) in [
        (0x1000u32, 0x5D00000u32 - 0x1000, 0x60000000u32),
        (0x5D00000, 0x898C000 - 0x5D00000, 0x40000000),
        (0x898C000, 0xC000000 - 0x898C000, 0xC0000000),
    ]
    .into_iter()
    .enumerate()
    {
        let start = pe + 24 + 240 + i * 40;
        h[start + 8..start + 12].copy_from_slice(&len.to_le_bytes());
        h[start + 12..start + 16].copy_from_slice(&rva.to_le_bytes());
        h[start + 16..start + 20].copy_from_slice(&len.to_le_bytes());
        h[start + 36..start + 40].copy_from_slice(&flags.to_le_bytes());
    }
    fake.put(base, &h);
    for &(rva, bytes) in ANCHORS {
        fake.put(base + rva, bytes);
    }
    let stock = stock().unwrap();
    let mut registry = stock_bytes(&stock, "registry", 72 * 64).unwrap();
    for id in 0..72 {
        let entry = relocate(
            registry[id * 64..id * 64 + 64].to_vec(),
            base,
            &[0x10, 0x18, 0x30, 0x38],
        )
        .unwrap();
        registry[id * 64..id * 64 + 64].copy_from_slice(&entry);
    }
    fake.put(base + REGISTRY, &registry);
    fake.put(
        base + LABELS,
        &stock_bytes(&stock, "labels", 72 * 4).unwrap(),
    );
    fake.put(
        base + STATIC_AVAILABILITY,
        &stock_bytes(&stock, "staticAvailability", 72).unwrap(),
    );
    fake.put(base + RUNTIME_AVAILABILITY, &[0; 72]);
    let mut modes = stock_bytes(&stock, "modes", 13 * 16).unwrap();
    for i in 0..13 {
        let bytes = relocate(modes[i * 16..i * 16 + 16].to_vec(), base, &[0]).unwrap();
        modes[i * 16..i * 16 + 16].copy_from_slice(&bytes);
        fake.put(
            q(&bytes, 0),
            &MODE_IDS[i]
                .iter()
                .flat_map(|id| id.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    }
    fake.put(base + MODES, &modes);
    for id in PARAMETER_IDS {
        for offset in [0x30, 0x38] {
            let address = q(&registry, id as usize * 64 + offset);
            for p in address..address + policy::PARAMETER_BYTES as u64 {
                fake.data.entry(p).or_insert((p % 251) as u8);
            }
            for (i, start) in [0x10, 0x30].into_iter().enumerate() {
                fake.put(
                    address + start,
                    &hex(stock["framing"][id.to_string()][i].as_str().unwrap()).unwrap(),
                );
            }
        }
    }
    for id in [10, 51, 52, 53, 55] {
        let pointer = q(&registry, id * 64 + 0x10);
        let mut bytes = vec![0; 52];
        for (index, value) in [1u32, 1, 1, 10, 1, 10].into_iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        fake.put(pointer, &bytes);
    }
    fake
}
fn choices() -> Vec<Selection> {
    policy::CURATED
        .iter()
        .map(|(id, _)| Selection {
            id: *id,
            steps: Steps::new(1, 1).unwrap(),
        })
        .collect()
}
fn plan(fake: &Fake) -> Plan {
    capture(fake)
        .unwrap()
        .plan(&choices(), 0x70000000000)
        .unwrap()
}
#[test]
fn initialized_unpublished_camera_plan_is_reusable_without_reallocation() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    let prepared = f.data.clone();
    for _ in 0..5 {
        assert_eq!(p.publication(&f).unwrap(), Publication::FullyReset);
        p.compare_private(&f).unwrap();
        p.compare_before(&f).unwrap();
        assert_eq!(
            f.data, prepared,
            "Waiting must not initialize or publish again"
        );
    }
    f.apply(&p.edits);
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);
}
#[test]
fn camera_v2_profile_rejects_wrong_build_before_old_address_reads() {
    let mut f = fixture(PREFERRED);
    for (stamp, size) in [(0x6AA2013Du32, 0x36558000u32), (0x6AC53765, 0x36558000)] {
        f.put(f.base + 0x88, &stamp.to_le_bytes());
        f.put(f.base + 0xD0, &size.to_le_bytes());
        f.reads.set(0);
        assert!(verify_profile(&f).is_err());
        assert_eq!(f.reads.get(), 1);
    }
}
#[test]
fn camera_v2_all_immutable_anchors_are_independently_required() {
    let mut f = fixture(PREFERRED);
    verify_profile(&f).unwrap();
    for &(rva, bytes) in ANCHORS {
        let mut wrong = bytes.to_vec();
        wrong[0] ^= 1;
        f.put(f.base + rva, &wrong);
        assert!(verify_profile(&f).is_err(), "{rva:X}");
        f.put(f.base + rva, bytes);
    }
    let start = 0x80 + 24 + 240 + 2 * 40 + 36;
    f.put(f.base + start as u64, &0xE0000000u32.to_le_bytes());
    assert!(verify_profile(&f).is_err());
}
#[test]
fn camera_v2_plan_matches_tracking_and_preserves_unselected_modes_and_source_bytes() {
    let mut f = fixture(0x180000000);
    let captured = capture(&f).unwrap();
    let p = captured.plan(&choices(), 0x70000000000).unwrap();
    assert_eq!(
        p.allocation_bytes,
        choices().len() * SLOT_STRIDE + 13 * 4096
    );
    assert_eq!(p.edits.len(), 7 * 4 + 13 * 2);
    assert_eq!(p.slots.len(), 7);
    let original = f.data.clone();
    p.compare_before(&f).unwrap();
    f.initialize(&p);
    f.apply(&p.edits);
    verify_profile(&f).unwrap();
    for id in SOURCE_IDS {
        assert_eq!(
            read(&f, f.base + REGISTRY + id as u64 * 64, 64).unwrap(),
            captured.entry(id)
        );
    }
    for (index, slot) in p.slots.iter().enumerate() {
        assert_eq!(
            q(&slot.entry, 0),
            crate::camera_names::key(slot.id).unwrap()
        );
        assert_eq!(slot.entry_at, f.base + REGISTRY + (63 + index as u64) * 64);
        if [51, 52, 53, 55].contains(&slot.id) {
            let metadata = read(&f, q(&slot.entry, 0x10), 52).unwrap();
            assert_eq!(d(&metadata, 4), 0);
            assert_eq!(q(&slot.entry, 0x18), q(&slot.entry, 0x10) + 0x24);
        }
    }
    for i in 0..13 {
        let descriptor = read(&f, f.base + MODES + i * 16, 12).unwrap();
        let pointer = q(&descriptor, 0);
        let count = d(&descriptor, 8) as usize;
        let ids = read(&f, pointer, count * 4).unwrap();
        assert_eq!(
            ids[count * 4 - 28..],
            (63u32..70).flat_map(u32::to_le_bytes).collect::<Vec<_>>()
        );
        assert_eq!(
            read(&f, f.base + MODES + i * 16 + 12, 4).unwrap(),
            vec![0; 4]
        );
    }
    let restore = p.restore(&f).unwrap();
    assert_eq!(restore[0].kind, EditKind::ModeCount);
    assert_eq!(restore[1].kind, EditKind::ModePointer);
    f.apply(&restore);
    for (address, byte) in original {
        assert_eq!(
            f.data[&address], byte,
            "Original address {address:X} not restored"
        );
    }
    // Private pages intentionally remain alive after image pointer restoration.
    assert!(f.data.contains_key(&p.private_base));
}
#[test]
fn camera_v2_unknown_duplicate_fractional_and_image_overlapping_allocations_rejected() {
    let f = fixture(PREFERRED);
    let c = capture(&f).unwrap();
    assert!(required_bytes(&[]).is_err());
    let one = Selection {
        id: 51,
        steps: Steps::new(1, 1).unwrap(),
    };
    assert!(required_bytes(&[one.clone(), one.clone()]).is_err());
    assert!(
        required_bytes(&[Selection {
            id: 21,
            ..one.clone()
        }])
        .is_err()
    );
    assert!(
        required_bytes(&[Selection {
            id: 999,
            ..one.clone()
        }])
        .is_err()
    );
    for address in [
        0,
        0xffff,
        PREFERRED,
        PREFERRED + IMAGE_BYTES - 4096,
        0x70000000001,
        u64::MAX - 4095,
    ] {
        assert!(c.plan(&[one.clone()], address).is_err());
    }
    assert!(Steps::from_stored(1.1, 1.).is_err());
}
#[test]
fn camera_v2_stock_capture_rejects_foreign_owner_malformed_geometry_and_torn_modes() {
    let mut f = fixture(PREFERRED);
    let original = f.data.clone();
    for rva in [
        REGISTRY + 63 * 64,
        REGISTRY + 51 * 64 + 0x30,
        LABELS + 63 * 4,
        STATIC_AVAILABILITY + 63,
        MODES,
        0x9984610 + 0x14,
    ] {
        f.data = original.clone();
        *f.data.get_mut(&(f.base + rva)).unwrap() ^= 1;
        assert!(capture(&f).is_err(), "Accepted changed {rva:X}");
    }
    f.data = original;
    f.reads.set(0);
    f.torn.set(true);
    assert!(capture(&f).is_err());
}
#[test]
fn camera_v2_restore_leaves_foreign_changes_untouched_and_handles_partial_publication() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    // Simulate a write failure after the first three independent operations.
    f.apply(&p.edits[..3]);
    let restore = p.restore(&f).unwrap();
    assert_eq!(restore.len(), 3);
    f.apply(&restore);
    p.compare_before(&f).unwrap();
    f.apply(&p.edits);
    let image = f.data.clone();
    let slot = p.slots[0].entry_at;
    f.put(slot + 0x20, &99u32.to_le_bytes());
    f.put(slot + 0x28, &123u32.to_le_bytes());
    assert!(p.restore(&f).is_ok());
    f.put(slot, &0x1234567812345678u64.to_le_bytes());
    let before = f.data.clone();
    assert!(p.restore(&f).is_err());
    assert_eq!(f.data, before);
    f.data = image;
    let e = p.edits.iter().find(|e| e.kind == EditKind::Label).unwrap();
    f.put(e.at, &[1, 2, 3, 4]);
    let before = f.data.clone();
    assert!(p.restore(&f).is_err());
    assert_eq!(f.data, before);
}
#[test]
fn camera_v2_height_zoom_maintenance_is_private_only_unless_lock_is_explicit() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    f.apply(&p.edits);
    assert!(p.maintenance(&f, false).unwrap().is_empty());
    let low = &p.slots[0];
    f.put(low.entry_at + 0x20, &4u32.to_le_bytes());
    f.put(low.entry_at + 0x24, &3u32.to_le_bytes());
    let edits = p.maintenance(&f, false).unwrap();
    assert!(!edits.is_empty());
    assert!(edits.iter().all(|e| e.kind == EditKind::PrivateGeometry
        && e.at >= p.private_base
        && e.at + 4 <= p.private_base + p.allocation_bytes as u64));
    f.apply(&edits);
    assert_eq!(d(&read(&f, low.entry_at, 64).unwrap(), 0x20), 4);
    assert!(p.maintenance(&f, false).unwrap().is_empty());
    let locked = p.maintenance(&f, true).unwrap();
    assert!(
        locked
            .iter()
            .any(|e| e.kind == EditKind::Registry && e.at == low.entry_at + 0x20)
    );
    f.apply(&locked);
    assert_eq!(d(&read(&f, low.entry_at, 64).unwrap(), 0x20), 1);
    f.put(low.entry_at + 0x20, &101u32.to_le_bytes());
    assert!(p.maintenance(&f, false).is_err());
}

#[test]
fn camera_v2_publication_classifies_active_reset_transition_and_conflict() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    assert_eq!(p.publication(&f).unwrap(), Publication::FullyReset);
    f.apply(&p.edits);
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);

    let slot = p.slots[0].entry_at;
    f.put(slot + 0x20, &4u32.to_le_bytes());
    f.put(slot + 0x24, &3u32.to_le_bytes());
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);

    let runtime = p
        .edits
        .iter()
        .find(|edit| edit.kind == EditKind::RuntimeAvailability)
        .unwrap();
    // Runtime availability and the native settings block are game-owned.
    f.put(runtime.at, &runtime.before);
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);
    f.put(slot + 0x28, &[0x5a; 8]);
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);
    f.put(runtime.at, &[2]);
    assert_eq!(p.publication(&f).unwrap(), Publication::Conflict);
    f.put(runtime.at, &runtime.after);
    let mode = p
        .edits
        .iter()
        .find(|edit| edit.kind == EditKind::ModeCount)
        .unwrap();
    f.put(mode.at, &mode.before);
    assert_eq!(p.publication(&f).unwrap(), Publication::Transitional);

    for edit in p.edits.iter().rev() {
        f.put(edit.at, &edit.before);
    }
    assert_eq!(p.publication(&f).unwrap(), Publication::FullyReset);

    let label = p
        .edits
        .iter()
        .find(|edit| edit.kind == EditKind::Label)
        .unwrap();
    f.put(label.at, &[0xff; 4]);
    assert_eq!(p.publication(&f).unwrap(), Publication::Conflict);
}

#[test]
fn camera_v2_every_changed_public_edit_is_transition_or_conflict_not_active() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    f.apply(&p.edits);
    for edit in p.edits.iter().filter(|edit| edit.before != edit.after) {
        if edit.kind == EditKind::RuntimeAvailability {
            f.put(edit.at, &edit.before);
            assert_eq!(p.publication(&f).unwrap(), Publication::Active);
            f.put(edit.at, &[2]);
            assert_eq!(p.publication(&f).unwrap(), Publication::Conflict);
            f.put(edit.at, &edit.after);
            continue;
        }
        f.put(edit.at, &edit.before);
        assert_eq!(
            p.publication(&f).unwrap(),
            Publication::Transitional,
            "Reset {:?} edit was incorrectly reported active",
            edit.kind
        );
        f.put(edit.at, &edit.after);

        let mut foreign = edit.after.clone();
        let offset = if edit.kind == EditKind::Registry {
            0
        } else {
            foreign.len() - 1
        };
        foreign[offset] = (0u8..=u8::MAX)
            .find(|value| *value != edit.before[offset] && *value != edit.after[offset])
            .unwrap();
        f.put(edit.at, &foreign);
        assert_eq!(
            p.publication(&f).unwrap(),
            Publication::Conflict,
            "Foreign {:?} edit was incorrectly accepted",
            edit.kind
        );
        f.put(edit.at, &edit.after);
    }
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);
}

#[test]
fn camera_v2_rearm_requires_owned_private_lists_metadata_and_geometry() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    p.compare_private(&f).unwrap();

    let parameter = p
        .slots
        .iter()
        .find_map(|slot| slot.parameters.first().map(|(at, _)| (slot, *at)))
        .unwrap();
    f.put(parameter.1 + 0x14, &1234.0f32.to_le_bytes());
    assert!(
        p.compare_private(&f).is_err(),
        "Unknown geometry was adopted"
    );
    let valid = policy::stepped_parameters(
        parameter.0.id,
        &parameter.0.parameters[0].1,
        Steps::new(-100, 100).unwrap(),
    )
    .unwrap();
    for offset in [0x14, 0x18, 0x20, 0x34, 0x38, 0x40] {
        f.put(parameter.1 + offset as u64, &valid[offset..offset + 4]);
    }
    p.compare_private(&f).unwrap();

    let original = f.data[&(parameter.1 + 0x24)];
    f.put(parameter.1 + 0x24, &[original ^ 1]);
    assert!(
        p.compare_private(&f).is_err(),
        "Non-geometry private parameter corruption was adopted"
    );
    f.put(parameter.1 + 0x24, &[original]);

    let list = p
        .initialization
        .iter()
        .find(|init| {
            !p.slots
                .iter()
                .any(|slot| slot.parameters.iter().any(|(at, _)| *at == init.at))
        })
        .unwrap();
    let original = f.data[&list.at];
    f.put(list.at, &[original ^ 1]);
    assert!(
        p.compare_private(&f).is_err(),
        "Private mode-list or metadata corruption was adopted"
    );
}
#[test]
fn camera_v2_publication_requires_unchanged_preconditions() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    p.compare_before(&f).unwrap();
    for address in [
        f.base + MODES + 8,
        f.base + REGISTRY + 10 * 64 + 0x30,
        f.base + 0x997E720 + 0x14,
        f.base + 0x996566C + 4,
    ] {
        let old = f.data[&address];
        f.put(address, &[old ^ 1]);
        assert!(
            p.compare_before(&f).is_err(),
            "Camera source {address:X} changed after capture"
        );
        f.put(address, &[old]);
    }
}
#[test]
fn camera_v2_interrupted_publication_with_game_owned_drift_restores_and_replans_in_place() {
    // The 0.44 live failure: after a public prefix, the game rewrote the
    // settings blocks of written slots and cleared their runtime flags.
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    for prefix in 1..=p.edits.len() {
        f.apply(&p.edits[..prefix]);
        for edit in &p.edits[..prefix] {
            match edit.kind {
                EditKind::Registry => f.put(edit.at + 0x20, &[0x33; 16]),
                EditKind::RuntimeAvailability => f.put(edit.at, &edit.before),
                _ => {}
            }
        }
        let restore = p.restore(&f).unwrap();
        f.apply(&restore);
        assert_eq!(p.publication(&f).unwrap(), Publication::FullyReset);
        let fresh = capture(&f)
            .unwrap()
            .plan(&choices(), p.private_base)
            .unwrap();
        assert!(p.same_private_layout(&fresh));
        fresh.compare_private(&f).unwrap();
        fresh.compare_before(&f).unwrap();
    }
    f.apply(&p.edits);
    assert_eq!(p.publication(&f).unwrap(), Publication::Active);
}
#[test]
fn camera_v2_replan_into_another_allocation_is_not_the_same_private_layout() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    let moved = capture(&f)
        .unwrap()
        .plan(&choices(), p.private_base + 0x100000)
        .unwrap();
    assert!(!p.same_private_layout(&moved));
}
#[test]
fn camera_v2_every_partial_publication_prefix_is_conditionally_restorable() {
    let mut f = fixture(PREFERRED);
    let p = plan(&f);
    f.initialize(&p);
    for prefix in 0..=p.edits.len() {
        p.compare_before(&f).unwrap();
        f.apply(&p.edits[..prefix]);
        let restore = p.restore(&f).unwrap();
        f.apply(&restore);
        p.compare_before(&f).unwrap();
    }
    assert!(f.data.contains_key(&p.private_base));
}
#[test]
#[ignore = "explicit pinned installed Version 2 camera disk audit; no process/game writes"]
fn camera_v2_actual_installed_disk_profile_and_full_plan() {
    let path =
        std::env::var_os("C27_CAMERA_V2_DISK").expect("Explicit installed EXE path required");
    let mut file = File::open(path).unwrap();
    let c = verify_disk(&mut file).unwrap();
    let p = c.plan(&choices(), 0x70000000000).unwrap();
    println!(
        "Camera Version 2 full disk SHA/profile/7 choices/13 modes verified; {} private bytes; {} image edits; no process access or game file changes",
        p.allocation_bytes,
        p.edits.len()
    );
}
