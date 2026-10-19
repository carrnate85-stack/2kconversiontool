//! Data-only signature configuration tables, ported from the WPF launcher.
//! Private cloned lists never modify stock/shared payload storage. No I/O or
//! game-memory access belongs here; the caller owns publication and lifetime.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MAX_BYTES: usize = 16 * 1024 * 1024;
const HEADER: usize = 16;
const RECORD: usize = 16;

fn span(bytes: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    let end = at
        .checked_add(size)
        .context("Signature table range overflow")?;
    bytes
        .get(at..end)
        .context("Truncated signature configuration table")
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(span(bytes, at, 4)?.try_into()?))
}
fn u64_at(bytes: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(span(bytes, at, 8)?.try_into()?))
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn align16(value: usize) -> Result<usize> {
    Ok(value
        .checked_add(15)
        .context("Signature alignment overflow")?
        & !15)
}
fn target(bytes: &[u8], field: usize) -> Result<usize> {
    let relative = u64_at(bytes, field)?;
    ensure!(relative > 0, "Null signature configuration pointer");
    usize::try_from(
        (field as u64)
            .checked_add(relative - 1)
            .context("Signature pointer overflow")?,
    )
    .context("Signature pointer exceeds addressable file range")
}
fn relative(field: usize, to: usize) -> Result<u64> {
    ensure!(
        to >= field,
        "Signature configuration pointer must be forward-relative"
    );
    Ok((to - field) as u64 + 1)
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub id: u32,
    pub count: usize,
    pub offset: usize,
    pub list: usize,
}
#[derive(Clone, Debug)]
pub(crate) struct Table {
    pub maximum: usize,
    pub records_end: usize,
    pub entries: Vec<Entry>,
}
impl Table {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            (HEADER..=MAX_BYTES).contains(&bytes.len()),
            "Signature table size is invalid"
        );
        let count = u32_at(bytes, 0)? as usize;
        let maximum = u32_at(bytes, 8)? as usize;
        ensure!(
            (1..=100_000).contains(&count) && (1..=64).contains(&maximum),
            "Signature table header is invalid"
        );
        let records_end = HEADER + count * RECORD;
        span(bytes, 0, records_end)?;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let offset = HEADER + i * RECORD;
            let id = u32_at(bytes, offset)?;
            ensure!(
                entries.last().is_none_or(|e: &Entry| e.id < id),
                "Signature configuration IDs are not strictly sorted"
            );
            let count = u32_at(bytes, offset + 4)? as usize;
            // The stock All table contains a few 12/14-entry records despite
            // maximumConfigs=9. Preserve those; the UI/clone limit stays nine.
            ensure!(
                (1..=64).contains(&count),
                "Signature record configuration count is invalid"
            );
            let list = target(bytes, offset + 8)?;
            ensure!(
                list >= records_end,
                "Signature list points into the record table"
            );
            span(bytes, list, count * RECORD)?;
            for index in 0..count {
                let payload = target(bytes, list + index * RECORD)?;
                ensure!(
                    payload >= records_end,
                    "Signature payload points into the record table"
                );
                span(bytes, payload, 4)?;
            }
            entries.push(Entry {
                id,
                count,
                offset,
                list,
            });
        }
        Ok(Self {
            maximum,
            records_end,
            entries,
        })
    }
    pub fn hashes(&self, bytes: &[u8], entry: &Entry) -> Result<Vec<u32>> {
        (0..entry.count)
            .map(|i| u32_at(bytes, target(bytes, entry.list + i * RECORD)?))
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CloneRequest {
    pub target_id: u32,
    pub donor_id: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Replacement {
    pub id: u32,
    pub names: Vec<String>,
}

pub(crate) fn clone_records(source: &[u8], clones: &[CloneRequest]) -> Result<Vec<u8>> {
    let parsed = Table::parse(source)?;
    if clones.is_empty() {
        return Ok(source.to_vec());
    }
    ensure!(
        clones.len() <= 100_000 - parsed.entries.len(),
        "Too many cloned signature records"
    );
    let original: BTreeMap<_, _> = parsed.entries.iter().map(|e| (e.id, e)).collect();
    let mut requested = BTreeMap::new();
    for item in clones {
        ensure!(
            item.target_id <= 99999
                && !original.contains_key(&item.target_id)
                && requested.insert(item.target_id, item.donor_id).is_none(),
            "Duplicate, existing or invalid cloned player ID"
        );
        let donor = original
            .get(&item.donor_id)
            .context("Signature configuration donor is missing")?;
        ensure!(
            donor.count <= parsed.maximum && donor.count <= 9,
            "Signature configuration donor exceeds the supported clone limit"
        );
    }
    let shift = clones.len() * RECORD;
    let mut length = align16(
        source
            .len()
            .checked_add(shift)
            .context("Signature size overflow")?,
    )?;
    for donor in requested.values() {
        length += original[donor].count * 32;
    }
    ensure!(
        length <= MAX_BYTES,
        "Generated signature table exceeds the size limit"
    );
    let mut output = vec![0; length];
    output[..HEADER].copy_from_slice(&source[..HEADER]);
    output[parsed.records_end + shift..source.len() + shift]
        .copy_from_slice(&source[parsed.records_end..]);
    put32(&mut output, 0, (parsed.entries.len() + clones.len()) as u32);
    let mut ids: BTreeSet<_> = original.keys().copied().collect();
    ids.extend(requested.keys().copied());
    let mut offsets = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        let offset = HEADER + i * RECORD;
        offsets.insert(*id, offset);
        put32(&mut output, offset, *id);
        if let Some(old) = original.get(id) {
            put32(&mut output, offset + 4, old.count as u32);
            put64(
                &mut output,
                offset + 8,
                relative(offset + 8, old.list + shift)?,
            );
        }
    }
    let mut cursor = align16(source.len() + shift)?;
    for (id, donor_id) in requested {
        let donor = original[&donor_id];
        let list = cursor;
        let payload = list + donor.count * RECORD;
        for i in 0..donor.count {
            let donor_item = donor.list + i * RECORD;
            let exact = target(source, donor_item)?;
            let block = exact & !15;
            ensure!(
                block >= parsed.records_end,
                "Donor payload block overlaps table records"
            );
            let bytes = span(source, block, RECORD)?;
            let item = list + i * RECORD;
            let dest = payload + i * RECORD;
            put64(&mut output, item, relative(item, dest + exact - block)?);
            output[item + 8..item + 16].copy_from_slice(span(source, donor_item + 8, 8)?);
            output[dest..dest + RECORD].copy_from_slice(bytes);
        }
        let offset = offsets[&id];
        put32(&mut output, offset + 4, donor.count as u32);
        put64(&mut output, offset + 8, relative(offset + 8, list)?);
        cursor = payload + donor.count * RECORD;
    }
    ensure!(
        cursor == output.len(),
        "Generated signature table length differs"
    );
    let verified = Table::parse(&output)?;
    let mut lists = BTreeSet::new();
    ensure!(
        verified.entries.iter().all(|e| lists.insert(e.list)),
        "Generated signature configuration lists are not private"
    );
    Ok(output)
}

pub(crate) fn name_hash(name: &str) -> Result<u32> {
    ensure!(
        !name.is_empty()
            && name.len() <= 255
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "Invalid signature configuration name"
    );
    let mut crc = !0u32;
    for byte in name.bytes().map(|b| b.to_ascii_uppercase()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb88320 } else { 0 };
        }
    }
    Ok(!crc)
}
pub(crate) fn replace_lists(source: &[u8], replacements: &[Replacement]) -> Result<Vec<u8>> {
    let parsed = Table::parse(source)?;
    if replacements.is_empty() {
        return Ok(source.to_vec());
    }
    ensure!(
        replacements.len() <= parsed.entries.len(),
        "Too many signature replacements"
    );
    let entries: BTreeMap<_, _> = parsed.entries.iter().map(|e| (e.id, e)).collect();
    let mut seen = BTreeSet::new();
    let mut plans = Vec::with_capacity(replacements.len());
    let mut length = align16(source.len())?;
    for replacement in replacements {
        let entry = entries
            .get(&replacement.id)
            .context("Replacement signature player is missing")?;
        ensure!(
            seen.insert(replacement.id)
                && (1..=9).contains(&replacement.names.len())
                && replacement.names.len() <= parsed.maximum,
            "Invalid or duplicate signature replacement"
        );
        let hashes: Vec<u32> = replacement
            .names
            .iter()
            .map(|s| name_hash(s))
            .collect::<Result<_>>()?;
        ensure!(
            hashes.iter().copied().collect::<BTreeSet<_>>().len() == hashes.len(),
            "Duplicate configuration name or CRC32 collision"
        );
        length = length
            .checked_add(hashes.len() * 32)
            .context("Signature size overflow")?;
        plans.push((*entry, hashes));
    }
    ensure!(
        length <= MAX_BYTES,
        "Generated signature table exceeds the size limit"
    );
    let mut output = vec![0; length];
    output[..source.len()].copy_from_slice(source);
    let mut cursor = align16(source.len())?;
    for (entry, hashes) in plans {
        let list = cursor;
        let payload = list + hashes.len() * RECORD;
        for (i, hash) in hashes.iter().enumerate() {
            let item = list + i * RECORD;
            let dest = payload + i * RECORD;
            put64(&mut output, item, relative(item, dest)?);
            put32(&mut output, dest, *hash);
        }
        put32(&mut output, entry.offset + 4, hashes.len() as u32);
        put64(
            &mut output,
            entry.offset + 8,
            relative(entry.offset + 8, list)?,
        );
        cursor = payload + hashes.len() * RECORD;
    }
    Table::parse(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::io::Read;

    pub(super) fn fixture() -> Vec<u8> {
        let mut data = vec![0; 16 + 16 + 32 * 3];
        put32(&mut data, 0, 1);
        put32(&mut data, 8, 9);
        put32(&mut data, 16, 4090);
        put32(&mut data, 20, 3);
        put64(&mut data, 24, 9);
        for i in 0..3 {
            put64(
                &mut data,
                32 + 16 * i,
                relative(32 + 16 * i, 80 + 16 * i).unwrap(),
            );
            put32(&mut data, 80 + 16 * i, (i + 1) as u32);
        }
        data
    }
    #[test]
    fn matches_reference_launcher_byte_for_byte() {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../assets/sigconfig-regressions.json.gz")[..],
        )
        .take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .unwrap();
        let fixture: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(fixture["schema"], "nba2k27-sigconfig-oracle-v1");
        assert_eq!(fixture["cases"].as_array().unwrap().len(), 60);
        for case in fixture["cases"].as_array().unwrap() {
            let data = |key: &str| -> Vec<u8> {
                case[key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect()
            };
            let source = data("source");
            let clones: Vec<CloneRequest> = serde_json::from_value(case["clones"].clone()).unwrap();
            let replacements: Vec<Replacement> =
                serde_json::from_value(case["replacements"].clone()).unwrap();
            let result = clone_records(&source, &clones)
                .and_then(|out| replace_lists(&out, &replacements))
                .unwrap();
            assert_eq!(result, data("expected"), "{}", case["name"]);
            assert_eq!(source, data("source"));
        }
    }
    #[test]
    fn private_clones_preserve_original_hashes_and_metadata() {
        let source = fixture();
        let out = clone_records(
            &source,
            &[
                CloneRequest {
                    target_id: 10000,
                    donor_id: 4090,
                },
                CloneRequest {
                    target_id: 12,
                    donor_id: 4090,
                },
            ],
        )
        .unwrap();
        let table = Table::parse(&out).unwrap();
        assert_eq!(
            table.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
            [12, 4090, 10000]
        );
        let lists = table
            .entries
            .iter()
            .map(|e| e.list)
            .collect::<BTreeSet<_>>();
        assert_eq!(lists.len(), 3);
        for entry in &table.entries {
            assert_eq!(table.hashes(&out, entry).unwrap(), [1, 2, 3]);
        }
        assert_eq!(source, fixture());
    }
    #[test]
    fn rejects_malformed_tables_without_panics() {
        let source = fixture();
        // The final twelve bytes are padding, not a required hash field.
        for end in 0..source.len() - 12 {
            assert!(Table::parse(&source[..end]).is_err());
        }
        for (offset, bytes) in [
            (0, u32::MAX.to_le_bytes().to_vec()),
            (8, 0u32.to_le_bytes().to_vec()),
            (20, 0u32.to_le_bytes().to_vec()),
            (24, u64::MAX.to_le_bytes().to_vec()),
            (32, 0u64.to_le_bytes().to_vec()),
        ] {
            let mut malformed = source.clone();
            malformed[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert!(Table::parse(&malformed).is_err());
        }
        let mut seed = 12345u64;
        for length in 0..512 {
            let mut data = vec![0; length];
            for byte in &mut data {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *byte = seed as u8;
            }
            assert!(Table::parse(&data).is_err());
        }
    }
    #[test]
    fn refuses_duplicate_existing_or_unknown_clone_requests() {
        let source = fixture();
        for requests in [
            vec![CloneRequest {
                target_id: 4090,
                donor_id: 4090,
            }],
            vec![CloneRequest {
                target_id: 100000,
                donor_id: 4090,
            }],
            vec![CloneRequest {
                target_id: 10000,
                donor_id: 42,
            }],
            vec![
                CloneRequest {
                    target_id: 10000,
                    donor_id: 4090
                };
                2
            ],
        ] {
            assert!(clone_records(&source, &requests).is_err());
            assert_eq!(source, fixture());
        }
    }
    #[test]
    fn replacements_append_and_validate_names_without_mutating_shared_storage() {
        let source = fixture();
        let good = Replacement {
            id: 4090,
            names: vec!["AFRO".into(), "headband_fury".into()],
        };
        let out = replace_lists(&source, &[good.clone()]).unwrap();
        assert_eq!(&out[32..source.len()], &source[32..]);
        let table = Table::parse(&out).unwrap();
        assert_eq!(
            table.hashes(&out, &table.entries[0]).unwrap(),
            [
                name_hash("afro").unwrap(),
                name_hash("HEADBAND_FURY").unwrap()
            ]
        );
        assert_eq!(name_hash("123456789").unwrap(), 0xcbf43926);
        for names in [
            vec![],
            vec!["afro", "AFRO"],
            vec!["../bad"],
            vec![""],
            vec!["a b"],
            vec!["é"],
            vec!["x"; 10],
        ] {
            assert!(
                replace_lists(
                    &source,
                    &[Replacement {
                        id: 4090,
                        names: names.into_iter().map(str::to_owned).collect()
                    }]
                )
                .is_err()
            );
        }
        assert!(replace_lists(&source, &[good.clone(), good]).is_err());
        assert_eq!(source, fixture());
    }
}
