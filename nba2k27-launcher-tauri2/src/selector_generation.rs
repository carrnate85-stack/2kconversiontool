//! Pure WPF-equivalent selector planning/building. Never installs files, reads
//! game memory, picks an approximate donor or alters a user's existing bundle.
#![allow(dead_code)]
use crate::{
    selector_tables::{self, Definition, Prepared},
    sigconfig::{self, CloneRequest, Replacement, Table},
    stock_selectors::Manifest,
};
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    sync::OnceLock,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Base {
    pub path: String,
    pub info: Option<Value>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Group {
    pub suffixes: BTreeSet<String>,
    pub base_paths: Vec<String>,
    pub bases: Vec<Base>,
}
pub(crate) type Inputs = BTreeMap<u32, Group>;
pub(crate) fn inventory(paths: impl IntoIterator<Item = String>) -> Result<Inputs> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"(?i)^png([0-9]{4,5})(?:_config_([a-z0-9_-]+))?\.iff$").unwrap()
    });
    let mut groups = BTreeMap::new();
    for path in paths {
        let leaf = path.rsplit('/').next().unwrap_or(&path);
        if let Some(capture) = pattern.captures(leaf) {
            ensure!(
                groups.len() < 100_000,
                "Player config inventory exceeds bounds"
            );
            let group: &mut Group = groups.entry(capture[1].parse()?).or_default();
            if let Some(name) = capture.get(2) {
                group.suffixes.insert(name.as_str().to_ascii_lowercase());
            } else {
                group.base_paths.push(path);
            }
        }
    }
    groups.retain(|_, g| !g.suffixes.is_empty());
    ensure!(
        groups.len() <= 10_000,
        "More than 10,000 mod configuration sets"
    );
    for group in groups.values_mut() {
        group.base_paths.sort();
    }
    Ok(groups)
}
/// Only inspect appearance_info.json, not every mesh/texture in the player's
/// archive. Bounds and CRC still apply; nothing is extracted to a filesystem.
pub(crate) fn appearance(mut source: impl Read + std::io::Seek) -> Result<Value> {
    let mut zip = zip::ZipArchive::new(&mut source)?;
    ensure!(
        (1..=10_000).contains(&zip.len()),
        "Player archive member count exceeds bounds"
    );
    let mut found = None;
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        if entry
            .name_raw()
            .eq_ignore_ascii_case(b"appearance_info.json")
        {
            ensure!(found.replace(i).is_none(), "Duplicate appearance_info.json");
        }
    }
    let entry = zip.by_index(found.context("No appearance_info.json")?)?;
    ensure!(
        !entry.is_dir()
            && !entry.encrypted()
            && entry.size() <= 4 * 1024 * 1024
            && matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ),
        "Unsupported or oversized appearance_info.json"
    );
    let expected = entry.size();
    let mut bytes = Vec::new();
    entry.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == expected,
        "appearance_info.json size differs after CRC verification"
    );
    let document: Value = serde_json::from_slice(&bytes)?;
    let info = document
        .get("accessory_items")
        .context("Player archive has no accessory configuration definition")?;
    ensure!(
        info.is_object(),
        "Player accessory definition must be an object"
    );
    Ok(info.clone())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Eligible {
    pub id: u32,
    pub donor_id: u32,
    pub config_count: usize,
    pub suffixes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Warning {
    pub id: u32,
    pub reason: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Report {
    pub eligible: Vec<Eligible>,
    pub replacements: Vec<u32>,
    pub skipped: Vec<Warning>,
}
pub(crate) struct Output {
    pub prepared: Option<Prepared>,
    pub report: Report,
}
pub(crate) fn build(stock: &Manifest, sources: &[Vec<u8>; 3], mods: &Inputs) -> Result<Output> {
    ensure!(mods.len() <= 10_000, "Mod selector planning limit exceeded");
    let mut all_zip = selector_tables::archive(&sources[0])?;
    let mut edit_zip = selector_tables::archive(&sources[1])?;
    let all_source = selector_tables::required(&all_zip, selector_tables::MEMBERS[0])?.to_vec();
    let without_source =
        selector_tables::required(&edit_zip, selector_tables::MEMBERS[1])?.to_vec();
    let with_source = selector_tables::required(&edit_zip, selector_tables::MEMBERS[2])?.to_vec();
    let all = Table::parse(&all_source)?;
    let records: BTreeMap<_, _> = all.entries.iter().map(|r| (r.id, r)).collect();
    let edit_ids = [&without_source, &with_source]
        .into_iter()
        .map(|bytes| {
            Table::parse(bytes)
                .map(|t| t.entries.into_iter().map(|r| r.id).collect::<BTreeSet<_>>())
        })
        .collect::<Result<Vec<_>>>()?;
    let mut donors = BTreeMap::new();
    // Keep manifest order, like WPF's first matching stock style-set donor.
    for (id, suffixes) in &stock.configs {
        if (2..=9).contains(&suffixes.len())
            && records.get(id).is_some_and(|r| r.count == suffixes.len())
        {
            donors.entry(suffixes.clone()).or_insert(*id);
        }
    }
    let mut report = Report::default();
    let mut replacements: Vec<(u32, Definition)> = Vec::new();
    for (id, group) in mods {
        let reason = if stock.bases.contains(id) || records.contains_key(id) {
            if !group.base_paths.is_empty()
                && records.contains_key(id)
                && edit_ids.iter().all(|ids| ids.contains(id))
            {
                let definition = (|| -> Result<Definition> {
                    ensure!(
                        group.bases.len() == group.base_paths.len(),
                        "Player definitions were not fully inspected"
                    );
                    let mut result = None;
                    for base in &group.bases {
                        let info = base.info.as_ref().with_context(|| {
                            base.error
                                .clone()
                                .unwrap_or_else(|| "Missing player definition".into())
                        })?;
                        let current = selector_tables::definition(info, &group.suffixes)?;
                        ensure!(
                            result.as_ref().is_none_or(|previous| previous == &current),
                            "Conflicting duplicate player definitions"
                        );
                        result = Some(current);
                    }
                    result.context("No player definition")
                })();
                match definition {
                    Ok(definition) => {
                        replacements.push((*id, definition));
                        report.replacements.push(*id);
                        None
                    }
                    Err(_) => Some("replacement-layout-unverified"),
                }
            } else {
                Some("stock-player")
            }
        } else if group.base_paths.is_empty() {
            Some("base-png-missing")
        } else if group.suffixes.len() < 2 {
            Some("single-config")
        } else if group.suffixes.len() > 9 {
            Some("over-nine-config-limit")
        } else if let Some(donor_id) = donors.get(&group.suffixes).filter(|id| **id != 0) {
            report.eligible.push(Eligible {
                id: *id,
                donor_id: *donor_id,
                config_count: group.suffixes.len(),
                suffixes: group.suffixes.iter().cloned().collect(),
            });
            None
        } else {
            Some("no-exact-stock-style-set")
        };
        if let Some(reason) = reason {
            report.skipped.push(Warning {
                id: *id,
                reason: reason.into(),
            });
        }
    }
    if report.eligible.is_empty() && replacements.is_empty() {
        return Ok(Output {
            prepared: None,
            report,
        });
    }
    let clones: Vec<_> = report
        .eligible
        .iter()
        .map(|e| CloneRequest {
            target_id: e.id,
            donor_id: e.donor_id,
        })
        .collect();
    for (archive, source, member, which) in [
        (&mut all_zip, &all_source, selector_tables::MEMBERS[0], 0),
        (
            &mut edit_zip,
            &without_source,
            selector_tables::MEMBERS[1],
            1,
        ),
    ] {
        let replacement = replacements
            .iter()
            .map(|(id, d)| Replacement {
                id: *id,
                names: if which == 0 {
                    d.names.clone()
                } else {
                    d.without_headband.clone()
                },
            })
            .collect::<Vec<_>>();
        let generated =
            sigconfig::replace_lists(&sigconfig::clone_records(source, &clones)?, &replacement)?;
        let key = archive
            .keys()
            .find(|key| key.eq_ignore_ascii_case(member))
            .unwrap()
            .clone();
        archive.insert(key, generated);
    }
    let replacement = replacements
        .iter()
        .map(|(id, d)| Replacement {
            id: *id,
            names: d.with_headband.clone(),
        })
        .collect::<Vec<_>>();
    let generated = sigconfig::replace_lists(
        &sigconfig::clone_records(&with_source, &clones)?,
        &replacement,
    )?;
    let key = edit_zip
        .keys()
        .find(|key| key.eq_ignore_ascii_case(selector_tables::MEMBERS[2]))
        .unwrap()
        .clone();
    edit_zip.insert(key, generated);
    let registry = replacements
        .iter()
        .map(|(id, d)| (*id, d.accessory_items.clone()))
        .collect::<Vec<_>>();
    let mut prepared = Prepared::existing([
        selector_tables::pack(&all_zip)?,
        selector_tables::pack(&edit_zip)?,
        selector_tables::replace_registry(&sources[2], &registry)?,
    ])?;
    prepared.summary.source =
        "Native-generated selectors from installed stock tables and selected mod definitions"
            .into();
    prepared.summary.generated_players = report.eligible.len();
    prepared.summary.replacement_players = report.replacements.len();
    prepared.summary.skipped_players = report.skipped.len();
    prepared.summary.warnings = report.skipped.iter().take(12).cloned().collect();
    Ok(Output {
        prepared: Some(prepared),
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;
    fn json_equivalent(left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                a == b
                    || a.as_f64().zip(b.as_f64()).is_some_and(|(a, b)| {
                        a.is_finite()
                            && b.is_finite()
                            && a.abs() <= 9_007_199_254_740_991.0
                            && b.abs() <= 9_007_199_254_740_991.0
                            && a == b
                    })
            }
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_equivalent(a, b))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(key, a)| b.get(key).is_some_and(|b| json_equivalent(a, b)))
            }
            _ => left == right,
        }
    }
    #[test]
    #[ignore = "Explicit disk-only installed-stock/mod-folder audit; no publication or game process access"]
    fn read_only_full_library_generation_matches_existing_wpf_bundle() {
        use std::{
            fs,
            path::PathBuf,
            sync::{Arc, atomic::AtomicBool},
        };
        let game = PathBuf::from(
            std::env::var("NBA2K27_STOCK_SELECTOR_AUDIT_ROOT")
                .expect("Explicit game path required"),
        );
        let mods = PathBuf::from(
            std::env::var("NBA2K27_SELECTOR_AUDIT_ROOT").expect("Explicit mod path required"),
        );
        let started = std::time::Instant::now();
        let original: [Vec<u8>; 3] =
            selector_tables::PATHS.map(|name| fs::read(mods.join(name)).unwrap());
        let service = crate::provider_assets::Service::open(
            &mods,
            &[],
            &[],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let inputs = service.selector_inputs().unwrap();
        let manifest = Manifest::read(&game, || Ok(())).unwrap();
        let decoder = crate::stock_selectors::Decoder::load(&game).unwrap();
        let packed: [Vec<u8>; 3] =
            std::array::from_fn(|i| manifest.entries[i].packed(&game, || Ok(())).unwrap());
        let sources: [Vec<u8>; 3] =
            std::array::from_fn(|i| decoder.decode(&packed[i], || Ok(())).unwrap());
        let output = build(&manifest, &sources, &inputs).unwrap();
        let prepared = output
            .prepared
            .expect("This library must have compatible selectors");
        for i in 0..3 {
            let existing = selector_tables::archive(&original[i]).unwrap();
            let generated = selector_tables::archive(&prepared.bytes[i]).unwrap();
            assert_eq!(
                existing.keys().collect::<Vec<_>>(),
                generated.keys().collect::<Vec<_>>()
            );
            for (name, bytes) in &existing {
                let actual = &generated[name];
                if name != "fpplayer_global_table.json" && name.ends_with(".json") {
                    // WPF's JSON.stringify writes 0.0 as 0; these are the
                    // same JSON numeric value, not an appearance difference.
                    assert!(
                        json_equivalent(
                            &serde_json::from_slice::<Value>(bytes).unwrap(),
                            &serde_json::from_slice::<Value>(actual).unwrap()
                        ),
                        "Registry JSON differs: {name}"
                    );
                } else {
                    assert_eq!(bytes, actual, "{name}");
                }
            }
            assert_eq!(
                fs::read(mods.join(selector_tables::PATHS[i])).unwrap(),
                original[i]
            );
            assert_eq!(
                manifest.entries[i].packed(&game, || Ok(())).unwrap(),
                packed[i]
            );
        }
        println!(
            "{}",
            json!({"inputConfigurationSets":inputs.len(),"summary":prepared.summary,"report":output.report,"allThreeGeneratedArchivesMatchExistingWpfContents":true,"elapsedMs":started.elapsed().as_millis(),"gameProcessAccess":false,"gameFilesChanged":false,"modFilesChanged":false,"cacheFilesCreated":false})
        );
    }
    #[test]
    fn generated_plans_and_every_table_byte_match_preserved_wpf() {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../assets/selector-generation-regressions.json.gz")[..],
        )
        .take(16 * 1024 * 1024)
        .read_to_end(&mut decoded)
        .unwrap();
        let fixture: Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(fixture["schema"], "nba2k27-selector-generation-oracle-v1");
        assert_eq!(fixture["cases"].as_array().unwrap().len(), 15);
        let bytes = |v: &Value| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
        };
        for case in fixture["cases"].as_array().unwrap() {
            let stock = Manifest {
                entries: std::array::from_fn(|_| crate::stock_selectors::Resource {
                    archive: "0C".into(),
                    offset: 0,
                    size: 25,
                }),
                bases: serde_json::from_value(case["stock"]["bases"].clone()).unwrap(),
                configs: serde_json::from_value(case["stock"]["configs"].clone()).unwrap(),
            };
            let sources: [Vec<u8>; 3] = case["sources"]
                .as_array()
                .unwrap()
                .iter()
                .map(&bytes)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            let inputs: Inputs = serde_json::from_value(case["inputs"].clone()).unwrap();
            let result = build(&stock, &sources, &inputs).unwrap();
            assert_eq!(
                serde_json::to_value(&result.report.eligible).unwrap(),
                case["expected"]["eligible"],
                "{}",
                case["name"]
            );
            assert_eq!(
                serde_json::to_value(&result.report.replacements).unwrap(),
                case["expected"]["replacements"],
                "{}",
                case["name"]
            );
            assert_eq!(
                serde_json::to_value(&result.report.skipped).unwrap(),
                case["expected"]["skipped"],
                "{}",
                case["name"]
            );
            if case["expected"]["output"].is_null() {
                assert!(result.prepared.is_none());
                continue;
            }
            let actual = result.prepared.unwrap();
            for (which, member, expected) in [
                (0, selector_tables::MEMBERS[0], 0),
                (1, selector_tables::MEMBERS[1], 1),
                (1, selector_tables::MEMBERS[2], 2),
            ] {
                let archive = selector_tables::archive(&actual.bytes[which]).unwrap();
                assert_eq!(
                    selector_tables::required(&archive, member).unwrap(),
                    bytes(&case["expected"]["output"]["tables"][expected]),
                    "{} {member}",
                    case["name"]
                );
            }
            let actual_registry = selector_tables::archive(&actual.bytes[2]).unwrap();
            for row in case["expected"]["output"]["registry"].as_array().unwrap() {
                let name = row["name"].as_str().unwrap();
                let expected = bytes(&row["bytes"]);
                if name == "fpplayer_global_table.json" {
                    assert_eq!(actual_registry[name], expected);
                } else {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&actual_registry[name]).unwrap(),
                        serde_json::from_slice::<Value>(&expected).unwrap(),
                        "{} {name}",
                        case["name"]
                    );
                }
            }
        }
    }
    #[test]
    fn recursive_inventory_matches_reference_filename_rules_and_does_not_invent_bases() {
        let paths = [
            "char/sig/png10000.iff",
            "players/Test/png10000_config_AFRO.iff",
            "char/sig/pkg/png10000_config_headband.iff",
            "char/sig/png123_config_bad.iff",
            "char/sig/png10001_config_orphan.iff",
            "char/sig/random.txt",
        ];
        let inputs = inventory(paths.into_iter().map(str::to_owned)).unwrap();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[&10000].base_paths, ["char/sig/png10000.iff"]);
        assert_eq!(
            inputs[&10000].suffixes,
            BTreeSet::from(["afro".into(), "headband".into()])
        );
        assert!(inputs[&10001].base_paths.is_empty());
    }
    #[test]
    fn appearance_inspection_reads_only_the_exact_bounded_crc_checked_member() {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "appearance_info.json",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer
            .write_all(br#"{"accessory_items":{"configurations":[{"name":"AFRO"}]}}"#)
            .unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        assert_eq!(
            appearance(Cursor::new(bytes.clone())).unwrap(),
            json!({"configurations":[{"name":"AFRO"}]})
        );
        assert!(appearance(Cursor::new(b"not a zip")).is_err());
        let mut bad = bytes;
        let at = bad.windows(4).position(|s| s == b"AFRO").unwrap();
        bad[at] ^= 1;
        assert!(appearance(Cursor::new(bad)).is_err());
    }
}
