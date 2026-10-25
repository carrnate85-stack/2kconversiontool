//! Unwired Version 2 QA capability for the native gameplay setting only.
//! The prior Normal filename belongs to props too and is NEVER edited here.
//! No scalar permit exists without an empty typed world and unloaded resources.
#![allow(dead_code)]
use crate::{
    ball_v2,
    blacktop_arena::Inspect,
    blacktop_ball as ball,
    blacktop_native::{ContextState, Memory},
    rules_v2,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub(crate) const WORLD_ROOT: u64 = 0x0BBB5678;
const WORLD_VT: u64 = 0x07E54F28;
const MANAGER_VT: u64 = 0x07E03F48;
const MODEL: u64 = 0xDA4;
const STRIDE: u64 = 0x1A0;
const RECORD_OFFSET: u64 = 0xF4D28;
const ANCHORS: &[(u64, &[u8])] = &[
    (0x4140EB0, &[0x48, 0x8b, 0x05, 0xc1, 0x47, 0xa7, 0x07, 0xc3]),
    (
        0x417ADF0,
        &[
            0x48, 0x8b, 0x81, 0x48, 0x03, 0, 0, 0x48, 0x8b, 0x80, 0x90, 0, 0, 0, 0xc3,
        ],
    ),
    (
        0x428A4CB,
        &[0x48, 0x8d, 0x05, 0x76, 0x9a, 0xb7, 0x03, 0x48, 0x89, 0x06],
    ),
    (0x3C970E7, &[0x48, 0x89, 0x50, 0x18]),
    (
        0x3C96B16,
        &[
            0x48, 0x8d, 0x48, 0x10, 0x45, 0x89, 0xc0, 0x4c, 0x8d, 0x0d, 0x2c, 0xac, 0xfe, 0x03,
        ],
    ),
    (0x3C96B41, &[0x4c, 0x8b, 0x40, 0x10, 0x49, 0x39, 0xc0]),
    (0x3C96B58, &[0x49, 0x8b, 0x00, 0x49, 0x8b, 0x50, 0x10]),
    (
        0x3C89720,
        &[0x48, 0x8d, 0x05, 0xb1, 0x8d, 0xff, 0x03, 0x49, 0x89, 0x07],
    ),
    (0x4140364, &[0xe8, 0x77, 0x77, 0x46, 0xfe, 0x89, 0x46, 0x74]),
    (
        0x4427539,
        &[
            0x8b, 0x94, 0x24, 0x8c, 0, 0, 0, 0x44, 0x8b, 0x84, 0x24, 0xd4, 0, 0, 0, 0x48, 0x89,
            0xc1, 0xe8, 0xe0, 0x2f, 0xe6, 0xff,
        ],
    ),
    (0x428A5E7, &[0x89, 0xbb, 0x4c, 0x03, 0, 0]),
    (0x417AEC9, &[0x41, 0x8b, 0x8e, 0x4c, 0x03, 0, 0]),
    (
        0x26FA9AB,
        &[
            0x48, 0x8b, 0x05, 0x6e, 0x24, 0x46, 0x08, 0x39, 0x88, 0xa4, 0x0d, 0, 0, 0x74, 0x06,
            0x89, 0x88, 0xa4, 0x0d, 0, 0,
        ],
    ),
    (
        0x2E95A4,
        &[
            0xe8, 0x47, 0xe5, 0x2b, 0x02, 0x89, 0xc1, 0xba, 0x4a, 0x11, 0x8f, 0xfd, 0x41, 0xb8,
            0x83, 0x03, 0, 0, 0xe8, 0x55, 0x3f, 0xd9, 0x02,
        ],
    ),
    (
        0x2E9932,
        &[
            0xe8, 0xb9, 0xe1, 0x2b, 0x02, 0x89, 0xc1, 0xba, 0x4a, 0x11, 0x8f, 0xfd, 0x41, 0xb8,
            0xd1, 0x03, 0, 0, 0xe8, 0x27, 0x42, 0xd9, 0x02,
        ],
    ),
];
fn addr(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base()
        .checked_add(rva)
        .context("Gameplay ball RVA overflow")
}
fn read(m: &impl Memory, a: u64, n: usize) -> Result<Vec<u8>> {
    ensure!(
        a >= 0x10000
            && (1..=8192).contains(&n)
            && a.checked_add(n as u64).is_some_and(|e| e < 0x7fff00000000),
        "Invalid bounded gameplay-ball read"
    );
    let mut bytes = Vec::with_capacity(n);
    for offset in (0..n).step_by(4096) {
        m.checkpoint()?;
        let count = (n - offset).min(4096);
        let next = m.read(a + offset as u64, count)?;
        ensure!(next.len() == count, "Partial gameplay-ball read");
        bytes.extend(next);
    }
    Ok(bytes)
}
fn q(m: &impl Memory, a: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, a, 8)?.try_into().unwrap()))
}
fn d(m: &impl Memory, a: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(read(m, a, 4)?.try_into().unwrap()))
}
fn private(m: &impl Inspect, a: u64, n: usize) -> Result<u64> {
    let r = m.region(a)?;
    ensure!(
        a % 4 == 0
            && r.committed
            && r.kind == 0x20000
            && r.protection == 4
            && r.allocation >= 0x10000
            && r.allocation % 4096 == 0
            && r.allocation <= r.start
            && a >= r.start
            && a.checked_add(n as u64)
                .zip(r.start.checked_add(r.len))
                .is_some_and(|(e, l)| e <= l),
        "Gameplay-ball owner is not exact private RW data"
    );
    Ok(r.allocation)
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    ball_v2::verify_profile(m)?;
    for &(rva, bytes) in ANCHORS {
        ensure!(
            read(m, addr(m, rva)?, bytes.len())? == bytes,
            "Version 2 gameplay-ball proof changed at {rva:X}"
        );
    }
    ensure!(
        read(m, addr(m, 0x7C81750)?, 24)?
            == [
                96u64.to_le_bytes(),
                120u64.to_le_bytes(),
                96u64.to_le_bytes()
            ]
            .concat(),
        "Gameplay-ball iterator layout changed"
    );
    // Exact primary RTTI association, not a merely executable-looking pointer.
    for (vt, col, ty, name) in [
        (WORLD_VT, 0x7E5E350, 0x9E01660, ".?AVAI_WORLD_FULL@@"),
        (
            MANAGER_VT,
            0x7E14B10,
            0x9DF9C10,
            ".?AVAI_BALL_MANAGER_BASKETBALL@@",
        ),
        (0x7C824D8, 0x7C87350, 0x9DD5210, ".?AVAI_BALL@@"),
    ] {
        ensure!(
            q(m, addr(m, vt - 8)?)? == addr(m, col)?,
            "Gameplay-ball typed vtable locator differs"
        );
        let c = read(m, addr(m, col)?, 24)?;
        ensure!(
            u32::from_le_bytes(c[0..4].try_into()?) == 1
                && c[4..12] == [0; 8]
                && u32::from_le_bytes(c[12..16].try_into()?) as u64 == ty
                && u32::from_le_bytes(c[20..24].try_into()?) as u64 == col,
            "Gameplay-ball typed primary RTTI changed"
        );
        let mut wanted = name.as_bytes().to_vec();
        wanted.push(0);
        ensure!(
            read(m, addr(m, ty + 16)?, wanted.len())? == wanted,
            "Gameplay-ball type identity changed"
        );
    }
    m.checkpoint()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EmptyWorld {
    world: u64,
    systems: u64,
    manager: u64,
    allocations: [u64; 3],
    sentinel: Vec<u8>,
}
pub(crate) fn validate_manifest(bytes: &[u8], selected: &str) -> Result<()> {
    ball::choice(selected)?;
    ensure!(
        !ball::current(selected) && !bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024,
        "Gameplay-ball source manifest is invalid"
    );
    let text = std::str::from_utf8(bytes)?;
    let mut found = 0;
    for line in text.lines() {
        let mut fields = line.split(',');
        let name = fields.next().unwrap_or("").replace('\\', "/");
        if !name.eq_ignore_ascii_case(selected) {
            continue;
        }
        let archive = fields.next().context("Missing ball source archive")?;
        let offset = fields
            .next()
            .context("Missing ball source offset")?
            .parse::<u64>()?;
        let len = fields
            .next()
            .context("Missing ball source length")?
            .parse::<u64>()?;
        ensure!(
            !archive.is_empty()
                && archive.len() <= 4
                && archive.bytes().all(|b| b.is_ascii_alphanumeric())
                && len > 0
                && offset.checked_add(len).is_some()
                && fields.next().is_none(),
            "Invalid selected ball source manifest row"
        );
        found += 1;
        ensure!(
            found == 1,
            "Selected ball source is duplicated in stock manifest"
        );
    }
    ensure!(
        found == 1,
        "Selected gameplay ball is absent from stock manifest"
    );
    Ok(())
}
fn manager_view(m: &impl Inspect) -> Result<EmptyWorld> {
    let world = q(m, addr(m, WORLD_ROOT)?)?;
    // Live 0.48 probe: at Version 2 Blacktop selection no gameplay world exists
    // yet (null root). Record that exact state; it cannot hold active balls.
    if world == 0 {
        return Ok(EmptyWorld {
            world: 0,
            systems: 0,
            manager: 0,
            allocations: [0; 3],
            sentinel: Vec::new(),
        });
    }
    let wa = private(m, world, 0x350)?;
    ensure!(
        q(m, world)? == addr(m, WORLD_VT)?,
        "Current gameplay world is not typed AI_WORLD_FULL"
    );
    let systems = q(m, world + 0x348)?;
    let sa = private(m, systems, 0x98)?;
    let manager = q(m, systems + 0x90)?;
    let ma = private(m, manager, 0x98)?;
    ensure!(
        q(m, manager)? == addr(m, MANAGER_VT)? && q(m, manager + 0x18)? == world,
        "Gameplay-ball manager/backlink changed"
    );
    let sentinel = read(m, manager + 0x60, 24)?;
    Ok(EmptyWorld {
        world,
        systems,
        manager,
        allocations: [wa, sa, ma],
        sentinel,
    })
}
/// Read-only confirmation of native simulation models, NOT final pixels.
/// The renderer queue is not used: the typed AI_BALL's own model is authoritative.
pub(crate) fn observe_gameplay(m: &impl Inspect) -> Result<(u32, Vec<u32>)> {
    verify_profile(m)?;
    let context = rules_v2::context(m)?;
    ensure!(
        rules_v2::boundary(&context, m.base()) == "match",
        "Gameplay confirmation requires a Blacktop match"
    );
    let setting = d(m, context.owner + MODEL)?;
    let view = manager_view(m)?;
    ensure!(view.world != 0, "No gameplay world is loaded yet");
    let sentinel = view.manager + 0x60;
    let mut node = u64::from_le_bytes(view.sentinel[16..24].try_into()?);
    let mut previous = sentinel;
    let mut nodes = Vec::new();
    let mut models = Vec::new();
    while node != sentinel {
        ensure!(
            nodes.len() < 28 && !nodes.iter().any(|(a, _, _, _, _)| *a == node),
            "Gameplay-ball active list is cyclic or exceeds native bound"
        );
        private(m, node, 24)?;
        let link = read(m, node, 24)?;
        ensure!(
            u64::from_le_bytes(link[8..16].try_into()?) == previous,
            "Gameplay-ball active list backlink changed"
        );
        let object = u64::from_le_bytes(link[..8].try_into()?);
        let allocation = private(m, object, 0x620)?;
        ensure!(
            q(m, object)? == addr(m, 0x7C824D8)? && q(m, object + 0x5E0)? == view.world,
            "Active gameplay ball type/world changed"
        );
        let model = canonical_ball(d(m, object + 0x34C)?);
        ensure!(model != 0, "Active gameplay ball model is unresolved");
        models.push(model);
        nodes.push((node, link.clone(), object, model, allocation));
        previous = node;
        node = u64::from_le_bytes(link[16..24].try_into()?);
    }
    ensure!(
        previous == u64::from_le_bytes(view.sentinel[8..16].try_into()?),
        "Gameplay-ball active list tail changed"
    );
    for (node, link, object, model, allocation) in nodes {
        ensure!(
            read(m, node, 24)? == link
                && q(m, object)? == addr(m, 0x7C824D8)?
                && q(m, object + 0x5E0)? == view.world
                && canonical_ball(d(m, object + 0x34C)?) == model
                && private(m, object, 0x620)? == allocation,
            "Gameplay-ball observation became torn"
        );
    }
    ensure!(
        view == manager_view(m)?
            && context == rules_v2::context(m)?
            && setting == d(m, context.owner + MODEL)?,
        "Gameplay-ball world/context changed during confirmation"
    );
    Ok((setting, models))
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
struct Record {
    model: u32,
    index: u32,
    address: u64,
    allocation: u64,
    bytes: Vec<u8>,
    references: i32,
    resource: Vec<u8>,
}
impl Record {
    fn unloaded(&self) -> bool {
        ball::unloaded(self.references, &self.resource)
    }
}
fn record(m: &impl Inspect, s: &ball_v2::Snapshot, model: u32) -> Result<Record> {
    ensure!(
        model != 0 && model != ball::DEFAULT,
        "Unresolved native gameplay ball alias"
    );
    let entries = read(m, s.lease.registry, s.lease.registry_count as usize * 8)?;
    let mut index = None;
    for e in entries.chunks_exact(8) {
        if u32::from_le_bytes(e[..4].try_into()?) == model {
            ensure!(index.is_none(), "Ambiguous gameplay model");
            index = Some(u32::from_le_bytes(e[4..].try_into()?));
        }
    }
    let index = index.context("Gameplay ball model is absent from registry")?;
    ensure!(index < 500, "Gameplay ball index changed");
    let address = s
        .lease
        .records
        .checked_add(index as u64 * STRIDE)
        .context("Gameplay ball record overflow")?;
    let allocation = private(m, address, STRIDE as usize)?;
    let bytes = read(m, address, STRIDE as usize)?;
    ensure!(
        u32::from_le_bytes(bytes[4..8].try_into()?) == model
            && ball::valid_filename(&ball::filename(&bytes)?),
        "Gameplay ball record identity changed"
    );
    Ok(Record {
        model,
        index,
        address,
        allocation,
        bytes,
        references: d(m, addr(m, ball_v2::REFERENCES + 0x18 + index as u64 * 4)?)? as i32,
        resource: read(
            m,
            addr(m, ball_v2::RENDERER + 0x28 + index as u64 * 48)?,
            48,
        )?,
    })
}
/// The court ball spawns from the game's own owner+0xDA4 setting; a launcher
/// write to it is reset at match load (live 0.49). WPF instead swapped the
/// asset filename of the record the court ball loads, while that record was
/// unloaded. V2 keeps WPF's normalization (disk 0x21E7B78: cmp edi,DC1BFEC5;
/// mov [rsp+48],CB5691BE): live 0.50 court ball DC1BFEC5 loaded CB5691BE. A
/// 0.49 session used 42A7A5EF directly. Any other stock choice is refused, as
/// WPF reported "incompatible".
pub(crate) const COURT_DEFAULT: u32 = 0x42A7A5EF;
pub(crate) const COURT_DEFAULT_FILE: &str = "ball_gameplay_official_DEFAULT.iff";
pub(crate) fn court_model(setting: u32) -> Result<u32> {
    let model = if setting == 0 || setting == ball::DEFAULT {
        ball::NORMAL
    } else {
        setting
    };
    ensure!(
        model == ball::NORMAL || model == COURT_DEFAULT,
        "The game selected a different stock court ball model; live swaps cannot prepare in this game process"
    );
    Ok(model)
}
fn court_file(model: u32) -> Option<&'static str> {
    match model {
        ball::NORMAL => Some(ball::NORMAL_FILE),
        COURT_DEFAULT => Some(COURT_DEFAULT_FILE),
        _ => None,
    }
}
/// Normalized model of an active gameplay ball (the default alias is the
/// NBA Official record, exactly as the native normalization above).
pub(crate) fn canonical_ball(model: u32) -> u32 {
    if model == ball::DEFAULT {
        ball::NORMAL
    } else {
        model
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    base: u64,
    context: ContextState,
    normal: ball_v2::Snapshot,
    original: Record,
    chosen: Record,
    selected: String,
}
pub(crate) fn capture(m: &impl Inspect, selected: &str) -> Result<Snapshot> {
    verify_profile(m)?;
    let normal = ball_v2::capture(m)?;
    let target = court_model(normal.native_ball)?;
    let source = ball_v2::resolve_excluding(m, &normal, selected, Some(target))?;
    let original = record(m, &normal, target)?;
    let chosen = record(m, &normal, source.model)?;
    ensure!(
        chosen.bytes == source.record
            && chosen.address == source.address
            && chosen.allocation == normal.lease.allocation
            && original.allocation == normal.lease.allocation,
        "Catalog model ownership changed"
    );
    let s = Snapshot {
        base: m.base(),
        context: normal.context.clone(),
        normal,
        original,
        chosen,
        selected: selected.into(),
    };
    ensure!(
        s.normal == ball_v2::capture(m)?
            && s.original == record(m, &s.normal, s.original.model)?
            && s.chosen == record(m, &s.normal, s.chosen.model)?
            && s.context == rules_v2::context(m)?,
        "Gameplay-ball state changed during capture"
    );
    Ok(s)
}
/// Filename-field bytes: stock court name (or the chosen name) plus the zero
/// terminator, over the longer of the two names. The unused tail must already
/// be zero in the stock record, exactly as WPF's "verified free space".
fn field(court: &Record, chosen: &Record) -> Result<(Vec<u8>, Vec<u8>)> {
    let stock = ball::filename(&court.bytes)?;
    let name = ball::filename(&chosen.bytes)?;
    ensure!(
        court_file(court.model) == Some(stock.as_str())
            && ball::valid_filename(&name)
            && !name.eq_ignore_ascii_case(&stock),
        "Court ball record is not its stock asset or the choice is invalid"
    );
    let len = stock.len().max(name.len()) + 1;
    ensure!(
        ball::FIELD + len <= STRIDE as usize
            && court.bytes[ball::FIELD + stock.len()..ball::FIELD + len]
                .iter()
                .all(|b| *b == 0),
        "Court ball filename has no verified free space"
    );
    let before = court.bytes[ball::FIELD..ball::FIELD + len].to_vec();
    let mut after = vec![0; len];
    after[..name.len()].copy_from_slice(name.as_bytes());
    Ok((before, after))
}
impl Snapshot {
    fn ready(&self) -> bool {
        rules_v2::boundary(&self.context, self.base) == "selection"
            && self.original.unloaded()
            && court_model(self.normal.native_ball).ok() == Some(self.original.model)
            && self.chosen.model != self.original.model
            && field(&self.original, &self.chosen).is_ok()
    }
    fn same_owner(&self, other: &Self) -> bool {
        // Lease identity only: its `original` byte copy is the NBA Official
        // record, which is itself the owned court record when the game uses
        // the default alias. verify() compares that record byte-exact.
        let (a, b) = (&self.normal.lease, &other.normal.lease);
        self.base == other.base
            && a.holder == b.holder
            && a.records == b.records
            && a.address == b.address
            && a.allocation == b.allocation
            && a.index == b.index
            && a.registry == b.registry
            && a.registry_count == b.registry_count
            && self.original.address == other.original.address
            && self.original.allocation == other.original.allocation
            && self.chosen.address == other.chosen.address
            && self.chosen.allocation == other.chosen.allocation
            && self.selected == other.selected
    }
}
/// Accumulates four independently spaced observations; a mismatch resets it.
pub(crate) struct Samples {
    first: Option<Snapshot>,
    last_ms: Option<u64>,
    count: usize,
}
impl Samples {
    pub(crate) fn new() -> Self {
        Self {
            first: None,
            last_ms: None,
            count: 0,
        }
    }
    /// A ready observation is pending its three spaced confirmations.
    pub(crate) fn started(&self) -> bool {
        self.count > 0
    }
    pub(crate) fn push(&mut self, s: Snapshot, elapsed_ms: u64) -> Result<Option<Plan>> {
        if !s.ready() {
            *self = Self::new();
            anyhow::bail!("Wait for Blacktop selection with the court ball resource unloaded");
        }
        if self.first.as_ref().is_some_and(|a| a != &s) {
            self.first = None;
            self.last_ms = None;
            self.count = 0;
        }
        if let Some(last) = self.last_ms {
            ensure!(
                elapsed_ms >= last,
                "Non-monotonic gameplay-ball sample clock"
            );
            if elapsed_ms - last < 100 {
                return Ok(None);
            }
        }
        if self.first.is_none() {
            self.first = Some(s.clone());
        }
        self.last_ms = Some(elapsed_ms);
        self.count = self.count.saturating_add(1).min(4);
        if self.count < 4 {
            return Ok(None);
        }
        let (before, after) = field(&s.original, &s.chosen)?;
        Ok(Some(Plan {
            original: s,
            before,
            after,
        }))
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    original: Snapshot,
    before: Vec<u8>,
    after: Vec<u8>,
}
impl Plan {
    pub(crate) fn selected(&self) -> &str {
        &self.original.selected
    }
    pub(crate) fn address(&self) -> u64 {
        self.original.original.address + ball::FIELD as u64
    }
    pub(crate) fn court(&self) -> u32 {
        self.original.original.model
    }
    #[cfg(test)]
    pub(crate) fn before_bytes(&self) -> &[u8] {
        &self.before
    }
    #[cfg(test)]
    pub(crate) fn after_bytes(&self) -> &[u8] {
        &self.after
    }
    pub(crate) fn allocation(&self) -> u64 {
        self.original.original.allocation
    }
    pub(crate) fn receipt(&self) -> Value {
        let s = &self.original;
        json!({"base":s.base,"selected":s.selected,"courtModel":s.original.model,"at":self.address(),"allocation":self.allocation(),"before":self.before,"after":self.after,"context":{"currentIndex":s.context.current_index,"alternateIndex":s.context.alternate_index,"current":s.context.current,"alternate":s.context.alternate,"owner":s.context.owner,"rootTag":s.context.root_tag,"stage":s.context.stage,"active":s.context.active,"kind":s.context.kind,"gameMode":s.context.game_mode},"normalLease":s.normal.lease,"courtRecord":s.original,"selectedRecord":s.chosen,"liveAcceptanceRequired":true})
    }
    /// Read-only: the court record now names the chosen file and the game has
    /// loaded it (references or renderer resource present).
    pub(crate) fn court_shows_choice(&self, m: &impl Inspect) -> Result<bool> {
        let court = record(m, &ball_v2::capture(m)?, self.original.original.model)?;
        Ok(court.address == self.original.original.address
            && court.bytes[ball::FIELD..ball::FIELD + self.after.len()] == self.after[..]
            && ball::observed_load(court.references, &court.resource))
    }
    fn verify(&self, m: &impl Inspect, undo: bool, after_write: bool) -> Result<()> {
        verify_profile(m)?;
        let s = capture(m, &self.original.selected)?;
        let expected = if undo ^ after_write {
            &self.after
        } else {
            &self.before
        };
        let at = ball::FIELD;
        let len = expected.len();
        let stock = &self.original.original.bytes;
        ensure!(
            s.original.bytes[at..at + len] == expected[..]
                && s.original.bytes[..at] == stock[..at]
                && s.original.bytes[at + len..] == stock[at + len..],
            "Court ball record is no longer exact-owned"
        );
        ensure!(
            self.original.same_owner(&s)
                && s.chosen == self.original.chosen
                && s.original.unloaded(),
            "Court ball owner, record or unload state changed"
        );
        let boundary = rules_v2::boundary(&s.context, s.base);
        ensure!(
            if undo {
                matches!(boundary, "selection" | "main-menu" | "quick-play")
            } else {
                boundary == "selection"
            },
            "Gameplay-ball publication boundary changed"
        );
        if !undo {
            ensure!(
                s.context == self.original.context
                    && court_model(s.normal.native_ball).ok() == Some(self.court()),
                "Selection or the game court-ball setting changed after stable samples"
            );
        }
        m.checkpoint()
    }
    pub(crate) fn forward(&self, m: &impl Inspect, manifest: &[u8]) -> Result<Permit<'_>> {
        self.forward_checked(m, &|selected| validate_manifest(manifest, selected))
    }
    /// `source` proves the selected stock row exactly as validate_manifest
    /// does (the runtime reuses a proof bound to the unchanged manifest file).
    pub(crate) fn forward_checked(
        &self,
        m: &impl Inspect,
        source: &dyn Fn(&str) -> Result<()>,
    ) -> Result<Permit<'_>> {
        source(self.selected())?;
        self.verify(m, false, false)?;
        Ok(Permit {
            plan: self,
            undo: false,
        })
    }
    pub(crate) fn restored(&self, m: &impl Inspect) -> Result<()> {
        self.verify(m, true, true)
    }
    /// Fresh read-only post-publication validation. This does not mint a write
    /// permit or reuse a previous match's observation.
    pub(crate) fn prepared(&self, m: &impl Inspect) -> Result<()> {
        self.verify(m, false, true)
    }
    pub(crate) fn restore_stable<'a>(
        &'a self,
        m: &impl Inspect,
        samples: &mut RestoreSamples,
        now_ms: u64,
    ) -> Result<Option<Permit<'a>>> {
        if let Err(e) = self.verify(m, true, false) {
            *samples = RestoreSamples::new();
            return Err(e);
        }
        let state = match capture(m, &self.original.selected) {
            Ok(state) => state,
            Err(error) => {
                *samples = RestoreSamples::new();
                return Err(error);
            }
        };
        if samples.state.as_ref() != Some(&state) {
            samples.state = Some(state);
            samples.last_ms = Some(now_ms);
            samples.count = 1;
            return Ok(None);
        }
        if let Some(last) = samples.last_ms {
            if now_ms < last {
                *samples = RestoreSamples::new();
                anyhow::bail!("Restore sample clock moved backwards");
            }
            if now_ms - last < 100 {
                return Ok(None);
            }
        }
        samples.last_ms = Some(now_ms);
        samples.count = samples.count.saturating_add(1).min(4);
        if samples.count < 4 {
            return Ok(None);
        }
        if let Err(error) = self.verify(m, true, false) {
            *samples = RestoreSamples::new();
            return Err(error);
        }
        Ok(Some(Permit {
            plan: self,
            undo: true,
        }))
    }
}
pub(crate) struct RestoreSamples {
    state: Option<Snapshot>,
    last_ms: Option<u64>,
    count: usize,
}
impl RestoreSamples {
    pub(crate) fn new() -> Self {
        Self {
            state: None,
            last_ms: None,
            count: 0,
        }
    }
    pub(crate) fn started(&self) -> bool {
        self.count > 0
    }
}
/// Opaque authority over the court record's filename field only. Persisted
/// receipt addresses cannot construct it.
pub(crate) struct Permit<'a> {
    plan: &'a Plan,
    undo: bool,
}
pub(crate) struct Change {
    pub address: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
impl Permit<'_> {
    pub(crate) fn selected(&self) -> &str {
        self.plan.selected()
    }
    pub(crate) fn change(&self) -> Result<Change> {
        Ok(Change {
            address: self.address(),
            before: self.before().to_vec(),
            after: self.after().to_vec(),
        })
    }
    pub(crate) fn address(&self) -> u64 {
        self.plan.address()
    }
    pub(crate) fn allocation(&self) -> u64 {
        self.plan.allocation()
    }
    pub(crate) fn before(&self) -> &[u8] {
        if self.undo {
            &self.plan.after
        } else {
            &self.plan.before
        }
    }
    pub(crate) fn after(&self) -> &[u8] {
        if self.undo {
            &self.plan.before
        } else {
            &self.plan.after
        }
    }
    pub(crate) fn undo(&self) -> bool {
        self.undo
    }
    pub(crate) fn verify_before(&self, m: &impl Inspect) -> Result<()> {
        self.plan.verify(m, self.undo, false)
    }
    pub(crate) fn verify_after(&self, m: &impl Inspect) -> Result<()> {
        self.plan.verify(m, self.undo, true)
    }
    pub(crate) fn receipt(&self) -> Value {
        json!({"plan":self.plan.receipt(),"undo":self.undo,"at":self.address(),"before":self.before(),"after":self.after()})
    }
}
#[cfg(test)]
#[path = "ball_gameplay_v2_tests.rs"]
pub(crate) mod tests;
