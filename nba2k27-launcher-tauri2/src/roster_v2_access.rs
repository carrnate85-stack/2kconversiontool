//! Version 2 browser roster backend. Reads use Query/VM_READ handles. Save &
//! verify opens one short-lived VM_WRITE lease per transaction, bounded to a
//! single captured record in committed private PAGE_READWRITE data; planning,
//! journaling and rollback live in roster_v2_write. The caller supplies an
//! authenticated native launch binding, never browser identities.
//! Routed from roster_native::Service::request_tab_routed and save_*.
#![allow(dead_code)]
use crate::{
    blacktop_native::{self, Memory, Reader},
    roster_v2_profile as profile, roster_v2_readonly as model, roster_v2_services as services,
    roster_v2_write::{self as writer, Kind},
    session_safety::{self, Identity, Process},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    mem::size_of,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{Diagnostics::Debug::WriteProcessMemory, Memory::*, Threading::*},
    },
    core::PWSTR,
};

const BUDGET: Duration = Duration::from_secs(12);
const RIGHTS: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0);
/// Save-only lease rights. No thread, terminate, suspend or create rights.
const WRITE_RIGHTS: PROCESS_ACCESS_RIGHTS = PROCESS_ACCESS_RIGHTS(
    PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0,
);
const MAX_ALLOCATION: u64 = 128 * 1024 * 1024;
#[path = "roster_v2_appearance_access.rs"]
mod appearance_access;
#[path = "roster_v2_arena_clone_access.rs"]
mod arena_clone_access;
#[path = "season_structure_v2_native.rs"]
mod structural;
pub(crate) fn structural_pending(root: &Path) -> Result<bool> {
    structural::pending(root)
}
pub(crate) fn resolve_structural_ended(root: &Path, rows: &[Process]) -> Result<()> {
    structural::resolve_ended(root, rows)
}
pub(crate) fn profile_binding_is_newer(binding: &Binding, requested_ms: u64) -> bool {
    binding
        .game
        .started
        .parse::<u64>()
        .ok()
        .and_then(|ticks| ticks.checked_div(10_000))
        .and_then(|ms| ms.checked_sub(11_644_473_600_000))
        .is_some_and(|ms| ms >= requested_ms)
}
pub(crate) fn profile_admission(
    binding: &Binding,
    profile: &crate::custom_arena_profiles::Profile,
    mods_root: &Path,
) -> Result<(String, bool)> {
    arena_clone_access::profile_admission(binding, profile, mods_root)
}
pub(crate) fn reapply_profile(
    root: &Path,
    binding: &Binding,
    profile: &crate::custom_arena_profiles::Profile,
    mods_root: &Path,
    gate: &session_safety::WriteGuard,
) -> Result<Value> {
    arena_clone_access::reapply_profile(root, binding, profile, mods_root, gate)
}
pub(crate) fn execute_owned_transaction(
    ctx: &mut crate::owned_transaction::Context,
    operation: &crate::owned_transaction::Operation,
) -> Result<Value> {
    match operation {
        crate::owned_transaction::Operation::Season(spec) => structural::execute_inside(ctx, spec),
        crate::owned_transaction::Operation::ArenaClone(_)
        | crate::owned_transaction::Operation::ArenaProfile(_) => {
            arena_clone_access::execute_inside(ctx, operation)
        }
    }
}
pub(crate) fn verify_owned_transaction(
    ctx: &mut crate::owned_transaction::ReadContext,
    operation: &crate::owned_transaction::Operation,
    receipt: &Value,
) -> Result<()> {
    match operation {
        crate::owned_transaction::Operation::Season(spec) => {
            structural::verify_inside(ctx, spec, receipt)
        }
        crate::owned_transaction::Operation::ArenaClone(_)
        | crate::owned_transaction::Operation::ArenaProfile(_) => {
            arena_clone_access::verify_inside(ctx, operation, receipt)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    game: Identity,
    parent: Identity,
    root: PathBuf,
    session: String,
    companions: Vec<Identity>,
}
impl Binding {
    /// Only call using NativeSession::binding / the retained provider launch
    /// record. Do not deserialize these fields from HTTP/Tauri arguments.
    pub fn from_launched(
        game: Identity,
        parent: Identity,
        root: PathBuf,
        session: String,
    ) -> Result<Self> {
        ensure!(
            root.is_absolute()
                && Path::new(&game.path).is_absolute()
                && Path::new(&parent.path).is_absolute()
                && game.pid > 0
                && parent.pid > 0
                && game.pid != parent.pid
                && session.len() == 48
                && session.bytes().all(|v| v.is_ascii_hexdigit())
                && game.started.parse::<u64>().is_ok_and(|v| v > 0)
                && parent.started.parse::<u64>().is_ok_and(|v| v > 0)
                && crate::native_session::same_path(&game.path, &root.join("NBA2K27.exe")),
            "Invalid owned Version 2 roster binding"
        );
        Ok(Self {
            game,
            parent,
            root,
            session,
            companions: Vec::new(),
        })
    }
}
fn guard(rows: &[Process], binding: &Binding) -> Result<()> {
    guard_with_current(
        rows,
        binding,
        session_safety::identity(std::process::id()).ok().as_ref(),
    )
}
fn transaction_access(
    game: &Identity,
    parent: &Identity,
    session: &str,
    companions: Vec<Identity>,
) -> Result<Access> {
    let root = Path::new(&game.path)
        .parent()
        .context("Game directory is absent")?
        .to_path_buf();
    let mut binding =
        Binding::from_launched(game.clone(), parent.clone(), root, session.to_owned())?;
    ensure!(
        (1..=2).contains(&companions.len()),
        "Missing retained transaction companions"
    );
    for (index, companion) in companions.iter().enumerate() {
        ensure!(
            companion.pid != game.pid
                && companion.pid != parent.pid
                && companion.started.parse::<u64>().is_ok_and(|v| v > 0)
                && crate::native_session::same_path(&companion.path, Path::new(&parent.path))
                && companions[..index]
                    .iter()
                    .all(|other| other.pid != companion.pid),
            "Invalid retained transaction companion"
        );
    }
    ensure!(
        companions
            .iter()
            .any(|identity| identity.pid == std::process::id()),
        "Transaction helper identity is missing"
    );
    binding.companions = companions;
    Access::open(&binding, Arc::new(AtomicBool::new(false)))
}
fn structural_access(ctx: &crate::owned_transaction::Context) -> Result<Access> {
    transaction_access(
        ctx.game(),
        ctx.parent(),
        ctx.session(),
        ctx.trusted_companions()?,
    )
}
fn structural_verify_access(ctx: &crate::owned_transaction::ReadContext) -> Result<Access> {
    transaction_access(
        ctx.game(),
        ctx.parent(),
        ctx.session(),
        ctx.trusted_companions()?,
    )
}
fn guard_with_current(
    rows: &[Process],
    binding: &Binding,
    current: Option<&Identity>,
) -> Result<()> {
    // process_scan deliberately omits this process. When the browser service
    // lives in the UI, prove its exact identity separately; never require the
    // current launcher to appear in its own filtered process snapshot.
    ensure!(
        current == Some(&binding.parent)
            || rows
                .iter()
                .any(|p| p.identity.as_ref() == Some(&binding.parent)),
        "Owning launcher is absent or changed"
    );
    let others: Vec<_> = rows
        .iter()
        .filter(|p| {
            !(session_safety::launcher_process(p)
                && (p.identity.as_ref() == Some(&binding.parent)
                    || binding
                        .companions
                        .iter()
                        .any(|identity| p.identity.as_ref() == Some(identity))))
        })
        .map(|p| Process {
            name: p.name.clone(),
            pid: p.pid,
            parent: p.parent,
            identity: p.identity.clone(),
        })
        .collect();
    blacktop_native::guard(&others, &binding.game)
}
struct QueryHandle(HANDLE);
impl Drop for QueryHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
unsafe impl Send for QueryHandle {} // Owned; all access is under the service operation mutex.
fn handle_matches(handle: HANDLE, expected: &Identity) -> Result<()> {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u)?;
    }
    let birth = (u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime);
    ensure!(
        e.dwLowDateTime == 0 && e.dwHighDateTime == 0 && expected.started == birth.to_string(),
        "Retained roster game ended or birth identity changed"
    );
    let mut text = vec![0u16; 32768];
    let mut len = text.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(text.as_mut_ptr()),
            &mut len,
        )?;
    }
    ensure!(
        len > 0
            && (len as usize) < text.len()
            && crate::native_session::same_path(
                &String::from_utf16(&text[..len as usize])?,
                Path::new(&expected.path)
            ),
        "Retained roster image path changed"
    );
    Ok(())
}
#[derive(Clone, Copy, Debug)]
struct Region {
    base: u64,
    allocation: u64,
    len: u64,
    state: u32,
    kind: u32,
    protection: u32,
}
fn contains(base: u64, len: u64, at: u64, bytes: usize) -> bool {
    at >= base
        && bytes > 0
        && at
            .checked_add(bytes as u64)
            .zip(base.checked_add(len))
            .is_some_and(|(end, limit)| end <= limit)
}
fn private(r: Region) -> bool {
    r.state == MEM_COMMIT.0 && r.kind == MEM_PRIVATE.0 && matches!(r.protection, 2 | 4 | 8)
}
/// Walk only the allocation selected by the verified active owner, never the
/// address space. Every callback is a bounded VirtualQueryEx, not a game read.
fn allocation(
    table: u64,
    mut query: impl FnMut(u64) -> Result<Region>,
) -> Result<profile::OwnedRegion> {
    let first = query(table)?;
    let base = first.allocation;
    ensure!(
        private(first)
            && base >= 0x10000
            && base % 4096 == 0
            && contains(first.base, first.len, table, 1),
        "Active table is not committed non-executable private data"
    );
    let mut at = base;
    for _ in 0..32770 {
        let r = query(at)?;
        if r.allocation != base {
            let len = at.checked_sub(base).context("Allocation range underflow")?;
            ensure!(
                (8 * 1024 * 1024..=MAX_ALLOCATION).contains(&len) && contains(base, len, table, 1),
                "Active roster allocation size differs"
            );
            return Ok(profile::OwnedRegion { base, len });
        }
        ensure!(
            r.base == at && private(r) && r.len >= 4096 && r.len % 4096 == 0,
            "Roster allocation has a hole, executable page or unsupported protection"
        );
        at = at.checked_add(r.len).context("Allocation range overflow")?;
        ensure!(
            at - base <= MAX_ALLOCATION,
            "Roster allocation exceeds128MiB bound"
        );
    }
    anyhow::bail!("Roster allocation query budget exceeded")
}
pub(crate) struct Access {
    reader: Reader,
    query: QueryHandle,
    binding: Binding,
    stop: Arc<AtomicBool>,
    started: Instant,
    recovery_started: Cell<Option<Instant>>,
}
impl Access {
    pub fn open(binding: &Binding, stop: Arc<AtomicBool>) -> Result<Self> {
        ensure!(
            !stop.load(Ordering::Acquire),
            "Read-only roster service is closing"
        );
        guard(&session_safety::processes()?, binding)?;
        // Independent observer gate verifies first-module path and absence of
        // protected modules. Its legacy writer/snapshot gate is never called.
        let reader = Reader::open_observer(&binding.game, &binding.root)?;
        let query = QueryHandle(unsafe { OpenProcess(RIGHTS, false, binding.game.pid)? });
        handle_matches(query.0, &binding.game)?;
        let access = Self {
            reader,
            query,
            binding: binding.clone(),
            stop,
            started: Instant::now(),
            recovery_started: Cell::new(None),
        };
        model::verify_profile(&access)?;
        access.finish()?;
        Ok(access)
    }
    fn begin_recovery(&self) -> Result<()> {
        handle_matches(self.query.0, &self.binding.game)?;
        if self.recovery_started.get().is_none() {
            self.recovery_started.set(Some(Instant::now()));
        }
        self.checkpoint()
    }
    fn start(&mut self) -> Result<()> {
        ensure!(
            self.recovery_started.get().is_none(),
            "Recovery access cannot start another forward request; recapture required"
        );
        self.started = Instant::now();
        self.checkpoint()?;
        guard(&session_safety::processes()?, &self.binding)?;
        self.modules()
    }
    fn modules(&self) -> Result<()> {
        self.checkpoint()?;
        let current = Reader::open_observer(&self.binding.game, &self.binding.root)?;
        ensure!(
            current.base() == self.reader.base(),
            "Roster image module base changed"
        );
        self.checkpoint()
    }
    fn finish(&self) -> Result<()> {
        self.checkpoint()?;
        guard(&session_safety::processes()?, &self.binding)?;
        self.modules()
    }
    fn query(&self, at: u64) -> Result<Region> {
        self.checkpoint()?;
        ensure!(
            (0x10000..=0x7fff_ffff_ffff).contains(&at),
            "Invalid native roster query"
        );
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let done = unsafe {
            VirtualQueryEx(
                self.query.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            done == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Native roster query failed"
        );
        Ok(Region {
            base: info.BaseAddress as u64,
            allocation: info.AllocationBase as u64,
            len: info.RegionSize as u64,
            state: info.State.0,
            kind: info.Type.0,
            protection: info.Protect.0,
        })
    }
    fn region(&self) -> Result<profile::OwnedRegion> {
        profile::verify_identity_profile(self)?;
        let root = self.pointer(self.base() + profile::ROOT)?;
        let owner = self.pointer(root.checked_add(0xa8).context("Root pointer overflow")?)?;
        let table = self.pointer(owner.checked_add(0x18).context("Owner pointer overflow")?)?;
        let result = allocation(table, |at| self.query(at))?;
        profile::inspect_owner(self, result)?;
        Ok(result)
    }
    fn pointer(&self, at: u64) -> Result<u64> {
        let value = u64::from_le_bytes(
            self.read(at, 8)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("Short roster pointer"))?,
        );
        ensure!(
            (0x10000..=0x7fff_ffff_fff8).contains(&value) && value % 8 == 0,
            "Invalid roster ownership pointer"
        );
        Ok(value)
    }
}
impl Memory for Access {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.checkpoint()?;
        ensure!(
            (1..=4096).contains(&len)
                && at >= 0x10000
                && at
                    .checked_add(len as u64)
                    .is_some_and(|end| end <= 0x7fff_ffff_ffff),
            "Unbounded native roster read"
        );
        let r = self.query(at)?;
        let allowed = if r.kind == MEM_IMAGE.0 {
            r.allocation == self.base() && matches!(r.protection, 2 | 4 | 8 | 0x20)
        } else {
            private(r)
        };
        ensure!(
            allowed && r.state == MEM_COMMIT.0 && contains(r.base, r.len, at, len),
            "Roster read escaped committed readable image/private data"
        );
        let value = self.reader.read(at, len)?;
        ensure!(value.len() == len, "Partial native roster read");
        self.checkpoint()?;
        Ok(value)
    }
    fn checkpoint(&self) -> Result<()> {
        if let Some(started) = self.recovery_started.get() {
            ensure!(
                started.elapsed() < Duration::from_secs(8),
                "Roster recovery exceeded 8-second budget"
            );
            return handle_matches(self.query.0, &self.binding.game);
        }
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Read-only roster request cancelled"
        );
        ensure!(
            self.started.elapsed() < BUDGET,
            "Read-only roster request exceeded12-second budget"
        );
        handle_matches(self.query.0, &self.binding.game)
    }
}
fn position(value: u64) -> &'static str {
    match value {
        0 => "PG",
        1 => "SG",
        2 => "SF",
        3 => "PF",
        4 => "C",
        _ => "—",
    }
}
fn birthday(y: u64, m: u64, d: u64) -> Value {
    if !(1900..=2100).contains(&y) || !(1..=12).contains(&m) {
        return Value::Null;
    }
    let days = match m {
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=days).contains(&d) {
        Value::Null
    } else {
        json!(format!("{y:04}-{m:02}-{d:02}"))
    }
}
fn player_document(value: &Value) -> Result<Value> {
    let meta = &value["metadata"];
    let number = |key: &str| {
        meta[key]
            .as_u64()
            .with_context(|| format!("Missing verified player metadata {key}"))
    };
    let (y, m, d) = (
        number("BirthYear")?,
        number("BirthMonth")?,
        number("BirthDay")?,
    );
    let jersey = number("JerseyNumber")?;
    let raw = value["ratingsRaw"]
        .as_array()
        .context("Verified raw fields missing")?;
    ensure!(raw.len() == 455, "Incomplete raw player fields");
    let potential = crate::roster_snapshot::rating_fields()
        .iter()
        .position(|f| f.key == "PotentialRaw")
        .and_then(|i| raw.get(i))
        .and_then(Value::as_u64)
        .context("Verified potential field missing")?;
    let mut out = json!({"source":"live-roster","nameStatus":"live-exact","readOnly":false,"writeAvailable":true,"recordIndex":value["index"],"recordAddress":null,
        "name":value["name"],"firstName":value["firstName"],"lastName":value["lastName"],"cyberfaceId":number("GraphicId")?,"uniqueId":number("UniqueId")?,
        "positionId":number("Position")?,"position":position(number("Position")?),"secondaryPositionId":number("SecondaryPosition")?,"secondaryPosition":position(number("SecondaryPosition")?),
        "jerseyNumberRaw":jersey,"jerseyNumber":if jersey==100 {"00".into()}else{jersey.to_string()},"bioLayoutVerified":true,"birthYear":y,"birthMonth":m,"birthDay":d,"birthDate":birthday(y,m,d),"yearsPro":number("YearsPro")?,
        "ratingsRaw":raw,"potentialRaw":potential,"potential":110u64.min(25+(potential+1)/3),"teamName":"","overallStatus":"not-verified-v2","clothing":{},"manifestListed":false,"modPresent":false,"referenceFaceIds":[]});
    out["editCapabilities"] = json!({"fullNameId":value["fullNameId"].as_i64().is_some_and(|id| (-1..=32767).contains(&id)),"heightCm":false,"wingspanCm":false,"fromName":false});
    // Live-proven player+0x60 team link (display only; addresses never leave native).
    out["teamIndex"] = value["teamIndex"].clone();
    out["teamName"] = json!(value["teamName"].as_str().unwrap_or(""));
    out["archetypeKey"] = value["archetypeKey"].clone();
    out["weightLbs"] = value["weightLbs"].clone();
    // First/last mappings are read-only, captured from the same native owner and
    // reprojected after a verified name edit. The scalar remains independently bound.
    if value["fullNameId"].is_i64() {
        out["audio"] = value
            .get("audio")
            .filter(|a| a.is_object())
            .cloned()
            .unwrap_or_else(
                || json!({"firstName":null,"lastName":null,"nameOverrideActive":false}),
            );
        out["audio"]["fullNameId"] = value["fullNameId"].clone();
    }
    for key in [
        "teamAddress",
        "heightRaw",
        "heightCm",
        "wingspanRaw",
        "wingspanCm",
        "overall",
        "cachedOverallRaw",
        "fromIndex",
        "collegeName",
    ] {
        out[key] = Value::Null;
    }
    for key in [
        "nativeOriginIndex",
        "nativeOriginCatalogId",
        "originState",
        "originSelectable",
        "originChoicesAvailable",
        "collegeName",
    ] {
        out[key] = value[key].clone();
    }
    out["editCapabilities"]["fromName"] = json!(
        value["nativeOriginCatalogId"]
            .as_str()
            .is_some_and(|id| id.len() == 64)
            && value["originChoicesAvailable"] == true
            && (value["originState"] == "unassigned"
                || (value["originState"] == "member"
                    && value["collegeName"].as_str().is_some_and(|s| !s.is_empty())))
    );
    Ok(out)
}
/// Stock uniform art listed in the game manifest (WPF manifestAssets.uniformBases:
/// `char/uniforms/<base>/<base>.iff`). Cached by file size and mtime.
fn uniform_bases(root: &Path) -> Result<Arc<std::collections::HashSet<String>>> {
    type Cache = Option<(
        PathBuf,
        u64,
        std::time::SystemTime,
        Arc<std::collections::HashSet<String>>,
    )>;
    static CACHE: Mutex<Cache> = Mutex::new(None);
    let path = root.join("manifest");
    let meta = std::fs::metadata(&path)?;
    ensure!(
        meta.is_file() && meta.len() <= 512 * 1024 * 1024,
        "Unsupported game manifest"
    );
    let modified = meta.modified()?;
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("Manifest cache unavailable"))?;
    if let Some((p, len, time, bases)) = cache.as_ref() {
        if *p == path && *len == meta.len() && *time == modified {
            return Ok(bases.clone());
        }
    }
    use std::io::BufRead;
    let mut bases = std::collections::HashSet::new();
    for line in std::io::BufReader::new(std::fs::File::open(&path)?).lines() {
        let line = line?;
        let logical = line
            .split(',')
            .next()
            .unwrap_or("")
            .replace('\\', "/")
            .to_ascii_lowercase();
        if let Some((base, file)) = logical
            .strip_prefix("char/uniforms/")
            .and_then(|r| r.split_once('/'))
        {
            if file.strip_suffix(".iff") == Some(base) {
                bases.insert(base.to_owned());
            }
        }
    }
    let bases = Arc::new(bases);
    *cache = Some((path, meta.len(), modified, bases.clone()));
    Ok(bases)
}
pub(crate) fn browser_document(snapshot: &model::Snapshot) -> Result<Value> {
    browser_document_with(snapshot, &std::collections::HashSet::new())
}
pub(crate) fn browser_document_with(
    snapshot: &model::Snapshot,
    stock_uniforms: &std::collections::HashSet<String>,
) -> Result<Value> {
    let small = snapshot.document();
    let players: Result<Vec<_>> = snapshot
        .cached_players()?
        .iter()
        .map(player_document)
        .collect();
    let arena_catalog = snapshot
        .arenas()
        .map(|a| json!({"arenas":a.clone_choices()}));
    let arena_options = arena_catalog
        .as_ref()
        .and_then(|v| v["arenas"].as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut a| {
            a["arenaId"] = a["id"].clone();
            a["arenaFile"] = a["model"].clone();
            if a["emptyCandidate"].is_null() {
                a["emptyCandidate"] = json!(false);
            }
            a
        })
        .collect::<Vec<_>>();
    let teams: Result<Vec<_>> = small["teams"].as_array().context("Verified teams missing")?.iter().map(|t| {
        let index = t["index"].as_u64().and_then(|n| usize::try_from(n).ok()).context("Invalid captured team index")?;
        let row = snapshot.row(Kind::Team, index)?;
        let year = writer::get(row, (0x1548, 19, 8))?;
        let mut value=json!({"source":"live-team","readOnly":false,"writeAvailable":true,"editCapabilities":{"infoFields":["name","cityName","cityShort","stateShort","mainLogo"],"arena":false,"rosterTypeId":t["kind"].as_u64().is_some_and(|k| k<=35),"historicYearId":false,"classicTeamEndYearRaw":year<=127},"recordIndex":t["index"],"recordAddress":null,"teamId":t["id"],"name":t["name"],"rosterTypeId":t["kind"],
            "classicTeamEndYearRaw":year,"historicYearId":0,"nativeTeamMetadataReadOnly":false,
            "cityName":t["cityName"],"cityShort":t["cityShort"],"stateShort":t["stateShort"],"mainLogo":t["mainLogo"],"historicYear":null,"arenaId":null,"arenaName":"","arenaFile":"","floorFile":"",
            "uniformSlotCapacity":t["uniformSlotCapacity"],"ownedUniformSlotCount":t["ownedUniformSlotCount"],"invalidUniformSlotCount":t["invalidUniformSlotCount"],"emptyUniformSlots":t["emptyUniformSlots"],"uniforms":t["uniforms"]});
        let current_pointer=u64::from_le_bytes(row[crate::roster_v2_arena::TEAM_POINTER..crate::roster_v2_arena::TEAM_POINTER+8].try_into()?);
        let current=snapshot.arenas().and_then(|a|a.witness_at(current_pointer).ok());
        value["nativeArenaId"]=json!(current.as_ref().map(|w|w.id()));
        value["nativeArenaCatalogId"]=json!(snapshot.arenas().map(|a|a.id()));
        value["arenaId"]=json!(0);
        value["editCapabilities"]["arena"]=json!(current.is_some()&&!arena_options.is_empty());
        if let Some(w)=current { if let Some(a)=arena_options.iter().find(|a|a["arenaId"].as_u64()==Some(w.id() as u64)) {value["arenaName"]=a["name"].clone();value["arenaFile"]=a["arenaFile"].clone();value["floorFile"]=a["floorFile"].clone();} }
        Ok(value)
    }).collect();
    let teams = teams?;
    let players = players?;
    let linked = players.iter().filter(|p| !p["teamIndex"].is_null()).count();
    let loose: Vec<Value> = snapshot
        .unreferenced_uniforms()?
        .into_iter()
        .map(|mut u| {
            let base = u["baseFilename"]
                .as_str()
                .unwrap_or("")
                .to_ascii_lowercase();
            u["stockResourceAvailable"] = json!(stock_uniforms.contains(&base));
            u
        })
        .collect();
    let uniforms: usize = teams
        .iter()
        .map(|t| t["uniforms"].as_array().map_or(0, Vec::len))
        .sum();
    Ok(
        json!({"schema":"nba2k27-live-player-research-v1","status":"ready","ok":true,"source":"version-2-owned","access":"guarded-write","readOnly":false,"writes":0,"writeAvailable":true,"gameplayVerified":false,
        "message":format!("{} verified players and {} teams · Version 2. Each editor enables only fields supported by its current native record. Saves recheck ownership and keep recovery evidence. Structural season edits require the offline Home menu; saved custom arenas apply once per owned Play session.",players.len(),teams.len()),
        "declaredPlayerCount":small["directoryCounts"]["players"],"decodedPlayerCount":players.len(),"totalCatalogEntries":players.len(),"teamCount":small["directoryCounts"]["teams"],"namedTeamCount":teams.len(),"playerStride":"0x4F8",
        "arenaCount":arena_options.len(),"arenaCatalogId":snapshot.arenas().map(|a|a.id()),"arenaLinkedTeamCount":teams.iter().filter(|t| !t["nativeArenaId"].is_null()).count(),"uniformRecordCount":uniforms,"verifiedTeamLinks":linked,"unreferencedPopulatedUniforms":loose,"players":players,"teams":teams,"arenas":arena_options,"origins":snapshot.origins().map(|o|o.choices()).unwrap_or_else(||json!([])),"unsupported":["selected private-history season structure","custom arena reapplication after an in-game roster reload"]}),
    )
}
/// One browser Save & verify request. The record comes from the request's
/// recordIndex and must exist in this tab's captured directory.
pub(crate) struct Save<'a> {
    pub kind: Kind,
    pub root: &'a Path,
    pub input: &'a Value,
    /// WPF team-uniform:link (one empty team slot pointer) instead of a field save.
    pub link: bool,
}
pub(crate) trait Captured: Send {
    fn document(&self) -> &Value;
    fn detail(&mut self, index: usize) -> Result<Value>;
    /// Returns the transaction report. On success the capture has adopted the
    /// verified read-back and refreshed its document.
    fn save(&mut self, _request: &Save<'_>) -> Result<Value> {
        anyhow::bail!("This Version 2 capture cannot save; no game write was attempted")
    }
    /// False after a committed write whose read-back could not be adopted.
    fn usable(&self) -> bool {
        true
    }
    /// Staff/season editor services (WPF team:staff, player:seasons).
    fn editor(&mut self, _request: &Editor<'_>) -> Result<Value> {
        anyhow::bail!("This Version 2 capture has no editor services; no game write was attempted")
    }
}
pub(crate) trait Engine: Send + Sync {
    fn capture(&self, binding: &Binding, stop: Arc<AtomicBool>) -> Result<Box<dyn Captured>>;
}
struct NativeEngine;
struct NativeCapture {
    access: Access,
    snapshot: model::Snapshot,
    document: Value,
    stale: bool,
    staff: Option<StaffSnapshot>,
    rotations: Option<RotationSnapshot>,
    seasons: std::collections::HashMap<String, SeasonEntry>,
    prepared_seasons: std::collections::HashMap<String, SeasonEntry>,
    structural_review: Option<structural::Review>,
    stock_uniforms: Arc<std::collections::HashSet<String>>,
}
fn model_text(row: &[u8], at: usize, len: usize) -> Result<String> {
    let words: Vec<u16> = row
        .get(at..at + len)
        .context("Incomplete record text")?
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0)
        .collect();
    Ok(String::from_utf16_lossy(&words).trim().to_owned())
}
fn directory(state: &profile::OwnerSnapshot, kind: Kind) -> Result<profile::Directory> {
    let d = match kind {
        Kind::Player => state.players.clone(),
        Kind::Team => state.teams.clone(),
        Kind::Uniform => state
            .uniforms
            .clone()
            .context("This roster has no uniform directory")?,
        Kind::Staff | Kind::Season => {
            anyhow::bail!("Staff and season directories are read per request")
        }
    };
    ensure!(d.stride == kind.stride(), "Roster directory stride differs");
    Ok(d)
}
fn record_address(
    state: &profile::OwnerSnapshot,
    d: &profile::Directory,
    kind: Kind,
    index: usize,
) -> Result<u64> {
    ensure!(index < d.count, "Record is outside its captured directory");
    let at = d
        .pointer
        .checked_add((index * kind.stride()) as u64)
        .context("Record address overflow")?;
    ensure!(
        at >= state.region.base
            && at + kind.stride() as u64 <= state.region.base + state.region.len,
        "Record is outside the owned roster allocation"
    );
    Ok(at)
}
/// Double-read one record through the checked reader (never a write handle).
fn read_record(access: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    let once = || -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(len);
        for offset in (0..len).step_by(4096) {
            access.checkpoint()?;
            let n = (len - offset).min(4096);
            let part = access.read(
                at.checked_add(offset as u64)
                    .context("Record address overflow")?,
                n,
            )?;
            ensure!(part.len() == n, "Incomplete record read");
            bytes.extend(part);
        }
        Ok(bytes)
    };
    let bytes = once()?;
    ensure!(bytes == once()?, "Record changed during read");
    Ok(bytes)
}
/// The only write capability: one record of one captured directory, checked
/// to be committed private PAGE_READWRITE memory inside the verified roster
/// allocation before every write. No image/code pages, no protection change.
struct WriteLease<'a> {
    access: &'a Access,
    handle: QueryHandle,
    state: &'a profile::OwnerSnapshot,
    directory: profile::Directory,
    kind: Kind,
    index: usize,
    record: u64,
    /// Live bytes that must stay exact for every ownership check (directory
    /// headers, the team slot link of a staff record, a player's season slot).
    guards: Vec<(u64, Vec<u8>)>,
    /// Every season Save retains fresh all-player link membership. This is
    /// read-only evidence and grants no additional writable memory.
    season_links: Option<Arc<Vec<u8>>>,
    retained: Vec<RecordWitness>,
    arena_dependency: Option<(
        Arc<crate::roster_v2_arena::Snapshot>,
        crate::roster_v2_arena::Selection,
    )>,
    origin_dependency: Option<(
        Arc<crate::roster_v2_origins::Snapshot>,
        crate::roster_v2_origins::Selection,
    )>,
    uniform_reference: Option<(usize, usize, u64)>,
    limited_season: bool,
}
impl<'a> WriteLease<'a> {
    fn open(
        access: &'a Access,
        state: &'a profile::OwnerSnapshot,
        kind: Kind,
        index: usize,
    ) -> Result<Self> {
        Self::open_in(
            access,
            state,
            kind,
            directory(state, kind)?,
            index,
            Vec::new(),
        )
    }
    fn open_in(
        access: &'a Access,
        state: &'a profile::OwnerSnapshot,
        kind: Kind,
        directory: profile::Directory,
        index: usize,
        guards: Vec<(u64, Vec<u8>)>,
    ) -> Result<Self> {
        access.checkpoint()?;
        ensure!(
            directory.stride == kind.stride(),
            "Roster directory stride differs"
        );
        ensure!(
            guards
                .iter()
                .all(|(at, b)| !b.is_empty() && b.len() <= 64 && *at >= 0x10000),
            "Invalid record guard"
        );
        let record = record_address(state, &directory, kind, index)?;
        let handle =
            QueryHandle(unsafe { OpenProcess(WRITE_RIGHTS, false, access.binding.game.pid)? });
        handle_matches(handle.0, &access.binding.game)?;
        let lease = Self {
            access,
            handle,
            state,
            directory,
            kind,
            index,
            record,
            guards,
            season_links: None,
            retained: Vec::new(),
            arena_dependency: None,
            origin_dependency: None,
            uniform_reference: None,
            limited_season: false,
        };
        lease.writable(record, kind.stride())?;
        Ok(lease)
    }
    fn writable(&self, mut at: u64, len: usize) -> Result<()> {
        let end = at + len as u64;
        while at < end {
            let r = self.access.query(at)?;
            ensure!(
                writable_region(r, self.state.region.base)
                    && contains(r.base, r.len, at, 1)
                    && at >= self.record
                    && end <= self.record + self.kind.stride() as u64,
                "Roster destination is not owned private read/write data"
            );
            at = (r.base + r.len).min(end);
        }
        Ok(())
    }
}
fn writable_region(r: Region, allocation: u64) -> bool {
    r.state == MEM_COMMIT.0
        && r.kind == MEM_PRIVATE.0
        && r.protection == PAGE_READWRITE.0
        && r.allocation == allocation
}
fn arena_allocation(access: &Access, state: &profile::OwnerSnapshot) -> Result<()> {
    let mut at = state.arenas.pointer;
    let size = state
        .arenas
        .count
        .checked_mul(crate::roster_v2_arena::STRIDE)
        .context("Arena directory overflow")?;
    let end = at
        .checked_add(size as u64)
        .context("Arena directory overflow")?;
    while at < end {
        let r = access.query(at)?;
        ensure!(
            private(r) && r.allocation == state.region.base && contains(r.base, r.len, at, 1),
            "Arena dependency left its private roster allocation"
        );
        let next = r
            .base
            .checked_add(r.len)
            .context("Arena region overflow")?
            .min(end);
        ensure!(next > at, "Arena region made no progress");
        at = next;
    }
    Ok(())
}
fn origin_allocation(
    access: &Access,
    state: &profile::OwnerSnapshot,
    catalog: &crate::roster_v2_origins::Snapshot,
) -> Result<()> {
    let (mut at, size) = catalog.span();
    let end = at
        .checked_add(size as u64)
        .context("Origin directory overflow")?;
    while at < end {
        let r = access.query(at)?;
        ensure!(
            private(r) && r.allocation == state.region.base && contains(r.base, r.len, at, 1),
            "Origin dependency left its private roster allocation"
        );
        let next = r
            .base
            .checked_add(r.len)
            .context("Origin region overflow")?
            .min(end);
        ensure!(next > at, "Origin region made no progress");
        at = next;
    }
    Ok(())
}
/// Read-only, typed full-record witness; no additional write capability.
struct RecordWitness {
    address: u64,
    bytes: Vec<u8>,
    // Only the exact owned link destination may move between before/after.
    owned_slot: Option<usize>,
}
fn retained_matches(before: &[u8], now: &[u8], owned_slot: Option<usize>) -> bool {
    if before.len() != now.len() {
        return false;
    }
    match owned_slot {
        None => before == now,
        Some(at) if at.checked_add(8).is_some_and(|end| end <= before.len()) => {
            before[..at] == now[..at] && before[at + 8..] == now[at + 8..]
        }
        _ => false,
    }
}
fn season_witness_matches(
    players: &[u8],
    expected_links: &[u8],
    player_offset: usize,
    player_row: &[u8],
    limited_index: Option<usize>,
) -> Result<()> {
    ensure!(
        season_link_projection(players)? == expected_links,
        "Player season links changed after review. Refresh before saving."
    );
    let end = player_offset
        .checked_add(player_row.len())
        .context("Player witness overflow")?;
    ensure!(
        players.get(player_offset..end) == Some(player_row),
        "The selected full player record changed"
    );
    if let Some(index) = limited_index {
        ensure!(
            services::season_reference_occurrences(players, index)? == 1,
            "Imported season has multiple logical references"
        );
    }
    Ok(())
}
fn uniform_references_valid(
    rows: &[u8],
    selected: usize,
    slot: usize,
    candidate: u64,
) -> Result<bool> {
    ensure!(
        rows.len() % Kind::Team.stride() == 0 && slot < writer::UNIFORM_SLOT_COUNT,
        "Invalid uniform reference witness"
    );
    ensure!(
        selected < rows.len() / Kind::Team.stride(),
        "Uniform team absent"
    );
    for (team, row) in rows.chunks_exact(Kind::Team.stride()).enumerate() {
        for i in 0..writer::UNIFORM_SLOT_COUNT {
            let at = writer::UNIFORM_SLOTS + i * 8;
            let p = u64::from_le_bytes(row[at..at + 8].try_into()?);
            if team == selected && i == slot {
                if p != 0 && p != candidate {
                    return Ok(false);
                }
            } else if p == candidate {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
impl writer::Access for WriteLease<'_> {
    fn owner(&self) -> Result<writer::Owner> {
        profile::revalidate(self.access, self.state)?;
        if let Some((catalog, selection)) = &self.arena_dependency {
            arena_allocation(self.access, self.state)?;
            catalog.revalidate_selection(self.access, self.state, selection)?;
            arena_allocation(self.access, self.state)?;
        }
        if let Some((catalog, selection)) = &self.origin_dependency {
            origin_allocation(self.access, self.state, catalog)?;
            selection.revalidate(self.access, self.state)?;
            origin_allocation(self.access, self.state, catalog)?;
        }
        for (at, expected) in &self.guards {
            ensure!(
                self.access.read(*at, expected.len())? == *expected,
                "The record's directory or owner link changed"
            );
        }
        if let Some(expected) = &self.season_links {
            let players = read_directory(self.access, &self.state.players)?;
            for witness in &self.retained {
                let offset = witness
                    .address
                    .checked_sub(self.state.players.pointer)
                    .context("Selected player is outside its directory")?
                    as usize;
                season_witness_matches(
                    &players,
                    expected,
                    offset,
                    &witness.bytes,
                    self.limited_season.then_some(self.index),
                )?;
            }
        }
        for witness in self.retained.iter().filter(|_| self.season_links.is_none()) {
            ensure!(
                retained_matches(
                    &witness.bytes,
                    &read_record(self.access, witness.address, witness.bytes.len())?,
                    witness.owned_slot
                ),
                "The selected full record changed. Refresh before saving."
            );
        }
        if let Some((team, slot, candidate)) = self.uniform_reference {
            let rows = read_directory(self.access, &self.state.teams)?;
            ensure!(
                uniform_references_valid(&rows, team, slot, candidate)?,
                "Uniform reference eligibility changed"
            );
        }
        let raw = self.capture()?;
        if self.limited_season {
            ensure!(
                services::season_reference_count(&raw)? == 1,
                "Imported season is no longer unshared"
            );
        }
        Ok(writer::Owner {
            game: self.access.binding.game.clone(),
            session: self.access.binding.session.clone(),
            image: self.access.base(),
            root: self.state.root,
            owner: self.state.owner,
            region_base: self.state.region.base,
            region_len: self.state.region.len,
            kind: self.kind,
            table: self.directory.pointer,
            count: self.directory.count,
            index: self.index,
            identity: writer::identity(self.kind, &raw)?,
            proof: writer::proof_tag().into(),
        })
    }
    fn capture(&self) -> Result<Vec<u8>> {
        read_record(self.access, self.record, self.kind.stride())
    }
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        ensure!(
            !bytes.is_empty()
                && bytes.len() <= 128
                && offset
                    .checked_add(bytes.len())
                    .is_some_and(|end| end <= self.kind.stride()),
            "Roster write is outside its owned record"
        );
        let at = self.record + offset as u64;
        self.writable(at, bytes.len())?;
        handle_matches(self.handle.0, &self.access.binding.game)?;
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
            "Partial roster write; transaction verification required"
        );
        Ok(())
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            self.access.recovery_started.get().is_none(),
            "Forward checkpoint refused after recovery began"
        );
        self.access.checkpoint()
    }
    fn begin_recovery(&self) -> Result<()> {
        self.access.begin_recovery()
    }
}
fn request_index(input: &Value, max: f64) -> Result<usize> {
    let n = input["recordIndex"]
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0. && (0. ..=max).contains(n))
        .context("Invalid record index")?;
    Ok(n as usize)
}
fn find<'v>(rows: &'v Value, key: &str, index: usize) -> Result<&'v Value> {
    rows.as_array()
        .context("Snapshot list missing")?
        .iter()
        .find(|r| r[key].as_u64() == Some(index as u64))
        .context("This record is absent from the tab's snapshot; refresh the roster")
}
fn season_link_projection(bytes: &[u8]) -> Result<Vec<u8>> {
    let stride = Kind::Player.stride();
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_ALLOCATION as usize && bytes.len() % stride == 0,
        "Invalid bounded player link snapshot"
    );
    let mut projection = Vec::with_capacity(bytes.len() / stride * 74);
    for row in bytes.chunks_exact(stride) {
        projection.extend_from_slice(
            &row[services::PLAYER_PRIVATE_SEASONS..services::PLAYER_PRIVATE_SEASONS + 8],
        );
        projection.extend_from_slice(&row[services::PLAYER_SEASONS..services::PLAYER_SEASONS + 66]);
    }
    Ok(projection)
}
fn read_season_link_projection(m: &impl Memory, directory: &profile::Directory) -> Result<Vec<u8>> {
    ensure!(
        directory.stride == Kind::Player.stride(),
        "Player link stride changed"
    );
    season_link_projection(&read_directory(m, directory)?)
}
fn read_directory(m: &impl Memory, directory: &profile::Directory) -> Result<Vec<u8>> {
    let len = directory
        .count
        .checked_mul(directory.stride)
        .context("Player link size overflow")?;
    ensure!(
        len > 0 && len <= MAX_ALLOCATION as usize,
        "Player link snapshot exceeds bounds"
    );
    let mut bytes = Vec::with_capacity(len);
    for offset in (0..len).step_by(4096) {
        m.checkpoint()?;
        let count = (len - offset).min(4096);
        let at = directory
            .pointer
            .checked_add(offset as u64)
            .context("Player link address overflow")?;
        let part = m.read(at, count)?;
        ensure!(part.len() == count, "Incomplete player link snapshot");
        bytes.extend_from_slice(&part);
    }
    Ok(bytes)
}
fn editor_writes(method: &str, input: &Value) -> bool {
    match method {
        "saveTeamStaff" | "cloneArena" => true,
        "playerSeasons" => !matches!(
            input["action"].as_str(),
            Some("read" | "teams" | "review-existing-import" | "review-import")
        ),
        "teamRotations" => input["action"] != "read",
        _ => false,
    }
}
fn season_token(seed: &str, row: &[u8], raw: &[u8], import_links: Option<&[u8]>) -> String {
    let mut hash = sha2::Sha256::new();
    hash.update(if import_links.is_some() {
        b"v2-season-import-v1".as_slice()
    } else {
        b"v2-season-read-v1".as_slice()
    });
    hash.update(seed.as_bytes());
    hash.update(row);
    hash.update(raw);
    if let Some(links) = import_links {
        hash.update(sha2::Sha256::digest(links));
    }
    format!("{:x}", hash.finalize())
}
impl NativeCapture {
    /// WPF linkTeamUniform: one 8-byte pointer into an empty slot of this team,
    /// pointing at an unreferenced same-team uniform with stock art.
    fn link_uniform(&mut self, request: &Save<'_>) -> Result<Value> {
        let state = self.snapshot.owner().clone();
        let edit = writer::validate_uniform_link(request.input)?;
        let team = find(&self.document["teams"], "recordIndex", edit.record_index)?.clone();
        let candidate = find(
            &self.document["unreferencedPopulatedUniforms"],
            "recordIndex",
            edit.uniform_record_index,
        )?
        .clone();
        let base = edit.expected_base_filename.to_ascii_lowercase();
        let stock = candidate["stockResourceAvailable"] == true
            && uniform_bases(&self.access.binding.root).is_ok_and(|b| b.contains(&base));
        let uniforms = directory(&state, Kind::Uniform)?;
        let address =
            writer::uniform_address(uniforms.pointer, uniforms.count, edit.uniform_record_index)?;
        let uniform_raw = read_record(&self.access, address, Kind::Uniform.stride())?;
        let header = self.access.read(state.owner + 0x290, 16)?;
        ensure!(
            u64::from_le_bytes(header[..8].try_into()?) == uniforms.count as u64
                && u64::from_le_bytes(header[8..].try_into()?) == uniforms.pointer,
            "The uniform directory changed. Refresh teams."
        );
        let teams = directory(&state, Kind::Team)?;
        let mut referenced = false;
        for i in 0..teams.count {
            let slots = self.access.read(
                teams.pointer + (i * Kind::Team.stride() + writer::UNIFORM_SLOTS) as u64,
                writer::UNIFORM_SLOT_COUNT * 8,
            )?;
            referenced |= slots
                .chunks_exact(8)
                .any(|p| u64::from_le_bytes(p.try_into().unwrap()) == address);
        }
        let mut lease = WriteLease::open_in(
            &self.access,
            &state,
            Kind::Team,
            teams,
            edit.record_index,
            vec![
                (state.owner + 0x290, header),
                (address + 8, uniform_raw[8..72].to_vec()),
                (address + 0x50, uniform_raw[0x50..0x54].to_vec()),
            ],
        )?;
        let team_raw = writer::Access::capture(&lease)?;
        let plan = writer::plan_uniform_link(
            &edit,
            &team,
            &candidate,
            &team_raw,
            &uniform_raw,
            address,
            referenced,
            stock,
        )?;
        lease.retained.push(RecordWitness {
            address: lease.record,
            bytes: team_raw,
            owned_slot: Some(writer::UNIFORM_SLOTS + edit.slot * 8),
        });
        lease.uniform_reference = Some((edit.record_index, edit.slot, address));
        let owner = writer::Access::owner(&lease)?;
        let commit = writer::apply(&lease, &owner, &plan, |i| writer::save(request.root, i))?;
        drop(lease);
        match self
            .snapshot
            .replace(Kind::Team, edit.record_index, &commit.read_back)
            .and_then(|_| browser_document_with(&self.snapshot, &self.stock_uniforms))
        {
            Ok(document) => self.document = document,
            Err(e) => {
                self.stale = true;
                anyhow::bail!(
                    "The uniform was linked and verified, but the editor could not reload it ({e:#}). Refresh live teams."
                );
            }
        }
        Ok(
            json!({"ok":true,"schema":"nba2k27-team-uniform-link-v1","gameCodeHooks":0,"linkReadBackVerified":true,"rollbackOnFailure":true,
            "physicalWrites":plan.patches.len(),"restoredToNull":false,"recordIndex":edit.record_index,"teamId":edit.team_id,"slot":edit.slot,
            "uniformRecordIndex":edit.uniform_record_index,"uniformId":edit.uniform_id,"baseFilename":edit.expected_base_filename}),
        )
    }
    fn commit(&mut self, request: &Save<'_>) -> Result<Value> {
        if request.link {
            return self.link_uniform(request);
        }
        let measurement = |key: &str| -> Result<Option<f64>> {
            match request.input.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => Ok(Some(
                    value
                        .as_f64()
                        .filter(|v| v.is_finite())
                        .with_context(|| format!("Invalid {key}"))?,
                )),
            }
        };
        let measurements = if request.kind == Kind::Player {
            (measurement("heightCm")?, measurement("wingspanCm")?)
        } else {
            (None, None)
        };
        let mut ordinary_input = request.input.clone();
        if request.kind == Kind::Player {
            let object = ordinary_input
                .as_object_mut()
                .context("Expected player request")?;
            object.remove("heightCm");
            object.remove("wingspanCm");
        }
        let ordinary_request = Save {
            kind: request.kind,
            root: request.root,
            input: &ordinary_input,
            link: request.link,
        };
        let request = &ordinary_request;
        let state = self.snapshot.owner().clone();
        let root = request.root;
        let (index, plan, lease) = match request.kind {
            Kind::Player => {
                let index = request_index(request.input, 8191.)?;
                let cached = find(&self.document["players"], "recordIndex", index)?;
                let mut lease = WriteLease::open(&self.access, &state, Kind::Player, index)?;
                let raw = writer::Access::capture(&lease)?;
                let has_index = request.input.get("nativeOriginIndex").is_some();
                let has_catalog = request.input.get("nativeOriginCatalogId").is_some();
                ensure!(
                    has_index == has_catalog,
                    "Origin selection requires its native catalog identity"
                );
                let selection = if has_index {
                    ensure!(
                        cached["editCapabilities"]["fromName"] == true,
                        "Current player origin is unavailable; refresh before saving"
                    );
                    let target = request.input["nativeOriginIndex"]
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .context("Invalid native origin index")?;
                    let token = request.input["nativeOriginCatalogId"]
                        .as_str()
                        .context("Missing native origin catalog identity")?;
                    ensure!(
                        cached["nativeOriginCatalogId"].as_str() == Some(token),
                        "Origin selection belongs to another editor snapshot"
                    );
                    let catalog = self
                        .snapshot
                        .origins()
                        .context("Native origin catalog is unavailable")?
                        .clone();
                    ensure!(
                        catalog.id() == token,
                        "Origin catalog changed; refresh before saving"
                    );
                    origin_allocation(&self.access, &state, &catalog)?;
                    catalog.revalidate(&self.access, &state)?;
                    let selected = catalog.prepare_change(
                        self.snapshot.row(Kind::Player, index)?,
                        &raw,
                        target,
                    )?;
                    selected.validate_catalog(token)?;
                    selected.revalidate(&self.access, &state)?;
                    origin_allocation(&self.access, &state, &catalog)?;
                    lease.origin_dependency = Some((catalog, selected.clone()));
                    Some(selected)
                } else {
                    None
                };
                (
                    index,
                    writer::plan_player_with_origin(
                        request.input,
                        cached,
                        &raw,
                        selection.as_ref(),
                    )?,
                    lease,
                )
            }
            Kind::Team => {
                let index = request_index(request.input, 1023.)?;
                let cached = find(&self.document["teams"], "recordIndex", index)?;
                let mut lease = WriteLease::open(&self.access, &state, Kind::Team, index)?;
                let raw = writer::Access::capture(&lease)?;
                let has_id = request.input.get("nativeArenaId").is_some();
                let has_token = request.input.get("nativeArenaCatalogId").is_some();
                ensure!(
                    has_id == has_token,
                    "Arena selection requires its native catalog identity"
                );
                let selection = if has_id {
                    ensure!(
                        cached["editCapabilities"]["arena"] == true,
                        "Current team arena is not uniquely available; refresh teams"
                    );
                    ensure!(
                        raw == self.snapshot.row(Kind::Team, index)?,
                        "Selected team changed since its arena catalog was captured"
                    );
                    let id = request.input["nativeArenaId"]
                        .as_u64()
                        .filter(|n| *n <= u16::MAX as u64)
                        .context("Invalid native arena ID")?;
                    let token = request.input["nativeArenaCatalogId"]
                        .as_str()
                        .context("Missing native arena catalog identity")?;
                    ensure!(
                        cached["nativeArenaCatalogId"].as_str() == Some(token),
                        "Arena selection belongs to another editor snapshot"
                    );
                    let catalog = self
                        .snapshot
                        .arenas()
                        .context("Native arena catalog is unavailable")?
                        .clone();
                    let pointer = u64::from_le_bytes(
                        raw[crate::roster_v2_arena::TEAM_POINTER
                            ..crate::roster_v2_arena::TEAM_POINTER + 8]
                            .try_into()?,
                    );
                    let selected = catalog.selection(pointer, id as u16, token)?;
                    arena_allocation(&self.access, &state)?;
                    catalog.revalidate(&self.access, &state)?; // Fresh full-catalog/unique-ID admission.
                    catalog.revalidate_selection(&self.access, &state, &selected)?;
                    lease.arena_dependency = Some((catalog, selected.clone()));
                    Some(selected)
                } else {
                    None
                };
                (
                    index,
                    writer::plan_team_with_arena(request.input, cached, &raw, selection.as_ref())?,
                    lease,
                )
            }
            Kind::Uniform => {
                let team_index = request_index(request.input, 1023.)?;
                let team = find(&self.document["teams"], "recordIndex", team_index)?;
                let slot = request.input["slot"]
                    .as_u64()
                    .context("Invalid uniform slot")?;
                let uniform = team["uniforms"]
                    .as_array()
                    .context("Uniform list missing")?
                    .iter()
                    .find(|u| u["slot"].as_u64() == Some(slot))
                    .context("Uniform is absent from this tab's team slot")?;
                let index = uniform["recordIndex"]
                    .as_u64()
                    .context("Uniform record missing")? as usize;
                let uniforms = directory(&state, Kind::Uniform)?;
                let address = writer::uniform_address(uniforms.pointer, uniforms.count, index)?;
                let teams = directory(&state, Kind::Team)?;
                let team_raw = read_record(
                    &self.access,
                    record_address(&state, &teams, Kind::Team, team_index)?,
                    Kind::Team.stride(),
                )?;
                let mut lease = WriteLease::open(&self.access, &state, Kind::Uniform, index)?;
                let raw = writer::Access::capture(&lease)?;
                let (_, plan) = writer::plan_uniform(
                    request.input,
                    team,
                    uniform,
                    &team_raw,
                    &raw,
                    address,
                    index,
                )?;
                lease.retained.push(RecordWitness {
                    address: record_address(&state, &teams, Kind::Team, team_index)?,
                    bytes: team_raw,
                    owned_slot: None,
                });
                (index, plan, lease)
            }
            Kind::Staff | Kind::Season => {
                anyhow::bail!("Staff and season saves use their own editor services")
            }
        };
        let owner = writer::Access::owner(&lease)?;
        let commit = if measurements.0.is_some() || measurements.1.is_some() {
            appearance_access::commit(root, &lease, &owner, &plan, measurements.0, measurements.1)?
        } else {
            writer::apply(&lease, &owner, &plan, |i| writer::save(root, i))?
        };
        drop(lease);
        let adopted = self
            .snapshot
            .replace(request.kind, index, &commit.read_back)
            .and_then(|_| browser_document_with(&self.snapshot, &self.stock_uniforms));
        match adopted {
            Ok(document) => self.document = document,
            Err(e) => {
                self.stale = true;
                anyhow::bail!(
                    "The change was saved and verified, but the editor could not reload it ({e:#}). Refresh the Version 2 roster before another edit."
                );
            }
        }
        let mut report = commit.report;
        if request.kind == Kind::Team {
            report["teamId"] = json!(writer::get(&commit.read_back, (writer::TEAM_ID, 0, 16))?);
        }
        Ok(report)
    }
}
/// Team staff read snapshot (WPF TeamStaffService.snapshot): the exact rows
/// a later save must still match.
struct StaffSnapshot {
    team_index: usize,
    team_id: u64,
    name: String,
    team_address: u64,
    team_hash: [u8; 32],
    team_raw: Vec<u8>,
    header: Vec<u8>,
    table: u64,
    count: usize,
    rows: Vec<(usize, usize, Vec<u8>, bool)>,
}
/// Team rotation read snapshot (WPF TeamRotationService.snapshot).
struct RotationSnapshot {
    team_index: usize,
    team_id: u64,
    name: String,
    token: String,
    raw: Vec<u8>,
    team_hash: [u8; 32],
    members: Vec<(u16, usize, String)>,
    member_digest: String,
}
/// One read season row (WPF season cache entry keyed by token).
#[derive(Clone)]
struct SeasonEntry {
    player: usize,
    identity: Value,
    slot: usize,
    index: usize,
    raw: Vec<u8>,
    links: Vec<usize>,
    header: Vec<u8>,
    table: u64,
    count: usize,
    import_links: Option<Arc<Vec<u8>>>,
    player_row: Vec<u8>,
    limited: bool,
}
fn adopt_prepared<T>(
    cache: &mut std::collections::HashMap<String, T>,
    prepared: std::collections::HashMap<String, T>,
) -> Result<()> {
    let added = prepared
        .keys()
        .filter(|key| !cache.contains_key(*key))
        .count();
    ensure!(
        cache.len() + added <= 50,
        "Prepared season review limit reached. Reopen the roster tab before another import review."
    );
    cache.extend(prepared);
    Ok(())
}
fn adopt_successful_review<T>(
    cache: &mut std::collections::HashMap<String, T>,
    entries: std::collections::HashMap<String, T>,
    review: Result<Value>,
) -> Result<Value> {
    let review = review?;
    adopt_prepared(cache, entries)?;
    Ok(review)
}
/// One browser editor service call routed to the Version 2 capture.
pub(crate) struct Editor<'a> {
    pub method: &'a str,
    pub root: &'a Path,
    pub input: &'a Value,
    pub confirm: &'a dyn Fn(&str) -> bool,
    pub write_gate: Option<&'a session_safety::WriteGuard>,
}
fn int(input: &Value, key: &str, max: u64) -> Result<u64> {
    input[key]
        .as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0. && *n >= 0. && *n <= max as f64)
        .map(|n| n as u64)
        .with_context(|| format!("Invalid {key}"))
}
fn table_header(
    access: &Access,
    state: &profile::OwnerSnapshot,
    offset: u64,
    stride: usize,
    max: usize,
) -> Result<(Vec<u8>, u64, usize)> {
    let header = access.read(state.owner + offset, 16)?;
    let count = u64::from_le_bytes(header[..8].try_into()?);
    let table = u64::from_le_bytes(header[8..].try_into()?);
    ensure!(
        (1..=max as u64).contains(&count)
            && table % 8 == 0
            && table >= state.region.base + 0x680
            && table
                .checked_add(count * stride as u64)
                .is_some_and(|end| end <= state.region.base + state.region.len),
        "The staff/season directory is outside the owned roster allocation"
    );
    Ok((header, table, count as usize))
}
fn digest(bytes: &[u8]) -> [u8; 32] {
    sha2::Sha256::digest(bytes).into()
}
impl NativeCapture {
    fn begin_service(&mut self) -> Result<profile::OwnerSnapshot> {
        ensure!(!self.stale, "Refresh the Version 2 roster first");
        self.access.start()?;
        services::verify_profile(&self.access)?;
        profile::revalidate(&self.access, self.snapshot.owner())?;
        Ok(self.snapshot.owner().clone())
    }
    fn live_team(
        &self,
        state: &profile::OwnerSnapshot,
        input: &Value,
    ) -> Result<(usize, u64, String, u64, Vec<u8>)> {
        let index = int(input, "recordIndex", 1023)? as usize;
        let team_id = int(input, "teamId", 65535)?;
        let name = input["expectedName"]
            .as_str()
            .context("Invalid staff inspection request.")?
            .to_owned();
        let cached = find(&self.document["teams"], "recordIndex", index)?;
        ensure!(
            cached["source"] == "live-team"
                && cached["teamId"].as_u64() == Some(team_id)
                && cached["name"] == name.as_str(),
            "Load this team’s roster and refresh live teams first."
        );
        let teams = directory(state, Kind::Team)?;
        let address = record_address(state, &teams, Kind::Team, index)?;
        let raw = read_record(&self.access, address, Kind::Team.stride())?;
        ensure!(
            writer::get(&raw, (writer::TEAM_ID, 0, 16))? == team_id
                && model_text(&raw, 0x2FA, 48)? == name,
            "The selected team changed. Refresh before inspecting staff."
        );
        Ok((index, team_id, name, address, raw))
    }
    /// WPF RunTeamStaff (read) + TeamStaffService.read. Shared records (staff+0x18
    /// names another team) are listed but saved only from the owning team.
    fn team_staff(&mut self, input: &Value) -> Result<Value> {
        let state = self.begin_service()?;
        let (team_index, team_id, name, address, team) = self.live_team(&state, input)?;
        let (header, table, count) = table_header(
            &self.access,
            &state,
            services::STAFF_HEADER,
            services::STAFF_STRIDE,
            8192,
        )?;
        let teams = directory(&state, Kind::Team)?;
        let mut rows = Vec::new();
        let mut staff = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for slot in 0..32 {
            let at = services::TEAM_STAFF_SLOTS + slot * 8;
            let pointer = u64::from_le_bytes(team[at..at + 8].try_into()?);
            if pointer == 0 {
                continue;
            }
            let delta = pointer
                .checked_sub(table)
                .context("A team staff slot is not a staff record.")?;
            ensure!(
                delta % services::STAFF_STRIDE as u64 == 0
                    && delta / (services::STAFF_STRIDE as u64) < count as u64
                    && seen.insert(pointer),
                "A team staff slot is not a unique aligned staff record."
            );
            let index = (delta / services::STAFF_STRIDE as u64) as usize;
            let raw = read_record(&self.access, pointer, services::STAFF_STRIDE)?;
            ensure!(
                (raw[0x182] & 31) as usize == slot,
                "A staff role does not match its team slot."
            );
            let owner = u64::from_le_bytes(
                raw[services::STAFF_OWNER..services::STAFF_OWNER + 8].try_into()?,
            );
            let owned = owner == address;
            let mut value = services::decode_staff(&raw, slot, index)?;
            value["shared"] = json!(!owned);
            if !owned {
                let other = owner
                    .checked_sub(teams.pointer)
                    .filter(|d| d % Kind::Team.stride() as u64 == 0)
                    .map(|d| (d / Kind::Team.stride() as u64) as usize)
                    .filter(|i| *i < teams.count)
                    .and_then(|i| find(&self.document["teams"], "recordIndex", i).ok())
                    .and_then(|t| t["name"].as_str().map(str::to_owned))
                    .unwrap_or_else(|| "another team".into());
                value["ownerTeam"] = json!(other);
            }
            staff.push(value);
            rows.push((slot, index, raw, owned));
        }
        profile::revalidate(&self.access, &state)?;
        ensure!(
            self.access.read(state.owner + services::STAFF_HEADER, 16)? == header
                && read_record(&self.access, address, Kind::Team.stride())? == team,
            "The roster changed during staff inspection. Refresh and try again."
        );
        let shared = rows.iter().filter(|r| !r.3).count();
        self.staff = Some(StaffSnapshot {
            team_index,
            team_id,
            name,
            team_address: address,
            team_hash: digest(&team),
            team_raw: team,
            header,
            table,
            count,
            rows,
        });
        self.access.finish()?;
        let mut status = "Stage mapped settings, then Save & verify. IDs and assignment links stay unchanged; save the roster in-game to retain edits. Raw attribute and badge levels are shown without tier conversion.".to_owned();
        if shared > 0 {
            status.push_str(&format!(" {shared} staff member(s) are shared with another team and can only be saved from that team."));
        }
        Ok(json!({"staff":staff,"readOnly":true,"writes":0,"editingEnabled":true,"status":status}))
    }
    /// WPF TeamStaffService.save + RunTeamStaff --team-staff-write, as one
    /// guarded roster_v2_write transaction.
    fn save_team_staff(&mut self, input: &Value, root: &Path) -> Result<Value> {
        let state = self.begin_service()?;
        let s = self
            .staff
            .as_ref()
            .context("Refresh this team’s staff before saving.")?;
        ensure!(
            input["recordIndex"].as_u64() == Some(s.team_index as u64)
                && input["teamId"].as_u64() == Some(s.team_id)
                && input["expectedName"] == s.name.as_str(),
            "Refresh this team’s staff before saving."
        );
        let slot = int(input, "slot", 31)? as usize;
        let index = int(input, "staffIndex", 8191)? as usize;
        let (_, _, snapshot, owned) = s
            .rows
            .iter()
            .find(|r| r.0 == slot && r.1 == index)
            .context("The selected staff assignment changed.")?;
        ensure!(
            self.access.read(state.owner + services::STAFF_HEADER, 16)? == s.header
                && digest(&read_record(
                    &self.access,
                    s.team_address,
                    Kind::Team.stride()
                )?) == s.team_hash,
            "The staff directory or team changed. Refresh staff."
        );
        let record = s.table + (index * services::STAFF_STRIDE) as u64;
        let link = s.team_address + (services::TEAM_STAFF_SLOTS + slot * 8) as u64;
        let mut lease = WriteLease::open_in(
            &self.access,
            &state,
            Kind::Staff,
            profile::Directory {
                count: s.count,
                pointer: s.table,
                stride: services::STAFF_STRIDE,
            },
            index,
            vec![
                (state.owner + services::STAFF_HEADER, s.header.clone()),
                (link, record.to_le_bytes().to_vec()),
            ],
        )?;
        lease.retained.push(RecordWitness {
            address: s.team_address,
            bytes: s.team_raw.clone(),
            owned_slot: None,
        });
        let live = writer::Access::capture(&lease)?;
        let plan = services::plan_staff(input, snapshot, &live, index, slot, *owned)?;
        let owner = writer::Access::owner(&lease)?;
        let commit = writer::apply(&lease, &owner, &plan, |i| writer::save(root, i))?;
        drop(lease);
        let writes: usize = plan.patches.iter().map(|p| p.after.len()).sum();
        if let Some(row) = self
            .staff
            .as_mut()
            .and_then(|s| s.rows.iter_mut().find(|r| r.0 == slot && r.1 == index))
        {
            row.2 = commit.read_back;
        }
        if self.access.finish().is_err() {
            self.staff = None;
        }
        Ok(
            json!({"ok":true,"schema":"nba2k27-team-staff-write-v1","saved":true,"writes":writes,"readBackVerified":true,
            "recordIndex":input["recordIndex"],"teamId":input["teamId"],"staffIndex":index,"noChanges":plan.patches.is_empty()}),
        )
    }
    fn player_identity(&self, input: &Value) -> Result<(usize, Value)> {
        let index = int(input, "recordIndex", 8191)? as usize;
        let identity = json!({"recordIndex":index,"firstName":input["firstName"],"lastName":input["lastName"],"cyberfaceId":input["cyberfaceId"]});
        let cached = find(&self.document["players"], "recordIndex", index)?;
        ensure!(
            cached["firstName"] == identity["firstName"]
                && cached["lastName"] == identity["lastName"]
                && cached["cyberfaceId"] == identity["cyberfaceId"],
            "The player changed. Refresh the roster."
        );
        Ok((index, identity))
    }
    fn team_name(&self, state: &profile::OwnerSnapshot, pointer: u64) -> Value {
        if pointer == 0 {
            return Value::Null;
        }
        pointer
            .checked_sub(state.teams.pointer)
            .filter(|d| d % Kind::Team.stride() as u64 == 0)
            .map(|d| (d / Kind::Team.stride() as u64) as usize)
            .filter(|i| *i < state.teams.count)
            .and_then(|i| self.snapshot.row(Kind::Team, i).ok())
            .and_then(|row| model_text(row, 0x2FA, 48).ok())
            .map_or(json!("Unknown team"), |n| json!(n))
    }
    /// WPF SeasonStats.cs read: the 25 regular-season slots of one player.
    fn read_seasons(&mut self, input: &Value) -> Result<Value> {
        let (rows, entries) = self.read_seasons_for_review(input, false)?;
        self.seasons = entries;
        Ok(rows)
    }
    fn read_seasons_for_review(
        &mut self,
        input: &Value,
        import: bool,
    ) -> Result<(Value, std::collections::HashMap<String, SeasonEntry>)> {
        let state = self.begin_service()?;
        let (player, identity) = self.player_identity(input)?;
        let players = directory(&state, Kind::Player)?;
        let at = record_address(&state, &players, Kind::Player, player)?;
        let row = read_record(&self.access, at, Kind::Player.stride())?;
        ensure!(
            model_text(&row, 40, 40)? == identity["firstName"].as_str().unwrap_or("")
                && model_text(&row, 0, 40)? == identity["lastName"].as_str().unwrap_or("")
                && Some(writer::get(&row, (0x110, 0, 16))?) == identity["cyberfaceId"].as_u64(),
            "The player changed. Refresh the roster."
        );
        ensure!(
            row[services::PLAYER_PRIVATE_SEASONS..services::PLAYER_PRIVATE_SEASONS + 8] == [0; 8],
            "Private season histories are not supported yet."
        );
        // Both ordinary warning and limited-import eligibility need fresh links;
        // cached other-player rows cannot establish current membership.
        let live = Some(self.live_players(&state)?);
        let import_links = live
            .as_ref()
            .map(|bytes| season_link_projection(bytes).map(Arc::new))
            .transpose()?;
        let (header, table, count) = table_header(
            &self.access,
            &state,
            services::SEASON_HEADER,
            services::SEASON_STRIDE,
            1_000_000,
        )?;
        let mut rows = Vec::new();
        let mut entries = std::collections::HashMap::new();
        for slot in 0..services::REGULAR_SEASONS {
            let at = services::PLAYER_SEASONS + slot * 2;
            let index = i16::from_le_bytes([row[at], row[at + 1]]);
            if index < 0 {
                continue;
            }
            let index = index as usize;
            ensure!(index < count, "Invalid season index.");
            let raw = read_record(
                &self.access,
                table + (index * services::SEASON_STRIDE) as u64,
                services::SEASON_STRIDE,
            )?;
            // Linked entries: every captured player without a private history
            // whose 33 season slots reference this record (WPF Links()).
            let mut links = Vec::new();
            for p in 0..players.count {
                let r = if let Some(bytes) = &live {
                    &bytes[p * Kind::Player.stride()..(p + 1) * Kind::Player.stride()]
                } else if p == player {
                    &row[..]
                } else {
                    self.snapshot.row(Kind::Player, p)?
                };
                if r[services::PLAYER_PRIVATE_SEASONS..services::PLAYER_PRIVATE_SEASONS + 8]
                    != [0; 8]
                {
                    continue;
                }
                if (0..33).any(|s| {
                    let o = services::PLAYER_SEASONS + s * 2;
                    i16::from_le_bytes([r[o], r[o + 1]]) as i64 == index as i64
                }) {
                    links.push(p);
                }
            }
            let seed = format!(
                "{}:{}:{}:{}:{}:{}:{:?}",
                self.access.binding.session, state.owner, table, player, slot, index, links
            );
            let token = season_token(
                &seed,
                &row,
                &raw,
                if import {
                    import_links.as_ref().map(|links| links.as_slice())
                } else {
                    None
                },
            );
            let mut public = services::season_row(
                slot,
                &raw,
                json!(
                    self.team_name(&state, u64::from_le_bytes(raw[0..8].try_into()?))
                        .as_str()
                        .unwrap_or("—")
                ),
                self.team_name(&state, u64::from_le_bytes(raw[8..16].try_into()?)),
                &token,
                &links,
            )?;
            if import {
                let index_of = |pointer: u64| -> Result<Value> {
                    if pointer == 0 {
                        return Ok(Value::Null);
                    }
                    let offset = pointer
                        .checked_sub(state.teams.pointer)
                        .context("Season team is outside the captured directory")?;
                    ensure!(
                        offset % Kind::Team.stride() as u64 == 0,
                        "Season team is not a roster record"
                    );
                    let index = offset / Kind::Team.stride() as u64;
                    ensure!(
                        index < state.teams.count as u64,
                        "Season team index is unavailable"
                    );
                    Ok(json!(index))
                };
                public["primaryTeamIndex"] = index_of(u64::from_le_bytes(raw[0..8].try_into()?))?;
                public["secondaryTeamIndex"] =
                    index_of(u64::from_le_bytes(raw[8..16].try_into()?))?;
                public["linkedCount"] = json!(links.len());
                let occurrences =
                    services::season_reference_occurrences(live.as_ref().unwrap(), index)?;
                // Existing limited import admits exactly one logical slot and native reference.
                if occurrences != 1 || services::season_reference_count(&raw)? != 1 {
                    public["linkedCount"] = json!(0);
                }
            }
            rows.push(public);
            entries.insert(
                token,
                SeasonEntry {
                    player,
                    identity: identity.clone(),
                    slot,
                    index,
                    raw,
                    links,
                    header: header.clone(),
                    table,
                    count,
                    import_links: import_links.clone(),
                    player_row: row.clone(),
                    limited: import,
                },
            );
        }
        profile::revalidate(&self.access, &state)?;
        if let Some(before) = &live {
            ensure!(
                season_link_projection(before)?
                    == season_link_projection(&self.live_players(&state)?)?,
                "Season links changed during import review. Refresh before importing."
            );
        }
        ensure!(
            read_record(&self.access, at, Kind::Player.stride())? == row
                && self
                    .access
                    .read(state.owner + services::SEASON_HEADER, 16)?
                    == header,
            "The roster changed. Refresh season stats."
        );
        // Stable raw rows, not only a stable selected-player directory.
        for e in entries.values() {
            ensure!(
                read_record(
                    &self.access,
                    table + (e.index * services::SEASON_STRIDE) as u64,
                    services::SEASON_STRIDE
                )? == e.raw,
                "Season row changed during inspection"
            );
        }
        self.access.finish()?;
        Ok((json!(rows), entries))
    }
    fn season_import_teams(&self) -> Result<Value> {
        let teams = self.document["teams"]
            .as_array()
            .context("Captured teams are unavailable")?;
        let mut output = Vec::with_capacity(teams.len());
        for team in teams {
            let index = team["recordIndex"]
                .as_u64()
                .context("Captured team index is unavailable")?;
            let name = team["name"]
                .as_str()
                .context("Captured team name is unavailable")?;
            let kind = team["rosterTypeId"]
                .as_u64()
                .context("Captured team type is unavailable")?;
            output.push(json!({"index":index,"name":name,"displayName":name,"normal":kind == 0}));
        }
        Ok(json!(output))
    }
    fn review_existing_season_import(&mut self, input: &Value) -> Result<Value> {
        ensure!(
            input["seasons"]
                .as_array()
                .is_some_and(|rows| rows.len() == 1),
            "Stage one existing, nonshared season at a time, then save it before importing another."
        );
        let (rows, entries) = self.read_seasons_for_review(input, true)?;
        let review = crate::season_import_review::review(
            &input["seasons"],
            &rows,
            &self.season_import_teams()?,
            input,
        );
        // Prepared closures may be discarded by the client. Never evict another
        // visible draft to adopt them; bounded exhaustion refuses before effects.
        adopt_successful_review(&mut self.prepared_seasons, entries, review)
    }
    /// WPF PlayerSeasonService.save + SeasonStats.cs write (one 64-byte record).
    fn save_season(
        &mut self,
        input: &Value,
        root: &Path,
        confirm: &dyn Fn(&str) -> bool,
    ) -> Result<Value> {
        let state = self.begin_service()?;
        let (_, identity) = self.player_identity(input)?;
        let token = input["token"]
            .as_str()
            .context("Refresh season stats before saving.")?;
        let e = self
            .seasons
            .get(token)
            .or_else(|| self.prepared_seasons.get(token))
            .filter(|e| e.identity == identity)
            .context("Refresh season stats before saving.")?;
        let (after, warnings) = services::plan_season_record(&e.raw, &input["changes"])?;
        if after == e.raw {
            return Ok(json!({"saved":false}));
        }
        let mut notes: Vec<String> = services::shared_warning(&e.links)?.into_iter().collect();
        notes.extend(warnings);
        if !notes.is_empty() && !confirm(&notes.join("\n\n")) {
            return Ok(json!({"canceled":true}));
        }
        let players = directory(&state, Kind::Player)?;
        let player = record_address(&state, &players, Kind::Player, e.player)?;
        let slot = player + (services::PLAYER_SEASONS + e.slot * 2) as u64;
        let mut lease = WriteLease::open_in(
            &self.access,
            &state,
            Kind::Season,
            profile::Directory {
                count: e.count,
                pointer: e.table,
                stride: services::SEASON_STRIDE,
            },
            e.index,
            vec![
                (state.owner + services::SEASON_HEADER, e.header.clone()),
                (slot, (e.index as i16).to_le_bytes().to_vec()),
                (player + services::PLAYER_PRIVATE_SEASONS as u64, vec![0; 8]),
            ],
        )?;
        lease.season_links = e.import_links.clone();
        lease.retained.push(RecordWitness {
            address: player,
            bytes: e.player_row.clone(),
            owned_slot: None,
        });
        lease.limited_season = e.limited;
        ensure!(
            writer::Access::capture(&lease)? == e.raw,
            "Season history or linked players changed. Refresh before saving."
        );
        let plan = services::plan_season(e.index, &e.raw, after)?;
        let owner = writer::Access::owner(&lease)?;
        writer::apply(&lease, &owner, &plan, |i| writer::save(root, i))?;
        drop(lease);
        let linked = e.links.len();
        self.seasons.clear();
        self.prepared_seasons.clear();
        let _ = self.access.finish();
        Ok(json!({"ok":true,"saved":true,"linkedCount":linked,"readBackVerified":true}))
    }
    /// Every player record, read live through the checked reader.
    fn live_players(&self, state: &profile::OwnerSnapshot) -> Result<Vec<u8>> {
        let d = &state.players;
        let len = d.count * Kind::Player.stride();
        let mut bytes = Vec::with_capacity(len);
        for offset in (0..len).step_by(4096) {
            bytes.extend(
                self.access
                    .read(d.pointer + offset as u64, (len - offset).min(4096))?,
            );
        }
        Ok(bytes)
    }
    /// WPF TeamRotationService.read + --team-rotations.
    fn read_rotations(&mut self, input: &Value) -> Result<Value> {
        let state = self.begin_service()?;
        let (team_index, team_id, name, address, team) = self.live_team(&state, input)?;
        let players = self.live_players(&state)?;
        let members = services::rotation_members(address, &players)?;
        ensure!(
            self.live_players(&state)? == players
                && read_record(&self.access, address, Kind::Team.stride())? == team,
            "The rotation roster changed during inspection."
        );
        let raw =
            team[services::ROTATIONS..services::ROTATIONS + services::ROTATION_BYTES].to_vec();
        let advanced = team[services::ROTATION_FLAG] & 1 == 1;
        let token = crate::backend::unique_id();
        self.rotations = Some(RotationSnapshot {
            team_index,
            team_id,
            name,
            token: token.clone(),
            raw: raw.clone(),
            team_hash: digest(&team),
            member_digest: services::member_digest(&members),
            members: members.clone(),
        });
        self.access.finish()?;
        Ok(
            json!({"rows":services::rotation_rows(&raw)?,"members":members.iter().map(|(id,index,name)|json!({"id":id,"index":index,"name":name})).collect::<Vec<_>>(),
            "token":token,"readOnly":true,"writes":0,"advancedRotation":advanced}),
        )
    }
    /// WPF TeamRotationService.save + --team-rotations-write. Version 2 only
    /// edits teams the game already flags as having a custom rotation
    /// (team+0x154C bit 0); otherwise the game ignores the block.
    fn save_rotations(&mut self, input: &Value, root: &Path) -> Result<Value> {
        let state = self.begin_service()?;
        let s = self
            .rotations
            .as_ref()
            .context("Refresh rotations before saving.")?;
        ensure!(
            input["token"].as_str() == Some(s.token.as_str())
                && input["recordIndex"].as_u64() == Some(s.team_index as u64)
                && input["teamId"].as_u64() == Some(s.team_id)
                && input["expectedName"] == s.name.as_str(),
            "Refresh rotations before saving."
        );
        let after_block = services::pack_rotations(&s.raw, &input["rows"], &s.members)?;
        if after_block == s.raw {
            return Ok(json!({"saved":true,"writes":0,"readBackVerified":true}));
        }
        let teams = directory(&state, Kind::Team)?;
        let address = record_address(&state, &teams, Kind::Team, s.team_index)?;
        ensure!(
            services::member_digest(&services::rotation_members(
                address,
                &self.live_players(&state)?
            )?) == s.member_digest,
            "The team or its players changed. Refresh rotations before saving."
        );
        let lease = WriteLease::open_in(
            &self.access,
            &state,
            Kind::Team,
            teams,
            s.team_index,
            Vec::new(),
        )?;
        let live = writer::Access::capture(&lease)?;
        ensure!(
            digest(&live) == s.team_hash,
            "The rotations changed. Refresh before saving."
        );
        ensure!(
            live[services::ROTATION_FLAG] & 1 == 1,
            "This team uses the game's automatic rotation (no custom rotation saved). Save a custom rotation in the game's Rotations screen first; no game write was attempted."
        );
        let plan = services::plan_rotations(s.team_index, &live, &after_block)?;
        let owner = writer::Access::owner(&lease)?;
        let commit = writer::apply(&lease, &owner, &plan, |i| {
            if !i.pending && i.result == "applied and read back verified" {
                profile::revalidate(&self.access, &state)?;
                ensure!(
                    services::member_digest(&services::rotation_members(
                        address,
                        &self.live_players(&state)?
                    )?) == s.member_digest,
                    "The team's players changed during rotation Save"
                );
            }
            writer::save(root, i)
        })?;
        drop(lease);
        let writes = (0..services::ROTATION_BYTES)
            .step_by(2)
            .filter(|i| s.raw[*i..i + 2] != after_block[*i..i + 2])
            .count();
        let index = s.team_index;
        if let Some(r) = self.rotations.as_mut() {
            r.raw = after_block;
            r.team_hash = digest(&commit.read_back);
        }
        let _ = self.snapshot.replace(Kind::Team, index, &commit.read_back);
        let _ = self.access.finish();
        Ok(
            json!({"ok":true,"schema":"nba2k27-team-rotations-write-v1","saved":true,"writes":writes,"readBackVerified":true,
            "recordIndex":input["recordIndex"],"teamId":input["teamId"],"teamDigest":format!("{:x}", sha2::Sha256::digest(&commit.read_back))}),
        )
    }
    fn editor(&mut self, request: &Editor<'_>) -> Result<Value> {
        match (request.method, request.input["action"].as_str()) {
            ("cloneArena", _) => arena_clone_access::editor(self, request),
            ("teamRotations", Some("read")) => self.read_rotations(request.input),
            ("teamRotations", Some("save")) => self.save_rotations(request.input, request.root),
            ("teamRotations", _) => anyhow::bail!("Invalid team rotations action."),
            ("teamStaff", _) => self.team_staff(request.input),
            ("saveTeamStaff", _) => self.save_team_staff(request.input, request.root),
            ("playerSeasons", Some("read")) => self.read_seasons(request.input),
            ("playerSeasons", Some("teams")) => {
                // The same exact player/owner read gate precedes choices and
                // capabilities. Caller-provided addresses/rows are never used.
                self.read_seasons_for_review(request.input, false)?;
                let capabilities = structural::capabilities(&self.access);
                Ok(json!({"teams":self.season_import_teams()?,"capabilities":capabilities}))
            }
            (
                "playerSeasons",
                Some(
                    "add-older" | "remove-oldest" | "copy-shared" | "apply-import"
                    | "import-seasons",
                ),
            ) => structural::editor(self, request),
            ("playerSeasons", Some("review-import")) => {
                structural::review_import(self, request.input)
            }
            ("playerSeasons", Some("review-existing-import")) => {
                self.review_existing_season_import(request.input)
            }
            ("playerSeasons", Some("save")) => {
                self.save_season(request.input, request.root, request.confirm)
            }
            ("playerSeasons", _) => anyhow::bail!(
                "This season action is unavailable. Refresh the player and review the supported season actions; no game write was attempted."
            ),
            _ => anyhow::bail!(
                "This Version 2 editor operation is not ported yet. No game read or write was attempted."
            ),
        }
    }
}
impl Captured for NativeCapture {
    fn document(&self) -> &Value {
        &self.document
    }
    fn detail(&mut self, index: usize) -> Result<Value> {
        self.access.start()?;
        let value = model::player_details(&self.access, &self.snapshot, index)?;
        let mut document = player_document(&value)?;
        // Unsupported/shared appearance sources remain read-only. This probe
        // opens no VM_WRITE handle and never calls the in-process executor.
        let _ =
            appearance_access::details(&self.access, self.snapshot.owner(), index, &mut document);
        self.access.finish()?;
        Ok(document)
    }
    fn save(&mut self, request: &Save<'_>) -> Result<Value> {
        ensure!(!self.stale, "Refresh the Version 2 roster before saving");
        self.access.start()?;
        // One ownership/profile proof per transaction, never per field.
        model::verify_profile(&self.access)?;
        writer::verify_write_profile(&self.access)?;
        profile::revalidate(&self.access, self.snapshot.owner())?;
        let report = self.commit(request)?;
        // The write is already verified; a later guard failure only means the
        // next operation will need a fresh capture.
        if self.access.finish().is_err() {
            self.stale = true;
        }
        Ok(report)
    }
    fn usable(&self) -> bool {
        !self.stale
    }
    fn editor(&mut self, request: &Editor<'_>) -> Result<Value> {
        NativeCapture::editor(self, request)
    }
}
impl Engine for NativeEngine {
    fn capture(&self, binding: &Binding, stop: Arc<AtomicBool>) -> Result<Box<dyn Captured>> {
        let mut access = Access::open(binding, stop)?;
        access.start()?;
        let snapshot = model::capture(&access, access.region()?)?;
        // Missing/unreadable manifest: candidates show without verified stock art,
        // and WPF's renderer hides them, exactly as WPF did.
        let stock_uniforms = uniform_bases(&binding.root).unwrap_or_default();
        let document = browser_document_with(&snapshot, &stock_uniforms)?;
        access.finish()?;
        Ok(Box::new(NativeCapture {
            access,
            snapshot,
            document,
            stale: false,
            staff: None,
            rotations: None,
            seasons: std::collections::HashMap::new(),
            prepared_seasons: std::collections::HashMap::new(),
            structural_review: None,
            stock_uniforms,
        }))
    }
}
fn idle(status: &str, message: &str) -> Value {
    json!({"status":status,"readOnly":true,"writes":0,"writeAvailable":false,"message":message,"players":[],"teams":[],"arenas":[],"origins":[]})
}
fn valid_tab(tab: &str) -> bool {
    tab.len() == 36
        && tab.bytes().enumerate().all(|(i, v)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                v == b'-'
            } else {
                v.is_ascii_hexdigit()
            }
        })
}
struct State {
    binding: Option<Binding>,
    latest: Option<Box<dyn Captured>>,
    // The capture is lent to one Save; status polls keep the current view.
    saving: bool,
    revision: u64,
    tabs: BTreeMap<String, u64>,
    tab_used: RefCell<BTreeMap<String, Instant>>,
    view: Value,
}
impl State {
    fn bind(&mut self, binding: &Binding) {
        if self.binding.as_ref() != Some(binding) {
            self.latest = None;
            self.tabs.clear();
            self.view = idle(
                "idle",
                "Refresh the verified Version 2 roster before editing or saving.",
            );
            self.binding = Some(binding.clone());
        }
    }
    fn lease(&mut self, tab: &str) {
        if self.tabs.len() >= 16 && !self.tabs.contains_key(tab) {
            if let Some(key) = self
                .tabs
                .keys()
                .min_by_key(|key| self.tab_used.borrow().get(*key).copied())
                .cloned()
            {
                self.tabs.remove(&key);
            }
        }
        self.tabs.insert(tab.into(), self.revision);
        let used = self.tab_used.get_mut();
        used.retain(|key, _| self.tabs.contains_key(key));
        used.insert(tab.into(), Instant::now());
    }
    fn current(&self, tab: &str) -> bool {
        let current =
            (self.latest.is_some() || self.saving) && self.tabs.get(tab) == Some(&self.revision);
        if current {
            self.tab_used
                .borrow_mut()
                .insert(tab.into(), Instant::now());
        }
        current
    }
}
pub(crate) struct Service {
    operation: Mutex<()>,
    state: Mutex<State>,
    stop: Arc<AtomicBool>,
    engine: Arc<dyn Engine>,
}
impl Service {
    pub fn new() -> Self {
        Self {
            operation: Mutex::new(()),
            state: Mutex::new(State {
                binding: None,
                latest: None,
                saving: false,
                revision: 0,
                tabs: BTreeMap::new(),
                tab_used: RefCell::new(BTreeMap::new()),
                view: idle(
                    "idle",
                    "Open your owned Version 2 game session, then refresh. Saves need a current snapshot.",
                ),
            }),
            stop: Arc::new(AtomicBool::new(false)),
            engine: Arc::new(NativeEngine),
        }
    }
    /// Compatible with Workspace::with_roster's existing (method, tab) contract;
    /// binding must be resolved in the trusted native callback, not the browser.
    pub fn request(&self, method: &str, tab: &str, binding: &Binding) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Read-only roster service is closing"
        );
        ensure!(valid_tab(tab), "Invalid roster tab identity");
        ensure!(
            matches!(
                method,
                "bootstrap"
                    | "status"
                    | "playerResearchStatus"
                    | "rosterAutoAttach"
                    | "scanPlayerResearch"
            ),
            "Version2 roster backend rejects this operation; no write capability exists"
        );
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Read-only roster state unavailable"))?;
            state.bind(binding);
            if matches!(method, "bootstrap" | "status" | "playerResearchStatus") {
                return Ok(if state.current(tab) {
                    state.view.clone()
                } else {
                    idle(
                        "idle",
                        "Refresh this Version 2 tab; it has no current native snapshot.",
                    )
                });
            }
            if method == "rosterAutoAttach" && state.latest.is_some() {
                state.lease(tab);
                return Ok(state.view.clone());
            }
        }
        let _operation = match self.operation.try_lock() {
            Ok(g) => g,
            Err(_) => {
                return Ok(idle(
                    "scanning",
                    "A Version 2 roster operation is already in progress.",
                ));
            }
        };
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Read-only roster state unavailable"))?;
            state.view = idle("scanning", "Reading the verified Version 2 roster…");
        }
        let result = self.engine.capture(binding, self.stop.clone());
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Read-only roster state unavailable"))?;
        ensure!(
            state.binding.as_ref() == Some(binding),
            "Owned launch binding changed while reading"
        );
        match result {
            Ok(captured) => {
                state.view = captured.document().clone();
                state.latest = Some(captured);
                state.revision = state
                    .revision
                    .checked_add(1)
                    .context("Roster revision exhausted")?;
                state.lease(tab);
                Ok(state.view.clone())
            }
            Err(error) => {
                state.latest = None;
                state.tabs.clear();
                state.view = idle(
                    if method == "rosterAutoAttach" {
                        "waiting"
                    } else {
                        "error"
                    },
                    &format!("{error:#}"),
                );
                Ok(state.view.clone())
            }
        }
    }
    /// Optional future lazy-details RPC; current browser receives inline fields.
    pub fn player_detail(&self, tab: &str, binding: &Binding, index: usize) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire) && valid_tab(tab),
            "Read-only roster tab is closed or invalid"
        );
        let _operation = self
            .operation
            .try_lock()
            .map_err(|_| anyhow::anyhow!("A read-only operation is in progress"))?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Read-only roster state unavailable"))?;
        ensure!(
            state.binding.as_ref() == Some(binding) && state.current(tab),
            "This native snapshot expired; refresh the roster"
        );
        state
            .latest
            .as_mut()
            .context("No native roster snapshot")?
            .detail(index)
    }
    #[cfg(test)]
    pub(crate) fn with_engine(engine: Arc<dyn Engine>) -> Self {
        Self {
            engine,
            ..Self::new()
        }
    }
    /// True while this tab holds a Version 2 lease (current or expired). The
    /// legacy editor uses it to refuse saves honestly instead of guessing.
    pub fn owns_tab(&self, tab: &str) -> bool {
        self.state
            .lock()
            .is_ok_and(|state| state.tabs.contains_key(tab))
    }
    /// Status poll for a tab this backend already leased. Needs no binding and
    /// never starts a capture or grants a fresh baseline.
    pub fn status(&self, tab: &str) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Read-only roster service is closing"
        );
        ensure!(valid_tab(tab), "Invalid roster tab identity");
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Read-only roster state unavailable"))?;
        Ok(if state.current(tab) {
            state.view.clone()
        } else {
            idle(
                "idle",
                "Refresh this Version 2 tab; it has no current native snapshot.",
            )
        })
    }
    /// Copy/list DNA from the existing leased snapshot only. This never calls
    /// the engine or Captured services, renews a lease, or stages a destination.
    pub fn player_dna(&self, tab: &str, request: &Value) -> Result<Value> {
        ensure!(valid_tab(tab), "Invalid roster tab identity");
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Read-only roster state unavailable"))?;
        ensure!(
            !self.stop.load(Ordering::Acquire)
                && !state.saving
                && state.binding.is_some()
                && state.current(tab)
                && state.view["status"] == "ready",
            "Refresh this Version 2 tab before copying Player DNA; its snapshot is unavailable or an operation is in progress"
        );
        let players = state.view["players"]
            .as_array()
            .context("Player list missing")?;
        let result = crate::player_dna::snapshot_request(players, request)?;
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Roster service is closing"
        );
        Ok(result)
    }
    /// One player of the tab's current view (header labels, artwork keys).
    pub fn view_player(&self, tab: &str, index: u64) -> Option<Value> {
        let state = self.state.lock().ok()?;
        if !state.current(tab) {
            return None;
        }
        state.view["players"]
            .as_array()?
            .iter()
            .find(|p| p["source"] == "live-roster" && p["recordIndex"].as_u64() == Some(index))
            .cloned()
    }
    /// Drops a tab's lease when the configured game is no longer Version 2.
    pub fn release(&self, tab: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.tabs.remove(tab);
        }
    }
    pub fn reject_write(&self) -> Result<Value> {
        anyhow::bail!(
            "This Version 2 roster operation is not enabled. Player, team and uniform Save & verify are; no game write was attempted."
        )
    }
    /// Browser Save & verify for a tab holding the current Version 2 capture.
    /// One owned transaction at a time; other tabs must refresh afterwards.
    /// Staff/season editor services for the tab's current capture. Reads
    /// never write; saves take the global write gate and refuse while any
    /// native roster journal is pending, exactly like Save & verify.
    pub fn editor(
        &self,
        method: &str,
        tab: &str,
        root: &Path,
        input: &Value,
        confirm: &dyn Fn(&str) -> bool,
    ) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Version 2 roster service is closing"
        );
        ensure!(valid_tab(tab), "Invalid roster tab identity");
        let writes = editor_writes(method, input);
        let _operation = self.operation.try_lock().map_err(|_| {
            anyhow::anyhow!("Another roster operation is in progress. Wait and try again.")
        })?;
        let _gate = if writes {
            let gate = session_safety::write_gate()?;
            ensure!(
                !writer::any_pending(root)?,
                "A native roster transaction needs review. Close NBA 2K27 and clear finished launch before retrying."
            );
            Some(gate)
        } else {
            None
        };
        let (mut captured, binding, revision) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Roster state unavailable"))?;
            ensure!(
                state.current(tab) && state.latest.is_some(),
                "This editor snapshot expired. Refresh the Version 2 roster first."
            );
            state.saving = true;
            (
                state.latest.take().context("No native roster snapshot")?,
                state.binding.clone(),
                state.revision,
            )
        };
        let mut result = captured.editor(&Editor {
            method,
            root,
            input,
            confirm,
            write_gate: _gate.as_ref(),
        });
        let mut refreshed = false;
        if (method == "cloneArena"
            || (method == "playerSeasons"
                && matches!(
                    input["action"].as_str(),
                    Some(
                        "add-older"
                            | "remove-oldest"
                            | "copy-shared"
                            | "apply-import"
                            | "import-seasons"
                    )
                )))
            && result.is_ok()
            && result.as_ref().is_ok_and(|value| value["canceled"] != true)
        {
            match binding
                .as_ref()
                .context("Launch binding ended")
                .and_then(|binding| self.engine.capture(binding, self.stop.clone()))
            {
                Ok(next) => {
                    captured = next;
                    refreshed = true;
                }
                Err(error) => {
                    if let Ok(value) = &mut result {
                        value["refreshRequired"] = json!(format!("{error:#}"));
                    }
                }
            }
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state unavailable"))?;
        state.saving = false;
        if state.binding == binding && state.revision == revision && captured.usable() {
            if refreshed {
                state.view = captured.document().clone();
                state.revision += 1;
                state.tabs.clear();
                state.lease(tab);
                if let Ok(value) = &mut result {
                    value["playerResearch"] = state.view.clone();
                }
            }
            state.latest = Some(captured);
        } else {
            state.tabs.remove(tab);
        }
        result
    }
    pub fn save(&self, kind: Kind, tab: &str, root: &Path, input: &Value) -> Result<Value> {
        self.save_with(kind, tab, root, input, false)
    }
    /// WPF team-uniform:link through the same guarded save pipeline.
    pub fn link_uniform(&self, tab: &str, root: &Path, input: &Value) -> Result<Value> {
        self.save_with(Kind::Team, tab, root, input, true)
    }
    fn save_with(
        &self,
        kind: Kind,
        tab: &str,
        root: &Path,
        input: &Value,
        link: bool,
    ) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Version 2 roster service is closing"
        );
        ensure!(valid_tab(tab), "Invalid roster tab identity");
        let _operation = self.operation.try_lock().map_err(|_| {
            anyhow::anyhow!("Another roster operation is in progress. Wait before saving.")
        })?;
        let _gate = session_safety::write_gate()?;
        ensure!(
            !writer::any_pending(root)?,
            "A native roster transaction needs review. Close NBA 2K27 and clear finished launch before retrying."
        );
        let (mut captured, binding, revision) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Roster state unavailable"))?;
            ensure!(
                state.current(tab) && state.latest.is_some(),
                "This editor snapshot expired. Refresh the Version 2 roster before saving."
            );
            state.saving = true;
            (
                state.latest.take().context("No native roster snapshot")?,
                state.binding.clone(),
                state.revision,
            )
        };
        let result = captured.save(&Save {
            kind,
            root,
            input,
            link,
        });
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Roster state unavailable"))?;
        state.saving = false;
        let same = state.binding == binding && state.revision == revision;
        match result {
            Ok(report) if same && captured.usable() => {
                state.view = captured.document().clone();
                state.latest = Some(captured);
                state.revision = state
                    .revision
                    .checked_add(1)
                    .context("Roster revision exhausted")?;
                state.lease(tab);
                Ok(json!({"report":report,"playerResearch":state.view}))
            }
            Ok(report) => {
                state.tabs.remove(tab);
                Ok(
                    json!({"report":report,"playerResearch":idle("idle","The change was saved and verified. Refresh the Version 2 roster before another edit.")}),
                )
            }
            Err(error) => {
                if same && captured.usable() {
                    state.latest = Some(captured);
                } else {
                    state.tabs.remove(tab);
                }
                Err(error)
            }
        }
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
    }
}
#[cfg(test)]
#[path = "roster_v2_access_tests.rs"]
mod tests;
