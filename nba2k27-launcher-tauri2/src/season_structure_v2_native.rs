//! Structural season transaction child. All native effects occur inside the
//! owned helper while the independently retained kernel state object is active.
//! Immutable pending intent authorizes exact derived spans, never replay.
use super::*;
use crate::{
    owned_transaction::{Context as Transaction, ReadContext},
    season_structure_v2 as graph,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::windows::fs::OpenOptionsExt,
    time::{SystemTime, UNIX_EPOCH},
};
const INTENT_LIMIT: usize = 64 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"C27SSV01";
const OWNER_BYTES: usize = 0x3d0;
const METHODS: &[(u64, usize, &str)] = &[
    // Current optional origin/name dictionary extent evidence, same as clone.
    (
        48107120,
        83,
        "6e367717cee6608cea195c8c55ac7bc18f875dfc15d351f88712e4f9f1d62e10",
    ),
    (
        48107264,
        214,
        "31454249e561831d510e9d2f7264292faac6009922484c2ef094a16c00d6132a",
    ),
    (
        0x2d398d0,
        691,
        "80b070e3a2d6433a7d29ea157d3d375e1d4d9ebd6b1381e5683117d8863994da",
    ),
    (
        0x2d3a3c0,
        624,
        "39212171afba42d7ad5927cd1954b34e3fdbe673c7da37ff7d72b41e1c669a1d",
    ),
    // Complete PERSONDATA enumerator and APPEARANCEDATA pool getter;
    // same read-only alias evidence as appearance-v2-production-proof.json.
    (
        0x2ca82d0,
        1775,
        "a38c4a0d4fda75ddb9e26ab36973c2a1a58c03221a82f1f2e3b75d3f9f04cf08",
    ),
    (
        0x2eddb00,
        180,
        "ab970e1b3ac8eedaacf380421b299f34e79cecf3d36b0718dd8ca5b19c461f49",
    ),
    (
        0x2e58120,
        180,
        "3151b876511aeded1369b806f3fab51e396f0b21b50b4f8bdae6836497b07b11",
    ),
    (
        0x2e59050,
        607,
        "af2fccdbf45187bf559f1dfa301059ce916a46166ae7f56cbd573c8127b4decc",
    ),
    (
        0x2e5bc20,
        594,
        "73b2abbd713886b293519d610477b526929f7561c2fb8f1c3cf49b049544a1a3",
    ),
    (
        0x2ddcf30,
        1126,
        "fef46200454646570eec38759a26d391c3dc50fe1369b304a680133209c7928a",
    ),
    (
        0x2eeb780,
        225,
        "c5f0362eb5e94626efe90a29cb4fbd6f4bad5153dd60b480e077c98933b1774c",
    ),
    (
        0x2eeb980,
        211,
        "a8ae048d5087c06ee54e3a9184d2342dafb1741766764969e5a646ee51351c9c",
    ),
    (
        0x2eeb950,
        39,
        "95564949fb3e503b977e169944cdbf37b285f7b77e374ddb0d7ff442c57170b2",
    ),
    (
        0x2fd0870,
        232,
        "62c65a65214862392c14e3aee85690ba899c05bdc258d9bbcba2d70f52a2b546",
    ),
    (
        0x2fd0830,
        60,
        "52ad9d8b2c3282f0d6fa8ca3c4c4a8624282cb9fadf65190015babb2caf99cc0",
    ),
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layout {
    image: u64,
    root: u64,
    owner: u64,
    allocation: u64,
    allocation_bytes: u64,
    players: u64,
    player_count: usize,
    teams: u64,
    team_count: usize,
    manager: u64,
    rows: u64,
    row_count: usize,
    headers: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selector {
    action: String,
    player_index: usize,
    expected_player_hash: String,
    expected_season_hash: String,
    expected_slot: usize,
    expected_row_index: usize,
    slot: Option<usize>,
    changes: Option<Value>,
    #[serde(default)]
    import: Option<BulkSelector>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BulkSelector {
    expected_teams_hash: String,
    expected_snapshot_hash: String,
    choices: Vec<graph::bulk::Choice>,
}
impl Selector {
    fn parse(v: &Value) -> Result<Self> {
        let s: Self = serde_json::from_value(v.clone())?;
        ensure!(
            matches!(
                s.action.as_str(),
                "addOlder" | "removeOldest" | "copyShared" | "importSeasons"
            ),
            "Unsupported structural season operation"
        );
        ensure!(
            s.expected_player_hash.len() == 64
                && s.expected_player_hash
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit()),
            "Invalid retained player snapshot digest"
        );
        Ok(s)
    }
    fn operation(&self) -> Result<graph::Operation> {
        ensure!(
            self.action == "importSeasons" || self.import.is_none(),
            "Unexpected import payload"
        );
        Ok(match self.action.as_str() {
            "importSeasons" => {
                let bulk = self
                    .import
                    .as_ref()
                    .context("Missing imported season choices")?;
                ensure!(
                    self.slot.is_none()
                        && self.changes.is_none()
                        && self.expected_slot == 0
                        && self.expected_row_index == 0
                        && bulk.expected_snapshot_hash.len() == 64
                        && bulk
                            .expected_snapshot_hash
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit())
                        && bulk.expected_teams_hash.len() == 64
                        && bulk
                            .expected_teams_hash
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit())
                        && (1..=25).contains(&bulk.choices.len()),
                    "Invalid imported season scope"
                );
                graph::Operation::Import {
                    player: self.player_index,
                    choices: bulk.choices.clone(),
                }
            }
            "addOlder" => {
                ensure!(
                    self.slot.is_none() && self.changes.is_none(),
                    "Unexpected add arguments"
                );
                graph::Operation::AddOlder {
                    player: self.player_index,
                }
            }
            "removeOldest" => {
                ensure!(
                    self.slot.is_none() && self.changes.is_none(),
                    "Unexpected remove arguments"
                );
                graph::Operation::RemoveOldest {
                    player: self.player_index,
                }
            }
            "copyShared" => graph::Operation::NarrowSharedCopy {
                player: self.player_index,
                slot: self.slot.context("Missing season slot")?,
                changes: self.changes.clone().context("Missing season changes")?,
            },
            _ => anyhow::bail!("Unsupported season action"),
        })
    }
}
struct Live<'a> {
    access: Access,
    transaction: RefCell<&'a mut Transaction>,
    reverse: bool,
}
impl Memory for Live<'_> {
    fn base(&self) -> u64 {
        self.access.base()
    }
    fn checkpoint(&self) -> Result<()> {
        self.transaction
            .borrow_mut()
            .checkpoint_read(self.reverse)?;
        self.access.checkpoint()
    }
    fn read(&self, at: u64, n: usize) -> Result<Vec<u8>> {
        self.checkpoint()?;
        self.access.read(at, n)
    }
}
fn bytes(m: &impl Memory, at: u64, n: usize) -> Result<Vec<u8>> {
    ensure!(n <= 20 * 1024 * 1024, "Structural read exceeds bound");
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let at = at
            .checked_add(out.len() as u64)
            .context("Structural read overflow")?;
        let count = (n - out.len()).min(4096 - (at as usize & 4095));
        m.checkpoint()?;
        let part = m.read(at, count)?;
        ensure!(part.len() == count, "Partial structural read");
        out.extend(part);
    }
    Ok(out)
}
fn q(v: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(v[i..i + 8].try_into().unwrap())
}
fn d(v: &[u8], i: usize) -> usize {
    u32::from_le_bytes(v[i..i + 4].try_into().unwrap()) as usize
}
fn digest(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}
fn proof(m: &impl Memory) -> Result<()> {
    profile::verify_identity_profile(m)?;
    services::verify_profile(m)?;
    crate::rules_v2::verify_profile(m)?;
    for &(rva, n, sha) in METHODS {
        ensure!(
            digest(&bytes(
                m,
                m.base()
                    .checked_add(rva)
                    .context("Structural profile overflow")?,
                n
            )?) == sha,
            "Structural season native method differs"
        );
    }
    Ok(())
}
fn private_span(
    a: &Access,
    state: &profile::OwnerSnapshot,
    at: u64,
    n: usize,
    write: bool,
) -> Result<()> {
    ensure!(
        n > 0
            && at >= state.region.base
            && at
                .checked_add(n as u64)
                .is_some_and(|end| end <= state.region.base + state.region.len),
        "Structural span escaped retained allocation"
    );
    let end = at + n as u64;
    let mut p = at;
    while p < end {
        let r = a.query(p)?;
        ensure!(
            private(r)
                && r.allocation == state.region.base
                && contains(r.base, r.len, p, 1)
                && (!write || r.protection == PAGE_READWRITE.0),
            "Structural span is not ordinary owned private data"
        );
        let next = r
            .base
            .checked_add(r.len)
            .context("Structural region overflow")?;
        ensure!(next > p, "Structural region failed to advance");
        p = next.min(end);
    }
    Ok(())
}
fn scope(m: &impl Memory) -> Result<Vec<u64>> {
    let a = crate::rules_v2::context(m)?;
    let b = crate::rules_v2::context(m)?;
    ensure!(
        a == b
            && a.current == a.alternate
            && a.current == 0x6509837b
            && a.stage <= 1
            && a.active == 0
            && a.kind == 0
            && a.root_tag == m.base() + 0x791e158,
        "Structural seasons require stable offline Home"
    );
    Ok(vec![
        a.current_index as u64,
        a.alternate_index as u64,
        a.current as u64,
        a.alternate as u64,
        a.stage as u64,
        a.active as u64,
        a.owner,
        a.kind as u64,
        a.game_mode as u64,
        a.root_tag,
    ])
}
#[derive(Clone)]
struct Captured {
    layout: Layout,
    graph: graph::Snapshot,
    teams: Vec<u8>,
    owner: Vec<u8>,
    scope: Vec<u64>,
}
impl Captured {
    fn same(&self, other: &Self) -> bool {
        self.layout == other.layout
            && self.graph == other.graph
            && self.teams == other.teams
            && self.owner == other.owner
            && self.scope == other.scope
    }
}
// Metadata spans may overlap one another. Directory spans must be pairwise
// disjoint and disjoint from every known metadata span; adjacency remains valid.
pub(super) fn directory_extents_disjoint(
    allocation: u64,
    allocation_bytes: u64,
    owner: u64,
    root: u64,
    extents: &[(u64, usize)],
) -> Result<()> {
    let limit = allocation
        .checked_add(allocation_bytes)
        .context("Allocation extent overflow")?;
    let prefix = allocation
        .checked_add(0x680)
        .context("Allocation prefix overflow")?;
    let protected = [
        (allocation, 0x680usize),
        (owner, OWNER_BYTES),
        (root, 0xb0usize),
    ];
    let mut ends = Vec::with_capacity(extents.len());
    for &(at, n) in extents {
        let end = at
            .checked_add(n as u64)
            .context("Directory extent overflow")?;
        ensure!(
            n > 0 && at >= prefix && end <= limit,
            "Structural directory overlaps allocation metadata or escapes owner"
        );
        for &(p, z) in &protected {
            let pend = p
                .checked_add(z as u64)
                .context("Metadata extent overflow")?;
            ensure!(
                end <= p || pend <= at,
                "Structural directory overlaps native owner/root metadata"
            );
        }
        ensure!(
            ends.iter().all(|&(p, e)| end <= p || e <= at),
            "Structural native directories overlap"
        );
        ends.push((at, end));
    }
    Ok(())
}
// The bounded transfer is shared by the real helper and pure/fault fixtures.
// No successful cached region proof can replace either fresh bracket.
fn frozen_transfer(
    n: usize,
    mut checkpoint: impl FnMut() -> Result<()>,
    mut region: impl FnMut() -> Result<()>,
    read: impl FnOnce(&mut [u8]) -> Result<usize>,
) -> Result<Vec<u8>> {
    ensure!(
        (1..=16 * 1024 * 1024).contains(&n),
        "Frozen structural directory read bound"
    );
    checkpoint()?;
    region()?;
    let mut out = vec![0u8; n];
    let done = read(&mut out)?;
    ensure!(done == n, "Partial frozen structural directory read");
    checkpoint()?;
    region()?;
    Ok(out)
}
fn frozen_directory_bytes(
    a: &Access,
    state: &profile::OwnerSnapshot,
    at: u64,
    n: usize,
    check: &dyn Fn() -> Result<()>,
) -> Result<Vec<u8>> {
    frozen_transfer(
        n,
        || {
            check()?;
            a.checkpoint()
        },
        || private_span(a, state, at, n, false),
        |out| {
            let mut done = 0usize;
            unsafe {
                windows::Win32::System::Diagnostics::Debug::ReadProcessMemory(
                    a.query.0,
                    at as _,
                    out.as_mut_ptr() as _,
                    out.len(),
                    Some(&mut done),
                )?;
            }
            Ok(done)
        },
    )
}
fn capture(
    m: &impl Memory,
    a: &Access,
    frozen: Option<&dyn Fn() -> Result<()>>,
) -> Result<Captured> {
    a.finish()?;
    proof(m)?;
    let boundary = scope(m)?;
    let region = a.region()?;
    let state = profile::inspect_owner(m, region)?;
    private_span(a, &state, state.owner, OWNER_BYTES, false)?;
    let owner_header = bytes(m, state.owner, OWNER_BYTES)?;
    let headers = owner_header[0x1c0..0x1e0].to_vec();
    let count = d(&headers, 0);
    let manager = q(&headers, 8);
    let row_count = d(&headers, 16);
    let rows = q(&headers, 24);
    ensure!(
        count == 1 && (1..32768).contains(&row_count) && manager % 8 == 0 && rows % 8 == 0,
        "Structural season directory differs"
    );
    let spans = [
        (manager, graph::MANAGER_BYTES),
        (rows, row_count * graph::STAT_BYTES),
    ];
    for (p, n) in spans {
        private_span(a, &state, p, n, false)?;
    }
    // Manager may end exactly where row0 begins (native stride56). It may never
    // overlap row0, players, teams, arenas or uniforms.
    let mut extents = vec![
        (
            state.players.pointer,
            state.players.count * state.players.stride,
        ),
        (state.teams.pointer, state.teams.count * state.teams.stride),
        (
            state.arenas.pointer,
            state.arenas.count * state.arenas.stride,
        ),
        (manager, graph::MANAGER_BYTES),
        (rows, row_count * graph::STAT_BYTES),
    ];
    if let Some(u) = &state.uniforms {
        extents.push((u.pointer, u.count * u.stride));
    }
    // PERSONDATA/APPEARANCEDATA share the allocation, never season storage.
    for (offset, stride, max) in [
        (0x180, 0x1c8usize, 8192usize),
        (0x3c0, 0x68usize, 65536usize),
    ] {
        let count = d(&owner_header, offset);
        let at = q(&owner_header, offset + 8);
        ensure!(
            (1..=max).contains(&count) && at % 8 == 0,
            "Structural alias directory differs"
        );
        let n = count
            .checked_mul(stride)
            .context("Alias directory size overflow")?;
        private_span(a, &state, at, n, false)?;
        extents.push((at, n));
    }
    for (offset, stride, max) in [
        (0x90, 72usize, 4096usize),
        (0x240, 48, 20000),
        (0x250, 48, 20000),
    ] {
        let count = d(&owner_header, offset);
        let at = q(&owner_header, offset + 8);
        if count == 0 && at == 0 {
            continue;
        }
        ensure!(
            (1..=max).contains(&count) && at % 8 == 0,
            "Structural optional dictionary differs"
        );
        let n = count
            .checked_mul(stride)
            .context("Dictionary extent overflow")?;
        private_span(a, &state, at, n, false)?;
        extents.push((at, n));
    }
    directory_extents_disjoint(region.base, region.len, state.owner, state.root, &extents)?;
    let layout = Layout {
        image: m.base(),
        root: state.root,
        owner: state.owner,
        allocation: region.base,
        allocation_bytes: region.len,
        players: state.players.pointer,
        player_count: state.players.count,
        teams: state.teams.pointer,
        team_count: state.teams.count,
        manager,
        rows,
        row_count,
        headers: headers.clone(),
    };
    let directory_read = |at, n| -> Result<Vec<u8>> {
        match frozen {
            Some(check) => frozen_directory_bytes(a, &state, at, n, check),
            None => bytes(m, at, n),
        }
    };
    let read = || -> Result<Captured> {
        let manager = bytes(m, manager, graph::MANAGER_BYTES)?;
        for (offset, rva) in [
            (0x18, 0x2eeb980),
            (0x20, 0x2eeb950),
            (0x28, 0x2fd0870),
            (0x30, 0x2fd0830),
        ] {
            ensure!(
                q(&manager, offset) == m.base() + rva,
                "Native season manager callback differs"
            );
        }
        for (p, n) in [
            (layout.players, layout.player_count * graph::PLAYER_BYTES),
            (layout.rows, layout.row_count * graph::STAT_BYTES),
            (layout.teams, layout.team_count * profile::TEAM_STRIDE),
            (layout.owner, OWNER_BYTES),
        ] {
            private_span(a, &state, p, n, false)?;
        }
        let graph = graph::Snapshot {
            players: directory_read(layout.players, layout.player_count * graph::PLAYER_BYTES)?
                .chunks_exact(graph::PLAYER_BYTES)
                .map(|r| r.try_into().unwrap())
                .collect(),
            rows: directory_read(layout.rows, layout.row_count * graph::STAT_BYTES)?
                .chunks_exact(graph::STAT_BYTES)
                .map(|r| r.try_into().unwrap())
                .collect(),
            manager: manager
                .try_into()
                .map_err(|_| anyhow::anyhow!("Partial manager"))?,
            pool: graph::PoolKind::Player,
            manager_count: 1,
            teams: graph::TeamDirectory {
                base: layout.teams,
                count: layout.team_count,
            },
        };
        Ok(Captured {
            layout: layout.clone(),
            graph,
            teams: directory_read(layout.teams, layout.team_count * profile::TEAM_STRIDE)?,
            owner: bytes(m, layout.owner, OWNER_BYTES)?,
            scope: boundary.clone(),
        })
    };
    let first = read()?;
    ensure!(
        first.owner == owner_header,
        "Structural alias directory changed before capture"
    );
    profile::revalidate(m, &state)?;
    ensure!(
        bytes(m, state.owner, OWNER_BYTES)? == owner_header && scope(m)? == boundary,
        "Structural capture changed headers or context"
    );
    let second = read()?;
    ensure!(
        first.same(&second),
        "Structural capture changed between passes"
    );
    profile::revalidate(m, &state)?;
    ensure!(
        bytes(m, state.owner, OWNER_BYTES)? == owner_header
            && scope(m)? == boundary
            && a.region()? == state.region,
        "Structural capture ending owner differs"
    );
    proof(m)?;
    a.finish()?;
    Ok(second)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentMeta {
    schema: String,
    game: Identity,
    parent: Identity,
    session: String,
    transaction: String,
    selector: Selector,
    layout: Layout,
    scope: Vec<u64>,
}
fn encode(meta: &IntentMeta, before: &Captured) -> Result<Vec<u8>> {
    let head = serde_json::to_vec(meta)?;
    ensure!(
        head.len() <= 65536,
        "Structural intent metadata exceeds bound"
    );
    let mut out = Vec::new();
    out.extend(MAGIC);
    out.extend((head.len() as u32).to_le_bytes());
    out.extend(head);
    for row in &before.graph.players {
        out.extend(row);
    }
    for row in &before.graph.rows {
        out.extend(row);
    }
    out.extend(before.graph.manager);
    out.extend(&before.teams);
    out.extend(&before.owner);
    ensure!(
        out.len() <= INTENT_LIMIT,
        "Structural intent exceeds durable byte bound"
    );
    Ok(out)
}
fn decode(raw: &[u8]) -> Result<(IntentMeta, Captured)> {
    ensure!(
        raw.len() >= 12 && raw.len() <= INTENT_LIMIT && &raw[..8] == MAGIC,
        "Unsupported structural intent"
    );
    let n = u32::from_le_bytes(raw[8..12].try_into()?) as usize;
    ensure!(
        n <= 65536 && n + 12 <= raw.len(),
        "Malformed structural intent metadata"
    );
    let meta: IntentMeta = serde_json::from_slice(&raw[12..12 + n])?;
    ensure!(
        meta.schema == "nba2k27-season-structure-v1"
            && meta.transaction.len() == 64
            && meta.transaction.bytes().all(|c| c.is_ascii_hexdigit())
            && (1..=8192).contains(&meta.layout.player_count)
            && (1..32768).contains(&meta.layout.row_count)
            && (1..=1024).contains(&meta.layout.team_count),
        "Malformed structural intent extents"
    );
    meta.selector.operation()?;
    let mut p = 12 + n;
    let mut take = |n: usize| -> Result<&[u8]> {
        let end = p.checked_add(n).context("Intent extent overflow")?;
        ensure!(end <= raw.len(), "Truncated structural intent");
        let x = &raw[p..end];
        p = end;
        Ok(x)
    };
    let players = take(meta.layout.player_count * graph::PLAYER_BYTES)?
        .chunks_exact(graph::PLAYER_BYTES)
        .map(|r| r.try_into().unwrap())
        .collect();
    let rows = take(meta.layout.row_count * graph::STAT_BYTES)?
        .chunks_exact(graph::STAT_BYTES)
        .map(|r| r.try_into().unwrap())
        .collect();
    let manager = take(graph::MANAGER_BYTES)?.try_into()?;
    let teams = take(meta.layout.team_count * profile::TEAM_STRIDE)?.to_vec();
    let owner = take(OWNER_BYTES)?.to_vec();
    ensure!(p == raw.len(), "Trailing structural intent bytes");
    let graph = graph::Snapshot {
        players,
        rows,
        manager,
        pool: graph::PoolKind::Player,
        manager_count: 1,
        teams: graph::TeamDirectory {
            base: meta.layout.teams,
            count: meta.layout.team_count,
        },
    };
    let capture = Captured {
        layout: meta.layout.clone(),
        graph,
        teams,
        owner,
        scope: meta.scope.clone(),
    };
    capture
        .graph
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid structural before graph: {e:?}"))?;
    Ok((meta, capture))
}
fn snapshot_digest(s: &Captured) -> Result<String> {
    let mut h = Sha256::new();
    h.update(b"season-structure-full-destination-v1");
    h.update(serde_json::to_vec(&s.layout)?);
    h.update(serde_json::to_vec(&s.scope)?);
    for p in &s.graph.players {
        h.update(p);
    }
    for r in &s.graph.rows {
        h.update(r);
    }
    h.update(s.graph.manager);
    h.update(&s.teams);
    h.update(&s.owner);
    Ok(format!("{:x}", h.finalize()))
}
fn history_digest(s: &graph::Snapshot) -> String {
    let mut h = Sha256::new();
    for row in &s.rows {
        h.update(row);
    }
    format!("{:x}", h.finalize())
}
fn plan(before: &Captured, s: &Selector) -> Result<graph::Plan> {
    s.operation()?;
    ensure!(
        s.player_index < before.graph.players.len()
            && digest(&before.graph.players[s.player_index])
                == s.expected_player_hash.to_ascii_lowercase(),
        "Selected player snapshot changed"
    );
    if s.action == "importSeasons" {
        let import = s
            .import
            .as_ref()
            .context("Missing imported season choices")?;
        ensure!(
            snapshot_digest(before)? == import.expected_snapshot_hash.to_ascii_lowercase()
                && digest(&before.teams) == import.expected_teams_hash.to_ascii_lowercase()
                && history_digest(&before.graph) == s.expected_season_hash.to_ascii_lowercase(),
            "Imported season review changed"
        );
        return match graph::plan(&before.graph, s.operation()?)
            .map_err(|e| anyhow::anyhow!("Imported season plan refused: {e:?}"))?
        {
            graph::Outcome::Planned(p) => Ok(p),
            graph::Outcome::Unchanged => anyhow::bail!("Imported seasons are unchanged"),
        };
    }
    ensure!(
        s.expected_slot < graph::REGULAR_SLOTS && s.expected_row_index < before.graph.rows.len(),
        "Stale structural season slot"
    );
    let selected = u16::from_le_bytes(
        before.graph.players[s.player_index]
            [0x130 + s.expected_slot * 2..0x132 + s.expected_slot * 2]
            .try_into()?,
    ) as usize;
    ensure!(
        selected == s.expected_row_index
            && digest(&before.graph.rows[selected]) == s.expected_season_hash.to_ascii_lowercase(),
        "Selected season row changed after review"
    );
    if matches!(s.action.as_str(), "addOlder" | "removeOldest") {
        let last = (0..graph::REGULAR_SLOTS)
            .filter(|&slot| {
                before.graph.players[s.player_index][0x130 + slot * 2..0x132 + slot * 2]
                    != [255, 255]
            })
            .last();
        ensure!(
            last == Some(s.expected_slot),
            "Selected season is no longer oldest"
        );
    }
    match graph::plan(&before.graph, s.operation()?)
        .map_err(|e| anyhow::anyhow!("Structural season operation refused: {e:?}"))?
    {
        graph::Outcome::Planned(p) => Ok(p),
        graph::Outcome::Unchanged => anyhow::bail!("Season operation is unchanged"),
    }
}
fn step_address(layout: &Layout, step: &graph::Step) -> Result<u64> {
    let (base, size) = match step.target {
        graph::Target::Manager => (layout.manager, graph::MANAGER_BYTES),
        graph::Target::Player(p) => {
            ensure!(p < layout.player_count, "Player span index differs");
            (
                layout
                    .players
                    .checked_add(
                        (p as u64)
                            .checked_mul(graph::PLAYER_BYTES as u64)
                            .context("Player span multiplication overflow")?,
                    )
                    .context("Player span base overflow")?,
                graph::PLAYER_BYTES,
            )
        }
        graph::Target::Row(r) => {
            ensure!(r < layout.row_count, "Season span index differs");
            (
                layout
                    .rows
                    .checked_add(
                        (r as u64)
                            .checked_mul(graph::STAT_BYTES as u64)
                            .context("Season span multiplication overflow")?,
                    )
                    .context("Season span base overflow")?,
                graph::STAT_BYTES,
            )
        }
    };
    ensure!(
        step.before.len() == step.after.len()
            && !step.before.is_empty()
            && step
                .offset
                .checked_add(step.before.len())
                .is_some_and(|n| n <= size),
        "Structural effect span differs"
    );
    base.checked_add(step.offset as u64)
        .context("Structural effect address overflow")
}
fn ensure_known(observed: &Captured, before: &Captured, expected: &graph::Snapshot) -> Result<()> {
    ensure!(
        observed.layout == before.layout
            && observed.owner == before.owner
            && observed.teams == before.teams
            && observed.scope == before.scope
            && observed.graph == *expected,
        "Structural native state differs from exact owned prefix"
    );
    Ok(())
}
fn append_progress(file: &mut File, event: &Value) -> Result<()> {
    let b = serde_json::to_vec(event)?;
    ensure!(b.len() <= 4096, "Structural progress record exceeds bound");
    file.write_all(&(b.len() as u32).to_le_bytes())?;
    file.write_all(&b)?;
    file.sync_all()?;
    Ok(())
}
fn write_step(
    live: &Live<'_>,
    handle: HANDLE,
    before: &Captured,
    step: &graph::Step,
    reverse: bool,
) -> Result<()> {
    let at = step_address(&before.layout, step)?;
    let state = profile::inspect_owner(live, live.access.region()?)?;
    private_span(&live.access, &state, at, step.before.len(), true)?;
    let (expected, value) = if reverse {
        (&step.after, &step.before)
    } else {
        (&step.before, &step.after)
    };
    ensure!(
        bytes(live, at, value.len())? == *expected,
        "Structural effect preimage changed"
    );
    live.access.finish()?;
    live.transaction.borrow_mut().before_effect(reverse)?;
    let mut written = 0;
    let result = unsafe {
        WriteProcessMemory(
            handle,
            at as _,
            value.as_ptr() as _,
            value.len(),
            Some(&mut written),
        )
    };
    ensure!(
        result.is_ok() && written == value.len(),
        "Structural native write was partial or failed"
    );
    live.transaction.borrow_mut().checkpoint_frozen(reverse)?;
    ensure!(
        bytes(live, at, value.len())? == *value,
        "Structural effect readback failed"
    );
    Ok(())
}
fn ordinary_new(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(1)
        .custom_flags(0x0020_0000)
        .open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "Structural journal is not an ordinary file"
    );
    Ok(file)
}
fn directory(root: &Path) -> Result<PathBuf> {
    let dir = root.join("Data").join("season-structure-v2");
    fs::create_dir_all(&dir)?;
    for p in [root.join("Data"), dir.clone()] {
        use std::os::windows::fs::MetadataExt;
        let m = fs::symlink_metadata(&p)?;
        ensure!(
            m.is_dir() && m.file_attributes() & 0x400 == 0,
            "Structural journal directory is redirected"
        );
    }
    Ok(dir)
}
fn inventory(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = root.join("Data").join("season-structure-v2");
    if !dir.try_exists()? {
        return Ok(Vec::new());
    }
    use std::os::windows::fs::MetadataExt;
    let meta = fs::symlink_metadata(&dir)?;
    ensure!(
        meta.is_dir() && meta.file_attributes() & 0x400 == 0,
        "Structural journal directory is redirected"
    );
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let e = entry?;
        if e.path().extension().and_then(|s| s.to_str()) != Some("pending") {
            continue;
        }
        ensure!(
            paths.len() < 128,
            "Structural journal inventory exceeds bound"
        );
        let name = e.path();
        let stem = name
            .file_stem()
            .and_then(|v| v.to_str())
            .context("Unknown structural journal filename")?;
        ensure!(
            stem.len() == 64 && stem.bytes().all(|c| c.is_ascii_hexdigit()),
            "Unknown structural journal filename"
        );
        paths.push(name);
    }
    Ok(paths)
}
fn read_pending(path: &Path, delete: bool) -> Result<(File, Vec<u8>, IntentMeta, Captured)> {
    let mut options = OpenOptions::new();
    options.read(true).share_mode(1).custom_flags(0x0020_0000);
    if delete {
        options.access_mode(0x8001_0000);
    }
    let mut f = options.open(path)?;
    use std::os::windows::fs::MetadataExt;
    let meta = f.metadata()?;
    ensure!(
        meta.is_file()
            && meta.file_attributes() & 0x400 == 0
            && (12..=INTENT_LIMIT as u64).contains(&meta.len()),
        "Structural journal is not ordinary bounded data"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(INTENT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == meta.len() && f.metadata()?.len() == meta.len(),
        "Structural journal changed while reading"
    );
    let (parsed, before) = decode(&bytes)?;
    ensure!(
        path.file_stem().and_then(|v| v.to_str()) == Some(parsed.transaction.as_str()),
        "Structural journal filename/identity mismatch"
    );
    Ok((f, bytes, parsed, before))
}
pub(super) fn pending(root: &Path) -> Result<bool> {
    for path in inventory(root)? {
        let (_file, raw, meta, _) = read_pending(&path, false)?;
        // Disk status files NEVER grant live permission. Only the exact current
        // parent's private watchdog response can populate this memory-only witness.
        if !crate::owned_transaction::verified_intent(&meta.game, &meta.session, &digest(&raw)) {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(super) fn resolve_ended(root: &Path, rows: &[Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving structural season transactions"
    );
    for path in inventory(root)? {
        let (file, _raw, _meta, _before) = read_pending(&path, true)?;
        // Rename the ORIGINAL retained file object with DELETE access and deny-write/
        // deny-delete sharing. No path-based replacement, overwrite, byte rewrite or
        // native graph recovery/replay is possible in this closed-game-only route.
        crate::owned_transaction::archive_intent(&file, &path.with_extension("ended"))?;
    }
    Ok(())
}
fn refuse_pending(root: &Path) -> Result<()> {
    ensure!(
        !pending(root)?,
        "An unresolved structural transaction is held. Close the game before more roster edits."
    );
    Ok(())
}

pub(super) fn execute_inside(ctx: &mut Transaction, spec: &Value) -> Result<Value> {
    let selector = Selector::parse(spec)?;
    let access = structural_access(ctx)?;
    let root = ctx.root().to_path_buf();
    let dir = directory(&root)?;
    refuse_pending(&root)?;
    let cell = RefCell::new(ctx);
    let mut live = Live {
        access,
        transaction: cell,
        reverse: false,
    };
    let before = capture(&live, &live.access, None)?;
    let plan = plan(&before, &selector)?;
    let nonce = digest(
        format!(
            "{}:{}:{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            before.layout.owner
        )
        .as_bytes(),
    );
    let meta = {
        let tx = live.transaction.borrow();
        IntentMeta {
            schema: "nba2k27-season-structure-v1".into(),
            game: tx.game().clone(),
            parent: tx.parent().clone(),
            session: tx.session().into(),
            transaction: nonce.clone(),
            selector,
            layout: before.layout.clone(),
            scope: before.scope.clone(),
        }
    };
    let raw = encode(&meta, &before)?;
    let file_path = dir.join(format!("{nonce}.pending"));
    let mut intent = ordinary_new(&file_path)?;
    intent.write_all(&raw)?;
    intent.sync_all()?;
    use std::io::{Seek, SeekFrom};
    intent.seek(SeekFrom::Start(0))?;
    let mut check = Vec::new();
    intent.read_to_end(&mut check)?;
    ensure!(check == raw, "Structural durable intent readback failed");
    live.transaction
        .borrow_mut()
        .retain_intent(&intent, &digest(&raw))?;
    let mut progress = ordinary_new(&file_path.with_extension("progress"))?;
    live.transaction.borrow_mut().freeze()?;
    let held = QueryHandle(unsafe { OpenProcess(WRITE_RIGHTS, false, meta.game.pid)? });
    handle_matches(held.0, &meta.game)?;
    let outcome = (|| -> Result<()> {
        let fresh = capture(
            &live,
            &live.access,
            Some(&|| {
                live.transaction
                    .borrow_mut()
                    .checkpoint_frozen(live.reverse)
            }),
        )?;
        ensure_known(&fresh, &before, plan.before())?;
        for (i, step) in plan.steps().iter().enumerate() {
            append_progress(&mut progress, &json!({"event":"effect-intent","ordinal":i}))?;
            write_step(&live, held.0, &before, step, false)?;
            let fresh = capture(
                &live,
                &live.access,
                Some(&|| {
                    live.transaction
                        .borrow_mut()
                        .checkpoint_frozen(live.reverse)
                }),
            )?;
            ensure_known(
                &fresh,
                &before,
                &plan
                    .expected_prefix(i + 1)
                    .context("Missing expected structural prefix")?,
            )?;
            append_progress(
                &mut progress,
                &json!({"event":"effect-applied","ordinal":i}),
            )?;
        }
        Ok(())
    })();
    let mut status = "after";
    if let Err(original) = outcome {
        live.reverse = true;
        let rollback = (|| -> Result<()> {
            let observed = capture(
                &live,
                &live.access,
                Some(&|| {
                    live.transaction
                        .borrow_mut()
                        .checkpoint_frozen(live.reverse)
                }),
            )?;
            ensure!(
                observed.layout == before.layout
                    && observed.owner == before.owner
                    && observed.teams == before.teams
                    && observed.scope == before.scope,
                "Structural dependencies changed; reverse refused"
            );
            let completed = match plan.inspect(&observed.graph) {
                graph::ModelState::Before => 0,
                graph::ModelState::After => plan.steps().len(),
                graph::ModelState::KnownPaused { completed_steps } => completed_steps,
                graph::ModelState::Unknown => {
                    anyhow::bail!("Partial or foreign graph is held; dependent rollback refused")
                }
            };
            for i in (0..completed).rev() {
                append_progress(
                    &mut progress,
                    &json!({"event":"reverse-intent","ordinal":i}),
                )?;
                write_step(&live, held.0, &before, &plan.steps()[i], true)?;
                let observed = capture(
                    &live,
                    &live.access,
                    Some(&|| {
                        live.transaction
                            .borrow_mut()
                            .checkpoint_frozen(live.reverse)
                    }),
                )?;
                ensure_known(
                    &observed,
                    &before,
                    &plan.expected_prefix(i).context("Missing reverse prefix")?,
                )?;
                append_progress(
                    &mut progress,
                    &json!({"event":"reverse-applied","ordinal":i}),
                )?;
            }
            Ok(())
        })();
        if let Err(reverse) = rollback {
            return Err(anyhow::anyhow!(
                "{original:#}; structural cleanup held: {reverse:#}"
            ));
        }
        status = "before";
    }
    let expected = if status == "after" {
        plan.after()
    } else {
        plan.before()
    };
    let final_read = capture(
        &live,
        &live.access,
        Some(&|| {
            live.transaction
                .borrow_mut()
                .checkpoint_frozen(live.reverse)
        }),
    )?;
    ensure_known(&final_read, &before, expected)?;
    final_read
        .graph
        .validate()
        .map_err(|e| anyhow::anyhow!("Final structural model invalid: {e:?}"))?;
    append_progress(
        &mut progress,
        &json!({"event":"consistent","status":status}),
    )?;
    live.transaction.borrow_mut().mark_consistent()?;
    Ok(
        json!({"transaction":nonce,"status":status,"operation":meta.selector.action,"playerIndex":meta.selector.player_index,"steps":plan.steps().len(),"verified":false,"message":if status=="after"{"Season graph prepared; awaiting independent verification"}else{"Season edit failed and original graph was restored"}}),
    )
}
pub(super) fn verify_inside(ctx: &mut ReadContext, spec: &Value, receipt: &Value) -> Result<()> {
    let raw = ctx.read_intent()?;
    let (meta, before) = decode(&raw)?;
    ensure!(
        meta.game == *ctx.game()
            && meta.parent == *ctx.parent()
            && meta.session == ctx.session()
            && serde_json::to_value(&meta.selector)?
                == serde_json::to_value(Selector::parse(spec)?)?,
        "Structural receipt scope differs"
    );
    if !ctx.effect_attempted() {
        let dir = directory(ctx.root())?;
        let path = dir.join(format!("{}.verified", meta.transaction));
        let mut output = ordinary_new(&path)?;
        let b = serde_json::to_vec(
            &json!({"schema":"nba2k27-season-structure-verified-v1","game":ctx.game(),"session":ctx.session(),"intentSha256":digest(&raw),"status":"no-effect","transaction":meta.transaction}),
        )?;
        output.write_all(&b)?;
        output.sync_all()?;
        return Ok(());
    }
    let plan = plan(&before, &meta.selector)?;
    let access = structural_verify_access(ctx)?;
    ctx.checkpoint_frozen()?;
    let observed = capture(&access, &access, Some(&|| ctx.checkpoint_frozen()))?;
    ctx.checkpoint_frozen()?;
    let status = if ensure_known(&observed, &before, plan.after()).is_ok() {
        "after"
    } else {
        ensure_known(&observed, &before, plan.before())?;
        "before"
    };
    observed
        .graph
        .validate()
        .map_err(|e| anyhow::anyhow!("Independent structural graph invalid: {e:?}"))?;
    if !receipt.is_null() {
        ensure!(
            receipt["transaction"] == meta.transaction && receipt["status"] == status,
            "Structural completion disagrees with independent graph"
        );
    }
    let dir = directory(ctx.root())?;
    let path = dir.join(format!("{}.verified", meta.transaction));
    let mut output = ordinary_new(&path)?;
    let record = json!({"schema":"nba2k27-season-structure-verified-v1","game":ctx.game(),"session":ctx.session(),"intentSha256":digest(&raw),"status":status,"transaction":meta.transaction});
    let b = serde_json::to_vec(&record)?;
    output.write_all(&b)?;
    output.sync_all()?;
    ctx.checkpoint_frozen()?;
    Ok(())
}

/// Ordinary browser request adapter. All identity/token fields originate in the
/// current captured tab. The child selector never contains browser addresses.
fn verified_completion(mut receipt: Value) -> Result<Value> {
    match receipt["status"].as_str() {
        Some("after") => {
            receipt["saved"] = json!(true);
            receipt["changed"] = json!(true);
            receipt["verified"] = json!(true);
            receipt["message"] = json!(
                "Season history saved and independently verified. Save the roster in-game to retain it."
            );
            Ok(receipt)
        }
        Some("before") => anyhow::bail!(
            "The season edit failed. The original history was restored and independently verified; no imported changes were retained."
        ),
        _ => anyhow::bail!(
            "The season transaction returned an unrecognized completion; refresh before continuing"
        ),
    }
}
pub(super) fn editor(capture: &mut NativeCapture, request: &Editor<'_>) -> Result<Value> {
    crate::owned_transaction::available()?;
    if matches!(
        request.input["action"].as_str(),
        Some("import-seasons" | "apply-import")
    ) {
        return import_editor(capture, request);
    }
    let state = capture.begin_service()?;
    let (index, identity) = capture.player_identity(request.input)?;
    let token = request.input["token"]
        .as_str()
        .context("Refresh season history before changing its structure")?;
    let e = capture
        .seasons
        .get(token)
        .filter(|e| e.identity == identity && e.player == index)
        .context("Season review expired; refresh history")?
        .clone();
    let action = match request.input["action"].as_str() {
        Some("add-older") => "addOlder",
        Some("remove-oldest") => "removeOldest",
        Some("copy-shared") => "copyShared",
        _ => anyhow::bail!("Invalid structural season action"),
    };
    ensure!(
        e.slot < graph::REGULAR_SLOTS
            && e.player_row.len() == graph::PLAYER_BYTES
            && e.raw.len() == graph::STAT_BYTES,
        "Incomplete retained season review"
    );
    let at = record_address(&state, &state.players, Kind::Player, index)?;
    ensure!(
        read_record(&capture.access, at, graph::PLAYER_BYTES)? == e.player_row,
        "Player changed after season review"
    );
    ensure!(
        read_record(
            &capture.access,
            e.table + e.index as u64 * graph::STAT_BYTES as u64,
            graph::STAT_BYTES
        )? == e.raw,
        "Season row changed after review"
    );
    let selector = Selector {
        action: action.into(),
        player_index: index,
        expected_player_hash: digest(&e.player_row),
        expected_season_hash: digest(&e.raw),
        expected_slot: e.slot,
        expected_row_index: e.index,
        slot: (action == "copyShared").then_some(e.slot),
        changes: (action == "copyShared").then(|| request.input["changes"].clone()),
        import: None,
    };
    selector.operation()?;
    // The old browser confirm text does not authorize emergency close. Always
    // show this trusted native confirmation, even if confirmed:true was sent.
    if !(request.confirm)(
        "Change this player's season history? Stay at the Home menu and use a backup roster. The game will pause briefly. If an interrupted edit cannot be verified or safely restored, the launcher will close this exact game without saving to protect the roster. Continue?",
    ) {
        return Ok(json!({"canceled":true}));
    }
    capture.access.finish()?;
    let b = &capture.access.binding;
    let result = crate::owned_transaction::execute_guarded(
        request
            .write_gate
            .context("Structural editor requires the existing global write guard")?,
        request.root,
        &b.game,
        &b.parent,
        &b.session,
        crate::owned_transaction::Operation::Season(serde_json::to_value(selector)?),
        true,
    );
    // Any attempted transaction invalidates all old graph-dependent edit tokens;
    // the outer service must recapture before reporting a fresh view.
    capture.seasons.clear();
    capture.prepared_seasons.clear();
    capture.stale = true;
    result.and_then(verified_completion)
}
pub(super) fn available(access: &Access) -> bool {
    crate::owned_transaction::available().is_ok() && proof(access).is_ok() && scope(access).is_ok()
}

#[cfg(test)]
#[path = "season_structure_v2_native_tests.rs"]
mod tests;
// Imported values are never native addresses. Resolve display labels only when
// unique in the current captured team directory; explicit index mappings remain
// bounded to that same directory and its full immutable digest.
#[derive(Debug)]
struct RequiredMapping {
    year: u16,
    role: &'static str,
    name: String,
}
impl std::fmt::Display for RequiredMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Choose the destination {} team for {}",
            self.role, self.year
        )
    }
}
impl std::error::Error for RequiredMapping {}
fn import_choices(input: &Value, teams: &Value) -> Result<Vec<graph::bulk::Choice>> {
    let source = crate::player_dna::seasons(&input["seasons"])?;
    let source = source.as_array().context("Choose imported season rows")?;
    ensure!(
        (1..=25).contains(&source.len()),
        "Choose one to25 imported seasons"
    );
    let teams = teams
        .as_array()
        .context("Current destination teams unavailable")?;
    ensure!(
        (1..=1024).contains(&teams.len()),
        "Invalid destination teams"
    );
    let resolve = |year: u16,
                   role: &'static str,
                   label: &Value,
                   mapping: Option<&Value>|
     -> Result<Option<usize>> {
        if label.is_null() && mapping.is_none_or(Value::is_null) {
            return Ok(None);
        }
        let index = if let Some(v) = mapping.filter(|v| !v.is_null()) {
            usize::try_from(v.as_u64().context("Choose a destination team index")?)?
        } else {
            let label = label.as_str().context("Destination team name missing")?;
            let mut matching = teams.iter().filter(|t| t["name"].as_str() == Some(label));
            let first = matching.next();
            if first.is_none() || matching.next().is_some() {
                return Err(anyhow::Error::new(RequiredMapping {
                    year,
                    role,
                    name: label.into(),
                }));
            }
            let t = first.unwrap();
            usize::try_from(t["index"].as_u64().context("Invalid destination team")?)?
        };
        let mut matching = teams
            .iter()
            .filter(|t| t["index"].as_u64() == Some(index as u64));
        let team = matching
            .next()
            .context("Destination team no longer exists")?;
        ensure!(
            matching.next().is_none()
                && (input["normalTeamsOnly"] != true || team["normal"] == true),
            "Destination team mapping is ambiguous or filtered"
        );
        Ok(Some(index))
    };
    let mut choices = Vec::with_capacity(source.len());
    for row in source {
        let year = u16::try_from(row["year"].as_u64().context("Imported year missing")?)?;
        let key = year.to_string();
        let mapping = &input["teamMappings"][&key];
        let mut preserve = std::collections::HashSet::new();
        if let Some(fields) = input["preserveTotals"].get(&key) {
            for field in fields.as_array().context("Invalid missing-field list")? {
                let field = field.as_str().context("Invalid missing-field name")?;
                ensure!(
                    services::SEASON_FIELDS.iter().any(|f| f.0 == field) && preserve.insert(field),
                    "Unknown or duplicate missing-field total"
                );
            }
        }
        let mut changes = serde_json::Map::new();
        for (field, value) in row["totals"]
            .as_object()
            .context("Imported totals missing")?
        {
            if !preserve.contains(field.as_str()) {
                changes.insert(field.clone(), value.clone());
            }
        }
        if let Some(awards) = row.get("awards") {
            for (field, value) in awards.as_object().context("Invalid imported awards")? {
                changes.insert(format!("award:{field}"), value.clone());
            }
        }
        choices.push(graph::bulk::Choice {
            year,
            primary_team_index: resolve(
                year,
                "primary",
                &row["team"],
                mapping.get("primaryTeamIndex"),
            )?
            .context("Destination primary team missing")?,
            secondary_team_index: resolve(
                year,
                "secondary",
                &row["secondaryTeam"],
                mapping.get("secondaryTeamIndex"),
            )?,
            changes: Value::Object(changes),
        });
    }
    Ok(choices)
}
#[derive(Clone)]
pub(super) struct Review {
    id: String,
    selector: Selector,
    identity: Value,
    selection: Value,
    public: Value,
}
fn import_selection(input: &Value) -> Value {
    json!({"seasons":input["seasons"],"teamMappings":input["teamMappings"],"preserveTotals":input["preserveTotals"],"normalTeamsOnly":input["normalTeamsOnly"]==true})
}
fn destination(tab: &mut NativeCapture, input: &Value) -> Result<(Captured, usize, Value, Value)> {
    let state = tab.begin_service()?;
    let (index, identity) = tab.player_identity(input)?;
    let before = capture(&tab.access, &tab.access, None)?;
    ensure!(
        before.layout.owner == state.owner
            && before.layout.players == state.players.pointer
            && before.layout.player_count == state.players.count
            && before.layout.teams == state.teams.pointer
            && before.layout.team_count == state.teams.count,
        "Destination roster changed before import"
    );
    ensure!(
        before.graph.players.get(index).map(|r| r.as_slice())
            == Some(tab.snapshot.row(Kind::Player, index)?),
        "Destination player changed since capture"
    );
    for (i, row) in before.teams.chunks_exact(profile::TEAM_STRIDE).enumerate() {
        ensure!(
            row == tab.snapshot.row(Kind::Team, i)?,
            "Destination teams changed; refresh before importing"
        );
    }
    let teams = tab.season_import_teams()?;
    Ok((before, index, identity, teams))
}
fn find_year(
    g: &graph::Snapshot,
    player: usize,
    year: u16,
) -> Option<(usize, &[u8; graph::STAT_BYTES])> {
    (0..graph::REGULAR_SLOTS)
        .filter_map(|slot| {
            let at = 0x130 + slot * 2;
            let row =
                u16::from_le_bytes(g.players[player][at..at + 2].try_into().unwrap()) as usize;
            g.rows
                .get(row)
                .filter(|r| r[17] as u16 + 1900 == year)
                .map(|r| (row, r))
        })
        .next()
}
fn revoke_import_review(
    review: &mut Option<Review>,
    prior: Option<&Value>,
    identity: &Value,
) -> Result<()> {
    if let Some(id) = prior {
        ensure!(
            id.as_str().is_some_and(|id| review
                .as_ref()
                .is_some_and(|r| r.id == id && r.identity == *identity)),
            "Import review expired; review again"
        );
    }
    *review = None;
    Ok(())
}
pub(super) fn review_import(tab: &mut NativeCapture, input: &Value) -> Result<Value> {
    crate::owned_transaction::available()?;
    let (before, index, identity, teams) = destination(tab, input)?;
    revoke_import_review(&mut tab.structural_review, input.get("reviewId"), &identity)?;
    let choices = match import_choices(input, &teams) {
        Ok(c) => c,
        Err(e) => {
            if let Some(required) = e.downcast_ref::<RequiredMapping>() {
                return Ok(
                    json!({"ready":false,"mappingRequired":{"year":required.year,"role":required.role,"sourceName":required.name},"teams":teams}),
                );
            }
            return Err(e);
        }
    };
    let selector = Selector {
        action: "importSeasons".into(),
        player_index: index,
        expected_player_hash: digest(&before.graph.players[index]),
        expected_season_hash: history_digest(&before.graph),
        expected_slot: 0,
        expected_row_index: 0,
        slot: None,
        changes: None,
        import: Some(BulkSelector {
            expected_teams_hash: digest(&before.teams),
            expected_snapshot_hash: snapshot_digest(&before)?,
            choices,
        }),
    };
    let outcome = graph::plan(&before.graph, selector.operation()?)
        .map_err(|e| anyhow::anyhow!("Season import refused: {e:?}"))?;
    let (after, allocated, changed) = match &outcome {
        graph::Outcome::Planned(p) => (
            p.after(),
            p.phases()
                .iter()
                .filter(|&&p| p == graph::Phase::InitializeRow)
                .count(),
            true,
        ),
        graph::Outcome::Unchanged => (&before.graph, 0, false),
    };
    let mut rows = Vec::new();
    for choice in &selector.import.as_ref().unwrap().choices {
        let (new_index, new) =
            find_year(after, index, choice.year).context("Missing planned season")?;
        let old = find_year(&before.graph, index, choice.year);
        let operation = match old {
            None => "create",
            Some((old_index, _)) if old_index != new_index => "copy-shared",
            Some((_, old)) if old != new => "update",
            _ => "unchanged",
        };
        let get_team = |id: usize| {
            teams
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["index"].as_u64() == Some(id as u64))
                .cloned()
                .context("Missing planned team")
        };
        let before_row=old.map(|(_,old)|->Result<Value>{
            let pointer=q(old,0);let label=pointer.checked_sub(before.layout.teams).filter(|offset|offset%profile::TEAM_STRIDE as u64==0).and_then(|offset|teams.as_array().unwrap().iter().find(|t|t["index"].as_u64()==Some(offset/profile::TEAM_STRIDE as u64))).map(|t|t["displayName"].as_str().or_else(||t["name"].as_str())).flatten();
            Ok(json!({"team":label,"totals":services::decode_totals(old)?,"awards":services::decode_awards(old)?}))
        }).transpose()?;
        rows.push(json!({"year":choice.year,"primaryTeam":get_team(choice.primary_team_index)?,"secondaryTeam":choice.secondary_team_index.map(get_team).transpose()?,"operation":operation,"before":before_row,"totals":services::decode_totals(new)?,"awards":services::decode_awards(new)?}));
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = digest(
        format!(
            "{}:{}:{}:{}",
            selector.expected_player_hash,
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
        .as_bytes(),
    );
    let public = json!({"ready":true,"reviewId":id,"rows":rows,"warnings":[],"changed":changed,"allocated":allocated});
    tab.access.finish()?;
    tab.structural_review = Some(Review {
        id,
        selector,
        identity,
        selection: import_selection(input),
        public: public.clone(),
    });
    Ok(public)
}
fn retained_import(review: &Option<Review>, input: &Value, identity: &Value) -> Result<Review> {
    let r = review
        .as_ref()
        .filter(|r| input["reviewId"].as_str() == Some(r.id.as_str()) && *identity == r.identity)
        .context("Season import review expired; review again")?;
    if input.get("seasons").is_some() {
        ensure!(
            import_selection(input) == r.selection,
            "Imported selection changed; review again"
        );
    }
    Ok(r.clone())
}
fn import_editor(tab: &mut NativeCapture, request: &Editor<'_>) -> Result<Value> {
    tab.begin_service()?;
    let (_, identity) = tab.player_identity(request.input)?;
    let review = retained_import(&tab.structural_review, request.input, &identity)?;
    if review.public["changed"] == false {
        return Ok(json!({"saved":false,"changed":false}));
    }
    let count = review
        .selector
        .import
        .as_ref()
        .context("Incomplete import review")?
        .choices
        .len();
    let text = format!(
        "Import {count} reviewed seasons into this player? Existing unselected years will be preserved. Shared changed seasons use private copies. Stay at Home and use a backup roster. The game will pause briefly. If an interrupted edit cannot be verified or safely restored, the launcher will close this exact game without saving to protect the roster. Continue?"
    );
    if !(request.confirm)(&text) {
        return Ok(json!({"canceled":true}));
    }
    tab.access.finish()?;
    let b = &tab.access.binding;
    let result = crate::owned_transaction::execute_guarded(
        request
            .write_gate
            .context("Season import requires the existing global write guard")?,
        request.root,
        &b.game,
        &b.parent,
        &b.session,
        crate::owned_transaction::Operation::Season(serde_json::to_value(review.selector)?),
        true,
    );
    tab.structural_review = None;
    tab.seasons.clear();
    tab.prepared_seasons.clear();
    tab.stale = true;
    result.and_then(verified_completion)
}

pub(super) fn capabilities(access: &Access) -> Value {
    if available(access) {
        json!({"existingRowsOnly":false,"nonsharedOnly":false,"structure":true,"bulkApply":true,"maxStagedRows":25})
    } else {
        crate::season_import_review::capabilities()
    }
}
