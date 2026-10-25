use super::*;
const PINNED: &[(u64, &str)] = &[
    (
        0x2de0f20,
        "564883ec20488bb1b00000004885f67437488d0db822d007e8e39133ff4889c1482bb09800000048c1ee0369c6398ee33885c078083981900000007705b8ffffffff4883c4205ec3b8020200004883c4205ec3",
    ),
    (
        0x2de0fb0,
        "56574883ec7889d74889ce488d0d2e22d007e8599133ff83ffff742685ff78293bb890000000732189f9488d0cc948c1e10348038898000000740748898eb00000004883c4785f5ec3c5fc1005bfc29b04c5fc11442450488d05e26ff7024889442460488d05d26ef7024889442450c5f877e8a91f27fd488d4c245048894c2440488d0d74bb510448894c2430488d0d34b1700448894c2420c744243802000000c744242818000000488d0d27f0f102488d1507be88034c8d0d2a799c044989c0e8ba44d90184c00f8474ffffffcc4883c4785f5ec3",
    ),
    (
        0x2cc6290,
        "564883ec304889cec5f857c0c5fc110166c741200000c5f857c0c5f811412848c74136000000000fb6413ec7442428010000008944242cc6413e00488d542428c5f877e808e9ffff80663fc00fb74646c1e010894644c74640ffff00004883c4305ec3",
    ),
    (
        0x2cc4ce0,
        "56574883ec784889d74889ce0fb60288010fb642018841010fb642028841020fb642038841030fb642048841040fb642058841050fb642068841060fb642078841070fb642088841080fb642098841090fb6420a88410a0fb6420b88410b0fb6420c88410c0fb6420d88410d0fb6420e88410e0fb6420f88410f0fb642108841100fb642118841110fb642128841120fb642138841130fb642148841140fb642158841150fb642168841160fb642178841170fb642188841180fb642198841190fb6421a88411a0fb6421b88411b0fb6421c88411c0fb6421d88411d0fb6421e88411e0fb6421f88411f0fb642208841200fb64221884121488b4228488941280fb74230668941300fb7423283f806730966894632e983000000488d4c244c48894c2450488d0df5b2ec0148894c2458488d1599fcf406488954246048894c24688944244c488d4424504889442440488d0556389f034889442430488d05b4296e044889442420c744243802000000c74424280a000000488d0d29b20303488d15b7389f034c8d05a2389f034c8d0dadb9ae04e8b806eb0184c07401cc8b4f3483e17f8b463483e08009c8894634b980ff0300234f34257f00fcff09c8894634b90000fc03234f3425ffff03fc09c8894634b90000000c234f3425fffffff309c8894634b9000000f0234f3425ffffff0f09c8894634c5fa104738c5fa1146380fb6473c88463c0fb6473d88463d0fb6473e83f82473250fb64e3ec744245001000000894c245488463e488d5424504889f1e8c9fcffffe983000000488d4c244c48894c2450488d0dd3b1ec0148894c2458488d157bfbf406488954246048894c24688944244c488d4424504889442440488d0534379f034889442430488d0592286e044889442420c744243802000000c744242816000000488d0d07b10303488d1595379f034c8d0580379f034c8d0dabb8ae04e89605eb0184c07401cc0fb6473f243f0fb64e3f80e1c008c1884e3f0fb74f400fb74642c1e01009c88946400fb64f42c1e11025ffff00ff09c8894640b90000003f234f4025ffffffc009c8894640b900000040234f4025ffffffbf09c8894640b900000080234f4025ffffff7f09c88946400fbf47443dd500000073130fb74e46c1e11009c1894e444883c4785f5ec3488d4c244c48894c2450488d0dc9b0ec0148894c2458488d1575faf406488954246048894c24688944244c488d4424504889442440488d052a369f034889442430488d0588276e044889442420c744243802000000c744242820000000488d0dfdaf0303488d158b369f034c8d0576369f034c8d0dbeb7ae04e88c04eb0184c00f8473ffffffcc4883c4785f5ec3",
    ),
    (
        0x2de1090,
        "564883ec7089d6488d0d5221d007e87d9033ff83feff0f84b800000085f678323bb090000000732a89f1488d0cc948c1e103480388980000000f84950000008b4134f7d0a90000000c0f95c04883c4705ec3c5fc1005d6c19b04c5fc11442450488d05f96ef7024889442460488d05e96df7024889442450c5f877e8c01e27fd488d4c245048894c2440488d0d8bba510448894c2430488d0d4bb0700448894c2420c744243802000000c744242818000000488d0d3eeff102488d151ebd88034c8d0d41789c044989c0e8d143d90184c07401cc31c04883c4705ec3",
    ),
    (
        0x2de1170,
        "564883ec7089d6488d0d7220d007e89d8f33ff83feff0f84b800000085f678323bb090000000732a89f1488d0cc948c1e103480388980000000f84950000008b4134f7d0a90000000c0f95c04883c4705ec3c5fc1005f6c09b04c5fc11442450488d05196ef7024889442460488d05096df7024889442450c5f877e8e01d27fd488d4c245048894c2440488d0dabb9510448894c2430488d0d6baf700448894c2420c744243802000000c744242818000000488d0d5eeef102488d153ebc88034c8d0d61779c044989c0e8f142d90184c07401cc31c04883c4705ec3",
    ),
    (
        0x4bc5ab0,
        "56574883ec284889d648894c2420b2014889c84809f00f84b90000004885f60f94c04885c90f94c108c1740731d2e9a2000000488d7c24200f1f840000000000488b4c24200fbe0185c0781448ffc148894c2420eb15662e0f1f8400000000004889fa41b001e875b000000fb70e89ca81e200fc000081fa00d8000075420fb756024189d04181e000fc00004181f800dc0000752b81e1bf030000c1e10a4883c60481e2ff03000001d181c10000010039c80f94c285c07514eb1a0f1f4400004883c60239c80f94c285c0740839c80f846bffffff89d04883c4285f5ec3",
    ),
    (
        0x2f1a480,
        "4157415641554154565755534883ec784c63a9900000004d85ed0f8ee90000004889d64889cf31ed4c8d3de9e4880431db4531f66666662e0f1f8400000000008b87900000004939c673154c8ba7980000004901dce987000000660f1f440000c5fc1005d82d8804c5fc11442450488d05fbdae3024889442460488d05ebd9e3024889442450c5f877e8c28a13fd488d4c245048894c2440488d0d8d263e0448894c2430488d0d4d1c5d0448894c2420c744243802000000c744242818000000488d0d405bde02488d15202975034989c04d89f9e8d7afc50184c07401cc4531e44c89e14889f2e844b5ca0184c04c0f44e54d85e4751549ffc64883c3484d39ee0f8239ffffffeb034531e44c89e04883c4785b5d5f5e415c415d415e415fc3",
    ),
];
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
const CONTAINER: u64 = 0x240000000;
const FIRST: u64 = REGION.base + 0x300000;
const LAST: u64 = REGION.base + 0x400000;
struct Fake {
    bytes: RefCell<BTreeMap<u64, u8>>,
    ended: Cell<bool>,
    short: Cell<u64>,
    reads: RefCell<Vec<(u64, usize)>>,
    mutation: RefCell<Option<(u64, u64, Vec<u8>)>>,
    mutation_after: Cell<usize>,
}
impl Fake {
    fn put(&self, at: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.borrow_mut().insert(at + i as u64, *v);
        }
    }
    fn q(&self, at: u64, n: u64) {
        self.put(at, &n.to_le_bytes())
    }
    fn descriptor(&self, offset: u64, count: u32, opaque: u32, at: u64) {
        let mut b = Vec::new();
        b.extend(count.to_le_bytes());
        b.extend(opaque.to_le_bytes());
        b.extend(at.to_le_bytes());
        self.put(OWNER + offset, &b);
    }
    fn fixture() -> (Self, owner::OwnerSnapshot) {
        let m = Self {
            bytes: RefCell::new(BTreeMap::new()),
            ended: Cell::new(false),
            short: Cell::new(0),
            reads: RefCell::new(Vec::new()),
            mutation: RefCell::new(None),
            mutation_after: Cell::new(0),
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
            m.put(BASE + rva, b);
        }
        m.q(BASE + owner::ROOT_CLASS, BASE + 0x24e66a0);
        m.q(BASE + owner::ROOT_CLASS + 8, BASE + 0x24e6710);
        m.q(BASE + owner::ROOT, CONTAINER);
        m.q(CONTAINER, BASE + owner::ROOT_CLASS);
        m.q(CONTAINER + 0xa8, OWNER);
        for (offset, count, at) in [
            (0x10, 100, REGION.base + 0x680),
            (0x60, 1, REGION.base + 0x100000),
            (0x80, 1, REGION.base + 0x200000),
            (0x290, 0, 0),
        ] {
            m.descriptor(offset, count, 0, at);
        }
        m.descriptor(0x240, 1, 0xfedcba98, FIRST);
        m.descriptor(0x250, 1, 0x12345678, LAST);
        m.put(FIRST, &vec![0; 48]);
        m.put(LAST, &vec![0; 48]);
        for &(rva, hex) in PINNED {
            let b: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            m.put(BASE + rva, &b);
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
        let mut v = (0..len)
            .map(|i| b.get(&(at + i as u64)).copied().context("unmapped"))
            .collect::<Result<Vec<_>>>()?;
        drop(b);
        if self
            .mutation
            .borrow()
            .as_ref()
            .is_some_and(|(trigger, _, _)| {
                *trigger == at
                    && self.reads.borrow().iter().filter(|(a, _)| *a == at).count()
                        >= self.mutation_after.get()
            })
        {
            let (_, target, bytes) = self.mutation.borrow_mut().take().unwrap();
            self.put(target, &bytes);
        }
        if self.short.get() == at {
            v.pop();
        }
        Ok(v)
    }
}
const ORIGINS: u64 = REGION.base + 0x500000;
fn origin_row(name: &[u8], flags: u32) -> Vec<u8> {
    let mut row = vec![0; STRIDE];
    let n = name.len().min(34);
    row[..n].copy_from_slice(&name[..n]);
    row[0x34..0x38].copy_from_slice(&flags.to_le_bytes());
    row
}
fn origins() -> (Fake, owner::OwnerSnapshot) {
    let (m, s) = Fake::fixture();
    m.descriptor(0x90, 2, 0xfedcba98, ORIGINS);
    m.put(
        ORIGINS,
        &[
            origin_row(b"University", 0),
            origin_row(b"Hidden", 0x0c000000),
        ]
        .concat(),
    );
    (m, s)
}
fn player(pointer: u64) -> Vec<u8> {
    let mut p = vec![0; owner::PLAYER_STRIDE];
    p[0xb0..0xb8].copy_from_slice(&pointer.to_le_bytes());
    p
}
#[test]
fn captures_stable_native_indices_selectability_and_null_is_not514() {
    let (m, s) = origins();
    let view = capture(&m, &s).unwrap().unwrap();
    view.revalidate(&m, &s).unwrap();
    let calls = m.reads.borrow().len();
    assert_eq!(view.choices(), json!([{"index":0,"name":"University"}]));
    assert_eq!(
        view.project(&player(0)).unwrap()["originState"],
        "unassigned"
    );
    assert!(view.project(&player(0)).unwrap()["nativeOriginIndex"].is_null());
    assert_eq!(
        view.project(&player(ORIGINS)).unwrap()["nativeOriginIndex"],
        0
    );
    assert_eq!(
        view.project(&player(ORIGINS + STRIDE as u64)).unwrap()["originSelectable"],
        false
    );
    assert_eq!(
        view.project(&player(ORIGINS + 1)).unwrap()["originState"],
        "invalid"
    );
    assert_eq!(calls, m.reads.borrow().len());
    m.descriptor(0x90, 515, 0, ORIGINS);
    let mut bytes = vec![0; 515 * STRIDE];
    bytes[514 * STRIDE..515 * STRIDE].copy_from_slice(&origin_row(b"Actual row514", 0));
    m.put(ORIGINS, &bytes);
    let view = capture(&m, &s).unwrap().unwrap();
    view.revalidate(&m, &s).unwrap();
    assert_eq!(
        view.project(&player(ORIGINS + 514 * STRIDE as u64))
            .unwrap()["nativeOriginIndex"],
        514
    );
    assert!(view.project(&player(0)).unwrap()["nativeOriginIndex"].is_null());
}
#[test]
fn bounded_utf8_names_and_native_flags() {
    let mut bytes = Vec::new();
    bytes.extend(origin_row(b"Good", 0));
    bytes.extend(origin_row(&[b'x'; 34], 0));
    bytes.extend(origin_row(&[0xff], 0));
    bytes.extend(origin_row(b"Bad\x01", 0));
    bytes.extend(origin_row("International \u{00e9}".as_bytes(), 0));
    let rows = decode(&bytes);
    assert_eq!(rows[0].0.as_deref(), Some("Good"));
    assert!(rows[1].0.is_none() && rows[2].0.is_none() && rows[3].0.is_none());
    assert!(rows[4].0.is_some());
    for bits in 0..4 {
        let row = origin_row(b"Flag", bits << 26);
        assert_eq!(decode(&row)[0].1, bits != 3);
    }
}
#[test]
fn refuses_foreign_alias_count_range_partial_lease_and_owner_changes() {
    for fault in 0..8 {
        let (m, s) = origins();
        let view = capture(&m, &s).unwrap().unwrap();
        match fault {
            0 => m.put(OWNER + 0x94, &0u32.to_le_bytes()),
            1 => m.put(ORIGINS + 71, &[1]),
            2 => m.q(BASE + owner::ROOT, CONTAINER + 8),
            3 => m.put(BASE + PROFILE[0].0, &[0xcc]),
            4 => m.short.set(ORIGINS),
            5 => m.ended.set(true),
            6 => m.descriptor(0x240, 2, 0, ORIGINS),
            _ => m.descriptor(0x90, 2, 0, ORIGINS + 8),
        }
        assert!(view.revalidate(&m, &s).is_err(), "fault{fault}");
    }
    for (at, count) in [
        (ORIGINS, MAX_ROWS as u32 + 1),
        (synth_player_pointer(), 1),
        (OWNER, 1),
        (REGION.base + REGION.len - 8, 1),
        (ORIGINS + 1, 1),
        (FIRST, 1),
    ] {
        let (m, s) = origins();
        m.descriptor(0x90, count, 0, at);
        assert!(capture(&m, &s).unwrap().is_none());
    }
}
fn synth_player_pointer() -> u64 {
    REGION.base + 0x680
}
#[test]
fn optional_absence_unsupported_profile_never_swallows_cancel_or_read_failure() {
    let (m, s) = origins();
    m.descriptor(0x90, 0, 0x12345678, 0);
    assert!(capture(&m, &s).unwrap().is_none());
    let (m, s) = origins();
    m.put(BASE + PROFILE[0].0, &[0xcc]);
    assert!(capture(&m, &s).unwrap().is_none());
    let (m, s) = origins();
    m.short.set(OWNER + 0x90);
    assert!(capture(&m, &s).is_err());
    let (m, s) = origins();
    m.ended.set(true);
    assert!(capture(&m, &s).is_err());
}
#[test]
fn final_read_mutations_cannot_hide_behind_previously_valid_headers_and_dependencies() {
    for fault in 0..3 {
        let (m, s) = origins();
        let view = capture(&m, &s).unwrap().unwrap();
        let (target, bytes) = match fault {
            0 => (OWNER + 0x94, 0u32.to_le_bytes().to_vec()),
            1 => (BASE + PROFILE[0].0, vec![0xcc]),
            _ => (OWNER + 0x248, ORIGINS.to_le_bytes().to_vec()),
        };
        *m.mutation.borrow_mut() = Some((ORIGINS, target, bytes));
        assert!(view.revalidate(&m, &s).is_err());
    }
}

#[test]
fn selection_admits_null_to_real_zero_and_refuses_unknown_current_or_target() {
    let (m, state) = origins();
    let view = capture(&m, &state).unwrap().unwrap();
    view.revalidate(&m, &state).unwrap();
    let unchanged = player(0);
    let selected = view.prepare_change(&unchanged, &unchanged, 0).unwrap();
    assert_eq!(selected.original_pointer(), 0);
    assert_eq!(selected.target_pointer(), ORIGINS);
    assert_eq!(selected.target_index(), 0);
    selected.revalidate(&m, &state).unwrap();
    let mut changed = unchanged.clone();
    changed[0x2e8] = 1;
    assert!(view.prepare_change(&unchanged, &changed, 0).is_err());
    assert!(
        view.prepare_change(&player(ORIGINS + 1), &player(ORIGINS + 1), 0)
            .is_err()
    );
    assert!(view.prepare_change(&unchanged, &unchanged, 1).is_err());
    assert!(
        view.prepare_change(&unchanged, &unchanged, usize::MAX)
            .is_err()
    );
}
#[test]
fn selection_retains_original_target_rows_and_all_external_guards_without_player_preimage() {
    for fault in 0..7 {
        let (m, state) = origins();
        m.put(ORIGINS + STRIDE as u64, &origin_row(b"Second", 0));
        let view = capture(&m, &state).unwrap().unwrap();
        view.revalidate(&m, &state).unwrap();
        let selected = view
            .prepare_change(&player(ORIGINS), &player(ORIGINS), 1)
            .unwrap();
        assert_eq!(selected.original_pointer(), ORIGINS);
        assert_eq!(selected.target_pointer(), ORIGINS + STRIDE as u64);
        match fault {
            0 => m.put(ORIGINS + 71, &[1]),
            1 => m.put(ORIGINS + STRIDE as u64 + 71, &[1]),
            2 => m.put(OWNER + 0x94, &0u32.to_le_bytes()),
            3 => m.q(BASE + owner::ROOT, CONTAINER + 8),
            4 => m.put(BASE + PROFILE[0].0, &[0xcc]),
            5 => m.short.set(ORIGINS),
            _ => m.ended.set(true),
        }
        assert!(selected.revalidate(&m, &state).is_err(), "fault{fault}");
    }
    let (m, state) = origins();
    let view = capture(&m, &state).unwrap().unwrap();
    let selected = view.prepare_change(&player(0), &player(0), 0).unwrap();
    m.put(synth_player_pointer() + 0xb0, &ORIGINS.to_le_bytes());
    selected.revalidate(&m, &state).unwrap(); // The future Plan, not this external witness, guards own evolving PLAYER bytes.
}
#[test]
fn selection_real_index514_is_not_null_sentinel_and_late_target_tamper_refuses() {
    let (m, state) = origins();
    m.descriptor(0x90, 515, 0, ORIGINS);
    let mut b = vec![0; 515 * STRIDE];
    b[514 * STRIDE..].copy_from_slice(&origin_row(b"Real514", 0));
    m.put(ORIGINS, &b);
    let view = capture(&m, &state).unwrap().unwrap();
    view.revalidate(&m, &state).unwrap();
    let selected = view.prepare_change(&player(0), &player(0), 514).unwrap();
    assert_eq!(selected.target_pointer(), ORIGINS + 514 * STRIDE as u64);
    selected.revalidate(&m, &state).unwrap();
    *m.mutation.borrow_mut() = Some((
        selected.target_pointer(),
        selected.target_pointer() + 71,
        vec![1],
    ));
    assert!(selected.revalidate(&m, &state).is_err());
}

#[test]
fn selection_token_request_and_post_last_row_header_are_bound() {
    let (m, state) = origins();
    let view = capture(&m, &state).unwrap().unwrap();
    let selected = view.prepare_change(&player(0), &player(0), 0).unwrap();
    selected.validate_catalog(view.id()).unwrap();
    assert!(selected.validate_catalog("").is_err());
    assert!(selected.validate_catalog("foreign").is_err());
    assert_eq!(selected.validate_request(0, &player(0)).unwrap(), ORIGINS);
    assert!(selected.validate_request(1, &player(0)).is_err());
    assert!(selected.validate_request(0, &player(ORIGINS)).is_err());
    let next = capture(&m, &state).unwrap().unwrap();
    assert_ne!(view.id(), next.id());
    assert!(selected.validate_catalog(next.id()).is_err());
    let reads = m
        .reads
        .borrow()
        .iter()
        .filter(|(a, _)| *a == ORIGINS)
        .count();
    m.mutation_after.set(reads + 2);
    *m.mutation.borrow_mut() = Some((ORIGINS, OWNER + 0x94, 0u32.to_le_bytes().to_vec()));
    assert!(selected.revalidate(&m, &state).is_err());
}
