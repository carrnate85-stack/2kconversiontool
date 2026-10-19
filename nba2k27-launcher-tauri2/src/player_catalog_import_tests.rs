use super::*;
fn fixture() -> Value {
    let mut b = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("../assets/players.json.gz")[..])
        .read_to_end(&mut b)
        .unwrap();
    let mut v: Value = serde_json::from_slice(&b).unwrap();
    v["players"] = json!([v["players"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| !p["history"].as_array().unwrap().is_empty())
        .unwrap()]);
    let refs = v["players"][0]["history"].as_array().unwrap().clone();
    v["records"] = json!(
        v["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| refs.contains(&r["id"]))
            .collect::<Vec<_>>()
    );
    v
}
use std::io::Read;
#[test]
fn validates_real_bundled_and_minimal_sources() {
    let v = fixture();
    let clean = validate(&v, &|| Ok(())).unwrap();
    assert_eq!(clean.value()["players"].as_array().unwrap().len(), 1);
    let mut stats = v;
    stats["format"] = json!("2k23-season-catalog");
    stats.as_object_mut().unwrap().remove("fieldKeys");
    let clean = validate(&stats, &|| Ok(())).unwrap();
    let response=crate::player_catalog::browser_request_with_id(clean.value(),&json!({"action":"player","catalogId":"fixture","playerId":clean.value()["players"][0]["id"]}),"fixture").unwrap();
    assert!(response["data"].is_null());
    assert_eq!(response["sectionCounts"]["info"], 0);
}
#[test]
fn rejects_cross_references_values_schemas_and_cancelled_work() {
    let v = fixture();
    for key in ["id", "primaryTeamId"] {
        if !v["records"].as_array().unwrap().is_empty() {
            let mut x = v.clone();
            x["records"][0][key] = json!(65535);
            assert!(validate(&x, &|| Ok(())).is_err());
        }
    }
    let mut x = v.clone();
    x["source"]["historySchema"] = json!("unknown");
    assert!(validate(&x, &|| Ok(())).is_err());
    let mut x = v.clone();
    x["players"][0]["data"]["values"] = json!([]);
    assert!(validate(&x, &|| Ok(())).is_err());
    let mut x = v.clone();
    x["players"][0]["data"]["info"]["fullNameId"] = json!(1);
    assert!(validate(&x, &|| Ok(())).is_err());
    assert!(
        parse(&serde_json::to_vec(&v).unwrap(), &|| anyhow::bail!(
            "cancelled"
        ))
        .is_err()
    );
    assert!(parse(&[0xff, 0xff], &|| Ok(())).is_err());
    assert!(
        parse(
            format!("{}0{}", "[".repeat(33), "]".repeat(33)).as_bytes(),
            &|| Ok(())
        )
        .is_err()
    );
}
#[test]
fn stats_only_selection_is_portable_and_preserves_season_values() {
    let mut v = fixture();
    let player = v["players"][0].clone();
    if player["history"].as_array().unwrap().is_empty() {
        return;
    }
    v["format"] = json!("2k23-season-catalog");
    let clean = validate(&v, &|| Ok(())).unwrap();
    let r=crate::player_catalog::browser_request_with_id(clean.value(),&json!({"action":"selection","catalogId":"fixture","playerId":player["id"],"sections":["seasons"],"seasonIds":[player["history"][0]],"includeAwards":true}),"fixture").unwrap();
    assert_eq!(r["data"]["seasonHistory"].as_array().unwrap().len(), 1);
    assert_eq!(r["data"]["fields"], json!({}));
    assert_eq!(r["data"]["info"], json!({}));
}

#[test]
fn entire_actual_bundled_catalog_passes_startup_boundary() {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("../assets/players.json.gz")[..])
        .read_to_end(&mut bytes)
        .unwrap();
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    let clean = parse(&bytes, &|| Ok(())).unwrap();
    for key in ["players", "records", "teams", "fieldKeys"] {
        assert_eq!(
            clean.value()[key].as_array().unwrap().len(),
            original[key].as_array().unwrap().len()
        );
    }
    assert_eq!(clean.value()["source"], original["source"]);
    let status = crate::player_catalog::browser_request_with_id(
        clean.value(),
        &json!({"action":"status"}),
        "actual-bundled",
    )
    .unwrap();
    assert_eq!(status["players"], 4274);
    assert_eq!(status["seasons"], 8605);
    assert_eq!(status["mappedFields"], 180);
}
