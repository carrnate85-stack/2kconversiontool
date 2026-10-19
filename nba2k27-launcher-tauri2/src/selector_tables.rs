//! Bounded IFF/ZIP selector data. Archives remain in memory; there is no ZIP
//! extraction, arbitrary stock override, filesystem installation or native code.
#![allow(dead_code)]
use crate::sigconfig::{self, Table};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub(crate) const PATHS: [&str; 3] = [
    "char/signatureconfigs_all.iff",
    "char/signatureconfigs_editplayer.iff",
    "char/fpplayer_global.iff",
];
pub(crate) const MEMBERS: [&str; 3] = [
    "All.SigConfigTable",
    "EditPlayer_WithoutHeadband.SigConfigTable",
    "EditPlayer_WithHeadband.SigConfigTable",
];
const MAX_EXPANDED: usize = 128 * 1024 * 1024;
type Archive = BTreeMap<String, Vec<u8>>;

fn member_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name.is_ascii()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(&b))
        && name
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}
pub(crate) fn archive(source: &[u8]) -> Result<Archive> {
    ensure!(
        (22..=sigconfig::MAX_BYTES).contains(&source.len()),
        "Player selector archive size is invalid"
    );
    let mut zip =
        ZipArchive::new(Cursor::new(source)).context("Invalid player selector ZIP/IFF")?;
    ensure!(
        (1..=10_000).contains(&zip.len()),
        "Player selector member count is invalid"
    );
    let mut expanded = 0usize;
    let mut names = BTreeSet::new();
    // Check every declared size before allocating or inflating any member.
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name =
            std::str::from_utf8(entry.name_raw()).context("Non-UTF8 player selector member")?;
        ensure!(
            member_name(name)
                && !entry.is_dir()
                && entry.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000)
                && matches!(
                    entry.compression(),
                    CompressionMethod::Stored | CompressionMethod::Deflated
                )
                && names.insert(name.to_ascii_lowercase()),
            "Unsafe, duplicate or unsupported player selector member"
        );
        let size = usize::try_from(entry.size()).context("Selector member size exceeds bounds")?;
        ensure!(
            size <= sigconfig::MAX_BYTES,
            "Player selector member is oversized"
        );
        expanded = expanded
            .checked_add(size)
            .context("Player selector expanded size overflow")?;
        ensure!(
            expanded <= MAX_EXPANDED,
            "Player selector expanded content is oversized"
        );
    }
    let mut entries = BTreeMap::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = std::str::from_utf8(entry.name_raw())?.to_owned();
        let expected = entry.size() as usize;
        let mut data = Vec::with_capacity(expected);
        entry
            .take((sigconfig::MAX_BYTES + 1) as u64)
            .read_to_end(&mut data)
            .with_context(|| format!("Player selector CRC/decompression failed: {name}"))?;
        ensure!(
            data.len() == expected,
            "Player selector member size differs after decompression"
        );
        entries.insert(name, data);
    }
    Ok(entries)
}
/// WPF `archiveContentFingerprint`: every member's name, length and SHA-256,
/// in name order. Two selector archives with equal fingerprints are the same
/// bundle even if their ZIP containers were written differently.
pub(crate) fn content_fingerprint(source: &[u8]) -> Result<Vec<(String, usize, String)>> {
    Ok(archive(source)?
        .into_iter()
        .map(|(name, data)| (name, data.len(), crate::loader_files::digest(&data)))
        .collect())
}
pub(crate) fn archives_equivalent(left: &[u8], right: &[u8]) -> bool {
    match (content_fingerprint(left), content_fingerprint(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
pub(crate) fn pack(entries: &Archive) -> Result<Vec<u8>> {
    ensure!(
        (1..=10_000).contains(&entries.len()),
        "Invalid selector output member count"
    );
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        ensure!(
            member_name(name) && names.insert(name.to_ascii_lowercase()),
            "Invalid selector output member name"
        );
        total = total
            .checked_add(data.len())
            .context("Selector output overflow")?;
        ensure!(
            data.len() <= sigconfig::MAX_BYTES && total <= MAX_EXPANDED,
            "Selector output is oversized"
        );
        writer.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )?;
        writer.write_all(data)?;
    }
    let output = writer.finish()?.into_inner();
    ensure!(
        archive(&output)? == *entries,
        "Generated selector archive read-back differs"
    );
    Ok(output)
}
pub(crate) fn required<'a>(archive: &'a Archive, name: &str) -> Result<&'a [u8]> {
    archive
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, data)| data.as_slice())
        .with_context(|| format!("Missing player selector member: {name}"))
}
fn ids(table: &Table) -> BTreeSet<u32> {
    table.entries.iter().map(|e| e.id).collect()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Summary {
    pub enabled: bool,
    pub players: usize,
    pub source: String,
    pub sha256: BTreeMap<String, String>,
    #[serde(default)]
    pub generated_players: usize,
    #[serde(default)]
    pub replacement_players: usize,
    #[serde(default)]
    pub skipped_players: usize,
    #[serde(default)]
    pub warnings: Vec<crate::selector_generation::Warning>,
}
impl Summary {
    pub fn verify_loader_report(&self, report: &str) -> Result<()> {
        let values: Vec<_> = report
            .lines()
            .filter_map(|line| line.strip_prefix("LIVE_SELECTOR_OVERRIDE="))
            .collect();
        ensure!(
            values == [if self.enabled { "ON" } else { "OFF" }],
            "Native loader did not confirm this session's player selector mode"
        );
        Ok(())
    }
    pub fn absent() -> Self {
        Self {
            source: "No prepared selector bundle in selected mod folder".into(),
            ..Default::default()
        }
    }
}
pub(crate) struct Prepared {
    pub summary: Summary,
    pub bytes: [Vec<u8>; 3],
}
impl Prepared {
    pub fn existing(bytes: [Vec<u8>; 3]) -> Result<Self> {
        let all = archive(&bytes[0])?;
        let edit = archive(&bytes[1])?;
        let registry = archive(&bytes[2])?;
        let all_table = Table::parse(required(&all, MEMBERS[0])?)?;
        let without = Table::parse(required(&edit, MEMBERS[1])?)?;
        let with = Table::parse(required(&edit, MEMBERS[2])?)?;
        let players = ids(&all_table);
        ensure!(
            players == ids(&without) && players == ids(&with),
            "Player selector archives describe different player sets"
        );
        // The game's global_table.json is its own object-list syntax, NOT JSON.
        // Preserve it byte-for-byte rather than attempting a JSON round trip.
        ensure!(
            !required(&registry, "fpplayer_global_table.json")?.is_empty(),
            "Empty player render registry"
        );
        let mut player_members = 0;
        for (name, data) in &registry {
            if let Some(id) = name.strip_suffix("_modified.json") {
                ensure!(
                    id.len() == 5 && id.bytes().all(|b| b.is_ascii_digit()),
                    "Invalid player render registry member ID"
                );
                ensure!(
                    players.contains(&id.parse::<u32>()?),
                    "Player render registry has no matching selector record"
                );
                let document: Value =
                    serde_json::from_slice(data).context("Invalid player render registry JSON")?;
                ensure!(
                    document.is_object(),
                    "Player render registry member must be an object"
                );
                player_members += 1;
            }
        }
        ensure!(
            player_members > 0,
            "Player render registry contains no players"
        );
        let sha256 = PATHS
            .iter()
            .zip(&bytes)
            .map(|(name, data)| ((*name).to_owned(), format!("{:X}", Sha256::digest(data))))
            .collect();
        Ok(Self {
            summary: Summary {
                enabled: true,
                players: players.len(),
                source: "Validated selected mod folder; session-frozen memory copy".into(),
                sha256,
                ..Default::default()
            },
            bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Definition {
    pub names: Vec<String>,
    pub without_headband: Vec<String>,
    pub with_headband: Vec<String>,
    pub accessory_items: Value,
}
fn config_names(info: &Value) -> Result<Vec<String>> {
    let configurations = info["configurations"]
        .as_array()
        .context("Player configurations are missing")?;
    ensure!(
        (1..=9).contains(&configurations.len()),
        "Player configuration count must be one to nine"
    );
    let names: Vec<String> = configurations
        .iter()
        .map(|c| {
            let name = c["name"]
                .as_str()
                .context("Player configuration name is missing")?;
            sigconfig::name_hash(name)?;
            Ok(name.to_owned())
        })
        .collect::<Result<_>>()?;
    ensure!(
        names
            .iter()
            .map(|n| n.to_ascii_uppercase())
            .collect::<BTreeSet<_>>()
            .len()
            == names.len(),
        "Duplicate player configuration names"
    );
    Ok(names)
}
fn optional_array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(values) => values
            .as_array()
            .map(Vec::as_slice)
            .with_context(|| format!("Player {key} must be an array")),
    }
}
fn optional_name<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(
            value
                .as_str()
                .with_context(|| format!("Invalid player {key}"))?,
        )),
    }
}
pub(crate) fn definition(info: &Value, available: &BTreeSet<String>) -> Result<Definition> {
    let raw_names = config_names(info)?;
    let names: Vec<_> = raw_names.iter().map(|n| n.to_ascii_uppercase()).collect();
    ensure!(
        names
            .iter()
            .all(|n| available.contains(&n.to_ascii_lowercase())),
        "Player definition references a missing configuration file"
    );
    for key in ["default_config", "default_out_of_uniform_config"] {
        if let Some(value) = optional_name(info, key)? {
            ensure!(
                value.is_empty() || names.contains(&value.to_ascii_uppercase()),
                "Player default configuration is undeclared"
            );
        }
    }
    let items = optional_array(info, "items")?;
    for configuration in info["configurations"].as_array().unwrap() {
        optional_array(configuration, "items")?;
    }
    let headband_items: BTreeSet<_> = items
        .iter()
        .filter(|item| {
            format!(
                "{} {}",
                item["type"].as_str().unwrap_or(""),
                item["name"].as_str().unwrap_or("")
            )
            .to_ascii_lowercase()
            .contains("headband")
        })
        .filter_map(|item| item["name"].as_str().map(str::to_ascii_lowercase))
        .collect();
    let headband: Vec<_> = info["configurations"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&names)
        .filter(|(config, name)| {
            name.to_ascii_lowercase().contains("headband")
                || config["items"].as_array().is_some_and(|items| {
                    items.iter().any(|i| {
                        i.as_str()
                            .is_some_and(|s| headband_items.contains(&s.to_ascii_lowercase()))
                    })
                })
        })
        .map(|(_, name)| name.clone())
        .collect();
    let ordinary: Vec<_> = names
        .iter()
        .filter(|n| !headband.contains(n))
        .cloned()
        .collect();
    let default = info["default_config"]
        .as_str()
        .unwrap_or("")
        .to_ascii_uppercase();
    let base = if ordinary.contains(&default) {
        default
    } else {
        ordinary.first().unwrap_or(&names[0]).clone()
    };
    let list = |entries: &[String]| {
        std::iter::once(base.clone())
            .chain(entries.iter().filter(|n| **n != base).cloned())
            .collect()
    };
    Ok(Definition {
        names,
        without_headband: list(&ordinary),
        with_headband: list(&headband),
        accessory_items: info.clone(),
    })
}
pub(crate) fn replace_registry(source: &[u8], replacements: &[(u32, Value)]) -> Result<Vec<u8>> {
    let mut entries = archive(source)?;
    let mut seen = BTreeSet::new();
    for (id, info) in replacements {
        ensure!(
            *id <= 99999 && seen.insert(*id),
            "Invalid or duplicate registry player ID"
        );
        let member = format!("{id:05}_modified.json");
        let original = entries
            .get(&member)
            .context("Missing stock player render registry member")?;
        let names = config_names(info)?;
        for key in ["default_config", "default_out_of_uniform_config"] {
            if let Some(value) = optional_name(info, key)? {
                ensure!(
                    value.is_empty() || names.contains(&value.to_owned()),
                    "Registry default configuration is undeclared"
                );
            }
        }
        let items = optional_array(info, "items")?;
        let item_names: BTreeSet<_> = items
            .iter()
            .filter_map(|item| item["name"].as_str())
            .collect();
        for configuration in info["configurations"].as_array().unwrap() {
            let values = optional_array(configuration, "items")?;
            ensure!(
                values
                    .iter()
                    .all(|v| v.as_str().is_some_and(|s| item_names.contains(s))),
                "Registry configuration references an undeclared item"
            );
            if let Some(fallback) = optional_name(configuration, "fallback")? {
                ensure!(
                    fallback.is_empty() || names.contains(&fallback.to_owned()),
                    "Registry fallback configuration is undeclared"
                );
            }
        }
        let mut document: Value = serde_json::from_slice(original)?;
        let object = document
            .as_object_mut()
            .context("Player render registry member must be an object")?;
        object.insert("accessory_items".into(), info.clone());
        entries.insert(member, serde_json::to_vec(&document)?);
    }
    pack(&entries)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    pub(crate) fn bundle() -> [Vec<u8>; 3] {
        let mut table = vec![0; 64];
        for (offset, value) in [(0, 1u32), (8, 9), (16, 4090), (20, 1)] {
            table[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        table[24..32].copy_from_slice(&9u64.to_le_bytes());
        table[32..40].copy_from_slice(&17u64.to_le_bytes());
        let make = |pairs: Vec<(&str, Vec<u8>)>| {
            pack(&pairs.into_iter().map(|(n, v)| (n.to_owned(), v)).collect()).unwrap()
        };
        [
            make(vec![(MEMBERS[0], table.clone())]),
            make(vec![(MEMBERS[1], table.clone()), (MEMBERS[2], table)]),
            make(vec![
                (
                    "fpplayer_global_table.json",
                    b"{ { \"id\": \"04090\" } }".to_vec(),
                ),
                (
                    "04090_modified.json",
                    br#"{"appearance":{"skin color":"4"},"other":1}"#.to_vec(),
                ),
            ]),
        ]
    }
    #[test]
    fn prepares_coordinated_archives_and_preserves_non_json_game_index() {
        let bytes = bundle();
        let prepared = Prepared::existing(bytes.clone()).unwrap();
        assert!(prepared.summary.enabled);
        assert_eq!(prepared.summary.players, 1);
        assert_eq!(prepared.bytes, bytes);
        assert_eq!(prepared.summary.sha256.len(), 3);
        assert!(!Summary::absent().enabled);
    }
    #[test]
    fn loader_must_confirm_the_same_selector_mode_before_active_status() {
        let prepared = Prepared::existing(bundle()).unwrap().summary;
        assert!(
            prepared
                .verify_loader_report("LIVE_SESSION_ACTIVE=YES\r\nLIVE_SELECTOR_OVERRIDE=ON\r\n")
                .is_ok()
        );
        for report in [
            "",
            "LIVE_SELECTOR_OVERRIDE=OFF\n",
            "LIVE_SELECTOR_OVERRIDE=ON\nLIVE_SELECTOR_OVERRIDE=OFF\n",
            "LIVE_SELECTOR_OVERRIDE=ON\nLIVE_SELECTOR_OVERRIDE=ON\n",
        ] {
            assert!(prepared.verify_loader_report(report).is_err());
        }
        assert!(
            Summary::absent()
                .verify_loader_report("LIVE_SELECTOR_OVERRIDE=OFF\r\n")
                .is_ok()
        );
        assert!(
            Summary::absent()
                .verify_loader_report("LIVE_SELECTOR_OVERRIDE=ON\n")
                .is_err()
        );
    }
    #[test]
    fn refuses_mismatched_ids_and_required_members() {
        let mut bytes = bundle();
        let mut edit = archive(&bytes[1]).unwrap();
        edit.get_mut(MEMBERS[1]).unwrap()[16..20].copy_from_slice(&42u32.to_le_bytes());
        bytes[1] = pack(&edit).unwrap();
        assert!(Prepared::existing(bytes).is_err());
        let mut bytes = bundle();
        bytes[0] = pack(&BTreeMap::from([("wrong.bin".into(), vec![1])])).unwrap();
        assert!(Prepared::existing(bytes).is_err());
    }
    #[test]
    fn rejects_archive_traversal_duplicates_crc_corruption_and_inflation() {
        for names in [
            vec!["../outside"],
            vec!["/absolute"],
            vec!["one", "ONE"],
            vec!["C:/absolute"],
            vec!["one\\two"],
        ] {
            let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
            for name in names {
                zip.start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
                )
                .unwrap();
                zip.write_all(b"test").unwrap();
            }
            assert!(archive(&zip.finish().unwrap().into_inner()).is_err());
        }
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("large", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&vec![0; sigconfig::MAX_BYTES + 1]).unwrap();
        assert!(archive(&zip.finish().unwrap().into_inner()).is_err());
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            "test",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(b"unique-content").unwrap();
        let mut corrupt = zip.finish().unwrap().into_inner();
        let index = corrupt
            .windows(14)
            .position(|s| s == b"unique-content")
            .unwrap();
        corrupt[index] ^= 1;
        assert!(archive(&corrupt).is_err());
    }
    #[test]
    fn headband_partition_has_only_one_ordinary_base_entry() {
        let info = json!({"default_config":"SHORT", "default_out_of_uniform_config":"AFRO", "items":[{"name":"band", "type":"headband"}], "configurations":[{"name":"AFRO"},{"name":"SHORT"},{"name":"SPECIAL", "items":["band"]},{"name":"HEADBAND_FURY"}]});
        let available = ["afro", "short", "special", "headband_fury"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let result = definition(&info, &available).unwrap();
        assert_eq!(result.without_headband, ["SHORT", "AFRO"]);
        assert_eq!(result.with_headband, ["SHORT", "SPECIAL", "HEADBAND_FURY"]);
        assert!(definition(&info, &BTreeSet::new()).is_err());
    }
    #[test]
    fn registry_replacements_only_change_one_accessory_definition() {
        let source = bundle()[2].clone();
        let info = json!({"default_config":"AFRO", "items":[{"name":"band", "type":"headband"}], "configurations":[{"name":"AFRO"},{"name":"HEADBAND", "items":["band"], "fallback":"AFRO"}]});
        let output = replace_registry(&source, &[(4090, info.clone())]).unwrap();
        let before = archive(&source).unwrap();
        let after = archive(&output).unwrap();
        assert_eq!(
            before["fpplayer_global_table.json"],
            after["fpplayer_global_table.json"]
        );
        let doc: Value = serde_json::from_slice(&after["04090_modified.json"]).unwrap();
        assert_eq!(doc["other"], 1);
        assert_eq!(doc["appearance"]["skin color"], "4");
        assert_eq!(doc["accessory_items"], info);
        assert!(replace_registry(&source, &[(4090, info.clone()), (4090, info.clone())]).is_err());
        assert!(replace_registry(&source, &[(12345, info.clone())]).is_err());
        let mut bad = info.clone();
        bad["configurations"][1]["items"] = json!(["undeclared"]);
        assert!(replace_registry(&source, &[(4090, bad)]).is_err());
        let mut bad = info;
        bad["configurations"][1]["fallback"] = json!("missing");
        assert!(replace_registry(&source, &[(4090, bad)]).is_err());
    }
    #[test]
    fn malformed_optional_item_arrays_are_not_silently_discarded() {
        let source = bundle()[2].clone();
        let good = json!({"configurations":[{"name":"AFRO"}]});
        let available = BTreeSet::from(["afro".to_owned()]);
        for invalid in [json!({}), json!("not an item list"), json!(1)] {
            let mut bad = good.clone();
            bad["items"] = invalid.clone();
            assert!(definition(&bad, &available).is_err());
            assert!(replace_registry(&source, &[(4090, bad)]).is_err());
            let mut bad = good.clone();
            bad["configurations"][0]["items"] = invalid;
            assert!(definition(&bad, &available).is_err());
            assert!(replace_registry(&source, &[(4090, bad)]).is_err());
        }
        assert!(definition(&good, &available).is_ok());
        assert!(replace_registry(&source, &[(4090, good)]).is_ok());
        let nulls = json!({"items":null, "default_config":null, "default_out_of_uniform_config":null, "configurations":[{"name":"AFRO", "items":null, "fallback":null}]});
        assert!(definition(&nulls, &available).is_ok());
        assert!(replace_registry(&source, &[(4090, nulls)]).is_ok());
    }
}
