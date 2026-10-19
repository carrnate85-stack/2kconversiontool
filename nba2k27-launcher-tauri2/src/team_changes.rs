//! Pure team request validation and copy-on-write arena/floor planning.
//! No process handles, browser-supplied addresses or file mutations.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) const TEAM_STRIDE: usize = 0x1550;
pub(crate) const ARENA_STRIDE: usize = 0x1278;
pub(crate) const ARENA_POINTER: usize = 0xEE0;
pub(crate) const TEAM_ID: usize = 0xF20;
pub(crate) const ARENA_ID: usize = 0x792;

fn js_string(v: &Value) -> Result<String> {
    Ok(match v {
        Value::Null => "null".into(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n
            .as_f64()
            .map(|n| {
                if n == 0. {
                    "0".into()
                } else if n.abs() >= 1e21 || n.abs() < 1e-6 {
                    let s = format!("{n:e}");
                    let (mantissa, exponent) = s.split_once('e').unwrap();
                    let exponent: i32 = exponent.parse().unwrap();
                    format!(
                        "{mantissa}e{}{exponent}",
                        if exponent >= 0 { "+" } else { "" }
                    )
                } else {
                    n.to_string()
                }
            })
            .unwrap_or_default(),
        Value::Array(a) => a
            .iter()
            .map(|v| {
                if v.is_null() {
                    Ok(String::new())
                } else {
                    js_string(v)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join(","),
        Value::Object(o) => {
            ensure!(
                !o.contains_key("toString"),
                "Unsupported object-to-text conversion"
            );
            "[object Object]".into()
        }
    })
}
pub(crate) fn number(v: Option<&Value>, max: u64) -> Result<u64> {
    let v = v.context("Missing numeric team detail")?;
    let n = match v {
        Value::Null => 0.,
        Value::Bool(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        Value::Number(n) => n.as_f64().context("Invalid team number")?,
        Value::String(_) | Value::Array(_) => {
            let text = js_string(v)?;
            let s = text.trim_matches(crate::player_changes::js_space);
            if s.is_empty() {
                0.
            } else if s
                .get(..2)
                .is_some_and(|p| ["0x", "0X", "0b", "0B", "0o", "0O"].contains(&p))
            {
                let radix = match s.as_bytes()[1] {
                    b'x' | b'X' => 16,
                    b'b' | b'B' => 2,
                    _ => 8,
                };
                u64::from_str_radix(&s[2..], radix)? as f64
            } else {
                s.parse::<f64>()?
            }
        }
        _ => anyhow::bail!("Invalid team number"),
    };
    ensure!(
        n.is_finite() && n.fract() == 0. && n >= 0. && n <= max as f64,
        "Team number is outside its supported range"
    );
    Ok(n as u64)
}
pub(crate) fn text(v: &Value, key: &str, max: usize) -> Result<String> {
    let s = v
        .get(key)
        .filter(|v| !v.is_null())
        .map(js_string)
        .transpose()?
        .unwrap_or_default();
    let s = s.trim_matches(crate::player_changes::js_space);
    ensure!(s.encode_utf16().count() <= max, "Team text is too long");
    Ok(s.into())
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Edit {
    pub record_index: usize,
    pub team_id: u16,
    pub expected_name: String,
    pub name: String,
    pub city_name: String,
    pub city_short: String,
    pub state_short: String,
    pub main_logo: String,
    pub roster_type_id: u8,
    pub historic_year_id: u8,
    pub arena_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor_file: Option<String>,
}
pub(crate) fn validate(v: &Value) -> Result<Edit> {
    ensure!(v.is_object(), "Expected a team-edit object");
    Ok(Edit {
        record_index: number(v.get("recordIndex"), 1023)? as usize,
        team_id: number(v.get("teamId"), 65535)? as u16,
        expected_name: text(v, "expectedName", 23)?,
        name: text(v, "name", 23)?,
        city_name: text(v, "cityName", 17)?,
        city_short: text(v, "cityShort", 6)?,
        state_short: text(v, "stateShort", 6)?,
        main_logo: text(v, "mainLogo", 35)?,
        roster_type_id: number(v.get("rosterTypeId"), 31)? as u8,
        historic_year_id: number(v.get("historicYearId"), 127)? as u8,
        arena_id: number(v.get("arenaId"), 65535)? as u16,
        floor_file: v
            .get("floorFile")
            .map(|_| text(v, "floorFile", 39))
            .transpose()?,
    })
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn uint(row: &[u8], at: usize, len: usize) -> Result<u64> {
    ensure!(len <= 8, "Invalid team integer width");
    let mut b = [0; 8];
    b[..len].copy_from_slice(
        row.get(at..at.checked_add(len).context("Team offset overflow")?)
            .context("Incomplete team record")?,
    );
    Ok(u64::from_le_bytes(b))
}
fn utf16(row: &[u8], at: usize, len: usize) -> Result<String> {
    let words: Vec<_> = row
        .get(at..at + len)
        .context("Incomplete team text")?
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0)
        .collect();
    Ok(String::from_utf16(&words)?.trim().to_owned())
}
fn ascii(row: &[u8], at: usize, len: usize) -> Result<String> {
    Ok(row
        .get(at..at + len)
        .context("Incomplete arena text")?
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| if b.is_ascii() { *b as char } else { '?' })
        .collect::<String>()
        .trim()
        .into())
}
pub(crate) fn fields(row: &[u8]) -> Result<Value> {
    ensure!(row.len() == TEAM_STRIDE, "Incomplete team record");
    Ok(
        json!({"name":utf16(row,0x2FA,48)?,"cityName":utf16(row,0x32C,36)?,"cityShort":utf16(row,0x350,14)?,"stateShort":utf16(row,0x35E,14)?,"mainLogo":utf16(row,0x1AE,72)?,"teamId":uint(row,TEAM_ID,2)?,"rosterTypeId":(row[0x123F]>>2)&31,"historicYearId":(uint(row,0x154A,2)?>>3)&127}),
    )
}
#[derive(Clone, Debug)]
pub(crate) struct Directory {
    pub team_table: u64,
    pub team_count: usize,
    pub arena_table: u64,
    pub arena_count: usize,
}
impl Directory {
    pub fn validate(&self) -> Result<()> {
        crate::roster_snapshot::checked_pointer(self.team_table)?;
        crate::roster_snapshot::checked_pointer(self.arena_table)?;
        ensure!(
            (1..=1024).contains(&self.team_count) && (1..=2048).contains(&self.arena_count),
            "Unsupported team or arena directory"
        );
        Ok(())
    }
    pub fn arena_address(&self, index: usize) -> Result<u64> {
        ensure!(index < self.arena_count, "Arena index is out of range");
        self.arena_table
            .checked_add((index * ARENA_STRIDE) as u64)
            .context("Arena address overflow")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Current {
    pub team: Vec<u8>,
    pub arenas: Vec<Vec<u8>>,
    pub references: Vec<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Area {
    Team,
    Clone,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    pub area: Area,
    pub offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub mask: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Raw {
    pub team: Vec<u8>,
    pub clone: Vec<u8>,
}
impl Raw {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.team.len() == TEAM_STRIDE
                && (self.clone.is_empty() || self.clone.len() == ARENA_STRIDE),
            "Incomplete owned team buffers"
        );
        Ok(())
    }
    pub fn bytes(&self, a: Area) -> &[u8] {
        match a {
            Area::Team => &self.team,
            Area::Clone => &self.clone,
        }
    }
    pub fn bytes_mut(&mut self, a: Area) -> &mut [u8] {
        match a {
            Area::Team => &mut self.team,
            Area::Clone => &mut self.clone,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub index: usize,
    pub team_id: u16,
    pub arena_id: u16,
    pub donor: usize,
    pub clone_index: Option<usize>,
    pub before: Raw,
    pub after: Raw,
    pub patches: Vec<Patch>,
}
impl Plan {
    pub fn expected(&self, n: usize) -> Result<Raw> {
        ensure!(n <= self.patches.len(), "Invalid team transaction step");
        let mut raw = self.before.clone();
        for p in self.patches.iter().take(n) {
            raw.bytes_mut(p.area)[p.offset..p.offset + p.after.len()].copy_from_slice(&p.after);
        }
        Ok(raw)
    }
}
fn fixed_text(s: &str, len: usize) -> Result<Vec<u8>> {
    // Normalization follows WPF; reject unroundtrippable text before planning.
    ensure!(
        !s.chars().any(char::is_control) && s.encode_utf16().count() < len / 2,
        "Team text contains unsupported control characters"
    );
    let mut b = vec![0; len];
    for (i, n) in s.encode_utf16().enumerate() {
        b[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
    }
    Ok(b)
}
fn empty_arena(row: &[u8]) -> Result<bool> {
    Ok(utf16(row, 0, 66)?.is_empty()
        && ascii(row, 0x7E, 38)?.is_empty()
        && ascii(row, 0xA4, 40)?.is_empty())
}
fn patch(plan: &mut Plan, area: Area, offset: usize, after: &[u8], mask: &[u8]) {
    let before = plan.before.bytes(area)[offset..offset + after.len()].to_vec();
    let after: Vec<_> = before
        .iter()
        .zip(after)
        .zip(mask)
        .map(|((&b, &a), &m)| (b & !m) | (a & m))
        .collect();
    if before != after {
        plan.after.bytes_mut(area)[offset..offset + after.len()].copy_from_slice(&after);
        plan.patches.push(Patch {
            area,
            offset,
            before,
            after,
            mask: mask.to_vec(),
        });
    }
}
pub(crate) fn plan(
    edit: &Edit,
    cached: &Value,
    directory: &Directory,
    current: &Current,
    expected_arena: u64,
) -> Result<Plan> {
    directory.validate()?;
    ensure!(
        edit.record_index < directory.team_count
            && current.references.len() == directory.team_count
            && current.arenas.len() == directory.arena_count
            && current.arenas.iter().all(|r| r.len() == ARENA_STRIDE),
        "Incomplete team/arena snapshot"
    );
    let original = fields(&current.team)?;
    for key in [
        "name",
        "cityName",
        "cityShort",
        "stateShort",
        "mainLogo",
        "teamId",
        "rosterTypeId",
        "historicYearId",
    ] {
        ensure!(
            original[key] == cached[key],
            "The team's {key} changed. Refresh teams before saving."
        );
    }
    ensure!(
        original["name"] == edit.expected_name
            && original["teamId"] == edit.team_id
            && uint(&current.team, ARENA_POINTER, 8)? == expected_arena
            && current.references[edit.record_index] == expected_arena,
        "The selected team identity or arena changed; refresh teams before saving"
    );
    let matches: Vec<_> = current
        .arenas
        .iter()
        .enumerate()
        .filter(|(_, r)| uint(r, ARENA_ID, 2).ok() == Some(edit.arena_id as u64))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        matches.len() == 1,
        "Arena ID did not resolve to one exact live arena"
    );
    let donor = matches[0];
    let mut target = donor;
    if let Some(floor) = &edit.floor_file {
        ensure!(
            !floor.is_empty()
                && floor.len() <= 39
                && floor
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b' ' || b == b'-'),
            "Choose a supported floor from the live catalog"
        );
        ensure!(
            !empty_arena(&current.arenas[donor])?
                && !ascii(&current.arenas[donor], 0x7E, 38)?.is_empty(),
            "Choose a populated arena and floor"
        );
        if ascii(&current.arenas[donor], 0xA4, 40)? != *floor {
            ensure!(
                current
                    .arenas
                    .iter()
                    .any(|r| empty_arena(r).ok() == Some(false)
                        && ascii(r, 0xA4, 40).ok().as_ref() == Some(floor)),
                "The floor is not in the live arena catalog"
            );
            target=(0..directory.arena_count).find(|&i|{
                let row=&current.arenas[i];let id=uint(row,ARENA_ID,2).unwrap_or(0);
                empty_arena(row).ok()==Some(true)&&(996..=1045).contains(&id)&&!current.references.contains(&directory.arena_address(i).unwrap_or(0))&&current.arenas.iter().filter(|r|uint(r,ARENA_ID,2).ok()==Some(id)).count()==1
            }).context("No unreferenced empty reserved arena slot is available; no team or floor was changed")?;
        }
    }
    let clone_index = (target != donor).then_some(target);
    let before = Raw {
        team: current.team.clone(),
        clone: clone_index
            .map(|i| current.arenas[i].clone())
            .unwrap_or_default(),
    };
    let mut plan = Plan {
        index: edit.record_index,
        team_id: edit.team_id,
        arena_id: uint(&current.arenas[target], ARENA_ID, 2)? as u16,
        donor,
        clone_index,
        after: before.clone(),
        before,
        patches: vec![],
    };
    if clone_index.is_some() {
        let mut after = current.arenas[donor].clone();
        let floor = edit.floor_file.as_ref().context("Missing clone floor")?;
        after[0xA4..0xCC].fill(0);
        after[0xA4..0xA4 + floor.len()].copy_from_slice(floor.as_bytes());
        after[ARENA_ID..ARENA_ID + 2].copy_from_slice(&plan.arena_id.to_le_bytes());
        for at in (0..ARENA_STRIDE).step_by(128) {
            let end = (at + 128).min(ARENA_STRIDE);
            let mut mask = vec![255; end - at];
            for (i, m) in mask.iter_mut().enumerate() {
                if (ARENA_ID..ARENA_ID + 2).contains(&(at + i)) {
                    *m = 0;
                }
            }
            patch(&mut plan, Area::Clone, at, &after[at..end], &mask);
        }
    }
    for (at, len, s) in [
        (0x2FA, 48, &edit.name),
        (0x32C, 36, &edit.city_name),
        (0x350, 14, &edit.city_short),
        (0x35E, 14, &edit.state_short),
        (0x1AE, 72, &edit.main_logo),
    ] {
        let value = fixed_text(s, len)?;
        patch(&mut plan, Area::Team, at, &value, &vec![255; len]);
    }
    patch(
        &mut plan,
        Area::Team,
        0x123F,
        &[edit.roster_type_id << 2],
        &[0x7C],
    );
    patch(
        &mut plan,
        Area::Team,
        0x154A,
        &((edit.historic_year_id as u16) << 3).to_le_bytes(),
        &0x3F8u16.to_le_bytes(),
    );
    patch(
        &mut plan,
        Area::Team,
        ARENA_POINTER,
        &directory.arena_address(target)?.to_le_bytes(),
        &[255; 8],
    );
    ensure!(
        plan.expected(plan.patches.len())? == plan.after,
        "Incomplete team write plan"
    );
    Ok(plan)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn fixture() -> Value {
        serde_json::from_str(include_str!("../assets/team-write-regressions.json")).unwrap()
    }
    pub fn hex(v: &Value) -> Vec<u8> {
        let s = v.as_str().unwrap();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
    pub fn data(c: &Value) -> (Directory, Current, Value) {
        let directory = Directory {
            team_table: c["teamTable"].as_str().unwrap().parse().unwrap(),
            team_count: c["teamCount"].as_u64().unwrap() as usize,
            arena_table: c["arenaTable"].as_str().unwrap().parse().unwrap(),
            arena_count: c["arenaCount"].as_u64().unwrap() as usize,
        };
        let raw = Current {
            team: hex(&c["before"]),
            arenas: hex(&c["arenasBefore"])
                .chunks_exact(ARENA_STRIDE)
                .map(|r| r.to_vec())
                .collect(),
            references: c["references"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().parse().unwrap())
                .collect(),
        };
        let cached = fields(&raw.team).unwrap();
        (directory, raw, cached)
    }
    pub fn clone_plan() -> (Plan, Directory, Current) {
        let rows = fixture();
        let c = &rows[2];
        let (d, r, v) = data(c);
        let p = plan(&validate(&c["input"]).unwrap(), &v, &d, &r, r.references[0]).unwrap();
        (p, d, r)
    }
    #[test]
    fn normalizer_matches_original_wpf() {
        let cases: Value = serde_json::from_str(include_str!(
            "../assets/team-write-validation-regressions.json"
        ))
        .unwrap();
        assert_eq!(cases.as_array().unwrap().len(), 263);
        for (i, c) in cases.as_array().unwrap().iter().enumerate() {
            let result = validate(&c["input"]);
            assert_eq!(result.is_ok(), c["ok"] == true, "case {i}: {result:?}");
            if let Ok(e) = result {
                assert_eq!(serde_json::to_value(e).unwrap(), c["expected"], "case {i}");
            }
        }
    }
    #[test]
    fn planned_bytes_match_original_csharp_writer() {
        let cases = fixture();
        for (i, c) in cases.as_array().unwrap().iter().enumerate() {
            let (d, r, cached) = data(c);
            let result = plan(
                &validate(&c["input"]).unwrap(),
                &cached,
                &d,
                &r,
                r.references[0],
            );
            assert_eq!(result.is_ok(), c["ok"] == true, "case {i}: {result:?}");
            if let Ok(p) = result {
                assert_eq!(p.after.team, hex(&c["after"]), "case {i}");
                let mut arenas = r.arenas.clone();
                if let Some(i) = p.clone_index {
                    arenas[i] = p.after.clone.clone();
                }
                assert_eq!(arenas.concat(), hex(&c["arenasAfter"]), "case {i}");
                assert_eq!(
                    p.after.team[TEAM_ID..TEAM_ID + 2],
                    p.before.team[TEAM_ID..TEAM_ID + 2]
                );
            }
        }
    }
    #[test]
    fn stale_values_bad_shapes_controls_and_changed_references_are_refused() {
        let cases = fixture();
        let c = &cases[1];
        let (d, r, cached) = data(c);
        let mut e = validate(&c["input"]).unwrap();
        let expected = r.references[0];
        for key in [
            "name",
            "cityName",
            "cityShort",
            "stateShort",
            "mainLogo",
            "teamId",
            "rosterTypeId",
            "historicYearId",
        ] {
            let mut stale = cached.clone();
            stale[key] = Value::Null;
            assert!(plan(&e, &stale, &d, &r, expected).is_err());
        }
        assert!(plan(&e, &cached, &d, &r, expected + 8).is_err());
        e.name = "QA\0hidden".into();
        assert!(plan(&e, &cached, &d, &r, expected).is_err());
        e.name = "QA".into();
        let mut bad = r.clone();
        bad.references.pop();
        assert!(plan(&e, &cached, &d, &bad, expected).is_err());
    }
}
