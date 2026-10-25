// Preview interpretation only. Export retains the original decoded-container bytes.
use crate::codec::{self, Archive, Oodle};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
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

// Only suppress explicitly authored alternatives whose primary model exists in
// the same scene scope. A similar filename elsewhere is not an LOD relationship.
fn alternate_lod_paths(nodes: &[(String, &Value)]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for (path, scope) in nodes {
        let (Some(models), Some(objects)) =
            (scope["Model"].as_object(), scope["Object"].as_object())
        else {
            continue;
        };
        let targets: std::collections::HashSet<_> = objects
            .values()
            .filter_map(|object| object["Target"].as_str())
            .collect();
        for object in objects.values() {
            let Some(target) = object["Target"].as_str() else {
                continue;
            };
            let Some(primary) = models.get(target) else {
                continue;
            };
            if !primary["VertexFormat"]["POSITION0"].is_object() {
                continue;
            }
            for lod in object["Lods"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if models.contains_key(lod) && !targets.contains(lod) {
                    out.insert(format!("{path}/Model/{lod}"));
                }
            }
        }
    }
    out
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

struct Buffers {
    values: HashMap<String, Vec<u8>>,
    bytes: usize,
}
impl Buffers {
    fn read(
        &mut self,
        archive: &mut Archive,
        catalog: &crate::catalog::Catalog,
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

fn legacy_model(
    archive: &mut Archive,
    buffers: &mut Buffers,
    model: &Value,
) -> Result<Option<Value>> {
    if model["IndexBuffer"]["Binary"].is_string() || !model["Binary"].is_string() {
        return Ok(None);
    }
    let source_name = text(&model["Binary"]);
    let bytes = archive
        .named(source_name)
        .with_context(|| format!("Reading legacy model {source_name}"))?;
    ensure!(
        bytes.len() >= 32
            && codec::u32_at(&bytes, 0)? == 0xb2463c45
            && codec::u32_at(&bytes, 4)? as usize == bytes.len(),
        "Invalid legacy model header"
    );
    let formats = model["VertexFormat"]
        .as_object()
        .context("Legacy model has no vertex format")?;
    let expected_sections = formats.len() + 1;
    let mut sections = Vec::<(usize, Vec<u8>)>::with_capacity(expected_sections);
    let mut cursor = 8usize;
    while sections.len() < expected_sections {
        ensure!(
            cursor + 24 <= bytes.len() && codec::u32_at(&bytes, cursor)? == 0x0011799a,
            "Invalid legacy model buffer section"
        );
        let length = codec::u32_at(&bytes, cursor + 4)? as usize;
        let end = cursor
            .checked_add(length)
            .context("Legacy model section overflow")?;
        ensure!(
            length >= 24 && end <= bytes.len(),
            "Truncated legacy model section"
        );
        let metadata = codec::u32_at(&bytes, cursor + 20)?;
        let stride = ((metadata >> 16) & 0xff) as usize;
        ensure!(stride > 0 && stride <= 64, "Invalid legacy model stride");
        let payload = bytes[cursor + 24..end].to_vec();
        ensure!(
            payload.len() % stride == 0,
            "Misaligned legacy model buffer"
        );
        sections.push((stride, payload));
        cursor = end;
    }
    ensure!(
        sections.len() == expected_sections,
        "Legacy model buffer count does not match vertex format"
    );
    let mut normalized = model.clone();
    let mut vertex_streams = Vec::with_capacity(formats.len());
    let (index_stride, index_bytes) = &sections[0];
    let index_name = format!("__legacy/{source_name}/indices");
    buffers.values.insert(
        format!("{index_name}:{}", index_bytes.len()),
        index_bytes.clone(),
    );
    buffers.bytes = buffers
        .bytes
        .checked_add(index_bytes.len())
        .context("Legacy preview buffer overflow")?;
    normalized["IndexBuffer"] = json!({
        "Binary":index_name,
        "Size":index_bytes.len(),
        "Format":match index_stride { 2 => "R16_UINT", 4 => "R32_UINT", _ => bail!("Unsupported legacy index stride") }
    });
    let format_names = formats.keys().cloned().collect::<Vec<_>>();
    for (index, semantic) in format_names.iter().enumerate() {
        let (stride, payload) = &sections[index + 1];
        let name = format!("__legacy/{source_name}/{semantic}");
        buffers
            .values
            .insert(format!("{name}:{}", payload.len()), payload.clone());
        buffers.bytes = buffers
            .bytes
            .checked_add(payload.len())
            .context("Legacy preview buffer overflow")?;
        ensure!(
            buffers.bytes <= 64 * 1024 * 1024,
            "Legacy preview buffers exceed 64 MiB"
        );
        normalized["VertexFormat"][semantic]["Stream"] = json!(index);
        normalized["VertexFormat"][semantic]["ByteOffset"] = json!(0);
        vertex_streams.push(json!({"Binary":name,"Size":payload.len(),"Stride":stride}));
    }
    normalized["VertexStream"] = json!(vertex_streams);
    ensure!(
        model["Prim"].is_array(),
        "Legacy model has no ordered primitives"
    );
    Ok(Some(normalized))
}

fn mesh(
    archive: &mut Archive,
    catalog: &crate::catalog::Catalog,
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
    let uv = &model["VertexFormat"][if tangent { "TEXCOORD1" } else { "TEXCOORD0" }];
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
        ("attachmentUvs", "TEXCOORD0", tangent),
        // Legacy player heads use TEXCOORD0 for the body atlases and
        // TEXCOORD1 for the normalized facial atlas.
        ("secondaryUvs", "TEXCOORD1", !tangent),
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
    // Some tangent-space hair has an unused, entirely zero TEXCOORD1. Keep
    // valid secondary UVs, but use the populated authored UV0 for that layout.
    // This affects only the in-memory preview, never the exported buffers.
    let fallback = use_attachment_texture_uvs(
        tangent,
        &uvs,
        extra_uvs
            .get("attachmentUvs")
            .map(|entry| entry.3.as_slice()),
    );
    if fallback {
        uvs.clone_from(&extra_uvs["attachmentUvs"].3);
    }
    let semantic = if tangent && !fallback {
        "TEXCOORD1"
    } else {
        "TEXCOORD0"
    };
    let mut result = json!({"name":path.rsplit('/').next().unwrap_or(path),"positions":output,"indices":indices,"uvs":uvs,"textureUvSemantic":semantic,"groups":groups,"sourceVertexIds":source_ids,"tangentOffsets":offsets,"tangentAnchorIds":anchors});
    for (key, (_, _, _, values)) in extra_uvs {
        result[key] = json!(values);
    }
    Ok((result, triangles))
}

fn use_attachment_texture_uvs(tangent: bool, primary: &[f32], attachment: Option<&[f32]>) -> bool {
    let Some(attachment) = attachment else {
        return false;
    };
    tangent
        && primary.len() >= 6
        && primary.len() % 2 == 0
        && primary.iter().all(|v| *v == 0.)
        && attachment.len() == primary.len()
        && attachment.iter().all(|v| v.is_finite())
        && attachment
            .chunks_exact(2)
            .any(|uv| (uv[0] - attachment[0]).abs() > 1e-6 || (uv[1] - attachment[1]).abs() > 1e-6)
}

fn direct_placement_target(path: &str, placement: &Value) -> bool {
    let normalize = |value: &str| value.replace('\\', "/").trim().to_lowercase();
    let path = normalize(path);
    let target = normalize(text(&placement["target"]));
    if path.is_empty() || target.is_empty() {
        return false;
    }
    if path == target {
        return true;
    }
    let leaf = |value: &str| value.rsplit(['/', ':']).next().unwrap_or(value).to_owned();
    leaf(&path) == leaf(&target)
}

fn selected_placements(
    path: &str,
    candidates: &[String],
    placed: &HashMap<String, Vec<Value>>,
) -> Option<Vec<Value>> {
    let found = candidates.iter().find_map(|key| placed.get(key))?;
    let direct: Vec<_> = found
        .iter()
        .filter(|placement| direct_placement_target(path, placement))
        .cloned()
        .collect();
    let selected = if direct.is_empty() {
        found.clone()
    } else {
        direct
    };
    let mut matrices = std::collections::HashSet::new();
    Some(
        selected
            .into_iter()
            .filter(|placement| {
                serde_json::to_string(&placement["matrix"])
                    .map(|matrix| matrices.insert(matrix))
                    .unwrap_or(false)
            })
            .collect(),
    )
}

#[cfg(test)]
fn placements(scene: &Value) -> Result<HashMap<String, Vec<Value>>> {
    let mut nodes = Vec::new();
    visit(scene, String::new(), &mut nodes, 0)?;
    let mut out: HashMap<String, Vec<Value>> = HashMap::new();
    for (_, root) in nodes {
        let Some(objects) = root["Object"].as_object() else {
            continue;
        };
        for (name, obj) in objects {
            let Some(matrix) = obj["Matrix"].as_array() else {
                continue;
            };
            if matrix.len() < 16 {
                continue;
            }
            let target = text(&obj["Target"]);
            if target.is_empty() {
                continue;
            }
            // Complex articulated/pivot transforms are explicitly not guessed by this prototype.
            if text(&obj["Transform"]).contains('|') {
                continue;
            }
            let m: Vec<f64> = matrix.iter().take(16).map(num).collect();
            let converted = vec![
                m[0], -m[2], m[1], 0., -m[8], m[10], -m[9], 0., m[4], -m[6], m[5], 0., m[12],
                -m[14], m[13], 1.,
            ];
            let place = json!({"name":name,"target":target,"matrix":converted});
            let key = target.to_lowercase();
            let leaf = target.rsplit('/').next().unwrap_or(target).to_lowercase();
            out.entry(key.clone()).or_default().push(place.clone());
            if leaf != key {
                out.entry(leaf).or_default().push(place);
            }
        }
    }
    Ok(out)
}

pub fn build(
    archive: &mut Archive,
    catalog: &crate::catalog::Catalog,
    oodle: &Oodle,
    file: &str,
    cancel: &AtomicBool,
) -> Result<Value> {
    archive.require_unique_members()?;
    let members = archive.names()?;
    let mut buffers = Buffers {
        values: HashMap::new(),
        bytes: 0,
    };
    let (mut meshes, mut skipped, mut scenes, mut triangles, mut found) =
        (Vec::<Value>::new(), Vec::new(), Vec::new(), 0usize, 0usize);
    let mut authored_objects = std::collections::HashSet::new();
    let arena = file.to_lowercase().contains("arena_");
    let mut usage: BTreeMap<String, usize> = BTreeMap::new();
    let mut unique = HashMap::<String, usize>::new();
    for member in members
        .iter()
        .filter(|m| text(&m["name"]).to_lowercase().ends_with(".scne"))
    {
        codec::cancelled(cancel)?;
        let name = text(&member["name"]);
        let scene = codec::tree_preview_scene(
            &archive.read(member["id"].as_u64().unwrap() as usize, 64 * 1024 * 1024)?,
        )
        .with_context(|| format!("Reading scene {name}"))?;
        let placed = crate::scene::placements(&scene)?;
        // Aliases repeat the same placement. Count actual authored objects,
        // not lookup keys; the original renderer uses this to select court view.
        for placement in placed.values().flatten() {
            authored_objects.insert(serde_json::to_string(placement)?);
        }
        let mut nodes = Vec::new();
        visit(&scene, String::new(), &mut nodes, 0)?;
        let lod_paths = alternate_lod_paths(&nodes);
        scenes.push(name.to_string());
        for (path, model) in nodes {
            if lod_paths.contains(&path) {
                continue;
            }
            let normalized = match legacy_model(archive, &mut buffers, model) {
                Ok(model) => model,
                Err(error) => {
                    found += 1;
                    skipped.push(json!({"name":path,"reason":error.to_string()}));
                    continue;
                }
            };
            let model = normalized.as_ref().unwrap_or(model);
            if !model["IndexBuffer"]["Binary"].is_string()
                || !model["VertexFormat"]["POSITION0"].is_object()
                || !model["Prim"].is_array()
            {
                continue;
            }
            found += 1;
            if arena && path.to_lowercase().contains("debug_helper_sphere") {
                continue;
            }
            let mut candidates = crate::scene::variants(&path);
            for prim in model["Prim"].as_array().unwrap() {
                for key in ["Mesh", "Material"] {
                    candidates.extend(crate::scene::variants(text(&prim[key])));
                }
            }
            let instances = selected_placements(&path, &candidates, &placed);
            let identity = serde_json::to_string(
                &json!({"index":model["IndexBuffer"],"streams":model["VertexStream"],"format":model["VertexFormat"],"prims":model["Prim"]}),
            )?;
            if arena {
                if let Some(existing) = unique.get(&identity) {
                    if let Some(additions) = instances.as_ref() {
                        let list = meshes[*existing]["instances"]
                            .as_array_mut()
                            .context("Instanced mesh lacks placements")?;
                        for addition in additions {
                            if !list.contains(addition) {
                                list.push(addition.clone());
                            }
                        }
                    }
                    continue;
                }
            }
            if meshes.len() >= 1000 || triangles >= 2_000_000 {
                skipped.push(json!({"name":path,"reason":"Preview budget (1,000 complete models / 2 million triangles); exports unaffected"}));
                continue;
            }
            match mesh(
                archive,
                catalog,
                oodle,
                &mut buffers,
                model,
                &path,
                2_000_000 - triangles,
            ) {
                Ok((mut item, count)) => {
                    if let Some(instances) = instances.as_ref() {
                        item["instances"] = json!(instances);
                    } else if arena {
                        // The original renderer keeps vertex-space models such as
                        // crowd cards when SCNE has no Object matrix for them.
                        // Preserve authored positions; never invent a transform.
                        item["instances"] = json!([]);
                        item["sourcePlacement"] = json!("authored-vertex-space");
                    }
                    if let Some(groups) = item["groups"].as_array() {
                        for group in groups {
                            *usage
                                .entry(text(&group["material"]).to_owned())
                                .or_default() += 1;
                        }
                    }
                    if arena {
                        unique.insert(identity, meshes.len());
                    }
                    meshes.push(item);
                    triangles += count;
                }
                Err(error) => skipped.push(json!({"name":path,"reason":error.to_string()})),
            }
        }
    }
    ensure!(
        !meshes.is_empty(),
        "No supported geometry found. {} models skipped: {}",
        skipped.len(),
        skipped
            .iter()
            .take(4)
            .map(|s| text(&s["reason"]))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let instances: usize = meshes
        .iter()
        .map(|m| m["instances"].as_array().map(Vec::len).unwrap_or(0).max(1))
        .sum();
    Ok(
        json!({"file":file,"meshes":meshes,"scenes":scenes,"materials":usage.iter().map(|(name,n)|json!({"name":name,"primitives":n})).collect::<Vec<_>>(),"textures":[],"skipped":skipped,"stats":{"triangles":triangles,"modelsFound":found,"modelsDecoded":meshes.len(),"instances":instances,"objectMatrices":authored_objects.len(),"arena":arena},"arena":arena,"notes":"Native shared-buffer geometry with authored object/pivot placement and instanced duplicates. Complete meshes only; any unsupported/budget-limited models are listed explicitly. Neutral facial morphs; previews never alter exported assets."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_authored_alternative_lods_are_suppressed_in_their_own_scope() {
        let scene = json!({"main":{
            "Model":{"head":{"VertexFormat":{"POSITION0":{}}},"low":{},"other":{}},
            "Object":{"head":{"Target":"head","Lods":["low","other"]},"separate":{"Target":"other"}}
        },"unrelated":{"Model":{"low":{}}}});
        let mut nodes = Vec::new();
        visit(&scene, String::new(), &mut nodes, 0).unwrap();
        assert_eq!(
            alternate_lod_paths(&nodes),
            std::collections::HashSet::from(["/main/Model/low".into()])
        );
        let missing =
            json!({"Model":{"low":{}},"Object":{"head":{"Target":"missing","Lods":["low"]}}});
        let mut nodes = Vec::new();
        visit(&missing, String::new(), &mut nodes, 0).unwrap();
        assert!(alternate_lod_paths(&nodes).is_empty());
    }
    #[test]
    fn empty_hair_uv1_uses_populated_authored_uv0_only() {
        let empty = [0.; 6];
        let populated = [0., 0., 1., 0., 0., 1.];
        assert!(use_attachment_texture_uvs(true, &empty, Some(&populated)));
        assert!(!use_attachment_texture_uvs(true, &populated, Some(&empty)));
        assert!(!use_attachment_texture_uvs(false, &empty, Some(&populated)));
        assert!(!use_attachment_texture_uvs(true, &empty, Some(&empty)));
        assert!(!use_attachment_texture_uvs(true, &empty, Some(&[0., 1.])));
        assert!(!use_attachment_texture_uvs(
            true,
            &empty,
            Some(&[f32::NAN; 6])
        ));
        assert!(!use_attachment_texture_uvs(true, &empty, None));
    }
    #[test]
    fn placement_aliases_do_not_duplicate_instances() {
        let scene = json!({"Object":{"chair_object":{"Target":"chair","Matrix":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}}});
        assert_eq!(placements(&scene).unwrap()["chair"].len(), 1);
    }
    #[test]
    fn direct_model_targets_beat_broad_scene_aliases() {
        let matrix = |y: i64| json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, y, 0, 1]);
        let placed = HashMap::from([(
            "main@basket0".to_owned(),
            vec![
                json!({"name":"basket0:basket","target":"arena@GameObjects@MAIN@basket0:basket","matrix":matrix(1)}),
                json!({"name":"basket0:glass","target":"arena@GameObjects@MAIN@basket0:glass","matrix":matrix(1)}),
                json!({"name":"basket1:basket","target":"arena@GameObjects@MAIN@basket0:basket","matrix":matrix(-1)}),
            ],
        )]);
        let selected = selected_placements(
            "arena@GameObjects@MAIN@basket0:basket",
            &["main@basket0".to_owned()],
            &placed,
        )
        .unwrap();
        assert_eq!(selected.len(), 2);
        assert!(
            selected
                .iter()
                .all(|item| text(&item["target"]).ends_with(":basket"))
        );
    }
    #[test]
    fn identical_alias_matrices_are_collapsed() {
        let matrix = json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1]);
        let placed = HashMap::from([(
            "shared_mesh".to_owned(),
            vec![
                json!({"name":"alias_a","target":"assembly","matrix":matrix}),
                json!({"name":"alias_b","target":"assembly","matrix":matrix}),
            ],
        )]);
        assert_eq!(
            selected_placements("unmatched_mesh", &["shared_mesh".to_owned()], &placed)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn packed_position_keeps_scale_offset() {
        let bits = 2u64 | (3 << 21) | (4 << 42);
        let descriptor = json!({"Format":"R21G21B22_UINT","Scale":[2,3,4],"Offset":[1,2,3]});
        assert_eq!(
            attribute(&bits.to_le_bytes(), 8, 0, &descriptor, 3).unwrap(),
            vec![5., 11., 19.]
        );
    }
    #[test]
    fn out_of_bounds_attribute_is_error() {
        assert!(attribute(&[], 8, 0, &json!({"Format":"R32G32B32_FLOAT"}), 3).is_err());
    }
}
