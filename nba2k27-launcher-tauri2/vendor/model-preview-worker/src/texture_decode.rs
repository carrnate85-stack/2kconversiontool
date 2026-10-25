//! App-local compatibility routing; shared decoder sources are not modified.
use crate::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
fn collect(value: &Value, out: &mut BTreeMap<String, Value>, depth: usize) -> Result<()> {
    ensure!(depth <= 128, "Texture metadata nesting exceeds limit");
    if let Some(map) = value.as_object() {
        if let Some(name) = map
            .get("Binary")
            .and_then(Value::as_str)
            .filter(|n| n.to_lowercase().ends_with(".tld") || n.to_lowercase().ends_with(".dds"))
        {
            out.insert(name.into(), value.clone());
        }
        for child in map.values() {
            collect(child, out, depth + 1)?;
        }
    }
    if let Some(array) = value.as_array() {
        for child in array {
            collect(child, out, depth + 1)?;
        }
    }
    Ok(())
}
pub fn descriptors(archive: &mut Archive, cancel: &AtomicBool) -> Result<BTreeMap<String, Value>> {
    archive.require_unique_members()?;
    let mut out = BTreeMap::new();
    for member in archive.names()? {
        codec::cancelled(cancel)?;
        let name = member["name"]
            .as_str()
            .context("Missing texture member name")?;
        let lower = name.to_lowercase();
        if lower.ends_with(".scne") || lower.ends_with(".txtr") {
            let bytes = archive.read(
                member["id"].as_u64().context("Invalid member id")? as usize,
                64 * 1024 * 1024,
            )?;
            if let Ok(tree) = codec::tree(&bytes) {
                collect(&tree, &mut out, 0)?;
            }
        }
        if lower.ends_with(".dds") || lower.ends_with(".tld") {
            out.entry(name.into()).or_insert(Value::Null);
        }
    }
    Ok(out)
}

/// A standalone TLD can author one inline Oodle stream in its own header.
/// Exact container length is essential: a following segment table is not pixels.
fn inline_info(bytes: &[u8], info: &Value, limit: usize) -> Result<Option<Value>> {
    if bytes.len() < 32
        || !bytes.starts_with(b"TLD")
        || info["Segments"].as_array().is_some_and(|a| !a.is_empty())
    {
        return Ok(None);
    }
    let raw = codec::u32_at(bytes, 24)? as usize;
    let packed = codec::u32_at(bytes, 28)? as usize;
    let authored = info["PixelDataSize"]
        .as_u64()
        .or_else(|| info["Size"].as_u64())
        .filter(|n| *n > 0);
    if raw == 0
        || packed == 0
        || 32usize.checked_add(packed) != Some(bytes.len())
        || authored.is_some_and(|n| n != raw as u64)
        || (authored.is_none() && packed >= raw)
    {
        return Ok(None);
    }
    ensure!(
        raw <= limit && limit <= codec::MAX,
        "Texture exceeds decoding budget"
    );
    let mut result = if info.is_object() {
        info.clone()
    } else {
        json!({})
    };
    result["Segments"] = json!([{"FileOffset":32,"FileSize":packed,"MemoryOffset":0,"MemorySize":raw,"InflatedSize":if packed==raw{0}else{raw}}]);
    Ok(Some(result))
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
    archive.require_unique_members()?;
    if archive.needs_name_adapter() {
        let mip = format!("{}.mip0", name.strip_suffix(".tld").unwrap_or(name));
        let mut input = archive.decoder_input(&[name, &mip], cancel)?;
        return reconstruct(&mut input, catalog, oodle, name, info, cancel, limit);
    }
    // SCNE/TXTR metadata and standalone TLD headers describe the same exact
    // single-stream layout. Both go through the existing strict segment decoder.
    if info["Segments"].as_array().is_none_or(|a| a.is_empty()) {
        let bytes = archive
            .named(name)
            .or_else(|_| codec::range(catalog, catalog.resolve(name)?))?;
        if let Some(authored) = inline_info(&bytes, info, limit)? {
            return nba2k_assets::textures::reconstruct(
                archive.shared()?,
                catalog,
                oodle,
                name,
                &authored,
                cancel,
                limit,
            );
        }
    }
    nba2k_assets::textures::reconstruct(
        archive.shared()?,
        catalog,
        oodle,
        name,
        info,
        cancel,
        limit,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tld(raw: u32, packed: u32) -> Vec<u8> {
        let mut bytes = vec![0; 32 + packed as usize];
        bytes[..3].copy_from_slice(b"TLD");
        bytes[24..28].copy_from_slice(&raw.to_le_bytes());
        bytes[28..32].copy_from_slice(&packed.to_le_bytes());
        bytes
    }
    #[test]
    fn standalone_inline_uses_exact_authored_header_not_size_guessing() {
        let value = inline_info(&tld(4096, 64), &Value::Null, 8192)
            .unwrap()
            .unwrap();
        assert_eq!(
            value["Segments"],
            json!([{"FileOffset":32,"FileSize":64,"MemoryOffset":0,"MemorySize":4096,"InflatedSize":4096}])
        );
        assert!(
            inline_info(&tld(64, 64), &Value::Null, 8192)
                .unwrap()
                .is_none()
        );
        assert!(
            inline_info(&tld(64, 65), &Value::Null, 8192)
                .unwrap()
                .is_none()
        );
        assert!(
            inline_info(&tld(0, 0), &Value::Null, 8192)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn tail_tables_and_partial_inline_streams_are_never_flattened() {
        let bytes = tld(4096, 64);
        let mut tail = bytes.clone();
        tail.extend([0; 24]);
        assert!(inline_info(&tail, &Value::Null, 8192).unwrap().is_none());
        assert!(
            inline_info(&bytes[..bytes.len() - 1], &Value::Null, 8192)
                .unwrap()
                .is_none()
        );
        assert!(
            inline_info(&bytes, &json!({"Segments":[{"FileOffset":32}]}), 8192)
                .unwrap()
                .is_none()
        );
        assert!(
            inline_info(b"not a TLD", &Value::Null, 8192)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn explicit_ranges_remain_authored_and_conflicts_are_not_overridden() {
        let bytes = tld(4096, 64);
        assert!(
            inline_info(&bytes, &json!({"PixelDataSize":4095}), 8192)
                .unwrap()
                .is_none()
        );
        let info = inline_info(&bytes, &json!({"PixelDataSize":4096,"Twiddled":1}), 8192)
            .unwrap()
            .unwrap();
        assert_eq!(info["Twiddled"], 1);
        let plain = inline_info(&tld(64, 64), &json!({"Size":64}), 8192)
            .unwrap()
            .unwrap();
        assert_eq!(plain["Segments"][0]["InflatedSize"], 0);
    }
    #[test]
    fn standalone_inline_enforces_both_caller_and_shared_allocation_limits() {
        let bytes = tld(4096, 64);
        assert!(inline_info(&bytes, &Value::Null, 4095).is_err());
        assert!(inline_info(&bytes, &Value::Null, codec::MAX + 1).is_err());
        assert!(inline_info(&bytes, &Value::Null, 4096).unwrap().is_some());
    }
}
pub fn one(
    archive: &mut Archive,
    catalog: &Catalog,
    name: &str,
    cancel: &AtomicBool,
) -> Result<Vec<u8>> {
    let descriptors = descriptors(archive, cancel)?;
    let (name, info) = descriptors
        .iter()
        .find(|(n, _)| {
            n.eq_ignore_ascii_case(name)
                || (n.to_lowercase().ends_with(".tld")
                    && format!("{}.dds", &n[..n.len() - 4]).eq_ignore_ascii_case(name))
        })
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
    mut decorate: impl FnMut(&str, &Value, &mut Archive, &mut Value),
) -> Result<Value> {
    let descriptors = descriptors(archive, cancel)?;
    let oodle = Oodle::load(&catalog.root)?;
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    let mut total = 0;
    for (name, info) in descriptors {
        codec::cancelled(cancel)?;
        if items.len() >= 256 || total >= 96 * 1024 * 1024 {
            skipped.push(json!({"name":name,"reason":"Gallery budget: 256 items / 96 MiB; exports unaffected"}));
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
                if total + dds.len() > 96 * 1024 * 1024 {
                    skipped.push(
                        json!({"name":name,"reason":"Gallery memory budget; exports unaffected"}),
                    );
                    continue;
                }
                total += dds.len();
                let mut item = crate::backend::texture(&name, &dds);
                item["resourceName"] = json!(name);
                if name.to_lowercase().ends_with(".tld") {
                    item["name"] = json!(format!("{}.dds", &name[..name.len() - 4]));
                }
                decorate(&name, &info, archive, &mut item);
                items.push(item);
            }
            Err(e) => skipped.push(json!({"name":name,"reason":format!("{e:#}")})),
        }
    }
    Ok(json!({"items":items,"skipped":skipped}))
}
