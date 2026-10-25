//! Narrow Version 2 direct GameData rules profile. No process APIs, native menu
//! hooks or implicit writes. Runtime adapters must independently retain the
//! owned game identity and require the complete owner span to be private RW,
//! committed, non-executable memory before accepting a write.
#![allow(dead_code)]
use crate::{
    blacktop_native::{ContextState, Memory},
    game_rules::{FIELDS, Snapshot},
};
use anyhow::{Context, Result, ensure};
use std::sync::OnceLock;

pub(crate) const OWNER: u64 = 0x0AB5CE20;
const ROOT: u64 = 0x0AF46870;
const ROOT_TAG: u64 = 0x0791E158;
const MODES: u64 = 0x0AD07408;
const CURRENT: u64 = 0x0AD07BD0;
const ALTERNATE: u64 = 0x0AD07BCC;
const ACTIVE: u64 = 0x0AB5D790;
const SHA: &str = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
const IMAGE_BYTES: u64 = 0x36532000;
pub(crate) const RIP_ANCHORS: &[(u64, &[u8], u64)] = &[
    (0x3457F00, &[0x48, 0x8D, 0x05], ROOT_TAG),
    (0x3457F07, &[0x48, 0x89, 0x05], ROOT),
    (0x25AA480, &[0x48, 0x8B, 0x05], OWNER),
    (0x308C6F9, &[0x48, 0x8D, 0x05], MODES),
    (0x308C700, &[0x48, 0x63, 0x0D], ALTERNATE),
    (0x300D7B3, &[0x48, 0x63, 0x05], CURRENT),
];
pub(crate) const FIELD_ANCHORS: &[(u64, &[u8])] = &[
    (0x32FBB36, &[0x8B, 0x81, 0xA8, 0x0E, 0, 0]),
    (0x24F7F75, &[0x80, 0x3D, 0x14, 0x58, 0x66, 0x08, 0]),
    // Native mode getter loads the same rules owner, then its +A04 mode.
    (
        0x24F5500,
        &[
            0x48, 0x8B, 0x05, 0x19, 0x79, 0x66, 0x08, 0x8B, 0x80, 0x04, 0x0A, 0, 0, 0xC3,
        ],
    ),
];

pub(crate) fn updated_getter(id: &str, old: u64) -> u64 {
    match id {
        "OverTheBackRule" => 0x25B0BA0,
        "GenericPersonalFoulRule" => 0x25B0840,
        "ChargingRule" => 0x25B0A20,
        "OffensiveOffballFoulRule" => 0x25B0A80,
        "DefensiveOffballFoulRule" => 0x25B0AE0,
        "BlockingRule" => 0x25B09C0,
        "ReachingFoulRule" => 0x25B08A0,
        "LooseBallRule" => 0x25B0C00,
        "ShootingFoulRule" => 0x25B0900,
        "IntentionalFoul" => 0x25B0C60,
        "IllegalPick" => 0x25B0B40,
        "TechnicalFoulRule" => 0x25B0960,
        "FlagrantFoulRule" => 0x25B07E0,
        _ => old + 0xB0,
    }
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        len > 0
            && len <= 4096
            && at >= 0x10000
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff00000000),
        "Invalid bounded Game Rules address"
    );
    m.checkpoint()?;
    let bytes = m.read(at, len)?;
    ensure!(bytes.len() == len, "Partial Game Rules read");
    Ok(bytes)
}
fn rva(m: &impl Memory, offset: u64) -> Result<u64> {
    m.base()
        .checked_add(offset)
        .context("Game Rules RVA overflow")
}
fn u32_at(m: &impl Memory, at: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(read(m, at, 4)?.as_slice().try_into()?))
}
fn u64_at(m: &impl Memory, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, at, 8)?.as_slice().try_into()?))
}
fn d(m: &impl Memory, offset: u64) -> Result<u32> {
    u32_at(m, rva(m, offset)?)
}
fn q(m: &impl Memory, offset: u64) -> Result<u64> {
    u64_at(m, rva(m, offset)?)
}
fn hex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        s.len() % 2 == 0 && s.len() <= 128,
        "Invalid Game Rules anchor hex"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
fn catalog() -> Result<&'static serde_json::Value> {
    // Cache only immutable embedded reference data, never a live process
    // snapshot. Failed parsing/validation leaves the cell empty so callers
    // retain the same Result failure path and may try again.
    static CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    if let Some(value) = CATALOG.get() {
        return Ok(value);
    }
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../assets/rules-v2-getters.json"))?;
    ensure!(
        value["schema"] == "nba2k27-v2-game-rules-getters-v1"
            && value["sha256"] == SHA
            && value["gameProcessAccess"] == false,
        "Invalid Game Rules reference"
    );
    // Concurrent first callers may both parse; only a validated reference is
    // published, and all callers receive the same immutable successful value.
    let _ = CATALOG.set(value);
    Ok(CATALOG.get().expect("validated Game Rules reference"))
}
// Batch only this call's immutable code proof. A failed/partial grouped read
// retries the original complete scalar proof, never accepting unread bytes.
fn verify_getters(m: &impl Memory, getters: &[(u64, Vec<u8>, &str)]) -> Result<()> {
    let mut groups = std::collections::BTreeMap::<u64, (u64, u64)>::new();
    for (at, expected, _) in getters {
        let end = at
            .checked_add(expected.len() as u64)
            .context("Getter span overflow")?;
        let span = groups.entry(at / 4096).or_insert((*at, end));
        span.0 = span.0.min(*at);
        span.1 = span.1.max(end);
    }
    let mut failed = false;
    for (page, (start, end)) in groups {
        match read(m, rva(m, start)?, (end - start) as usize) {
            Ok(bytes) => {
                // Never erase an observed complete-byte mismatch by retrying
                // after a later group's transport failure.
                for (at, expected, id) in getters.iter().filter(|(at, _, _)| at / 4096 == page) {
                    let offset = (at - start) as usize;
                    ensure!(
                        bytes.get(offset..offset + expected.len()) == Some(expected.as_slice()),
                        "Version 2 Game Rules getter changed for {}",
                        id
                    );
                }
            }
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    if failed {
        for (at, expected, id) in getters {
            ensure!(
                read(m, rva(m, *at)?, expected.len())? == *expected,
                "Version 2 Game Rules getter changed for {}",
                id
            );
        }
    }
    Ok(())
}
/// Read-only executable proof. Every mapped field has its own complete native
/// getter whose RIP-relative owner load resolves to OWNER; field offsets and
/// byte/DWORD widths are not inferred from a uniform build shift.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    let bytes = read(m, m.base(), 4096)?;
    ensure!(&bytes[..2] == b"MZ", "Invalid Game Rules executable header");
    let pe = u32::from_le_bytes(bytes[60..64].try_into()?) as usize;
    ensure!(
        (64..=3500).contains(&pe)
            && &bytes[pe..pe + 4] == b"PE\0\0"
            && &bytes[pe + 4..pe + 6] == [0x64, 0x86]
            && &bytes[pe + 24..pe + 26] == [0x0B, 0x02]
            && u32::from_le_bytes(bytes[pe + 8..pe + 12].try_into()?) == 0x6AD2E48C
            && u32::from_le_bytes(bytes[pe + 80..pe + 84].try_into()?) == IMAGE_BYTES as u32,
        "Game Rules are not mapped for this game build"
    );
    let value = catalog()?;
    let fields = value["fields"]
        .as_array()
        .context("Missing Game Rules getter catalog")?;
    ensure!(
        fields.len() == 39 && FIELDS.len() == 39,
        "Game Rules catalog count changed"
    );
    let mut getters = Vec::with_capacity(fields.len());
    for (getter, field) in fields.iter().zip(FIELDS) {
        ensure!(
            getter["id"] == field.id
                && getter["offset"].as_u64() == Some(field.offset)
                && getter["width"].as_u64() == Some(field.width as u64)
                && getter["ownerRva"].as_u64() == Some(OWNER),
            "Game Rules field mapping differs from verified reference"
        );
        let old_at = getter["rva"]
            .as_u64()
            .context("Missing rule getter address")?;
        let at = updated_getter(field.id, old_at);
        let mut expected = hex(getter["hex"]
            .as_str()
            .context("Missing rule getter bytes")?)?;
        let relative = i32::try_from(OWNER as i64 - at as i64 - 7)
            .context("Updated Game Rules getter displacement overflow")?;
        expected[3..7].copy_from_slice(&relative.to_le_bytes());
        ensure!(
            at >= 0x1000
                && at + (expected.len() as u64) < 0x5D00000
                && expected.starts_with(&[0x48, 0x8B, 0x05])
                && expected.last() == Some(&0xC3),
            "Invalid native rule getter reference"
        );
        let target = at as i64 + 7 + i32::from_le_bytes(expected[3..7].try_into()?) as i64;
        ensure!(
            target == OWNER as i64,
            "Version 2 Game Rules getter changed for {}",
            field.id
        );
        getters.push((at, expected, field.id));
    }
    verify_getters(m, &getters)?;
    for &(offset, prefix, target) in RIP_ANCHORS {
        let code = read(m, rva(m, offset)?, prefix.len() + 4)?;
        ensure!(
            code.starts_with(prefix)
                && offset as i64
                    + code.len() as i64
                    + i32::from_le_bytes(code[prefix.len()..].try_into()?) as i64
                    == target as i64,
            "Version 2 Game Rules context anchor changed"
        );
    }
    for &(offset, expected) in FIELD_ANCHORS {
        ensure!(
            read(m, rva(m, offset)?, expected.len())? == expected,
            "Version 2 Game Rules context field changed"
        );
    }
    let table = read(m, rva(m, ROOT_TAG)?, 16)?;
    ensure!(
        u64::from_le_bytes(table[..8].try_into()?) == rva(m, 0x23120)?
            && u64::from_le_bytes(table[8..].try_into()?) == rva(m, 0x346B4D0)?,
        "Version 2 Blacktop class identity changed"
    );
    m.checkpoint()
}
/// Live 0.47/0.48 probes at Version 2 Blacktop selection: the stable state
/// (stage 3, active 0, kind 0, gameMode 6) blips to active 1 / kind 1 /
/// gameMode 4 for sub-millisecond samples several times a second while every
/// owner and data field stays put (artifacts/qa047-presentation-selection-
/// context-probe.txt). Every Version 2 consumer compares contexts before and
/// after reading, so this returns the strict majority of identical samples;
/// a real mode change persists and wins the majority on the next read.
pub(crate) const CONTEXT_SAMPLES: usize = 5;
pub(crate) fn context(m: &impl Memory) -> Result<ContextState> {
    let mut samples = Vec::with_capacity(CONTEXT_SAMPLES);
    for _ in 0..CONTEXT_SAMPLES {
        samples.push(context_once(m)?);
        // Common case: two consecutive identical samples. A sub-millisecond
        // blip is a single sample, so it never repeats back-to-back; only a
        // disagreement pays for the full majority.
        if samples.len() == 2 && samples[0] == samples[1] {
            return Ok(samples.swap_remove(1));
        }
    }
    let count = |c: &ContextState| samples.iter().filter(|s| *s == c).count();
    let best = samples
        .iter()
        .max_by_key(|c| count(c))
        .context("Missing Version 2 mode sample")?;
    ensure!(
        count(best) * 2 > CONTEXT_SAMPLES,
        "Version 2 mode is still changing"
    );
    Ok(best.clone())
}
fn context_once(m: &impl Memory) -> Result<ContextState> {
    let current = d(m, CURRENT)?;
    let alternate = d(m, ALTERNATE)?;
    ensure!(
        current < 128 && alternate < 128,
        "Game Rules mode is initializing"
    );
    let owner = q(m, OWNER)?;
    ensure!(
        owner >= 0x10000
            && owner % 8 == 0
            && owner
                .checked_add(0xA9C)
                .is_some_and(|end| end <= 0x7fff00000000)
            && !(m.base()..m.base() + IMAGE_BYTES).contains(&owner),
        "Game Rules owner is unsupported"
    );
    Ok(ContextState {
        current_index: current,
        alternate_index: alternate,
        current: d(m, MODES + current as u64 * 64)?,
        alternate: d(m, MODES + alternate as u64 * 64)?,
        stage: d(m, ROOT + 0xEA8)?,
        active: read(m, rva(m, ACTIVE)?, 1)?[0],
        owner,
        kind: read(m, owner, 1)?[0],
        game_mode: u32_at(m, owner + 0xA04)?,
        root_tag: q(m, ROOT)?,
    })
}
pub(crate) fn boundary(context: &ContextState, base: u64) -> &'static str {
    if context.current != context.alternate {
        return "transition";
    }
    if context.current == 0x92DF06DE
        && context.stage == 3
        && context.active == 0
        && context.kind == 0
        && base.checked_add(ROOT_TAG) == Some(context.root_tag)
    {
        return "selection";
    }
    if context.active == 0 && context.kind == 0 {
        // Version 2 Main was observed live at stage 0 after leaving a mode
        // (artifacts/qa044-camera-live-failure.json); the legacy build used 1.
        // Same inactive Main proof as feature_writer::inactive_camera_menu.
        if context.current == 0x6509837B && context.stage <= 1 {
            return "main-menu";
        }
        if context.current == 0x079964C0 && context.stage == 1 {
            return "quick-play";
        }
    }
    if context.game_mode == 6
        && context.stage == 1
        && [0xB4D37B22, 0x3EEF98B9, 0x7C602DF8, 0x50BAD898].contains(&context.current)
    {
        return "match";
    }
    "transition-or-other-mode"
}
pub(crate) fn capture(m: &impl Memory) -> Result<Snapshot> {
    verify_profile(m)?;
    let before = context(m)?;
    ensure!(
        matches!(boundary(&before, m.base()), "selection" | "match"),
        "Open Blacktop player selection or a Blacktop match before reading rules"
    );
    let values = FIELDS
        .iter()
        .map(|field| {
            let bytes = read(m, before.owner + field.offset, field.width)?;
            Ok(if field.width == 1 {
                bytes[0] as u32
            } else {
                u32::from_le_bytes(bytes.as_slice().try_into()?)
            })
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        context(m)? == before,
        "Game Rules mode/owner changed during reading"
    );
    for (field, value) in FIELDS.iter().zip(&values) {
        ensure!(
            read(m, before.owner + field.offset, field.width)?
                == value.to_le_bytes()[..field.width],
            "Game Rules group changed during reading"
        );
    }
    ensure!(
        context(m)? == before,
        "Game Rules mode/owner changed after reading"
    );
    m.checkpoint()?;
    Ok(Snapshot {
        context: before,
        base: m.base(),
        values,
    })
}
#[derive(Clone, Debug)]
pub(crate) struct Change {
    pub at: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub index: usize,
    pub expected: Snapshot,
}
pub(crate) fn plan(snapshot: &Snapshot, field: &str, after: u32) -> Result<Change> {
    ensure!(
        snapshot.values.len() == FIELDS.len()
            && matches!(
                boundary(&snapshot.context, snapshot.base),
                "selection" | "match"
            ),
        "Unsupported Game Rules snapshot"
    );
    let index = FIELDS
        .iter()
        .position(|f| f.id == field)
        .context("Unknown Game Rule")?;
    let field = &FIELDS[index];
    let before = snapshot.values[index];
    ensure!(
        field.valid(before) && field.valid(after) && before != after,
        "Choose a different supported Game Rules value"
    );
    let mut expected = snapshot.clone();
    expected.values[index] = after;
    Ok(Change {
        at: snapshot
            .context
            .owner
            .checked_add(field.offset)
            .context("Game Rules field overflow")?,
        before: before.to_le_bytes()[..field.width].to_vec(),
        after: after.to_le_bytes()[..field.width].to_vec(),
        index,
        expected,
    })
}
pub(crate) fn verify_expected(m: &impl Memory, expected: &Snapshot) -> Result<()> {
    ensure!(
        capture(m)? == *expected,
        "Game Rules or mode changed; refresh current values"
    );
    Ok(())
}

#[cfg(test)]
#[path = "rules_v2_tests.rs"]
pub(crate) mod tests;
