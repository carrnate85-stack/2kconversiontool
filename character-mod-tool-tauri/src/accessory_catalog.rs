use anyhow::{Result, ensure};
use nba2k_assets::{catalog::Catalog, codec};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::atomic::AtomicBool,
};

fn numeric_id(value: &str) -> &str {
    let trimmed = value.trim_start_matches('0');
    if trimmed.is_empty() { "0" } else { trimmed }
}

pub fn build(catalog: &Catalog, names: &Value, kind: &str, cancel: &AtomicBool) -> Result<Value> {
    ensure!(
        matches!(kind, "hair" | "facialhair"),
        "Asset type must be hair or facialhair."
    );
    codec::cancelled(cancel)?;
    let geometry = Regex::new(r"(?i)^png([0-9]+)_geo_((?:hair|facialhair)(?:_.+)?)\.iff$")?;
    let player = Regex::new(r"(?i)^char/sig/png([0-9]+)\.iff$")?;
    let config = Regex::new(r"(?i)^char/sig/png([0-9]+)_config_.+\.iff$")?;
    let mut entries = BTreeMap::new();
    for (i, entry) in catalog.entries.iter().enumerate() {
        if i % 1024 == 0 {
            codec::cancelled(cancel)?;
        }
        if entry
            .name
            .get(..9)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("char/sig/"))
        {
            entries.insert(entry.name.to_ascii_lowercase(), entry);
        }
    }
    let mut players = HashSet::new();
    let mut configs = HashMap::<&str, Vec<&str>>::new();
    for (name, entry) in &entries {
        if let Some(captures) = player.captures(&entry.name) {
            players.insert(captures.get(1).unwrap().as_str());
        }
        if let Some(captures) = config.captures(name) {
            configs
                .entry(captures.get(1).unwrap().as_str())
                .or_default()
                .push(name);
        }
    }
    let mut assets = Vec::new();
    for (i, entry) in entries.values().enumerate() {
        if i % 256 == 0 {
            codec::cancelled(cancel)?;
        }
        let Some(captures) = geometry.captures(entry.name.rsplit('/').next().unwrap()) else {
            continue;
        };
        let png = captures.get(1).unwrap().as_str();
        let encoded_key = captures.get(2).unwrap().as_str();
        let lower = encoded_key.to_ascii_lowercase();
        if !(lower == kind || lower.starts_with(&format!("{kind}_"))) {
            continue;
        }
        let tangent = lower.ends_with("_tangentspace");
        if kind == "facialhair" && !tangent {
            continue;
        }
        let key = if tangent {
            &encoded_key[..encoded_key.len() - "_tangentspace".len()]
        } else {
            encoded_key
        };
        let suffix = key
            .strip_prefix("hair_")
            .unwrap_or(key)
            .to_ascii_lowercase();
        let has_config = kind == "hair"
            && configs.get(png).is_some_and(|rows| {
                rows.len() == 1 || rows.iter().any(|name| name.contains(&suffix))
            });
        let name = names[numeric_id(png)]
            .as_str()
            .filter(|name| !name.is_empty())
            .unwrap_or(if players.contains(png) {
                "Unknown Player"
            } else {
                "Unknown"
            });
        assets.push(json!({
            "id":entry.name, "sourcePng":png, "playerName":name, "key":key,
            "hasItem":entries.contains_key(&format!("char/sig/png{png}_item_{key}.iff").to_ascii_lowercase()),
            "hasConfig":has_config, "tangentspace":tangent, "origin":"archive"
        }));
    }
    assets.sort_by(|a, b| {
        let (a_png, b_png) = (
            numeric_id(a["sourcePng"].as_str().unwrap()),
            numeric_id(b["sourcePng"].as_str().unwrap()),
        );
        a_png
            .len()
            .cmp(&b_png.len())
            .then(a_png.cmp(b_png))
            .then_with(|| {
                a["key"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .cmp(&b["key"].as_str().unwrap().to_lowercase())
            })
            .then_with(|| {
                a["id"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .cmp(&b["id"].as_str().unwrap().to_lowercase())
            })
    });
    codec::cancelled(cancel)?;
    // Geometry descriptors are prepared only for the selected asset, not every list row.
    Ok(json!({"assetType":kind, "gameRoot":catalog.root, "assets":assets, "native":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nba2k_assets::catalog::Resource;
    use std::sync::atomic::Ordering;

    fn catalog(names: &[&str]) -> Catalog {
        Catalog {
            root: "C:/catalog-fixture".into(),
            sorted: Vec::new(),
            folders: BTreeMap::new(),
            entries: names
                .iter()
                .enumerate()
                .map(|(id, name)| Resource {
                    id,
                    name: (*name).into(),
                    archive: "0A".into(),
                    offset: id as u64,
                    size: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn lists_authored_modes_items_names_and_configs_without_eager_descriptors() {
        let catalog = catalog(&[
            "char/sig/png0002.iff",
            "char/sig/png0002_geo_hair_test.iff",
            "char/sig/png0002_geo_hair_test_tangentspace.iff",
            "char/sig/png0002_item_hair_test.iff",
            "char/sig/png0002_config_test.iff",
            "char/sig/png0002_config_other.iff",
            "char/sig/png0002_geo_facialhair_beard.iff",
            "char/sig/png0002_geo_facialhair_beard_tangentspace.iff",
            "char/sig/png0002_item_facialhair_beard.iff",
        ]);
        let names = json!({"2":"Sample Player"});
        let hair = build(&catalog, &names, "hair", &AtomicBool::new(false)).unwrap();
        let rows = hair["assets"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["playerName"], "Sample Player");
        assert_eq!(rows[0]["sourcePng"], "0002");
        assert_eq!(rows[0]["hasItem"], true);
        assert_eq!(rows[0]["hasConfig"], true);
        assert_eq!(rows[0]["tangentspace"], false);
        assert_eq!(rows[1]["tangentspace"], true);
        assert!(hair.get("previewDescriptors").is_none());
        let beard = build(&catalog, &names, "facialhair", &AtomicBool::new(false)).unwrap();
        assert_eq!(beard["assets"].as_array().unwrap().len(), 1);
        assert_eq!(beard["assets"][0]["key"], "facialhair_beard");
        assert_eq!(beard["assets"][0]["hasItem"], true);
        assert_eq!(beard["assets"][0]["hasConfig"], false);
    }

    #[test]
    fn missing_names_deduplication_case_and_large_ids_are_supported() {
        let catalog = catalog(&[
            "char/sig/png10_geo_hair_test.iff",
            "char/sig/png10_geo_hair_test.iff",
            "char/sig/png0002.iff",
            "char/sig/png0002_geo_hair_test.iff",
            "char/sig/png999999999999999999999_geo_hair_test.iff",
            "CHAR/SIG/png3_geo_hair_test.iff",
            "char/sig/png3_ITEM_HAIR_TEST.iff",
            "char/sig/png3_config_unrelated.iff",
        ]);
        let rows =
            build(&catalog, &Value::Null, "hair", &AtomicBool::new(false)).unwrap()["assets"]
                .clone();
        assert_eq!(rows.as_array().unwrap().len(), 4);
        assert_eq!(rows[0]["sourcePng"], "0002");
        assert_eq!(rows[0]["playerName"], "Unknown Player");
        assert_eq!(rows[1]["sourcePng"], "3");
        assert_eq!(rows[1]["hasItem"], true);
        assert_eq!(rows[1]["hasConfig"], true);
        assert_eq!(rows[2]["sourcePng"], "10");
        assert_eq!(rows[2]["playerName"], "Unknown");
        assert_eq!(rows[3]["sourcePng"], "999999999999999999999");
    }

    #[test]
    fn validation_and_cancellation_do_not_publish_partial_lists() {
        let catalog = catalog(&["char/sig/png1_geo_hair_test.iff"]);
        let cancel = AtomicBool::new(false);
        assert!(build(&catalog, &Value::Null, "unknown", &cancel).is_err());
        cancel.store(true, Ordering::SeqCst);
        assert!(
            build(&catalog, &Value::Null, "hair", &cancel)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }
}
