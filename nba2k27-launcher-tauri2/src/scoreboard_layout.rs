//! Fixed production scoreboard text adapters, rebuilt entirely in memory from
//! exact stock IFF members. The shared English archive is verification-only.
//! No aliasing arbitrary filenames, unknown layouts, or game-file mutation.
#![allow(dead_code)]
use crate::{scoreboard_profile, selector_tables};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const PATHS: [&str; 5] = [
    "gooey/mycourtscrimmagescorebug.iff",
    "gooey/myparkscorebugsimple.iff",
    "gooey/proamspectatorscorebug.iff",
    "gooey/shutitdownscorebug.iff",
    "gooey/thewscrimmagescorebug.iff",
];
const IDS: [&str; 5] = [
    "mycourt-scrimmage",
    "mypark-simple",
    "proam-spectator",
    "shut-it-down",
    "w-scrimmage",
];
const MAX: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Edit {
    offset: usize,
    before: String,
    after: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Member {
    member: String,
    source_sha256: String,
    edits: Vec<Edit>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Layout {
    id: String,
    asset: String,
    plans: Vec<Member>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    key: String,
    text: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    schema: String,
    original_source_sha256: String,
    layouts: Vec<Layout>,
    targets: Vec<Target>,
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn hex(s: &str, n: usize) -> Result<Vec<u8>> {
    ensure!(
        s.len() == n * 2
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Invalid scoreboard adapter hex"
    );
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| Ok(u8::from_str_radix(std::str::from_utf8(c)?, 16)?))
        .collect()
}
fn catalog() -> Result<Catalog> {
    let c: Catalog =
        serde_json::from_str(include_str!("../assets/scoreboard-layout-adapters.json"))?;
    ensure!(
        c.schema == "nba2k27-scoreboard-layout-adapters-v1"
            && c.layouts.len() == 5
            && c.targets.len() == 6
            && c.original_source_sha256.len() == 64
            && c.original_source_sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit()),
        "Scoreboard adapter catalog changed"
    );
    let mut keys = BTreeSet::new();
    for t in &c.targets {
        let k = u64::from_str_radix(&t.key, 16)?;
        ensure!(
            keys.insert(k)
                && t.text.is_ascii()
                && t.text.starts_with("{0:")
                && t.text.ends_with('}'),
            "Invalid scoreboard target expression"
        );
    }
    for ((l, id), path) in c.layouts.iter().zip(IDS).zip(PATHS) {
        ensure!(
            l.id == id
                && l.asset == path
                && l.plans.len() == 1
                && scoreboard_profile::choice(id)?.requires_layout_adapter
                && path == format!("gooey/{}", scoreboard_profile::choice(id)?.file),
            "Scoreboard layout allowlist differs"
        );
        let p = &l.plans[0];
        ensure!(
            p.member.ends_with(".VCUIELEMENT") && (1..=8).contains(&p.edits.len()),
            "Invalid scoreboard adapter member"
        );
        hex(&p.source_sha256, 32)?;
        for e in &p.edits {
            hex(&e.before, 8)?;
            let b = hex(&e.after, 8)?;
            ensure!(
                keys.contains(&u64::from_le_bytes(b.try_into().unwrap())),
                "Scoreboard adapter targets an unknown expression"
            );
        }
    }
    Ok(c)
}
fn resolve(text: &[u8], key: u64) -> Result<String> {
    ensure!(
        (64..=MAX).contains(&text.len()),
        "Scoreboard localization size differs"
    );
    let declared = u64::from_le_bytes(text[24..32].try_into()?);
    let pool = usize::try_from(
        declared
            .checked_add(63)
            .context("Scoreboard pool overflow")?
            & !63,
    )?;
    ensure!(
        (64..text.len()).contains(&pool),
        "Scoreboard localization pool differs"
    );
    let needle = key.to_le_bytes();
    let mut found = None;
    for at in (64..pool).step_by(8).filter(|a| a % 64 < 40) {
        if text.get(at..at + 8) == Some(needle.as_slice()) {
            ensure!(
                found.replace(at).is_none(),
                "Scoreboard expression key is ambiguous"
            );
        }
    }
    let a = found.context("Stock scoreboard expression is missing")?;
    let field = a / 64 * 64 + 40 + a % 64 / 8 * 4;
    let off = u32::from_le_bytes(
        text.get(field..field + 4)
            .context("Truncated scoreboard string offset")?
            .try_into()?,
    ) as usize;
    let mut pos = pool
        .checked_add(
            off.checked_mul(2)
                .context("Scoreboard string offset overflow")?,
        )
        .context("Scoreboard string offset overflow")?;
    let mut words = Vec::new();
    loop {
        let p = text
            .get(pos..pos + 2)
            .context("Unterminated scoreboard expression")?;
        let w = u16::from_le_bytes(p.try_into()?);
        if w == 0 {
            break;
        }
        words.push(w);
        ensure!(words.len() <= 4096, "Scoreboard expression exceeds bounds");
        pos += 2;
    }
    Ok(String::from_utf16(&words)?)
}
fn patch_member(source: &[u8], plan: &Member) -> Result<Vec<u8>> {
    ensure!(
        source.len() <= MAX && hash(source) == plan.source_sha256 && !plan.edits.is_empty(),
        "Scoreboard stock UI source SHA changed"
    );
    let mut output = source.to_vec();
    let mut touched = BTreeSet::new();
    for edit in &plan.edits {
        let end = edit
            .offset
            .checked_add(8)
            .context("Scoreboard edit overflow")?;
        ensure!(
            source.get(edit.offset..end) == Some(hex(&edit.before, 8)?.as_slice()),
            "Scoreboard stock text binding changed"
        );
        for n in edit.offset..end {
            ensure!(touched.insert(n), "Overlapping scoreboard UI edits");
        }
        output[edit.offset..end].copy_from_slice(&hex(&edit.after, 8)?);
    }
    ensure!(
        source.len() == output.len()
            && source
                .iter()
                .zip(&output)
                .enumerate()
                .all(|(i, (a, b))| a == b || touched.contains(&i)),
        "Unrelated scoreboard UI bytes changed"
    );
    Ok(output)
}
fn patch(source: &[u8], layout: &Layout) -> Result<Vec<u8>> {
    let original = selector_tables::archive(source)?;
    let mut entries = original.clone();
    let mut touched = BTreeSet::new();
    for p in &layout.plans {
        ensure!(touched.insert(&p.member), "Duplicate scoreboard member");
        let source = original
            .get(&p.member)
            .context("Stock scoreboard UI member missing")?;
        entries.insert(p.member.clone(), patch_member(source, p)?);
    }
    let output = selector_tables::pack(&entries)?;
    ensure!(
        selector_tables::archive(&output)? == entries && entries.len() == original.len(),
        "Scoreboard archive readback differs"
    );
    Ok(output)
}
#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    files: BTreeMap<String, Vec<u8>>,
    source_hashes: BTreeMap<String, String>,
}
impl Prepared {
    pub(crate) fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
    pub(crate) fn source_hashes(&self) -> &BTreeMap<String, String> {
        &self.source_hashes
    }
    pub(crate) fn ready_for(&self, id: &str) -> Result<bool> {
        let c = scoreboard_profile::choice(id)?;
        Ok(!c.requires_layout_adapter
            || self
                .files
                .get(&format!("gooey/{}", c.file))
                .is_some_and(|b| !b.is_empty()))
    }
    pub(crate) fn overrides(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }
}
/// Source closure is invoked only with these six fixed installed logical asset
/// paths. Returned bytes are never trusted without archive/English/source-SHA
/// verification. Publish these5 immutable virtual files before Steam starts;
/// freeze their exact OVERRIDE entries for this provider session.
pub(crate) fn prepare(
    mut source: impl FnMut(&str) -> Result<Vec<u8>>,
    check: impl Fn() -> Result<()>,
) -> Result<Prepared> {
    let c = catalog()?;
    check()?;
    let english = source("loc/english.iff")?;
    let archive = selector_tables::archive(&english)?;
    let text = selector_tables::required(&archive, "TEXT.VCLOCALIZEDATA")?;
    for t in &c.targets {
        ensure!(
            resolve(text, u64::from_str_radix(&t.key, 16)?)? == t.text,
            "Stock scoreboard expression changed"
        );
    }
    let mut files = BTreeMap::new();
    let mut hashes = BTreeMap::from([("loc/english.iff".into(), hash(&english))]);
    for layout in c.layouts {
        check()?;
        let raw = source(&layout.asset)?;
        hashes.insert(layout.asset.clone(), hash(&raw));
        let built = patch(&raw, &layout)?;
        ensure!(
            files.insert(layout.asset, built).is_none(),
            "Duplicate scoreboard output"
        );
    }
    check()?;
    ensure!(
        files.keys().all(|p| PATHS.contains(&p.as_str()))
            && files.len() == 5
            && !files.contains_key("loc/english.iff"),
        "Scoreboard output allowlist differs"
    );
    Ok(Prepared {
        files,
        source_hashes: hashes,
    })
}
#[cfg(test)]
#[path = "scoreboard_layout_tests.rs"]
mod tests;
