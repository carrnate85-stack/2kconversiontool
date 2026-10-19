//! Also executed without cfg(test) by the isolated native API harness. All
//! memory is sparse fake data; only fresh synthetic artifact journals touch disk.
use crate::{
    blacktop_arena::{Inspect, Region},
    blacktop_native::Memory,
    blacktop_outfits::{self as o, Composed, Page, Player, Selection},
    outfit_journal::{Prepared, Ready, Reservation, Submission},
    outfit_selection_session::Driver,
    outfit_selection_v2 as v2,
    outfit_selection_v2_session::{Service, Update},
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
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
struct FakeDriver {
    memory: Rc<RefCell<Fake>>,
    game: Identity,
    fail_publish: bool,
}
enum Held {
    Ready(Ready),
    Submitted(Submission),
}
struct Receipt {
    held: Held,
    after: Player,
}
impl Receipt {
    fn pin(&self) -> Result<crate::outfit_journal::Pin<'_>> {
        match &self.held {
            Held::Ready(v) => v.witness().pin(),
            Held::Submitted(v) => v.witness().pin(),
        }
    }
}
impl Driver for FakeDriver {
    type Receipt = Receipt;
    fn identity(&self) -> &Identity {
        &self.game
    }
    fn capture(&self, pages: &[Page]) -> Result<Selection> {
        v2::capture(&*self.memory.borrow(), &self.game, pages)
    }
    fn prepare(
        &self,
        r: Reservation,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Prepared> {
        let page = {
            let _pin = r.witness().pin()?;
            ensure!(r.intent().matches(p, c), "Intent differs");
            v2::revalidate(&*self.memory.borrow(), &self.game, p, pages)?;
            ensure!(
                v2::dirty(&*self.memory.borrow(), &p.key)? == 0,
                "Refreshing"
            );
            let allocation = 0x800000000 + pages.len() as u64 * 4096;
            let b = o::page_bytes(
                &self.game,
                &p.key,
                p.original_pointer,
                allocation,
                c.street_index,
                c.roster_shoes,
                &c.items,
            )?;
            self.memory.borrow_mut().put(allocation, &b);
            self.memory.borrow_mut().regions.insert(
                allocation,
                Region {
                    start: allocation,
                    allocation,
                    len: 4096,
                    kind: 0x20000,
                    protection: 2,
                    committed: true,
                },
            );
            let page = Page {
                allocation,
                owner: p.key.clone(),
                original_pointer: p.original_pointer,
                street_index: c.street_index,
                roster_shoes: c.roster_shoes,
                items: c.items.clone(),
                sha256: crate::loader_files::digest(&b),
            };
            page.verify(
                &*self.memory.borrow(),
                &self.game,
                &p.key,
                &r.intent().target(Some(&page))?,
            )?;
            page
        };
        r.prepared(page)
    }
    fn unchanged(&self, r: Ready, p: &Player, c: &Composed, pages: &[Page]) -> Result<Receipt> {
        {
            let _pin = r.witness().pin()?;
            ensure!(r.intent().matches(p, c), "Ready differs");
            v2::revalidate(&*self.memory.borrow(), &self.game, p, pages)?;
        }
        Ok(Receipt {
            held: Held::Ready(r),
            after: p.clone(),
        })
    }
    fn publish(&self, s: Submission, p: &Player, pages: &[Page]) -> Result<Receipt> {
        let after = {
            let _pin = s.witness().pin()?;
            ensure!(!self.fail_publish, "Injected publication fault");
            v2::revalidate(&*self.memory.borrow(), &self.game, p, pages)?;
            ensure!(
                s.intent().key == p.key && s.intent().before == p.source,
                "Source capability differs"
            );
            let target = s.intent().target(s.page())?;
            self.memory
                .borrow_mut()
                .put(p.key.description + 0xC8, &target.bytes());
            self.memory
                .borrow_mut()
                .put(p.key.controller + 0x30 + p.key.slot as u64 * 0x7B0, &[1]);
            let after = Player {
                source: target,
                items: s.intent().items.clone(),
                ..p.clone()
            };
            v2::revalidate(&*self.memory.borrow(), &self.game, &after, pages)?;
            after
        };
        Ok(Receipt {
            held: Held::Submitted(s),
            after,
        })
    }
    fn consumed(&self, r: &Receipt, pages: &[Page]) -> Result<bool> {
        let _pin = r.pin()?;
        v2::revalidate(&*self.memory.borrow(), &self.game, &r.after, pages)?;
        let n = v2::dirty(&*self.memory.borrow(), &r.after.key)?;
        ensure!(n <= 1, "Unknown dirty flag");
        Ok(n == 0)
    }
}
fn outfit(f: &Fake) -> crate::backend::Outfit {
    let p = v2::capture(f, &game(), &[]).unwrap().players.remove(0);
    let c = crate::outfits::catalog().unwrap();
    c.presets
        .iter()
        .filter(|r| r.pool == p.pool && r.separable && !r.manual_only)
        .find_map(|r| {
            let v = c.original_outfit(&r.id, "V2 selection QA").ok()?;
            (o::compose(&p, &v).ok()?.items != p.items).then_some(v)
        })
        .unwrap()
}
fn directory() -> std::path::PathBuf {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(format!("outfit-v2-fake-{}", crate::backend::unique_id()));
    std::fs::create_dir_all(root.join("Data")).unwrap();
    root
}
pub(crate) fn run() -> Result<()> {
    v2::verify_profile(&Fake::selection())?;
    for &(at, _) in v2::FIELD_ANCHORS {
        let mut f = Fake::selection();
        f.put(BASE + at, &[0]);
        ensure!(
            v2::verify_profile(&f).is_err(),
            "Changed native field anchor accepted"
        );
    }
    for &(table, offset, _) in v2::VIRTUAL_ANCHORS {
        let mut f = Fake::selection();
        f.q(BASE + table + offset, BASE + 0x1000);
        ensure!(
            v2::verify_profile(&f).is_err(),
            "Changed virtual table accepted"
        );
    }
    println!(
        "PASS all {} field bodies and {} virtual slots independently reject mutation",
        v2::FIELD_ANCHORS.len(),
        v2::VIRTUAL_ANCHORS.len()
    );
    let mut f = Fake::selection();
    f.d(BASE + 0x88, 0x6AA2013D);
    ensure!(v2::verify_profile(&f).is_err(), "Old build accepted");
    println!("PASS old build rejects before selection decoding");
    let f = Fake::selection();
    let capture = v2::capture(&f, &game(), &[])?;
    ensure!(
        capture.players.len() == 4 && capture.required == 4 && capture.selected == 4,
        "Selection count"
    );
    let p = capture.players[0].clone();
    v2::revalidate(&f, &game(), &p, &[])?;
    println!("PASS Version 2 native player bindings and CLOF wardrobe decoded");
    for (at, b) in [
        (p.key.description, vec![0; 8]),
        (p.key.description + 0xB8, vec![0; 8]),
        (p.key.controller + 0x18, vec![0]),
        (BASE + v2::LINEUP + 8, 2u32.to_le_bytes().to_vec()),
    ] {
        let mut bad = f.clone();
        bad.put(at, &b);
        ensure!(
            v2::revalidate(&bad, &game(), &p, &[]).is_err(),
            "Changed selection identity accepted"
        );
    }
    println!("PASS class/roster/readiness/count changes invalidate leases");
    let mut bad = f.clone();
    bad.q(BASE + v2::LINEUP + 0xC8, ROSTER);
    ensure!(
        v2::capture(&bad, &game(), &[]).is_err(),
        "Duplicate lineup accepted"
    );
    bad = f.clone();
    bad.tear = Some(BASE + v2::LINEUP + 0xC0);
    ensure!(
        v2::capture(&bad, &game(), &[]).is_err(),
        "Torn lineup accepted"
    );
    println!("PASS duplicate and torn lineups fail closed");
    bad = f.clone();
    bad.q(p.key.description + 0xC8, 0x800000080);
    bad.d(p.key.description + 0xD0, 3);
    ensure!(
        v2::capture(&bad, &game(), &[])?.unavailable.contains(&0),
        "Unowned private page adopted"
    );
    println!("PASS unowned private wardrobe is not adopted");
    let root = directory();
    let memory = Rc::new(RefCell::new(f.clone()));
    let draft = outfit(&f);
    let mut s = Service::create(
        &root,
        &crate::backend::unique_id(),
        FakeDriver {
            memory: memory.clone(),
            game: game(),
            fail_publish: false,
        },
    )?;
    let view = s.list(0)?;
    let binding = view.players[0].binding.clone();
    s.apply(&binding, &draft, 1)?;
    ensure!(
        s.busy() && s.poll(2)? == Update::Waiting,
        "Refresh not pending"
    );
    ensure!(
        memory.borrow().read(p.key.description + 0xD4, 0x24)?
            == f.read(p.key.description + 0xD4, 0x24)?,
        "Neighbor/uniform changed"
    );
    memory.borrow_mut().put(CONTROLLER + 0x30, &[0]);
    ensure!(
        matches!(
            s.poll(3)?,
            Update::SelectionUpdated {
                match_carry_over: false,
                ..
            }
        ),
        "Falsely claimed carry-over"
    );
    s.restore(&binding, 4)?;
    memory.borrow_mut().put(CONTROLLER + 0x30, &[0]);
    s.poll(5)?;
    ensure!(
        v2::source(&*memory.borrow(), &p.key)? == p.source,
        "Restore differs"
    );
    s.leave_selection()?;
    s.finish_ended(|| Ok(()))?;
    std::fs::remove_dir_all(&root)?;
    println!(
        "PASS real durable journal -> sealed page -> selection refresh -> restore lifecycle; neighbors unchanged"
    );
    let root = directory();
    let mut s = Service::create(
        &root,
        &crate::backend::unique_id(),
        FakeDriver {
            memory: Rc::new(RefCell::new(f.clone())),
            game: game(),
            fail_publish: true,
        },
    )?;
    let binding = s.list(0)?.players[0].binding.clone();
    ensure!(
        s.apply(&binding, &draft, 1).is_err(),
        "Injected failure accepted"
    );
    ensure!(
        s.held().is_some() && s.list(2).is_err(),
        "Uncertain publication retried"
    );
    s.finish_ended(|| Ok(()))?;
    std::fs::remove_dir_all(&root)?;
    println!("PASS uncertain publication is held with no automatic retry");
    let caps = crate::outfit_selection_v2_session::capabilities();
    ensure!(
        caps.selection_fitting
            && !caps.match_carry_over
            && !caps.automatic_randomization
            && !caps.gameplay_verified,
        "Capability overclaim"
    );
    println!("PASS selection-only capability; no native worker or gameplay claim");
    Ok(())
}
/// Disk image only. Does not enumerate, open or read a game process.
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
    v2::verify_profile(&Disk {
        file: RefCell::new(file),
        sections,
    })?;
    println!(
        "PASS installed Version 2 full SHA plus independent selection/descriptor/refresh/class anchors"
    );
    Ok(())
}
#[cfg(test)]
#[test]
fn version_two_selection_contract_and_durable_lifecycle() {
    run().unwrap();
}

#[path = "../src/outfit_carry_v2_tests.rs"]
mod carry;
pub(crate) fn carry_run() -> Result<()> {
    carry::run()
}
