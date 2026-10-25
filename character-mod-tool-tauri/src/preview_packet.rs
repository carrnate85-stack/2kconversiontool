use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const MAGIC: &[u8; 8] = b"CMTBIN01";
const HEADER_LIMIT: usize = 4 * 1024 * 1024;
const BODY_LIMIT: usize = 128 * 1024 * 1024;

fn encode(metadata: Value, chunks: &[&[u8]]) -> Result<Vec<u8>> {
    let header = serde_json::to_vec(&metadata)?;
    ensure!(
        header.len() <= HEADER_LIMIT,
        "Preview packet metadata exceeds budget"
    );
    let bytes = chunks
        .iter()
        .try_fold(0usize, |sum, chunk| sum.checked_add(chunk.len()))
        .context("Preview packet size overflow")?;
    ensure!(bytes <= BODY_LIMIT, "Preview packet data exceeds budget");
    let mut packet = Vec::with_capacity(12 + header.len() + bytes);
    packet.extend_from_slice(MAGIC);
    packet.extend_from_slice(&(header.len() as u32).to_le_bytes());
    packet.extend_from_slice(&header);
    for chunk in chunks {
        packet.extend_from_slice(chunk);
    }
    Ok(packet)
}

pub fn archive(
    members: &BTreeMap<String, Vec<u8>>,
    geometry: &str,
    content: &str,
    warnings: &[String],
) -> Result<Vec<u8>> {
    ensure!(members.len() <= 20000, "Too many preview packet members");
    let mut offset = 0usize;
    let rows: Vec<_> = members
        .iter()
        .map(|(name, bytes)| {
            let row = json!({"name":name,"size":bytes.len(),"offset":offset});
            offset += bytes.len();
            row
        })
        .collect();
    encode(
        json!({"packetKind":"archive","members":rows,"geometryKey":geometry,
        "contentKey":content,"warnings":warnings,"native":true}),
        &members.values().map(Vec::as_slice).collect::<Vec<_>>(),
    )
}

pub fn file(bytes: &[u8]) -> Result<Vec<u8>> {
    encode(json!({"packetKind":"file","size":bytes.len()}), &[bytes])
}

pub fn catalog_geometry(
    asset: &Value,
    base: Option<&Value>,
    stats: &Value,
    asset_bytes: &[u8],
    base_bytes: &[u8],
) -> Result<Vec<u8>> {
    ensure!(
        base.is_some() || base_bytes.is_empty(),
        "Unexpected donor geometry bytes"
    );
    encode(
        json!({"packetKind":"catalog-geometry","asset":asset,"base":base,"stats":stats,
        "assetBytes":asset_bytes.len(),"baseBytes":base_bytes.len()}),
        &[asset_bytes, base_bytes],
    )
}

pub fn archive_json(packet: &[u8]) -> Result<Value> {
    ensure!(
        packet.len() >= 12 && &packet[..8] == MAGIC,
        "Invalid preview packet"
    );
    let length = u32::from_le_bytes(packet[8..12].try_into()?) as usize;
    ensure!(
        length <= HEADER_LIMIT && length <= packet.len() - 12,
        "Invalid preview packet header"
    );
    let mut metadata: Value = serde_json::from_slice(&packet[12..12 + length])?;
    ensure!(
        metadata["packetKind"] == "archive",
        "Unexpected preview packet kind"
    );
    let body = &packet[12 + length..];
    let rows = metadata["members"]
        .as_array_mut()
        .context("Missing preview members")?;
    let mut end = 0usize;
    for row in rows {
        let offset = usize::try_from(row["offset"].as_u64().context("Invalid preview offset")?)?;
        let size = usize::try_from(
            row["size"]
                .as_u64()
                .context("Invalid preview member size")?,
        )?;
        ensure!(
            offset == end && size <= body.len().saturating_sub(offset),
            "Invalid preview member range"
        );
        end = offset
            .checked_add(size)
            .context("Preview member overflow")?;
        let fields = row.as_object_mut().context("Invalid preview member")?;
        fields.remove("offset");
        fields.insert(
            "data".into(),
            json!(base64::engine::general_purpose::STANDARD.encode(&body[offset..end])),
        );
    }
    ensure!(end == body.len(), "Trailing preview packet data");
    metadata.as_object_mut().unwrap().remove("packetKind");
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_packet_preserves_binary_bytes_empty_members_and_metadata() {
        let members = BTreeMap::from([
            ("A.SCNE".into(), b"{}".to_vec()),
            ("empty.bin".into(), Vec::new()),
            ("pixels.dds".into(), vec![0, 255, 128, 1]),
        ]);
        let packet = archive(&members, "geometry", "content", &["warning".into()]).unwrap();
        let decoded = archive_json(&packet).unwrap();
        assert_eq!(decoded["geometryKey"], "geometry");
        assert_eq!(decoded["warnings"], json!(["warning"]));
        for row in decoded["members"].as_array().unwrap() {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(row["data"].as_str().unwrap())
                .unwrap();
            assert_eq!(&bytes, &members[row["name"].as_str().unwrap()]);
            assert!(row.get("offset").is_none());
        }
    }
    #[test]
    fn malformed_packets_are_rejected_without_unbounded_reads() {
        assert!(archive_json(b"short").is_err());
        let members = BTreeMap::from([("test".into(), vec![1, 2, 3])]);
        let packet = archive(&members, "", "", &[]).unwrap();
        assert!(archive_json(&packet[..packet.len() - 1]).is_err());
        let mut extra = packet.clone();
        extra.push(4);
        assert!(archive_json(&extra).is_err());
        let mut oversized = packet;
        oversized[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(archive_json(&oversized).is_err());
        assert!(archive_json(&file(b"file").unwrap()).is_err());
    }
    #[test]
    fn geometry_packet_preserves_separate_sections_and_header_metadata() {
        let asset = json!({"meshes":[{"positions":{"kind":"f32","offset":0,"size":4}}]});
        let base = json!({"meshes":[{"sourceVertexIds":{"kind":"u32","offset":0,"size":4}}]});
        let packet = catalog_geometry(
            &asset,
            Some(&base),
            &json!({"headCacheHit":true}),
            &(-0.0f32).to_le_bytes(),
            &65537u32.to_le_bytes(),
        )
        .unwrap();
        let length = u32::from_le_bytes(packet[8..12].try_into().unwrap()) as usize;
        let metadata: Value = serde_json::from_slice(&packet[12..12 + length]).unwrap();
        assert_eq!(metadata["packetKind"], "catalog-geometry");
        assert_eq!(metadata["asset"], asset);
        assert_eq!(metadata["base"], base);
        assert_eq!(metadata["assetBytes"], 4);
        assert_eq!(metadata["baseBytes"], 4);
        assert_eq!(&packet[12 + length..16 + length], &(-0.0f32).to_le_bytes());
        assert_eq!(&packet[16 + length..], &65537u32.to_le_bytes());
        assert!(catalog_geometry(&asset, None, &json!({}), &[], &[1]).is_err());
        assert!(archive_json(&packet).is_err());
    }
}
