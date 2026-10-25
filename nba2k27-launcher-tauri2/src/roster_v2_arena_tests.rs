use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
const BASE: u64 = 0x180000000;
const REGION: owner::OwnedRegion = owner::OwnedRegion {
    base: 0x220000000,
    len: 0x800000,
};
const OWNER: u64 = REGION.base + 0x50;
const ROOT: u64 = 0x240000000;
const ARENAS: u64 = REGION.base + 0x100000;
struct Fake {
    bytes: RefCell<BTreeMap<u64, u8>>,
    ended: Cell<bool>,
    short: Cell<u64>,
    reads: RefCell<Vec<(u64, usize)>>,
    mutation: RefCell<Option<(u64, u64, Vec<u8>)>>,
}
impl Fake {
    fn put(&self, at: u64, b: &[u8]) {
        let mut m = self.bytes.borrow_mut();
        for (i, v) in b.iter().enumerate() {
            m.insert(at + i as u64, *v);
        }
    }
    fn q(&self, at: u64, v: u64) {
        self.put(at, &v.to_le_bytes())
    }
    fn fixture() -> (Self, owner::OwnerSnapshot) {
        let m = Self {
            bytes: RefCell::new(BTreeMap::new()),
            ended: Cell::new(false),
            short: Cell::new(0),
            reads: RefCell::new(Vec::new()),
            mutation: RefCell::new(None),
        };
        let mut h = vec![0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[60..64].copy_from_slice(&128u32.to_le_bytes());
        h[128..132].copy_from_slice(b"PE\0\0");
        h[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        h[136..140].copy_from_slice(&0x6ad2e48cu32.to_le_bytes());
        h[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
        h[208..212].copy_from_slice(&0x36532000u32.to_le_bytes());
        m.put(BASE, &h);
        for &(rva, prefix, target) in owner::RIP_ANCHORS {
            let mut b = prefix.to_vec();
            b.extend(((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes());
            m.put(BASE + rva, &b);
        }
        for &(rva, b) in owner::FIELD_ANCHORS {
            m.put(BASE + rva, b)
        }
        m.q(BASE + owner::ROOT_CLASS, BASE + 0x24e66a0);
        m.q(BASE + owner::ROOT_CLASS + 8, BASE + 0x24e6710);
        m.q(BASE + owner::ROOT, ROOT);
        m.q(ROOT, BASE + owner::ROOT_CLASS);
        m.q(ROOT + 0xa8, OWNER);
        for (offset, count, at) in [
            (0x10, 100, REGION.base + 0x680),
            (0x60, 3, ARENAS),
            (0x80, 1, REGION.base + 0x200000),
            (0x290, 0, 0),
        ] {
            m.q(OWNER + offset, count);
            m.q(OWNER + offset + 8, at);
        }
        let p = proof().unwrap();
        for span in p["spans"].as_array().unwrap() {
            m.put(
                BASE + span["rva"].as_u64().unwrap(),
                &bytes(span["hex"].as_str().unwrap()).unwrap(),
            );
        }
        for field in p["fields"].as_array().unwrap() {
            let at = BASE + field["rva"].as_u64().unwrap();
            m.put(at, &bytes(field["prefixHex"].as_str().unwrap()).unwrap());
            let meta = field["metadataRva"].as_u64().unwrap();
            if meta != 0 {
                m.q(at + 24, BASE + meta);
                m.put(
                    BASE + meta,
                    &bytes(field["metadataHex"].as_str().unwrap()).unwrap(),
                );
            }
        }
        for i in 0..3 {
            m.put(
                ARENAS + i as u64 * STRIDE as u64,
                &row(i as u16, "Arena", "floor"),
            );
        }
        let state = owner::inspect_owner(&m, REGION).unwrap();
        (m, state)
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        BASE
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(!self.ended.get(), "cancelled");
        Ok(())
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((at, len));
        let b = self.bytes.borrow();
        let mut out = (0..len)
            .map(|i| *b.get(&(at + i as u64)).unwrap_or(&0))
            .collect::<Vec<_>>();
        drop(b);
        if self
            .mutation
            .borrow()
            .as_ref()
            .is_some_and(|(trigger, _, _)| *trigger == at)
        {
            let (_, target, b) = self.mutation.borrow_mut().take().unwrap();
            self.put(target, &b);
        }
        if self.short.get() == at {
            out.pop();
        }
        Ok(out)
    }
}
fn row(id: u16, name: &str, floor: &str) -> Vec<u8> {
    let mut b = vec![0; STRIDE];
    for (i, u) in name.encode_utf16().enumerate() {
        b[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes())
    }
    b[0x7e..0x83].copy_from_slice(b"model");
    b[0xa4..0xa4 + floor.len()].copy_from_slice(floor.as_bytes());
    b[0x792..0x794].copy_from_slice(&id.to_le_bytes());
    b
}
#[test]
fn complete_catalog_has_no_public_pointers_and_internal_exact_witnesses() {
    let (m, state) = Fake::fixture();
    let s = capture(&m, &state).unwrap().unwrap();
    let d = s.document();
    assert_eq!(d["arenas"].as_array().unwrap().len(), 3);
    assert_eq!(d["arenas"][0]["floorFile"], "floor");
    let text = serde_json::to_string(&d).unwrap();
    for key in ["address", "pointer", "owner", "raw"] {
        assert!(!text.contains(key));
    }
    let w = s.witness(1).unwrap();
    assert_eq!(w.address(), ARENAS + STRIDE as u64);
    assert_eq!(w.bytes().len(), STRIDE);
    assert_eq!(w.id(), 1);
    s.revalidate_witness(&m, &state, &w).unwrap();
    assert!(m.reads.borrow().iter().all(|(_, n)| *n <= 4096));
    let mut team = vec![0; owner::TEAM_STRIDE];
    team[TEAM_POINTER..TEAM_POINTER + 8].copy_from_slice(&ARENAS.to_le_bytes());
    let selected = s.selection(ARENAS, 1, s.id()).unwrap();
    assert_eq!(selected.validate_request(1, &team).unwrap(), w.address());
    assert!(selected.validate_request(2, &team).is_err());
    team[TEAM_POINTER] ^= 8;
    assert!(selected.validate_request(1, &team).is_err());
    assert!(s.selection(ARENAS, 1, "stale").is_err());
}
#[test]
fn duplicate_even_empty_ids_are_unselectable_without_losing_catalog() {
    let (m, state) = Fake::fixture();
    let mut empty = vec![0; STRIDE];
    empty[0x792..0x794].copy_from_slice(&1u16.to_le_bytes());
    m.put(ARENAS + 2 * STRIDE as u64, &empty);
    let s = capture(&m, &state).unwrap().unwrap();
    assert!(s.witness(1).is_err());
    assert!(s.witness(0).is_ok());
    assert_eq!(s.document()["arenas"].as_array().unwrap().len(), 1);
}
#[test]
fn text_bounds_strict_malformed_rows_not_selected() {
    for kind in 0..5 {
        let (m, state) = Fake::fixture();
        let mut b = row(1, "Arena", "floor");
        match kind {
            0 => b[..70].fill(65),
            1 => b[..2].copy_from_slice(&0xd800u16.to_le_bytes()),
            2 => b[0xa4] = 1,
            3 => b[0xa4..0xcc].fill(b'x'),
            _ => b[0xa4..0xcc].fill(0),
        }
        m.put(ARENAS + STRIDE as u64, &b);
        let s = capture(&m, &state).unwrap().unwrap();
        assert!(s.witness(1).is_err());
        assert!(s.witness(0).is_ok());
    }
    let (m, state) = Fake::fixture();
    let mut b = row(1, &"N".repeat(34), &"f".repeat(39));
    m.put(ARENAS + STRIDE as u64, &b);
    assert!(capture(&m, &state).unwrap().unwrap().witness(1).is_ok());
    b[68] = b'N';
    b[69] = 0;
    m.put(ARENAS + STRIDE as u64, &b);
    assert!(capture(&m, &state).unwrap().unwrap().witness(1).is_err());
}
#[test]
fn full_rows_two_pass_and_ending_owner_check() {
    let (m, state) = Fake::fixture();
    *m.mutation.borrow_mut() = Some((ARENAS, ARENAS, vec![88]));
    assert!(capture(&m, &state).is_err());
    let (m, state) = Fake::fixture();
    *m.mutation.borrow_mut() = Some((ARENAS, OWNER + 0x60, 4u64.to_le_bytes().to_vec()));
    assert!(capture(&m, &state).is_err());
    let (m, state) = Fake::fixture();
    m.short.set(ARENAS);
    assert!(capture(&m, &state).is_err());
}
#[test]
fn retained_witness_rejects_other_row_duplicate_owner_and_profile_change() {
    let (m, state) = Fake::fixture();
    let s = capture(&m, &state).unwrap().unwrap();
    let w = s.witness(0).unwrap();
    m.put(ARENAS + STRIDE as u64 + 0x792, &0u16.to_le_bytes());
    // Sealed addresses do not rebind when an unrelated row later duplicates an ID.
    assert!(s.revalidate_witness(&m, &state, &w).is_ok());
    assert!(s.revalidate(&m, &state).is_err()); // New admission refuses changed catalog.
    let (m, state) = Fake::fixture();
    let s = capture(&m, &state).unwrap().unwrap();
    m.q(ROOT + 0xa8, OWNER + 8);
    assert!(s.revalidate(&m, &state).is_err());
    let (m, state) = Fake::fixture();
    let s = capture(&m, &state).unwrap().unwrap();
    m.put(BASE + SPANS[0].0, &[0]);
    assert!(s.revalidate(&m, &state).is_err());
}
#[test]
fn profile_mismatch_only_initially_optional_and_cancel_never_optional() {
    let (m, state) = Fake::fixture();
    m.put(BASE + SPANS[0].0, &[0]);
    assert!(capture(&m, &state).unwrap().is_none());
    assert!(verify_profile(&m).is_err());
    let (m, state) = Fake::fixture();
    m.q(BASE + 0x8a02f70 + 24, BASE + 0x89f6990 + 8);
    assert!(capture(&m, &state).unwrap().is_none());
    let (m, state) = Fake::fixture();
    m.ended.set(true);
    assert!(capture(&m, &state).is_err());
}
#[test]
fn catalog_membership_and_allocation_bounds_are_exact() {
    let (m, state) = Fake::fixture();
    let s = capture(&m, &state).unwrap().unwrap();
    for ptr in [0, ARENAS - 8, ARENAS + 8, ARENAS + 3 * STRIDE as u64] {
        assert!(s.witness_at(ptr).is_err());
    }
    let mut bad = state.clone();
    bad.arenas.count = 2049;
    assert!(bounds(&bad).is_err());
    bad = state.clone();
    bad.arenas.pointer = state.teams.pointer;
    assert!(bounds(&bad).is_err());
    bad = state.clone();
    bad.arenas.pointer = REGION.base + REGION.len - STRIDE as u64;
    assert!(bounds(&bad).is_err());
}

#[test]
fn sealed_pair_read_budget_deduplicates_and_never_rebinds() {
    let (m, state) = Fake::fixture();
    let s = capture(&m, &state).unwrap().unwrap();
    for (id, expected) in [(0, 4), (1, 8)] {
        let selection = s.selection(ARENAS, id, s.id()).unwrap();
        m.reads.borrow_mut().clear();
        s.revalidate_selection(&m, &state, &selection).unwrap();
        let row_reads = m
            .reads
            .borrow()
            .iter()
            .filter(|(at, _)| *at >= ARENAS && *at < ARENAS + 3 * STRIDE as u64)
            .count();
        assert_eq!(
            row_reads, expected,
            "only full selected rows, twice, with chunk4096 bound"
        );
    }
    let selection = s.selection(ARENAS, 1, s.id()).unwrap();
    *m.mutation.borrow_mut() = Some((ARENAS, ARENAS + STRIDE as u64 + STRIDE as u64 - 1, vec![1]));
    assert!(s.revalidate_selection(&m, &state, &selection).is_err());
    let mut changed = state.clone();
    changed.owner += 8;
    assert!(s.revalidate_selection(&m, &changed, &selection).is_err());
}
