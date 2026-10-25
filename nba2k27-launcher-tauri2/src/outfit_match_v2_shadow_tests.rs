//! Synthetic memory only; no game handles or calls.
use super::*;
// Also executed without cfg(test) by the isolated native API harness. All
// memory is sparse fake data; only fresh synthetic artifact journals touch disk.
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

#[test]
fn pre_stage_ready_is_facts_only_and_real_capture_remains_strict() -> Result<()> {
    let mut f = seed();
    f.d(BASE + v2::ROOT + 0xEA8, 0);
    assert!(crate::outfit_match_v2::capture(&f, &[ROSTER]).is_err());
    deep(&f, &[ROSTER])?;
    assert!(wpf_like(&f, &[ROSTER]).is_ok());
    Ok(())
}
#[test]
fn fresh_context_and_original_binding_races_never_produce_facts() {
    let mut f = seed();
    f.tear = Some(0xA00000000 + 0x500);
    assert!(deep(&f, &[ROSTER]).is_err());
    let mut f = seed();
    f.tear = Some(0xA00000000 + 0x500);
    assert!(generation(&f, &[ROSTER]).is_err());
    let mut f = seed();
    f.q(0xA00000000 + 0x518, 0xC00000000);
    assert!(generation(&f, &[ROSTER]).is_err());
}
#[test]
fn diagnostic_cannot_relax_online_profile_queue_or_actor_guards() {
    for mutation in 0..5 {
        let mut f = seed();
        match mutation {
            0 => {
                let owner = q(&f, BASE + crate::rules_v2::OWNER).unwrap();
                f.put(owner, &[1]);
            }
            1 => f.q(0xB00000000, BASE + PLAYER + 8),
            2 => f.put(BASE + super::profile().methods[0].rva, &[0]),
            3 => f.q(BASE + QUEUE_SENTINEL + 8, BASE + QUEUE_SENTINEL + 0x100),
            _ => f.q(0xC00000000 + 0x118, 1),
        }
        assert!(deep(&f, &[ROSTER]).is_err(), "mutation {mutation}");
    }
}
#[test]
fn cpu_byte_wall_and_cancel_are_rejections_not_partial_readiness() -> Result<()> {
    let f = seed();
    let m = Budget::new(&f, BURST_US)?;
    assert!(m.read(BASE, 4097).is_err());
    m.bytes.set(PROBE_BYTES - 3);
    assert!(m.read(BASE, 4).is_err());
    let m = Budget::new(&f, BURST_US)?;
    std::thread::sleep(Duration::from_millis(41));
    assert!(m.guard().is_err());
    let mut bucket = CpuBucket::start(100);
    assert_eq!(bucket.allowance(100)?, 25_000);
    bucket.debit(25_000);
    assert_eq!(bucket.allowance(200)?, 2500);
    assert!(bucket.allowance(199).is_err());
    assert_eq!(
        RIGHTS.0,
        PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_SYNCHRONIZE.0
    );
    assert_eq!(
        RIGHTS.0
            & (PROCESS_VM_WRITE.0
                | PROCESS_VM_OPERATION.0
                | PROCESS_CREATE_THREAD.0
                | PROCESS_TERMINATE.0),
        0
    );
    Ok(())
}
#[test]
fn private_generation_reuse_and_fact_serialization_do_not_leak_identity() -> Result<()> {
    let f = seed();
    let before = generation(&f, &[ROSTER])?;
    assert_eq!(generation(&f, &[ROSTER])?, before);
    let facts = Facts {
        tick_ms: 1,
        elapsed_ms: 1,
        window: 1,
        mode: 0,
        stage: 0,
        game_mode: 6,
        strict_boundary: false,
        queued: 1,
        wpf_like: true,
        deep: true,
        stable_three: false,
        generation: "reused-generation",
        generation_ordinal: 1,
        reason: "diagnostic-only-no-admission",
        probe_kind: "deep",
        reads: 1,
        bytes: 8,
    };
    let value = serde_json::to_string(&facts)?;
    for secret in [
        "roster",
        "actor",
        "pointer",
        "items",
        "name",
        "description",
        "choiceId",
        "signature",
    ] {
        assert!(
            !value
                .to_ascii_lowercase()
                .contains(&secret.to_ascii_lowercase()),
            "private field {secret}"
        );
    }
    assert!(value.len() < 1024);
    Ok(())
}
fn first_unseeded_read(f: &Fake) -> Option<(u64, usize, u64)> {
    for &(at, len) in f.reads.borrow().iter() {
        for offset in 0..len {
            let byte = at + offset as u64;
            if !f.bytes.contains_key(&byte)
                && !f
                    .ranges
                    .iter()
                    .any(|(start, size)| byte >= *start && byte < start + *size as u64)
            {
                return Some((at, len, byte));
            }
        }
    }
    None
}
#[test]
fn description_reads_are_chunked_and_cost_is_bounded() -> Result<()> {
    let mut f = seed();
    // The sparse legacy fixture deliberately leaves padding unmapped to exercise
    // scalar fallback. This byte-count fixture models readable image padding;
    // real diagnostic transport errors remain poisoned, without scalar retry.
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../assets/rules-v2-getters.json"))?;
    let mut groups = BTreeMap::<u64, (u64, u64)>::new();
    for getter in catalog["fields"].as_array().unwrap() {
        let at = crate::rules_v2::updated_getter(
            getter["id"].as_str().unwrap(),
            getter["rva"].as_u64().unwrap(),
        );
        let end = at + getter["hex"].as_str().unwrap().len() as u64 / 2;
        groups
            .entry(at / 4096)
            .and_modify(|range| {
                range.0 = range.0.min(at);
                range.1 = range.1.max(end);
            })
            .or_insert((at, end));
    }
    let mut healthy_spans = groups
        .values()
        .map(|(start, end)| (BASE + start, (end - start) as usize))
        .collect::<Vec<_>>();
    // Exact constructor-owned metadata and the two known two-item allocations.
    // Preserve every initialized field; only otherwise-unspecified padding is 0.
    let owner = 0xE00000000u64;
    healthy_spans.extend([
        (owner, 0x28),
        (owner + 0x1000, 2 * 8),
        (owner + 0x2000, 2 * 8),
    ]);
    for (start, len) in healthy_spans {
        for offset in 0..len {
            f.bytes.entry(start + offset as u64).or_insert(0);
        }
    }
    // Exercise every healthy read once without the poison wrapper so any other
    // fallback-only sparse fixture span is reported before budget accounting.
    deep(&f, &[ROSTER])?;
    ensure!(
        first_unseeded_read(&f).is_none(),
        "unseeded healthy batched read: {:?}",
        first_unseeded_read(&f)
    );
    f.reads.borrow_mut().clear();
    let mut budget = Budget::new(&f, u64::MAX)?;
    // Read/byte accounting is deterministic; timing rejection has its own tests.
    budget.wall_limit = Duration::MAX;
    if let Err(error) = deep(&budget, &[ROSTER]) {
        return Err(error).context(format!(
            "synthetic cost: {} bytes / {} reads; last {:?}; first unseeded {:?}",
            budget.bytes.get(),
            budget.reads.get(),
            f.reads.borrow().last(),
            first_unseeded_read(&f)
        ));
    }
    assert!(budget.bytes.get() < PROBE_BYTES);
    assert!(f.reads.borrow().iter().all(|(_, len)| *len <= 4096));
    println!(
        "shadow single-player deep: {} bytes / {} reads",
        budget.bytes.get(),
        budget.reads.get()
    );
    Ok(())
}

#[test]
fn overspending_cpu_debt_must_be_repaid_before_any_fresh_full_probe() -> Result<()> {
    let mut b = CpuBucket::start(0);
    b.debit(75_000);
    for tick in [
        250, 500, 750, 1000, 1250, 1500, 1750, 2000, 2250, 2500, 2750,
    ] {
        assert!(b.allowance(tick)? < BURST_US, "overrun debt lost at {tick}");
    }
    assert_eq!(b.allowance(3000)?, BURST_US);
    Ok(())
}
#[test]
fn swallowed_transport_or_window_limit_error_poison_the_whole_observation() -> Result<()> {
    let f = seed();
    let mut m = Budget::new(&f, 1_000_000)?;
    m.byte_limit = 3;
    assert!(m.read(BASE, 4).ok().is_none());
    assert_eq!(m.bytes.get(), 0);
    assert!(m.guard().is_err());
    assert!(m.read(BASE, 1).is_err());
    let m = Budget::new(&f, 1_000_000)?;
    assert!(m.read(0x123456780, 8).ok().is_none());
    assert!(m.guard().is_err());
    Ok(())
}

#[test]
fn cancellation_checkpoint_failure_poison_cannot_be_swallowed() -> Result<()> {
    struct Cancelled<'a>(&'a Fake);
    impl Memory for Cancelled<'_> {
        fn base(&self) -> u64 {
            self.0.base()
        }
        fn checkpoint(&self) -> Result<()> {
            anyhow::bail!("synthetic cancelled")
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            self.0.read(at, len)
        }
    }
    let f = seed();
    let cancelled = Cancelled(&f);
    let m = Budget::new(&cancelled, BURST_US)?;
    assert!(m.read(BASE, 8).ok().is_none());
    assert!(m.poisoned.get());
    assert!(m.guard().is_err());
    assert_eq!(m.bytes.get(), 0);
    Ok(())
}
#[test]
fn logger_failure_and_event_cap_have_no_effect_api() -> Result<()> {
    let facts = Facts {
        tick_ms: 1,
        elapsed_ms: 1,
        window: 1,
        mode: 0,
        stage: 0,
        game_mode: 6,
        strict_boundary: false,
        queued: 1,
        wpf_like: false,
        deep: false,
        stable_three: false,
        generation: "baseline-unknown",
        generation_ordinal: 0,
        reason: "observation-unknown",
        probe_kind: "unknown",
        reads: 0,
        bytes: 0,
    };
    let mut bytes = Vec::new();
    let mut events = 0;
    for _ in 0..MAX_EVENTS {
        record_into(&mut bytes, &mut events, &facts)?;
    }
    assert!(record_into(&mut bytes, &mut events, &facts).is_err());
    assert!(bytes.len() < 32 * 1024);
    assert_eq!(events, 32);
    struct Failed;
    impl Write for Failed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("synthetic log failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut events = 0;
    assert!(record_into(&mut Failed, &mut events, &facts).is_err());
    assert_eq!(events, 0);
    Ok(())
}
#[test]
fn generation_baseline_missing_reused_and_replaced_remain_informational() -> Result<()> {
    let f = seed();
    let baseline = generation(&f, &[ROSTER])?;
    assert_eq!(baseline, generation(&f, &[ROSTER])?);
    let mut replacement = f.clone();
    let primary = 0xC00000000;
    replacement.q(primary + 0x110, 0xB00010000);
    replacement.q(0xB00010000, BASE + PLAYER);
    assert_ne!(baseline, generation(&replacement, &[ROSTER])?);
    let mut torn = f.clone();
    torn.tear = Some(0xA00000000 + 0x500);
    assert!(generation(&torn, &[ROSTER]).is_err());
    // No result type here is a permit, description, player, plan or writer.
    Ok(())
}

#[test]
fn hold_drop_and_empty_session_only_cancel_the_private_monitor() {
    let real_stop = Arc::new(AtomicBool::new(false));
    let private = Arc::new(AtomicBool::new(false));
    let input = Arc::new(Mutex::new(Input::default()));
    let monitor = Monitor {
        input: input.clone(),
        cancel: private.clone(),
    };
    let choices = vec![(ROSTER, "a".repeat(48))];
    monitor.offer(choices.clone());
    assert_eq!(input.lock().unwrap().frozen.0, choices);
    monitor.offer(Vec::new());
    assert!(input.lock().unwrap().frozen.0.is_empty());
    monitor.offer(choices.clone());
    monitor.disable();
    assert!(private.load(Ordering::Acquire));
    assert!(!real_stop.load(Ordering::Acquire));
    monitor.offer(choices.clone());
    assert!(input.lock().unwrap().frozen.0.is_empty());
    drop(monitor);
    assert!(private.load(Ordering::Acquire));
    assert!(!real_stop.load(Ordering::Acquire));
    assert_eq!(choices, vec![(ROSTER, "a".repeat(48))]);
    let private = Arc::new(AtomicBool::new(false));
    let monitor = Monitor {
        input: Arc::new(Mutex::new(Input::default())),
        cancel: private.clone(),
    };
    drop(monitor);
    assert!(private.load(Ordering::Acquire));
    assert!(!real_stop.load(Ordering::Acquire));
}

#[test]
fn wrapped_loaded_pointer_rejects_without_panicking_or_following_wrapped_address() {
    for pointer in [u64::MAX, u64::MAX - 0x100, 0x7fff_ffff_ffff] {
        let mut f = seed();
        f.q(BASE + TABLE + 0x28, pointer);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| generation(&f, &[ROSTER])))
                .unwrap()
                .is_err()
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| deep(&f, &[ROSTER])))
                .unwrap()
                .is_err()
        );
    }
}
