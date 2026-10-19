//! Read-only, bounded search of the app's embedded NBA 2K23 source catalog.
use anyhow::{Context, Result, ensure};
#[path = "player_catalog_import.rs"]
pub(crate) mod import;
pub(crate) fn portable_name(first: &str, last: &str) -> String {
    let text = format!("{first} {last}");
    let mut units = 0;
    text.trim()
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= 80
        })
        .collect()
}
use serde_json::{Value, json};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use windows::Win32::Globalization::{NormalizationKD, NormalizeString};
fn normalize(input: &str) -> Result<String> {
    let text = if input.is_ascii() {
        input.to_owned()
    } else {
        let source: Vec<u16> = input.encode_utf16().collect();
        let estimate = unsafe { NormalizeString(NormalizationKD, &source, None) };
        ensure!(
            estimate > 0 && estimate < 16384,
            "Could not normalize player search"
        );
        let mut result = vec![0u16; estimate as usize];
        let mut size = unsafe { NormalizeString(NormalizationKD, &source, Some(&mut result)) };
        if size < 0 && size > -16384 {
            result.resize((-size) as usize, 0);
            size = unsafe { NormalizeString(NormalizationKD, &source, Some(&mut result)) };
        }
        ensure!(
            size > 0 && size as usize <= result.len(),
            "Could not normalize player search"
        );
        String::from_utf16(&result[..size as usize])?
    };
    static MARKS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let text = MARKS
        .get_or_init(|| regex::Regex::new(r"\p{M}").unwrap())
        .replace_all(&text, "")
        .to_lowercase();
    // JavaScript's \s set (not Rust's slightly different White_Space set).
    let space = |c: char| matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}');
    Ok(text
        .split(space)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}

// Original browser UI's source-search/review/selection contract. Selection is
// portable data only; it cannot dispatch a memory write or arbitrary command.
#[cfg(test)]
pub fn browser_request(catalog: &Value, args: &Value) -> Result<Value> {
    let id = format!("{:x}", Sha256::digest(serde_json::to_vec(catalog)?));
    browser_request_with_id(catalog, args, &id)
}
pub fn browser_request_with_id(catalog: &Value, args: &Value, id: &str) -> Result<Value> {
    let players = catalog["players"].as_array().context("Players missing")?;
    let records = catalog["records"].as_array().context("Seasons missing")?;
    let teams = catalog["teams"].as_array().context("Teams missing")?;
    let action = args["action"].as_str().context("Catalog action missing")?;
    if action == "status" {
        return Ok(
            json!({"loaded":true,"bundled":true,"catalogId":id,"sourceSha256":catalog["source"]["sha256"],"players":players.len(),"playersWithHistory":players.iter().filter(|p|p["history"].as_array().is_some_and(|r|!r.is_empty())).count(),"seasons":records.len(),"unified":catalog["format"]!="2k23-season-catalog","mappedFields":catalog["fieldKeys"].as_array().map(Vec::len).unwrap_or(0)}),
        );
    }
    ensure!(
        args["catalogId"].as_str() == Some(id),
        "Source catalog changed. Reopen the player search."
    );
    let summary = |p: &Value| {
        let mut row = p.clone();
        let object = row.as_object_mut().unwrap();
        object.remove("data");
        object.remove("history");
        let history = p["history"].as_array().unwrap();
        let years: Vec<i64> = records
            .iter()
            .filter(|r| history.contains(&r["id"]))
            .filter_map(|r| r["year"].as_i64())
            .collect();
        object.insert(
            "name".into(),
            json!(format!(
                "{} {}",
                p["firstName"].as_str().unwrap_or(""),
                p["lastName"].as_str().unwrap_or("")
            )),
        );
        object.insert(
            "context".into(),
            json!(
                p["teams"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|id| teams.iter().find(|t| t["id"] == *id))
                    .map(|t| [
                        t["city"].as_str().unwrap_or(""),
                        t["name"].as_str().unwrap_or("")
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "))
                    .collect::<Vec<_>>()
            ),
        );
        object.insert("seasonCount".into(), json!(years.len()));
        object.insert("firstYear".into(), json!(years.iter().min()));
        object.insert("lastYear".into(), json!(years.iter().max()));
        row
    };
    match action {
        "search" => {
            let mut request = args.clone();
            if request.get("limit").is_none() {
                request["limit"] = json!(40);
            }
            let found = search(catalog, &request)?;
            let rows = found["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| summary(players.iter().find(|p| p["id"] == r["id"]).unwrap()))
                .collect::<Vec<_>>();
            Ok(
                json!({"catalogId":id,"total":found["total"],"offset":found["offset"],"limit":found["limit"],"players":rows}),
            )
        }
        "selection" => {
            let selected = browser_request_with_id(
                catalog,
                &json!({"action":"player","catalogId":id,"playerId":args["playerId"]}),
                id,
            )?;
            let sections = args["sections"]
                .as_array()
                .context("Choose import sections")?;
            ensure!(
                !sections.is_empty() && sections.len() <= 20,
                "Choose at least one valid import section"
            );
            let mut data = selected["data"].clone();
            if data.is_null() {
                data = json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":crate::player_dna::schema()["schemaId"],"name":portable_name(selected["player"]["firstName"].as_str().unwrap_or(""),selected["player"]["lastName"].as_str().unwrap_or("")),"info":{},"fields":{}});
            }
            if sections.contains(&json!("seasons")) {
                let ids = args["seasonIds"]
                    .as_array()
                    .context("Choose source seasons")?;
                ensure!(
                    !ids.is_empty() && ids.len() <= 25,
                    "Choose 1–25 unique source seasons"
                );
                let mut seen = BTreeSet::new();
                let mut history = Vec::new();
                for value in ids {
                    let id = source_integer(value, 65534)?;
                    ensure!(seen.insert(id), "Choose unique source seasons");
                    let r = selected["seasons"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|r| r["id"].as_u64() == Some(id))
                        .context("Selected season does not belong to this source player")?;
                    let mut row = json!({"year":r["year"],"team":r["primaryTeam"]["name"],"secondaryTeam":r["secondaryTeam"].get("name").cloned().unwrap_or(Value::Null),"totals":r["totals"]});
                    if args["includeAwards"] != false {
                        row["awards"] = r["awards"].clone();
                    }
                    history.push(row);
                }
                data["version"] = json!(2);
                data["seasonHistory"] = json!(history);
            }
            Ok(
                json!({"data":crate::player_dna::subset(&data,&args["sections"] )?,"warnings":selected["warnings"],"sourcePlayer":selected["player"],"catalogId":id}),
            )
        }
        "player" => {
            let source = source_integer(&args["playerId"], 8191)?;
            let p = players
                .iter()
                .find(|p| p["id"].as_u64() == Some(source))
                .context("Source player unavailable")?;
            let data = if catalog["format"] == "2k23-season-catalog" {
                Value::Null
            } else {
                let mut source = p.clone();
                // Portable DNA name has a smaller limit than source display names.
                source["firstName"] = json!(portable_name(
                    p["firstName"].as_str().unwrap_or(""),
                    p["lastName"].as_str().unwrap_or("")
                ));
                source["lastName"] = json!("");
                crate::player_dna::from_catalog(
                    &source,
                    &catalog["fieldKeys"],
                    &catalog["targetSchemaId"],
                )?
            };
            let groups = [
                "info",
                "appearance",
                "attributes",
                "tendencies",
                "personality",
                "hotzones",
            ];
            let counts: serde_json::Map<String, Value> = groups
                .iter()
                .map(|group| {
                    let info = data["info"]
                        .as_object()
                        .into_iter()
                        .flat_map(|o| o.keys())
                        .filter(|key| crate::player_dna::info_group(key) == *group)
                        .count();
                    let fields = crate::player_dna::schema()["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|f| {
                            f["group"] == *group
                                && data["fields"].get(f["key"].as_str().unwrap()).is_some()
                        })
                        .count();
                    ((*group).into(), json!(info + fields))
                })
                .collect();
            let mut seasons: Vec<Value> = records
                .iter()
                .filter(|r| p["history"].as_array().unwrap().contains(&r["id"]))
                .map(|r| {
                    let mut row = r.clone();
                    row["primaryTeam"] = teams
                        .iter()
                        .find(|t| t["id"] == r["primaryTeamId"])
                        .unwrap()
                        .clone();
                    row["secondaryTeam"] = teams
                        .iter()
                        .find(|t| t["id"] == r["secondaryTeamId"])
                        .cloned()
                        .unwrap_or(Value::Null);
                    let n = |k: &str| r["totals"][k].as_i64().unwrap_or(0);
                    row["scoringWarning"] =
                        json!(n("points") != 2 * n("fgm") + n("threeMade") + n("ftm"));
                    row
                })
                .collect();
            seasons.sort_by_key(|r| std::cmp::Reverse(r["year"].as_i64().unwrap()));
            let warnings: Vec<Value> = catalog["unsupported"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(p["data"]["warnings"].as_array().into_iter().flatten())
                .cloned()
                .collect();
            Ok(
                json!({"catalogId":id,"player":summary(p),"data":data,"sectionCounts":counts,"warnings":warnings,"seasons":seasons}),
            )
        }
        _ => anyhow::bail!("Unsupported source catalog action."),
    }
}
fn source_integer(value: &Value, max: u64) -> Result<u64> {
    let n = value.as_f64().context("Source ID must be a number")?;
    ensure!(
        n.is_finite() && n.fract() == 0. && n >= 0. && n <= max as f64,
        "Source ID is outside its supported range"
    );
    Ok(n as u64)
}

pub fn search(catalog: &Value, args: &Value) -> Result<Value> {
    let query = match args.get("query") {
        None | Some(Value::Null) => "",
        Some(v) => v.as_str().context("Search must be text")?,
    };
    ensure!(
        query.encode_utf16().count() <= 200 && !query.chars().any(|c| c <= '\u{001f}'),
        "Search is too long or contains controls"
    );
    let query = normalize(query)?;
    let offset = integer(args, "offset", 0, 0, 10_000)?;
    let limit = integer(args, "limit", 60, 1, 60)?;
    let players = catalog["players"].as_array().context("Players missing")?;
    let teams = catalog["teams"].as_array().context("Teams missing")?;
    let categories: BTreeSet<&str> = players
        .iter()
        .flat_map(|p| p["categories"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let category = match args.get("category") {
        None | Some(Value::Null) => "",
        Some(v) => v.as_str().context("Category must be text")?,
    };
    ensure!(
        category.is_empty() || categories.contains(category),
        "Unknown source category"
    );
    let context = |p: &Value| -> Vec<String> {
        p["teams"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|id| teams.iter().find(|t| t["id"] == *id))
            .map(|t| {
                format!(
                    "{} {}",
                    t["city"].as_str().unwrap_or(""),
                    t["name"].as_str().unwrap_or("")
                )
                .trim()
                .to_owned()
            })
            .collect()
    };
    let mut matches = Vec::new();
    for p in players {
        if catalog["format"] == "2k23-season-catalog"
            && p["history"].as_array().is_none_or(Vec::is_empty)
        {
            continue;
        }
        if !category.is_empty()
            && !p["categories"]
                .as_array()
                .is_some_and(|c| c.iter().any(|v| v.as_str() == Some(category)))
        {
            continue;
        }
        let groups = p["categories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        let haystack = normalize(&format!(
            "{} {} {} {}",
            p["firstName"].as_str().unwrap_or(""),
            p["lastName"].as_str().unwrap_or(""),
            context(p).join(" "),
            groups
        ))?;
        if query.split(' ').all(|term| haystack.contains(term)) {
            matches.push(p);
        }
    }
    Ok(
        json!({"total":matches.len(),"offset":offset,"limit":limit,"categories":categories,
        "items":matches.iter().skip(offset).take(limit).map(|p|json!({
            "id":p["id"],"name":format!("{} {}",p["firstName"].as_str().unwrap_or(""),p["lastName"].as_str().unwrap_or("")),
            "seasons":p["history"].as_array().map(Vec::len).unwrap_or(0),"teams":p["teams"],
            "categories":p["categories"],"context":context(p)
        })).collect::<Vec<_>>()}),
    )
}
fn integer(args: &Value, key: &str, default: usize, min: usize, max: usize) -> Result<usize> {
    let n = match args.get(key) {
        None => default as u64,
        Some(v) => v.as_u64().with_context(|| format!("Invalid {key}"))?,
    };
    ensure!(n >= min as u64 && n <= max as u64, "Invalid {key}");
    Ok(n as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
        if a == b {
            return None;
        }
        // JSON 220.0 and 220 are the same Number in the original browser.
        if a.is_number() && b.is_number() && a.as_f64() == b.as_f64() {
            return None;
        }
        if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
            for key in a.keys().chain(b.keys()) {
                if let Some(result) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    &format!("{path}/{key}"),
                ) {
                    return Some(result);
                }
            }
            return None;
        } else if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
            if a.len() != b.len() {
                return Some(format!("{path}: lengths {} != {}", a.len(), b.len()));
            }
            for (i, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                if let Some(result) = difference(a, b, &format!("{path}/{i}")) {
                    return Some(result);
                }
            }
            return None;
        }
        Some(format!("{path}: {a} != {b}"))
    }
    fn catalog() -> Value {
        json!({"players":(0..85).map(|id|json!({"id":id,"firstName":"Test","lastName":format!("Player {id}"),
            "categories":[if id < 60 {"current"} else {"historic"}],"teams":[7],"history":[]})).collect::<Vec<_>>(),
            "teams":[{"id":7,"city":"Philadelphia","name":"76ers"}]})
    }
    #[test]
    fn original_browser_catalog_search_and_review_match_wpf() {
        use std::io::Read;
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../assets/players.json.gz")[..])
            .read_to_end(&mut bytes)
            .unwrap();
        let c: Value = serde_json::from_slice(&bytes).unwrap();
        let oracle: Value =
            serde_json::from_slice(include_bytes!("../assets/browser-catalog-regressions.json"))
                .unwrap();
        let status = browser_request(&c, &json!({"action":"status"})).unwrap();
        let catalog_id = status["catalogId"].as_str().unwrap();
        assert_eq!(status["players"], 4274);
        for case in oracle["cases"].as_array().unwrap() {
            let mut request = case["request"].clone();
            request["action"] = json!("search");
            request["catalogId"] = status["catalogId"].clone();
            let mut actual = browser_request_with_id(&c, &request, catalog_id).unwrap();
            actual.as_object_mut().unwrap().remove("catalogId");
            assert_eq!(actual, case["expected"], "{}", case["request"]);
        }
        for case in oracle["reviews"].as_array().unwrap() {
            let mut actual = browser_request_with_id(
                &c,
                &json!({"action":"player","catalogId":status["catalogId"],"playerId":case["id"]}),
                catalog_id,
            )
            .unwrap();
            actual.as_object_mut().unwrap().remove("catalogId");
            if let Some(diff) = difference(&actual, &case["expected"], "") {
                panic!("source {}: {diff}", case["id"]);
            }
        }
        assert!(browser_request(&c, &json!({"action":"search","catalogId":"stale"})).is_err());
        for action in ["selection", "import", "savePlayer", "launch"] {
            assert!(
                browser_request(
                    &c,
                    &json!({"action":action,"catalogId":status["catalogId"]})
                )
                .is_err()
            );
        }
    }
    #[test]
    fn original_browser_category_and_season_selections_match_wpf_without_writes() {
        use std::io::Read;
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../assets/players.json.gz")[..])
            .read_to_end(&mut bytes)
            .unwrap();
        let catalog: Value = serde_json::from_slice(&bytes).unwrap();
        let oracle: Value = serde_json::from_slice(include_bytes!(
            "../assets/browser-transfer-regressions.json"
        ))
        .unwrap();
        for (index, case) in oracle["selections"].as_array().unwrap().iter().enumerate() {
            let mut request = case["input"].clone();
            request["catalogId"] = json!("qa-source");
            let actual = browser_request_with_id(&catalog, &request, "qa-source");
            if case["expected"].is_null() {
                assert!(actual.is_err(), "Unexpected selection acceptance {index}");
            } else {
                let mut expected = case["expected"].clone();
                expected["catalogId"] = json!("qa-source");
                let actual = actual.unwrap_or_else(|e| panic!("Selection {index}: {e:#}"));
                if let Some(diff) = difference(&actual, &expected, "") {
                    panic!("Selection {index}: {diff}");
                }
            }
        }
        let integer = browser_request_with_id(
            &catalog,
            &json!({"action":"player","catalogId":"qa-source","playerId":512}),
            "qa-source",
        )
        .unwrap();
        assert_eq!(
            browser_request_with_id(
                &catalog,
                &json!({"action":"player","catalogId":"qa-source","playerId":512.0}),
                "qa-source"
            )
            .unwrap(),
            integer
        );
        assert!(browser_request_with_id(&catalog,&json!({"action":"selection","catalogId":"stale","playerId":512,"sections":["info"]}),"qa-source").is_err());
    }
    #[test]
    fn bounded_pages_cover_catalog_without_duplicates() {
        let c = catalog();
        let mut ids = Vec::new();
        for offset in [0, 40, 80] {
            let result = search(&c, &json!({"offset":offset,"limit":40})).unwrap();
            assert_eq!(result["total"], 85);
            ids.extend(
                result["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| p["id"].as_u64().unwrap()),
            );
        }
        assert_eq!(ids, (0..85).collect::<Vec<_>>());
        assert!(
            search(&c, &json!({"offset":9999})).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn team_search_categories_and_legacy_defaults() {
        let c = catalog();
        assert_eq!(
            search(
                &c,
                &json!({"query":"  PHILADELPHIA  ","category":"historic"})
            )
            .unwrap()["total"],
            25
        );
        assert_eq!(
            search(&c, &json!({})).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            60
        );
        assert_eq!(
            search(&c, &json!({"query":"Test Player 84"})).unwrap()["items"][0]["id"],
            84
        );
        assert_eq!(search(&c, &json!({"query":"nothing"})).unwrap()["total"], 0);
    }
    #[test]
    fn malformed_search_and_paging_fail_closed() {
        for args in [
            json!({"offset":-1}),
            json!({"offset":0.5}),
            json!({"offset":10001}),
            json!({"limit":0}),
            json!({"limit":61}),
            json!({"limit":"40"}),
            json!({"query":1}),
            json!({"query":"x".repeat(201)}),
            json!({"category":"fake"}),
            json!({"category":1}),
        ] {
            assert!(search(&catalog(), &args).is_err(), "{args}");
        }
    }
}
