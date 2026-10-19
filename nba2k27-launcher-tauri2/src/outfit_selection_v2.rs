//! Independently mapped Version 2 selection-only outfit decoding.
//! Reuses immutable CLOF/catalog/page data, never the legacy native write gate.
//! No match actor clone, queue, rebuild hook or worker is supported here.
#![allow(dead_code)]
use crate::{
    blacktop_native::Memory,
    blacktop_outfits::{
        MAX_PAGES, Page, Player, PlayerKey, Selection, Source, Stock, clothing_pool, d, groups, q,
        span, validate_layout,
    },
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};

pub(crate) const ROOT: u64 = 0x0AF46870;
pub(crate) const LINEUP: u64 = 0x09C496C0;
pub(crate) const MODEL: u64 = 0x067DD188;
pub(crate) const DESCRIPTION: u64 = 0x068920C8;
pub(crate) const MATCH_CARRY_OVER_SUPPORTED: bool = false;
// Each descriptor field is tied to a distinct native virtual method, not a
// version delta. Selection accessors independently prove controller readiness,
// +20/+28, stride and the refresh byte.
pub(crate) const FIELD_ANCHORS: &[(u64, &[u8])] = &[
    (0x3B0006, &[0x80, 0x79, 0x18, 1]),
    (
        0x3B000F,
        &[
            0x48, 0x69, 0xF0, 0xB0, 7, 0, 0, 0x48, 0x8B, 0x44, 0x31, 0x20,
        ],
    ),
    (
        0x3B015B,
        &[0x48, 0x69, 0xC0, 0xB0, 7, 0, 0, 0x48, 0x8B, 0x4C, 1, 0x28],
    ),
    (
        0x3B2050,
        &[
            0x83, 0xFA, 0x1E, 0x77, 0x0E, 0x89, 0xD0, 0x48, 0x69, 0xC0, 0xB0, 7, 0, 0, 0xC6, 0x44,
            1, 0x30, 1, 0xC3,
        ],
    ),
    (0x32FB9E0, &[0x48, 0x8B, 0x8E, 0x80, 0x10, 0, 0]),
    (
        0x17CC50,
        &[
            0x48, 0x89, 0x91, 0xC8, 0, 0, 0, 0x44, 0x89, 0x81, 0xD0, 0, 0, 0, 0xC3,
        ],
    ),
    (0x17CC60, &[0x48, 0x8B, 0x81, 0xC8, 0, 0, 0, 0xC3]),
    (0x17CC70, &[0x8B, 0x81, 0xD0, 0, 0, 0, 0xC3]),
    (0x803A80, &[0x48, 0x8B, 0x81, 0xF0, 0, 0, 0, 0xC3]),
    (0x827120, &[0x48, 0x89, 0x91, 0xF0, 0, 0, 0, 0xC3]),
    (
        0x72A760,
        &[
            0x48, 0x8B, 0x81, 0xB8, 0, 0, 0, 0x48, 0x85, 0xC0, 0x74, 8, 0x0F, 0xB7, 0x80, 0x10, 1,
            0, 0, 0xC3, 0x31, 0xC0, 0xC3,
        ],
    ),
];
pub(crate) const VIRTUAL_ANCHORS: &[(u64, u64, u64)] = &[
    (MODEL, 0, 0x3D8A90),
    (MODEL, 8, 0x3D8AF0),
    (DESCRIPTION, 0, 0x14F0),
    (DESCRIPTION, 8, 0x827040),
    (DESCRIPTION, 0xC8, 0x17CC50),
    (DESCRIPTION, 0xD8, 0x17CC60),
    (DESCRIPTION, 0xE0, 0x17CC70),
    (DESCRIPTION, 0x100, 0x803A80),
    (DESCRIPTION, 0x1B0, 0x827120),
    (DESCRIPTION, 0x120, 0x72A760),
];
// Group only fresh immutable anchor bytes within one image page. Complete
// mismatches reject immediately; transport/partial failures re-read the entire
// scalar proof. No live bytes survive this invocation or publication boundary.
fn verify_anchors(m: &impl Memory, anchors: &[(u64, Vec<u8>, &'static str)]) -> Result<()> {
    let mut groups = std::collections::BTreeMap::<u64, (u64, u64)>::new();
    for (at, bytes, _) in anchors {
        let end = at
            .checked_add(bytes.len() as u64)
            .context("Outfit anchor overflow")?;
        ensure!(
            !bytes.is_empty() && (at / 4096) == ((end - 1) / 4096),
            "Outfit anchor page bounds"
        );
        let group = groups.entry(at / 4096).or_insert((*at, end));
        group.0 = group.0.min(*at);
        group.1 = group.1.max(end);
    }
    let mut fallback = false;
    for (page, (start, end)) in groups {
        let at = m
            .base()
            .checked_add(start)
            .context("Outfit anchor RVA overflow")?;
        match m.read(at, (end - start) as usize) {
            Ok(bytes) if bytes.len() == (end - start) as usize => {
                for (rva, expected, reason) in
                    anchors.iter().filter(|(rva, _, _)| rva / 4096 == page)
                {
                    let offset = (rva - start) as usize;
                    ensure!(
                        bytes.get(offset..offset + expected.len()) == Some(expected.as_slice()),
                        "{reason}"
                    );
                }
            }
            _ => {
                fallback = true;
                break;
            }
        }
    }
    if fallback {
        for (at, expected, reason) in anchors {
            ensure!(
                m.read(
                    m.base()
                        .checked_add(*at)
                        .context("Outfit anchor RVA overflow")?,
                    expected.len()
                )? == *expected,
                "{reason}"
            );
        }
    }
    Ok(())
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    crate::rules_v2::verify_profile(m)?;
    let mut anchors = FIELD_ANCHORS
        .iter()
        .map(|(at, bytes)| {
            (
                *at,
                bytes.to_vec(),
                "Version 2 outfit field/refresh method changed",
            )
        })
        .collect::<Vec<_>>();
    for &(table, offset, method) in VIRTUAL_ANCHORS {
        anchors.push((
            table + offset,
            m.base()
                .checked_add(method)
                .context("Outfit virtual method RVA overflow")?
                .to_le_bytes()
                .to_vec(),
            "Version 2 outfit class virtual method changed",
        ));
    }
    // Exact RIP stores still prove both count addresses independently. The
    // reference is rebuilt each call; grouped transport changes no comparison.
    for (at, target) in [(0x32F799Au64, LINEUP + 4), (0x32F79A0, LINEUP + 8)] {
        let mut expected = vec![0x89, 0x05];
        expected.extend_from_slice(&i32::try_from(target as i64 - (at + 6) as i64)?.to_le_bytes());
        anchors.push((at, expected, "Version 2 outfit lineup directory changed"));
    }
    verify_anchors(m, &anchors)?;
    m.checkpoint()
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
// A bounded read is only an ephemeral observation at this exact proof point.
// If the wider span is unreadable (e.g. a region boundary), re-read every field
// through the original scalar path; never mix a partial batch with old values.
fn block<const N: usize>(m: &impl Memory, at: u64) -> Option<[u8; N]> {
    m.read(at, N).ok()?.try_into().ok()
}
pub(crate) fn source(m: &impl Memory, key: &PlayerKey) -> Result<Source> {
    key.validate()?;
    let entry = key.controller + 0x20 + key.slot as u64 * 0x7b0;
    let (model, description) = if let Some(bytes) = block::<16>(m, entry) {
        (
            u64::from_le_bytes(bytes[..8].try_into()?),
            u64::from_le_bytes(bytes[8..].try_into()?),
        )
    } else {
        (q(m, entry)?, q(m, entry + 8)?)
    };
    ensure!(
        key.slot < 10
            && model == key.model
            && description == key.description
            && q(m, key.model)? == m.base() + MODEL
            && q(m, key.description)? == m.base() + DESCRIPTION,
        "Selected player identity changed"
    );
    let (roster, source) = if let Some(bytes) = block::<64>(m, key.description + 0xB8) {
        (
            u64::from_le_bytes(bytes[..8].try_into()?),
            Source {
                pointer: u64::from_le_bytes(bytes[0x10..0x18].try_into()?),
                count: u32::from_le_bytes(bytes[0x18..0x1C].try_into()?),
                uniform: u64::from_le_bytes(bytes[0x38..0x40].try_into()?),
            },
        )
    } else {
        (
            q(m, key.description + 0xB8)?,
            Source {
                pointer: q(m, key.description + 0xC8)?,
                count: d(m, key.description + 0xD0)?,
                uniform: q(m, key.description + 0xF0)?,
            },
        )
    };
    ensure!(roster == key.roster, "Selected player identity changed");
    Ok(source)
}
pub(crate) fn lineup_rows(m: &impl Memory, count: u32) -> Result<Vec<u64>> {
    ensure!(count <= 10, "Lineup exceeds bounded selected-player limit");
    if count == 0 {
        return Ok(Vec::new());
    }
    let at = m.base() + LINEUP + 0xC0;
    let length = count as usize * 8;
    if let Ok(bytes) = m.read(at, length) {
        if bytes.len() == length {
            return bytes
                .chunks_exact(8)
                .map(|value| Ok(u64::from_le_bytes(value.try_into()?)))
                .collect();
        }
    }
    (0..count)
        .map(|index| q(m, at + index as u64 * 8))
        .collect()
}

type LayoutHints = std::collections::HashMap<(u64, u64), (usize, usize)>;
static LAYOUT_HINTS: std::sync::Mutex<Option<LayoutHints>> = std::sync::Mutex::new(None);
fn layout_hint(base: u64, original: u64) -> Option<(usize, usize)> {
    LAYOUT_HINTS
        .lock()
        .ok()?
        .as_ref()?
        .get(&(base, original))
        .copied()
}
fn remember_layout(base: u64, original: u64, group: usize, record: usize) {
    if let Ok(mut hints) = LAYOUT_HINTS.lock() {
        let map = hints.get_or_insert_with(Default::default);
        if map.len() >= 256 {
            map.clear();
        }
        map.insert((base, original), (group, record));
    }
}
#[cfg(test)]
pub(crate) fn clear_layout_hints() {
    if let Ok(mut hints) = LAYOUT_HINTS.lock() {
        *hints = None;
    }
}
// Ephemeral search hints only. Counts read in bounded pages nominate layouts;
// every nominated layout still runs the complete FRESH scalar proof. A failed
// prefetch or no proven nominee falls back to the original exhaustive search.
fn prefilter_layout(m: &impl Memory, original: u64) -> Result<Option<(u64, usize, usize)>> {
    let mut pages: std::collections::HashMap<u64, Option<Vec<u8>>> = Default::default();
    for (group_index, group) in groups().iter().enumerate() {
        for (record_index, record) in group.records.iter().enumerate() {
            let Some(basis) = original.checked_sub(record.array as u64) else {
                continue;
            };
            let Some(probe) = basis.checked_add(16) else {
                continue;
            };
            let page = probe & !4095;
            let offset = (probe - page) as usize;
            if page < 65536 || offset + 8 > 4096 {
                continue;
            }
            if !pages.contains_key(&page) {
                if pages.len() >= 128 {
                    return Ok(None);
                }
                m.checkpoint()?;
                let bytes = m.read(page, 4096).ok().filter(|bytes| bytes.len() == 4096);
                pages.insert(page, bytes);
            }
            if let Some(bytes) = pages.get(&page).and_then(Option::as_ref) {
                if u64::from_le_bytes(bytes[offset..offset + 8].try_into()?) == group.count as u64 {
                    m.checkpoint()?;
                    if validate_layout(m, basis, group, record) {
                        return Ok(Some((basis, group_index, record_index)));
                    }
                }
            }
        }
    }
    Ok(None)
}
fn exhaustive_layout(m: &impl Memory, original: u64) -> Result<Option<(u64, usize, usize)>> {
    for (group_index, group) in groups().iter().enumerate() {
        for (record_index, record) in group.records.iter().enumerate() {
            m.checkpoint()?;
            let Some(basis) = original.checked_sub(record.array as u64) else {
                continue;
            };
            if validate_layout(m, basis, group, record) {
                return Ok(Some((basis, group_index, record_index)));
            }
        }
    }
    Ok(None)
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
    // Search-order hint only: the group that last proved this stock pointer is
    // tried first and must pass the same full validate_layout proof; any miss
    // falls back to the complete catalog search below (Random all speed).
    if let Some((group_index, record_index)) = layout_hint(m.base(), original) {
        if let Some((group, record)) = groups()
            .get(group_index)
            .and_then(|g| g.records.get(record_index).map(|r| (g, r)))
        {
            m.checkpoint()?;
            if let Some(basis) = original.checked_sub(record.array as u64) {
                if validate_layout(m, basis, group, record) {
                    found = Some((basis, group_index));
                }
            }
        }
    }
    if found.is_none() {
        let nominee = prefilter_layout(m, original)?;
        let proven = match nominee {
            Some(value) => Some(value),
            None => exhaustive_layout(m, original)?,
        };
        if let Some((basis, group_index, record_index)) = proven {
            found = Some((basis, group_index));
            remember_layout(m.base(), original, group_index, record_index);
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
    verify_profile(m)?;
    let c = crate::rules_v2::context(m)?;
    ensure!(
        crate::rules_v2::boundary(&c, m.base()) == "selection"
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
    let rows = lineup_rows(m, count)?;
    ensure!(
        rows.iter().filter(|r| **r == p.key.roster).count() == 1
            && rows.iter().enumerate().all(|(i, r)| !rows[..i].contains(r)),
        "Selected player left or is ambiguous in the lineup"
    );
    ensure!(
        source(m, &p.key)? == p.source
            && crate::rules_v2::context(m)? == c
            && q(m, m.base() + ROOT + 0x1080)? == p.key.controller
            && d(m, m.base() + LINEUP + 0xB8)? == count
            && d(m, m.base() + LINEUP + 4)? == required
            && d(m, m.base() + LINEUP + 8)? == required
            && m.read(p.key.controller + 0x18, 1)? == [1],
        "Player changed during outfit revalidation"
    );
    ensure!(
        lineup_rows(m, count)? == rows,
        "Lineup changed during outfit revalidation"
    );
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
    verify_profile(m)?;
    let context = crate::rules_v2::context(m)?;
    let boundary = crate::rules_v2::boundary(&context, m.base());
    let game_mode = d(m, context.owner + 0xA04)?;
    // WPF parity (BlacktopTool.cs:207/399): selection is the Blacktop root
    // vtable + stage 3 only. Game mode 6 is a MATCH test (version-proxy
    // in_match); V2 reports game mode 0 at selection when entered some ways
    // (0.50 live), so it is not required here. The error still names it.
    ensure!(
        boundary == "selection",
        "Open Blacktop player selection (game reports {boundary}, mode {:08X}/{:08X}, stage {}, game mode {game_mode})",
        context.current,
        context.alternate,
        context.stage
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
    let lineup = lineup_rows(m, selected)?;
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
    let mut reasons = Vec::new();
    for slot in 0..10u8 {
        m.checkpoint()?;
        let entry = controller + 0x20 + slot as u64 * 0x7b0;
        if q(m, entry)? == 0 && q(m, entry + 8)? == 0 {
            continue;
        }
        match read_player(m, game, controller, slot, pages) {
            Ok(player) if lineup.contains(&player.key.roster) => players.push(player),
            Ok(_) => {
                unavailable.push(slot);
                reasons.push((slot, "Not in the selected Blacktop lineup yet".into()));
            }
            Err(e) => {
                unavailable.push(slot);
                reasons.push((slot, format!("{e:#}").chars().take(200).collect()));
            }
        }
    }
    ensure!(
        crate::rules_v2::context(m)? == context
            && q(m, m.base() + ROOT + 0x1080)? == controller
            && d(m, m.base() + LINEUP + 4)? == required
            && d(m, m.base() + LINEUP + 8)? == required
            && d(m, m.base() + LINEUP + 0xB8)? == selected
            && m.read(controller + 0x18, 1)? == [1],
        "Blacktop selection changed during capture"
    );
    ensure!(
        lineup_rows(m, selected)? == lineup,
        "Lineup changed during capture"
    );
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
        reasons,
    })
}

#[cfg(test)]
#[path = "outfit_selection_v2_tests.rs"]
mod tests;
