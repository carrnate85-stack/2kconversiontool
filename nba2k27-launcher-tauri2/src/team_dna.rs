//! Portable Team DNA validation and offline staging. Never transfers destination IDs.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::OnceLock};
pub fn schema() -> &'static Value {
    static S: OnceLock<Value> = OnceLock::new();
    S.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/team-schema.json"))
            .expect("Verified team schema")
    })
}
pub fn read_file(path: &std::path::Path) -> Result<Value> {
    use std::io::Read;
    ensure!(
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("2kteam")),
        "Select a .2kteam file"
    );
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= 512 * 1024,
        "Team DNA exceeds 512 KB"
    );
    let mut bytes = Vec::new();
    file.take(512 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 512 * 1024,
        "Team DNA changed or exceeds 512 KB"
    );
    validate(&serde_json::from_slice(&bytes)?)
}
fn obj(v: &Value) -> Result<&serde_json::Map<String, Value>> {
    v.as_object().context("Expected a Team DNA object")
}
fn text(v: &Value, max: usize) -> Result<&str> {
    let s = v.as_str().context("Expected Team DNA text")?;
    ensure!(
        s.encode_utf16().count() <= max && !s.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
        "Invalid Team DNA text"
    );
    Ok(s)
}
fn integer(v: &Value, min: f64, max: f64) -> Result<f64> {
    let n = v.as_f64().context("Expected a numeric Team DNA value")?;
    ensure!(
        n.is_finite() && n.fract() == 0. && n >= min && n <= max,
        "Team DNA value out of range"
    );
    Ok(n)
}
const NATIVE_TEAM_NAMES: &[&str] = &[
    "NORMAL",
    "HISTORIC",
    "USER",
    "ONLINE",
    "ALLSTARS_EAST",
    "ALLSTARS_WEST",
    "STREET_HOME",
    "STREET_AWAY",
    "ROOKIES",
    "SOPHOMORES",
    "ROOKIE_SHOWCASE",
    "MYTEAM_TEAM",
    "EUROLEAGUE",
    "DOMINATION",
    "HIGH_SCHOOL",
    "COLLEGE_MENS",
    "COLLEGE_WOMENS",
    "AAU",
    "G_LEAGUE",
    "IGNITE",
    "SPORTS_2K",
    "HOOPS_SUMMIT",
    "FIBA",
    "FIBA_HIDDEN",
    "EXPANSION_SLOT",
    "EXPANSION_PREMADE",
    "HISTORIC_SNAPSHOT",
    "ALL_TIME",
    "E_LEAGUE",
    "WNBA",
    "DECADE",
    "JORDAN",
    "REBRAND_TEMPLATE",
    "HIDDEN",
    "RANDOMIZED",
    "COED",
];
fn native_metadata(v: &Value) -> Result<Value> {
    let fields = obj(v)?;
    ensure!(
        fields.keys().all(|k| [
            "format",
            "version",
            "rosterTypeName",
            "classicSeasonEndYear",
            "arena"
        ]
        .contains(&k.as_str())),
        "Unsupported native Team DNA metadata field"
    );
    ensure!(
        v["format"] == "nba2k27-native-team-metadata" && v["version"].as_u64() == Some(1),
        "Unsupported native Team DNA metadata version"
    );
    if let Some(kind) = v.get("rosterTypeName") {
        ensure!(
            kind.as_str()
                .is_some_and(|s| NATIVE_TEAM_NAMES.contains(&s)),
            "Unknown native Team DNA team type"
        );
    }
    if let Some(year) = v.get("classicSeasonEndYear").filter(|x| !x.is_null()) {
        integer(year, 1960., 2059.)?;
    }
    if let Some(arena) = v.get("arena") {
        ensure!(
            obj(arena)?.len() == 3
                && ["name", "arenaFile", "floorFile"]
                    .iter()
                    .all(|k| arena.get(*k).is_some()),
            "Native Team DNA arena needs exact logical identity"
        );
        // Native arena name is 35 UTF-16 units including its terminator.
        text(&arena["name"], 34)?;
        for (key, max) in [("arenaFile", 37), ("floorFile", 39)] {
            let s = text(&arena[key], max)?;
            ensure!(
                !s.is_empty() && s.bytes().all(|b| (32..=126).contains(&b)),
                "Invalid native Team DNA arena asset"
            );
        }
    }
    Ok(v.clone())
}
pub fn staff_values(v: &Value) -> Result<Value> {
    let fields = schema()["staff"].as_array().unwrap();
    let values = obj(v)?;
    ensure!(values.len() <= fields.len(), "Too many staff changes");
    for (key, value) in values {
        let f = fields
            .iter()
            .find(|f| f["key"] == *key)
            .with_context(|| format!("Unsupported staff field: {key}"))?;
        if f["text"] == true {
            text(value, f["maximum"].as_u64().unwrap() as usize)?;
        } else {
            let n = integer(
                value,
                f["minimum"].as_f64().unwrap(),
                f["maximum"].as_f64().unwrap(),
            )?;
            let choices = f["choices"].as_array().unwrap();
            ensure!(
                choices.is_empty()
                    || n < (choices.len() as f64)
                    || (key.ends_with("SystemId") && n == 15.),
                "Unsupported staff choice: {key}"
            );
        }
    }
    Ok(v.clone())
}
fn uniform_values(v: &Value) -> Result<Value> {
    for (key, value) in obj(v)? {
        let f = schema()["uniforms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == *key)
            .with_context(|| format!("Unsupported uniform field: {key}"))?;
        integer(value, 0., f["maximum"].as_f64().unwrap())?;
    }
    Ok(v.clone())
}
pub fn validate(v: &Value) -> Result<Value> {
    ensure!(
        serde_json::to_vec(v)?.len() <= 512 * 1024,
        "Team DNA exceeds 512 KB"
    );
    obj(v)?;
    ensure!(
        v["format"] == "2kteam" && v["version"].as_f64() == Some(1.) && v["game"] == "NBA2K27",
        "Unsupported Team DNA file"
    );
    text(&v["name"], 80)?;
    let info = obj(&v["info"])?;
    for (key, value) in info {
        match key.as_str() {
            "name" => {
                text(value, 23)?;
            }
            "cityName" => {
                text(value, 17)?;
            }
            "cityShort" | "stateShort" => {
                text(value, 6)?;
            }
            "mainLogo" => {
                text(value, 35)?;
            }
            "rosterTypeId" => {
                integer(value, 0., 31.)?;
            }
            "historicYearId" => {
                integer(value, 0., 127.)?;
            }
            _ => anyhow::bail!("Unsupported Team DNA field: {key}"),
        }
    }
    let mut arena = json!(null);
    if let Some(a) = v.get("arena").filter(|a| !a.is_null()) {
        obj(a)?;
        let name = text(&a["name"], 32)?;
        let arena_file = text(&a["arenaFile"], 37)?;
        let floor_file = text(&a["floorFile"], 39)?;
        for stem in [arena_file, floor_file] {
            ensure!(
                !stem.is_empty()
                    && stem
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_ -".contains(&b)),
                "Invalid arena or floor asset stem"
            );
        }
        arena = json!({"name":name,"arenaFile":arena_file,"floorFile":floor_file});
    }
    let rows = v["staff"].as_array().context("Staff must be an array")?;
    ensure!(rows.len() <= 32, "Too many staff slots");
    let mut slots = HashSet::new();
    let mut staff = Vec::new();
    for row in rows {
        obj(row)?;
        let slot = integer(&row["slot"], 0., 31.)? as u32;
        ensure!(slots.insert(slot), "Duplicate staff slot");
        staff.push(json!({"slot":slot,"values":staff_values(&row["values"])?}));
    }
    let mut rotations = json!(null);
    if let Some(r) = v.get("rotations").filter(|r| !r.is_null()) {
        let rows = r.as_array().context("Rotations must be an array")?;
        ensure!(rows.len() == 48, "Rotations need 48 segments");
        for row in rows {
            let names = row
                .as_array()
                .context("Rotation segment must be an array")?;
            ensure!(names.len() == 5, "Each segment needs five players");
            let mut unique = HashSet::new();
            for name in names {
                ensure!(
                    unique.insert(text(name, 39)?),
                    "Duplicate player in rotation segment"
                );
            }
        }
        rotations = r.clone();
    }
    let empty = json!([]);
    let rows = v
        .get("uniforms")
        .unwrap_or(&empty)
        .as_array()
        .context("Uniforms must be an array")?;
    ensure!(rows.len() <= 128, "Too many uniform slots");
    let mut uniforms = Vec::new();
    for row in rows {
        obj(row)?;
        let m = uniform_values(&row["match"])?;
        let keys = obj(&m)?;
        ensure!(
            keys.len() == 3
                && ["typeId", "sideId", "designationId"]
                    .iter()
                    .all(|k| keys.contains_key(*k)),
            "Uniform match needs type, side and designation"
        );
        uniforms.push(json!({"match":m,"values":uniform_values(&row["values"])?}));
    }
    let mut result = json!({"format":"2kteam","version":1,"game":"NBA2K27","name":v["name"],"info":info,"arena":arena,"staff":staff,"rotations":rotations,"uniforms":uniforms});
    if let Some(metadata) = v.get("nativeMetadata") {
        result["nativeMetadata"] = native_metadata(metadata)?;
    }
    Ok(result)
}
pub fn subset(v: &Value, sections: &Value) -> Result<Value> {
    let mut clean = validate(v)?;
    let selected = sections.as_array().context("Choose import sections")?;
    ensure!(
        !selected.is_empty()
            && selected
                .iter()
                .all(|s| schema()["sections"].as_array().unwrap().contains(s)),
        "Choose at least one valid Team DNA section"
    );
    for (key, empty) in [
        ("info", json!({})),
        ("arena", json!(null)),
        ("staff", json!([])),
        ("rotations", json!(null)),
        ("uniforms", json!([])),
    ] {
        if !selected.contains(&json!(key)) {
            clean[key] = empty;
        }
    }
    if let Some(metadata) = clean
        .get_mut("nativeMetadata")
        .and_then(Value::as_object_mut)
    {
        if !selected.contains(&json!("info")) {
            metadata.remove("rosterTypeName");
            metadata.remove("classicSeasonEndYear");
        }
        if !selected.contains(&json!("arena")) {
            metadata.remove("arena");
        }
    }
    Ok(clean)
}
pub fn stage(destination: &Value, source: &Value, sections: &Value) -> Result<Value> {
    let mut target = validate(destination)?;
    let imported = subset(source, sections)?;
    for section in sections.as_array().unwrap() {
        let key = section.as_str().unwrap();
        if key == "info" {
            for (k, v) in obj(&imported["info"])? {
                target["info"][k] = v.clone();
            }
        } else {
            target[key] = imported[key].clone();
        }
    }
    if let Some(imported_metadata) = imported.get("nativeMetadata") {
        if target.get("nativeMetadata").is_none() {
            target["nativeMetadata"] = json!({"format":"nba2k27-native-team-metadata","version":1});
        }
        for (key, value) in obj(imported_metadata)? {
            target["nativeMetadata"][key] = value.clone();
        }
    }
    validate(&target)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn blank() -> Value {
        json!({"format":"2kteam","version":1,"game":"NBA2K27","name":"Destination","info":{"name":"Team"},"staff":[]})
    }
    #[test]
    fn baseline_validation_oracle() {
        let cases: Value =
            serde_json::from_str(include_str!("../assets/team-dna-regressions.json")).unwrap();
        for c in cases.as_array().unwrap() {
            assert_eq!(
                validate(&c["input"]).ok(),
                c.get("expected").filter(|v| !v.is_null()).cloned(),
                "{}",
                c["input"]
            );
        }
    }
    #[test]
    fn category_staging_preserves_destination_and_privacy() {
        let mut source = blank();
        source["name"] = json!("Source");
        source["info"]["cityName"] = json!("Boston");
        source["staff"] = json!([{"slot":0,"values":{"faceId":123,"preferredSystemId":15}}]);
        source["teamId"] = json!(999);
        source["pointer"] = json!("0x123");
        let r = stage(&blank(), &source, &json!(["staff"])).unwrap();
        assert_eq!(r["name"], "Destination");
        assert_eq!(r["info"]["name"], "Team");
        assert_eq!(r["staff"][0]["values"]["faceId"], 123);
        assert!(r.get("teamId").is_none());
        assert!(subset(&source, &json!([])).is_err());
        source["staff"][0]["values"]["staffId"] = json!(7);
        assert!(validate(&source).is_err());
    }
    #[test]
    fn native_metadata_is_semantic_versioned_and_section_scoped() {
        let mut v = blank();
        v["nativeMetadata"] = json!({"format":"nba2k27-native-team-metadata","version":1,"rosterTypeName":"WNBA","classicSeasonEndYear":2017,"arena":{"name":"Court","arenaFile":"arena_court","floorFile":"floor_court"}});
        let clean = validate(&v).unwrap();
        assert_eq!(clean["nativeMetadata"], v["nativeMetadata"]);
        let info = subset(&v, &json!(["info"])).unwrap();
        assert!(info["nativeMetadata"].get("arena").is_none());
        assert_eq!(info["nativeMetadata"]["rosterTypeName"], "WNBA");
        let arena = subset(&v, &json!(["arena"])).unwrap();
        assert!(
            arena["nativeMetadata"]
                .get("classicSeasonEndYear")
                .is_none()
        );
        assert!(arena["nativeMetadata"]["arena"].is_object());
        let mut destination = v.clone();
        destination["nativeMetadata"]["rosterTypeName"] = json!("NORMAL");
        let staged = stage(&destination, &v, &json!(["arena"])).unwrap();
        assert_eq!(staged["nativeMetadata"]["rosterTypeName"], "NORMAL");
        for (field, bad) in [
            ("version", json!(2)),
            ("rosterTypeName", json!(29)),
            ("rosterTypeName", json!("Unknown")),
            ("classicSeasonEndYear", json!(2059.5)),
            ("classicSeasonEndYear", json!(1959)),
            ("pointer", json!(123)),
        ] {
            let mut corrupt = v.clone();
            corrupt["nativeMetadata"][field] = bad;
            assert!(validate(&corrupt).is_err(), "{field}");
        }
        let mut corrupt = v.clone();
        corrupt["nativeMetadata"]["arena"]["arenaId"] = json!(12);
        assert!(validate(&corrupt).is_err());
        for units in [33, 34, 35] {
            let mut named = v.clone();
            named["nativeMetadata"]["arena"]["name"] = json!("a".repeat(units));
            assert_eq!(validate(&named).is_ok(), units <= 34);
        }
        let mut named = v.clone();
        named["nativeMetadata"]["arena"]["name"] = json!("😀".repeat(18));
        assert!(validate(&named).is_err());
        let mut logical = v.clone();
        logical["nativeMetadata"]["arena"]["arenaFile"] = json!("arena.court#1");
        assert!(validate(&logical).is_ok());
        for year in [json!(null), json!(1960), json!(2000), json!(2059)] {
            v["nativeMetadata"]["classicSeasonEndYear"] = year;
            assert!(validate(&v).is_ok());
        }
        v.as_object_mut().unwrap().remove("nativeMetadata");
        assert!(validate(&v).unwrap().get("nativeMetadata").is_none());
    }
}
