// Shared pure texture decoding. Gallery limits never truncate export data.
use crate::{
    catalog::Catalog,
    codec::{self, Archive, MAX, Oodle},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn number(v: &Value) -> usize {
    v.as_u64().unwrap_or(0) as usize
}
fn segment_limit(tld: &[u8]) -> Result<usize> {
    // Segment tables cover every array layer; keep ordinary textures at 32.
    let layers = usize::from(codec::u16_at(tld, 22)?).max(1);
    Ok(layers.min(8) * 32)
}
fn collect(v: &Value, out: &mut BTreeMap<String, Value>, depth: usize) -> Result<()> {
    ensure!(depth <= 128, "Texture metadata nesting exceeds limit");
    if let Some(map) = v.as_object() {
        if let Some(name) = map
            .get("Binary")
            .and_then(Value::as_str)
            .filter(|n| n.to_lowercase().ends_with(".tld") || n.to_lowercase().ends_with(".dds"))
        {
            out.insert(name.to_string(), v.clone());
        }
        for child in map.values() {
            collect(child, out, depth + 1)?;
        }
    }
    if let Some(array) = v.as_array() {
        for child in array {
            collect(child, out, depth + 1)?;
        }
    }
    Ok(())
}
fn resource(archive: &mut Archive, catalog: &Catalog, name: &str) -> Result<Vec<u8>> {
    archive
        .named(name)
        .or_else(|_| codec::range(catalog, catalog.resolve(name)?))
}
type ReadHeader = unsafe extern "C" fn(*const u8, *mut i64, *mut i64) -> i64;
type Scratch = unsafe extern "C" fn(i64) -> i64;
type Bc7 = unsafe extern "C" fn(*mut u8, i64, *const u8, i64, *const u8, i32, *mut u8, i64) -> i64;
fn bc7(root: &std::path::Path, prepared: &[u8]) -> Result<Vec<u8>> {
    ensure!(prepared.len() >= 48, "Truncated BC7Prep");
    unsafe {
        let library = libloading::Library::new(
            root.join("data/oodle/oo2texrt_9_win64.dll")
                .canonicalize()?,
        )?;
        let header = library.get::<ReadHeader>(b"OodleTexRT_BC7Prep_ReadHeader\0")?;
        let scratch = library.get::<Scratch>(b"OodleTexRT_BC7Prep_MinDecodeScratchSize\0")?;
        let decode = library.get::<Bc7>(b"OodleTexRT_BC7Prep_Decode\0")?;
        let (mut blocks, mut encoded) = (0, 0);
        let status = header(prepared.as_ptr(), &mut blocks, &mut encoded);
        ensure!(
            status == 0
                && blocks > 0
                && blocks <= MAX as i64 / 16
                && encoded > 0
                && encoded <= prepared.len() as i64 - 48,
            "Invalid BC7Prep header"
        );
        let scratch_size = scratch(blocks);
        ensure!(
            scratch_size > 0 && scratch_size <= MAX as i64,
            "Invalid BC7 scratch size"
        );
        let mut work = vec![0; scratch_size as usize];
        let mut output = vec![0; blocks as usize * 16];
        let count = decode(
            output.as_mut_ptr(),
            output.len() as i64,
            prepared[48..].as_ptr(),
            encoded,
            prepared.as_ptr(),
            0,
            work.as_mut_ptr(),
            scratch_size,
        );
        ensure!(
            count == output.len() as i64,
            "BC7Prep decoded unexpected size"
        );
        Ok(output)
    }
}
pub fn reconstruct(
    archive: &mut Archive,
    catalog: &Catalog,
    oodle: &Oodle,
    name: &str,
    info: &Value,
    cancel: &AtomicBool,
    limit: usize,
) -> Result<Vec<u8>> {
    let tld = resource(archive, catalog, name)?;
    if tld.starts_with(b"DDS ") {
        return codec::dds(&tld);
    }
    if let Ok(dds) = codec::dds(&tld) {
        return Ok(dds);
    }
    ensure!(
        tld.len() >= 32 && tld.starts_with(b"TLD"),
        "Not a TLD texture"
    );
    let tail;
    let info = if info["Segments"]
        .as_array()
        .is_some_and(|segments| !segments.is_empty())
    {
        info
    } else {
        tail = tail_info(&tld)?;
        &tail
    };
    let segments = info["Segments"]
        .as_array()
        .context("No SCNE segment table; standalone tail format pending")?;
    ensure!(
        !segments.is_empty() && segments.len() <= segment_limit(&tld)?,
        "Invalid texture segment count"
    );
    let size = codec::u32_at(&tld, 24)? as usize;
    ensure!(
        size > 0 && size <= limit && limit <= MAX,
        "Texture exceeds decoding budget"
    );
    let inline_end = 32usize
        .checked_add(codec::u32_at(&tld, 28)? as usize)
        .context("TLD size overflow")?;
    ensure!(inline_end <= tld.len(), "TLD inline size exceeds container");
    let format = tld[10];
    let block = match format {
        70..=72 | 79..=81 => 8,
        73..=78 | 82..=84 | 94..=99 => 16,
        10 | 11 => 8,
        26 | 28 => 4,
        49 => 2,
        61 => 1,
        _ => 0,
    };
    ensure!(block > 0, "Unsupported TLD element format");
    let mut payload = vec![0; size];
    let mut covered = vec![false; size];
    let mut mip = None;
    for segment in segments {
        codec::cancelled(cancel)?;
        let off = number(&segment["FileOffset"]);
        let packed = number(&segment["FileSize"]);
        let target = number(&segment["MemoryOffset"]);
        let raw = number(&segment["MemorySize"]);
        let inflated = number(&segment["InflatedSize"]);
        ensure!(
            packed > 0 && raw > 0 && target <= size && raw <= size - target,
            "Invalid segment destination"
        );
        let source = if off < inline_end {
            tld.get(off..off.checked_add(packed).context("Texture source overflow")?)
                .context("Truncated inline segment")?
        } else {
            if mip.is_none() {
                let mip_name = format!("{}.mip0", name.strip_suffix(".tld").unwrap_or(name));
                mip = Some(resource(archive, catalog, &mip_name)?);
            }
            let start = off - inline_end;
            mip.as_ref()
                .unwrap()
                .get(start..start.checked_add(packed).context("Mip source overflow")?)
                .context("Truncated external mip segment")?
        };
        let inflated = if inflated == 0 && packed == raw {
            source.to_vec()
        } else {
            let expected = if inflated > 0 { inflated } else { raw };
            ensure!(expected <= limit, "Inflated segment too large");
            let (data, count) = oodle.decode(source, expected)?;
            ensure!(
                count == expected as i64,
                "Oodle texture segment length mismatch"
            );
            data
        };
        let decoded = if (format == 98 || format == 99) && inflated.starts_with(&[0xbc, 7, 0, 0]) {
            bc7(&catalog.root, &inflated)?
        } else if number(&info["Twiddled"]) != 0 && block >= 4 {
            ensure!(
                raw % block == 0 && inflated.len() >= raw,
                "Invalid twiddled element planes"
            );
            let blocks = raw / block;
            let plane_size = blocks * 4;
            let mut out = vec![0; raw];
            for b in 0..blocks {
                for plane in 0..block / 4 {
                    out[b * block + plane * 4..b * block + plane * 4 + 4].copy_from_slice(
                        &inflated[plane * plane_size + b * 4..plane * plane_size + b * 4 + 4],
                    );
                }
            }
            out
        } else {
            ensure!(inflated.len() >= raw, "Truncated texture segment");
            inflated[..raw].to_vec()
        };
        ensure!(
            decoded.len() == raw && !covered[target..target + raw].iter().any(|v| *v),
            "Texture segment overlap or size mismatch"
        );
        payload[target..target + raw].copy_from_slice(&decoded);
        covered[target..target + raw].fill(true);
    }
    ensure!(
        covered.iter().all(|v| *v),
        "Texture segments do not cover the complete payload"
    );
    let mut inline = tld[..32].to_vec();
    inline[24..28].copy_from_slice(&(size as u32).to_le_bytes());
    inline[28..32].copy_from_slice(&(size as u32).to_le_bytes());
    inline.extend(payload);
    codec::dds(&inline)
}
// Same compact standalone segment table used by the stable native decoder.
// It is metadata, not texture pixels; never append it to an exported DDS.
fn tail_info(tld: &[u8]) -> Result<Value> {
    ensure!(
        tld.len() >= 56 && tld.starts_with(b"TLD"),
        "No standalone TLD segment table"
    );
    let end = 32usize
        .checked_add(codec::u32_at(tld, 28)? as usize)
        .context("TLD tail overflow")?;
    ensure!(
        end <= tld.len()
            && tld.len() > end
            && (tld.len() - end) % 24 == 0
            && (tld.len() - end) / 24 <= segment_limit(tld)?,
        "Invalid standalone TLD segment table"
    );
    let mut external = false;
    let mut segments = Vec::new();
    for p in (end..tld.len()).step_by(24) {
        let mut off = codec::u32_at(tld, p)? as usize;
        external |= off == 0;
        if external {
            off = off.checked_add(end).context("External offset overflow")?;
        }
        segments.push(json!({"FileOffset":off,"FileSize":codec::u32_at(tld,p+4)?,"MemoryOffset":codec::u32_at(tld,p+8)?,"MemorySize":codec::u32_at(tld,p+12)?,"InflatedSize":codec::u32_at(tld,p+16)?}));
    }
    Ok(json!({"Twiddled":1,"Segments":segments}))
}
fn descriptors(archive: &mut Archive, cancel: &AtomicBool) -> Result<BTreeMap<String, Value>> {
    let members = archive.names()?;
    let mut textures = BTreeMap::new();
    for m in &members {
        let name = text(&m["name"]);
        let lower = name.to_lowercase();
        if lower.ends_with(".scne") || lower.ends_with(".txtr") {
            codec::cancelled(cancel)?;
            let bytes = archive.read(number(&m["id"]), 32 * 1024 * 1024)?;
            if let Ok(tree) = codec::tree(&bytes) {
                collect(&tree, &mut textures, 0)?;
            }
        }
        if lower.ends_with(".dds") || lower.ends_with(".tld") {
            textures.entry(name.to_string()).or_insert(Value::Null);
        }
    }
    Ok(textures)
}
pub fn one(
    archive: &mut Archive,
    catalog: &Catalog,
    name: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let textures = descriptors(archive, cancel)?;
    let (name, info) = textures
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .context("Texture is not a member/reference of this IFF")?;
    reconstruct(
        archive,
        catalog,
        &Oodle::load(&catalog.root)?,
        name,
        info,
        cancel,
        16 * 1024 * 1024,
    )
}
pub fn gallery(
    archive: &mut Archive,
    catalog: &Catalog,
    cancel: &AtomicBool,
    mut decorate: impl FnMut(&str, &mut Archive, &mut Value),
) -> Result<Value> {
    let oodle = Oodle::load(&catalog.root)?;
    let textures = descriptors(archive, cancel)?;
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    let mut total = 0;
    for (name, info) in textures {
        codec::cancelled(cancel)?;
        if items.len() >= 120 || total >= 32 * 1024 * 1024 {
            skipped.push(json!({"name":name,"reason":"Gallery budget: 120 items / 32 MiB"}));
            continue;
        }
        match reconstruct(
            archive,
            catalog,
            &oodle,
            &name,
            &info,
            cancel,
            16 * 1024 * 1024,
        ) {
            Ok(dds) => {
                if total + dds.len() > 32 * 1024 * 1024 {
                    skipped.push(json!({"name":name,"reason":"Gallery memory budget"}));
                    continue;
                }
                total += dds.len();
                let fourcc = dds
                    .get(84..88)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .unwrap_or("DDS");
                let format = if fourcc == "DX10" {
                    format!("DXGI {}", codec::u32_at(&dds, 128).unwrap_or(0))
                } else {
                    fourcc.into()
                };
                use base64::Engine;
                let mut item = json!({"name":name,"data":base64::engine::general_purpose::STANDARD.encode(&dds),"width":codec::u32_at(&dds,16).unwrap_or(0),"height":codec::u32_at(&dds,12).unwrap_or(0),"mips":codec::u32_at(&dds,28).unwrap_or(1).max(1),"format":format});
                decorate(&name, archive, &mut item);
                items.push(item);
            }
            Err(error) => skipped.push(json!({"name":name,"reason":format!("{error:#}")})),
        }
    }
    Ok(json!({"items":items,"skipped":skipped}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_segment_budget_is_bounded_and_single_layer_limit_is_unchanged() {
        let mut bytes = vec![0; 32];
        assert_eq!(segment_limit(&bytes).unwrap(), 32);
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(segment_limit(&bytes).unwrap(), 32);
        bytes[22..24].copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(segment_limit(&bytes).unwrap(), 256);
        bytes[22..24].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(segment_limit(&bytes).unwrap(), 256);
        assert!(segment_limit(&bytes[..23]).is_err());
    }
    #[test]
    fn forty_segment_array_tail_is_accepted_but_not_for_one_layer() {
        let mut bytes = vec![0; 40 + 40 * 24];
        bytes[..3].copy_from_slice(b"TLD");
        bytes[22..24].copy_from_slice(&8u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(
            tail_info(&bytes).unwrap()["Segments"]
                .as_array()
                .unwrap()
                .len(),
            40
        );
        bytes[22..24].copy_from_slice(&1u16.to_le_bytes());
        assert!(tail_info(&bytes).is_err());
        bytes[22..24].copy_from_slice(&8u16.to_le_bytes());
        bytes.resize(40 + 257 * 24, 0);
        assert!(tail_info(&bytes).is_err());
    }
    #[test]
    fn standalone_tail_offsets_switch_to_external_mip() {
        let mut bytes = vec![0; 64];
        bytes[..3].copy_from_slice(b"TLD");
        bytes[28..32].copy_from_slice(&8u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&16u32.to_le_bytes());
        bytes[52..56].copy_from_slice(&16u32.to_le_bytes());
        let info = tail_info(&bytes).unwrap();
        assert_eq!(info["Segments"][0]["FileOffset"], 40);
        assert_eq!(info["Twiddled"], 1);
    }
    #[test]
    fn malformed_standalone_tail_is_rejected() {
        assert!(tail_info(&[0; 64]).is_err());
    }
}
