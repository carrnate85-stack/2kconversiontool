//! Strict portable custom-catalog boundary. No game handles or native IDs escape.
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const LIMIT: usize = 32 * 1024 * 1024;
const CATEGORIES: [&str; 6] = [
    "current",
    "historic",
    "all_time",
    "other",
    "historic_unassigned",
    "unassigned",
];
const PLAYER_SCHEMA: &str = "48362c10f664d79bb3fc8c81d54a72288d80debc52d2efd5253e690547af2eaa";
const HISTORY_SCHEMA: &str = "df2519eb509843ef0df97ecb8666131875c75a6deeeba22db51c2e3f6790ee36";
pub(crate) struct Validated(Value);
impl Validated {
    pub(crate) fn value(&self) -> &Value {
        &self.0
    }
}
fn array(v: &Value, max: usize) -> Result<&Vec<Value>> {
    let a = v.as_array().context("Catalog array missing")?;
    ensure!(a.len() <= max, "Catalog array exceeds bounds");
    Ok(a)
}
fn integer(v: &Value, max: u64) -> Result<u64> {
    let n = v.as_f64().context("Catalog integer missing")?;
    ensure!(
        n.is_finite() && n.fract() == 0. && n >= 0. && n <= max as f64,
        "Catalog integer outside bounds"
    );
    Ok(n as u64)
}
fn text(v: &Value, max: usize) -> Result<&str> {
    let s = v.as_str().context("Catalog text missing")?;
    ensure!(
        s.encode_utf16().count() <= max && !s.chars().any(|c| c <= '\u{1f}'),
        "Invalid catalog text"
    );
    Ok(s)
}
fn strings(v: Option<&Value>, max: usize) -> Result<Value> {
    let Some(v) = v else { return Ok(json!([])) };
    Ok(json!(
        array(v, max)?
            .iter()
            .map(|v| text(v, 300).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?
    ))
}
fn ids(v: &Value, max: usize, exists: &BTreeSet<u64>) -> Result<Vec<u64>> {
    let a = array(v, max)?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(a.len());
    for v in a {
        let n = integer(v, 65534)?;
        ensure!(
            exists.contains(&n) && seen.insert(n),
            "Missing or duplicate catalog reference"
        );
        out.push(n)
    }
    Ok(out)
}
/// Bound lexical allocation before serde builds a tree. Serde still owns full JSON syntax.
fn budget(bytes: &[u8], check: &dyn Fn() -> Result<()>) -> Result<()> {
    ensure!(
        (2..=LIMIT).contains(&bytes.len()),
        "Catalog must be between 2 bytes and 32 MiB"
    );
    std::str::from_utf8(bytes).context("Catalog is not UTF-8")?;
    let (mut depth, mut nodes, mut quoted, mut escaped, mut string_len) =
        (0usize, 0usize, false, false, 0usize);
    for (i, &b) in bytes.iter().enumerate() {
        if i % 65536 == 0 {
            check()?;
        }
        if quoted {
            string_len += 1;
            ensure!(string_len <= 8192, "Catalog string token exceeds bound");
            if escaped {
                escaped = false
            } else if b == b'\\' {
                escaped = true
            } else if b == b'"' {
                quoted = false
            }
            continue;
        }
        match b {
            b'"' => {
                quoted = true;
                string_len = 0;
                nodes += 1
            }
            b'[' | b'{' => {
                depth += 1;
                nodes += 1;
                ensure!(depth <= 32, "Catalog nesting exceeds 32 levels")
            }
            b']' | b'}' => {
                ensure!(depth > 0, "Invalid catalog nesting");
                depth -= 1
            }
            b'0'..=b'9' | b'-' | b't' | b'f' | b'n' => {
                if i == 0
                    || matches!(
                        bytes[i - 1],
                        b'[' | b':' | b',' | b' ' | b'\r' | b'\n' | b'\t'
                    )
                {
                    nodes += 1
                }
            }
            _ => {}
        }
        ensure!(nodes <= 8_000_000, "Catalog JSON node budget exceeded");
    }
    ensure!(!quoted && depth == 0, "Incomplete catalog JSON");
    check()
}
pub(crate) fn parse(bytes: &[u8], check: &dyn Fn() -> Result<()>) -> Result<Validated> {
    budget(bytes, check)?;
    let v: Value = serde_json::from_slice(bytes).context("Catalog is not valid JSON")?;
    validate(&v, check)
}
pub(crate) fn validate(v: &Value, check: &dyn Fn() -> Result<()>) -> Result<Validated> {
    ensure!(
        v.is_object() && v["version"].as_f64() == Some(1.) && v["game"] == "NBA2K23",
        "Choose a version 1 NBA 2K23 catalog"
    );
    let unified = match v["format"].as_str() {
        Some("2k23-player-catalog") => true,
        Some("2k23-season-catalog") => false,
        _ => anyhow::bail!("Choose a .2k23catalog or .2k23stats catalog"),
    };
    let source = &v["source"];
    let hash = source["sha256"]
        .as_str()
        .context("Catalog source fingerprint missing")?;
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && source["playerSchema"] == PLAYER_SCHEMA
            && source["historySchema"] == HISTORY_SCHEMA,
        "Unsupported source schema/fingerprint"
    );
    let schema = crate::player_dna::schema();
    let fields = schema["fields"]
        .as_array()
        .context("Player schema missing")?;
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    if unified {
        ensure!(
            schema["compatible"]
                .as_array()
                .context("Schema compatibility missing")?
                .contains(&v["targetSchemaId"]),
            "Unsupported target player schema"
        );
        for key in array(&v["fieldKeys"], fields.len())? {
            let k = key.as_str().context("Invalid field key")?;
            let f = fields
                .iter()
                .find(|f| f["key"] == k)
                .context("Unknown source field")?;
            ensure!(
                seen.insert(k)
                    && !["PotentialRaw", "AgilityAbilityRaw"].contains(&k)
                    && (["attributes", "tendencies", "hotzones"]
                        .iter()
                        .any(|g| f["group"] == *g)
                        || (f["group"] == "personality" && f["maximum"].as_u64() == Some(1))
                        || ["BestHand", "BestDunkHand"].contains(&k)),
                "Unsupported source-to-target field mapping"
            );
            keys.push(k.to_owned());
        }
    }
    let mut teams = Vec::new();
    let mut team_ids = BTreeSet::new();
    for t in array(&v["teams"], 1024)? {
        let id = integer(&t["id"], 65534)?;
        ensure!(team_ids.insert(id), "Duplicate source team ID");
        let category = text(&t["category"], 32)?;
        ensure!(
            CATEGORIES.contains(&category),
            "Invalid source team category"
        );
        teams.push(json!({"id":id,"name":text(&t["name"],100)?,"city":text(&t["city"],100)?,"category":category,"classicEndYearRaw":integer(&t["classicEndYearRaw"],255)?}));
    }
    ensure!(!teams.is_empty(), "Catalog has no teams");
    let mut records = Vec::new();
    let mut record_ids = BTreeSet::new();
    let mut record_years = BTreeMap::new();
    for (index, r) in array(&v["records"], 65535)?.iter().enumerate() {
        if index % 256 == 0 {
            check()?
        }
        let id = integer(&r["id"], 65534)?;
        ensure!(record_ids.insert(id), "Duplicate source season ID");
        let primary = integer(&r["primaryTeamId"], 65534)?;
        ensure!(team_ids.contains(&primary), "Missing source season team");
        ensure!(
            r.get("secondaryTeamId").is_some(),
            "Secondary source team reference missing"
        );
        let secondary = if r["secondaryTeamId"].is_null() {
            Value::Null
        } else {
            let n = integer(&r["secondaryTeamId"], 65534)?;
            ensure!(team_ids.contains(&n), "Missing secondary season team");
            json!(n)
        };
        let year = integer(&r["year"], 2155)?;
        ensure!(year >= 1900, "Invalid source season year");
        // Reuse the already-tested exact portable totals/awards limits and cross-field rules.
        ensure!(r["awards"].is_object(), "Incomplete source awards");
        let clean = crate::player_dna::seasons(
            &json!([{"year":year,"team":"source","totals":r["totals"],"awards":r["awards"]}]),
        )?;
        let normalize = |obj: &Value| -> Result<Value> {
            let map = obj.as_object().context("Catalog season values missing")?;
            Ok(Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        Ok((
                            k.clone(),
                            json!(v.as_f64().context("Season integer missing")? as i64),
                        ))
                    })
                    .collect::<Result<Map<String, Value>>>()?,
            ))
        };
        records.push(json!({"id":id,"year":year,"primaryTeamId":primary,"secondaryTeamId":secondary,"totals":normalize(&clean[0]["totals"])?,"awards":normalize(&clean[0]["awards"])?}));
        record_years.insert(id, year);
    }
    let mut players = Vec::new();
    let mut player_ids = BTreeSet::new();
    let mut references = BTreeSet::new();
    for (index, p) in array(&v["players"], 8192)?.iter().enumerate() {
        if index % 128 == 0 {
            check()?
        }
        let id = integer(&p["id"], 8191)?;
        ensure!(player_ids.insert(id), "Duplicate source player ID");
        let first = text(&p["firstName"], 100)?;
        let last = text(&p["lastName"], 100)?;
        let history = ids(&p["history"], 25, &record_ids)?;
        let contexts = ids(&p["teams"], 1024, &team_ids)?;
        let groups = array(&p["categories"], 6)?;
        let mut group_names = BTreeSet::new();
        for g in groups {
            let s = g.as_str().context("Invalid player category")?;
            ensure!(
                CATEGORIES.contains(&s) && group_names.insert(s),
                "Invalid/duplicate player category"
            );
        }
        ensure!(!groups.is_empty(), "Player category missing");
        let mut years = BTreeSet::new();
        for r in &history {
            ensure!(
                years.insert(record_years[r]),
                "Duplicate player season year"
            );
            references.insert(*r);
        }
        let mut row = json!({"id":id,"firstName":first,"lastName":last,"history":history,"teams":contexts,"categories":groups});
        if unified {
            let values = array(&p["data"]["values"], keys.len())?;
            ensure!(
                values.len() == keys.len(),
                "Source values vector length mismatch"
            );
            let fs: Map<String, Value> = keys.iter().cloned().zip(values.iter().cloned()).collect();
            let name = super::portable_name(first, last);
            let dna = crate::player_dna::validate(
                &json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":schema["schemaId"],"name":name,"info":p["data"]["info"],"fields":fs}),
            )?;
            ensure!(
                dna["info"]
                    .as_object()
                    .context("Player info missing")?
                    .keys()
                    .all(|k| [
                        "firstName",
                        "lastName",
                        "potential",
                        "positionId",
                        "secondaryPositionId",
                        "jerseyNumberRaw",
                        "weightLbs",
                        "birthDate",
                        "yearsPro"
                    ]
                    .contains(&k.as_str())),
                "Game-specific source IDs/dimensions are not portable catalog fields"
            );
            row["data"] = json!({"info":dna["info"],"values":values,"warnings":strings(p["data"].get("warnings"),30)?});
        }
        players.push(row);
    }
    ensure!(
        !players.is_empty() && references.len() == records.len(),
        "Empty catalog or unreferenced source seasons"
    );
    check()?;
    let mut clean = json!({"format":v["format"],"version":1,"game":"NBA2K23","source":{"sha256":hash,"playerSchema":PLAYER_SCHEMA,"historySchema":HISTORY_SCHEMA},"teams":teams,"records":records,"players":players});
    if unified {
        clean["fieldKeys"] = json!(keys);
        clean["targetSchemaId"] = schema["schemaId"].clone();
        clean["unsupported"] = strings(v.get("unsupported"), 30)?;
    }
    ensure!(
        serde_json::to_vec(&clean)?.len() <= LIMIT,
        "Normalized catalog exceeds 32 MiB"
    );
    Ok(Validated(clean))
}
#[cfg(test)]
#[path = "player_catalog_import_tests.rs"]
mod tests;
