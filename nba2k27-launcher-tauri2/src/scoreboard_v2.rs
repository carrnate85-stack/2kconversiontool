//! Version 2 cached Blacktop scoreboard descriptor. No native hooks/calls,
//! allocation, process discovery or file/game writes. Plans expose at most
//! three 12-byte headers, never list pointers or an arbitrary renderer address.
//! UNWIRED: the retained session executor must journal before publishing them.
#![allow(dead_code)]
use crate::{
    blacktop_arena::{Inspect, Region},
    blacktop_native::{ContextState, Memory},
    rules_v2,
    scoreboard_profile::{self as profile, Choice, Plan},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const PACKAGES: u64 = 0x0A98CC40;
const ACTIVE: u64 = 0x0A99B850;
const OTHER: u64 = 0x0A976900;
const RESOURCES: u64 = 0x0BE95238;
const VTABLE: u64 = 0x08101C38;
const METHOD: u64 = 0x050C5FF0;
const LOCATOR: u64 = 0x08101E00;
// The native context class alone permits omission of its mutable lifecycle
// list head. Complete methods bind the constructor/global and list semantics.
const CONTEXT_VTABLE: u64 = 0x07724EE8;
const LIFECYCLE_METHODS: [(u64, usize, &str); 8] = [
    (
        0x5080090,
        225,
        "EA2150313878DD3FFCDD907F7DA860028D7DC7A69914AD2E774476C990DBDA2F",
    ),
    (
        0x5080200,
        379,
        "AFC9A2EA90F9F8D382EB5E7B6C5D792513E5E937062BE70479A6AC2A8D8AEA22",
    ),
    (
        0x5081030,
        164,
        "60A1C45DEE81C7A2C99D3D94E2C20756F5DF26466E218B989E4191DC3B3DC5A1",
    ),
    (
        0x5081120,
        190,
        "417CE0770E14AF728A4AAF9C04FF7841E07A3FF4688CB10FFE6E8DBE2146B997",
    ),
    (
        0x50CA1D0,
        257,
        "C3C2E4BB353E07CC92A4C7F02239BA6F917DBBCE18082771FE3148511C290E31",
    ),
    (
        0x50d26c0,
        403,
        "040D960E25C4A3456A580E9471BE69F7A1D98A4B0F8F86C751AE079B97A02AAB",
    ),
    (
        0x50d23e0,
        216,
        "43FFED7F8D96DA11B5AFC2694475EBE930559D1ACA98FDD84603E6236576973D",
    ),
    (
        0x50d25d0,
        198,
        "9B331DE31B32D14423FE8CFE13271E7161AA076C795D5191365C4D58038DCEC2",
    ),
];
const SHA: &str = "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B";
fn at(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base().checked_add(rva).context("Scoreboard RVA overflow")
}
fn valid_pointer(a: u64) -> bool {
    a >= 0x10000 && a % 8 == 0 && a < 0x7fff00000000
}
fn d(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(o..o + 4)
            .context("Truncated scoreboard DWORD")?
            .try_into()?,
    ))
}
fn q(b: &[u8], o: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(o..o + 8)
            .context("Truncated scoreboard pointer")?
            .try_into()?,
    ))
}
fn read(m: &impl Memory, a: u64, n: usize) -> Result<Vec<u8>> {
    ensure!(
        a >= 0x10000
            && (1..=2 * 1024 * 1024).contains(&n)
            && a.checked_add(n as u64).is_some_and(|e| e < 0x7fff00000000),
        "Invalid bounded scoreboard read"
    );
    m.checkpoint()?;
    let b = m.read(a, n)?;
    ensure!(b.len() == n, "Partial scoreboard read");
    Ok(b)
}
fn decode(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len() % 2 == 0 && s.len() <= 256,
        "Invalid scoreboard reference bytes"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| Ok(u8::from_str_radix(std::str::from_utf8(c)?, 16)?))
        .collect()
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    rules_v2::verify_profile(m)?;
    let v: serde_json::Value =
        serde_json::from_str(include_str!("../assets/scoreboard-v2-anchors.json"))?;
    ensure!(
        v["schema"] == "nba2k27-v2-scoreboard-anchors-v1"
            && v["sha256"] == SHA
            && v["gameProcessAccess"] == false,
        "Scoreboard native provenance changed"
    );
    for (key, rva) in [
        ("packageTableRva", PACKAGES),
        ("activeRva", ACTIVE),
        ("secondaryActiveRva", OTHER),
        ("resourcesRva", RESOURCES),
        ("resourceVtableRva", VTABLE),
        ("resourceMethodRva", METHOD),
        ("resourceLocatorRva", LOCATOR),
    ] {
        ensure!(
            v[key].as_u64() == Some(rva),
            "Scoreboard native mapping differs from its proof"
        );
    }
    let anchors = v["anchors"]
        .as_array()
        .context("Missing scoreboard native proof")?;
    ensure!(anchors.len() == 18, "Scoreboard proof count changed");
    for a in anchors {
        let rva = a["rva"]
            .as_u64()
            .context("Missing scoreboard code location")?;
        let b = decode(a["hex"].as_str().context("Missing scoreboard code bytes")?)?;
        ensure!(
            (0x1000..0x5D00000).contains(&rva)
                && !b.is_empty()
                && read(m, at(m, rva)?, b.len())? == b,
            "Version 2 scoreboard native code changed: {}",
            a["label"]
        );
    }
    ensure!(
        q(&read(m, at(m, VTABLE - 8)?, 8)?, 0)? == at(m, LOCATOR)?
            && q(&read(m, at(m, VTABLE + 0xC8)?, 8)?, 0)? == at(m, METHOD)?,
        "Scoreboard resource vtable changed"
    );
    for (rva, bytes, expected) in LIFECYCLE_METHODS {
        ensure!(
            format!("{:X}", Sha256::digest(read(m, at(m, rva)?, bytes)?)) == expected,
            "Version 2 resource lifecycle semantics changed at {rva:#X}"
        );
    }
    m.checkpoint()
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Span {
    address: u64,
    bytes: Vec<u8>,
    allocation: u64,
    kind: u32,
    protection: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct RegistryProof {
    address: u64,
    count: usize,
    bytes: Vec<u8>,
    allocation: u64,
    kind: u32,
    protection: u32,
    stock_row: u64,
    selected_row: u64,
}
impl RegistryProof {
    fn same_owner(&self, other: &Self) -> bool {
        self.address == other.address
            && self.count == other.count
            && (self.allocation, self.kind, self.protection)
                == (other.allocation, other.kind, other.protection)
            && self.stock_row == other.stock_row
            && self.selected_row == other.selected_row
            && self.bytes[4..]
                .chunks_exact(260)
                .zip(other.bytes[4..].chunks_exact(260))
                .all(|(previous, current)| previous[..4] == current[..4])
            && self.row(self.stock_row) == other.row(other.stock_row)
            && self.row(self.selected_row) == other.row(other.selected_row)
    }
    fn row(&self, address: u64) -> &[u8] {
        let offset = usize::try_from(address - self.address).unwrap();
        &self.bytes[offset..offset + 260]
    }
}
fn region(m: &impl Inspect, a: u64, n: usize, writable: bool) -> Result<Region> {
    let p = m.region(a)?;
    ensure!(
        p.committed
            && matches!(p.kind, 0x20000 | 0x1000000)
            && matches!(p.protection, 2 | 4)
            && (!writable || (p.kind == 0x20000 && p.protection == 4))
            && valid_pointer(p.allocation)
            && a >= p.start
            && a.checked_add(n as u64)
                .zip(p.start.checked_add(p.len))
                .is_some_and(|(e, l)| e <= l),
        "Scoreboard owner left plain readable/private RW non-executable data"
    );
    Ok(p)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    base: u64,
    context: ContextState,
    spans: Vec<Span>,
    fields: BTreeMap<u64, Vec<u8>>,
    field_pages: BTreeMap<u64, (u64, u32, u32)>,
    registry: RegistryProof,
    choice: Choice,
}
impl Snapshot {
    pub(crate) fn base(&self) -> u64 {
        self.base
    }
    pub(crate) fn context(&self) -> &ContextState {
        &self.context
    }
    pub(crate) fn fields(&self) -> &BTreeMap<u64, Vec<u8>> {
        &self.fields
    }
    pub(crate) fn plan(&self) -> Result<Plan> {
        profile::plan(&self.fields, &self.choice.id)
    }
    pub(crate) fn choice(&self) -> &Choice {
        &self.choice
    }
    pub(crate) fn allocation(&self, address: u64) -> Result<u64> {
        Ok(self
            .field_pages
            .get(&address)
            .context("Scoreboard field is outside snapshot ownership")?
            .0)
    }
}
struct Capture<'a, M: Inspect> {
    m: &'a M,
    spans: Vec<Span>,
    budget: usize,
}
impl<M: Inspect> Capture<'_, M> {
    fn keep(&mut self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.budget = self
            .budget
            .checked_add(n)
            .context("Scoreboard capture overflow")?;
        ensure!(
            self.budget <= 2 * 1024 * 1024 && self.spans.len() < 1024,
            "Scoreboard capture exceeds ownership bounds"
        );
        let p = region(self.m, a, n, false)?;
        let bytes = read(self.m, a, n)?;
        self.spans.push(Span {
            address: a,
            bytes: bytes.clone(),
            allocation: p.allocation,
            kind: p.kind,
            protection: p.protection,
        });
        Ok(bytes)
    }
    fn pointer(&mut self, a: u64) -> Result<u64> {
        let v = q(&self.keep(a, 8)?, 0)?;
        ensure!(
            valid_pointer(v),
            "Scoreboard owner is unavailable or unaligned"
        );
        Ok(v)
    }
    fn resources(&mut self, c: &Choice) -> Result<RegistryProof> {
        let context = self.pointer(at(self.m, RESOURCES)?)?;
        // Preserve the original single-region proof for the complete header.
        region(self.m, context, 0xB0, false)?;
        // Only the class pointer identifies the context. The native lifecycle
        // methods mutate opaque bookkeeping throughout +08..+9F while the
        // same context, bucket table and selected resource remain valid. Live
        // exact-session evidence observed +48 and +68 changing after a fully
        // verified scoreboard publication. Retaining that opaque block made a
        // normal lifecycle tick look like foreign ownership.
        let class = self.keep(context, 8)?;
        ensure!(
            q(&class, 0)? == at(self.m, CONTEXT_VTABLE)?,
            "Scoreboard resource context class differs from native lifecycle proof"
        );
        let suffix = self.keep(context + 0xA0, 0x10)?;
        // Bucket count/pointer and the selected resource's own serial at+B0
        // remain exact (selected resource keep(owner,0xB8) below). All
        // identity-bearing roots, links, owners, registry rows, package tree,
        // properties and allocation metadata are still retained verbatim.
        let count = d(&suffix, 0)?;
        let buckets = q(&suffix, 8)?;
        ensure!(
            count > 0 && count <= 1048576 && count.is_power_of_two() && valid_pointer(buckets),
            "Unsupported scoreboard resource bucket table"
        );
        let mut node = q(
            &self.keep(buckets + 8 * u64::from(0x988E62C4u32 & (count - 1)), 8)?,
            0,
        )?;
        let mut seen = BTreeSet::new();
        let mut registry = None;
        // Live 0.48 probe: Version 2 buckets are circular doubly-linked lists
        // (+0x30 prev, +0x38 next) that wrap back to the head. Stop at the
        // head, require every back-link, and still refuse any other cycle.
        let head = node;
        let mut previous = None;
        let mut links = Vec::new();
        while node != 0 && !(previous.is_some() && node == head) {
            ensure!(
                valid_pointer(node) && seen.len() < 512 && seen.insert(node),
                "Cyclic/oversized scoreboard resource chain"
            );
            let row = self.keep(node, 64)?;
            links.push((q(&row, 0x30)?, previous));
            previous = Some(node);
            if d(&row, 0)? == 0x988E62C4 && d(&row, 4)? == 0x7DE5F718 && row[0x23] & 2 == 0 {
                let owner = q(&row, 0x28)?;
                ensure!(
                    valid_pointer(owner),
                    "Unsupported scoreboard resource owner"
                );
                let o = self.keep(owner, 0xB8)?;
                if d(&o, 0x54)? == 0xDCE5273B {
                    ensure!(
                        registry.is_none() && q(&o, 0)? == at(self.m, VTABLE)? && q(&o, 0x80)? != 0,
                        "Ambiguous scoreboard resource ownership"
                    );
                    let p = q(&row, 0x18)?;
                    ensure!(
                        valid_pointer(p),
                        "Scoreboard resource registry is not ready"
                    );
                    registry = Some(p);
                }
            }
            node = q(&row, 0x38)?;
        }
        if node != 0 {
            // Wrapped: the head's back-link is the tail; every other node's is its predecessor.
            ensure!(
                links
                    .iter()
                    .enumerate()
                    .all(|(i, (back, prev))| Some(*back) == if i == 0 { previous } else { *prev }),
                "Scoreboard resource chain back-link changed"
            );
        }
        let registry = registry.context("Scoreboard resource registry is not loaded")?;
        let count = d(&self.keep(registry, 4)?, 0)? as usize;
        // Count is bounded and read from V2's typed native resource, not copied from
        // the previous build's 1525-row assumption. Both exact stock+selected hashes
        // AND filenames must occur uniquely; a hash alone is not sufficient.
        ensure!(
            (1..=4096).contains(&count),
            "Unsupported scoreboard resource count"
        );
        let bytes = 4 + count * 260;
        let page = region(self.m, registry, bytes, false)?;
        let data = read(self.m, registry, bytes)?;
        let stock = profile::choice("default")?;
        let mut found = 0;
        let mut original = 0;
        let mut stock_row = 0;
        let mut selected_row = 0;
        for (index, row) in data[4..].chunks_exact(260).enumerate() {
            let id = d(row, 0)?;
            if id != c.resource && id != profile::STOCK {
                continue;
            }
            let words = row[4..]
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect::<Vec<_>>();
            let end = words
                .iter()
                .position(|v| *v == 0)
                .context("Unterminated scoreboard registry name")?;
            let name = String::from_utf16(&words[..end])?;
            ensure!(name.is_ascii(), "Non-ASCII scoreboard registry filename");
            if id == profile::STOCK {
                ensure!(
                    name.eq_ignore_ascii_case(&stock.file),
                    "Stock scoreboard resource filename differs"
                );
                original += 1;
                stock_row = registry + 4 + index as u64 * 260;
            }
            if id == c.resource {
                ensure!(
                    name.eq_ignore_ascii_case(&c.file),
                    "Selected scoreboard resource filename differs"
                );
                found += 1;
                selected_row = registry + 4 + index as u64 * 260;
            }
        }
        ensure!(
            found == 1 && original == 1,
            "Selected/stock scoreboard resource is absent or ambiguous"
        );
        Ok(RegistryProof {
            address: registry,
            count,
            bytes: data,
            allocation: page.allocation,
            kind: page.kind,
            protection: page.protection,
            stock_row,
            selected_row,
        })
    }
    fn descriptor(&mut self) -> Result<(BTreeMap<u64, Vec<u8>>, BTreeMap<u64, (u64, u32, u32)>)> {
        let slots = self.keep(at(self.m, PACKAGES)?, 128 * 24)?;
        let mut package = None;
        for slot in slots.chunks_exact(24) {
            if d(slot, 0)? == 0x0A2ADB76 {
                ensure!(
                    package.is_none() && slot[16] == 0,
                    "Blacktop scoreboard package is ambiguous or loading"
                );
                let p = q(slot, 8)?;
                ensure!(valid_pointer(p), "Blacktop scoreboard package is not ready");
                package = Some(p);
            }
        }
        let package = package.context("Enter Blacktop selection to prepare this scoreboard")?;
        let ph = self.keep(package, 32)?;
        let (mut node, storage, used, cap) = (
            q(&ph, 0)?,
            q(&ph, 8)?,
            u64::from(d(&ph, 16)?),
            u64::from(d(&ph, 20)?),
        );
        ensure!(
            valid_pointer(storage) && used > 0 && used <= cap && cap <= 300,
            "Unsupported Blacktop scoreboard package bounds"
        );
        region(self.m, storage, (used * 0x88) as usize, false)?;
        let mut visited = BTreeSet::new();
        let (sentinel, mut field) = loop {
            ensure!(
                valid_pointer(node)
                    && node >= storage
                    && node
                        .checked_sub(storage)
                        .is_some_and(|o| o % 0x88 == 0 && o / 0x88 < used)
                    && visited.insert(node),
                "Invalid/cyclic scoreboard package tree"
            );
            let bytes = self.keep(node, 0x88)?;
            let id = d(&bytes, 0)?;
            if id == profile::STOCK {
                break (node + 8, q(&bytes, 0x58)?);
            }
            node = q(&bytes, if profile::STOCK < id { 0x78 } else { 0x80 })?;
            ensure!(node != 0, "Stock Blacktop scoreboard descriptor is absent");
        };
        visited.clear();
        let mut fields = BTreeMap::new();
        let mut pages = BTreeMap::new();
        while field != sentinel {
            ensure!(
                field >= 0x10000 && field % 4 == 0 && visited.len() < 128 && visited.insert(field),
                "Invalid/cyclic scoreboard property list"
            );
            let p = region(self.m, field, 96, true)?;
            let bytes = read(self.m, field, 96)?;
            let next = q(&bytes, 0x50)?;
            fields.insert(field, bytes);
            pages.insert(field, (p.allocation, p.kind, p.protection));
            field = next;
        }
        ensure!(
            (3..=128).contains(&fields.len()),
            "Unsupported scoreboard property count"
        );
        Ok((fields, pages))
    }
}
fn raw(m: &impl Inspect, c: &Choice, allowed: &[&str]) -> Result<Snapshot> {
    verify_profile(m)?;
    let before = rules_v2::context(m)?;
    ensure!(
        allowed.contains(&rules_v2::boundary(&before, m.base())),
        "Scoreboard changes wait for stable Blacktop selection or a safe menu boundary"
    );
    let mut r = Capture {
        m,
        spans: Vec::new(),
        budget: 0,
    };
    for g in [ACTIVE, OTHER] {
        ensure!(
            q(&r.keep(at(m, g)?, 8)?, 0)? == 0,
            "A scoreboard instance is still active; leave the match first"
        );
    }
    let registry = r.resources(c)?;
    let (fields, field_pages) = r.descriptor()?;
    ensure!(
        rules_v2::context(m)? == before,
        "Scoreboard mode changed during capture"
    );
    let snapshot = Snapshot {
        base: m.base(),
        context: before,
        spans: r.spans,
        fields,
        field_pages,
        registry,
        choice: c.clone(),
    };
    stable(m, &snapshot, &snapshot.fields, true, true)?;
    Ok(snapshot)
}
fn registry_stable(m: &impl Inspect, s: &Snapshot, exact: bool) -> Result<()> {
    let r = &s.registry;
    let bytes = 4 + r.count * 260;
    let page = region(m, r.address, bytes, false)?;
    ensure!(
        (page.allocation, page.kind, page.protection) == (r.allocation, r.kind, r.protection),
        "Scoreboard registry allocation changed"
    );
    let data = read(m, r.address, bytes)?;
    ensure!(
        d(&data, 0)? as usize == r.count,
        "Scoreboard resource count changed"
    );
    ensure!(
        data[4..]
            .chunks_exact(260)
            .zip(r.bytes[4..].chunks_exact(260))
            .all(|(current, original)| current[..4] == original[..4]),
        "Scoreboard registry row identity changed"
    );
    let stock = profile::choice("default")?;
    let mut found = Vec::new();
    let mut original = Vec::new();
    for (index, row) in data[4..].chunks_exact(260).enumerate() {
        let id = d(row, 0)?;
        if id != s.choice.resource && id != profile::STOCK {
            continue;
        }
        let words = row[4..]
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect::<Vec<_>>();
        let end = words
            .iter()
            .position(|v| *v == 0)
            .context("Unterminated scoreboard registry name")?;
        let name = String::from_utf16(&words[..end])?;
        ensure!(name.is_ascii(), "Non-ASCII scoreboard registry filename");
        let address = r.address + 4 + index as u64 * 260;
        if id == profile::STOCK {
            ensure!(
                name.eq_ignore_ascii_case(&stock.file),
                "Stock scoreboard resource filename differs"
            );
            original.push(address);
        }
        if id == s.choice.resource {
            ensure!(
                name.eq_ignore_ascii_case(&s.choice.file),
                "Selected scoreboard resource filename differs"
            );
            found.push(address);
        }
    }
    ensure!(
        original == [r.stock_row]
            && found == [r.selected_row]
            && &data[usize::try_from(r.stock_row - r.address)?
                ..usize::try_from(r.stock_row - r.address)? + 260]
                == r.row(r.stock_row)
            && &data[usize::try_from(r.selected_row - r.address)?
                ..usize::try_from(r.selected_row - r.address)? + 260]
                == r.row(r.selected_row)
            && (!exact || data == r.bytes),
        "Scoreboard registry ownership data changed"
    );
    Ok(())
}
fn stable(
    m: &impl Inspect,
    s: &Snapshot,
    expected: &BTreeMap<u64, Vec<u8>>,
    same_context: bool,
    exact_registry: bool,
) -> Result<()> {
    ensure!(
        m.base() == s.base,
        "Scoreboard process image identity changed"
    );
    for span in &s.spans {
        let p = region(m, span.address, span.bytes.len(), false)?;
        ensure!(
            (p.allocation, p.kind, p.protection) == (span.allocation, span.kind, span.protection)
                && read(m, span.address, span.bytes.len())? == span.bytes,
            "Scoreboard ownership data changed"
        );
    }
    registry_stable(m, s, exact_registry)?;
    ensure!(
        expected.len() == s.fields.len(),
        "Scoreboard property set changed"
    );
    for (a, b) in expected {
        let p = region(m, *a, 96, true)?;
        ensure!(
            s.field_pages.get(a) == Some(&(p.allocation, p.kind, p.protection))
                && read(m, *a, 96)? == *b,
            "Scoreboard property or allocation changed outside this session"
        );
    }
    if same_context {
        ensure!(
            rules_v2::context(m)? == s.context,
            "Scoreboard mode changed after ownership validation"
        );
    }
    m.checkpoint()
}
/// Initial selection grants no write authority unless the entire current group
/// is a recognized production profile and all ownership data is recaptured.
pub(crate) fn capture(m: &impl Inspect, id: &str, layout_ready: bool) -> Result<Snapshot> {
    let c = profile::choice(id)?;
    ensure!(
        !c.requires_layout_adapter || layout_ready,
        "This scoreboard's exact UI adapter was not prepared before launch"
    );
    let s = raw(m, &c, &["selection"])?;
    s.plan()?;
    Ok(s)
}
fn expected(s: &Snapshot, p: &Plan, applied: usize) -> Result<BTreeMap<u64, Vec<u8>>> {
    ensure!(
        s.plan()? == *p && applied <= p.changes.len(),
        "Scoreboard plan differs from its native snapshot"
    );
    let mut b = s.fields.clone();
    for c in p.changes.iter().take(applied) {
        b.get_mut(&c.address).context("Scoreboard field missing")?[..12].copy_from_slice(&c.after);
    }
    Ok(b)
}
/// Call after durable intent, immediately before EVERY publication, and again
/// after read-back. Validates the full original/owned prefix, not just next12.
pub(crate) fn guard_progress(
    m: &impl Inspect,
    s: &Snapshot,
    p: &Plan,
    applied: usize,
) -> Result<()> {
    verify_profile(m)?;
    let before = rules_v2::context(m)?;
    ensure!(
        rules_v2::boundary(&before, m.base()) == "selection" && before == s.context,
        "Scoreboard selection/owner changed before publication"
    );
    stable(
        m,
        s,
        &expected(s, p, applied)?,
        true,
        applied < p.changes.len(),
    )
}
/// Returns a fresh bounded rollback transaction ONLY at selection/Main/Quick
/// Play with no active instances. The old directory, list links, flags and each
/// owned header must still match exactly. It never adopts a replacement owner.
pub(crate) fn restore_plan(
    m: &impl Inspect,
    s: &Snapshot,
    p: &Plan,
    applied: usize,
) -> Result<(Snapshot, Plan)> {
    let owned = expected(s, p, applied)?;
    let now = raw(m, &s.choice, &["selection", "main-menu", "quick-play"])?;
    ensure!(
        now.base == s.base,
        "Scoreboard restore process image base changed"
    );
    ensure!(
        now.context.owner == s.context.owner,
        "Scoreboard restore rules owner changed"
    );
    ensure!(
        now.registry.same_owner(&s.registry),
        "Scoreboard restore registry owner changed"
    );
    if now.spans != s.spans {
        let index = now
            .spans
            .iter()
            .zip(&s.spans)
            .position(|(current, previous)| current != previous)
            .unwrap_or(now.spans.len().min(s.spans.len()));
        // Metadata only: identify the retained owner that changed without
        // exposing resource/descriptor bytes or adopting a replacement span.
        let metadata = |span: &Span| {
            (
                span.address,
                span.bytes.len(),
                span.allocation,
                span.kind,
                span.protection,
            )
        };
        let previous = s.spans.get(index);
        let current = now.spans.get(index);
        let byte_offset = previous
            .zip(current)
            .and_then(|(a, b)| a.bytes.iter().zip(&b.bytes).position(|(x, y)| x != y));
        anyhow::bail!(
            "Scoreboard restore descriptor ownership data changed: span {index}, counts {} -> {}, previous {:?}, current {:?}, changed byte offset {:?}",
            s.spans.len(),
            now.spans.len(),
            previous.map(metadata),
            current.map(metadata),
            byte_offset
        );
    }
    ensure!(
        now.field_pages == s.field_pages,
        "Scoreboard restore property allocation changed"
    );
    ensure!(
        now.fields == owned,
        "Scoreboard restore property bytes differ from the exact owned prefix"
    );
    let changes = p
        .changes
        .iter()
        .take(applied)
        .rev()
        .map(|c| profile::Change {
            address: c.address,
            before: owned[&c.address].clone(),
            after: c.before[..12].to_vec(),
        })
        .collect();
    Ok((
        now,
        Plan {
            current: p.wanted,
            wanted: p.current,
            changes,
        },
    ))
}
/// Rollback plans are not ordinary forward catalog transitions (an interrupted
/// group can be mixed). This guard accepts ONLY the exact reverse owned prefix.
pub(crate) fn guard_restore(
    m: &impl Inspect,
    original: &Snapshot,
    forward: &Plan,
    applied: usize,
    restored: usize,
) -> Result<()> {
    verify_profile(m)?;
    let before = rules_v2::context(m)?;
    ensure!(
        before.owner == original.context.owner
            && ["selection", "main-menu", "quick-play"]
                .contains(&rules_v2::boundary(&before, m.base())),
        "Scoreboard restore waits for the same owner at a safe menu"
    );
    let mut bytes = expected(original, forward, applied)?;
    ensure!(
        restored <= applied,
        "Scoreboard restore progress exceeds ownership"
    );
    for c in forward.changes.iter().take(applied).rev().take(restored) {
        bytes.get_mut(&c.address).unwrap()[..12].copy_from_slice(&c.before[..12]);
    }
    stable(m, original, &bytes, false, false)?;
    ensure!(
        rules_v2::context(m)? == before,
        "Scoreboard boundary changed during restore guard"
    );
    Ok(())
}
#[cfg(test)]
#[path = "scoreboard_v2_tests.rs"]
pub(crate) mod tests;
