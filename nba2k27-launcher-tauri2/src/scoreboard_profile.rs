//! Production WPF scoreboard catalog and three-scalar planner, ported without
//! native calls/hooks. Only the private descriptor reader may eventually grant
//! runtime authority; this module performs no process or filesystem writes.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const STOCK: u32 = 0x39264490;
const NAME: u32 = 0xFE11D138;
const RESOURCE: u32 = 0x45E79640;
const CLASS: u32 = 0x2C8A369B;
const MODE: u32 = 0xFEC8E933;
const HANDLER: u32 = 0xC63510F0;
const LEGACY: [u32; 3] = [0x2A22DFCC, MODE, 0xAFA29832];
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Choice {
    pub id: String,
    pub label: String,
    pub resource: u32,
    pub file: String,
    pub profile: [u32; 3],
    pub requires_layout_adapter: bool,
    pub selectable: bool,
}
pub(crate) fn catalog() -> Result<Vec<Choice>> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Catalog {
        schema: String,
        source_sha256: [String; 2],
        choices: Vec<Choice>,
    }
    let catalog: Catalog =
        serde_json::from_str(include_str!("../assets/scoreboard-native-catalog.json"))?;
    ensure!(
        catalog.schema == "nba2k27-scoreboard-native-catalog-v1"
            && catalog.choices.len() == 21
            && catalog.choices.iter().filter(|c| c.selectable).count() == 19,
        "Scoreboard catalog contract changed"
    );
    ensure!(
        catalog
            .source_sha256
            .iter()
            .all(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
        "Scoreboard source hashes missing"
    );
    let mut ids = BTreeSet::new();
    let mut resources = BTreeSet::new();
    let mut files = BTreeSet::new();
    for choice in &catalog.choices {
        ensure!(
            !choice.id.is_empty()
                && choice.id.len() <= 64
                && choice
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                && ids.insert(&choice.id)
                && resources.insert(choice.resource)
                && files.insert(&choice.file)
                && choice.file.ends_with(".iff")
                && choice
                    .file
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.')
                && [
                    LEGACY,
                    [0x3A09A326, MODE, 0xAFA29832],
                    [0x75AFE63A, HANDLER, 0x759F1AB6]
                ]
                .contains(&choice.profile)
                && choice.selectable
                    == !matches!(choice.id.as_str(), "mycareer-story" | "flashback-openrun"),
            "Invalid scoreboard catalog row"
        );
    }
    ensure!(
        catalog.choices[0].id == "default" && catalog.choices[0].resource == STOCK,
        "Stock scoreboard identity changed"
    );
    Ok(catalog.choices)
}
pub(crate) fn choice(id: &str) -> Result<Choice> {
    catalog()?
        .into_iter()
        .find(|c| c.id == id && c.selectable)
        .context("Unknown/unavailable scoreboard choice")
}
/// Explicit migration only. New user requests must go through choice(), which
/// rejects blocked IDs instead of silently selecting another scoreboard.
pub(crate) fn migrate_saved(id: &str) -> Result<Choice> {
    choice(match id {
        "mycareer-story" => "broadcast",
        "flashback-openrun" => "default",
        other => other,
    })
}
fn d(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .context("Truncated scoreboard scalar")?
            .try_into()?,
    ))
}
fn header(original: &[u8], key: u32, value: u32) -> Vec<u8> {
    let mut result = original[..12].to_vec();
    result[..4].copy_from_slice(&key.to_le_bytes());
    result[8..12].copy_from_slice(&value.to_le_bytes());
    result
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    pub address: u64,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub current: u32,
    pub wanted: u32,
    pub changes: Vec<Change>,
}
pub(crate) fn plan(fields: &BTreeMap<u64, Vec<u8>>, wanted: &str) -> Result<Plan> {
    plan_resource(fields, choice(wanted)?.resource)
}
fn plan_resource(fields: &BTreeMap<u64, Vec<u8>>, wanted: u32) -> Result<Plan> {
    ensure!(
        (3..=128).contains(&fields.len()),
        "Scoreboard property count exceeds bounds"
    );
    let mut keys = BTreeSet::new();
    let mut end = 0;
    for (address, bytes) in fields {
        ensure!(
            *address >= 0x10000
                && address % 4 == 0
                && *address >= end
                && address.checked_add(96).is_some_and(|n| n < 0x7fff00000000)
                && bytes.len() == 96
                && keys.insert(d(bytes, 0)?),
            "Invalid, overlapping or duplicate scoreboard properties"
        );
        end = address + 96;
    }
    let groups = [
        fields
            .iter()
            .filter(|(_, b)| matches!(d(b, 0), Ok(NAME | RESOURCE)))
            .collect::<Vec<_>>(),
        fields
            .iter()
            .filter(|(_, b)| d(b, 0).is_ok_and(|key| key == CLASS))
            .collect::<Vec<_>>(),
        fields
            .iter()
            .filter(|(_, b)| matches!(d(b, 0), Ok(MODE | HANDLER)))
            .collect::<Vec<_>>(),
    ];
    ensure!(
        groups.iter().all(|g| g.len() == 1),
        "Scoreboard descriptor scalar fields are not unique"
    );
    let selected = [groups[0][0], groups[1][0], groups[2][0]];
    ensure!(
        selected
            .iter()
            .all(|(_, b)| d(b, 4).is_ok_and(|n| n & 15 == 5)),
        "Unexpected scoreboard scalar type"
    );
    let current = d(selected[0].1, 8)?;
    let catalog = catalog()?;
    let from = catalog
        .iter()
        .find(|c| c.resource == current)
        .context("Unknown current scoreboard resource")?;
    let to = catalog
        .iter()
        .find(|c| c.resource == wanted)
        .context("Unknown desired scoreboard resource")?;
    ensure!(
        d(selected[0].1, 0)? != NAME || current == STOCK,
        "Name property is not the stock scoreboard"
    );
    let actual = [
        d(selected[1].1, 8)?,
        d(selected[2].1, 0)?,
        d(selected[2].1, 8)?,
    ];
    ensure!(
        actual == LEGACY || actual == from.profile,
        "Unknown scoreboard controller configuration"
    );
    let values = [
        header(
            selected[0].1,
            if wanted == STOCK { NAME } else { RESOURCE },
            wanted,
        ),
        header(selected[1].1, CLASS, to.profile[0]),
        header(selected[2].1, to.profile[1], to.profile[2]),
    ];
    let changes = selected
        .into_iter()
        .zip(values)
        .filter_map(|((address, before), after)| {
            (before[..12] != after).then(|| Change {
                address: *address,
                before: before.clone(),
                after,
            })
        })
        .collect();
    Ok(Plan {
        current,
        wanted,
        changes,
    })
}
/// Full 96-byte guards include flags, link fields, unrelated properties, and
/// already-owned prefixes. The executor must rerun this immediately before
/// each write; checking only the next 12 bytes permits torn/mixed descriptors.
pub(crate) fn expected_progress(
    original: &BTreeMap<u64, Vec<u8>>,
    plan: &Plan,
    applied: usize,
    current: &BTreeMap<u64, Vec<u8>>,
) -> Result<()> {
    ensure!(
        applied <= plan.changes.len(),
        "Scoreboard write progress exceeds its plan"
    );
    let mut expected = original.clone();
    for change in plan.changes.iter().take(applied) {
        let field = expected
            .get_mut(&change.address)
            .context("Scoreboard plan leaves its original property set")?;
        ensure!(
            *field == change.before && change.after.len() == 12,
            "Scoreboard plan source changed"
        );
        field[..12].copy_from_slice(&change.after);
    }
    ensure!(
        &expected == current,
        "Scoreboard descriptor changed outside its exact owned write prefix"
    );
    Ok(())
}
#[cfg(test)]
#[path = "scoreboard_profile_tests.rs"]
mod tests;
