//! Also executed without cfg(test) by the isolated native API harness. All
//! memory is sparse fake data; only fresh synthetic artifact journals touch disk.
use crate::{
    blacktop_arena::{Inspect, Region},
    blacktop_native::Memory,
    blacktop_outfits::{self as o},
    outfit_selection_v2 as v2,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
const BASE: u64 = 0x180000000;
const CONTROLLER: u64 = 0x400000000;
const STOCK: u64 = 0x600000000;
const ROSTER: u64 = 0x700000000;
#[derive(Clone)]
struct Fake {
    bytes: BTreeMap<u64, u8>,
    ranges: Vec<(u64, usize)>,
    regions: BTreeMap<u64, Region>,
    tear: Option<u64>,
    seen: Cell<usize>,
    reads: RefCell<Vec<(u64, usize)>>,
}
impl Fake {
    fn put(&mut self, at: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.insert(at + i as u64, *v);
        }
    }
    fn q(&mut self, at: u64, n: u64) {
        self.put(at, &n.to_le_bytes());
    }
    fn d(&mut self, at: u64, n: u32) {
        self.put(at, &n.to_le_bytes());
    }
    fn text(&mut self, at: u64, s: &str) {
        self.put(
            at,
            &s.encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
    fn rip(&mut self, rva: u64, prefix: &[u8], target: u64) {
        let mut b = prefix.to_vec();
        b.extend_from_slice(
            &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
        );
        self.put(BASE + rva, &b);
    }
    fn selection() -> Self {
        let mut f = Self {
            bytes: BTreeMap::new(),
            ranges: vec![],
            regions: BTreeMap::new(),
            tear: None,
            seen: Cell::new(0),
            reads: RefCell::new(Vec::new()),
        };
        let mut h = vec![0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[60..64].copy_from_slice(&0x80u32.to_le_bytes());
        h[0x80..0x84].copy_from_slice(b"PE\0\0");
        h[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        h[0x88..0x8C].copy_from_slice(&0x6AC53765u32.to_le_bytes());
        h[0x98..0x9A].copy_from_slice(&0x20Bu16.to_le_bytes());
        h[0xD0..0xD4].copy_from_slice(&0x35BDE000u32.to_le_bytes());
        f.put(BASE, &h);
        let getters: serde_json::Value =
            serde_json::from_str(include_str!("../assets/rules-v2-getters.json")).unwrap();
        for g in getters["fields"].as_array().unwrap() {
            let b = g["hex"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            f.put(BASE + g["rva"].as_u64().unwrap(), &b);
        }
        for (r, p, t) in [
            (0x3457D80, &[0x48, 0x8D, 0x05][..], 0x791E158),
            (0x3457D87, &[0x48, 0x89, 0x05], v2::ROOT),
            (0x25AA3D0, &[0x48, 0x8B, 0x05], 0xAB5CE20),
            (0x308C649, &[0x48, 0x8D, 0x05], 0xAD07408),
            (0x308C650, &[0x48, 0x63, 0x0D], 0xAD07BCC),
            (0x300D703, &[0x48, 0x63, 0x05], 0xAD07BD0),
            (0x32F799A, &[0x89, 0x05], v2::LINEUP + 4),
            (0x32F79A0, &[0x89, 0x05], v2::LINEUP + 8),
        ] {
            f.rip(r, p, t);
        }
        f.put(BASE + 0x32FB9B6, &[0x8B, 0x81, 0xA8, 0x0E, 0, 0]);
        f.put(BASE + 0x24F7EC5, &[0x80, 0x3D, 0xC4, 0x58, 0x66, 8, 0]);
        f.put(
            BASE + 0x24F5450,
            &[
                0x48, 0x8B, 0x05, 0xC9, 0x79, 0x66, 8, 0x8B, 0x80, 4, 0x0A, 0, 0, 0xC3,
            ],
        );
        f.q(BASE + 0x791E158, BASE + 0x23120);
        f.q(BASE + 0x791E160, BASE + 0x346B350);
        for &(at, b) in v2::FIELD_ANCHORS {
            f.put(BASE + at, b);
        }
        for &(t, offset, method) in v2::VIRTUAL_ANCHORS {
            f.q(BASE + t + offset, BASE + method);
        }
        f.d(BASE + 0xAD07BD0, 1);
        f.d(BASE + 0xAD07BCC, 1);
        f.d(BASE + 0xAD07408 + 64, 0x92DF06DE);
        f.d(BASE + v2::ROOT + 0xEA8, 3);
        f.put(BASE + 0xAB5D790, &[0]);
        f.q(BASE + v2::ROOT, BASE + 0x791E158);
        f.q(BASE + 0xAB5CE20, 0x300000000);
        f.put(0x300000000, &[0]);
        f.d(0x300000000 + 0xA04, 6);
        f.q(BASE + v2::ROOT + 0x1080, CONTROLLER);
        f.ranges.push((CONTROLLER, 0x7B0 * 10 + 0x40));
        f.put(CONTROLLER + 0x18, &[1]);
        f.d(BASE + v2::LINEUP + 4, 4);
        f.d(BASE + v2::LINEUP + 8, 4);
        f.d(BASE + v2::LINEUP + 0xB8, 4);
        let g = &o::groups()[0];
        let record = &g.records[0];
        f.ranges.push((STOCK, g.length));
        f.q(STOCK + 16, g.count as u64);
        f.q(STOCK + g.descriptor as u64, g.hash.parse().unwrap());
        f.q(STOCK + g.descriptor as u64 + 8, g.outfit_count as u64);
        f.q(STOCK + record.record as u64, record.marker.parse().unwrap());
        f.q(STOCK + record.record as u64 + 8, 2);
        f.q(
            STOCK + record.record as u64 + 16,
            STOCK + record.array as u64,
        );
        for i in 0..2 {
            f.q(
                STOCK + record.array as u64 + i as u64 * 8,
                STOCK + record.strings[i] as u64,
            );
            f.text(STOCK + record.strings[i] as u64, &record.names[i]);
        }
        for slot in 0..4u64 {
            let model = 0x500000000 + slot * 0x2000;
            let desc = model + 0x800;
            let roster = ROSTER + slot * 0x4F8;
            f.q(CONTROLLER + 0x20 + slot * 0x7B0, model);
            f.q(CONTROLLER + 0x28 + slot * 0x7B0, desc);
            f.q(model, BASE + v2::MODEL);
            f.ranges.push((desc, 0xF8));
            f.q(desc, BASE + v2::DESCRIPTION);
            f.q(desc + 0xB8, roster);
            f.q(desc + 0xC8, STOCK + record.array as u64);
            f.d(desc + 0xD0, 2);
            f.ranges.push((roster, 0x4F8));
            f.text(roster, "Curry");
            f.text(roster + 0x28, "Stephen");
            f.put(roster + 0x110, &(100 + slot as u16).to_le_bytes());
            f.q(BASE + v2::LINEUP + 0xC0 + slot * 8, roster);
        }
        f
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        ensure!(len > 0 && len <= 4096, "Fake bounded read");
        self.reads.borrow_mut().push((at, len));
        let mut b = (0..len)
            .map(|i| {
                self.bytes
                    .get(&(at + i as u64))
                    .copied()
                    .or_else(|| {
                        self.ranges
                            .iter()
                            .any(|(a, n)| at + i as u64 >= *a && at + (i as u64) < *a + *n as u64)
                            .then_some(0)
                    })
                    .context("Unmapped sparse outfit test byte")
            })
            .collect::<Result<Vec<_>>>()?;
        if self.tear == Some(at) {
            let n = self.seen.get();
            self.seen.set(n + 1);
            if n > 0 {
                b[0] ^= 1;
            }
        }
        Ok(b)
    }
}
impl Inspect for Fake {
    fn region(&self, at: u64) -> Result<Region> {
        self.regions
            .get(&at)
            .copied()
            .context("No synthetic owned page")
    }
}
fn game() -> Identity {
    Identity {
        pid: 311,
        started: "134366400000000000".into(),
        path: r"C:\Fixture\NBA2K27.exe".into(),
    }
}

fn seed() -> Fake {
    let mut f = Fake::selection();
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../assets/outfit-match-v2-profile.json")).unwrap();
    for row in p["methods"].as_array().unwrap() {
        let b = row["hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        f.put(BASE + row["rva"].as_u64().unwrap(), &b);
    }
    for t in p["virtualTables"].as_array().unwrap() {
        let end = t["slots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["offset"].as_u64().unwrap() + 8)
            .max()
            .unwrap();
        f.ranges
            .push((BASE + t["rva"].as_u64().unwrap(), end as usize));
        for s in t["slots"].as_array().unwrap() {
            f.q(
                BASE + t["rva"].as_u64().unwrap() + s["offset"].as_u64().unwrap(),
                BASE + s["method"].as_u64().unwrap(),
            );
        }
    }
    f.d(BASE + 0xAD07408 + 64, 0xB4D37B22);
    f.d(BASE + v2::ROOT + 0xEA8, 1);
    f.put(BASE + 0xAB5D790, &[1]);
    f.ranges
        .push((BASE + crate::outfit_match_v2::TABLE, 30 * 0x50));
    let row = BASE + crate::outfit_match_v2::TABLE;
    let loaded = 0xA00000000;
    let actor = 0xB00000000;
    let primary = 0xC00000000;
    let state = 0xD00000000;
    let desc = actor + 0xF30;
    let owner = 0xE00000000;
    let array = owner + 0x1000;
    let owned = owner + 0x2000;
    f.q(row, BASE + crate::outfit_match_v2::WRAPPER);
    // Live V2 layout: the table and roster hold the manager interface base.
    f.q(row + 0x10, primary + 0x78);
    f.q(row + 0x18, state);
    f.put(row + 0x20, &[1]);
    f.q(row + 0x28, loaded);
    f.q(loaded + 0x500, ROSTER);
    f.q(loaded + 0x518, primary + 0x78);
    f.q(loaded + 0x520, state);
    f.ranges.push((primary, 0x148));
    f.q(primary, BASE + crate::outfit_match_v2::MANAGER);
    let active = BASE + crate::outfit_match_v2::ACTIVE;
    f.q(active, BASE + 0x687E1E8);
    f.q(active + 8, primary);
    f.q(active + 0x10, primary);
    f.q(primary + 8, active);
    f.q(primary + 0x10, active);
    let queue = BASE + crate::outfit_match_v2::QUEUE;
    let qhead = BASE + crate::outfit_match_v2::QUEUE_SENTINEL;
    f.q(queue, BASE + 0x68B86F8);
    f.q(qhead, BASE + 0x6860B08);
    f.q(qhead + 8, qhead);
    f.q(qhead + 0x10, qhead);
    f.put(queue + 0x5A, &[1]);
    f.d(BASE + crate::outfit_match_v2::MAIN_THREAD, 999);
    f.q(primary + 0x78, BASE + crate::outfit_match_v2::SECONDARY);
    f.q(primary + 0x110, actor);
    f.d(primary + 0x90, 1);
    let sentinel = primary + 0xF8;
    let node = primary + 0x1000;
    f.q(sentinel, BASE + crate::outfit_match_v2::SENTINEL);
    f.q(sentinel + 8, node);
    f.q(sentinel + 0x10, node);
    f.ranges.push((node, 0x40));
    f.q(node, BASE + crate::outfit_match_v2::MANAGED_INSTANCE);
    f.q(node + 8, sentinel);
    f.q(node + 0x10, sentinel);
    f.d(node + 0x18, 42);
    f.q(node + 0x20, actor);
    f.q(actor, BASE + crate::outfit_match_v2::PLAYER);
    f.ranges.push((desc, 0x2970));
    f.q(desc, BASE + crate::outfit_match_v2::PRIVATE_DESCRIPTION);
    f.q(desc + 0xB8, desc + 0x378);
    f.q(desc + 0xC8, array);
    f.d(desc + 0xD0, 2);
    f.d(desc + 0x88, 42);
    f.q(desc + 0x368, owner);
    f.q(owner, BASE + crate::outfit_match_v2::VARIABLE_OUTFIT);
    f.d(owner + 8, 2);
    f.q(owner + 0x10, array);
    f.q(owner + 0x20, owned);
    for (i, name) in ["tshirt_ss_team_016_orl", "basketball_shorts_black"]
        .iter()
        .enumerate()
    {
        let string = owner + 0x3000 + i as u64 * 512;
        f.ranges.push((string, 512));
        f.text(string, name);
        f.q(array + i as u64 * 8, string);
        f.q(owned + i as u64 * 8, string);
    }
    f
}
pub(crate) fn run() -> Result<()> {
    use crate::outfit_match_v2 as policy;
    let f = seed();
    policy::verify_profile(&f)?;
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../assets/outfit-match-v2-profile.json"))?;
    for row in p["methods"].as_array().unwrap() {
        let mut bad = f.clone();
        let at = BASE + row["rva"].as_u64().unwrap();
        let b = bad.bytes[&at];
        bad.put(at, &[b ^ 1]);
        ensure!(
            policy::verify_profile(&bad).is_err(),
            "Modified native method accepted"
        );
    }
    println!("PASS 25 independently found native match method bodies reject mutation");
    let rows = policy::capture(&f, &[ROSTER])?;
    ensure!(
        rows.len() == 1 && rows[0].description.names.len() == 2,
        "Match identity capture"
    );
    let row = &rows[0];
    ensure!(
        row.primary == 0xC00000000
            && row.actor == 0xB00000000
            && row.original_roster == ROSTER
            && row.description.address == row.actor + 0xF30,
        "Primary/secondary owner adjustment"
    );
    ensure!(
        row.table_manager == row.primary + 0x78,
        "Live interface-base table pointer was not normalized"
    );
    // The full-object form is still accepted, and a mismatched roster
    // back-pointer is refused in either form.
    let mut full = f.clone();
    full.q(row.row + 0x10, row.primary);
    full.q(row.loaded_roster + 0x518, row.primary);
    ensure!(
        policy::capture(&full, &[ROSTER])?[0].primary == row.primary,
        "Full-object manager form refused"
    );
    let mut split = f.clone();
    split.q(row.loaded_roster + 0x518, row.primary);
    ensure!(
        policy::capture(&split, &[ROSTER]).is_err(),
        "Mismatched roster manager back-pointer accepted"
    );
    println!(
        "PASS V2 wrapper primary manager, secondary interface, actor, original roster and private description ownership"
    );
    for (at, b) in [
        (row.primary + 0x78, vec![0; 8]),
        (row.primary + 0x110, vec![0; 8]),
        (row.loaded_roster + 0x518, vec![0; 8]),
        (row.loaded_roster + 0x520, vec![0; 8]),
        (row.row + 0x20, vec![0]),
        (row.primary + 0x130, vec![1]),
    ] {
        let mut bad = f.clone();
        bad.put(at, &b);
        ensure!(
            policy::capture(&bad, &[ROSTER]).is_err(),
            "Changed owner accepted"
        );
    }
    println!("PASS manager, actor, roster/state, boolean and pending ownership guards");
    let mut bad = f.clone();
    bad.q(row.description.owner + 0x2000, 1);
    ensure!(
        policy::capture(&bad, &[ROSTER]).is_err(),
        "Alias list mismatch"
    );
    bad = f.clone();
    bad.q(row.primary + 0x1000 + 0x38, 1);
    ensure!(
        policy::capture(&bad, &[ROSTER]).is_err(),
        "Pending native managed node"
    );
    bad = f.clone();
    bad.q(row.primary + 0x1000 + 0x10, row.primary + 0x1000);
    ensure!(
        policy::capture(&bad, &[ROSTER]).is_err(),
        "Cyclic managed node"
    );
    println!(
        "PASS owned/exposed arrays, current-node pending and bounded intrusive-list cycle guards"
    );
    let source = row.description.clone();
    let mut copy = source.clone();
    copy.address += 0x100000;
    copy.array += 0x10000;
    copy.owner += 0x10000;
    copy.owned_array += 0x10000;
    copy.items.iter_mut().for_each(|p| *p += 0x10000);
    copy.bytes[0x368..0x370].copy_from_slice(&copy.owner.to_le_bytes());
    policy::independent(&source, &copy, row.actor)?;
    for x in [0x368, 0x370, 0x2940, 0x2948, 0x2950, 0x2958] {
        for y in [0x368, 0x370, 0x2940, 0x2948, 0x2950, 0x2958] {
            let mut a = source.clone();
            let mut b = copy.clone();
            a.bytes[x..x + 8].copy_from_slice(&0xF00000000u64.to_le_bytes());
            b.bytes[y..y + 8].copy_from_slice(&0xF00000000u64.to_le_bytes());
            ensure!(
                policy::independent(&a, &b, row.actor).is_err(),
                "Destructor-owned cross alias accepted"
            );
        }
    }
    let mut alias = copy.clone();
    alias.address = row.actor + 8;
    ensure!(
        policy::independent(&source, &alias, row.actor).is_err(),
        "Actor alias accepted"
    );
    let mut alias = copy.clone();
    alias.items[0] = source.items[0];
    ensure!(
        policy::independent(&source, &alias, row.actor).is_err(),
        "Owned string alias accepted"
    );
    let mut grown = copy.clone();
    grown.array += 0x4000;
    grown.count = 3;
    grown.names.push("acc_headband_black".into());
    grown.bytes[0xC8..0xD0].copy_from_slice(&grown.array.to_le_bytes());
    grown.bytes[0xD0..0xD4].copy_from_slice(&grown.count.to_le_bytes());
    policy::expanded(&copy, &grown, &grown.names)?;
    let mut unrelated = grown.clone();
    unrelated.bytes[0xF0] ^= 1;
    ensure!(
        policy::expanded(&copy, &unrelated, &grown.names).is_err(),
        "Expansion changed uniform context"
    );
    println!(
        "PASS independent clone ownership (36 destructor-allocation aliases), actor/string alias rejection and narrow expansion"
    );
    policy::off_main_thread(&f, 1234)?;
    ensure!(
        policy::off_main_thread(&f, 999).is_err(),
        "Native main thread accepted"
    );
    let mut bad = f.clone();
    bad.put(BASE + policy::QUEUE + 0x5A, &[0]);
    ensure!(
        policy::capture(&bad, &[ROSTER]).is_err(),
        "Uninitialized queue accepted"
    );
    bad = f.clone();
    bad.q(BASE + policy::QUEUE_SENTINEL + 8, 0);
    ensure!(
        policy::capture(&bad, &[ROSTER]).is_err(),
        "Busy queue accepted"
    );
    bad = f.clone();
    bad.q(rows[0].primary + 8, 0);
    ensure!(
        policy::capture(&bad, &[ROSTER]).is_err(),
        "Broken active-manager membership"
    );
    println!(
        "PASS queue initialization/empty sentinel, active-manager membership and native main-thread exclusion"
    );
    ensure!(
        !policy::WRITES_SUPPORTED,
        "Read-only profile accidentally granted writes"
    );
    println!(
        "PASS no native function call, queue transfer, old profile enable or worker authority"
    );
    Ok(())
}
pub(crate) fn disk_profile(path: &std::path::Path) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Seek, SeekFrom};
    struct Disk {
        file: RefCell<std::fs::File>,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let rva = at.checked_sub(self.base()).context("Disk address")?;
            let offset = if rva < 4096 {
                rva
            } else {
                let &(start, bytes, raw) = self
                    .sections
                    .iter()
                    .find(|(start, bytes, _)| {
                        rva >= *start
                            && rva
                                .checked_add(len as u64)
                                .is_some_and(|v| v <= start + bytes)
                    })
                    .context("Disk RVA outside raw section")?;
                let _ = bytes;
                raw + rva - start
            };
            let mut f = self.file.borrow_mut();
            f.seek(SeekFrom::Start(offset))?;
            let mut b = vec![0; len];
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut chunk = vec![0; 8 * 1024 * 1024];
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    ensure!(
        format!("{:X}", hash.finalize())
            == "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1",
        "Installed Version 2 file SHA differs"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut h = vec![0; 4096];
    file.read_exact(&mut h)?;
    let pe = u32::from_le_bytes(h[60..64].try_into()?) as usize;
    let n = u16::from_le_bytes(h[pe + 6..pe + 8].try_into()?) as usize;
    let opt = u16::from_le_bytes(h[pe + 20..pe + 22].try_into()?) as usize;
    ensure!(
        n <= 32 && pe + 24 + opt + n * 40 <= 4096,
        "Disk section table bounds"
    );
    let sections = (0..n)
        .map(|i| {
            let at = pe + 24 + opt + i * 40;
            (
                u32::from_le_bytes(h[at + 12..at + 16].try_into().unwrap()) as u64,
                u32::from_le_bytes(h[at + 16..at + 20].try_into().unwrap()) as u64,
                u32::from_le_bytes(h[at + 20..at + 24].try_into().unwrap()) as u64,
            )
        })
        .collect();
    crate::outfit_match_v2::verify_profile(&Disk {
        file: RefCell::new(file),
        sections,
    })?;
    println!(
        "PASS installed Version 2 full SHA plus 25 native match method bodies and 10 class tables"
    );
    Ok(())
}

#[cfg(test)]
#[test]
fn version_two_match_ownership_only() {
    run().unwrap();
}

#[cfg(test)]
#[test]
fn match_capture_batches_fresh_image_reads_without_caching_profile_or_bindings() -> Result<()> {
    use crate::outfit_match_v2 as policy;
    let f = seed();
    let _ = policy::capture(&f, &[ROSTER])?;
    let reads = f.reads.borrow();
    ensure!(
        reads
            .iter()
            .filter(|(at, len)| *at == BASE + policy::TABLE && *len == 30 * 0x50)
            .count()
            == 1,
        "Wrapper table was read repeatedly"
    );
    ensure!(
        reads.contains(&(BASE + policy::TABLE, 30 * 0x50)),
        "Missing bounded wrapper table read"
    );
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../assets/outfit-match-v2-profile.json"))?;
    for table in p["virtualTables"].as_array().unwrap() {
        let at = BASE + table["rva"].as_u64().unwrap();
        let end = table["slots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slot| slot["offset"].as_u64().unwrap() + 8)
            .max()
            .unwrap();
        ensure!(
            reads
                .iter()
                .filter(|(address, _)| (*address >= at) && (*address < at + end))
                .count()
                == 1,
            "Virtual table did not use one fresh bounded read"
        );
    }
    drop(reads);
    for table in p["virtualTables"].as_array().unwrap() {
        for slot in table["slots"].as_array().unwrap() {
            let mut changed = f.clone();
            changed.q(
                BASE + table["rva"].as_u64().unwrap() + slot["offset"].as_u64().unwrap(),
                0,
            );
            ensure!(
                policy::capture(&changed, &[ROSTER]).is_err(),
                "Changed virtual slot was cached or accepted"
            );
        }
    }
    let mut changed = f.clone();
    changed.tear = Some(BASE + policy::TABLE + 0x10);
    ensure!(
        policy::capture(&changed, &[ROSTER]).is_ok(),
        "First independent binding read failed"
    );
    ensure!(
        policy::capture(&changed, &[ROSTER]).is_err(),
        "Changed live binding accepted after batched scan"
    );
    Ok(())
}

#[cfg(test)]
fn carry_choice() -> crate::blacktop_outfit_match::Choice {
    crate::blacktop_outfit_match::Choice {
        choice_id: "a".repeat(48),
        roster: ROSTER,
        slot: 0,
        name: "fixture".into(),
        items: vec!["top".into(), "bottom".into(), "sock".into()],
    }
}
#[cfg(test)]
#[test]
fn composite_handoff_keeps_full_fresh_proofs_with_two_owner_and_two_lineup_checks() -> Result<()> {
    use crate::outfit_carry_v2 as carry;
    let old = seed();
    let choice = carry_choice();
    let selected = carry::lineup(&old)?;
    let players = carry::ready(&old, &[choice.clone()])?;
    old.reads.borrow_mut().clear();
    let old_owners = Cell::new(0);
    let old_owner = || -> Result<()> {
        old_owners.set(old_owners.get() + 1);
        v2::verify_profile(&old)
    };
    // Previous production closure's four independent owner gates, three full
    // lineup captures and one full match capture; serves as measured reference.
    old_owner()?;
    old_owner()?;
    ensure!(
        crate::rules_v2::boundary(&crate::rules_v2::context(&old)?, BASE) == "match",
        "old boundary"
    );
    old_owner()?;
    ensure!(carry::lineup(&old)? == selected, "old lineup");
    old_owner()?;
    ensure!(
        carry::ready(&old, &[choice.clone()])? == players,
        "old readiness"
    );
    let old_reads = old.reads.borrow().len();
    let fresh = seed();
    fresh.reads.borrow_mut().clear();
    let owners = Cell::new(0);
    ensure!(
        carry::handoff(&fresh, &[choice.clone()], &selected, &players, || {
            owners.set(owners.get() + 1);
            v2::verify_profile(&fresh)
        })?,
        "composite rejected unchanged full proof"
    );
    let reads = fresh.reads.borrow();
    let fresh_reads = reads.len();
    ensure!(
        old_owners.get() == 4 && owners.get() == 2,
        "owner gate count"
    );
    ensure!(
        reads
            .iter()
            .filter(|(at, _)| *at == BASE + v2::LINEUP + 0xC0)
            .count()
            == 4,
        "must read first roster before/after EACH of the two full lineup captures"
    );
    ensure!(
        fresh_reads < old_reads,
        "composite did not reduce fresh reads"
    );
    println!(
        "composite handoff fresh reads {old_reads}->{fresh_reads}; owner scans4->2, full lineups3->2"
    );
    drop(reads);
    // Every new handoff must read the native method bytes afresh.
    let mut changed = fresh.clone();
    let anchor = v2::FIELD_ANCHORS[0].0;
    let byte = changed.bytes[&(BASE + anchor)];
    changed.put(BASE + anchor, &[byte ^ 1]);
    assert!(
        carry::handoff(&changed, &[choice.clone()], &selected, &players, || {
            v2::verify_profile(&changed)
        })
        .is_err()
    );
    let mut wrong = players.clone();
    wrong[0].description += 8;
    assert!(!carry::handoff(
        &fresh,
        &[choice],
        &selected,
        &wrong,
        || v2::verify_profile(&fresh)
    )?);
    Ok(())
}
#[cfg(test)]
#[test]
fn composite_handoff_refuses_lineup_context_and_capture_changes_during_read() -> Result<()> {
    use crate::outfit_carry_v2 as carry;
    struct Changing {
        memory: RefCell<Fake>,
        changed: Cell<bool>,
        context: bool,
    }
    impl Memory for Changing {
        fn base(&self) -> u64 {
            BASE
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let bytes = self.memory.borrow().read(at, len)?;
            if at == BASE + crate::outfit_match_v2::TABLE && !self.changed.replace(true) {
                let mut m = self.memory.borrow_mut();
                if self.context {
                    m.d(BASE + v2::ROOT + 0xEA8, 2);
                } else {
                    m.q(BASE + v2::LINEUP + 0xC0, ROSTER + 8);
                }
            }
            Ok(bytes)
        }
    }
    let baseline = seed();
    let choice = carry_choice();
    let selected = carry::lineup(&baseline)?;
    let players = carry::ready(&baseline, &[choice.clone()])?;
    for context in [false, true] {
        let m = Changing {
            memory: RefCell::new(seed()),
            changed: Cell::new(false),
            context,
        };
        let owners = Cell::new(0);
        assert!(!carry::handoff(
            &m,
            &[choice.clone()],
            &selected,
            &players,
            || {
                owners.set(owners.get() + 1);
                Ok(())
            }
        )?);
        assert_eq!(
            owners.get(),
            2,
            "ending owner gate skipped after capture refusal"
        );
    }
    let mut missing = seed();
    missing.q(BASE + crate::outfit_match_v2::TABLE + 0x28, 0);
    assert!(!carry::handoff(
        &missing,
        &[choice],
        &selected,
        &players,
        || Ok(())
    )?);
    Ok(())
}
#[cfg(test)]
#[test]
fn composite_handoff_late_owner_failure_is_not_deferred_or_cached() -> Result<()> {
    use crate::outfit_carry_v2 as carry;
    let m = seed();
    let choice = carry_choice();
    let selected = carry::lineup(&m)?;
    let players = carry::ready(&m, &[choice.clone()])?;
    let owners = Cell::new(0);
    let error = carry::handoff(&m, &[choice.clone()], &selected, &players, || {
        let n = owners.get() + 1;
        owners.set(n);
        ensure!(n != 2, "late cancellation/owner change");
        Ok(())
    })
    .unwrap_err();
    assert!(error.to_string().contains("late cancellation/owner change"));
    assert_eq!(owners.get(), 2);
    let reads = m.reads.borrow().len();
    assert!(
        carry::handoff(&m, &[choice], &selected, &players, || anyhow::bail!(
            "owner ended"
        ))
        .is_err()
    );
    assert_eq!(
        m.reads.borrow().len(),
        reads,
        "read after initial owner failure"
    );
    Ok(())
}

#[cfg(test)]
#[test]
fn initial_sample_equivalent_fresh_readiness_uses_two_owner_and_two_lineup_proofs() -> Result<()> {
    use crate::{
        outfit_carry_v2 as carry,
        outfit_journal::runtime::{Boundary, Driver, Readiness},
    };
    struct Old<'a> {
        m: &'a Fake,
        owners: Cell<usize>,
    }
    impl Driver for Old<'_> {
        fn check(&self) -> Result<()> {
            self.owners.set(self.owners.get() + 1);
            v2::verify_profile(self.m)
        }
        fn boundary(&self) -> Result<Boundary> {
            self.check()?;
            Ok(
                if crate::rules_v2::boundary(&crate::rules_v2::context(self.m)?, BASE) == "match" {
                    Boundary::Match
                } else {
                    Boundary::Other
                },
            )
        }
        fn lineup(&self) -> Result<Vec<u64>> {
            self.check()?;
            carry::lineup(self.m)
        }
        fn ready(
            &self,
            c: &[crate::blacktop_outfit_match::Choice],
        ) -> Result<Vec<crate::blacktop_outfit_match::MatchPlayer>> {
            self.check()?;
            carry::ready(self.m, c)
        }
    }
    let old = seed();
    let driver = Old {
        m: &old,
        owners: Cell::new(0),
    };
    let choice = carry_choice();
    let original = driver.sample(true, |_| Ok(vec![choice.clone()]))?;
    let old_reads = old.reads.borrow().len();
    let fresh = seed();
    let owners = Cell::new(0);
    let choose_calls = Cell::new(0);
    let sample = carry::sample(
        &fresh,
        true,
        |_| {
            choose_calls.set(choose_calls.get() + 1);
            Ok(vec![choice.clone()])
        },
        || {
            owners.set(owners.get() + 1);
            v2::verify_profile(&fresh)
        },
    )?;
    let fresh_reads = fresh.reads.borrow().len();
    match (original.readiness, sample.readiness) {
        (
            Some(Readiness::Ready {
                lineup: a,
                choices: b,
                players: c,
            }),
            Some(Readiness::Ready {
                lineup: x,
                choices: y,
                players: z,
            }),
        ) => assert_eq!((a, b, c), (x, y, z)),
        _ => anyhow::bail!("initial sample changed accepted readiness"),
    }
    assert_eq!(driver.owners.get(), 4);
    assert_eq!(owners.get(), 2);
    assert_eq!(choose_calls.get(), 1);
    assert_eq!(
        fresh
            .reads
            .borrow()
            .iter()
            .filter(|(at, _)| *at == BASE + v2::LINEUP + 0xC0)
            .count(),
        4
    );
    assert!(fresh_reads < old_reads);
    println!(
        "initial sample fresh reads {old_reads}->{fresh_reads}; owner scans4->2 and full lineups3->2"
    );
    Ok(())
}
#[cfg(test)]
#[test]
fn initial_sample_rearm_nonmatch_and_empty_choices_never_read_loaded_actors() -> Result<()> {
    use crate::{
        outfit_carry_v2 as carry,
        outfit_journal::runtime::{Boundary, Readiness},
    };
    for (m, allow, boundary) in [
        (seed(), false, Boundary::Match),
        (Fake::selection(), true, Boundary::Selection),
    ] {
        let owners = Cell::new(0);
        let sample = carry::sample(
            &m,
            allow,
            |_| anyhow::bail!("choose must not run"),
            || {
                owners.set(owners.get() + 1);
                Ok(())
            },
        )?;
        assert_eq!(sample.boundary, boundary);
        assert!(sample.readiness.is_none());
        assert_eq!(owners.get(), 2);
        assert!(
            !m.reads
                .borrow()
                .iter()
                .any(|(at, _)| *at == BASE + crate::outfit_match_v2::TABLE)
        );
        assert_eq!(sample.match_after_ms.is_some(), boundary == Boundary::Match);
    }
    let m = seed();
    let calls = Cell::new(0);
    let sample = carry::sample(
        &m,
        true,
        |_| {
            calls.set(calls.get() + 1);
            Ok(vec![])
        },
        || Ok(()),
    )?;
    assert!(matches!(sample.readiness, Some(Readiness::NoChoices)));
    assert_eq!(calls.get(), 1);
    assert!(
        !m.reads
            .borrow()
            .iter()
            .any(|(at, _)| *at == BASE + crate::outfit_match_v2::TABLE)
    );
    Ok(())
}
#[cfg(test)]
#[test]
fn initial_sample_capture_callback_and_late_owner_failures_keep_fresh_ending_gate() -> Result<()> {
    use crate::{outfit_carry_v2 as carry, outfit_journal::runtime::Readiness};
    let mut m = seed();
    m.q(BASE + crate::outfit_match_v2::TABLE + 0x28, 0);
    let calls = Cell::new(0);
    let sample = carry::sample(
        &m,
        true,
        |_| Ok(vec![carry_choice()]),
        || {
            calls.set(calls.get() + 1);
            Ok(())
        },
    )?;
    assert!(matches!(sample.readiness, Some(Readiness::NotReady(_))));
    assert_eq!(calls.get(), 2);
    calls.set(0);
    let err = carry::sample(
        &m,
        true,
        |_| Ok(vec![carry_choice()]),
        || {
            calls.set(calls.get() + 1);
            ensure!(calls.get() != 2, "late owner ended");
            Ok(())
        },
    )
    .err()
    .context("late owner failure accepted")?;
    assert!(err.to_string().contains("late owner ended"));
    assert_eq!(calls.get(), 2);
    calls.set(0);
    assert!(
        carry::sample(
            &m,
            true,
            |_| anyhow::bail!("ledger invalid"),
            || {
                calls.set(calls.get() + 1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(calls.get(), 2);
    let before = m.reads.borrow().len();
    assert!(
        carry::sample(
            &m,
            true,
            |_| Ok(vec![carry_choice()]),
            || anyhow::bail!("initial owner ended")
        )
        .is_err()
    );
    assert_eq!(m.reads.borrow().len(), before);
    let mut waiting = seed();
    waiting.d(BASE + v2::LINEUP + 0xB8, 1);
    assert!(matches!(
        carry::sample(&waiting, true, |_| anyhow::bail!("no lineup"), || Ok(()))?.readiness,
        Some(Readiness::LineupWaiting)
    ));
    Ok(())
}
#[cfg(test)]
#[test]
fn initial_sample_marks_match_before_loaded_work_and_refuses_lineup_race() -> Result<()> {
    use crate::{outfit_carry_v2 as carry, outfit_journal::runtime::Readiness};
    struct Changing {
        m: RefCell<Fake>,
        changed: Cell<bool>,
        context: bool,
    }
    impl Memory for Changing {
        fn base(&self) -> u64 {
            BASE
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let bytes = self.m.borrow().read(at, len)?;
            if at == BASE + crate::outfit_match_v2::TABLE && !self.changed.replace(true) {
                // Deterministic simulated loaded-read work; marker must precede it.
                std::thread::sleep(std::time::Duration::from_millis(8));
                if self.context {
                    self.m.borrow_mut().d(BASE + v2::ROOT + 0xEA8, 2);
                } else {
                    self.m.borrow_mut().q(BASE + v2::LINEUP + 0xC0, ROSTER + 8);
                }
            }
            Ok(bytes)
        }
    }
    for context in [false, true] {
        let m = Changing {
            m: RefCell::new(seed()),
            changed: Cell::new(false),
            context,
        };
        let owners = Cell::new(0);
        let started = std::time::Instant::now();
        let sample = carry::sample(
            &m,
            true,
            |_| Ok(vec![carry_choice()]),
            || {
                owners.set(owners.get() + 1);
                Ok(())
            },
        )?;
        assert!(matches!(sample.readiness, Some(Readiness::NotReady(_))));
        assert_eq!(owners.get(), 2);
        assert!(sample.readiness_ms >= 8);
        assert!(sample.match_after_ms.unwrap() + 8 <= started.elapsed().as_millis() as u64);
    }
    Ok(())
}

#[cfg(test)]
struct CarryRows {
    memory: Fake,
    fault: u8,
    batches: Cell<usize>,
    calls: RefCell<Vec<(u64, usize)>>,
}
#[cfg(test)]
impl Memory for CarryRows {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.calls.borrow_mut().push((at, len));
        let rows = at == BASE + v2::LINEUP + 0xC0 && len > 8 && len <= 80;
        if rows {
            let n = self.batches.get() + 1;
            self.batches.set(n);
            if self.fault == 1 {
                anyhow::bail!("wider row span unavailable");
            }
            if self.fault == 4 && n > 1 {
                anyhow::bail!("later row batch unavailable");
            }
        }
        let mut bytes = self.memory.read(at, len)?;
        if rows {
            if self.fault == 2 {
                bytes.pop();
            }
            if self.fault == 3 && self.batches.get() > 1 {
                bytes[0] ^= 8;
            }
            if self.fault == 4 && self.batches.get() == 1 {
                bytes[..8].fill(0);
            }
        }
        Ok(bytes)
    }
}
#[cfg(test)]
fn carry_rows_fixture(count: u32, fault: u8) -> CarryRows {
    let mut memory = seed();
    memory.d(BASE + v2::LINEUP + 4, count);
    memory.d(BASE + v2::LINEUP + 8, count);
    memory.d(BASE + v2::LINEUP + 0xB8, count);
    for n in 0..count {
        memory.q(
            BASE + v2::LINEUP + 0xC0 + u64::from(n) * 8,
            ROSTER + u64::from(n) * 0x4F8,
        );
    }
    CarryRows {
        memory,
        fault,
        batches: Cell::new(0),
        calls: RefCell::new(vec![]),
    }
}
#[cfg(test)]
#[test]
fn carry_lineup_rows_batch_two_four_ten_and_partial_error_scalar_fallback() -> Result<()> {
    for count in [2, 4, 10] {
        let expected = (0..count)
            .map(|n| ROSTER + u64::from(n) * 0x4F8)
            .collect::<Vec<_>>();
        for fault in [0, 1, 2] {
            let m = carry_rows_fixture(count, fault);
            assert_eq!(crate::outfit_carry_v2::lineup(&m)?, expected);
            let reads = m.calls.borrow();
            let row_reads = reads
                .iter()
                .filter(|(at, _)| {
                    *at >= BASE + v2::LINEUP + 0xC0
                        && *at < BASE + v2::LINEUP + 0xC0 + u64::from(count) * 8
                })
                .count();
            assert_eq!(m.batches.get(), 2);
            assert_eq!(
                row_reads,
                if fault == 0 {
                    2
                } else {
                    2 * (count as usize + 1)
                },
                "complete scalar fallback before AND after"
            );
        }
        println!(
            "carry lineup{count} fresh pointer reads {}->2; partial/error reread all scalar rows",
            count * 2
        );
    }
    Ok(())
}
#[cfg(test)]
#[test]
fn carry_lineup_rows_never_erase_complete_bad_or_reuse_first_observation() {
    for count in [2, 4, 10] {
        let m = carry_rows_fixture(count, 3);
        assert!(crate::outfit_carry_v2::lineup(&m).is_err());
        assert_eq!(m.batches.get(), 2);
        // First complete block has an invalid pointer. A hypothetical later
        // transport failure/scalar recovery must not erase that observation.
        let bad = carry_rows_fixture(count, 4);
        assert!(crate::outfit_carry_v2::lineup(&bad).is_err());
        assert_eq!(bad.batches.get(), 1);
        // Sparse scalar fallback must also refuse changed complete row bytes.
        let mut scalar = carry_rows_fixture(count, 1);
        scalar.memory.q(BASE + v2::LINEUP + 0xC8, ROSTER);
        assert!(crate::outfit_carry_v2::lineup(&scalar).is_err());
        assert_eq!(scalar.batches.get(), 1);
    }
}

#[cfg(test)]
fn dense_description_fixture() -> Fake {
    let mut f = seed();
    f.ranges.push((0xE00000000, 40));
    f
}
#[cfg(test)]
#[test]
fn description_snapshot_reuses_complete_bytes_and_fresh_bounded_owner_arrays() -> Result<()> {
    use crate::outfit_match_v2 as policy;
    let f = dense_description_fixture();
    let at = 0xB00000000 + 0xF30;
    let owner = 0xE00000000;
    let description = policy::description(&f, at)?;
    assert_eq!(description.address, at);
    assert_eq!(description.owner, owner);
    assert_eq!(description.count, 2);
    assert_eq!(description.bytes.len(), 0x2970);
    assert_eq!(
        description.names,
        vec!["tshirt_ss_team_016_orl", "basketball_shorts_black"]
    );
    let reads = f.reads.borrow();
    assert_eq!(
        reads.len(),
        14,
        "6fullsnapshot chunks+2owner blocks+4arrays+2names"
    );
    assert_eq!(
        reads
            .iter()
            .filter(|(p, n)| *p == owner && *n == 40)
            .count(),
        2
    );
    for array in [owner + 0x1000, owner + 0x2000] {
        assert_eq!(
            reads
                .iter()
                .filter(|(p, n)| *p == array && *n == 16)
                .count(),
            2
        );
    }
    for offset in [0, 0xB8, 0xC8, 0xD0, 0x368] {
        assert!(
            !reads.contains(&(at + offset, if offset == 0xD0 { 4 } else { 8 })),
            "duplicated parsed snapshot field"
        );
    }
    println!(
        "description2piece bounded reads21->14; full0x2970 initial/end retained, owner/items rechecked at end"
    );
    Ok(())
}
#[cfg(test)]
struct DescriptionTransport {
    memory: RefCell<Fake>,
    fault: u8,
    owner_batches: Cell<usize>,
    array_batches: Cell<usize>,
    changed: Cell<bool>,
}
#[cfg(test)]
impl Memory for DescriptionTransport {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        let owner = 0xE00000000;
        let array = owner + 0x1000;
        let owned = owner + 0x2000;
        let owner_block = at == owner && len == 40;
        let array_block = (at == array || at == owned) && len == 16;
        if owner_block {
            self.owner_batches.set(self.owner_batches.get() + 1);
        }
        if array_block {
            self.array_batches.set(self.array_batches.get() + 1);
        }
        if (owner_block || array_block) && self.fault == 1 {
            anyhow::bail!("wider wardrobe span unavailable");
        }
        if array_block && self.fault == 3 {
            anyhow::bail!("later array transport failure");
        }
        let mut bytes = self.memory.borrow().read(at, len)?;
        if (owner_block || array_block) && self.fault == 2 {
            bytes.pop();
        }
        if owner_block && self.fault == 3 {
            bytes[..8].fill(0);
        }
        if at == owner + 0x3000 && len == 512 && !self.changed.replace(true) {
            let mut memory = self.memory.borrow_mut();
            match self.fault {
                4 => memory.q(array, owner + 0x3008),
                5 => memory.d(owner + 8, 1),
                6 => memory.q(owner, BASE + policy_variable_outfit() + 8),
                7 => memory.put(0xB00000000 + 0xF30 + 0x2000, &[1]),
                8 => memory.text(owner + 0x3000, "tshirt_ss_team_017_mia"),
                9 => memory.put(owner + 0x18, &[1]), // unrelated padding stays non-authoritative
                _ => {}
            }
        }
        Ok(bytes)
    }
}
#[cfg(test)]
fn policy_variable_outfit() -> u64 {
    crate::outfit_match_v2::VARIABLE_OUTFIT
}
#[cfg(test)]
fn description_transport(fault: u8) -> DescriptionTransport {
    DescriptionTransport {
        memory: RefCell::new(dense_description_fixture()),
        fault,
        owner_batches: Cell::new(0),
        array_batches: Cell::new(0),
        changed: Cell::new(false),
    }
}
#[cfg(test)]
#[test]
fn description_transport_fallback_and_completed_bad_owner_never_adopt_later_recovery() -> Result<()>
{
    use crate::outfit_match_v2 as policy;
    let at = 0xB00000000 + 0xF30;
    let expected = policy::description(&dense_description_fixture(), at)?;
    for fault in [1, 2] {
        let m = description_transport(fault);
        assert_eq!(policy::description(&m, at)?, expected);
        assert_eq!(m.owner_batches.get(), 2);
        assert_eq!(m.array_batches.get(), 4);
        m.memory.borrow_mut().q(0xE00000000, 0);
        assert!(
            policy::description(&m, at).is_err(),
            "bad scalar fallback owner accepted"
        );
    }
    let bad = description_transport(3);
    assert!(policy::description(&bad, at).is_err());
    assert_eq!(bad.owner_batches.get(), 1);
    assert_eq!(
        bad.array_batches.get(),
        0,
        "followed pointer after complete invalid owner"
    );
    Ok(())
}
#[cfg(test)]
#[test]
fn description_rejects_late_array_owner_count_class_opaque_and_string_changes() -> Result<()> {
    use crate::outfit_match_v2 as policy;
    let at = 0xB00000000 + 0xF30;
    for fault in [4, 5, 6, 7] {
        let m = description_transport(fault);
        assert!(
            policy::description(&m, at).is_err(),
            "accepted late wardrobe mutation{fault}"
        );
    }
    // Item string mutation after its first read is detected by the existing
    // independent second complete description capture in the match proof.
    let m = description_transport(8);
    assert!(policy::capture(&m, &[ROSTER]).is_err());
    assert!(
        policy::description(&description_transport(9), at).is_ok(),
        "opaque owner padding became a new stability restriction"
    );
    for offset in [0, 0xB8, 0xC8, 0xD0, 0x368, 0x7D] {
        let mut f = dense_description_fixture();
        let old = f.read(at + offset, 1)?[0];
        f.put(at + offset, &[old ^ 1]);
        assert!(
            policy::description(&f, at).is_err(),
            "bad initial parsed field{offset:x} accepted"
        );
    }
    Ok(())
}
