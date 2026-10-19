use super::*;
use crate::blacktop_arena::{Inspect, Region};
use std::{cell::Cell, collections::HashMap};
pub(crate) const BASE: u64 = 0x140000000;
pub(crate) struct Fixture {
    pub bytes: HashMap<u64, u8>,
    ranges: Vec<(u64, usize)>,
    pub regions: HashMap<u64, Region>,
    pub tear: Option<u64>,
    reads: Cell<usize>,
}
impl Fixture {
    pub fn empty() -> Self {
        Self {
            bytes: HashMap::new(),
            ranges: Vec::new(),
            regions: HashMap::new(),
            tear: None,
            reads: Cell::new(0),
        }
    }
    pub fn map(&mut self, at: u64, len: usize) {
        self.ranges.push((at, len));
    }
    pub fn put(&mut self, at: u64, data: &[u8]) {
        for (i, b) in data.iter().enumerate() {
            self.bytes.insert(at + i as u64, *b);
        }
    }
    pub fn q(&mut self, at: u64, n: u64) {
        self.put(at, &n.to_le_bytes());
    }
    pub fn d(&mut self, at: u64, n: u32) {
        self.put(at, &n.to_le_bytes());
    }
    pub fn text(&mut self, at: u64, n: &str) {
        let b = n
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        self.put(at, &b);
    }
    pub fn layout(&mut self, basis: u64, g: &Group, r: &LayoutRecord) {
        self.map(basis, g.length);
        self.q(basis + 16, g.count as u64);
        self.q(basis + g.descriptor as u64, g.hash.parse().unwrap());
        self.q(basis + g.descriptor as u64 + 8, g.outfit_count as u64);
        self.q(basis + r.record as u64, r.marker.parse().unwrap());
        self.q(basis + r.record as u64 + 8, 2);
        self.q(basis + r.record as u64 + 16, basis + r.array as u64);
        for i in 0..2 {
            self.q(
                basis + r.array as u64 + i as u64 * 8,
                basis + r.strings[i] as u64,
            );
            self.text(basis + r.strings[i] as u64, &r.names[i]);
        }
    }
    pub fn selection() -> Self {
        let mut f = Self::empty();
        for (at, bytes) in crate::blacktop_native::selection_memory_fixture() {
            f.put(at, &bytes);
        }
        f.map(BASE + 0x5000, 0x7b0 * 10 + 0x40);
        let group = &groups()[0];
        let record = &group.records[0];
        let stock = 0x600000000;
        f.layout(stock, group, record);
        for i in 0..4u64 {
            let model = BASE + 0x20000 + i * 0x1000;
            let desc = model + 0x800;
            let roster = 0x700000000 + i * 0x4F8;
            f.map(desc, 0x400);
            f.map(roster, 0x4F8);
            f.q(desc + 0xB8, roster);
            f.q(desc + 0xC8, stock + record.array as u64);
            f.d(desc + 0xD0, 2);
            f.q(BASE + LINEUP + 0xC0 + i * 8, roster);
            f.text(roster, "Curry");
            f.text(roster + 0x28, "Stephen");
            f.put(roster + 0x110, &(100 + i as u16).to_le_bytes());
        }
        f
    }
    pub fn private(&mut self, game: &Identity, p: &Player, composed: &Composed) -> Page {
        let allocation = 0x800000000;
        let bytes = page_bytes(
            game,
            &p.key,
            p.original_pointer,
            allocation,
            composed.street_index,
            composed.roster_shoes,
            &composed.items,
        )
        .unwrap();
        self.put(allocation, &bytes);
        self.q(p.key.description + 0xC8, allocation + 128);
        self.d(p.key.description + 0xD0, composed.items.len() as u32);
        self.regions.insert(
            allocation,
            Region {
                start: allocation,
                len: 4096,
                allocation,
                kind: 0x20000,
                protection: 2,
                committed: true,
            },
        );
        Page {
            allocation,
            owner: p.key.clone(),
            original_pointer: p.original_pointer,
            street_index: composed.street_index,
            roster_shoes: composed.roster_shoes,
            items: composed.items.clone(),
            sha256: format!("{:X}", Sha256::digest(&bytes)),
        }
    }
}
impl Memory for Fixture {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(len > 0 && len <= 4096, "Fixture read bounds");
        if self.tear == Some(at) {
            let n = self.reads.get();
            self.reads.set(n + 1);
            if n > 0 {
                return Ok(vec![0; len]);
            }
        }
        let mut result = Vec::new();
        for i in 0..len {
            let address = at + i as u64;
            let value = if let Some(b) = self.bytes.get(&address) {
                *b
            } else {
                ensure!(
                    self.ranges
                        .iter()
                        .any(|(a, n)| address >= *a && address < *a + *n as u64),
                    "Unknown owned fixture address"
                );
                0
            };
            result.push(value);
        }
        Ok(result)
    }
}
impl Inspect for Fixture {
    fn region(&self, at: u64) -> Result<Region> {
        self.regions
            .get(&at)
            .copied()
            .context("Unknown fixture region")
    }
}
pub(crate) fn game() -> Identity {
    Identity {
        pid: 311,
        started: "134366400000000000".into(),
        path: "C:\\Fixture\\NBA2K27.exe".into(),
    }
}
fn outfit() -> crate::backend::Outfit {
    let c = outfits::catalog().unwrap();
    c.presets
        .iter()
        .filter(|p| p.pool.as_deref() == Some("male") && p.separable && !p.manual_only)
        .find_map(|p| c.original_outfit(&p.id, "QA outfit").ok())
        .unwrap()
}
#[test]
fn every_preserved_clof_layout_validates_and_changed_markers_are_rejected() {
    let mut count = 0;
    for group in groups() {
        for r in &group.records {
            let mut f = Fixture::empty();
            let basis = 0x200000000;
            f.layout(basis, group, r);
            assert!(validate_layout(&f, basis, group, r), "{}", group.entry);
            f.q(
                basis + r.record as u64,
                r.marker.parse::<u64>().unwrap() ^ 1,
            );
            assert!(!validate_layout(&f, basis, group, r));
            count += 1;
        }
    }
    assert_eq!(count, 1791);
}
#[test]
fn private_page_bytes_and_clothing_pools_match_preserved_csharp() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("../assets/outfit-policy-regressions.json")).unwrap();
    assert_eq!(oracle["validLayouts"], 1791);
    assert_eq!(oracle["changedMarkersRejected"], 1791);
    for case in oracle["pools"].as_array().unwrap() {
        let pool = clothing_pool(
            case["names"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap()),
        );
        assert_eq!(serde_json::to_value(pool).unwrap(), case["pool"]);
    }
    for case in oracle["pages"].as_array().unwrap() {
        let n = |key: &str| case[key].as_u64().unwrap();
        let id = Identity {
            pid: n("pid") as u32,
            started: case["started"].as_str().unwrap().into(),
            path: "C:\\Fixture\\NBA2K27.exe".into(),
        };
        let key = PlayerKey {
            controller: n("controller"),
            slot: n("slot") as u8,
            description: n("description"),
            model: n("model"),
            roster: n("roster"),
        };
        let items = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let actual = page_bytes(
            &id,
            &key,
            n("sourcePointer"),
            n("allocation"),
            n("index") as usize,
            case["rosterShoes"].as_bool().unwrap(),
            &items,
        )
        .unwrap();
        let encoded = case["bytes"].as_str().unwrap();
        let expected = (0..encoded.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}
#[test]
fn selection_is_native_owned_and_unknown_or_torn_sources_are_not_offered() {
    let mut f = Fixture::selection();
    let id = game();
    let s = capture(&f, &id, &[]).unwrap();
    assert_eq!(s.players.len(), 4);
    assert!(s.unavailable.is_empty());
    assert_eq!(s.players[0].name, "Stephen Curry");
    assert_eq!(s.players[0].pool.as_deref(), Some("male"));
    let player = s.players[0].clone();
    f.q(player.key.description + 0xC8, 0x600000088);
    let s = capture(&f, &id, &[]).unwrap();
    assert!(!s.players.iter().any(|p| p.key.slot == 0));
    assert_eq!(s.unavailable, [0]);
    let mut f = Fixture::selection();
    f.tear = Some(BASE + 0xAC77E28);
    assert!(capture(&f, &id, &[]).is_err());
    let mut f = Fixture::selection();
    f.q(BASE + LINEUP + 0xC0 + 8, player.key.roster);
    assert!(capture(&f, &id, &[]).is_err());
    let mut f = Fixture::selection();
    f.d(BASE + 0xAC77660 + 64, 0x079964C0);
    assert!(capture(&f, &id, &[]).is_err());
}
#[test]
fn composed_pages_are_exact_readonly_owned_and_foreign_buffers_are_never_adopted() {
    let mut f = Fixture::selection();
    let id = game();
    let original = capture(&f, &id, &[]).unwrap().players.remove(0);
    let composed = compose(&original, &outfit()).unwrap();
    let page = f.private(&id, &original, &composed);
    let current = capture(&f, &id, &[page.clone()]).unwrap();
    let p = current.players.iter().find(|p| p.key.slot == 0).unwrap();
    assert_eq!(p.items, composed.items);
    assert_eq!(p.original_pointer, original.original_pointer);
    assert!(capture(&f, &id, &[]).unwrap().unavailable.contains(&0));
    let mut other = id.clone();
    other.started = "134366400000000001".into();
    assert!(
        capture(&f, &other, &[page.clone()])
            .unwrap()
            .unavailable
            .contains(&0)
    );
    f.regions.get_mut(&page.allocation).unwrap().protection = 4;
    assert!(
        capture(&f, &id, &[page.clone()])
            .unwrap()
            .unavailable
            .contains(&0)
    );
    f.regions.get_mut(&page.allocation).unwrap().protection = 2;
    f.put(page.allocation + 256, &[0, 0]);
    assert!(capture(&f, &id, &[page]).unwrap().unavailable.contains(&0));
}
#[test]
fn page_and_composition_preflight_rejects_incompatible_pool_unverified_uniform_and_bad_names() {
    let f = Fixture::selection();
    let id = game();
    let p = capture(&f, &id, &[]).unwrap().players.remove(0);
    let mut other = p.clone();
    other.pool = Some("female".into());
    assert!(compose(&other, &outfit()).is_err());
    for items in [
        vec![],
        vec!["a".into()],
        vec!["a".into(); 8],
        vec!["bad name".into(), "b".into()],
        vec!["x".repeat(256), "b".into()],
    ] {
        assert!(
            page_bytes(
                &id,
                &p.key,
                p.original_pointer,
                0x800000000,
                0,
                false,
                &items
            )
            .is_err()
        );
    }
    let composed = compose(&p, &outfit()).unwrap();
    assert!(
        page_bytes(
            &id,
            &p.key,
            p.original_pointer,
            0x800000001,
            0,
            false,
            &composed.items
        )
        .is_err()
    );
    let mut bad = p.key.clone();
    bad.slot = 10;
    assert!(
        page_bytes(
            &id,
            &bad,
            p.original_pointer,
            0x800000000,
            0,
            false,
            &composed.items
        )
        .is_err()
    );
    let c = outfits::catalog().unwrap();
    if let Some(uniform) = c
        .presets
        .iter()
        .filter(|p| {
            p.pool.as_deref() == Some("male") && p.original.iter().any(|n| n.starts_with("UNIFORM"))
        })
        .find_map(|v| c.original_outfit(&v.id, "Uniform").ok())
    {
        assert!(compose(&p, &uniform).is_err());
    }
}
#[test]
fn cached_wardrobe_hints_still_recheck_source_owner_lineup_names_and_page_integrity() {
    let mut f = Fixture::selection();
    let id = game();
    let p = capture(&f, &id, &[]).unwrap().players.remove(0);
    revalidate(&f, &id, &p, &[]).unwrap();
    assert_eq!(dirty(&f, &p.key).unwrap(), 0);
    let mut bad = p.clone();
    bad.group_index = usize::MAX;
    assert!(revalidate(&f, &id, &bad, &[]).is_err());
    bad = p.clone();
    bad.group_basis = u64::MAX;
    assert!(revalidate(&f, &id, &bad, &[]).is_err());
    f.q(p.key.description + 0xF0, 0x900000000);
    assert!(revalidate(&f, &id, &p, &[]).is_err());
    f.q(p.key.description + 0xF0, 0);
    f.q(BASE + LINEUP + 0xC0 + 8, p.key.roster);
    assert!(revalidate(&f, &id, &p, &[]).is_err());
    f.q(BASE + LINEUP + 0xC0 + 8, p.key.roster + 0x4f8);
    let g = &groups()[p.group_index];
    let name = p.group_basis + g.records[0].strings[0] as u64;
    f.put(name, &[0, 0]);
    assert!(revalidate(&f, &id, &p, &[]).is_err());
    let mut f = Fixture::selection();
    let c = compose(&p, &outfit()).unwrap();
    let page = f.private(&id, &p, &c);
    let current = capture(&f, &id, &[page.clone()]).unwrap().players.remove(0);
    revalidate(&f, &id, &current, &[page.clone()]).unwrap();
    assert!(revalidate(&f, &id, &current, &[]).is_err());
    let mut wrong = id.clone();
    wrong.started.push('1');
    assert!(revalidate(&f, &wrong, &current, &[page.clone()]).is_err());
    let mut invalid = page.clone();
    invalid.allocation = u64::MAX;
    assert!(
        invalid
            .verify(&f, &id, &current.key, &current.source)
            .is_err()
    );
    f.put(page.allocation + 300, &[99]);
    assert!(revalidate(&f, &id, &current, &[page]).is_err());
}
