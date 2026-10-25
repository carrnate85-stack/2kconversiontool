// Preview interpretation only. Export retains the original decoded-container bytes.
use anyhow::{Context, Result, bail, ensure};
use nba2k_assets::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    sync::atomic::AtomicBool,
};

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn visit<'a>(
    value: &'a Value,
    path: String,
    out: &mut Vec<(String, &'a Value)>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 128, "Scene nesting exceeds limit");
    if value.is_object() || value.is_array() {
        out.push((path.clone(), value));
    }
    if let Some(map) = value.as_object() {
        for (name, child) in map {
            visit(child, format!("{path}/{name}"), out, depth + 1)?;
        }
    }
    if let Some(arr) = value.as_array() {
        for (index, child) in arr.iter().enumerate() {
            visit(child, format!("{path}[{index}]"), out, depth + 1)?;
        }
    }
    Ok(())
}

struct Attribute {
    offset: usize,
    bits: Vec<usize>,
    kind: String,
    packed: bool,
    scale: Vec<f64>,
    origin: Vec<f64>,
    components: usize,
}
impl Attribute {
    fn new(d: &Value, components: usize) -> Result<Self> {
        let format = text(&d["Format"]);
        let packed = format == "R21G21B22_UINT";
        let (channels, kind) = format.rsplit_once('_').context("Unknown vertex format")?;
        let bits: Vec<usize> = channels
            .split(['R', 'G', 'B', 'A'])
            .filter(|s| !s.is_empty())
            .map(|s| s.parse())
            .collect::<std::result::Result<_, _>>()?;
        ensure!(
            packed && components == 3
                || bits.len() >= components
                    && bits.iter().all(|n| *n == 8 || *n == 16 || *n == 32)
                    && ["UNORM", "SNORM", "FLOAT", "UINT", "SINT"].contains(&kind),
            "Unsupported vertex format {format}"
        );
        let offset = d["ByteOffset"].as_u64().unwrap_or(0) as usize;
        let scale = (0..components)
            .map(|i| d["Scale"][i].as_f64().unwrap_or(1.))
            .collect();
        let origin = (0..components).map(|i| num(&d["Offset"][i])).collect();
        Ok(Self {
            offset,
            bits,
            kind: kind.to_string(),
            packed,
            scale,
            origin,
            components,
        })
    }
    fn read(&self, bytes: &[u8], stride: usize, vertex: usize) -> Result<Vec<f32>> {
        let p = vertex
            .checked_mul(stride)
            .and_then(|v| v.checked_add(self.offset))
            .context("Attribute offset overflow")?;
        let mut raw = Vec::with_capacity(self.components);
        if self.packed {
            let packed = codec::u64_at(bytes, p)?;
            raw.extend([
                (packed & 0x1fffff) as f64,
                ((packed >> 21) & 0x1fffff) as f64,
                ((packed >> 42) & 0x3fffff) as f64,
            ]);
        } else {
            let mut cursor = p;
            for width in self.bits.iter().take(self.components) {
                let value = match (*width, self.kind.as_str()) {
                    (8, "UINT") => *bytes.get(cursor).context("Truncated component")? as f64,
                    (8, "SINT") => *bytes.get(cursor).context("Truncated component")? as i8 as f64,
                    (8, "UNORM") => {
                        *bytes.get(cursor).context("Truncated component")? as f64 / 255.
                    }
                    (8, "SNORM") => {
                        (*bytes.get(cursor).context("Truncated component")? as i8 as f64 / 127.)
                            .max(-1.)
                    }
                    (16, "UINT") => codec::u16_at(bytes, cursor)? as f64,
                    (16, "SINT") => codec::u16_at(bytes, cursor)? as i16 as f64,
                    (32, "UINT") => codec::u32_at(bytes, cursor)? as f64,
                    (32, "SINT") => codec::u32_at(bytes, cursor)? as i32 as f64,
                    (16, "UNORM") => codec::u16_at(bytes, cursor)? as f64 / 65535.,
                    (16, "SNORM") => {
                        (codec::u16_at(bytes, cursor)? as i16 as f64 / 32767.).max(-1.)
                    }
                    (16, "FLOAT") => half::f16::from_bits(codec::u16_at(bytes, cursor)?).to_f64(),
                    (32, "FLOAT") => f32::from_bits(codec::u32_at(bytes, cursor)?) as f64,
                    (32, "UNORM") => codec::u32_at(bytes, cursor)? as f64 / u32::MAX as f64,
                    (32, "SNORM") => {
                        (codec::u32_at(bytes, cursor)? as i32 as f64 / i32::MAX as f64).max(-1.)
                    }
                    _ => bail!("Unsupported component"),
                };
                raw.push(value);
                cursor += *width / 8;
            }
        }
        raw.iter()
            .enumerate()
            .map(|(i, v)| {
                let x = v * self.scale[i] + self.origin[i];
                ensure!(
                    x.is_finite() && x.abs() <= f32::MAX as f64,
                    "Invalid attribute value"
                );
                Ok(x as f32)
            })
            .collect()
    }
}
#[cfg(test)]
fn attribute(
    bytes: &[u8],
    stride: usize,
    vertex: usize,
    d: &Value,
    components: usize,
) -> Result<Vec<f32>> {
    Attribute::new(d, components)?.read(bytes, stride, vertex)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_coordinates_keep_scale_and_offset() {
        let packed = 2u64 | (3u64 << 21) | (4u64 << 42);
        let descriptor = json!({"Format":"R21G21B22_UINT","Scale":[2,3,4],"Offset":[1,2,3]});
        assert_eq!(
            attribute(&packed.to_le_bytes(), 8, 0, &descriptor, 3).unwrap(),
            vec![5., 11., 19.]
        );
    }
    #[test]
    fn truncated_attributes_are_rejected() {
        let descriptor = json!({"Format":"R16G16_FLOAT"});
        assert!(attribute(&[0, 0, 0], 4, 0, &descriptor, 2).is_err());
        assert!(attribute(&[0; 4], 4, usize::MAX, &descriptor, 2).is_err());
    }
}

struct Buffers {
    values: HashMap<String, Vec<u8>>,
    bytes: usize,
}
impl Buffers {
    fn read(
        &mut self,
        archive: &mut Archive,
        catalog: &Catalog,
        oodle: &Oodle,
        node: &Value,
    ) -> Result<Vec<u8>> {
        let name = text(&node["Binary"]);
        let expected = node["Size"].as_u64().unwrap_or(0) as usize;
        ensure!(expected <= codec::MAX, "SCNE buffer size exceeds limit");
        let key = format!("{name}:{expected}");
        if let Some(hit) = self.values.get(&key) {
            return Ok(hit.clone());
        }
        let bytes = archive
            .named(name)
            .or_else(|_| codec::range(catalog, catalog.resolve(name)?))?;
        let out = if codec::vcz(&bytes) {
            ensure!(expected > 0, "VCZ buffer needs SCNE Size");
            oodle.buffer(&bytes, expected)?
        } else if bytes.starts_with(&[0x1f, 0x8b, 8]) {
            ensure!(expected > 0, "GZIP buffer needs SCNE Size");
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&bytes[..])
                .take(expected as u64 + 1)
                .read_to_end(&mut out)?;
            out
        } else {
            bytes
        };
        ensure!(
            expected == 0 || out.len() == expected,
            "Geometry buffer length does not match SCNE Size: {name}"
        );
        if self.bytes + out.len() <= 64 * 1024 * 1024 {
            self.bytes += out.len();
            self.values.insert(key, out.clone());
        }
        Ok(out)
    }
}

fn streams(model: &Value) -> Vec<&Value> {
    if let Some(a) = model["VertexStream"].as_array() {
        return a.iter().collect();
    }
    if model["VertexStream"]["VertexBuffer"].is_object() {
        return vec![&model["VertexStream"]["VertexBuffer"]];
    }
    model["VertexStream"]
        .as_object()
        .map(|m| m.values().filter(|v| v["Binary"].is_string()).collect())
        .unwrap_or_default()
}
fn mesh(
    archive: &mut Archive,
    catalog: &Catalog,
    oodle: &Oodle,
    buffers: &mut Buffers,
    model: &Value,
    path: &str,
    budget: usize,
) -> Result<(Value, usize)> {
    let position = &model["VertexFormat"]["POSITION0"];
    let tangent = text(&position["Format"]) == "R16G16B16A16_UINT"
        && model["VertexFormat"]["POSITION1"].is_object()
        && model["VertexFormat"]["TEXCOORD0"].is_object()
        && model["VertexFormat"]["TEXCOORD1"].is_object();
    let streams = streams(model);
    let ps = streams
        .get(num(&position["Stream"]) as usize)
        .context("Missing position stream")?;
    let stride = num(&ps["Stride"]) as usize;
    ensure!(stride > 0 && stride <= 1024, "Invalid stride");
    let positions = buffers.read(archive, catalog, oodle, ps)?;
    let ib = buffers.read(archive, catalog, oodle, &model["IndexBuffer"])?;
    let index_stride = match text(&model["IndexBuffer"]["Format"]) {
        "R16_UINT" => 2,
        "R32_UINT" => 4,
        _ => bail!("Unsupported index format"),
    };
    ensure!(ib.len() % index_stride == 0, "Truncated index buffer");
    let mut source_indices = Vec::with_capacity(ib.len() / index_stride);
    for p in (0..ib.len()).step_by(index_stride) {
        source_indices.push(if index_stride == 2 {
            codec::u16_at(&ib, p)? as usize
        } else {
            codec::u32_at(&ib, p)? as usize
        });
    }
    let vertex_count = positions.len() / stride;
    let mut uv_semantic = if tangent { "TEXCOORD1" } else { "TEXCOORD0" };
    if tangent {
        let mut best = -1.0f32;
        for semantic in ["TEXCOORD1", "TEXCOORD0"] {
            let descriptor = &model["VertexFormat"][semantic];
            let stream = streams
                .get(num(&descriptor["Stream"]) as usize)
                .context("Missing UV stream")?;
            let data = buffers.read(archive, catalog, oodle, stream)?;
            let reader = Attribute::new(descriptor, 2)?;
            let samples = vertex_count.min(128);
            let mut low = [f32::INFINITY; 2];
            let mut high = [f32::NEG_INFINITY; 2];
            for sample in 0..samples {
                let vertex = if samples == 1 {
                    0
                } else {
                    sample * (vertex_count - 1) / (samples - 1)
                };
                let xy = reader.read(&data, num(&stream["Stride"]) as usize, vertex)?;
                for axis in 0..2 {
                    low[axis] = low[axis].min(xy[axis]);
                    high[axis] = high[axis].max(xy[axis]);
                }
            }
            let score = if samples == 0 {
                -1.0
            } else {
                high[0] - low[0] + high[1] - low[1]
            };
            if score > best {
                best = score;
                uv_semantic = semantic;
            }
        }
    }
    let uv = &model["VertexFormat"][uv_semantic];
    let uv_stream = streams.get(num(&uv["Stream"]) as usize);
    let uv_bytes = if uv.is_object() {
        Some(buffers.read(
            archive,
            catalog,
            oodle,
            uv_stream.context("Missing UV stream")?,
        )?)
    } else {
        None
    };
    let position_reader = Attribute::new(position, 3)?;
    let uv_reader = if uv_bytes.is_some() {
        Some(Attribute::new(uv, 2)?)
    } else {
        None
    };
    // Preserve authored extra UV sets for floor logos, detail layers and overlays.
    let floor = path.to_lowercase().contains("floor_") && path.to_lowercase().contains("court");
    let mut extra_uvs = BTreeMap::new();
    for (key, semantic, enabled) in [
        ("courtUvs", "TEXCOORD2", floor),
        ("detailUvs", "TEXCOORD2", true),
        ("overlayUvs", "TEXCOORD3", true),
        ("graffitiUvs", "TEXCOORD1", floor),
        (
            "attachmentUvs",
            if uv_semantic == "TEXCOORD0" {
                "TEXCOORD1"
            } else {
                "TEXCOORD0"
            },
            tangent,
        ),
    ] {
        let descriptor = &model["VertexFormat"][semantic];
        if enabled && descriptor.is_object() {
            let stream = streams
                .get(num(&descriptor["Stream"]) as usize)
                .context("Missing extra UV stream")?;
            let data = buffers.read(archive, catalog, oodle, stream)?;
            extra_uvs.insert(
                key,
                (
                    Attribute::new(descriptor, 2)?,
                    data,
                    num(&stream["Stride"]) as usize,
                    Vec::<f32>::new(),
                ),
            );
        }
    }
    let primitives = model["Prim"].as_array().context("No primitives")?;
    let (mut cursor, mut inherited, mut triangles) = (0usize, 0usize, 0usize);
    let (mut output, mut uvs, mut indices, mut groups) = (
        Vec::<f32>::new(),
        Vec::<f32>::new(),
        Vec::<u32>::new(),
        Vec::<Value>::new(),
    );
    let mut remap = HashMap::new();
    let mut source_ids = Vec::new();
    let mut offsets = Vec::<f32>::new();
    let mut anchors = Vec::<u32>::new();
    let mut material = "Unassigned";
    for prim in primitives {
        if let Some(count) = prim["Count"].as_u64() {
            inherited = count as usize;
        }
        if let Some(name) = prim["Material"].as_str() {
            material = name;
        }
        // Use highest available LOD; do not apply byte offsets/base vertices to global indices.
        let range = prim["LodList"]
            .as_array()
            .and_then(|lods| lods.iter().find(|l| l["Count"].as_u64().unwrap_or(0) >= 3));
        let start = range.unwrap_or(prim)["Start"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(cursor);
        let count = range.unwrap_or(prim)["Count"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(inherited);
        cursor = start
            .checked_add(count)
            .context("Primitive range overflow")?;
        ensure!(
            start <= source_indices.len() && count <= source_indices.len() - start,
            "Primitive range exceeds index buffer"
        );
        ensure!(count % 3 == 0, "Incomplete triangle primitive");
        ensure!(
            count / 3 <= budget.saturating_sub(triangles),
            "Complete mesh exceeds remaining preview budget; no partial geometry displayed"
        );
        let group_start = indices.len();
        for source in &source_indices[start..start + count] {
            ensure!(
                *source < vertex_count,
                "Index {source} exceeds vertex count {vertex_count}"
            );
            let target = if let Some(existing) = remap.get(source) {
                *existing
            } else {
                let xyz = if tangent {
                    vec![0.; 3]
                } else {
                    position_reader.read(&positions, stride, *source)?
                };
                let new = (output.len() / 3) as u32;
                output.extend([xyz[0], -xyz[2], xyz[1]]);
                source_ids.push(*source as u32);
                if tangent {
                    let p = source
                        .checked_mul(stride)
                        .and_then(|p| p.checked_add(position_reader.offset))
                        .context("Tangent offset overflow")?;
                    for axis in 0..3 {
                        let value =
                            half::f16::from_bits(codec::u16_at(&positions, p + axis * 2)?).to_f32();
                        ensure!(value.is_finite(), "Nonfinite tangent attachment offset");
                        offsets.push(value);
                    }
                    anchors.push(codec::u16_at(&positions, p + 6)? as u32);
                }
                if let Some(uv_bytes) = uv_bytes.as_ref() {
                    let xy = uv_reader.as_ref().unwrap().read(
                        uv_bytes,
                        num(&uv_stream.unwrap()["Stride"]) as usize,
                        *source,
                    )?;
                    uvs.extend(xy);
                } else {
                    uvs.extend([0., 0.]);
                }
                for (reader, data, stride, values) in extra_uvs.values_mut() {
                    values.extend(reader.read(data, *stride, *source)?);
                }
                remap.insert(*source, new);
                new
            };
            indices.push(target);
        }
        if count > 0 {
            groups.push(
                json!({"start":group_start,"count":count,"material":material,"mesh":prim["Mesh"]}),
            );
            triangles += count / 3;
        }
    }
    ensure!(!indices.is_empty(), "No triangle-list primitives");
    let mut result = json!({"tangentSpaceAttachment":tangent,"name":path.rsplit('/').next().unwrap_or(path),"positions":output,"indices":indices,"uvs":uvs,"groups":groups,"sourceVertexIds":source_ids,"tangentOffsets":offsets,"tangentAnchorIds":anchors});
    result["textureUvSemantic"] = json!(uv_reader.as_ref().map(|_| uv_semantic));
    for (key, (_, _, _, values)) in extra_uvs {
        if key == "attachmentUvs" {
            // This is the alternate stream, not necessarily TEXCOORD0.
            result["attachmentUvSemantic"] = json!(if uv_semantic == "TEXCOORD0" {
                "TEXCOORD1"
            } else {
                "TEXCOORD0"
            });
        }
        result[key] = json!(values);
    }
    Ok((result, triangles))
}

pub fn build(
    archive: &mut Archive,
    catalog: &Catalog,
    oodle: &Oodle,
    file: &str,
    head_only: bool,
    cancel: &AtomicBool,
) -> Result<Value> {
    let mut buffers = Buffers {
        values: HashMap::new(),
        bytes: 0,
    };
    let mut meshes = Vec::new();
    let mut scenes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut triangles = 0usize;
    for member in archive.names()? {
        let name = text(&member["name"]);
        if !name.to_lowercase().ends_with(".scne") {
            continue;
        }
        codec::cancelled(cancel)?;
        let scene = codec::tree(&archive.read(
            member["id"].as_u64().context("Missing scene id")? as usize,
            64 * 1024 * 1024,
        )?)?;
        scenes.push(name.to_owned());
        let mut nodes = Vec::new();
        visit(&scene, name.to_owned(), &mut nodes, 0)?;
        for (path, model) in nodes {
            if !model["IndexBuffer"]["Binary"].is_string()
                || !model["VertexFormat"]["POSITION0"].is_object()
                || !model["Prim"].is_array()
            {
                continue;
            }
            if head_only
                && !model["Prim"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| super::frames::head_group(p))
            {
                continue;
            }
            let signature = serde_json::to_string(
                &json!({"index":model["IndexBuffer"],"streams":model["VertexStream"],"format":model["VertexFormat"],"prims":model["Prim"]}),
            )?;
            if !seen.insert(signature.clone()) {
                continue;
            }
            codec::cancelled(cancel)?;
            ensure!(
                meshes.len() < 80 && triangles < 350000,
                "Catalog geometry exceeds preview budget"
            );
            let (mut decoded, _) = mesh(
                archive,
                catalog,
                oodle,
                &mut buffers,
                model,
                &path,
                350000 - triangles,
            )?;
            if head_only {
                let Some(head) = super::frames::fit_head(&decoded)? else {
                    continue;
                };
                decoded = head;
            }
            for group in decoded["groups"]
                .as_array_mut()
                .context("Missing mesh groups")?
            {
                if group["mesh"].is_null() {
                    group["mesh"] = json!("");
                }
            }
            decoded["sourceName"] = json!(file.rsplit('/').next().unwrap_or(file));
            decoded["sourceKind"] = json!("main");
            decoded["geometryKey"] = json!(format!(
                "{}:{:x}",
                file,
                Sha256::digest(signature.as_bytes())
            ));
            decoded["bones"] = json!([]);
            decoded["skinOffsets"] = json!(vec![
                0u32;
                decoded["positions"].as_array().unwrap().len() / 3
                    + 1
            ]);
            decoded["skinBones"] = json!([]);
            decoded["skinWeights"] = json!([]);
            triangles += decoded["indices"]
                .as_array()
                .context("Missing indices")?
                .len()
                / 3;
            meshes.push(decoded);
        }
    }
    ensure!(
        !meshes.is_empty(),
        "No compatible highest-LOD catalog geometry found"
    );
    Ok(
        json!({"file":file,"scenes":scenes,"meshes":meshes,"textures":[],"skipped":[],"stats":{"modelsDecoded":seen.len(),"triangles":triangles,"companions":0}}),
    )
}
