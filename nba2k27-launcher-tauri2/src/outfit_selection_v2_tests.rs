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
    reads: Cell<usize>,
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
        Self::selection_group(0)
    }
    /// Lineup whose stock wardrobe uses CLOF group `group` (1054 = last, the
    /// worst case for the catalog search).
    fn selection_group(group: usize) -> Self {
        let mut f = Self {
            bytes: BTreeMap::new(),
            ranges: vec![],
            regions: BTreeMap::new(),
            tear: None,
            seen: Cell::new(0),
            reads: Cell::new(0),
        };
        let mut h = vec![0; 4096];
        h[..2].copy_from_slice(b"MZ");
        h[60..64].copy_from_slice(&0x80u32.to_le_bytes());
        h[0x80..0x84].copy_from_slice(b"PE\0\0");
        h[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        h[0x88..0x8C].copy_from_slice(&0x6AD2E48Cu32.to_le_bytes());
        h[0x98..0x9A].copy_from_slice(&0x20Bu16.to_le_bytes());
        h[0xD0..0xD4].copy_from_slice(&0x36532000u32.to_le_bytes());
        f.put(BASE, &h);
        let getters: serde_json::Value =
            serde_json::from_str(include_str!("../assets/rules-v2-getters.json")).unwrap();
        for g in getters["fields"].as_array().unwrap() {
            let mut b = g["hex"]
                .as_str()
                .unwrap()
                .as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            let old = g["rva"].as_u64().unwrap();
            let at = crate::rules_v2::updated_getter(g["id"].as_str().unwrap(), old);
            let disp = (crate::rules_v2::OWNER as i64 - at as i64 - 7) as i32;
            b[3..7].copy_from_slice(&disp.to_le_bytes());
            f.put(BASE + at, &b);
        }
        for (r, p, t) in [
            (0x3457F00, &[0x48, 0x8D, 0x05][..], 0x791E158),
            (0x3457F07, &[0x48, 0x89, 0x05], v2::ROOT),
            (0x25AA480, &[0x48, 0x8B, 0x05], 0xAB5CE20),
            (0x308C6F9, &[0x48, 0x8D, 0x05], 0xAD07408),
            (0x308C700, &[0x48, 0x63, 0x0D], 0xAD07BCC),
            (0x300D7B3, &[0x48, 0x63, 0x05], 0xAD07BD0),
            (0x32F7B1A, &[0x89, 0x05], v2::LINEUP + 4),
            (0x32F7B20, &[0x89, 0x05], v2::LINEUP + 8),
        ] {
            f.rip(r, p, t);
        }
        f.put(BASE + 0x32FBB36, &[0x8B, 0x81, 0xA8, 0x0E, 0, 0]);
        f.put(BASE + 0x24F7F75, &[0x80, 0x3D, 0x14, 0x58, 0x66, 8, 0]);
        f.put(
            BASE + 0x24F5500,
            &[
                0x48, 0x8B, 0x05, 0x19, 0x79, 0x66, 8, 0x8B, 0x80, 4, 0x0A, 0, 0, 0xC3,
            ],
        );
        f.q(BASE + 0x791E158, BASE + 0x23120);
        f.q(BASE + 0x791E160, BASE + 0x346B4D0);
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
        let g = &o::groups()[group];
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
        self.reads.set(self.reads.get() + 1);
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
    // WPF parity (BlacktopTool.cs): selection = Blacktop root + stage 3, no
    // game-mode test. 0.50 live reported game mode 0 at selection; 6 also seen.
    for mode in [0u32, 6] {
        bad = f.clone();
        bad.d(0x300000000 + 0xA04, mode);
        let view = v2::capture(&bad, &game(), &[])?;
        ensure!(
            view.players.len() == 4,
            "Game mode {mode} at selection refused"
        );
    }
    // A non-selection screen is still refused and named.
    bad = f.clone();
    bad.d(BASE + v2::ROOT + 0xEA8, 1);
    let refused = format!("{:#}", v2::capture(&bad, &game(), &[]).unwrap_err());
    ensure!(
        refused.contains("Open Blacktop player selection") && refused.contains("stage 1"),
        "Refusal does not name the game state: {refused}"
    );
    println!(
        "PASS selection accepted with game mode 0 or 6; other screens refused with their observed state"
    );
    bad = f.clone();
    bad.q(p.key.description + 0xC8, 0x800000080);
    bad.d(p.key.description + 0xD0, 3);
    let refused = v2::capture(&bad, &game(), &[])?;
    ensure!(
        refused.unavailable.contains(&0),
        "Unowned private page adopted"
    );
    // The overlay names why the slot was refused instead of skipping it.
    ensure!(
        refused
            .reasons
            .iter()
            .any(|(slot, why)| *slot == 0 && why.contains("Unknown private outfit page")),
        "Refused slot reason missing: {:?}",
        refused.reasons
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
    // Random all on a complete lineup (required == selected == 4): every
    // slot is listed and each apply confirms its own refresh in turn.
    let root = directory();
    let memory = Rc::new(RefCell::new(f.clone()));
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
    ensure!(
        view.required == 4
            && view.selected == 4
            && view.players.len() == 4
            && view.unavailable.is_empty()
            && view.unavailable_reasons.is_empty(),
        "Full lineup not listed: {} of {}",
        view.players.len(),
        view.required
    );
    let mut now = 10;
    for player in &view.players {
        s.apply(&player.binding, &draft, now)?;
        ensure!(s.poll(now + 1)? == Update::Waiting, "Refresh not pending");
        memory
            .borrow_mut()
            .put(CONTROLLER + 0x30 + player.slot as u64 * 0x7B0, &[0]);
        ensure!(
            matches!(s.poll(now + 2)?, Update::SelectionUpdated { slot, .. } if slot == player.slot),
            "Slot {} did not confirm",
            player.slot
        );
        now += 10;
    }
    for slot in 0..4u64 {
        let desc = u64::from_le_bytes(
            f.read(CONTROLLER + 0x28 + slot * 0x7B0, 8)?
                .try_into()
                .unwrap(),
        );
        ensure!(
            memory.borrow().read(desc + 0xC8, 8)? != f.read(desc + 0xC8, 8)?,
            "Slot {slot} clothing did not change"
        );
    }
    s.leave_selection()?;
    s.finish_ended(|| Ok(()))?;
    std::fs::remove_dir_all(&root)?;
    println!("PASS full 4-of-4 lineup: every slot listed and applied in sequence");
    // Random all batch: one validated pass, every slot written back to back,
    // then ONE combined wait that confirms each slot by its own readback.
    let root = directory();
    let memory = Rc::new(RefCell::new(f.clone()));
    let mut s = crate::outfit_selection_session::Session::create(
        &root,
        &crate::backend::unique_id(),
        FakeDriver {
            memory: memory.clone(),
            game: game(),
            fail_publish: false,
        },
    )?;
    let view = s.list(0)?;
    let items = view
        .players
        .iter()
        .map(|p| (p.token.clone(), draft.clone()))
        .collect::<Vec<_>>();
    s.apply_many(&items, 10)?;
    ensure!(s.busy(), "Batch not pending");
    ensure!(
        s.apply(&items[0].0, &draft, 11).is_err(),
        "A second write was accepted while the batch refresh is pending"
    );
    // The game clears two slots first: still one combined wait.
    for slot in [0u64, 2] {
        memory
            .borrow_mut()
            .put(CONTROLLER + 0x30 + slot * 0x7B0, &[0]);
    }
    ensure!(
        s.poll(20)? == crate::outfit_selection_session::Poll::Waiting,
        "Batch confirmed before every slot refreshed"
    );
    for slot in [1u64, 3] {
        memory
            .borrow_mut()
            .put(CONTROLLER + 0x30 + slot * 0x7B0, &[0]);
    }
    match s.poll(30)? {
        crate::outfit_selection_session::Poll::ConfirmedMany { players, timing } => {
            let mut slots = players.iter().map(|p| p.slot).collect::<Vec<_>>();
            slots.sort();
            ensure!(slots == [0, 1, 2, 3], "Batch slots {slots:?}");
            ensure!(timing.refresh_ms == 20, "Batch timing {timing:?}");
        }
        other => anyhow::bail!("Batch not confirmed: {other:?}"),
    }
    ensure!(!s.busy(), "Batch still pending after confirmation");
    for slot in 0..4u64 {
        let desc = u64::from_le_bytes(
            f.read(CONTROLLER + 0x28 + slot * 0x7B0, 8)?
                .try_into()
                .unwrap(),
        );
        ensure!(
            memory.borrow().read(desc + 0xC8, 8)? != f.read(desc + 0xC8, 8)?,
            "Batch slot {slot} clothing did not change"
        );
    }
    s.leave_blacktop()?;
    s.finish(|| Ok(()))?;
    std::fs::remove_dir_all(&root)?;
    // An unconfirmed slot holds the session (no retry), exactly as before.
    let root = directory();
    let memory = Rc::new(RefCell::new(f.clone()));
    let mut s = crate::outfit_selection_session::Session::create(
        &root,
        &crate::backend::unique_id(),
        FakeDriver {
            memory: memory.clone(),
            game: game(),
            fail_publish: false,
        },
    )?;
    let view = s.list(0)?;
    let items = view
        .players
        .iter()
        .map(|p| (p.token.clone(), draft.clone()))
        .collect::<Vec<_>>();
    s.apply_many(&items, 10)?;
    ensure!(
        s.poll(10 + 3000 * 4 + 1).is_err() && s.held().is_some(),
        "Unconfirmed batch did not hold the session"
    );
    std::fs::remove_dir_all(&root).ok();
    println!(
        "PASS Random all batch: one write pass, one combined wait, per-slot readback, unconfirmed holds"
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
            == "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B",
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

#[test]
fn random_all_capture_reuses_the_proven_layout_instead_of_searching_the_catalog() {
    // Random all speed (0.49 live: "incredibly slow"). Every apply runs several
    // full lineup captures; each searched up to 1,791 CLOF records per player.
    let f = Fake::selection_group(o::groups().len() - 1);
    v2::clear_layout_hints();
    f.reads.set(0);
    let start = std::time::Instant::now();
    let cold = v2::capture(&f, &game(), &[]).unwrap();
    let (cold_reads, cold_time) = (f.reads.get(), start.elapsed());
    f.reads.set(0);
    let start = std::time::Instant::now();
    let warm = v2::capture(&f, &game(), &[]).unwrap();
    let (warm_reads, warm_time) = (f.reads.get(), start.elapsed());
    assert_eq!(cold.players.len(), 4);
    assert_eq!(
        cold.players
            .iter()
            .map(|p| (&p.key, &p.source, &p.items))
            .collect::<Vec<_>>(),
        warm.players
            .iter()
            .map(|p| (&p.key, &p.source, &p.items))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "lineup capture, 4 players, last CLOF group: before {cold_reads} reads / {cold_time:?}; after {warm_reads} reads / {warm_time:?}"
    );
    // Measured 2,198 -> 408 reads; the remainder is fixed profile/context/lineup cost.
    // Cold discovery now also uses the bounded page prefilter. Warm lookup
    // still must not add reads; direct exhaustive-vs-prefilter coverage below
    // measures the newly optimized cold path independently of global hints.
    assert!(warm_reads <= cold_reads, "{warm_reads} vs {cold_reads}");
    // A wrong hint is never trusted: a different stock layout still resolves.
    let other = Fake::selection();
    assert_eq!(v2::capture(&other, &game(), &[]).unwrap().players.len(), 4);
}

#[test]
fn cold_layout_prefilter_preserves_full_proof_and_falls_back_when_pages_fail() {
    struct NoPages<'a>(&'a Fake);
    impl Memory for NoPages<'_> {
        fn base(&self) -> u64 {
            self.0.base()
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            ensure!(len != 4096, "Synthetic prefetch refusal");
            self.0.read(at, len)
        }
    }
    let index = o::groups().len() - 1;
    let f = Fake::selection_group(index);
    let original = STOCK + o::groups()[index].records[0].array as u64;
    f.reads.set(0);
    let exhaustive = v2::exhaustive_layout(&f, original).unwrap();
    let exhaustive_reads = f.reads.get();
    f.reads.set(0);
    let filtered = v2::prefilter_layout(&f, original).unwrap();
    let filtered_reads = f.reads.get();
    assert_eq!(filtered, exhaustive);
    assert!(
        filtered_reads * 5 < exhaustive_reads,
        "{filtered_reads} vs {exhaustive_reads}"
    );
    eprintln!(
        "cold CLOF discovery: exhaustive {exhaustive_reads} reads vs prefilter {filtered_reads} reads"
    );
    assert!(
        v2::prefilter_layout(&NoPages(&f), original)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        v2::exhaustive_layout(&NoPages(&f), original).unwrap(),
        exhaustive
    );
    // A count matching the prefilter never substitutes for any other proof.
    let mut forged = f.clone();
    let group = &o::groups()[index];
    forged.q(
        STOCK + group.records[0].record as u64,
        group.records[0].marker.parse::<u64>().unwrap() ^ 1,
    );
    assert!(v2::prefilter_layout(&forged, original).unwrap().is_none());
    assert!(v2::exhaustive_layout(&forged, original).unwrap().is_none());
}

// Synthetic per-observation transport faults: no cached bytes or native calls.
#[derive(Clone)]
struct BoundedProof<'a> {
    inner: &'a Fake,
    key: &'a o::PlayerKey,
    fallback: u8, // 1 = inaccessible wider span, 2 = partial wider read
    tear: Option<(u64, usize)>,
    seen: Cell<usize>,
    calls: Cell<usize>,
}
impl Memory for BoundedProof<'_> {
    fn base(&self) -> u64 {
        self.inner.base()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        let batched = (at == self.key.controller + 0x20 + self.key.slot as u64 * 0x7b0
            && len == 16)
            || (at == self.key.description + 0xB8 && len == 64)
            || (at == BASE + v2::LINEUP + 0xC0 && len > 8 && len <= 80);
        if batched && self.fallback == 1 {
            anyhow::bail!("Synthetic wider region boundary");
        }
        let mut bytes = self.inner.read(at, len)?;
        if batched && self.fallback == 2 {
            bytes.pop();
        }
        if self.tear == Some((at, len)) {
            let seen = self.seen.get();
            self.seen.set(seen + 1);
            if seen > 0 {
                bytes[0] ^= 1;
            }
        }
        Ok(bytes)
    }
}
impl Inspect for BoundedProof<'_> {
    fn region(&self, at: u64) -> Result<Region> {
        self.inner.region(at)
    }
}
#[test]
fn bounded_source_and_lineup_reads_preserve_scalar_values_and_reduce_read_count() {
    let f = Fake::selection();
    let player = v2::capture(&f, &game(), &[]).unwrap().players.remove(0);
    let proof = BoundedProof {
        inner: &f,
        key: &player.key,
        fallback: 0,
        tear: None,
        seen: Cell::new(0),
        calls: Cell::new(0),
    };
    let source = v2::source(&proof, &player.key).unwrap();
    assert_eq!(source, player.source);
    assert_eq!(
        proof.calls.get(),
        4,
        "one16B controller, two vtables, one64B description"
    );
    let rows = v2::lineup_rows(&proof, 4).unwrap();
    assert_eq!(proof.calls.get(), 5, "four lineup pointers use one32B read");
    for mode in [1, 2] {
        let fallback = BoundedProof {
            fallback: mode,
            calls: Cell::new(0),
            ..proof.clone()
        };
        assert_eq!(v2::source(&fallback, &player.key).unwrap(), source);
        assert_eq!(
            fallback.calls.get(),
            10,
            "two rejected blocks plus original eight scalar reads"
        );
        assert_eq!(v2::lineup_rows(&fallback, 4).unwrap(), rows);
        assert_eq!(
            fallback.calls.get(),
            15,
            "one rejected lineup span plus four scalar pointers"
        );
        v2::revalidate(&fallback, &game(), &player, &[]).unwrap();
    }
    assert!(v2::lineup_rows(&proof, 11).is_err());
    let reads = proof.calls.get();
    assert!(v2::lineup_rows(&proof, 0).unwrap().is_empty());
    assert_eq!(
        proof.calls.get(),
        reads,
        "empty lineup must not issue a zero-length read"
    );
    eprintln!(
        "V2 source: original eight scalar reads -> four bounded reads; four-player lineup: four scalar reads -> one32B read (each independent proof pass)"
    );
}
#[test]
fn bounded_observations_never_reuse_the_initial_source_or_lineup_at_final_proof() {
    let f = Fake::selection();
    let player = v2::capture(&f, &game(), &[]).unwrap().players.remove(0);
    for tear in [
        (player.key.description + 0xB8, 64),
        (player.key.controller + 0x20, 16),
        (BASE + v2::LINEUP + 0xC0, 32),
    ] {
        let proof = BoundedProof {
            inner: &f,
            key: &player.key,
            fallback: 0,
            tear: Some(tear),
            seen: Cell::new(0),
            calls: Cell::new(0),
        };
        assert!(
            v2::revalidate(&proof, &game(), &player, &[]).is_err(),
            "torn independent proof {tear:?}"
        );
        assert!(
            proof.seen.get() >= 2,
            "the final proof must perform a fresh observation"
        );
    }
}
#[test]
fn bounded_source_refuses_foreign_controller_vtable_roster_and_outfit_values() {
    let original = Fake::selection();
    let player = v2::capture(&original, &game(), &[])
        .unwrap()
        .players
        .remove(0);
    for at in [
        player.key.controller + 0x20,
        player.key.controller + 0x28,
        player.key.model,
        player.key.description,
        player.key.description + 0xB8,
        player.key.description + 0xC8,
        player.key.description + 0xD0,
        player.key.description + 0xF0,
    ] {
        let mut f = original.clone();
        let old = f.read(at, 1).unwrap()[0];
        f.put(at, &[old ^ 1]);
        assert!(
            v2::revalidate(&f, &game(), &player, &[]).is_err(),
            "changed identity/source {at:x}"
        );
    }
}

#[test]
fn selection_profile_groups_fresh_anchors_without_caching_live_code_or_slots() {
    let mut f = Fake::selection();
    for &(at, _) in v2::FIELD_ANCHORS {
        f.ranges.push((BASE + at / 4096 * 4096, 4096));
    }
    for &(at, _, _) in v2::VIRTUAL_ANCHORS {
        f.ranges.push((BASE + at / 4096 * 4096, 4096));
    }
    f.ranges.push((BASE + 0x32F7000, 4096));
    f.reads.set(0);
    crate::rules_v2::verify_profile(&f).unwrap();
    let rules_reads = f.reads.get();
    f.reads.set(0);
    v2::verify_profile(&f).unwrap();
    assert!(
        f.reads.get() <= rules_reads + 11,
        "{} vs {rules_reads}",
        f.reads.get()
    );
    // A later successful call must read live virtual slots and code again.
    f.q(BASE + v2::DESCRIPTION + 0xC8, BASE + 0x17CC51);
    assert!(
        v2::verify_profile(&f)
            .unwrap_err()
            .to_string()
            .contains("class virtual method")
    );
    f.q(BASE + v2::DESCRIPTION + 0xC8, BASE + 0x17CC50);
    f.put(BASE + v2::FIELD_ANCHORS[0].0, &[0x81]);
    assert!(
        v2::verify_profile(&f)
            .unwrap_err()
            .to_string()
            .contains("field/refresh")
    );
}

struct AnchorTransport {
    calls: RefCell<Vec<(u64, usize)>>,
    partial: bool,
    mismatch: bool,
}
impl Memory for AnchorTransport {
    fn base(&self) -> u64 {
        BASE
    }
    fn checkpoint(&self) -> Result<()> {
        Ok(())
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.calls.borrow_mut().push((at, len));
        if at == BASE + 0x2000 && len == 18 {
            if self.partial {
                return Ok(vec![3]);
            }
            anyhow::bail!("synthetic transport failure");
        }
        let mut bytes = vec![0; len];
        for (start, expected) in [
            (0x1000, [1, 2]),
            (0x1010, [3, 4]),
            (0x2000, [5, 6]),
            (0x2010, [7, 8]),
        ] {
            for (i, value) in expected.into_iter().enumerate() {
                if (at..at + len as u64).contains(&(BASE + start + i as u64)) {
                    bytes[(BASE + start + i as u64 - at) as usize] = value;
                }
            }
        }
        if self.mismatch && at == BASE + 0x1000 && len == 18 {
            bytes[0] ^= 1;
        }
        Ok(bytes)
    }
}
fn anchor_transport_fixture() -> Vec<(u64, Vec<u8>, &'static str)> {
    vec![
        (0x1000, vec![1, 2], "first"),
        (0x1010, vec![3, 4], "second"),
        (0x2000, vec![5, 6], "third"),
        (0x2010, vec![7, 8], "fourth"),
    ]
}
#[test]
fn selection_anchor_transport_failure_or_partial_rereads_complete_scalar_proof() {
    for partial in [false, true] {
        let m = AnchorTransport {
            calls: RefCell::new(vec![]),
            partial,
            mismatch: false,
        };
        v2::verify_anchors(&m, &anchor_transport_fixture()).unwrap();
        assert_eq!(
            &*m.calls.borrow(),
            &[
                (BASE + 0x1000, 18),
                (BASE + 0x2000, 18),
                (BASE + 0x1000, 2),
                (BASE + 0x1010, 2),
                (BASE + 0x2000, 2),
                (BASE + 0x2010, 2)
            ]
        );
    }
}
#[test]
fn selection_anchor_observed_mismatch_cannot_be_erased_by_later_transport_fallback() {
    let m = AnchorTransport {
        calls: RefCell::new(vec![]),
        partial: false,
        mismatch: true,
    };
    assert_eq!(
        v2::verify_anchors(&m, &anchor_transport_fixture())
            .unwrap_err()
            .to_string(),
        "first"
    );
    assert_eq!(&*m.calls.borrow(), &[(BASE + 0x1000, 18)]);
}
