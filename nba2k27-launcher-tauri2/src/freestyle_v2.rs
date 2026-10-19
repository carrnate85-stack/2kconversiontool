//! Version 2 WNBA Freestyle category navigation. This module cannot write memory.
//!
//! Production WPF uses four shared category-table DWORDs, not the experimental
//! combined-list mask/menu-field writes. Applying requires a stable, deep-checked
//! Freestyle picker. Once applied it is observed for the exact game lifetime;
//! returning to Quick Play does not reapply or adopt someone else's changes.
use crate::blacktop_native::Memory;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const PREVIOUS: u64 = 0x7A9FB10;
pub(crate) const NEXT: u64 = 0x7A9FB30;
pub(crate) const PAGE: u64 = 0x7A9F000;
pub(crate) const PAGE_BYTES: usize = 4096;
const MASK_PAGES: u64 = 0x7AC98A8;
const SELECTED: u64 = 0xB1E9BA8;
const OTHER_SELECTED: u64 = 0xB1EA100;
const MASK_WINDOW: u64 = 0xB1EA738;
const SETUP: u64 = 0xB1EA768;
const PICKER: u64 = 0xB1F9050;
const PICKER_VTABLE: u64 = 0x7B6B768;
const ROSTER_ROOT: u64 = 0x9ACE338;
const MODES: u64 = 0xAD07408;
const CURRENT: u64 = 0xAD07BD0;
const ALTERNATE: u64 = 0xAD07BCC;
const FREESTYLE: u32 = 0xF54D5D12;
pub(crate) const TEAM_STRIDE: usize = 0x1550;
const STOCK_MASK: u64 = 0x401033;
const CATEGORY_MASKS: [u64; 5] = [0x400031, 0x400002, 0x48000000, 0x20400030, 0x400000];
const ORIGINAL: [[u32; 6]; 2] = [[4, 0, 1, 3, 2, 5], [1, 2, 4, 3, 0, 5]];
const ENABLED: [[u32; 6]; 2] = [[4, 0, 1, 2, 3, 5], [1, 2, 3, 4, 0, 5]];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegionKind {
    Image,
    Private,
}
/// The caller must query a retained process handle and prove the ENTIRE requested
/// range is committed and contained in this allocation, not just its first byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Region {
    pub allocation: u64,
    pub kind: RegionKind,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
}
pub(crate) trait Regions {
    fn region(&self, at: u64, len: usize) -> Result<Region>;
}
fn address(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base().checked_add(rva).context("Freestyle RVA overflow")
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        at >= 0x10000
            && len > 0
            && len <= 1024 * TEAM_STRIDE
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7FFFFFFFFFFF),
        "Freestyle read range rejected"
    );
    let mut bytes = Vec::with_capacity(len);
    // The shared reader deliberately caps each ReadProcessMemory at 4096 bytes.
    while bytes.len() < len {
        m.checkpoint()?;
        let n = (len - bytes.len()).min(4096);
        let part = m.read(at + bytes.len() as u64, n)?;
        ensure!(
            part.len() == n,
            "Partial Freestyle read; snapshot discarded"
        );
        bytes.extend_from_slice(&part);
    }
    Ok(bytes)
}
fn image(m: &impl Memory, rva: u64, len: usize) -> Result<Vec<u8>> {
    read(m, address(m, rva)?, len)
}
fn d(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn q(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn pointer(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(q(&read(m, at, 8)?, 0))
}
fn area(
    m: &impl Memory,
    regions: &impl Regions,
    at: u64,
    len: usize,
    kind: RegionKind,
    writable: bool,
) -> Result<Region> {
    ensure!(
        at >= 0x10000
            && len > 0
            && len <= 1024 * TEAM_STRIDE
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7FFFFFFFFFFF),
        "Freestyle allocation range rejected"
    );
    let r = regions.region(at, len)?;
    ensure!(
        r.kind == kind
            && r.readable
            && !r.executable
            && (!writable || r.writable)
            && r.allocation >= 0x10000
            && r.allocation <= at
            && (kind != RegionKind::Image || r.allocation == m.base()),
        "Freestyle data allocation/protection differs"
    );
    Ok(r)
}

/// The first two tuple fields are an instruction RVA and its opcode prefix;
/// the last is its independently disassembled, exact RIP-relative target.
pub(crate) const RIP_ANCHORS: &[(u64, &[u8], u64)] = &[
    (0x36342A4, &[0x48, 0x8D, 0x05], PREVIOUS),
    (0x3634591, &[0x48, 0x8D, 0x05], NEXT),
    (0x363584A, &[0x8B, 0x05], SELECTED + 8),
    (0x3635854, &[0x48, 0x8D, 0x0D], MASK_PAGES),
    (0x363585F, &[0x48, 0x8D, 0x0D], MASK_WINDOW + 8),
    (0x363589A, &[0x48, 0x3B, 0x05], SELECTED),
    (0x363590A, &[0x8B, 0x05], OTHER_SELECTED + 8),
    (0x3635965, &[0x48, 0x8B, 0x05], OTHER_SELECTED),
    (0x37CAC51, &[0x48, 0x8D, 0x05], PICKER_VTABLE),
    (0x37CAC58, &[0x48, 0x89, 0x05], PICKER),
    (0x3622600, &[0x48, 0x8D, 0x05], PICKER),
    (0x211A0FB, &[0x48, 0x8B, 0x05], ROSTER_ROOT),
    (0x211A120, &[0x48, 0x8B, 0x05], ROSTER_ROOT),
    (0x363330C, &[0x89, 0x2D], SETUP),
    (0x3633323, &[0x89, 0x3D], SETUP + 12),
    (0x3633329, &[0x89, 0x1D], SETUP + 20),
    (0x308C649, &[0x48, 0x8D, 0x05], MODES),
    (0x308C650, &[0x48, 0x63, 0x0D], ALTERNATE),
    (0x300D703, &[0x48, 0x63, 0x05], CURRENT),
];
pub(crate) const FIELD_ANCHORS: &[(u64, &[u8])] = &[
    (0x211A102, &[0x48, 0x8B, 0x80, 0xA8, 0, 0, 0]),
    (0x2DDD3B1, &[0x3B, 0x91, 0x80, 0, 0, 0]),
    (
        0x2DDD3BB,
        &[
            0x48, 0x69, 0xC0, 0x50, 0x15, 0, 0, 0x48, 0x03, 0x81, 0x88, 0, 0, 0,
        ],
    ),
    (
        0x27E7950,
        &[0x41, 0x8B, 0x86, 0x3C, 0x12, 0, 0, 0xC1, 0xE8, 0x1A],
    ),
    (0x2DDA683, &[0x0F, 0xB7, 0x81, 0x20, 0x0F, 0, 0]),
    (0x2DDA68E, &[0x48, 0x8D, 0x81, 0xFA, 0x02, 0, 0]),
    (
        0x3629D0A,
        &[
            0xC6, 0x44, 0x24, 0x20, 1, 0xBA, 6, 0, 0, 0, 0x41, 0xB8, 1, 0, 0, 0, 0x41, 0xB9, 4, 0,
            0, 0,
        ],
    ),
    (0x363334C, &[0xB8, 0x33, 0x10, 0x40, 0]),
];
/// Immutable feature-specific proof. Does not relax the legacy global write gate
/// and deliberately accepts neither the previous build nor category-table edits.
pub(crate) fn validate_profile(m: &impl Memory) -> Result<()> {
    m.checkpoint()?;
    let h = image(m, 0, 4096)?;
    ensure!(&h[..2] == b"MZ", "Invalid Freestyle executable header");
    let pe = d(&h, 60) as usize;
    ensure!(
        (64..=3800).contains(&pe)
            && &h[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(h[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(h[pe + 24..pe + 26].try_into()?) == 0x20B
            && d(&h, pe + 8) == 0x6AC53765
            && d(&h, pe + 80) == 0x35BDE000,
        "WNBA Freestyle requires the verified Version 2 profile"
    );
    for &(rva, prefix, target) in RIP_ANCHORS {
        let b = image(m, rva, prefix.len() + 4)?;
        ensure!(
            b[..prefix.len()] == *prefix,
            "Freestyle code anchor differs at {rva:X}"
        );
        let relative = i32::from_le_bytes(b[prefix.len()..].try_into()?);
        ensure!(
            rva as i64 + b.len() as i64 + relative as i64 == target as i64,
            "Freestyle code anchor targets a different object at {rva:X}"
        );
    }
    for &(rva, expected) in FIELD_ANCHORS {
        ensure!(
            image(m, rva, expected.len())? == expected,
            "Freestyle field signature differs at {rva:X}"
        );
    }
    let masks = image(m, MASK_PAGES, 40)?;
    ensure!(
        masks
            .chunks_exact(8)
            .zip(CATEGORY_MASKS)
            .all(|(b, expected)| q(b, 0) == expected),
        "Freestyle category filters were changed by another tool"
    );
    let vt = image(m, PICKER_VTABLE, 24)?;
    for (n, method) in [0x23120, 0x38299A0, 0x24EB5C0].into_iter().enumerate() {
        ensure!(
            Some(q(&vt, n * 8)) == m.base().checked_add(method),
            "Freestyle picker class table differs"
        );
    }
    m.checkpoint()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Tables(pub [[u32; 6]; 2]);
impl Tables {
    pub(crate) fn original() -> Self {
        Self(ORIGINAL)
    }
    pub(crate) fn enabled() -> Self {
        Self(ENABLED)
    }
    pub(crate) fn read(m: &impl Memory) -> Result<Self> {
        let mut result = [[0; 6]; 2];
        for (row, rva) in result.iter_mut().zip([PREVIOUS, NEXT]) {
            let b = image(m, rva, 24)?;
            for (i, value) in row.iter_mut().enumerate() {
                *value = d(&b, i * 4);
            }
        }
        Ok(Self(result))
    }
    /// Partial mixtures are only recoverable with a matching durable journal.
    /// This predicate by itself never grants ownership or adoption authority.
    pub(crate) fn allowed_mixture(&self) -> bool {
        self.0.iter().enumerate().all(|(t, row)| {
            row.iter()
                .enumerate()
                .all(|(i, v)| *v == ORIGINAL[t][i] || *v == ENABLED[t][i])
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Change {
    pub rva: u64,
    pub before: u32,
    pub after: u32,
}
pub(crate) const CHANGES: [Change; 4] = [
    Change {
        rva: PREVIOUS + 12,
        before: 3,
        after: 2,
    },
    Change {
        rva: PREVIOUS + 16,
        before: 2,
        after: 3,
    },
    Change {
        rva: NEXT + 8,
        before: 4,
        after: 3,
    },
    Change {
        rva: NEXT + 12,
        before: 3,
        after: 4,
    },
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ApplyPlan {
    pub changes: [Change; 4],
    pub before: Tables,
    pub after: Tables,
}
impl ApplyPlan {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.changes == CHANGES
                && self.before == Tables::original()
                && self.after == Tables::enabled(),
            "Freestyle plan is outside the four-DWORD allowlist"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContextSnapshot {
    base: u64,
    mode: u32,
    alternate: u32,
    setup: Vec<u8>,
    root: u64,
    owner: u64,
    header: Vec<u8>,
    table: u64,
    count: usize,
    category: Vec<u8>,
    other: Vec<u8>,
    mask: Vec<u8>,
}
/// Facts-only, bounded component telemetry. No pointers, team bytes, or keys.
#[derive(Debug)]
pub(crate) struct ReadinessChanged {
    pub(crate) components: u16,
}
impl std::fmt::Display for ReadinessChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Freestyle selection/roster changed (components=0x{:03X})",
            self.components
        )
    }
}
impl std::error::Error for ReadinessChanged {}
impl ContextSnapshot {
    fn difference(&self, b: &Self) -> u16 {
        let fields = [
            self.base != b.base,
            self.mode != b.mode,
            self.alternate != b.alternate,
            self.setup != b.setup,
            self.root != b.root,
            self.owner != b.owner,
            self.header != b.header,
            self.table != b.table,
            self.count != b.count,
            self.category != b.category,
            self.other != b.other,
            self.mask != b.mask,
        ];
        fields
            .into_iter()
            .enumerate()
            .fold(0, |bits, (i, changed)| bits | (u16::from(changed) << i))
    }
    fn require_same(&self, b: &Self) -> Result<()> {
        let components = self.difference(b);
        if components != 0 {
            return Err(ReadinessChanged { components }.into());
        }
        Ok(())
    }
}
/// Fresh compact metadata only. This cannot establish immutable team rows or
/// produce a plan: the caller must retain its deep pair and perform deep post.
/// ALL context fields remain exact, including selected/category/mask windows.
pub(crate) fn verify_compact(
    m: &impl Memory,
    regions: &impl Regions,
    baseline: &Snapshot,
) -> Result<()> {
    validate_profile(m)?;
    let first = context(m, regions)?;
    baseline.context.require_same(&first)?;
    let second = context(m, regions)?;
    first.require_same(&second)?;
    baseline.context.require_same(&second)?;
    m.checkpoint()
}
pub(crate) fn verify_deep_equal(baseline: &Snapshot, fresh: &Snapshot) -> Result<()> {
    baseline.context.require_same(&fresh.context)?;
    if baseline.rows != fresh.rows
        || baseline.team_count != fresh.team_count
        || baseline.eligible != fresh.eligible
        || baseline.profile != fresh.profile
    {
        return Err(ReadinessChanged { components: 0x1000 }.into());
    }
    Ok(())
}
/// A small, deeply owned selection/roster generation fingerprint. It contains
/// no team rows, so an unchanged missing roster can be observed without
/// rereading roughly two megabytes every provider tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RosterFingerprint(ContextSnapshot);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RosterProfile {
    Mixed,
    WnbaOnly,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    context: ContextSnapshot,
    rows: Vec<u8>,
    pub team_count: usize,
    pub eligible: usize,
    pub profile: RosterProfile,
}
impl Snapshot {
    pub(crate) fn same(&self, other: &Self) -> bool {
        self == other
    }
}
fn read_mode(m: &impl Memory, rva: u64) -> Result<u32> {
    let index = d(&image(m, rva, 4)?, 0);
    ensure!(index < 128, "Freestyle mode index outside native table");
    if d(&image(m, MODES + u64::from(index) * 64, 4)?, 0) != FREESTYLE {
        return Err(ReadinessChanged { components: 0x006 }.into());
    }
    Ok(index)
}
fn context(m: &impl Memory, regions: &impl Regions) -> Result<ContextSnapshot> {
    m.checkpoint()?;
    let mode = read_mode(m, CURRENT)?;
    let alternate = read_mode(m, ALTERNATE)?;
    area(m, regions, address(m, SETUP)?, 24, RegionKind::Image, true)?;
    let setup = image(m, SETUP, 24)?;
    ensure!(
        d(&setup, 0) == 6 && d(&setup, 12) == 1 && d(&setup, 16) == 1 && d(&setup, 20) == 4,
        "Waiting for the initialized Freestyle team picker"
    );
    area(m, regions, address(m, PICKER)?, 8, RegionKind::Image, true)?;
    ensure!(
        pointer(m, address(m, PICKER)?)? == address(m, PICKER_VTABLE)?,
        "Waiting for the Freestyle picker class"
    );
    let root = pointer(m, address(m, ROSTER_ROOT)?)?;
    area(m, regions, root, 0xB0, RegionKind::Private, false)?;
    let owner = pointer(m, root + 0xA8)?;
    let allocation = area(m, regions, owner, 0x90, RegionKind::Private, false)?.allocation;
    let header = read(m, owner + 0x80, 16)?;
    let count = d(&header, 0) as usize;
    ensure!(
        (1..=1024).contains(&count),
        "Waiting for the mixed NBA/WNBA roster"
    );
    let table = q(&header, 8);
    let table_allocation = area(
        m,
        regions,
        table,
        count * TEAM_STRIDE,
        RegionKind::Private,
        false,
    )?
    .allocation;
    ensure!(
        allocation == table_allocation,
        "The Freestyle roster allocation is changing"
    );
    area(
        m,
        regions,
        address(m, SELECTED)?,
        20,
        RegionKind::Image,
        true,
    )?;
    area(
        m,
        regions,
        address(m, OTHER_SELECTED)?,
        12,
        RegionKind::Image,
        true,
    )?;
    area(
        m,
        regions,
        address(m, MASK_WINDOW)?,
        24,
        RegionKind::Image,
        true,
    )?;
    let category = image(m, SELECTED, 20)?;
    let other = image(m, OTHER_SELECTED, 12)?;
    for selected in [q(&category, 0), q(&other, 0)] {
        ensure!(
            selected >= table
                && selected < table + (count * TEAM_STRIDE) as u64
                && (selected - table) % TEAM_STRIDE as u64 == 0,
            "Waiting for initialized team choices"
        );
    }
    let mask = image(m, MASK_WINDOW, 24)?;
    ensure!(
        q(&mask, 8) == STOCK_MASK && d(&category, 8) < 5 && d(&other, 8) < 5,
        "A different Freestyle filter experiment is active; no category writes allowed"
    );
    Ok(ContextSnapshot {
        base: m.base(),
        mode,
        alternate,
        setup,
        root,
        owner,
        header,
        table,
        count,
        category,
        other,
        mask,
    })
}
pub(crate) fn fingerprint(m: &impl Memory, regions: &impl Regions) -> Result<RosterFingerprint> {
    validate_profile(m)?;
    Ok(RosterFingerprint(context(m, regions)?))
}
/// Read the complete roster with bounded chunks and recapture all ownership and
/// picker windows. No mixed generation can yield a write plan.
pub(crate) fn capture(m: &impl Memory, regions: &impl Regions) -> Result<Snapshot> {
    capture_with_profile(m, regions, false)
}

/// The sibling stock TKU WNBA loader can legitimately return a WNBA-only
/// roster on Version 2. Accept that shape only after the runtime has proven it
/// owned the stock-loader swap; ordinary category-only capture remains mixed
/// roster strict.
pub(crate) fn capture_after_wnba_loader(
    m: &impl Memory,
    regions: &impl Regions,
) -> Result<Snapshot> {
    capture_with_profile(m, regions, true)
}

fn capture_with_profile(
    m: &impl Memory,
    regions: &impl Regions,
    allow_wnba_only: bool,
) -> Result<Snapshot> {
    validate_profile(m)?;
    let before = context(m, regions)?;
    let rows = read(m, before.table, before.count * TEAM_STRIDE)?;
    before.require_same(&context(m, regions)?)?;
    m.checkpoint()?;
    let (eligible, profile) = verify_teams_with_profile(&rows, before.count, allow_wnba_only)?;
    Ok(Snapshot {
        team_count: before.count,
        context: before,
        rows,
        eligible,
        profile,
    })
}
/// Call with TWO independently captured deep snapshots immediately before the
/// executor journals its intent. Already-enabled or partially changed tables
/// cannot be adopted even though each individual value is on the allowlist.
pub(crate) fn plan(tables: Tables, first: &Snapshot, second: &Snapshot) -> Result<ApplyPlan> {
    ensure!(
        tables == Tables::original(),
        "Freestyle categories are not stock; ownership cannot be adopted"
    );
    verify_deep_equal(first, second)?;
    let plan = ApplyPlan {
        changes: CHANGES,
        before: Tables::original(),
        after: Tables::enabled(),
    };
    plan.validate()?;
    Ok(plan)
}
/// Applied categories persist across mode boundaries. Do not require the picker
/// to remain open, and never silently rewrite on return from Quick Play.
pub(crate) fn observe(tables: Tables) -> Result<()> {
    ensure!(
        tables == Tables::enabled(),
        "Freestyle navigation changed after applying; no automatic overwrite"
    );
    Ok(())
}
#[derive(Debug)]
pub(crate) struct MissingRosterTeams {
    pub teams: usize,
    pub women: usize,
    pub national: usize,
}
impl std::fmt::Display for MissingRosterTeams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The loaded roster has {} teams, with {}/15 expected WNBA clubs and {}/2 national teams. WNBA navigation was not applied; the missing teams must be loaded first.",
            self.teams, self.women, self.national
        )
    }
}
impl std::error::Error for MissingRosterTeams {}
fn verify_teams(rows: &[u8], count: usize) -> Result<usize> {
    Ok(verify_teams_with_profile(rows, count, false)?.0)
}
fn verify_teams_with_profile(
    rows: &[u8],
    count: usize,
    allow_wnba_only: bool,
) -> Result<(usize, RosterProfile)> {
    ensure!(
        (1..=1024).contains(&count) && rows.len() == count * TEAM_STRIDE,
        "Freestyle roster length rejected"
    );
    let (mut women, mut national, mut counts) = (
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeMap::<u32, usize>::new(),
    );
    let mut eligible = 0;
    for row in rows.chunks_exact(TEAM_STRIDE) {
        let id = u16::from_le_bytes(row[0xF20..0xF22].try_into()?);
        let kind = d(row, 0x123C) >> 26;
        *counts.entry(kind).or_default() += 1;
        // This count is diagnostic only; the combined mask is NEVER written.
        if (0x68400033u64 & (1u64 << kind)) != 0 {
            eligible += 1;
        }
        if kind == 29 || (kind == 22 && [69, 361].contains(&id)) {
            let units: Vec<u16> = row[0x2FA..0x32A]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
                .take_while(|c| *c != 0)
                .collect();
            let name = String::from_utf16(&units)
                .context("Freestyle team name is not initialized UTF-16")?;
            ensure!(
                !name.is_empty() && name.chars().all(|c| !c.is_control()),
                "Freestyle team name is not initialized"
            );
            ensure!(
                if kind == 29 {
                    women.insert(id)
                } else {
                    national.insert(id)
                },
                "Duplicate Freestyle team identity rejected"
            );
        }
    }
    let expected_women: BTreeSet<_> = (300..312).chain([315, 316, 317]).collect();
    let expected_national = BTreeSet::from([69, 361]);
    if women != expected_women {
        return Err(MissingRosterTeams {
            teams: count,
            women: women.intersection(&expected_women).count(),
            national: national.intersection(&expected_national).count(),
        }
        .into());
    }
    if national == expected_national
        && counts.get(&0) == Some(&30)
        && counts.get(&1).copied().unwrap_or(0) > 0
        && counts.get(&27).copied().unwrap_or(0) > 0
    {
        return Ok((eligible, RosterProfile::Mixed));
    }
    ensure!(
        allow_wnba_only
            && national.is_empty()
            && counts.get(&0).copied().unwrap_or(0) == 0
            && counts.get(&29) == Some(&15),
        "NBA, Classic and All-Time teams are not ready, and the roster is not a verified WNBA-only stock load"
    );
    Ok((eligible, RosterProfile::WnbaOnly))
}

#[cfg(test)]
#[path = "freestyle_v2_tests.rs"]
mod tests;
