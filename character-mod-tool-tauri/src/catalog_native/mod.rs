pub mod frames;
pub mod geometry;

use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};

pub const HEAD_BUDGET: usize = 32 * 1024 * 1024;
const HEAD_LIMIT: usize = 8;
const GEOMETRY_BUDGET: usize = 128 * 1024 * 1024;
const ARRAY_KEYS: &[&str] = &[
    "positions",
    "uvs",
    "attachmentUvs",
    "tangentOffsets",
    "skinWeights",
    "detailUvs",
    "overlayUvs",
    "courtUvs",
    "graffitiUvs",
    "indices",
    "sourceVertexIds",
    "tangentAnchorIds",
    "skinOffsets",
    "skinBones",
];

fn array_kind(key: &str) -> (&'static str, usize) {
    if key == "skinBones" {
        ("u16", 2)
    } else if matches!(
        key,
        "indices" | "sourceVertexIds" | "tangentAnchorIds" | "skinOffsets"
    ) {
        ("u32", 4)
    } else {
        ("f32", 4)
    }
}

pub struct PackedPreview {
    metadata: Value,
    data: Vec<u8>,
}

impl PackedPreview {
    pub fn new(mut metadata: Value) -> Result<Self> {
        let meshes = metadata["meshes"].as_array().context("Missing meshes")?;
        ensure!(meshes.len() <= 1000, "Too many native geometry meshes");
        let mut total = 0usize;
        for mesh in meshes {
            for key in ARRAY_KEYS {
                let Some(values) = mesh.get(*key).filter(|value| !value.is_null()) else {
                    continue;
                };
                let values = values.as_array().context("Invalid native geometry array")?;
                let size = values
                    .len()
                    .checked_mul(array_kind(key).1)
                    .context("Geometry array size overflow")?;
                let padded = size
                    .checked_add(3)
                    .context("Geometry array size overflow")?
                    & !3;
                total = total
                    .checked_add(padded)
                    .context("Geometry data size overflow")?;
                ensure!(
                    total <= GEOMETRY_BUDGET,
                    "Native geometry exceeds packet budget"
                );
            }
        }
        let mut data = Vec::with_capacity(total);
        for mesh in metadata["meshes"].as_array_mut().unwrap() {
            for key in ARRAY_KEYS {
                let Some(values) = mesh.get(*key).filter(|value| !value.is_null()) else {
                    continue;
                };
                let values = values.as_array().context("Invalid native geometry array")?;
                let (kind, _) = array_kind(key);
                let offset = data.len();
                for value in values {
                    match kind {
                        "u16" => data.extend_from_slice(
                            &u16::try_from(value.as_u64().context("Invalid bone index")?)?
                                .to_le_bytes(),
                        ),
                        "u32" => data.extend_from_slice(
                            &u32::try_from(value.as_u64().context("Invalid vertex index")?)?
                                .to_le_bytes(),
                        ),
                        _ => {
                            let value = value.as_f64().context("Invalid vertex component")? as f32;
                            ensure!(value.is_finite(), "Invalid finite game-float component");
                            data.extend_from_slice(&value.to_le_bytes());
                        }
                    }
                }
                let size = data.len() - offset;
                mesh[*key] = json!({"kind":kind,"offset":offset,"size":size});
                while data.len() % 4 != 0 {
                    data.push(0);
                }
            }
        }
        Ok(Self { metadata, data })
    }

    pub fn json(&self) -> Result<Value> {
        let mut metadata = self.metadata.clone();
        for mesh in metadata["meshes"]
            .as_array_mut()
            .context("Missing meshes")?
        {
            for key in ARRAY_KEYS {
                let value = &mesh[*key];
                if value.is_null() {
                    continue;
                }
                let kind = value["kind"]
                    .as_str()
                    .context("Invalid packed array kind")?;
                let offset = usize::try_from(
                    value["offset"]
                        .as_u64()
                        .context("Invalid packed array offset")?,
                )?;
                let size = usize::try_from(
                    value["size"]
                        .as_u64()
                        .context("Invalid packed array size")?,
                )?;
                let end = offset.checked_add(size).context("Packed array overflow")?;
                let bytes = self
                    .data
                    .get(offset..end)
                    .context("Packed array outside buffer")?;
                mesh[*key] = json!({"kind":kind,"data":base64::engine::general_purpose::STANDARD.encode(bytes)});
            }
        }
        Ok(metadata)
    }

    fn bytes(&self) -> Result<usize> {
        Ok(serde_json::to_vec(&self.metadata)?.len() + self.data.len())
    }
}

pub struct CatalogGeometry {
    pub asset: PackedPreview,
    pub head: Option<Arc<Head>>,
    pub stats: Value,
}
impl CatalogGeometry {
    pub fn json(&self) -> Result<Value> {
        Ok(
            json!({"asset":self.asset.json()?,"base":self.head.as_ref().map(|head| head.preview.json()).transpose()?,"stats":self.stats}),
        )
    }
    pub fn packet(&self) -> Result<Vec<u8>> {
        let base = self.head.as_ref().map(|head| &head.preview);
        crate::preview_packet::catalog_geometry(
            &self.asset.metadata,
            base.map(|base| &base.metadata),
            &self.stats,
            &self.asset.data,
            base.map_or(&[], |base| base.data.as_slice()),
        )
    }
}
pub struct Head {
    pub preview: PackedPreview,
    pub frame: frames::Frame,
    bytes: usize,
}
impl Head {
    pub fn new(preview: Value, frame: frames::Frame) -> Result<Self> {
        let preview = PackedPreview::new(preview)?;
        let bytes = preview.bytes()? + frame.bytes();
        ensure!(
            bytes <= HEAD_BUDGET,
            "Donor head exceeds catalog cache budget"
        );
        Ok(Self {
            preview,
            frame,
            bytes,
        })
    }
}

#[derive(Default)]
pub struct Heads {
    entries: HashMap<String, (Arc<Head>, u64)>,
    pub bytes: usize,
    clock: u64,
    pub hits: usize,
    pub builds: usize,
}
impl Heads {
    pub fn get(&mut self, key: &str) -> Option<Arc<Head>> {
        self.clock += 1;
        let (head, touched) = self.entries.get_mut(key)?;
        *touched = self.clock;
        self.hits += 1;
        Some(head.clone())
    }
    pub fn insert(&mut self, key: String, head: Head) -> Arc<Head> {
        if let Some((old, _)) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        while self.entries.len() >= HEAD_LIMIT || self.bytes + head.bytes > HEAD_BUDGET {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, touched))| touched)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else {
                break;
            };
            self.bytes -= self.entries.remove(&oldest).unwrap().0.bytes;
        }
        self.clock += 1;
        self.bytes += head.bytes;
        self.builds += 1;
        let head = Arc::new(head);
        self.entries.insert(key, (head.clone(), self.clock));
        head
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicBool;
    fn head() -> Head {
        let mesh = json!({"positions":[0,0,0,1,0,0,0,1,0],"uvs":[0,0,1,0,0,1],"indices":[0,1,2],"sourceVertexIds":[7,8,9]});
        Head::new(
            json!({"meshes":[mesh]}),
            frames::Frame::new(&mesh, &AtomicBool::new(false)).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn donor_lru_preserves_recent_heads_and_releases_frames() {
        let mut heads = Heads::default();
        for i in 0..8 {
            heads.insert(i.to_string(), head());
        }
        assert!(heads.get("0").is_some());
        heads.insert("8".into(), head());
        assert!(heads.get("1").is_none());
        assert!(heads.get("0").is_some());
        assert_eq!(heads.len(), 8);
        assert!(heads.bytes <= HEAD_BUDGET);
        heads.clear();
        assert_eq!(heads.bytes, 0);
        assert_eq!(heads.len(), 0);
    }
    #[test]
    fn transport_preserves_float_bits_and_original_indices() {
        let preview = PackedPreview::new(json!({"meshes":[{"positions":[-0.0,0.3333333432674408,1.0],"sourceVertexIds":[65537,2,9]}]})).unwrap().json().unwrap();
        let floats = base64::engine::general_purpose::STANDARD
            .decode(preview["meshes"][0]["positions"]["data"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(floats[0..4].try_into().unwrap()),
            (-0.0f32).to_bits()
        );
        assert_eq!(
            f32::from_le_bytes(floats[4..8].try_into().unwrap()),
            0.33333334f32
        );
        let ids = base64::engine::general_purpose::STANDARD
            .decode(
                preview["meshes"][0]["sourceVertexIds"]["data"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(u32::from_le_bytes(ids[0..4].try_into().unwrap()), 65537);
    }

    #[test]
    fn packed_arrays_are_aligned_bounded_and_immutable_across_json_conversion() {
        let packed = PackedPreview::new(
            json!({"meshes":[{"positions":[-0.0,1.5,2.0],"skinBones":[1,65535,7],"indices":[]}]}),
        )
        .unwrap();
        assert_eq!(packed.data.len() % 4, 0);
        assert_eq!(packed.metadata["meshes"][0]["skinBones"]["size"], 6);
        let data = packed.data.clone();
        let first = packed.json().unwrap();
        assert_eq!(packed.json().unwrap(), first);
        assert_eq!(packed.data, data);
        assert!(
            packed.metadata["meshes"][0]["positions"]
                .get("data")
                .is_none()
        );
        for invalid in [
            json!({"skinBones":[65536]}),
            json!({"indices":[4294967296u64]}),
            json!({"positions":[1e100]}),
            json!({"uvs":"not an array"}),
        ] {
            assert!(PackedPreview::new(json!({"meshes":[invalid]})).is_err());
        }
        assert!(PackedPreview::new(json!({"meshes":vec![json!({});1001]})).is_err());
    }
}
