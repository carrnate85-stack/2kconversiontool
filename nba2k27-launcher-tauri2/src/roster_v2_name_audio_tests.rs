use super::*;
const PINNED: &[(u64, &str)] = &[
    (
        0x2d39980,
        "5541574156415541545657534881ecd8000000488dac248000000048c74550feffffff4889ce488d15731cc3fe488d4d4fe81a1761ff4885f60f84420200004889f1e85977e80185c07e3689c231c949b800000000812400000f1f8000000000440fb70c4e4983f92d77184d0fa3c8731248ffc14839ca75e789c185c07941eb5431c939c17d2c89ca4189c00f1f4000440fb70c564183f92a0f84050200004183f92c0f84fb01000048ffc24939d075dfeb0289c885c0781c0f1f800000000089c266833c5620750c85c08d40ff7ff0b8ffffffff31ffbb0000000029c87e1e83f820bb200000000f42d889c8488d1446448d041b488d4dd0e85222730283fb1fb81f0000000f42c366c74445d00000c7453000000000b001a80174270f1f004863f78b7cb53089f1e802fcffff01f88d7e018944b53483fe020f95c00f44fea80175dcb001488d352397da07488d7dd031c9a8010f8426010000894d484c63e9428b5cad30468b74ad3441ffc54589f74129df0f8eef000000660f1f4400004889f1e818063eff41d1ef4101df4183ffff0f84a60000004585ff7823443bb840020000731a4489f94c8d244949c1e4044c03a048020000e9840000000f1f00c5fc10057837a604c5fc114510488d059ce4010348894520488d058de3010348894510c5f877e8659431fd488d4d1048894c2440488d0d31305c0448894c2430488d0df1257b0448894c2420c744243802000000c744242833000000488d0de464fc02488d15c43293034989c04c8d0d52f2a604e877b9e30184c07401cc4531e44c89e14889fae8549ce80185c07434418d4f0185c00f48d9450f49f74589f74129df0f8f17ffffff8b4d4883f9020f95c0440f44e94489e9a8010f85dafeffff4531e4488d4d4fe8139d4dff4c89e04881c4d80000005b5f5e415c415d415e415f5dc389d085c00f8912feffffe922feffff",
    ),
    (
        0x2d3a470,
        "5657534881ec800000004889d74889ce0fb7026689010fb74202668941020fb74204668941040fb74206668941060fb74208668941080fb7420a6689410a0fb7420c6689410c0fb7420e6689410e0fb74210668941100fb74212668941120fb74214668941140fb74216668941160fb74218668941180fb7421a6689411a0fb7421c6689411c0fb7421e6689411e0fb74220668941200fb74222668941220fb74224668941240fb74226668941260fb74228668941280fb7422a6689412a8b422c8b492c89c283e27f83e18009d1894e2c488d5c244848895c2450488d0d4ea3e50148894c2458488d1562bbed06488954246048894c2468488d1549bbed06488954247048894c2478b10184c97565c1e019c1f81948984889442448488d4424504889442440488d058b04aa044889442430488d056589a7044889442420c744243803000000c744242808000000488d0de504aa04488d1550e197034c8d053be197034c8d0d3a0caa04e851afe30184c07401ccb980010000234f2cb87ffeffff23462c09c889462c8b4f2cc1e90983e10383f9030f858700000048895c2450488d05e95ae5014889442458488d0d25a6ed0648894c24604889442468c744244803000000488d4424504889442440488d0546e097034889442430488d05bc88a7044889442420c744243802000000c74424280a000000488d0d195afc02488d15a7e097034c8d0592e097034c8d0d9a88a704e8a8aee30184c07401cc8b462ceb0dc1e10925fff9ffff09c889462cb900080000234f2c25fff7ffff09c889462cb900100000234f2c25ffefffff09c889462cb900200000234f2c25ffdfffff09c889462c4881c4800000005b5f5ec3",
    ),
    (
        0x2d94920,
        "564883ec20488b71504885f67415e86db532024889c14889f24883c4205ee96db532024889c84883c4205ec3",
    ),
    (
        0x2d3d240,
        "4883ec28e837c7ffff4885c074090fbf40284883c428c3b8ffffffff4883c428c3",
    ),
    (
        0x2d3d270,
        "4883ec28e8c7caffff4885c074090fbf40284883c428c3b8ffffffff4883c428c3",
    ),
    (
        0x2d3d330,
        "4883ec28e807caffff4885c074090fbf402a4883c428c3b8ffffffff4883c428c3",
    ),
    (
        0x2d3d300,
        "4883ec28e877c6ffff4885c074090fbf402a4883c428c3b8ffffffff4883c428c3",
    ),
    (
        0x2d39d40,
        "5541574156415541545657534881ecd8000000488dac248000000048c74550feffffff4889ce488d15b318c3fe488d4d4fe85a1361ff488d0d7394da07e89e033eff4885f60f84460200004889f1e88d73e80185c07e3a89c231c949b8000000008124000066662e0f1f840000000000440fb70c4e4983f92d77184d0fa3c8731248ffc14839ca75e789c185c07941eb5431c939c17d2c89ca4189c00f1f4000440fb70c564183f92a0f84050200004183f92c0f84fb01000048ffc24939d075dfeb0289c885c0781c0f1f800000000089c266833c5620750c85c08d40ff7ff0b8ffffffff31ffbb0000000029c87e1e83f820bb200000000f42d889c8488d1446448d041b488d4dd0e8821e730283fb1fb81f0000000f42c366c74445d00000c7453000000000b001a80174270f1f004863f78b7cb53089f1e8f2f9ffff01f88d7e018944b53483fe020f95c00f44fea80175dcb001488d355393da07488d7dd031c9a8010f8426010000894d484c63e9428b5cad30468b74ad3441ffc54589f74129df0f8eef000000660f1f4400004889f1e848023eff41d1ef4101df4183ffff0f84a60000004585ff7823443bb850020000731a4489f94c8d244949c1e4044c03a058020000e9840000000f1f00c5fc1005a833a604c5fc114510488d05cce0010348894520488d05bddf010348894510c5f877e8959031fd488d4d1048894c2440488d0d612c5c0448894c2430488d0d21227b0448894c2420c744243802000000c744242834000000488d0d1461fc02488d15f42e93034989c04c8d0da6eea604e8a7b5e30184c07401cc4531e44c89e14889fae88498e80185c07434418d4f0185c00f48d9450f49f74589f74129df0f8f17ffffff8b4d4883f9020f95c0440f44e94489e9a8010f85dafeffff4531e4488d4d4fe843994dff4c89e04881c4d80000005b5f5e415c415d415e415f5dc389d085c00f8912feffffe922feffff",
    ),
    (
        0x2d39c80,
        "4883ec7883faff0f84a400000085d2781e3b9140020000731689d0488d044048c1e004480381480200004883c478c3c5fc10050936a604c5fc11442450488d052ce301034889442460488d051ce201034889442450c5f877e8f39231fd488d4c245048894c2440488d0dbe2e5c0448894c2430488d0d7e247b0448894c2420c744243802000000c744242833000000488d0d7163fc02488d15513193034c8d0de2f0a6044989c0e804b8e30184c07401cc31c04883c478c3",
    ),
    (
        0x2d3a050,
        "4883ec7883faff0f84a400000085d2781e3b9150020000731689d0488d044048c1e004480381580200004883c478c3c5fc10053932a604c5fc11442450488d055cdf01034889442460488d054cde01034889442450c5f877e8238f31fd488d4c245048894c2440488d0dee2a5c0448894c2430488d0dae207b0448894c2420c744243802000000c744242834000000488d0da15ffc02488d15812d93034c8d0d36eda6044989c0e834b4e30184c07401cc31c04883c478c3",
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
        m.put(FIRST, &row("First", -1, 7));
        m.put(LAST, &row("Last", 23, -2));
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
            .is_some_and(|(trigger, _, _)| *trigger == at)
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
fn row(name: &str, c: i16, pa: i16) -> Vec<u8> {
    let mut b = vec![0; 48];
    for (i, v) in name.encode_utf16().take(20).enumerate() {
        b[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
    }
    b[40..42].copy_from_slice(&c.to_le_bytes());
    b[42..44].copy_from_slice(&pa.to_le_bytes());
    b
}
fn player() -> Vec<u8> {
    let mut b = vec![0; owner::PLAYER_STRIDE];
    b[..40].copy_from_slice(&row("Last", 0, 0)[..40]);
    b[40..80].copy_from_slice(&row("First", 0, 0)[..40]);
    b[0x2e8..0x2ea].copy_from_slice(&(-1i16).to_le_bytes());
    b
}
#[test]
fn stable_tables_signed_mapping_opaque_header_and_cached_projection() {
    let (m, state) = Fake::fixture();
    let audio = capture(&m, &state).unwrap().unwrap();
    audio.revalidate(&m, &state).unwrap();
    let count = m.reads.borrow().len();
    let value = project(Some(&audio), &player()).unwrap();
    assert_eq!(value["firstName"]["commentaryId"], -1);
    assert_eq!(value["firstName"]["paId"], 7);
    assert_eq!(value["lastName"]["commentaryId"], 23);
    assert_eq!(value["lastName"]["paId"], -2);
    assert_eq!(value["nameOverrideActive"], false);
    assert_eq!(count, m.reads.borrow().len());
}
#[test]
fn ambiguity_localized_surname_and_renamed_player_never_reuse_old_mapping() {
    let (m, state) = Fake::fixture();
    m.descriptor(0x240, 2, 0, FIRST);
    m.put(FIRST, &[row("First", 1, 2), row("First", 1, 2)].concat());
    let duplicate = capture(&m, &state).unwrap().unwrap();
    duplicate.revalidate(&m, &state).unwrap();
    assert_eq!(
        project(Some(&duplicate), &player()).unwrap()["firstName"]["commentaryId"],
        1
    );

    m.descriptor(0x240, 3, 0, FIRST);
    m.put(
        FIRST,
        &[row("First", 1, 2), row("First", 1, 2), row("First", 3, 2)].concat(),
    );
    let audio = capture(&m, &state).unwrap().unwrap();
    audio.revalidate(&m, &state).unwrap();
    let mut p = player();
    assert!(project(Some(&audio), &p).unwrap()["firstName"].is_null());
    p[0x50] = 1;
    let v = project(Some(&audio), &p).unwrap();
    assert!(v["lastName"].is_null());
    assert_eq!(v["nameOverrideActive"], true);
    p[0x50] = 0;
    p[..40].copy_from_slice(&row("Changed", 0, 0)[..40]);
    assert!(project(Some(&audio), &p).unwrap()["lastName"].is_null());
}
#[test]
fn final_guard_refuses_header_row_owner_profile_partial_and_cancel() {
    for fault in 0..6 {
        let (m, state) = Fake::fixture();
        let audio = capture(&m, &state).unwrap().unwrap();
        match fault {
            0 => m.put(OWNER + 0x244, &0u32.to_le_bytes()),
            1 => m.put(FIRST + 47, &[1]),
            2 => m.q(BASE + owner::ROOT, CONTAINER + 8),
            3 => m.put(BASE + PROFILE[0].0, &[0xcc]),
            4 => m.short.set(FIRST),
            _ => m.ended.set(true),
        }
        assert!(audio.revalidate(&m, &state).is_err(), "fault {fault}");
    }
}
#[test]
fn absent_unsupported_and_read_failure_are_distinct() {
    let (m, state) = Fake::fixture();
    m.descriptor(0x240, 0, 7, 0);
    m.descriptor(0x250, 0, 8, 0);
    assert!(capture(&m, &state).unwrap().is_none());
    let (m, state) = Fake::fixture();
    m.put(BASE + PROFILE[0].0, &[0xcc]);
    assert!(capture(&m, &state).unwrap().is_none());
    let (m, state) = Fake::fixture();
    m.short.set(OWNER + 0x240);
    assert!(capture(&m, &state).is_err());
    let (m, state) = Fake::fixture();
    m.ended.set(true);
    assert!(capture(&m, &state).is_err());
}
#[test]
fn count_alias_range_chunk_and_utf16_bounds() {
    let (m, state) = Fake::fixture();
    let bounds = Bounds::new(&state).unwrap();
    for (at, count) in [
        (FIRST, MAX_ROWS as u32 + 1),
        (state.players.pointer, 1),
        (OWNER, 1),
        (REGION.base + REGION.len - 8, 1),
        (FIRST + 1, 1),
    ] {
        m.descriptor(0x240, count, 0, at);
        assert!(capture(&m, &state).unwrap().is_none());
    }
    m.descriptor(0x240, 86, 0, FIRST);
    m.put(
        FIRST,
        &(0..86).flat_map(|_| row("First", 1, 2)).collect::<Vec<_>>(),
    );
    let audio = capture(&m, &state).unwrap().unwrap();
    audio.revalidate(&m, &state).unwrap();
    assert!(
        m.reads
            .borrow()
            .iter()
            .any(|(a, l)| *a == FIRST + 4096 && *l == 32)
    );
    assert!(bounds.descriptor(&[255; 16]).is_none());
    let mut invalid = row("First", 0, 0);
    invalid[..2].copy_from_slice(&0xd800u16.to_le_bytes());
    assert!(row_name(&invalid).is_none());
    assert_eq!(
        row_name(&row("abcdefghijklmnopqrst", 0, 0)).unwrap().len(),
        20
    );
}

#[test]
fn mutations_during_ending_table_read_are_not_hidden_by_earlier_header_profile_checks() {
    for code in [false, true] {
        let (m, state) = Fake::fixture();
        let audio = capture(&m, &state).unwrap().unwrap();
        let (target, bytes) = if code {
            (BASE + PROFILE[0].0, vec![0xcc])
        } else {
            (OWNER + 0x254, 0u32.to_le_bytes().to_vec())
        };
        *m.mutation.borrow_mut() = Some((LAST, target, bytes));
        assert!(audio.revalidate(&m, &state).is_err());
    }
}
