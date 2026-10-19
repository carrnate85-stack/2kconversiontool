//! Helper-only reserved arena authority. A child of roster_v2_access so native
//! bindings, read handles and region proofs never become browser capabilities.
use super::*;
use crate::{
    owned_transaction::{Context as Frozen, Operation, ReadContext},
    roster_v2_arena as arena, roster_v2_arena_clone as clone,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
};
const IMMUTABLE_MAX: usize = 256 * 1024;
const MOD_MAX: u64 = 512 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selector {
    edit: clone::Edit,
    expected_team_hash: String,
    mods_root: PathBuf,
}
impl Selector {
    fn parse(operation: &Operation) -> Result<Self> {
        let Operation::ArenaClone(value) = operation else {
            anyhow::bail!("Not an arena clone operation")
        };
        let s: Self = serde_json::from_value(value.clone())?;
        clone::Edit::parse(&serde_json::to_value(&s.edit)?)?;
        ensure!(
            s.mods_root.is_absolute()
                && s.expected_team_hash.len() == 64
                && s.expected_team_hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid native arena review"
        );
        Ok(s)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfileSelector {
    profile: crate::custom_arena_profiles::Profile,
    mods_root: PathBuf,
}
fn select(
    access: &Access,
    state: &profile::OwnerSnapshot,
    catalog: &str,
    operation: &Operation,
) -> Result<(Selector, clone::Mode)> {
    match operation {
        Operation::ArenaClone(_) => Ok((Selector::parse(operation)?, clone::Mode::CloneAndLink)),
        Operation::ArenaProfile(value) => {
            let request: ProfileSelector = serde_json::from_value(value.clone())?;
            request.profile.validate()?;
            ensure!(
                request.profile.enabled && request.mods_root.is_absolute(),
                "Saved arena application is not enabled or Mods path is invalid"
            );
            let rows = read_directory(access, &state.teams)?;
            let matches = rows
                .chunks_exact(profile::TEAM_STRIDE)
                .enumerate()
                .filter(|(_, r)| {
                    u16::from_le_bytes(r[writer::TEAM_ID..writer::TEAM_ID + 2].try_into().unwrap())
                        == request.profile.team_id
                })
                .collect::<Vec<_>>();
            ensure!(
                matches.len() == 1,
                "Saved arena team ID is absent or ambiguous"
            );
            let (index, team) = matches[0];
            let raw = &team[0x2fa..0x2fa + 48];
            let words = raw
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>();
            let n = words
                .iter()
                .position(|v| *v == 0)
                .context("Saved arena team name is unterminated")?;
            ensure!(
                String::from_utf16(&words[..n])? == request.profile.expected_team_name,
                "Saved arena team name changed"
            );
            let p = request.profile;
            let edit = clone::Edit {
                record_index: index,
                team_id: p.team_id,
                expected_team_name: p.expected_team_name,
                donor_arena_id: p.donor_arena_id,
                target_arena_id: p.target_arena_id,
                arena_name: p.arena_name,
                arena_file: p.arena_file,
                floor_file: p.floor_file,
                catalog_id: catalog.into(),
            };
            Ok((
                Selector {
                    edit,
                    expected_team_hash: clone::hash(team),
                    mods_root: request.mods_root,
                },
                clone::Mode::PopulateOnly,
            ))
        }
        _ => anyhow::bail!("Not an arena operation"),
    }
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModProof {
    name: String,
    id: crate::file_identity::Stable,
    len: u64,
    sha256: String,
}
struct PinnedMod {
    proof: ModProof,
    file: File,
    _dirs: crate::loader_files::Directories,
}
impl PinnedMod {
    fn open(root: &Path, stem: &str) -> Result<Self> {
        clone::stem(stem, 39)?;
        let levels = root.join("levels");
        let dirs = crate::loader_files::Directories::pin(&levels)?;
        let name = format!("levels/{stem}.iff");
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(root.join(&name))
            .with_context(|| format!("Place {name} in the selected Mods folder before cloning"))?;
        let meta = file.metadata()?;
        ensure!(
            meta.is_file()
                && meta.file_attributes() & 0x400 == 0
                && (1..=MOD_MAX).contains(&meta.len()),
            "Arena mod must be a nonempty bounded ordinary .iff file"
        );
        let id = crate::file_identity::Stable::read(&file)?;
        let mut h = sha2::Sha256::new();
        let mut n = 0u64;
        let mut buf = [0u8; 65536];
        loop {
            let got = file.read(&mut buf)?;
            if got == 0 {
                break;
            }
            n += got as u64;
            ensure!(n <= MOD_MAX, "Arena mod exceeds bounds");
            h.update(&buf[..got]);
        }
        ensure!(
            n == meta.len()
                && file.metadata()?.len() == n
                && crate::file_identity::Stable::read(&file)? == id,
            "Arena mod changed while reading"
        );
        Ok(Self {
            proof: ModProof {
                name,
                id,
                len: n,
                sha256: format!("{:x}", h.finalize()),
            },
            file,
            _dirs: dirs,
        })
    }
    fn check(&self) -> Result<()> {
        let m = self.file.metadata()?;
        ensure!(
            m.is_file()
                && m.file_attributes() & 0x400 == 0
                && m.len() == self.proof.len
                && crate::file_identity::Stable::read(&self.file)? == self.proof.id,
            "Pinned arena mod changed"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Immutable {
    operation_hash: String,
    mode: clone::Mode,
    prepared_intent: clone::Intent,
    schema: String,
    session: String,
    game: Identity,
    edit: clone::Edit,
    expected_team_hash: String,
    mods_root: PathBuf,
    mods: Vec<ModProof>,
    owner: clone::Owner,
    before: clone::Raw,
    after: clone::Raw,
}
fn catalog_hash(state: &profile::OwnerSnapshot, rows: &[u8]) -> String {
    catalog_hash_parts(
        [
            state.root,
            state.owner,
            state.region.base,
            state.region.len,
            state.arenas.pointer,
            state.arenas.count as u64,
        ],
        rows,
    )
}
fn catalog_hash_parts(parts: [u64; 6], rows: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    for n in parts {
        h.update(n.to_le_bytes());
    }
    h.update(rows);
    format!("{:x}", h.finalize())
}
fn reconstruct(i: &Immutable, mut rows: Vec<u8>, mut team_links: Vec<u64>) -> Result<clone::Plan> {
    i.owner.validate()?;
    ensure!(
        rows.len() == i.owner.arena_count * clone::STRIDE
            && team_links.len() == i.owner.team.count
            && i.edit.record_index == i.owner.team.index,
        "Incomplete independent clone dependencies"
    );
    ensure!(
        clone::hash(&i.before.team) == i.expected_team_hash,
        "Retained full team review differs"
    );
    ensure!(
        i.before.arena.len() == clone::STRIDE && i.after.arena.len() == clone::STRIDE,
        "Incomplete retained arena image"
    );
    let range = i.owner.target * clone::STRIDE..(i.owner.target + 1) * clone::STRIDE;
    ensure!(
        rows[range.clone()] == i.before.arena || rows[range.clone()] == i.after.arena,
        "Independent arena is partial or foreign"
    );
    rows[range].copy_from_slice(&i.before.arena);
    ensure!(
        catalog_hash_parts(
            [
                i.owner.team.root,
                i.owner.team.owner,
                i.owner.team.region_base,
                i.owner.team.region_len,
                i.owner.arena_table,
                i.owner.arena_count as u64
            ],
            &rows
        ) == i.edit.catalog_id,
        "Independent original arena catalog differs"
    );
    team_links[i.edit.record_index] = u64::from_le_bytes(
        i.before
            .team
            .get(clone::LINK..clone::LINK + 8)
            .context("Incomplete retained team link")?
            .try_into()?,
    );
    let plan = match i.mode {
        clone::Mode::CloneAndLink => {
            clone::plan(i.owner.clone(), &i.edit, &i.before.team, &rows, &team_links)?
        }
        clone::Mode::PopulateOnly => {
            clone::plan_profile(i.owner.clone(), &i.edit, &i.before.team, &rows, &team_links)?
        }
    };
    clone::validate_prepared(&i.prepared_intent, &plan)?;
    ensure!(
        plan.before == i.before && plan.after == i.after,
        "Retained intent is not the native derived plan"
    );
    Ok(plan)
}
fn row_id(row: &[u8]) -> Result<u16> {
    Ok(u16::from_le_bytes(
        row.get(clone::ID..clone::ID + 2)
            .context("Incomplete arena ID")?
            .try_into()?,
    ))
}
fn links(access: &Access, state: &profile::OwnerSnapshot) -> Result<Vec<u64>> {
    ensure!(
        state.teams.stride == profile::TEAM_STRIDE && (1..=1024).contains(&state.teams.count),
        "Invalid native team directory"
    );
    (0..state.teams.count)
        .map(|i| {
            let at = record_address(state, &state.teams, Kind::Team, i)? + clone::LINK as u64;
            Ok(u64::from_le_bytes(
                read_record(access, at, 8)?
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Incomplete team arena pointer"))?,
            ))
        })
        .collect()
}
fn writable(access: &Access, state: &profile::OwnerSnapshot, at: u64, len: usize) -> Result<()> {
    ensure!(
        len > 0
            && at >= state.region.base
            && at
                .checked_add(len as u64)
                .is_some_and(|e| e <= state.region.base + state.region.len),
        "Clone destination escaped native allocation"
    );
    let end = at + len as u64;
    let mut p = at;
    while p < end {
        let r = access.query(p)?;
        ensure!(
            writable_region(r, state.region.base) && contains(r.base, r.len, p, 1),
            "Clone destination is not exact private PAGE_READWRITE data"
        );
        let next = (r.base + r.len).min(end);
        ensure!(next > p, "Clone region made no progress");
        p = next;
    }
    Ok(())
}
const OWNER_HEADER: usize = 0x3d0;
/// Current independently proven readonly pool extents. High DWORDs beside
/// DWORD counts are opaque and retained in the entire owner-header fingerprint.
fn protected_extents(header: &[u8]) -> Result<Vec<(u64, usize)>> {
    ensure!(
        header.len() == OWNER_HEADER,
        "Incomplete protected roster header"
    );
    let mut extents = Vec::new();
    for (offset, stride, min, max, is32, optional) in [
        (0x10, profile::PLAYER_STRIDE, 100, 8192, false, false),
        (0x60, clone::STRIDE, 1, 2048, false, false),
        (0x80, profile::TEAM_STRIDE, 1, 1024, false, false),
        (0x290, 0x170, 1, 8192, false, true),
        (0x180, 0x1c8, 1, 8192, true, false),
        (0x3c0, 0x68, 1, 65536, true, false),
        (0x1c0, 56, 1, 1, true, false),
        (0x1d0, 64, 1, 32767, true, false),
        // Existing readonly origin/name services prove these optional tables.
        (0x90, 72, 1, 4096, true, true),
        (0x240, 48, 1, 20000, true, true),
        (0x250, 48, 1, 20000, true, true),
    ] {
        let count = if is32 {
            u32::from_le_bytes(header[offset..offset + 4].try_into()?) as usize
        } else {
            usize::try_from(u64::from_le_bytes(header[offset..offset + 8].try_into()?))?
        };
        let at = u64::from_le_bytes(header[offset + 8..offset + 16].try_into()?);
        if optional && count == 0 && at == 0 {
            continue;
        }
        ensure!(
            (min..=max).contains(&count) && at % 8 == 0,
            "Protected roster pool count/alignment differs"
        );
        extents.push((
            at,
            count
                .checked_mul(stride)
                .context("Protected pool extent overflow")?,
        ));
    }
    Ok(extents)
}
fn protected_proof(access: &Access) -> Result<()> {
    services::verify_profile(access)?;
    for (rva, n, hash) in [
        // Existing readonly origin and name services prove the optional pools.
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
        (
            0x2eddb00,
            180,
            "ab970e1b3ac8eedaacf380421b299f34e79cecf3d36b0718dd8ca5b19c461f49",
        ),
        (
            0x2ca82d0,
            1775,
            "a38c4a0d4fda75ddb9e26ab36973c2a1a58c03221a82f1f2e3b75d3f9f04cf08",
        ),
        (
            0x2e58120,
            180,
            "3151b876511aeded1369b806f3fab51e396f0b21b50b4f8bdae6836497b07b11",
        ),
    ] {
        ensure!(
            clone::hash(&read_record(access, access.base() + rva, n)?) == hash,
            "Protected roster pool native methods differ"
        );
    }
    Ok(())
}
fn readable_allocation(
    access: &Access,
    state: &profile::OwnerSnapshot,
    at: u64,
    len: usize,
) -> Result<()> {
    ensure!(
        len > 0
            && at >= state.region.base
            && at
                .checked_add(len as u64)
                .is_some_and(|e| e <= state.region.base + state.region.len),
        "Protected pool escaped native allocation"
    );
    let mut cursor = at;
    let end = at + len as u64;
    while cursor < end {
        let r = access.query(cursor)?;
        ensure!(
            private(r) && r.allocation == state.region.base && contains(r.base, r.len, cursor, 1),
            "Protected pool no longer belongs to readable private roster data"
        );
        let next = r
            .base
            .checked_add(r.len)
            .context("Protected pool region overflow")?
            .min(end);
        ensure!(next > cursor, "Protected pool query made no progress");
        cursor = next;
    }
    Ok(())
}
fn verify_protected_header(header: &[u8], expected: &str) -> Result<()> {
    ensure!(
        header.len() == OWNER_HEADER && clone::hash(header) == expected,
        "Protected roster owner header changed"
    );
    Ok(())
}
fn protected_header(
    access: &Access,
    state: &profile::OwnerSnapshot,
    expected: Option<&str>,
) -> Result<Vec<u8>> {
    protected_proof(access)?;
    profile::revalidate(access, state)?;
    readable_allocation(access, state, state.owner, OWNER_HEADER)?;
    let header = read_record(access, state.owner, OWNER_HEADER)?;
    if let Some(hash) = expected {
        verify_protected_header(&header, hash)?;
    }
    // The four owner-directory entries must still describe the native capture.
    for (offset, d) in [
        (0x10, &state.players),
        (0x60, &state.arenas),
        (0x80, &state.teams),
    ] {
        ensure!(
            u64::from_le_bytes(header[offset..offset + 8].try_into()?) == d.count as u64
                && u64::from_le_bytes(header[offset + 8..offset + 16].try_into()?) == d.pointer,
            "Protected primary directory changed"
        );
    }
    let extents = protected_extents(&header)?;
    super::structural::directory_extents_disjoint(
        state.region.base,
        state.region.len,
        state.owner,
        state.root,
        &extents,
    )?;
    for (at, n) in extents {
        readable_allocation(access, state, at, n)?;
    }
    protected_proof(access)?;
    profile::revalidate(access, state)?;
    verify_protected_header(
        &read_record(access, state.owner, OWNER_HEADER)?,
        &clone::hash(&header),
    )?;
    Ok(header)
}
fn state(access: &Access) -> Result<profile::OwnerSnapshot> {
    let region = access.region()?;
    let state = profile::inspect_owner(access, region)?;
    protected_header(access, &state, None)?;
    arena_allocation(access, &state)?;
    arena::verify_profile(access)?;
    profile::revalidate(access, &state)?;
    Ok(state)
}
fn owner(
    access: &Access,
    state: &profile::OwnerSnapshot,
    index: usize,
    donor: usize,
    target: usize,
    catalog: &str,
) -> Result<clone::Owner> {
    let protected = protected_header(access, state, None)?;
    let protected_hash = clone::hash(&protected);
    profile::revalidate(access, state)?;
    arena_allocation(access, state)?;
    arena::verify_profile(access)?;
    ensure!(
        donor < state.arenas.count && target < state.arenas.count,
        "Arena index escaped native directory"
    );
    let team = read_record(
        access,
        record_address(state, &state.teams, Kind::Team, index)?,
        profile::TEAM_STRIDE,
    )?;
    let donor_raw = read_record(
        access,
        state.arenas.pointer + (donor * clone::STRIDE) as u64,
        clone::STRIDE,
    )?;
    let mut ids = Vec::with_capacity(state.arenas.count * 2);
    for i in 0..state.arenas.count {
        ids.extend(read_record(
            access,
            state.arenas.pointer + (i * clone::STRIDE + clone::ID) as u64,
            2,
        )?);
    }
    let target_id = u16::from_le_bytes(ids[target * 2..target * 2 + 2].try_into()?);
    let team_links = links(access, state)?;
    profile::revalidate(access, state)?;
    arena::verify_profile(access)?;
    arena_allocation(access, state)?;
    protected_header(access, state, Some(&protected_hash))?;
    let o = clone::Owner {
        protected_header_hash: protected_hash,
        team: writer::Owner {
            game: access.binding.game.clone(),
            session: access.binding.session.clone(),
            image: access.base(),
            root: state.root,
            owner: state.owner,
            region_base: state.region.base,
            region_len: state.region.len,
            kind: Kind::Team,
            table: state.teams.pointer,
            count: state.teams.count,
            index,
            identity: writer::identity(Kind::Team, &team)?,
            proof: writer::proof_tag().into(),
        },
        arena_table: state.arenas.pointer,
        arena_count: state.arenas.count,
        donor,
        target,
        target_id,
        donor_hash: clone::hash(&donor_raw),
        ids_hash: clone::hash(&ids),
        other_links_hash: clone::links_hash(&team_links, index),
        catalog_id: catalog.into(),
    };
    o.validate()?;
    Ok(o)
}
fn raw(access: &Access, state: &profile::OwnerSnapshot, o: &clone::Owner) -> Result<clone::Raw> {
    Ok(clone::Raw {
        team: read_record(
            access,
            record_address(state, &state.teams, Kind::Team, o.team.index)?,
            profile::TEAM_STRIDE,
        )?,
        arena: read_record(access, o.target_address(), clone::STRIDE)?,
    })
}
fn indices(rows: &[u8], edit: &clone::Edit) -> Result<(usize, usize)> {
    let find = |id| -> Result<usize> {
        let matches = rows
            .chunks_exact(clone::STRIDE)
            .enumerate()
            .filter_map(|(i, r)| (row_id(r).ok() == Some(id)).then_some(i))
            .collect::<Vec<_>>();
        ensure!(matches.len() == 1, "Arena ID is missing or ambiguous");
        Ok(matches[0])
    };
    Ok((find(edit.donor_arena_id)?, find(edit.target_arena_id)?))
}
fn entire_catalog(
    access: &Access,
    state: &profile::OwnerSnapshot,
    o: &clone::Owner,
    before: &clone::Raw,
    after: &clone::Raw,
) -> Result<()> {
    protected_header(access, state, Some(&o.protected_header_hash))?;
    profile::revalidate(access, state)?;
    arena_allocation(access, state)?;
    let mut rows = read_directory(access, &state.arenas)?;
    let range = o.target * clone::STRIDE..(o.target + 1) * clone::STRIDE;
    ensure!(
        rows[range.clone()] == before.arena || rows[range.clone()] == after.arena,
        "Reserved arena is partial or foreign"
    );
    rows[range].copy_from_slice(&before.arena);
    ensure!(
        catalog_hash(state, &rows) == o.catalog_id,
        "Unowned arena catalog bytes changed"
    );
    protected_header(access, state, Some(&o.protected_header_hash))?;
    profile::revalidate(access, state)?;
    arena::verify_profile(access)?;
    Ok(())
}
fn immutable(root: &Path, value: &Immutable) -> Result<(File, crate::loader_files::Directories)> {
    let data = root.join("Data");
    fs::create_dir_all(&data)?;
    let _data = crate::loader_files::Directories::pin(&data)?;
    let dir = data.join("Owned Arena Transactions");
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    let dirs = crate::loader_files::Directories::pin(&dir)?;
    let path = dir.join(format!("{}.json", crate::backend::unique_id()));
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= IMMUTABLE_MAX,
        "Arena immutable intent exceeds bounds"
    );
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)?;
    let id = crate::file_identity::Stable::read(&output)?;
    output.write_all(&bytes)?;
    output.sync_all()?;
    drop(output);
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)?;
    ensure!(
        crate::file_identity::Stable::read(&file)? == id,
        "Arena intent identity changed"
    );
    let mut check = Vec::new();
    (&mut file)
        .take(IMMUTABLE_MAX as u64 + 1)
        .read_to_end(&mut check)?;
    ensure!(check == bytes, "Arena intent readback differs");
    file.seek(SeekFrom::Start(0))?;
    Ok((file, dirs))
}
/// Closed-plan effect admission. No caller-selected address or byte mask.
fn effect(
    plan: &clone::Plan,
    area: clone::Area,
    offset: usize,
    bytes: &[u8],
    reverse: bool,
) -> Result<(u64, usize)> {
    plan.owner.validate()?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 128,
        "Unbounded clone effect"
    );
    let end = offset
        .checked_add(bytes.len())
        .context("Clone offset overflow")?;
    let base = match area {
        clone::Area::Team => {
            ensure!(
                plan.mode == clone::Mode::CloneAndLink && offset == clone::LINK && bytes.len() == 8,
                "Clone may only publish its selected team pointer"
            );
            plan.owner.team.table + (plan.owner.team.index * profile::TEAM_STRIDE) as u64
        }
        clone::Area::Arena => {
            ensure!(
                end <= clone::STRIDE && (end <= clone::ID || offset >= clone::ID + 2),
                "Clone may not write its reserved arena ID"
            );
            plan.owner.target_address()
        }
    };
    let index = plan
        .patches
        .iter()
        .position(|p| {
            p.area == area
                && p.offset == offset
                && if reverse {
                    p.before == bytes
                } else {
                    p.after == bytes
                }
        })
        .context("Effect is absent from admitted clone plan")?;
    Ok((
        base.checked_add(offset as u64)
            .context("Clone address overflow")?,
        if reverse { index + 1 } else { index },
    ))
}
struct Lease<'a> {
    access: &'a Access,
    state: &'a profile::OwnerSnapshot,
    plan: &'a clone::Plan,
    mods: &'a [PinnedMod],
    handle: QueryHandle,
    context: RefCell<&'a mut Frozen>,
    reverse: Cell<bool>,
}
impl clone::Access for Lease<'_> {
    fn owner(&self) -> Result<clone::Owner> {
        self.checkpoint()?;
        let o = owner(
            self.access,
            self.state,
            self.plan.owner.team.index,
            self.plan.owner.donor,
            self.plan.owner.target,
            &self.plan.owner.catalog_id,
        )?;
        self.checkpoint()?;
        Ok(o)
    }
    fn capture(&self) -> Result<clone::Raw> {
        self.checkpoint()?;
        let r = raw(self.access, self.state, &self.plan.owner)?;
        self.checkpoint()?;
        Ok(r)
    }
    fn checkpoint(&self) -> Result<()> {
        self.context
            .borrow_mut()
            .checkpoint_frozen(self.reverse.get())?;
        self.access.checkpoint()?;
        for m in self.mods {
            m.check()?;
        }
        Ok(())
    }
    fn begin_reverse(&self) -> Result<()> {
        self.reverse.set(true);
        self.checkpoint()
    }
    fn write(&self, area: clone::Area, offset: usize, bytes: &[u8]) -> Result<()> {
        let (at, phase) = effect(&self.plan, area, offset, bytes, self.reverse.get())?;
        self.checkpoint()?;
        ensure!(
            self.owner()? == self.plan.owner,
            "Clone owner changed before effect"
        );
        writable(self.access, self.state, at, bytes.len())?;
        handle_matches(self.handle.0, &self.access.binding.game)?;
        ensure!(
            raw(self.access, self.state, &self.plan.owner)? == self.plan.expected(phase)?,
            "Clone effect preimage changed after validation"
        );
        // This call precedes WPM even when it fails or reports partial bytes.
        self.context
            .borrow_mut()
            .before_effect(self.reverse.get())?;
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                self.handle.0,
                at as _,
                bytes.as_ptr() as _,
                bytes.len(),
                Some(&mut done),
            )?;
        }
        ensure!(
            done == bytes.len(),
            "Partial clone effect; frozen verification required"
        );
        self.checkpoint()
    }
}
pub(super) fn execute_inside(context: &mut Frozen, operation: &Operation) -> Result<Value> {
    let root = context.root().to_owned();
    ensure!(
        !writer::any_pending(&root)?,
        "Another roster recovery is pending"
    );
    let mut access = structural_access(context)?;
    let state = state(&access)?;
    let catalog =
        arena::capture(&access, &state)?.context("Native arena catalog is unavailable")?;
    let (selector, mode) = select(&access, &state, catalog.id(), operation)?;
    ensure!(
        catalog.id() == selector.edit.catalog_id,
        "Arena catalog review expired"
    );
    let rows = catalog.raw_rows();
    let (donor, target) = indices(rows, &selector.edit)?;
    let team = read_record(
        &access,
        record_address(&state, &state.teams, Kind::Team, selector.edit.record_index)?,
        profile::TEAM_STRIDE,
    )?;
    ensure!(
        clone::hash(&team) == selector.expected_team_hash,
        "Selected full team changed after review"
    );
    let o = owner(
        &access,
        &state,
        selector.edit.record_index,
        donor,
        target,
        catalog.id(),
    )?;
    let team_links = links(&access, &state)?;
    let plan = match mode {
        clone::Mode::CloneAndLink => clone::plan(o, &selector.edit, &team, rows, &team_links)?,
        clone::Mode::PopulateOnly => {
            clone::plan_profile(o, &selector.edit, &team, rows, &team_links)?
        }
    };
    let mods = [
        PinnedMod::open(&selector.mods_root, &selector.edit.arena_file)?,
        PinnedMod::open(&selector.mods_root, &selector.edit.floor_file)?,
    ];
    let intent = Immutable {
        operation_hash: clone::hash(&serde_json::to_vec(operation)?),
        mode,
        prepared_intent: clone::prepare_intent(&plan)?,
        schema: "nba2k27-v2-arena-clone-immutable-v1".into(),
        session: context.session().into(),
        game: context.game().clone(),
        edit: selector.edit,
        expected_team_hash: selector.expected_team_hash,
        mods_root: selector.mods_root,
        mods: mods
            .iter()
            .map(|m| ModProof {
                name: m.proof.name.clone(),
                id: m.proof.id.clone(),
                len: m.proof.len,
                sha256: m.proof.sha256.clone(),
            })
            .collect(),
        owner: plan.owner.clone(),
        before: plan.before.clone(),
        after: plan.after.clone(),
    };
    let (file, _dirs) = immutable(&root, &intent)?;
    context.retain_intent(&file, &clone::hash(&serde_json::to_vec(&intent)?))?;
    context.freeze()?;
    access.start()?;
    ensure!(
        owner(
            &access,
            &state,
            plan.owner.team.index,
            plan.owner.donor,
            plan.owner.target,
            &plan.owner.catalog_id
        )? == plan.owner,
        "Frozen arena owner differs"
    );
    entire_catalog(&access, &state, &plan.owner, &plan.before, &plan.after)?;
    ensure!(
        raw(&access, &state, &plan.owner)? == plan.before,
        "Frozen clone preimage changed"
    );
    let handle = QueryHandle(unsafe { OpenProcess(WRITE_RIGHTS, false, access.binding.game.pid)? });
    handle_matches(handle.0, &access.binding.game)?;
    let lease = Lease {
        access: &access,
        state: &state,
        plan: &plan,
        mods: &mods,
        handle,
        context: RefCell::new(context),
        reverse: Cell::new(false),
    };
    let result = clone::apply(&lease, &plan, |i| clone::save(&root, i));
    // No further forward work; retained cleanup/full verification is permitted
    // even when forward parent/watchdog approval has disappeared.
    lease.reverse.set(true);
    // Independent full native readback is required even when the core reports an
    // error/rollback. A prefix, status journal or matching final pointer cannot
    // mark the retained freeze consistent.
    let verified = (|| -> Result<()> {
        lease.context.borrow_mut().checkpoint_frozen(true)?;
        ensure!(
            clone::Access::owner(&lease)? == plan.owner,
            "Final clone owner changed"
        );
        let current = raw(&access, &state, &plan.owner)?;
        ensure!(
            current == plan.before || current == plan.after,
            "Clone ended in partial/foreign state"
        );
        entire_catalog(&access, &state, &plan.owner, &plan.before, &plan.after)?;
        access.finish()?;
        lease.context.borrow_mut().mark_consistent()
    })();
    verified?;
    result
}
pub(super) fn verify_inside(
    context: &mut ReadContext,
    operation: &Operation,
    _receipt: &Value,
) -> Result<()> {
    if !context.effect_attempted() {
        if !context.has_intent() {
            return Ok(());
        }
        let bytes = context.read_intent()?;
        ensure!(bytes.len() <= IMMUTABLE_MAX, "Arena intent exceeds bounds");
        let i: Immutable = serde_json::from_slice(&bytes)?;
        ensure!(
            i.operation_hash == clone::hash(&serde_json::to_vec(operation)?)
                && i.schema == "nba2k27-v2-arena-clone-immutable-v1"
                && i.session == context.session()
                && i.game == *context.game(),
            "No-effect arena intent belongs to another session"
        );
        return clone::resolve_no_effect(context.root(), &i.prepared_intent);
    }
    context.checkpoint_frozen()?;
    let bytes = context.read_intent()?;
    ensure!(bytes.len() <= IMMUTABLE_MAX, "Arena intent exceeds bounds");
    let i: Immutable = serde_json::from_slice(&bytes)?;
    let access = structural_verify_access(context)?;
    let state = state(&access)?;
    let (selector, mode) = select(&access, &state, &i.edit.catalog_id, operation)?;
    ensure!(mode == i.mode, "Arena operation mode differs");
    ensure!(
        i.operation_hash == clone::hash(&serde_json::to_vec(operation)?)
            && i.schema == "nba2k27-v2-arena-clone-immutable-v1"
            && i.session == context.session()
            && i.game == *context.game()
            && i.expected_team_hash == selector.expected_team_hash
            && i.mods_root == selector.mods_root
            && serde_json::to_value(&i.edit)? == serde_json::to_value(&selector.edit)?,
        "Retained arena intent belongs to another operation"
    );
    let mods = [
        PinnedMod::open(&i.mods_root, &i.edit.arena_file)?,
        PinnedMod::open(&i.mods_root, &i.edit.floor_file)?,
    ];
    ensure!(
        i.mods.len() == 2 && mods.iter().zip(&i.mods).all(|(m, p)| &m.proof == p),
        "Arena mod identity/content changed"
    );
    let current_owner = owner(
        &access,
        &state,
        i.edit.record_index,
        i.owner.donor,
        i.owner.target,
        &i.owner.catalog_id,
    )?;
    ensure!(current_owner == i.owner, "Native arena provenance changed");
    let current = raw(&access, &state, &i.owner)?;
    ensure!(
        current == i.before || current == i.after,
        "Independent verifier found a partial clone"
    );
    reconstruct(
        &i,
        read_directory(&access, &state.arenas)?,
        links(&access, &state)?,
    )?;
    entire_catalog(&access, &state, &i.owner, &i.before, &i.after)?;
    for m in &mods {
        m.check()?;
    }
    ensure!(
        raw(&access, &state, &i.owner)? == current,
        "Native clone changed during independent verification"
    );
    access.finish()?;
    context.checkpoint_frozen()
}
/// Deliberately one automatic attempt per owned launch/profile. Native headers
/// have no proven monotonic epoch and must not turn a partial effect or ABA into
/// a new retry permit. Roster reloads in the same launch remain withheld.
pub(super) fn profile_admission(
    binding: &Binding,
    profile: &crate::custom_arena_profiles::Profile,
    mods_root: &Path,
) -> Result<(String, bool)> {
    profile.validate()?;
    ensure!(
        profile.enabled && mods_root.is_absolute(),
        "Saved arena profile is not enabled"
    );
    let access = Access::open(binding, Arc::new(AtomicBool::new(false)))?;
    let state = state(&access)?;
    let catalog =
        arena::capture(&access, &state)?.context("Native arena catalog is unavailable")?;
    let operation = Operation::ArenaProfile(json!({"profile":profile,"modsRoot":mods_root}));
    let (selector, mode) = select(&access, &state, catalog.id(), &operation)?;
    ensure!(
        mode == clone::Mode::PopulateOnly,
        "Invalid saved arena admission mode"
    );
    let (donor, target) = indices(catalog.raw_rows(), &selector.edit)?;
    let team = read_record(
        &access,
        record_address(&state, &state.teams, Kind::Team, selector.edit.record_index)?,
        profile::TEAM_STRIDE,
    )?;
    let owner = owner(
        &access,
        &state,
        selector.edit.record_index,
        donor,
        target,
        catalog.id(),
    )?;
    let plan = clone::plan_profile(
        owner,
        &selector.edit,
        &team,
        catalog.raw_rows(),
        &links(&access, &state)?,
    )?;
    let mods = [
        PinnedMod::open(mods_root, &profile.arena_file)?,
        PinnedMod::open(mods_root, &profile.floor_file)?,
    ];
    for m in &mods {
        m.check()?;
    }
    catalog.revalidate(&access, &state)?;
    ensure!(
        raw(&access, &state, &plan.owner)? == plan.before,
        "Saved arena admission changed"
    );
    access.finish()?;
    Ok((profile_key(binding, profile)?, plan.before != plan.after))
}
fn profile_key(
    binding: &Binding,
    profile: &crate::custom_arena_profiles::Profile,
) -> Result<String> {
    Ok(clone::hash(&serde_json::to_vec(
        &json!({"session":binding.session,"game":binding.game,"profileId":profile.id,"scope":"initial-owned-launch-only"}),
    )?))
}

pub(super) fn reapply_profile(
    root: &Path,
    binding: &Binding,
    profile: &crate::custom_arena_profiles::Profile,
    mods_root: &Path,
    gate: &crate::session_safety::WriteGuard,
) -> Result<Value> {
    let (_key, needs_write) = profile_admission(binding, profile, mods_root)?;
    if !needs_write {
        return Ok(
            json!({"schema":"nba2k27-v2-arena-clone-v1","ok":true,"readBackVerified":true,"mode":"populateOnly","noChanges":true,"patchWrites":0,"gameCodeHooks":0}),
        );
    }
    let operation = Operation::ArenaProfile(json!({"profile":profile,"modsRoot":mods_root}));
    let report = crate::owned_transaction::execute_guarded(
        gate,
        root,
        &binding.game,
        &binding.parent,
        &binding.session,
        operation,
        true,
    )?;
    ensure!(
        report["ok"] == true
            && report["readBackVerified"] == true
            && report["mode"] == "populateOnly",
        "Native saved arena application was not verified"
    );
    Ok(report)
}
pub(super) fn editor(capture: &mut NativeCapture, request: &Editor<'_>) -> Result<Value> {
    let v = request.input.as_object().context("Invalid arena request")?;
    ensure!(
        v.len() == 2 && v.contains_key("edit") && v.contains_key("modsRoot"),
        "Unsupported arena request option"
    );
    let edit = clone::Edit::parse(&v["edit"])?;
    let mods_root = v["modsRoot"]
        .as_str()
        .context("Missing native Mods folder")?;
    ensure!(
        Path::new(mods_root).is_absolute(),
        "Mods folder must come from the native launcher"
    );
    let state = capture.begin_service()?;
    let cached = capture.snapshot.row(Kind::Team, edit.record_index)?;
    let at = record_address(&state, &state.teams, Kind::Team, edit.record_index)?;
    ensure!(
        read_record(&capture.access, at, profile::TEAM_STRIDE)? == cached,
        "Selected full team changed; refresh before cloning"
    );
    let catalog = capture
        .snapshot
        .arenas()
        .context("Native arena catalog is unavailable")?;
    ensure!(catalog.id() == edit.catalog_id, "Arena review expired");
    catalog.revalidate(&capture.access, &state)?;
    let input = json!({"edit":edit,"expectedTeamHash":clone::hash(cached),"modsRoot":mods_root});
    capture.access.finish()?;
    if !(request.confirm)(
        "Create this custom arena in an empty reserved slot and link the selected team? The game will pause briefly. If a rare partial native transaction cannot be safely verified, the owned game will close without saving to protect the roster.",
    ) {
        return Ok(json!({"canceled":true,"ok":false}));
    }
    capture.stale = true;
    let report = crate::owned_transaction::execute_guarded(
        request
            .write_gate
            .context("Arena editor must retain the global native write gate")?,
        request.root,
        &capture.access.binding.game,
        &capture.access.binding.parent,
        &capture.access.binding.session,
        Operation::ArenaClone(input),
        true,
    )?;
    ensure!(
        report["ok"] == true && report["readBackVerified"] == true,
        "Native arena helper did not confirm the complete clone"
    );
    Ok(json!({"report":report}))
}
#[cfg(test)]
#[path = "roster_v2_arena_clone_access_tests.rs"]
mod tests;
