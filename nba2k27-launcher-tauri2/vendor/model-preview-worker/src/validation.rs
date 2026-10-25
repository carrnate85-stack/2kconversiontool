//! In-memory export reports. Never rewrites output or creates sidecars.
use crate::{
    catalog::safe_relative,
    codec::{self, Archive},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
};

#[derive(Default)]
struct Report {
    checks: Vec<Value>,
    warnings: Vec<String>,
    errors: Vec<String>,
}
impl Report {
    fn check(&mut self, name: &str, errors: usize, detail: String) {
        self.checks.push(
            json!({"name":name,"status":if errors>0{"failed"}else{"passed"},"detail":detail}),
        );
    }
    fn finish(self, summary: Value) -> Value {
        json!({"status":if !self.errors.is_empty(){"failed"}else if !self.warnings.is_empty(){"warning"}else{"ready"},"checks":self.checks,"warnings":self.warnings,"errors":self.errors,"summary":summary})
    }
}
pub fn dds(header: &[u8], total: u64) -> Result<()> {
    ensure!(
        header.len() >= 128 && header.starts_with(b"DDS "),
        "Missing or truncated DDS header"
    );
    ensure!(
        codec::u32_at(header, 4)? == 124 && codec::u32_at(header, 76)? == 32,
        "Invalid DDS header sizes"
    );
    let mut w = codec::u32_at(header, 16)? as u64;
    let mut h = codec::u32_at(header, 12)? as u64;
    let caps = codec::u32_at(header, 112)?;
    let mut d = if caps & 0x200000 != 0 {
        codec::u32_at(header, 24)? as u64
    } else {
        1
    };
    let mips = codec::u32_at(header, 28)?.max(1);
    ensure!(
        w > 0 && h > 0 && d > 0 && mips <= 64 - w.max(h).max(d).leading_zeros(),
        "Invalid DDS dimensions or mip count"
    );
    let mut layers = 1u64;
    let mut offset = 128u64;
    let (block, pixel) = if header.get(84..88) == Some(b"DX10") {
        ensure!(header.len() >= 148, "Truncated DDS DX10 header");
        offset = 148;
        let format = codec::u32_at(header, 128)?;
        let dimension = codec::u32_at(header, 132)?;
        layers = codec::u32_at(header, 140)? as u64;
        ensure!(
            layers > 0 && matches!(dimension, 2..=4),
            "Invalid DDS array or resource dimension"
        );
        if dimension == 4 {
            d = codec::u32_at(header, 24)? as u64;
            ensure!(d > 0 && layers == 1, "Invalid DDS volume dimensions");
        }
        if codec::u32_at(header, 136)? & 4 != 0 {
            layers *= 6;
        }
        (
            match format {
                70..=72 | 79..=81 => 8,
                73..=78 | 82..=84 | 94..=99 => 16,
                _ => 0,
            },
            match format {
                2 => 16,
                10..=14 => 8,
                26..=31 | 87 | 91 => 4,
                49 => 2,
                61 => 1,
                _ => 0,
            },
        )
    } else {
        if caps & 0x200 != 0 {
            layers = [0x400, 0x800, 0x1000, 0x2000, 0x4000, 0x8000]
                .iter()
                .filter(|b| caps & **b != 0)
                .count() as u64;
        }
        (
            match &header[84..88] {
                b"DXT1" | b"ATI1" | b"BC4U" | b"BC4S" => 8,
                b"DXT2" | b"DXT3" | b"DXT4" | b"DXT5" | b"ATI2" | b"BC5U" | b"BC5S" => 16,
                _ => 0,
            },
            if codec::u32_at(header, 80)? & 4 == 0 {
                codec::u32_at(header, 88)? as u64 / 8
            } else {
                0
            },
        )
    };
    ensure!(layers > 0 && total > offset, "DDS has no texture payload");
    if block == 0 && pixel == 0 {
        return Ok(());
    } // Unknown formats aren't guessed.
    let mut required = 0u64;
    for _ in 0..mips {
        let row = if block > 0 {
            w.div_ceil(4)
                .checked_mul(h.div_ceil(4))
                .and_then(|n| n.checked_mul(block))
        } else {
            w.checked_mul(h).and_then(|n| n.checked_mul(pixel))
        }
        .context("DDS payload size overflow")?;
        required = required
            .checked_add(row.checked_mul(d).context("DDS payload size overflow")?)
            .context("DDS payload size overflow")?;
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        d = (d / 2).max(1);
    }
    ensure!(
        total
            >= offset
                .checked_add(
                    required
                        .checked_mul(layers)
                        .context("DDS payload size overflow")?
                )
                .context("DDS payload size overflow")?,
        "Truncated DDS texture or mip payload"
    );
    Ok(())
}
pub fn file(name: &str, bytes: &[u8], mode: &str, conversion: &Value) -> Value {
    let mut r = file_checks(name, &bytes[..bytes.len().min(148)], bytes.len() as u64);
    let lower = name.to_lowercase();
    if lower.ends_with(".scne") || lower.ends_with(".txtr") {
        if let Err(e) = codec::tree_exact(bytes) {
            r.errors.push(format!("Unreadable document: {e}"));
        }
    }
    finish_file(r, bytes.len() as u64, mode, conversion)
}
fn file_checks(name: &str, header: &[u8], size: u64) -> Report {
    let mut r = Report::default();
    if size == 0 {
        r.errors.push("Exported file is empty".into());
    }
    let lower = name.to_lowercase();
    if lower.ends_with(".iff") {
        r.errors
            .push("IFF output is not a ZIP/IFF container".into());
    }
    if lower.ends_with(".dds") {
        if let Err(e) = dds(header, size) {
            r.errors.push(e.to_string());
        }
    }
    r
}
pub fn streamed_file(
    name: &str,
    header: &[u8],
    size: u64,
    mode: &str,
    conversion: &Value,
) -> Result<Value> {
    ensure!(
        crate::raw_export::eligible_name(name, mode) && !header.starts_with(b"PK"),
        "Resource requires buffered conversion or validation"
    );
    Ok(finish_file(
        file_checks(name, header, size),
        size,
        mode,
        conversion,
    ))
}
fn finish_file(mut r: Report, size: u64, mode: &str, conversion: &Value) -> Value {
    conversion_warnings(&mut r, conversion);
    r.check(
        "File output",
        r.errors.len(),
        format!("{size} source-derived bytes; {mode}"),
    );
    r.finish(json!({"entries":1,"bytes":size,"mode":mode}))
}
fn conversion_warnings(r: &mut Report, conversion: &Value) {
    if let Some(warnings) = conversion["warnings"].as_array() {
        for w in warnings {
            r.warnings.push(format!(
                "{}: {}",
                w["name"].as_str().unwrap_or("Resource"),
                w["reason"].as_str().unwrap_or("Unsupported data preserved")
            ));
        }
    }
}
fn references(tree: &Value, out: &mut BTreeSet<String>, depth: usize) -> Result<()> {
    ensure!(depth <= 128, "Dependency nesting exceeds limit");
    match tree {
        Value::Object(obj) => {
            if let Some(name) = obj.get("Binary").and_then(Value::as_str) {
                out.insert(name.replace('\\', "/").to_lowercase());
            }
            for child in obj.values() {
                references(child, out, depth + 1)?;
            }
        }
        Value::Array(a) => {
            for child in a {
                references(child, out, depth + 1)?;
            }
        }
        _ => {}
    }
    ensure!(out.len() <= 100000, "Dependency count exceeds limit");
    Ok(())
}
pub fn archive(
    a: &mut Archive,
    mode: &str,
    conversion: &Value,
    cancel: &AtomicBool,
) -> Result<Value> {
    let mut r = Report::default();
    let mut names = BTreeSet::new();
    if a.has_shadowed_members() {
        r.warnings.push("Repeated identical member names preserved in the original container. All physical entries were CRC-checked and are separately inspectable/saveable. Name-based model/texture preview and mod-ready conversion remain unavailable for this ambiguous archive.".into());
    }
    let mut leaves = BTreeMap::<String, usize>::new();
    let mut refs = BTreeSet::new();
    let (mut scenes, mut textures, mut native, mut scene_errors, mut dds_errors) = (0, 0, 0, 0, 0);
    for i in 0..a.source_member_count() {
        codec::cancelled(cancel)?;
        let m = a.member(i)?;
        if m.is_dir() {
            continue;
        }
        let name = m.name.replace('\\', "/");
        safe_relative(&name)?;
        let lower = name.to_lowercase();
        if !names.insert(lower.clone()) {
            r.warnings.push(format!("Duplicate member: {name}"));
        }
        *leaves
            .entry(lower.rsplit('/').next().unwrap_or("").into())
            .or_default() += 1;
        if lower.ends_with(".tld") || lower.ends_with(".gz") {
            native += 1;
        }
        if lower.ends_with(".dds") {
            textures += 1;
            let size = m.size;
            let header = a.prefix(i, size.min(148) as usize, cancel)?;
            if let Err(e) = dds(&header, size) {
                dds_errors += 1;
                r.errors.push(format!("{name}: {e}"));
            }
        } else if lower.ends_with(".scne") || lower.ends_with(".txtr") {
            scenes += 1;
            let size = m.size;
            if size > 32 * 1024 * 1024 {
                r.warnings
                    .push(format!("Document inspection budget exceeded: {name}"));
                continue;
            }
            match a
                .read(i, 32 * 1024 * 1024)
                .and_then(|b| codec::tree_exact(&b))
            {
                Ok(tree) => references(&tree, &mut refs, 0)?,
                Err(e) => {
                    scene_errors += 1;
                    r.errors.push(format!("Unreadable document {name}: {e}"));
                }
            }
        }
    }
    let unresolved: Vec<_> = refs
        .into_iter()
        .filter(|name| {
            !names.contains(name) && leaves.get(name.rsplit('/').next().unwrap_or("")) != Some(&1)
        })
        .collect();
    conversion_warnings(&mut r, conversion);
    if mode == "mod-ready" && !unresolved.is_empty() {
        r.warnings.push(format!("{} references absent from exported IFF; shared or unavailable source data may still be required",unresolved.len()));
    }
    if mode == "mod-ready" && native > 0 {
        r.warnings.push(format!(
            "{native} native TLD/GZ members preserved; conversion was not guessed"
        ));
    }
    r.check(
        "Archive integrity",
        0,
        format!(
            "{} members CRC-checked before publication",
            a.source_member_count()
        ),
    );
    r.check(
        "SCNE/TXTR parse",
        scene_errors,
        format!("{scenes} documents checked"),
    );
    r.check(
        "DDS headers",
        dds_errors,
        format!("{textures} textures checked"),
    );
    r.checks.push(json!({"name":"Dependency closure","status":if mode=="original"{"not-applicable"}else if unresolved.is_empty(){"passed"}else{"warning"},"detail":if mode=="original"{"Untouched-container mode preserves authored references".to_string()}else{format!("{} references absent from IFF",unresolved.len())}}));
    Ok(r.finish(json!({"entries":a.source_member_count(),"indexedEntries":a.indexed_member_count(),"scneTxtr":scenes,"dds":textures,"nativeMembers":native,"unresolvedReferences":unresolved,"mode":mode})))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn header() -> Vec<u8> {
        let mut b = vec![0u8; 148];
        b[..4].copy_from_slice(b"DDS ");
        for (p, n) in [(4, 124u32), (76, 32), (12, 4), (16, 4), (80, 4)] {
            b[p..p + 4].copy_from_slice(&n.to_le_bytes());
        }
        b[84..88].copy_from_slice(b"DXT1");
        b
    }
    #[test]
    fn dds_rejects_short_payload_and_excess_mips() {
        let mut h = header();
        assert!(dds(&h, 136).is_ok());
        assert!(dds(&h, 135).is_err());
        h[28..32].copy_from_slice(&4u32.to_le_bytes());
        assert!(dds(&h, 4096).is_err());
    }
    #[test]
    fn streamed_file_report_matches_buffered_validation_and_rejects_documents() {
        for name in ["data.bin", "color.dds", "original.tld", "buffer.gz"] {
            for data in [b"source".to_vec(), header(), vec![]] {
                for mode in ["original", "mod-ready"] {
                    if crate::raw_export::eligible_name(name, mode) {
                        assert_eq!(
                            file(name, &data, mode, &Value::Null),
                            streamed_file(
                                name,
                                &data[..data.len().min(148)],
                                data.len() as u64,
                                mode,
                                &Value::Null
                            )
                            .unwrap()
                        );
                    }
                }
            }
        }
        for name in ["level.scne", "texture.txtr", "arena.iff"] {
            assert!(streamed_file(name, b"data", 4, "original", &Value::Null).is_err());
        }
        assert!(
            streamed_file("disguised.bin", b"PK\x03\x04", 4, "original", &Value::Null).is_err()
        );
        let data = header();
        assert_eq!(
            streamed_file(
                "huge.dds",
                &data,
                513 * 1024 * 1024,
                "mod-ready",
                &Value::Null
            )
            .unwrap()["status"],
            "ready"
        );
    }
    #[test]
    fn dds_array_cube_payload_is_bounded() {
        let mut h = header();
        h[84..88].copy_from_slice(b"DX10");
        for (p, n) in [(128, 71u32), (132, 3), (136, 4), (140, 2)] {
            h[p..p + 4].copy_from_slice(&n.to_le_bytes());
        }
        assert!(dds(&h, 148 + 12 * 8).is_ok());
        assert!(dds(&h, 148 + 6 * 8).is_err());
    }
    #[test]
    fn warnings_are_not_ready_and_empty_output_fails() {
        assert_eq!(
            file(
                "asset.bin",
                b"data",
                "mod-ready",
                &json!({"warnings":[{"name":"x","reason":"missing"}]})
            )["status"],
            "warning"
        );
        assert_eq!(
            file("empty.bin", b"", "original", &Value::Null)["status"],
            "failed"
        );
    }
}
