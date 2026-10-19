//! WPF uniform requests and pure, bit-preserving 52-control write plans.
//! No handles, browser addresses, process discovery or file operations.
use crate::team_changes;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
};
pub(crate) const STRIDE: usize = 0x170;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Layout {
    schema: String,
    source_sha256: String,
    reader_sha256: String,
    fields: Vec<Field>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Field {
    pub request: String,
    pub read_key: String,
    pub argument: String,
    pub maximum: u32,
    pub offset: usize,
    pub shift: u32,
    pub mask: u32,
    pub color: bool,
}
fn layout() -> &'static Layout {
    static L: OnceLock<Layout> = OnceLock::new();
    L.get_or_init(|| {
        let l: Layout = serde_json::from_str(include_str!("../assets/uniform-write-layout.json"))
            .expect("Original uniform write layout");
        assert_eq!(l.schema, "nba2k27-uniform-write-layout-v1");
        assert_eq!(l.source_sha256.len(), 64);
        assert_eq!(l.reader_sha256.len(), 64);
        assert_eq!(l.fields.len(), 52);
        let mut names = HashSet::new();
        let mut masks = BTreeMap::<usize, u32>::new();
        for f in &l.fields {
            assert!(
                names.insert(&f.request)
                    && !f.argument.is_empty()
                    && f.offset >= 0x10C
                    && f.offset + 4 <= STRIDE
                    && f.offset % 4 == 0
                    && f.shift < 32
                    && f.maximum <= f.mask
                    && ((f.mask as u64) << f.shift) <= u32::MAX as u64
            );
            let m = f.mask << f.shift;
            let old = masks.entry(f.offset).or_default();
            assert_eq!(*old & m, 0);
            *old |= m;
        }
        assert_eq!(l.fields.iter().filter(|f| f.color).count(), 21);
        assert_eq!(masks.len(), 25);
        assert_eq!(masks[&0x160], 0x3FFF00FF);
        assert_eq!(masks[&0x164], 0xC0000000);
        assert_eq!(masks[&0x168], 0x1FEBFFE0);
        assert_eq!(masks[&0x16C], 0xF7FFFFFF);
        l
    })
}
pub(crate) fn fields() -> &'static [Field] {
    &layout().fields
}
pub(crate) fn masks() -> BTreeMap<usize, u32> {
    let mut m = BTreeMap::new();
    for f in fields() {
        *m.entry(f.offset).or_default() |= f.mask << f.shift;
    }
    m
}
fn expected_key(key: &str) -> String {
    format!("expected{}{}", key[..1].to_ascii_uppercase(), &key[1..])
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Edit {
    pub record_index: usize,
    pub team_id: u16,
    pub expected_team_name: String,
    pub slot: usize,
    pub uniform_id: u16,
    pub expected_base_filename: String,
    #[serde(flatten)]
    pub values: BTreeMap<String, u32>,
}
pub(crate) fn validate(v: &Value) -> Result<Edit> {
    ensure!(v.is_object(), "Expected a uniform-edit object");
    let text = |key, max| -> Result<String> {
        let s = team_changes::text(v, key, max)?;
        ensure!(
            !s.is_empty() && !s.chars().any(|c| c <= '\u{1F}'),
            "Expected nonempty uniform identity text without control characters"
        );
        Ok(s)
    };
    let mut values = BTreeMap::new();
    for f in fields() {
        values.insert(
            f.request.clone(),
            team_changes::number(v.get(&f.request), f.maximum as u64)? as u32,
        );
        if f.color {
            let key = expected_key(&f.request);
            values.insert(
                key.clone(),
                team_changes::number(v.get(&key), u32::MAX as u64)? as u32,
            );
        }
    }
    for (key, max) in [
        ("expectedHome", 7),
        ("expectedAway", 7),
        ("expectedLogo", 3),
    ] {
        values.insert(key.into(), team_changes::number(v.get(key), max)? as u32);
    }
    for suffix in ["160", "164", "168", "16C"] {
        let key = format!("expectedFlags{suffix}");
        values.insert(
            key.clone(),
            team_changes::number(v.get(&key), u32::MAX as u64)? as u32,
        );
    }
    Ok(Edit {
        record_index: team_changes::number(v.get("recordIndex"), 1023)? as usize,
        team_id: team_changes::number(v.get("teamId"), 65535)? as u16,
        expected_team_name: text("expectedTeamName", 23)?,
        slot: team_changes::number(v.get("slot"), 31)? as usize,
        uniform_id: team_changes::number(v.get("uniformId"), 65535)? as u16,
        expected_base_filename: text("expectedBaseFilename", 63)?,
        values,
    })
}
pub(crate) fn ascii(row: &[u8], offset: usize, len: usize) -> Result<String> {
    Ok(row
        .get(offset..offset + len)
        .context("Incomplete uniform identity")?
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| if b.is_ascii() { *b as char } else { '?' })
        .collect::<String>()
        .trim()
        .into())
}
pub(crate) fn value(row: &[u8], f: &Field) -> Result<u32> {
    Ok((team_changes::uint(row, f.offset, 4)? as u32 >> f.shift) & f.mask)
}
fn cached_number(v: &Value) -> Result<u32> {
    if let Some(b) = v.as_bool() {
        return Ok(u32::from(b));
    }
    let n = v.as_f64().context("Original uniform value unavailable")?;
    ensure!(
        n.is_finite() && n.fract() == 0. && n >= 0. && n <= u32::MAX as f64,
        "Original uniform value is invalid"
    );
    Ok(n as u32)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    pub offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub mask: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub patches: Vec<Patch>,
    pub changed_controls: Vec<String>,
}
impl Plan {
    pub fn expected(&self, n: usize) -> Result<Vec<u8>> {
        ensure!(n <= self.patches.len(), "Invalid uniform transaction step");
        let mut raw = self.before.clone();
        for p in self.patches.iter().take(n) {
            raw[p.offset..p.offset + 4].copy_from_slice(&p.after);
        }
        Ok(raw)
    }
}
pub(crate) fn plan(
    edit: &Edit,
    cached_team: &Value,
    cached_uniform: &Value,
    team_raw: &[u8],
    uniform_raw: &[u8],
    uniform_address: u64,
) -> Result<Plan> {
    ensure!(
        team_raw.len() == team_changes::TEAM_STRIDE && uniform_raw.len() == STRIDE,
        "Incomplete uniform save buffers"
    );
    ensure!(
        cached_team["recordIndex"].as_u64() == Some(edit.record_index as u64)
            && cached_team["teamId"].as_u64() == Some(edit.team_id as u64)
            && cached_team["name"] == edit.expected_team_name,
        "The selected team differs from this tab's snapshot"
    );
    let identity = team_changes::fields(team_raw)?;
    ensure!(
        identity["teamId"] == edit.team_id
            && identity["name"] == edit.expected_team_name
            && team_changes::uint(team_raw, 0xDE0 + edit.slot * 8, 8)? == uniform_address,
        "The team identity or uniform slot changed; refresh teams"
    );
    ensure!(
        cached_uniform["slot"].as_u64() == Some(edit.slot as u64)
            && cached_uniform["uniformId"] == edit.uniform_id
            && cached_uniform["teamId"] == edit.team_id
            && cached_uniform["baseFilename"] == edit.expected_base_filename,
        "The selected uniform differs from this tab's snapshot"
    );
    ensure!(
        team_changes::uint(uniform_raw, 0x50, 2)? == edit.uniform_id as u64
            && team_changes::uint(uniform_raw, 0x52, 2)? == edit.team_id as u64
            && ascii(uniform_raw, 8, 64)? == edit.expected_base_filename,
        "The uniform identity changed; refresh teams"
    );
    for (offset, suffix) in [
        (0x160, "160"),
        (0x164, "164"),
        (0x168, "168"),
        (0x16C, "16C"),
    ] {
        let expected = edit.values[&format!("expectedFlags{suffix}")];
        ensure!(
            team_changes::uint(uniform_raw, offset, 4)? == expected as u64
                && cached_number(&cached_uniform[format!("flags{suffix}")])? == expected,
            "Packed uniform settings changed; refresh teams"
        );
    }
    for (key, read) in [
        ("expectedHome", "headbandColorHome"),
        ("expectedAway", "headbandColorAway"),
        ("expectedLogo", "headbandLogoTypeId"),
    ] {
        ensure!(
            cached_number(&cached_uniform[read])? == edit.values[key],
            "Headband baseline differs from this tab's snapshot"
        );
    }
    let mut p = Plan {
        before: uniform_raw.to_vec(),
        after: uniform_raw.to_vec(),
        patches: vec![],
        changed_controls: vec![],
    };
    for f in fields() {
        let old = value(uniform_raw, f)?;
        ensure!(
            cached_number(&cached_uniform[&f.read_key])? == old,
            "Uniform {} changed; refresh teams",
            f.read_key
        );
        if f.color {
            ensure!(
                edit.values[&expected_key(&f.request)] == old,
                "Uniform palette baseline differs"
            );
        }
        let new = edit.values[&f.request];
        let mask = f.mask << f.shift;
        let old_word = team_changes::uint(&p.after, f.offset, 4)? as u32;
        let word = (old_word & !mask) | ((new & f.mask) << f.shift);
        p.after[f.offset..f.offset + 4].copy_from_slice(&word.to_le_bytes());
        if new != old {
            p.changed_controls.push(f.request.clone());
        }
    }
    for (offset, mask) in masks() {
        let before = p.before[offset..offset + 4].to_vec();
        let after = p.after[offset..offset + 4].to_vec();
        if before != after {
            p.patches.push(Patch {
                offset,
                before,
                after,
                mask: mask.to_le_bytes().to_vec(),
            });
        }
    }
    ensure!(
        p.expected(p.patches.len())? == p.after,
        "Incomplete uniform plan"
    );
    Ok(p)
}
pub(crate) fn report(edit: &Edit, p: &Plan, pid: u32, address: u64) -> Value {
    json!({"schema":"nba2k27-team-uniform-edit-v4","ok":true,"pid":pid,"gameCodeHooks":0,"writes":52,"physicalWrites":p.patches.len(),"recordIndex":edit.record_index,"teamId":edit.team_id,"slot":edit.slot,"uniformId":edit.uniform_id,"recordAddress":format!("0x{address:X}"),"readBackVerified":true,"rollbackOnFailure":true,"noChanges":p.patches.is_empty()})
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn fixture() -> Value {
        serde_json::from_str(include_str!("../assets/uniform-write-regressions.json")).unwrap()
    }
    pub fn bytes(v: &Value) -> Vec<u8> {
        crate::team_changes::tests::hex(v)
    }
    pub fn case_data(c: &Value) -> (Edit, Vec<u8>, Vec<u8>, u64) {
        (
            validate(&c["input"]).unwrap(),
            bytes(&c["teamBefore"]),
            bytes(&c["before"]),
            c["uniformTable"].as_str().unwrap().parse().unwrap(),
        )
    }
    pub fn largest_plan() -> (Edit, Plan, Value) {
        let rows = fixture();
        let c = &rows[1];
        let (e, t, u, address) = case_data(c);
        let p = plan(
            &e,
            &c["cached"],
            &c["cached"]["uniforms"][0],
            &t,
            &u,
            address,
        )
        .unwrap();
        (e, p, c.clone())
    }
    #[test]
    fn request_normalizer_matches_original_wpf() {
        let rows: Value = serde_json::from_str(include_str!(
            "../assets/uniform-validation-regressions.json"
        ))
        .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 911);
        for (i, c) in rows.as_array().unwrap().iter().enumerate() {
            let result = validate(&c["input"]);
            assert_eq!(result.is_ok(), c["ok"] == true, "case {i}: {result:?}");
            if let Ok(e) = result {
                assert_eq!(serde_json::to_value(e).unwrap(), c["expected"], "case {i}");
            }
        }
    }
    #[test]
    fn all_52_controls_match_original_writer_bytes_and_preserve_ids() {
        let rows = fixture();
        assert_eq!(rows.as_array().unwrap().len(), 115);
        for (i, c) in rows.as_array().unwrap().iter().enumerate() {
            let (e, t, u, a) = case_data(c);
            let result = plan(&e, &c["cached"], &c["cached"]["uniforms"][0], &t, &u, a);
            assert_eq!(result.is_ok(), c["ok"] == true, "case {i}: {result:?}");
            if let Ok(p) = result {
                assert_eq!(p.after, bytes(&c["after"]), "case {i}");
                assert_eq!(p.before[..0x10C], p.after[..0x10C]);
                let mut all = bytes(&c["allUniformsBefore"]);
                all[..STRIDE].copy_from_slice(&p.after);
                assert_eq!(all, bytes(&c["allUniformsAfter"]));
                assert_eq!(bytes(&c["teamAfter"]), t);
            } else {
                assert_eq!(
                    bytes(&c["allUniformsBefore"]),
                    bytes(&c["allUniformsAfter"])
                );
            }
        }
    }
    #[test]
    fn changed_packed_choices_are_not_mistaken_for_stale_baselines() {
        let (e, p, c) = largest_plan();
        assert!(p.patches.len() <= 25 && p.patches.len() > 10);
        assert!(p.changed_controls.iter().any(|s| s == "typeId"));
        assert_eq!(report(&e, &p, 7, 0x10000)["writes"], 52);
        let mut cached = c["cached"]["uniforms"][0].clone();
        cached["primaryColor"] = Value::Null;
        assert!(
            plan(
                &e,
                &c["cached"],
                &cached,
                &bytes(&c["teamBefore"]),
                &p.before,
                c["uniformTable"].as_str().unwrap().parse().unwrap()
            )
            .is_err()
        );
    }
}
