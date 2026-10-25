//! Conservative member comparison. Different ZIP compression/order is harmless;
//! different pixels or buffers are never called equivalent without proof.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{Read, Seek},
    path::Path,
    sync::atomic::AtomicBool,
};
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const MAX_TEXT: usize = 64 * 1024 * 1024;
fn hash(reader: &mut impl Read, cancel: &AtomicBool, expected: u64) -> Result<String> {
    ensure!(expected <= MAX_TOTAL, "Comparison input exceeds budget");
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 128 * 1024];
    let mut total = 0u64;
    loop {
        crate::codec::cancelled(cancel)?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        ensure!(total <= MAX_TOTAL, "Comparison input exceeds 2 GiB limit");
        digest.update(&buffer[..count]);
    }
    ensure!(total == expected, "Comparison file/member length mismatch");
    Ok(format!("{:X}", digest.finalize()))
}
fn names<R: Read + Seek>(zip: &mut zip::ZipArchive<R>) -> Result<BTreeMap<(String, usize), usize>> {
    ensure!(zip.len() <= 100_000, "Too many comparison members");
    let mut names = BTreeMap::new();
    let mut counts = BTreeMap::<String, usize>::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        ensure!(!file.encrypted(), "Encrypted comparison member");
        total = total
            .checked_add(file.size())
            .context("Member size overflow")?;
        ensure!(total <= MAX_TOTAL, "Comparison members exceed 2 GiB budget");
        let name = file.name().replace('\\', "/").to_lowercase();
        let occurrence = counts.entry(name.clone()).or_default();
        names.insert((name, *occurrence), i);
        *occurrence += 1;
    }
    Ok(names)
}
fn document(bytes: &[u8]) -> Result<Value> {
    crate::codec::tree_exact(bytes).or_else(|_| {
        let text = std::str::from_utf8(bytes)?
            .trim_matches('\0')
            .trim()
            .trim_start_matches('\u{feff}');
        serde_json::from_str(text)
            .or_else(|_| serde_json::from_str(&format!("{{{text}}}")))
            .map_err(Into::into)
    })
}
fn references(value: &mut Value, available: &HashSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key.eq_ignore_ascii_case("binary") {
                    if let Some(source) = value.as_str() {
                        let normalized = source.replace('\\', "/").to_lowercase();
                        let target = if let Some(base) = normalized.strip_suffix(".tld") {
                            Some(format!("{base}.dds"))
                        } else {
                            normalized
                                .strip_suffix(".gz")
                                .map(|base| format!("{base}.bin"))
                        };
                        if let Some(target) = target {
                            // Never normalize an unresolved reference to a nonexistent file.
                            if available.contains(&target)
                                || available.contains(target.rsplit('/').next().unwrap())
                            {
                                *value = json!(target);
                            }
                        }
                    }
                } else {
                    references(value, available);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                references(value, available);
            }
        }
        _ => {}
    }
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|other| equal(v, other)))
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        _ => a == b,
    }
}
pub fn compare(native: &Path, oracle: &Path, cancel: &AtomicBool) -> Result<Value> {
    let native_hash = hash(
        &mut File::open(native)?,
        cancel,
        std::fs::metadata(native)?.len(),
    )?;
    let oracle_hash = hash(
        &mut File::open(oracle)?,
        cancel,
        std::fs::metadata(oracle)?.len(),
    )?;
    let mut a = zip::ZipArchive::new(File::open(native)?)?;
    let mut b = zip::ZipArchive::new(File::open(oracle)?)?;
    let an = names(&mut a)?;
    let bn = names(&mut b)?;
    let mut available: HashSet<_> = an.keys().map(|(n, _)| n.clone()).collect();
    let mut leaves = BTreeMap::<String, usize>::new();
    for (name, _) in an.keys() {
        *leaves
            .entry(name.rsplit('/').next().unwrap().to_owned())
            .or_default() += 1;
    }
    available.extend(
        leaves
            .into_iter()
            .filter(|(_, n)| *n == 1)
            .map(|(name, _)| name),
    );
    let mut identical = 0;
    let mut equivalent = 0;
    let mut differing = 0;
    let mut missing_native = 0;
    let mut missing_oracle = 0;
    let mut details = Vec::new();
    for key in an
        .keys()
        .chain(bn.keys().filter(|key| !an.contains_key(*key)))
    {
        crate::codec::cancelled(cancel)?;
        let (name, occurrence) = key;
        let status = match (an.get(key), bn.get(key)) {
            (Some(&ai), Some(&bi)) => {
                let mut am = a.by_index(ai)?;
                let mut bm = b.by_index(bi)?;
                let asize = am.size();
                let bsize = bm.size();
                if hash(&mut am, cancel, asize)? == hash(&mut bm, cancel, bsize)? {
                    identical += 1;
                    continue;
                }
                if (name.ends_with(".scne") || name.ends_with(".txtr"))
                    && am.size() <= MAX_TEXT as u64
                    && bm.size() <= MAX_TEXT as u64
                {
                    drop(am);
                    drop(bm);
                    let mut ab = Vec::new();
                    let mut bb = Vec::new();
                    a.by_index(ai)?
                        .take(MAX_TEXT as u64 + 1)
                        .read_to_end(&mut ab)?;
                    b.by_index(bi)?
                        .take(MAX_TEXT as u64 + 1)
                        .read_to_end(&mut bb)?;
                    ensure!(
                        ab.len() <= MAX_TEXT && bb.len() <= MAX_TEXT,
                        "Text member exceeds budget"
                    );
                    if let (Ok(mut av), Ok(mut bv)) = (document(&ab), document(&bb)) {
                        references(&mut av, &available);
                        references(&mut bv, &available);
                        if equal(&av, &bv) {
                            equivalent += 1;
                            "equivalent document encoding"
                        } else {
                            differing += 1;
                            "document values differ"
                        }
                    } else {
                        differing += 1;
                        "unparsed document bytes differ"
                    }
                } else {
                    differing += 1;
                    "payload bytes differ"
                }
            }
            (None, Some(_)) => {
                missing_native += 1;
                "missing from native"
            }
            (Some(_), None) => {
                missing_oracle += 1;
                "missing from mod.exe"
            }
            _ => unreachable!(),
        };
        if details.len() < 200 {
            details.push(json!({"name":name,"occurrence":occurrence,"status":status}));
        }
    }
    let mismatches = differing + missing_native + missing_oracle;
    Ok(
        json!({"status":if mismatches>0{"mismatch"}else if equivalent>0{"equivalent"}else{"match"},"containerBytesIdentical":native_hash==oracle_hash,"nativeSha256":native_hash,"modSha256":oracle_hash,"identicalMembers":identical,"equivalentDocuments":equivalent,"differingMembers":differing,"missingFromNative":missing_native,"missingFromMod":missing_oracle,"details":details,"detailsTruncated":differing+equivalent+missing_native+missing_oracle>200,"notice":"Member comparison is conservative: different buffer/pixel bytes and extra/missing members are reported, not silently accepted. A match does not certify an in-game load."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(path: &Path, members: &[(&str, &[u8])], deflate: bool) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, bytes) in members {
            zip.start_file(
                *name,
                zip::write::SimpleFileOptions::default().compression_method(if deflate {
                    zip::CompressionMethod::Deflated
                } else {
                    zip::CompressionMethod::Stored
                }),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    #[test]
    fn compression_order_and_duplicate_members() {
        let work = crate::mod_runner::Work::new().unwrap();
        let a = work.root.join("a");
        let b = work.root.join("b");
        fixture(&a, &[("A.bin", b"one"), ("B.bin", b"two")], false);
        fixture(&b, &[("B.bin", b"two"), ("A.bin", b"one")], true);
        let r = compare(&a, &b, &AtomicBool::new(false)).unwrap();
        assert_eq!(r["status"], "match");
        assert_eq!(r["containerBytesIdentical"], false);
        assert_eq!(r["identicalMembers"], 2);
    }
    #[test]
    fn different_pixels_and_missing_members_are_mismatches() {
        let work = crate::mod_runner::Work::new().unwrap();
        let a = work.root.join("a");
        let b = work.root.join("b");
        fixture(&a, &[("t.dds", b"pixels"), ("extra.bin", b"extra")], false);
        fixture(&b, &[("t.dds", b"different")], false);
        let r = compare(&a, &b, &AtomicBool::new(false)).unwrap();
        assert_eq!(r["status"], "mismatch");
        assert_eq!(r["differingMembers"], 1);
        assert_eq!(r["missingFromMod"], 1);
    }
    #[test]
    fn duplicate_occurrences_are_not_collapsed() {
        let work = crate::mod_runner::Work::new().unwrap();
        let a = work.root.join("a");
        let b = work.root.join("b");
        fixture(&a, &[("A.bin", b"one"), ("a.bin", b"two")], false);
        fixture(&b, &[("A.bin", b"one")], false);
        let report = compare(&a, &b, &AtomicBool::new(false)).unwrap();
        assert_eq!(report["identicalMembers"], 1);
        assert_eq!(report["missingFromMod"], 1);
        assert_eq!(report["status"], "mismatch");
    }
    #[test]
    fn reference_normalization_requires_existing_payload() {
        let mut a = json!({"Binary":"missing.gz"});
        let available = HashSet::new();
        references(&mut a, &available);
        assert_eq!(a["Binary"], "missing.gz");
        let mut a = json!({"Binary":"file.tld"});
        references(&mut a, &HashSet::from(["file.dds".into()]));
        assert_eq!(a["Binary"], "file.dds");
        assert!(!equal(
            &json!(9007199254740993u64),
            &json!(9007199254740992u64)
        ));
    }
    #[test]
    fn semantic_json_and_cancellation() {
        let work = crate::mod_runner::Work::new().unwrap();
        let a = work.root.join("a");
        let b = work.root.join("b");
        fixture(&a, &[("x.SCNE", br#"{"a":1,"b":2}"#)], false);
        fixture(&b, &[("x.SCNE", br#""b":2,"a":1"#)], false);
        assert_eq!(
            compare(&a, &b, &AtomicBool::new(false)).unwrap()["status"],
            "equivalent"
        );
        assert!(compare(&a, &b, &AtomicBool::new(true)).is_err());
    }
}
