//! Native validation and offline Player DNA editing. No live addresses are accepted or exported.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::OnceLock};
pub fn schema() -> &'static Value {
    static S: OnceLock<Value> = OnceLock::new();
    S.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/player-schema.json"))
            .expect("Build-time verified schema")
    })
}
fn object(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    v.as_object().context("Expected an object")
}
fn text(v: &Value, max: usize, nonempty: bool) -> Result<&str> {
    let s = v.as_str().context("Expected text")?;
    ensure!(
        s.encode_utf16().count() <= max
            && !s.chars().any(|c| c <= '\u{1f}')
            && (!nonempty || !s.trim_matches(crate::player_changes::js_space).is_empty()),
        "Text is invalid or too long"
    );
    Ok(s)
}
fn number(v: &Value, min: f64, max: f64, integer: bool) -> Result<()> {
    let n = v.as_f64().context("Expected a number")?;
    ensure!(
        n.is_finite() && n >= min && n <= max && (!integer || n.fract() == 0.),
        "Value is out of range"
    );
    Ok(())
}
fn date(v: &Value) -> Result<()> {
    let s = v.as_str().context("Birthday must be YYYY-MM-DD")?;
    ensure!(
        s.len() == 10
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && s.bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit()),
        "Birthday must be YYYY-MM-DD"
    );
    let y: u32 = s[..4].parse()?;
    let m: usize = s[5..7].parse()?;
    let d: u32 = s[8..].parse()?;
    let days = [
        31,
        if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ensure!(
        (1..=4095).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days[m - 1],
        "Birthday is not a real calendar date"
    );
    Ok(())
}
pub fn info_group(key: &str) -> &str {
    match key {
        "cyberfaceId" | "weightLbs" | "heightCm" | "wingspanCm" => "appearance",
        "fullNameId" => "audio",
        "potential" => "attributes",
        _ => "info",
    }
}
/// Copy/list only captured players; this helper has no engine, address, file or
/// destination-stage capability. Both roster editions share the same contract.
pub(crate) fn snapshot_request(players: &[Value], request: &Value) -> Result<Value> {
    let key = |p: &Value| {
        serde_json::to_string(&json!([p["firstName"], p["lastName"], p["cyberfaceId"]]))
    };
    match request["action"].as_str() {
        Some("players") => {
            let rows = players
                .iter()
                .filter(|p| p["source"] == "live-roster")
                .map(|p| {
                    let name = [
                        p["firstName"].as_str().unwrap_or(""),
                        p["lastName"].as_str().unwrap_or(""),
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                    Ok(json!({"recordIndex":p["recordIndex"],"name":name,
                    "teamName":p["teamName"].as_str().unwrap_or(""),"key":key(p)?}))
                })
                .collect::<Result<Vec<Value>>>()?;
            Ok(json!(rows))
        }
        Some("copy") => {
            let index = request["recordIndex"]
                .as_f64()
                .filter(|n| n.is_finite() && n.fract() == 0. && (0. ..=8191.).contains(n))
                .context("Invalid source player index")? as u64;
            let player = players
                .iter()
                .find(|p| p["source"] == "live-roster" && p["recordIndex"].as_u64() == Some(index))
                .context("Refresh live players before copying this player")?;
            ensure!(
                request["key"].as_str() == Some(&key(player)?),
                "Source player changed. Refresh live players before copying."
            );
            ensure!(
                !request["sections"]
                    .as_array()
                    .is_some_and(|s| s.contains(&json!("seasons"))),
                "Live season-history copying is not ported yet. Uncheck Season history to copy player fields."
            );
            subset(&from_player(player)?, &request["sections"])
        }
        _ => anyhow::bail!(
            "This Player DNA operation is not ported yet. No game operation was attempted."
        ),
    }
}

/// The original editor's loaded-player copy contract, reconstructed from
/// verified cached values only. Never exports process/table IDs or paths.
pub fn from_player(player: &Value) -> Result<Value> {
    let ratings = player["ratingsRaw"]
        .as_array()
        .context("Refresh the live roster before copying a player")?;
    let fields = schema()["fields"].as_array().unwrap();
    ensure!(
        player["source"] == "live-roster" && ratings.len() == fields.len(),
        "Refresh the live roster before copying a player"
    );
    let mut info = serde_json::Map::new();
    for (key, min, max, integer) in [
        ("cyberfaceId", 0., 65535., true),
        ("positionId", 0., 5., true),
        ("secondaryPositionId", 0., 5., true),
        ("weightLbs", 70., 500., false),
        ("heightCm", 120., 260., false),
        ("wingspanCm", 120., 320., false),
        ("potential", 25., 110., true),
    ] {
        if number(&player[key], min, max, integer).is_ok() {
            info.insert(key.into(), player[key].clone());
        }
    }
    for key in ["firstName", "lastName"] {
        if player[key]
            .as_str()
            .is_some_and(|s| s.encode_utf16().count() <= 19)
        {
            info.insert(key.into(), player[key].clone());
        }
    }
    if player["collegeName"].as_str().is_some_and(|s| {
        s.encode_utf16().count() <= 47
            && !s.trim_matches(crate::player_changes::js_space).is_empty()
    }) {
        info.insert("fromName".into(), player["collegeName"].clone());
    }
    let audio = &player["audio"]["fullNameId"];
    if number(audio, -1., 32767., true).is_ok() {
        info.insert("fullNameId".into(), audio.clone());
    }
    let jersey = if player["jerseyNumberRaw"]
        .as_f64()
        .is_some_and(|n| n.fract() == 0.)
    {
        player["jerseyNumberRaw"].clone()
    } else if player["jerseyNumber"] == "00" {
        json!(100)
    } else if let Some(s) = player["jerseyNumber"]
        .as_str()
        .filter(|s| (1..=2).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()))
    {
        json!(s.parse::<u64>()?)
    } else if player["jerseyNumber"].is_number() {
        player["jerseyNumber"].clone()
    } else {
        Value::Null
    };
    if number(&jersey, 0., 100., true).is_ok() {
        info.insert("jerseyNumberRaw".into(), jersey);
    }
    if player["bioLayoutVerified"] == true {
        if number(&player["yearsPro"], 0., 31., true).is_ok() {
            info.insert("yearsPro".into(), player["yearsPro"].clone());
        }
        if let Some(s) = player["birthDate"]
            .as_str()
            .filter(|_| date(&player["birthDate"]).is_ok())
        {
            if player["birthYear"].as_f64() == Some(s[..4].parse::<f64>()?)
                && player["birthMonth"].as_f64() == Some(s[5..7].parse::<f64>()?)
                && player["birthDay"].as_f64() == Some(s[8..].parse::<f64>()?)
            {
                info.insert("birthDate".into(), json!(s));
            }
        }
    }
    let mapped: serde_json::Map<String, Value> = fields
        .iter()
        .zip(ratings)
        .filter(|(f, v)| number(v, 0., f["maximum"].as_f64().unwrap(), true).is_ok())
        .map(|(f, v)| (f["key"].as_str().unwrap().to_owned(), v.clone()))
        .collect();
    let name = [
        player["firstName"].as_str().unwrap_or(""),
        player["lastName"].as_str().unwrap_or(""),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    validate(
        &json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":schema()["schemaId"],"name":name,"info":info,"fields":mapped}),
    )
}
pub fn validate(v: &Value) -> Result<Value> {
    ensure!(
        serde_json::to_vec(v)?.len() <= 256 * 1024,
        "Player DNA exceeds 256 KB"
    );
    object(v)?;
    let raw_version = v["version"].as_f64().context("DNA version required")?;
    ensure!(
        raw_version == 1. || raw_version == 2.,
        "Unsupported DNA version"
    );
    let version = raw_version as u64;
    ensure!(
        v["format"] == "2kplayer" && v["game"] == "NBA2K27" && (version == 1 || version == 2),
        "Unsupported .2kplayer file"
    );
    ensure!(
        (version == 2) == v.get("seasonHistory").is_some(),
        "Version 2 requires season history; version 1 cannot include it"
    );
    ensure!(
        schema()["compatible"]
            .as_array()
            .unwrap()
            .contains(&v["schemaId"]),
        "Player DNA uses an incompatible field schema"
    );
    text(&v["name"], 80, false)?;
    let mut info = serde_json::Map::new();
    let mut fields = serde_json::Map::new();
    for (key, value) in object(&v["info"])? {
        let check = match key.as_str() {
            "firstName" | "lastName" => text(value, 19, false).map(|_| ()),
            "fromName" => text(value, 47, true).map(|_| ()),
            "birthDate" => date(value),
            "cyberfaceId" => number(value, 0., 65535., true),
            "positionId" | "secondaryPositionId" => number(value, 0., 5., true),
            "jerseyNumberRaw" => number(value, 0., 100., true),
            "weightLbs" => number(value, 70., 500., false),
            "heightCm" => number(value, 120., 260., false),
            "wingspanCm" => number(value, 120., 320., false),
            "potential" => number(value, 25., 110., true),
            "fullNameId" => number(value, -1., 32767., true),
            "yearsPro" => number(value, 0., 31., true),
            _ => anyhow::bail!("Unsupported player information: {key}"),
        };
        check.with_context(|| format!("Invalid player information: {key}"))?;
        info.insert(key.clone(), value.clone());
    }
    for (key, value) in object(&v["fields"])? {
        let f = schema()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == *key)
            .with_context(|| format!("Unsupported player field: {key}"))?;
        number(value, 0., f["maximum"].as_f64().unwrap(), true)
            .with_context(|| format!("Invalid player field: {key}"))?;
        fields.insert(key.clone(), value.clone());
    }
    let mut clean = json!({"format":"2kplayer","version":version,"game":"NBA2K27","schemaId":schema()["schemaId"],"name":v["name"],"info":info,"fields":fields});
    if version == 2 {
        clean["seasonHistory"] = seasons(&v["seasonHistory"])?;
    }
    Ok(clean)
}
pub fn seasons(v: &Value) -> Result<Value> {
    let rows = v.as_array().context("Season history must be an array")?;
    ensure!(rows.len() <= 25, "At most 25 seasons may be imported");
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let limits = object(&schema()["seasonLimits"])?;
    for row in rows {
        object(row)?;
        number(&row["year"], 1900., 2155., true)?;
        text(&row["team"], 100, true)?;
        ensure!(
            seen.insert(serde_json::to_string(&json!([
                row["year"].as_f64().unwrap() as u64,
                row["team"],
                row.get("secondaryTeam")
            ]))?),
            "Duplicate season/team entries"
        );
        let totals = object(&row["totals"])?;
        ensure!(
            totals.keys().all(|key| limits.contains_key(key)),
            "Unsupported season total"
        );
        let mut clean_totals = serde_json::Map::new();
        for (key, max) in limits {
            let n = row["totals"][key].clone();
            number(
                &n,
                if key == "plusMinus" { -16384. } else { 0. },
                max.as_f64().unwrap(),
                true,
            )
            .with_context(|| format!("Invalid season total: {key}"))?;
            clean_totals.insert(key.clone(), n);
        }
        let n = |k: &str| row["totals"][k].as_f64().unwrap() as i64;
        ensure!(
            n("gs") <= n("gp") && n("doubleDoubles") <= n("gp") && n("tripleDoubles") <= n("gp"),
            "Season counts exceed games played"
        );
        ensure!(
            n("fgm") <= n("fga")
                && n("threeMade") <= n("threeAttempted")
                && n("ftm") <= n("fta")
                && n("threeMade") <= n("fgm")
                && n("threeAttempted") <= n("fga"),
            "Invalid season shooting totals"
        );
        let mut clean = json!({"year":row["year"],"team":row["team"],"totals":clean_totals});
        if let Some(team) = row.get("secondaryTeam") {
            if !team.is_null() {
                text(team, 100, true)?;
            }
            clean["secondaryTeam"] = team.clone();
        }
        if let Some(a) = row.get("awards") {
            let a = object(a)?;
            let fields = schema()["awards"].as_array().unwrap();
            ensure!(a.len() == fields.len(), "Incomplete season awards");
            let mut awards = serde_json::Map::new();
            for f in fields {
                let k = f["key"].as_str().unwrap();
                let n = a.get(k).context("Incomplete season awards")?;
                number(n, 0., f["maximum"].as_f64().unwrap(), true)?;
                awards.insert(k.into(), n.clone());
            }
            clean["awards"] = json!(awards);
        }
        result.push(clean);
    }
    Ok(json!(result))
}
pub fn subset(v: &Value, groups: &Value) -> Result<Value> {
    let mut v = validate(v)?;
    let groups = groups.as_array().context("Choose import sections")?;
    ensure!(
        !groups.is_empty()
            && groups
                .iter()
                .all(|g| schema()["groups"].as_array().unwrap().contains(g)),
        "Choose at least one valid DNA section"
    );
    v["info"]
        .as_object_mut()
        .unwrap()
        .retain(|k, _| groups.contains(&json!(info_group(k))));
    v["fields"].as_object_mut().unwrap().retain(|k, _| {
        schema()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == *k)
            .is_some_and(|f| groups.contains(&f["group"]))
    });
    if !groups.contains(&json!("seasons")) {
        v.as_object_mut().unwrap().remove("seasonHistory");
        v["version"] = json!(1);
    }
    Ok(v)
}
/// Mirrors WPF's reviewed year-based merge; no process tokens or source IDs survive.
pub fn merge_seasons(source: &Value, existing: &Value) -> Result<Value> {
    let existing = seasons(existing)?;
    let source = seasons(source)?;
    let mut by_year = std::collections::BTreeMap::new();
    for row in existing.as_array().unwrap() {
        ensure!(
            by_year
                .insert(row["year"].as_f64().unwrap() as u64, row.clone())
                .is_none(),
            "Destination has ambiguous season years"
        );
    }
    let mut selected = HashSet::new();
    for row in source.as_array().unwrap() {
        let year = row["year"].as_f64().unwrap() as u64;
        ensure!(
            selected.insert(year),
            "Choose one aggregate row per imported season"
        );
        let mut row = row.clone();
        if row.get("awards").is_none() {
            row["awards"] = by_year
                .get(&year)
                .and_then(|r| r.get("awards"))
                .cloned()
                .unwrap_or_else(|| {
                    json!(
                        schema()["awards"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|f| (f["key"].as_str().unwrap().to_owned(), json!(0)))
                            .collect::<serde_json::Map<String, Value>>()
                    )
                });
        }
        by_year.insert(year, row);
    }
    seasons(&json!(by_year.into_values().rev().collect::<Vec<_>>()))
}
pub fn stage_catalog_seasons(document: &Value, catalog: &Value, request: &Value) -> Result<Value> {
    let mut document = validate(document)?;
    let player = catalog["players"]
        .as_array()
        .context("Players missing")?
        .iter()
        .find(|p| p["id"] == request["id"])
        .context("Player not found")?;
    let history = player["history"].as_array().context("History missing")?;
    let chosen = request["selected"].as_array().context("Choose seasons")?;
    ensure!(
        !chosen.is_empty() && chosen.len() <= 25,
        "Choose 1–25 seasons"
    );
    let teams = catalog["teams"].as_array().context("Teams missing")?;
    let team = |v: &Value| -> Result<String> {
        let t = teams
            .iter()
            .find(|t| t["category"] == "current" && t["id"] == *v)
            .context("Choose a normal team for each checked season")?;
        Ok(t["name"].as_str().context("Team name missing")?.to_owned())
    };
    let mut ids = HashSet::new();
    let mut rows = Vec::new();
    for choice in chosen {
        ensure!(
            history.contains(&choice["id"]),
            "Season does not belong to this player"
        );
        ensure!(
            ids.insert(serde_json::to_string(&choice["id"])?),
            "Duplicate selected season"
        );
        let record = catalog["records"]
            .as_array()
            .context("Records missing")?
            .iter()
            .find(|r| r["id"] == choice["id"])
            .context("Season missing")?;
        let mut row = json!({"year":record["year"],"team":team(&choice["teamId"])? ,"totals":record["totals"]});
        if !record["secondaryTeamId"].is_null() {
            row["secondaryTeam"] = json!(team(&choice["secondaryTeamId"])?);
        }
        if request["awards"] == true {
            row["awards"] = record["awards"].clone();
        }
        rows.push(row);
    }
    document["seasonHistory"] = merge_seasons(
        &json!(rows),
        document.get("seasonHistory").unwrap_or(&json!([])),
    )?;
    document["version"] = json!(2);
    validate(&document)
}
pub fn from_catalog(p: &Value, keys: &Value, source_schema: &Value) -> Result<Value> {
    ensure!(
        schema()["compatible"]
            .as_array()
            .unwrap()
            .contains(source_schema),
        "Bundled player catalog schema is incompatible"
    );
    let keys = keys.as_array().context("Catalog field keys missing")?;
    let values = p["data"]["values"]
        .as_array()
        .context("Player values missing")?;
    ensure!(keys.len() == values.len(), "Catalog values are incomplete");
    let fields: serde_json::Map<String, Value> = keys
        .iter()
        .zip(values)
        .map(|(k, v)| {
            Ok((
                k.as_str().context("Invalid catalog key")?.to_string(),
                v.clone(),
            ))
        })
        .collect::<Result<_>>()?;
    validate(
        &json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":schema()["schemaId"],"name":([p["firstName"].as_str().unwrap_or(""),p["lastName"].as_str().unwrap_or("")].into_iter().filter(|s|!s.is_empty()).collect::<Vec<_>>().join(" ")),"info":p["data"]["info"],"fields":fields}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_loaded_player_copy_oracle_and_portable_privacy() {
        let oracle: Value = serde_json::from_slice(include_bytes!(
            "../assets/browser-transfer-regressions.json"
        ))
        .unwrap();
        for (index, case) in oracle["copies"].as_array().unwrap().iter().enumerate() {
            let actual = from_player(&case["input"]);
            if case["expected"].is_null() {
                assert!(actual.is_err(), "Unexpected copy acceptance {index}");
            } else {
                let actual = actual.unwrap_or_else(|e| panic!("Copy {index}: {e:#}"));
                assert_eq!(actual, case["expected"], "Copy {index}");
                for key in [
                    "recordAddress",
                    "recordIndex",
                    "pid",
                    "teamName",
                    "teamIndex",
                ] {
                    assert!(actual.get(key).is_none());
                }
            }
        }
    }
    #[test]
    fn baseline_validation_oracle() {
        let cases: Value =
            serde_json::from_str(include_str!("../assets/player-dna-regressions.json")).unwrap();
        for c in cases.as_array().unwrap() {
            assert_eq!(
                json!(validate(&c["input"]).ok()),
                c["expected"],
                "{}",
                c["input"]
            );
        }
    }
    fn valid() -> Value {
        json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":schema()["schemaId"],"name":"QA player","info":{"birthDate":"2000-02-29","yearsPro":3},"fields":{}})
    }
    #[test]
    fn privacy_and_versions() {
        let mut v = valid();
        v["pointer"] = json!(12345);
        let clean = validate(&v).unwrap();
        assert!(clean.get("pointer").is_none());
        v["version"] = json!(2);
        assert!(validate(&v).is_err());
        v["seasonHistory"] = json!([]);
        assert!(validate(&v).is_ok());
        v["version"] = json!(1);
        assert!(validate(&v).is_err());
    }
    #[test]
    fn birthday_and_years_pro_ranges() {
        let mut v = valid();
        assert!(validate(&v).is_ok());
        for date in [
            "1900-02-29",
            "4096-01-01",
            "2000-13-01",
            "2026-04-31",
            "not-a-date",
        ] {
            v["info"]["birthDate"] = json!(date);
            assert!(validate(&v).is_err());
        }
        v["info"]["birthDate"] = json!("2000-02-29");
        v["info"]["yearsPro"] = json!(32);
        assert!(validate(&v).is_err());
    }
    #[test]
    fn every_field_boundary() {
        for f in schema()["fields"].as_array().unwrap() {
            let mut v = valid();
            let key = f["key"].as_str().unwrap();
            let max = f["maximum"].as_i64().unwrap();
            v["fields"][key] = json!(max);
            assert_eq!(validate(&v).is_ok(), max >= 0, "{key}");
            v["fields"][key] = json!(max + 1);
            assert!(validate(&v).is_err(), "{key}");
        }
    }
    #[test]
    fn presentation_metadata_matches_existing_wpf_controls() {
        let fixture: Value =
            serde_json::from_str(include_str!("../assets/player-web-regressions.json")).unwrap();
        let fields = schema()["fields"].as_array().unwrap();
        assert_eq!(fields.len(), fixture["fields"].as_array().unwrap().len());
        for expected in fixture["fields"].as_array().unwrap() {
            let actual = fields.iter().find(|f| f["key"] == expected["key"]).unwrap();
            for key in ["group", "money", "maximum", "choices"] {
                assert_eq!(actual[key], expected[key], "{} {key}", expected["key"]);
            }
        }
    }
    #[test]
    fn sections_filter_and_remove_seasons() {
        let v = valid();
        assert!(subset(&v, &json!([])).is_err());
        let mut v = v;
        v["version"] = json!(2);
        v["seasonHistory"] = json!([]);
        let s = subset(&v, &json!(["attributes"])).unwrap();
        assert_eq!(s["version"], 1);
        assert!(s.get("seasonHistory").is_none());
        assert!(s["info"].as_object().unwrap().is_empty());
    }
    fn season(year: u32) -> Value {
        json!({"year":year,"team":"Warriors","totals":schema()["seasonLimits"].as_object().unwrap().keys()
            .map(|k|(k.clone(),json!(0))).collect::<serde_json::Map<String,Value>>()})
    }
    #[test]
    fn season_merge_matches_wpf() {
        let cases: Value =
            serde_json::from_str(include_str!("../assets/season-regressions.json")).unwrap();
        for c in cases.as_array().unwrap() {
            assert_eq!(
                json!(merge_seasons(&c["source"], &c["existing"]).ok()),
                c["expected"],
                "{c}"
            );
        }
    }
    #[test]
    fn floating_year_duplicate_and_merge_are_safe() {
        let a = season(2019);
        let mut b = a.clone();
        b["year"] = json!(2019.0);
        assert!(seasons(&json!([a, b])).is_err());
        b["year"] = json!(2020.0);
        assert_eq!(
            merge_seasons(&json!([b]), &json!([]))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn catalog_season_scope_and_team_review() {
        let mut record = season(2019);
        record["id"] = json!(10);
        record["secondaryTeamId"] = json!(null);
        record["awards"] = json!(
            schema()["awards"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (f["key"].as_str().unwrap().to_owned(), json!(0)))
                .collect::<serde_json::Map<String, Value>>()
        );
        record["awards"]["mvp"] = json!(1);
        let catalog = json!({"players":[{"id":1,"history":[10]}],"records":[record],
            "teams":[{"id":0,"name":"76ers","category":"current"},{"id":99,"name":"76ers","category":"historic"}]});
        let request = json!({"id":1,"selected":[{"id":10,"teamId":0}],"awards":true});
        let r = stage_catalog_seasons(&valid(), &catalog, &request).unwrap();
        assert_eq!(r["info"], valid()["info"]);
        assert_eq!(r["name"], valid()["name"]);
        assert_eq!(r["seasonHistory"][0]["team"], "76ers");
        assert_eq!(r["seasonHistory"][0]["awards"]["mvp"], 1);
        assert!(r["seasonHistory"][0].get("id").is_none());
        let mut bad = request.clone();
        bad["selected"][0]["teamId"] = json!(99);
        assert!(stage_catalog_seasons(&valid(), &catalog, &bad).is_err());
        bad = request.clone();
        bad["selected"][0]["id"] = json!(11);
        assert!(stage_catalog_seasons(&valid(), &catalog, &bad).is_err());
        bad = request.clone();
        bad["selected"] = json!([]);
        assert!(stage_catalog_seasons(&valid(), &catalog, &bad).is_err());
        let mut keep = request;
        keep["awards"] = json!(false);
        assert_eq!(
            stage_catalog_seasons(&r, &catalog, &keep).unwrap()["seasonHistory"][0]["awards"]["mvp"],
            1
        );
    }
}
