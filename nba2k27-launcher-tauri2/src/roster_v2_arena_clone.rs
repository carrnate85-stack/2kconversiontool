//! Reserved arena copy-on-write. Only an unused, empty 996..1045 row may be
//! populated, followed by the selected team's final pointer publication.
//! The public generic roster writer never acquires arena-record authority.
use crate::{backend, roster_v2_profile as profile, roster_v2_write as writer};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) const STRIDE: usize = 0x1278;
pub(crate) const ID: usize = 0x792;
pub(crate) const LINK: usize = 0xee0;
const LIMIT: u64 = 256 * 1024;
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn word(row: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        row.get(at..at + 2)
            .context("Incomplete arena word")?
            .try_into()?,
    ))
}
fn pointer(row: &[u8]) -> Result<u64> {
    Ok(u64::from_le_bytes(
        row.get(LINK..LINK + 8)
            .context("Incomplete team link")?
            .try_into()?,
    ))
}
fn text(row: &[u8], at: usize, len: usize) -> Result<String> {
    let units: Vec<_> = row
        .get(at..at + len)
        .context("Incomplete arena text")?
        .chunks_exact(2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .collect();
    let end = units
        .iter()
        .position(|v| *v == 0)
        .context("Unterminated arena text")?;
    let result = String::from_utf16(&units[..end])?;
    ensure!(!result.chars().any(char::is_control), "Invalid arena text");
    Ok(result)
}
fn ascii(row: &[u8], at: usize, len: usize) -> Result<String> {
    let b = row.get(at..at + len).context("Incomplete arena asset")?;
    let end = b
        .iter()
        .position(|v| *v == 0)
        .context("Unterminated arena asset")?;
    ensure!(
        b[..end].iter().all(|v| (32..=126).contains(v)),
        "Invalid arena asset"
    );
    Ok(String::from_utf8(b[..end].to_vec())?)
}
pub(crate) fn empty(row: &[u8]) -> bool {
    row.len() == STRIDE
        && row[..70].iter().all(|v| *v == 0)
        && row[0x7e..0xcc].iter().all(|v| *v == 0)
}
pub(crate) fn stem(value: &str, maximum: usize) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= maximum
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Use an arena asset filename without a path or extension"
    );
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Edit {
    pub record_index: usize,
    pub team_id: u16,
    pub expected_team_name: String,
    pub donor_arena_id: u16,
    pub target_arena_id: u16,
    pub arena_name: String,
    pub arena_file: String,
    pub floor_file: String,
    pub catalog_id: String,
}
impl Edit {
    pub(crate) fn parse(input: &Value) -> Result<Self> {
        let edit: Self = serde_json::from_value(input.clone())?;
        edit.validate()?;
        Ok(edit)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.record_index < 1024
                && self.donor_arena_id != self.target_arena_id
                && (996..=1045).contains(&self.target_arena_id),
            "Choose a different empty reserved arena slot"
        );
        ensure!(
            !self.arena_name.trim().is_empty()
                && self.arena_name.encode_utf16().count() < 35
                && !self.arena_name.chars().any(char::is_control),
            "Use an arena name of at most 34 characters"
        );
        ensure!(
            self.expected_team_name.encode_utf16().count() <= 24 && digest(&self.catalog_id),
            "Invalid team or arena review identity"
        );
        stem(&self.arena_file, 37)?;
        stem(&self.floor_file, 39)
    }
}
/// The selected team is the ONLY generic roster record this transaction can
/// alter. Arena permission remains a separately admitted, empty reserved row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Owner {
    pub team: writer::Owner,
    pub arena_table: u64,
    pub arena_count: usize,
    pub donor: usize,
    pub target: usize,
    pub target_id: u16,
    pub donor_hash: String,
    pub ids_hash: String,
    pub other_links_hash: String,
    pub catalog_id: String,
    pub protected_header_hash: String,
}
impl Owner {
    pub(crate) fn validate(&self) -> Result<()> {
        self.team.validate()?;
        ensure!(
            self.team.kind == writer::Kind::Team
                && (1..=2048).contains(&self.arena_count)
                && self.donor < self.arena_count
                && self.target < self.arena_count
                && self.target != self.donor
                && (996..=1045).contains(&self.target_id),
            "Invalid reserved arena ownership"
        );
        let end = self
            .arena_table
            .checked_add((self.arena_count * STRIDE) as u64)
            .context("Arena span overflow")?;
        ensure!(
            self.arena_table >= self.team.region_base
                && self.arena_table % 8 == 0
                && end <= self.team.region_base + self.team.region_len
                && (end <= self.team.table
                    || self.arena_table
                        >= self.team.table + (self.team.count * profile::TEAM_STRIDE) as u64),
            "Arena and team ownership overlap or escape"
        );
        let prefix = self
            .team
            .region_base
            .checked_add(0x680)
            .context("Arena metadata prefix overflow")?;
        for (at, n) in [
            (self.arena_table, self.arena_count * STRIDE),
            (self.team.table, self.team.count * profile::TEAM_STRIDE),
        ] {
            let end = at
                .checked_add(n as u64)
                .context("Arena protected extent overflow")?;
            ensure!(
                at >= prefix,
                "Arena or Team table overlaps allocation metadata prefix"
            );
            for (p, z) in [(self.team.owner, 0x3d0u64), (self.team.root, 0xb0u64)] {
                let limit = p.checked_add(z).context("Arena protected owner overflow")?;
                ensure!(
                    end <= p || limit <= at,
                    "Arena or Team table overlaps full owner/root metadata"
                );
            }
        }
        ensure!(
            [
                &self.donor_hash,
                &self.ids_hash,
                &self.other_links_hash,
                &self.catalog_id,
                &self.protected_header_hash
            ]
            .into_iter()
            .all(|s| digest(s)),
            "Invalid arena ownership witnesses"
        );
        Ok(())
    }
    pub(crate) fn target_address(&self) -> u64 {
        self.arena_table + (self.target * STRIDE) as u64
    }
}
pub(crate) fn links_hash(links: &[u64], selected: usize) -> String {
    hash(
        &links
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != selected)
            .flat_map(|(_, p)| p.to_le_bytes())
            .collect::<Vec<_>>(),
    )
}
pub(crate) fn ids_hash(rows: &[u8]) -> Result<String> {
    ensure!(rows.len() % STRIDE == 0, "Incomplete arena directory");
    Ok(hash(
        &rows
            .chunks_exact(STRIDE)
            .flat_map(|r| [r[ID], r[ID + 1]])
            .collect::<Vec<_>>(),
    ))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Raw {
    pub team: Vec<u8>,
    pub arena: Vec<u8>,
}
impl Raw {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.team.len() == profile::TEAM_STRIDE && self.arena.len() == STRIDE,
            "Incomplete clone preimage"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Area {
    Arena,
    Team,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Patch {
    pub area: Area,
    pub offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Mode {
    CloneAndLink,
    PopulateOnly,
}
impl Default for Mode {
    fn default() -> Self {
        Self::CloneAndLink
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub mode: Mode,
    pub owner: Owner,
    pub before: Raw,
    pub after: Raw,
    pub patches: Vec<Patch>,
    id: String,
}
impl Plan {
    pub(crate) fn expected(&self, n: usize) -> Result<Raw> {
        ensure!(n <= self.patches.len(), "Invalid clone phase");
        let mut raw = self.before.clone();
        for p in &self.patches[..n] {
            let b = if p.area == Area::Team {
                &mut raw.team
            } else {
                &mut raw.arena
            };
            ensure!(
                b[p.offset..p.offset + p.before.len()] == p.before,
                "Invalid clone dependency order"
            );
            b[p.offset..p.offset + p.after.len()].copy_from_slice(&p.after);
        }
        Ok(raw)
    }
}
pub(crate) fn plan(
    owner: Owner,
    edit: &Edit,
    team: &[u8],
    rows: &[u8],
    links: &[u64],
) -> Result<Plan> {
    plan_mode(Mode::CloneAndLink, owner, edit, team, rows, links)
}
/// Native saved-profile factory only. The renderer cannot request this mode.
pub(crate) fn plan_profile(
    owner: Owner,
    edit: &Edit,
    team: &[u8],
    rows: &[u8],
    links: &[u64],
) -> Result<Plan> {
    plan_mode(Mode::PopulateOnly, owner, edit, team, rows, links)
}
fn plan_mode(
    mode: Mode,
    owner: Owner,
    edit: &Edit,
    team: &[u8],
    rows: &[u8],
    links: &[u64],
) -> Result<Plan> {
    owner.validate()?;
    edit.validate()?;
    ensure!(
        owner.catalog_id == edit.catalog_id
            && owner.team.index == edit.record_index
            && links.len() == owner.team.count
            && rows.len() == owner.arena_count * STRIDE
            && team.len() == profile::TEAM_STRIDE,
        "Arena review expired"
    );
    ensure!(
        word(team, writer::TEAM_ID)? == edit.team_id
            && text(team, 0x2fa, 48)? == edit.expected_team_name
            && pointer(team)? == links[edit.record_index],
        "Selected team changed before cloning"
    );
    let donor = &rows[owner.donor * STRIDE..(owner.donor + 1) * STRIDE];
    let target = &rows[owner.target * STRIDE..(owner.target + 1) * STRIDE];
    ensure!(
        word(donor, ID)? == edit.donor_arena_id
            && word(target, ID)? == edit.target_arena_id
            && owner.target_id == edit.target_arena_id,
        "Arena IDs changed"
    );
    ensure!(
        [edit.donor_arena_id, edit.target_arena_id]
            .iter()
            .all(|id| rows
                .chunks_exact(STRIDE)
                .filter(|r| word(r, ID).ok() == Some(*id))
                .count()
                == 1),
        "Arena IDs are ambiguous"
    );
    ensure!(
        !text(donor, 0, 70)?.trim().is_empty()
            && !ascii(donor, 0x7e, 38)?.is_empty()
            && !ascii(donor, 0xa4, 40)?.is_empty(),
        "Donor arena is unavailable"
    );
    if mode == Mode::CloneAndLink {
        ensure!(
            empty(target) && !links.contains(&owner.target_address()),
            "Reserved arena is populated or referenced; it cannot be overwritten"
        );
    }
    ensure!(
        hash(donor) == owner.donor_hash
            && ids_hash(rows)? == owner.ids_hash
            && links_hash(links, edit.record_index) == owner.other_links_hash,
        "Arena dependencies changed"
    );
    let before = Raw {
        team: team.to_vec(),
        arena: target.to_vec(),
    };
    let mut after = before.clone();
    after.arena = donor.to_vec();
    after.arena[ID..ID + 2].copy_from_slice(&edit.target_arena_id.to_le_bytes());
    after.arena[..70].fill(0);
    for (i, c) in edit.arena_name.encode_utf16().enumerate() {
        after.arena[i * 2..i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    for (at, len, s) in [(0x7e, 38, &edit.arena_file), (0xa4, 40, &edit.floor_file)] {
        after.arena[at..at + len].fill(0);
        after.arena[at..at + s.len()].copy_from_slice(s.as_bytes());
    }
    if mode == Mode::CloneAndLink {
        after.team[LINK..LINK + 8].copy_from_slice(&owner.target_address().to_le_bytes());
    } else {
        ensure!(
            empty(target) || target == after.arena,
            "Saved arena target is populated with different bytes; it cannot be adopted or overwritten"
        );
    }
    let mut patches = Vec::new();
    // Never include the target ID in a writable span, including no-op bytes.
    for range in [0..ID, ID + 2..STRIDE] {
        for start in (range.start..range.end).step_by(128) {
            let end = (start + 128).min(range.end);
            if before.arena[start..end] != after.arena[start..end] {
                patches.push(Patch {
                    area: Area::Arena,
                    offset: start,
                    before: before.arena[start..end].to_vec(),
                    after: after.arena[start..end].to_vec(),
                });
            }
        }
    }
    if before.team[LINK..LINK + 8] != after.team[LINK..LINK + 8] {
        patches.push(Patch {
            area: Area::Team,
            offset: LINK,
            before: before.team[LINK..LINK + 8].to_vec(),
            after: after.team[LINK..LINK + 8].to_vec(),
        });
    }
    let result = Plan {
        mode,
        owner,
        before,
        after,
        patches,
        id: backend::unique_id(),
    };
    validate_patches(result.mode, &result.owner, &result.patches)?;
    ensure!(
        result.expected(result.patches.len())? == result.after,
        "Incomplete clone plan"
    );
    Ok(result)
}
fn validate_patches(mode: Mode, owner: &Owner, patches: &[Patch]) -> Result<()> {
    owner.validate()?;
    ensure!(
        (mode == Mode::PopulateOnly || !patches.is_empty()) && patches.len() <= 40,
        "Invalid clone patch count"
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut linked = false;
    for p in patches {
        ensure!(
            !linked
                && !p.before.is_empty()
                && p.before.len() == p.after.len()
                && p.before != p.after,
            "Invalid clone patch"
        );
        let end = p
            .offset
            .checked_add(p.after.len())
            .context("Clone patch overflow")?;
        if p.area == Area::Team {
            ensure!(
                mode == Mode::CloneAndLink
                    && p.offset == LINK
                    && p.after.len() == 8
                    && u64::from_le_bytes(p.after.as_slice().try_into()?) == owner.target_address(),
                "Unowned clone team write"
            );
            linked = true;
        } else {
            ensure!(
                end <= STRIDE && p.after.len() <= 128 && (end <= ID || p.offset >= ID + 2),
                "Unowned clone arena write"
            );
        }
        for at in p.offset..end {
            ensure!(seen.insert((p.area as u8, at)), "Overlapping clone patches");
        }
    }
    ensure!(
        linked == (mode == Mode::CloneAndLink),
        "Clone mode does not match final team link authority"
    );
    Ok(())
}
/// Adapter methods derive destinations from the retained Owner and immutable
/// Plan. No address, writable span or row buffer can come from the renderer.
pub(crate) trait Access {
    fn owner(&self) -> Result<Owner>;
    fn capture(&self) -> Result<Raw>;
    fn write(&self, area: Area, offset: usize, bytes: &[u8]) -> Result<()>;
    fn checkpoint(&self) -> Result<()>;
    fn begin_reverse(&self) -> Result<()> {
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Intent {
    #[serde(default)]
    mode: Mode,
    schema: String,
    id: String,
    owner: Owner,
    patches: Vec<Patch>,
    pub pending: bool,
    pub result: String,
}
impl Intent {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "nba2k27-v2-arena-clone-intent-v1"
                && self.id.len() == 48
                && self.id.bytes().all(|b| b.is_ascii_hexdigit())
                && self.result.len() <= 512,
            "Unsupported clone journal; left untouched"
        );
        validate_patches(self.mode, &self.owner, &self.patches)
    }
}
pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("Data/roster-v2-arena-clone-journal.json")
}
fn load(root: &Path) -> Result<Intent> {
    let mut b = Vec::new();
    std::fs::File::open(path(root))?
        .take(LIMIT + 1)
        .read_to_end(&mut b)?;
    ensure!(b.len() as u64 <= LIMIT, "Clone journal exceeds bounds");
    let value: Intent = serde_json::from_slice(&b)?;
    value.validate()?;
    Ok(value)
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    Ok(path(root).try_exists()? && load(root)?.pending)
}
/// Watchdog-only metadata finalization after the framework proves that the
/// stopped exact helper never attempted a native effect. This deliberately
/// does not claim that naturally changing roster bytes remain equal.
pub(crate) fn resolve_no_effect(root: &Path, expected: &Intent) -> Result<()> {
    expected.validate()?;
    if !path(root).try_exists()? {
        return Ok(());
    }
    let mut current = load(root)?;
    if !current.pending {
        return Ok(());
    }
    ensure!(
        current.id == expected.id
            && current.mode == expected.mode
            && current.owner == expected.owner
            && serde_json::to_value(&current.patches)? == serde_json::to_value(&expected.patches)?,
        "Pending arena journal is not this no-effect transaction"
    );
    current.pending = false;
    current.result = "helper attempted no native effects; no graph equality claimed".into();
    save(root, &current)
}
pub(crate) fn save(root: &Path, intent: &Intent) -> Result<()> {
    intent.validate()?;
    ensure!(
        serde_json::to_vec_pretty(intent)?.len() as u64 <= LIMIT,
        "Clone journal exceeds bounds"
    );
    backend::atomic_json(&path(root), intent)
}
pub(crate) fn prepare_intent(plan: &Plan) -> Result<Intent> {
    validate_patches(plan.mode, &plan.owner, &plan.patches)?;
    Ok(Intent {
        mode: plan.mode,
        schema: "nba2k27-v2-arena-clone-intent-v1".into(),
        id: plan.id.clone(),
        owner: plan.owner.clone(),
        patches: plan.patches.clone(),
        pending: true,
        result: "intent saved before write".into(),
    })
}
pub(crate) fn validate_prepared(intent: &Intent, plan: &Plan) -> Result<()> {
    intent.validate()?;
    ensure!(
        intent.pending
            && intent.result == "intent saved before write"
            && intent.mode == plan.mode
            && intent.owner == plan.owner
            && serde_json::to_value(&intent.patches)? == serde_json::to_value(&plan.patches)?,
        "Immutable prepared arena intent differs from native plan"
    );
    Ok(())
}
pub(crate) fn resolve_ended(root: &Path, rows: &[crate::session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving arena recovery"
    );
    if path(root).try_exists()? {
        let mut i = load(root)?;
        if i.pending {
            i.pending = false;
            i.result = "game-ended; memory changes cannot be replayed".into();
            save(root, &i)?;
        }
    }
    Ok(())
}
fn check(m: &impl Access, plan: &Plan, expected: &Raw) -> Result<Raw> {
    ensure!(m.owner()? == plan.owner, "Arena clone ownership changed");
    let raw = m.capture()?;
    raw.validate()?;
    ensure!(raw == *expected, "Arena clone or selected team changed");
    Ok(raw)
}
pub(crate) fn apply(
    m: &impl Access,
    plan: &Plan,
    mut persist: impl FnMut(&Intent) -> Result<()>,
) -> Result<Value> {
    validate_patches(plan.mode, &plan.owner, &plan.patches)?;
    plan.before.validate()?;
    plan.after.validate()?;
    ensure!(
        (empty(&plan.before.arena)
            || (plan.mode == Mode::PopulateOnly && plan.before == plan.after))
            && plan.expected(plan.patches.len())? == plan.after,
        "Unadmitted clone plan"
    );
    m.checkpoint()?;
    check(m, plan, &plan.before)?;
    let mut intent = prepare_intent(plan)?;
    persist(&intent)?;
    let mut attempted = 0;
    let result = (|| -> Result<()> {
        for (i, p) in plan.patches.iter().enumerate() {
            m.checkpoint()?;
            check(m, plan, &plan.expected(i)?)?;
            m.checkpoint()?;
            attempted = i + 1;
            m.write(p.area, p.offset, &p.after)?;
            check(m, plan, &plan.expected(i + 1)?)?;
        }
        check(m, plan, &plan.after)?;
        intent.pending = false;
        intent.result = "applied and read back verified".into();
        persist(&intent)
    })();
    if let Err(error) = result {
        let rollback = (|| -> Result<()> {
            m.begin_reverse()?;
            for i in (0..attempted).rev() {
                ensure!(m.owner()? == plan.owner, "Clone rollback owner changed");
                let raw = m.capture()?;
                let before = plan.expected(i)?;
                if raw == before {
                    continue;
                }
                ensure!(
                    raw == plan.expected(i + 1)?,
                    "Partial or foreign clone changes prevent rollback"
                );
                let p = &plan.patches[i];
                m.write(p.area, p.offset, &p.before)?;
                check(m, plan, &before)?;
            }
            check(m, plan, &plan.before)?;
            Ok(())
        })();
        intent.pending = rollback.is_err();
        intent.result = if rollback.is_ok() {
            "failed; original owned values verified"
        } else {
            "failed; recovery held for foreign or partial values"
        }
        .into();
        let recorded = persist(&intent);
        anyhow::bail!(
            "{error:#}. {}{}",
            if rollback.is_ok() {
                "Original team and reserved arena restored."
            } else {
                "Arena recovery is held. Close the game before clearing it."
            },
            if recorded.is_err() {
                " Recovery record remains incomplete."
            } else {
                ""
            }
        );
    }
    Ok(
        json!({"schema":"nba2k27-v2-arena-clone-v1","ok":true,"targetArenaId":plan.owner.target_id,"readBackVerified":true,"gameCodeHooks":0,"rollbackOnFailure":true,"patchWrites":plan.patches.len(),"mode":plan.mode,"noChanges":plan.before==plan.after}),
    )
}

#[cfg(test)]
#[path = "roster_v2_arena_clone_tests.rs"]
mod tests;
