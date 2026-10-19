//! Native outfit ownership/selection decoding and immutable page preparation.
//! Exact WPF CLOF layouts and catalog composition; no process discovery or
//! public pointers. The writer/worker integration remains a separate capability.
#![allow(dead_code)]
use crate::{
    blacktop_native::{self, Memory},
    outfits,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, sync::OnceLock};

const ROOT: u64 = 0xAEB69D0;
const LINEUP: u64 = 0x9BBF0A0;
const MODEL: u64 = 0x6778FC8;
const DESCRIPTION: u64 = 0x682D858;
pub(crate) const PRIVATE_MAGIC: u64 = 0x32464F4C43373243;
pub(crate) const PAGE_BYTES: usize = 4096;
pub(crate) const MAX_PAGES: usize = 256; // Same bounded lifetime budget as WPF.
pub(crate) const DOTNET_FILETIME_DELTA: u64 = 504_911_232_000_000_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LayoutRecord {
    pub record: usize,
    pub array: usize,
    pub marker: String,
    pub strings: [usize; 2],
    pub names: [String; 2],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Group {
    pub entry: String,
    pub length: usize,
    pub count: usize,
    pub descriptor: usize,
    pub hash: String,
    pub outfit_count: usize,
    pub records: Vec<LayoutRecord>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Layouts {
    schema: String,
    source_sha256: String,
    groups: Vec<Group>,
}
pub(crate) fn groups() -> &'static [Group] {
    static DATA: OnceLock<Layouts> = OnceLock::new();
    &DATA
        .get_or_init(|| {
            let mut bytes = Vec::new();
            flate2::read::GzDecoder::new(&include_bytes!("../assets/outfit-layouts.json.gz")[..])
                .take(4 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .expect("Bounded outfit layouts");
            let l: Layouts = serde_json::from_slice(&bytes).expect("Reference outfit layouts");
            assert_eq!(l.schema, "nba2k27-native-outfit-layouts-v1");
            assert_eq!(l.source_sha256.len(), 64);
            assert_eq!(l.groups.len(), 1055);
            for g in &l.groups {
                assert!(
                    (32..=2 * 1024 * 1024).contains(&g.length)
                        && (1..=10000).contains(&g.count)
                        && (1..=64).contains(&g.outfit_count)
                        && g.descriptor.checked_add(16).is_some_and(|n| n <= g.length)
                        && g.hash.parse::<u64>().is_ok()
                );
                assert!(!g.records.is_empty() && g.records.len() <= g.outfit_count);
                for r in &g.records {
                    assert!(
                        r.record.checked_add(24).is_some_and(|n| n <= g.length)
                            && r.array.checked_add(16).is_some_and(|n| n <= g.length)
                            && r.marker.parse::<u64>().is_ok()
                    );
                    for (s, name) in r.strings.iter().zip(&r.names) {
                        assert!(
                            !name.is_empty()
                                && name.encode_utf16().count() < 256
                                && !name.contains('\0')
                                && s.checked_add((name.encode_utf16().count() + 1) * 2)
                                    .is_some_and(|n| n <= g.length)
                        );
                    }
                }
            }
            l
        })
        .groups
}
pub(crate) fn span(at: u64, len: usize) -> Result<()> {
    ensure!(
        at >= 65536
            && at
                .checked_add(len as u64)
                .is_some_and(|n| n < 0x8000_0000_0000),
        "Outfit address outside user memory"
    );
    Ok(())
}
pub(crate) fn q(m: &impl Memory, at: u64) -> Result<u64> {
    span(at, 8)?;
    Ok(u64::from_le_bytes(
        m.read(at, 8)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial outfit pointer"))?,
    ))
}
pub(crate) fn d(m: &impl Memory, at: u64) -> Result<u32> {
    span(at, 4)?;
    Ok(u32::from_le_bytes(
        m.read(at, 4)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Partial outfit value"))?,
    ))
}
fn text(m: &impl Memory, at: u64, bytes: usize) -> Result<String> {
    span(at, bytes)?;
    ensure!(
        bytes > 0 && bytes <= 512 && bytes % 2 == 0,
        "Outfit text bounds"
    );
    let data = m.read(at, bytes)?;
    ensure!(data.len() == bytes, "Partial outfit text");
    let words = data
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    let end = words
        .iter()
        .position(|n| *n == 0)
        .context("Unterminated outfit text")?;
    Ok(String::from_utf16(&words[..end])?)
}
pub(crate) fn validate_layout(m: &impl Memory, basis: u64, g: &Group, r: &LayoutRecord) -> bool {
    (|| -> Result<()> {
        span(basis, g.length)?;
        ensure!(
            q(m, basis + 16)? == g.count as u64
                && q(m, basis + g.descriptor as u64)? == g.hash.parse::<u64>()?
                && q(m, basis + g.descriptor as u64 + 8)? == g.outfit_count as u64,
            "Outfit group differs"
        );
        ensure!(
            q(m, basis + r.record as u64)? == r.marker.parse::<u64>()?
                && q(m, basis + r.record as u64 + 8)? == 2
                && q(m, basis + r.record as u64 + 16)? == basis + r.array as u64,
            "Outfit record differs"
        );
        for i in 0..2 {
            ensure!(
                q(m, basis + r.array as u64 + i as u64 * 8)? == basis + r.strings[i] as u64
                    && text(
                        m,
                        basis + r.strings[i] as u64,
                        (r.names[i].encode_utf16().count() + 1) * 2
                    )? == r.names[i],
                "Outfit string differs"
            );
        }
        Ok(())
    })()
    .is_ok()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlayerKey {
    pub controller: u64,
    pub slot: u8,
    pub model: u64,
    pub description: u64,
    pub roster: u64,
}
impl PlayerKey {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.slot < 10, "Invalid selected player slot");
        for (at, len) in [
            (self.controller, 0x7b0 * 10 + 0x30),
            (self.model, 8),
            (self.description, 0xF8),
            (self.roster, 0x4F8),
        ] {
            span(at, len)?;
            ensure!(at % 8 == 0, "Selected player pointer alignment changed");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Source {
    pub pointer: u64,
    pub count: u32,
    pub uniform: u64,
}
impl Source {
    pub fn bytes(&self) -> [u8; 12] {
        let mut b = [0; 12];
        b[..8].copy_from_slice(&self.pointer.to_le_bytes());
        b[8..].copy_from_slice(&self.count.to_le_bytes());
        b
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stock {
    pub pointer: u64,
    pub names: [String; 2],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Player {
    pub key: PlayerKey,
    pub source: Source,
    pub original_pointer: u64,
    pub name: String,
    pub cyberface: u16,
    pub uniform_name: Option<String>,
    pub pool: Option<String>,
    pub stock: Vec<Stock>,
    pub items: Vec<String>,
    pub group_index: usize,
    pub group_basis: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct Selection {
    pub context: blacktop_native::ContextState,
    pub controller: u64,
    pub required: u32,
    pub selected: u32,
    pub lineup: Vec<u64>,
    pub players: Vec<Player>,
    pub unavailable: Vec<u8>,
    /// Why each unavailable slot was refused, for the overlay status.
    pub reasons: Vec<(u8, String)>,
}
pub(crate) fn clothing_pool<'a>(names: impl Iterator<Item = &'a str>) -> Option<String> {
    const PREFIXES: &[&str] = &[
        "tshirt_",
        "hoodie_",
        "jacket_",
        "jogger_",
        "jeans_",
        "warmup_",
        "pants_",
        "shorts_",
        "basketball_shorts_",
        "canvas_shorts_",
        "sweats_",
        "cargo_",
        "tanktop_",
        "shirt_",
    ];
    let mut gender = None;
    for name in names {
        let (female, body) = name
            .strip_prefix("female_")
            .map(|n| (true, n))
            .unwrap_or((false, name));
        if !PREFIXES.iter().any(|p| body.starts_with(p)) {
            continue;
        }
        let next = if female { "female" } else { "male" };
        if gender.is_some_and(|g| g != next) {
            return None;
        }
        gender = Some(next);
    }
    gender.map(str::to_owned)
}
fn uniform(m: &impl Memory, pointer: u64) -> Option<String> {
    if pointer == 0 {
        return None;
    }
    let data = m.read(pointer.checked_add(8)?, 64).ok()?;
    let end = data.iter().position(|b| *b == 0)?;
    let name = std::str::from_utf8(data.get(..end)?).ok()?;
    (name.len() >= 2
        && name.starts_with('u')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
    .then(|| name.into())
}
pub(crate) fn source(m: &impl Memory, key: &PlayerKey) -> Result<Source> {
    key.validate()?;
    ensure!(
        key.slot < 10
            && q(m, key.controller + 0x20 + key.slot as u64 * 0x7b0)? == key.model
            && q(m, key.controller + 0x28 + key.slot as u64 * 0x7b0)? == key.description
            && q(m, key.model)? == m.base() + MODEL
            && q(m, key.description)? == m.base() + DESCRIPTION
            && q(m, key.description + 0xB8)? == key.roster,
        "Selected player identity changed"
    );
    Ok(Source {
        pointer: q(m, key.description + 0xC8)?,
        count: d(m, key.description + 0xD0)?,
        uniform: q(m, key.description + 0xF0)?,
    })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Page {
    pub allocation: u64,
    pub owner: PlayerKey,
    pub original_pointer: u64,
    pub street_index: usize,
    pub roster_shoes: bool,
    pub items: Vec<String>,
    pub sha256: String,
}
pub(crate) fn valid_item(name: &str) -> bool {
    name == "NO SHIRT"
        || (!name.is_empty()
            && name.len() < 256
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}
pub(crate) fn page_bytes(
    game: &Identity,
    key: &PlayerKey,
    original_pointer: u64,
    allocation: u64,
    index: usize,
    roster_shoes: bool,
    items: &[String],
) -> Result<Vec<u8>> {
    key.validate()?;
    span(allocation, PAGE_BYTES)?;
    span(original_pointer, 16)?;
    ensure!(
        allocation % 4096 == 0
            && key.slot < 10
            && (2..=7).contains(&items.len())
            && items.iter().all(|s| valid_item(s)),
        "Invalid immutable outfit page"
    );
    let birth = game
        .started
        .parse::<u64>()?
        .checked_add(DOTNET_FILETIME_DELTA)
        .context("Outfit birth time overflow")?;
    ensure!(
        game.pid > 0 && birth > DOTNET_FILETIME_DELTA && index < outfits::catalog()?.presets.len(),
        "Invalid outfit page owner/catalog index"
    );
    let mut data = vec![0; PAGE_BYTES];
    for (i, n) in [
        PRIVATE_MAGIC,
        game.pid as u64,
        birth,
        key.description,
        key.model,
        key.roster,
        original_pointer,
        items.len() as u64,
        index as u64,
        u64::from(roster_shoes),
    ]
    .iter()
    .enumerate()
    {
        data[i * 8..i * 8 + 8].copy_from_slice(&n.to_le_bytes());
    }
    for (i, name) in items.iter().enumerate() {
        let start = 256 + i * 512;
        data[128 + i * 8..136 + i * 8].copy_from_slice(&(allocation + start as u64).to_le_bytes());
        for (j, w) in name.encode_utf16().enumerate() {
            data[start + j * 2..start + j * 2 + 2].copy_from_slice(&w.to_le_bytes());
        }
    }
    Ok(data)
}
impl Page {
    pub fn verify(
        &self,
        m: &impl crate::blacktop_arena::Inspect,
        game: &Identity,
        key: &PlayerKey,
        current: &Source,
    ) -> Result<()> {
        ensure!(
            self.owner == *key
                && self.allocation.checked_add(128) == Some(current.pointer)
                && current.count as usize == self.items.len(),
            "Private outfit owner changed"
        );
        let region = m.region(self.allocation)?;
        ensure!(
            region.start == self.allocation
                && region.allocation == self.allocation
                && region.len == 4096
                && region.kind == 0x20000
                && region.protection == 2
                && region.committed,
            "Private outfit page is not immutable owned data"
        );
        let expected = page_bytes(
            game,
            key,
            self.original_pointer,
            self.allocation,
            self.street_index,
            self.roster_shoes,
            &self.items,
        )?;
        ensure!(
            format!("{:X}", Sha256::digest(&expected)) == self.sha256
                && m.read(self.allocation, 4096)? == expected,
            "Private outfit page bytes changed"
        );
        Ok(())
    }
}
fn read_player(
    m: &impl crate::blacktop_arena::Inspect,
    game: &Identity,
    controller: u64,
    slot: u8,
    pages: &[Page],
) -> Result<Player> {
    let entry = controller + 0x20 + slot as u64 * 0x7b0;
    let key = PlayerKey {
        controller,
        slot,
        model: q(m, entry)?,
        description: q(m, entry + 8)?,
        roster: 0,
    };
    span(key.model, 8)?;
    span(key.description, 0xF8)?;
    let key = PlayerKey {
        roster: q(m, key.description + 0xB8)?,
        ..key
    };
    span(key.roster, 0x4F8)?;
    let before = source(m, &key)?;
    ensure!(
        (2..=7).contains(&before.count),
        "Unsupported selected outfit count"
    );
    let page = pages
        .iter()
        .find(|p| p.allocation.checked_add(128) == Some(before.pointer));
    let original = if let Some(page) = page {
        page.verify(m, game, &key, &before)?;
        page.original_pointer
    } else {
        ensure!(
            before.count == 2,
            "Unknown private outfit page; not adopted"
        );
        before.pointer
    };
    let mut found = None;
    'outer: for (group_index, group) in groups().iter().enumerate() {
        for record in &group.records {
            m.checkpoint()?;
            let Some(basis) = original.checked_sub(record.array as u64) else {
                continue;
            };
            if validate_layout(m, basis, group, record) {
                found = Some((basis, group_index));
                break 'outer;
            }
        }
    }
    let (basis, group_index) =
        found.context("Loaded wardrobe does not match a verified CLOF layout")?;
    let group = &groups()[group_index];
    let stock = group
        .records
        .iter()
        .filter(|r| validate_layout(m, basis, group, r))
        .map(|r| Stock {
            pointer: basis + r.array as u64,
            names: r.names.clone(),
        })
        .collect::<Vec<_>>();
    ensure!(
        !stock.is_empty() && stock.iter().any(|s| s.pointer == original),
        "Original wardrobe changed"
    );
    let items = if let Some(page) = page {
        page.items.clone()
    } else {
        stock
            .iter()
            .find(|s| s.pointer == before.pointer)
            .context("Current source is not verified")?
            .names
            .to_vec()
    };
    let pool = clothing_pool(
        stock
            .iter()
            .flat_map(|s| s.names.iter().map(String::as_str)),
    );
    let mut name = format!(
        "{} {}",
        text(m, key.roster + 0x28, 40)?,
        text(m, key.roster, 40)?
    )
    .trim()
    .to_owned();
    ensure!(
        !name.is_empty() && !name.chars().any(char::is_control),
        "Invalid selected player name"
    );
    name.truncate(
        name.char_indices()
            .nth(80)
            .map(|(i, _)| i)
            .unwrap_or(name.len()),
    );
    let cyberface = u16::from_le_bytes(
        m.read(key.roster + 0x110, 2)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Incomplete cyberface"))?,
    );
    ensure!(
        source(m, &key)? == before,
        "Outfit changed during player inspection"
    );
    Ok(Player {
        key,
        source: before.clone(),
        original_pointer: original,
        name,
        cyberface,
        uniform_name: uniform(m, before.uniform),
        pool,
        stock,
        items,
        group_index,
        group_basis: basis,
    })
}
/// Fast revalidation of an already captured native player. The hint comes only
/// from our full CLOF capture, and every group/list/name is still rechecked.
/// Applying ten outfits must not repeat the entire 1791-record lookup per step.
pub(crate) fn revalidate(
    m: &impl crate::blacktop_arena::Inspect,
    game: &Identity,
    p: &Player,
    pages: &[Page],
) -> Result<()> {
    blacktop_native::validate_build(m)?;
    let c = blacktop_native::context(m)?;
    ensure!(
        blacktop_native::boundary(&c, m.base()) == "selection"
            && d(m, c.owner + 0xA04)? == 6
            && q(m, m.base() + ROOT + 0x1080)? == p.key.controller
            && m.read(p.key.controller + 0x18, 1)? == [1],
        "Selected outfit screen changed"
    );
    ensure!(
        source(m, &p.key)? == p.source,
        "Selected outfit source changed"
    );
    let group = groups()
        .get(p.group_index)
        .context("Native wardrobe hint is invalid")?;
    let expected = group
        .records
        .iter()
        .filter(|r| validate_layout(m, p.group_basis, group, r))
        .map(|r| Stock {
            pointer: p.group_basis + r.array as u64,
            names: r.names.clone(),
        })
        .collect::<Vec<_>>();
    ensure!(
        expected == p.stock && expected.iter().any(|s| s.pointer == p.original_pointer),
        "Previously verified wardrobe changed"
    );
    let items = if p.source.pointer == p.original_pointer {
        ensure!(p.source.count == 2, "Stock outfit count changed");
        expected
            .iter()
            .find(|s| s.pointer == p.original_pointer)
            .unwrap()
            .names
            .to_vec()
    } else {
        let page = pages
            .iter()
            .find(|v| v.allocation.checked_add(128) == Some(p.source.pointer))
            .context("Current private outfit is not owned")?;
        page.verify(m, game, &p.key, &p.source)?;
        page.items.clone()
    };
    ensure!(
        items == p.items
            && clothing_pool(
                expected
                    .iter()
                    .flat_map(|s| s.names.iter().map(String::as_str))
            ) == p.pool
            && uniform(m, p.source.uniform) == p.uniform_name,
        "Selected outfit definition changed"
    );
    let count = d(m, m.base() + LINEUP + 0xB8)?;
    let required = d(m, m.base() + LINEUP + 4)?;
    ensure!(
        (2..=10).contains(&required)
            && required % 2 == 0
            && count <= required
            && d(m, m.base() + LINEUP + 8)? == required,
        "Lineup counts changed"
    );
    let rows = (0..count)
        .map(|i| q(m, m.base() + LINEUP + 0xC0 + i as u64 * 8))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        rows.iter().filter(|r| **r == p.key.roster).count() == 1
            && rows.iter().enumerate().all(|(i, r)| !rows[..i].contains(r)),
        "Selected player left or is ambiguous in the lineup"
    );
    ensure!(
        source(m, &p.key)? == p.source
            && blacktop_native::context(m)? == c
            && q(m, m.base() + ROOT + 0x1080)? == p.key.controller
            && d(m, m.base() + LINEUP + 0xB8)? == count
            && m.read(p.key.controller + 0x18, 1)? == [1],
        "Player changed during outfit revalidation"
    );
    for (i, r) in rows.iter().enumerate() {
        ensure!(
            q(m, m.base() + LINEUP + 0xC0 + i as u64 * 8)? == *r,
            "Lineup changed during outfit revalidation"
        );
    }
    Ok(())
}
pub(crate) fn dirty(m: &impl Memory, key: &PlayerKey) -> Result<u8> {
    key.validate()?;
    let b = m.read(key.controller + 0x30 + key.slot as u64 * 0x7b0, 1)?;
    ensure!(b.len() == 1, "Incomplete outfit refresh byte");
    Ok(b[0])
}
pub(crate) fn capture(
    m: &impl crate::blacktop_arena::Inspect,
    game: &Identity,
    pages: &[Page],
) -> Result<Selection> {
    ensure!(
        pages.len() <= MAX_PAGES,
        "Private outfit reservation limit exceeded"
    );
    blacktop_native::validate_build(m)?;
    let context = blacktop_native::context(m)?;
    ensure!(
        blacktop_native::boundary(&context, m.base()) == "selection"
            && d(m, context.owner + 0xA04)? == 6,
        "Open Blacktop player selection"
    );
    let controller = q(m, m.base() + ROOT + 0x1080)?;
    span(controller, 0x7b0 * 10 + 0x30)?;
    ensure!(
        m.read(controller + 0x18, 1)? == [1],
        "Blacktop preview models are not ready"
    );
    let required = d(m, m.base() + LINEUP + 4)?;
    let selected = d(m, m.base() + LINEUP + 0xB8)?;
    ensure!(
        (2..=10).contains(&required)
            && required % 2 == 0
            && selected <= required
            && d(m, m.base() + LINEUP + 8)? == required,
        "Lineup counts changed"
    );
    let lineup = (0..selected)
        .map(|i| q(m, m.base() + LINEUP + 0xC0 + i as u64 * 8))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        lineup.iter().all(|p| *p >= 65536 && *p < 0x8000_0000_0000)
            && lineup
                .iter()
                .enumerate()
                .all(|(i, p)| !lineup[..i].contains(p)),
        "Lineup player identity is invalid or ambiguous"
    );
    let mut players = Vec::new();
    let mut unavailable = Vec::new();
    for slot in 0..10u8 {
        m.checkpoint()?;
        let entry = controller + 0x20 + slot as u64 * 0x7b0;
        if q(m, entry)? == 0 && q(m, entry + 8)? == 0 {
            continue;
        }
        match read_player(m, game, controller, slot, pages) {
            Ok(player) if lineup.contains(&player.key.roster) => players.push(player),
            _ => unavailable.push(slot),
        }
    }
    ensure!(
        blacktop_native::context(m)? == context
            && q(m, m.base() + ROOT + 0x1080)? == controller
            && d(m, m.base() + LINEUP + 4)? == required
            && d(m, m.base() + LINEUP + 8)? == required
            && d(m, m.base() + LINEUP + 0xB8)? == selected
            && m.read(controller + 0x18, 1)? == [1],
        "Blacktop selection changed during capture"
    );
    for (i, roster) in lineup.iter().enumerate() {
        ensure!(
            q(m, m.base() + LINEUP + 0xC0 + i as u64 * 8)? == *roster,
            "Lineup changed during capture"
        );
    }
    for p in &players {
        ensure!(
            source(m, &p.key)? == p.source,
            "Selected outfit changed during capture"
        );
    }
    ensure!(
        players
            .iter()
            .enumerate()
            .all(|(i, p)| !players[..i].iter().any(|o| o.key.roster == p.key.roster)),
        "Duplicate selected player identity"
    );
    Ok(Selection {
        context,
        controller,
        required,
        selected,
        lineup,
        players,
        unavailable,
        reasons: Vec::new(),
    })
}
#[derive(Clone, Debug)]
pub(crate) struct Composed {
    pub selection: outfits::Selection,
    pub street_index: usize,
    pub items: Vec<String>,
    pub roster_shoes: bool,
}
pub(crate) fn compose(player: &Player, outfit: &crate::backend::Outfit) -> Result<Composed> {
    let catalog = outfits::catalog()?;
    crate::backend::validate_outfit(outfit)?;
    let selection = catalog.selection(outfit)?;
    let preset = catalog.base(&selection.street_id)?;
    ensure!(
        player.pool.is_some() && preset.pool == player.pool,
        "This outfit's clothing collection does not match the selected player's wardrobe"
    );
    let items = catalog.compose(&selection)?;
    ensure!(
        (2..=7).contains(&items.len()) && items.iter().all(|s| valid_item(s)),
        "Unsupported clothing item list"
    );
    ensure!(
        !items.iter().any(|s| s.starts_with("UNIFORM")) || player.uniform_name.is_some(),
        "This outfit requires the selected player's own verified uniform context"
    );
    let street_index = catalog
        .presets
        .iter()
        .position(|p| p.id == selection.street_id)
        .context("Unknown native top preset")?;
    let roster_shoes = selection.shoe.as_deref() == Some("roster");
    Ok(Composed {
        selection,
        street_index,
        items,
        roster_shoes,
    })
}
#[cfg(test)]
#[path = "blacktop_outfits_tests.rs"]
pub(crate) mod tests;
