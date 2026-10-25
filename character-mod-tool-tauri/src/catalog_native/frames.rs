//! Archive Studio's neutral tangent frame, retained per immutable donor head.
use anyhow::{Context, Result, ensure};
use nba2k_assets::codec;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::atomic::AtomicBool};
fn floats(v: &Value) -> Result<Vec<f32>> {
    v.as_array()
        .context("Missing mesh attribute")?
        .iter()
        .map(|n| {
            n.as_f64()
                .map(|n| n as f32)
                .context("Invalid mesh attribute")
        })
        .collect()
}
fn uints(v: &Value) -> Result<Vec<usize>> {
    v.as_array()
        .context("Missing mesh indices")?
        .iter()
        .map(|n| n.as_u64().map(|n| n as usize).context("Invalid mesh index"))
        .collect()
}
pub fn head_group(g: &Value) -> bool {
    let name = g["material"]
        .as_str()
        .or_else(|| g["Material"].as_str())
        .unwrap_or("");
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    PATTERN
        .get_or_init(|| regex::Regex::new(r"(?i)(?:^|_)face_shader(?:_|$)").unwrap())
        .is_match(name)
}

pub fn fit_head(mesh: &Value) -> Result<Option<Value>> {
    let positions = floats(&mesh["positions"])?;
    let uv = floats(&mesh["uvs"])?;
    let ids = uints(&mesh["sourceVertexIds"])?;
    let source = uints(&mesh["indices"])?;
    let mut remap = HashMap::new();
    let (mut points, mut uvs, mut sources, mut indices, mut groups) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for g in mesh["groups"]
        .as_array()
        .context("No groups")?
        .iter()
        .filter(|g| head_group(g))
    {
        let start = g["start"].as_u64().unwrap_or(0) as usize;
        let count = g["count"].as_u64().unwrap_or(0) as usize;
        ensure!(
            start <= source.len() && count <= source.len() - start,
            "Head group outside indices"
        );
        let group_start = indices.len();
        for v in &source[start..start + count] {
            let index = if let Some(n) = remap.get(v) {
                *n
            } else {
                let index = remap.len();
                remap.insert(*v, index);
                points.extend_from_slice(
                    positions
                        .get(v * 3..v * 3 + 3)
                        .context("Invalid head position")?,
                );
                uvs.extend_from_slice(uv.get(v * 2..v * 2 + 2).context("Invalid head UV")?);
                sources.push(*ids.get(*v).context("Missing head source identity")?);
                index
            };
            indices.push(index);
        }
        let mut group = g.clone();
        group["start"] = json!(group_start);
        group["count"] = json!(indices.len() - group_start);
        groups.push(group);
    }
    if indices.is_empty() {
        return Ok(None);
    }
    let mut out = mesh.clone();
    out["positions"] = json!(points);
    out["uvs"] = json!(uvs);
    out["sourceVertexIds"] = json!(sources);
    out["indices"] = json!(indices);
    out["groups"] = json!(groups);
    out["sourceKind"] = json!("facialhair-fit-base");
    Ok(Some(out))
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: [f64; 3]) -> [f64; 3] {
    let length = a.iter().map(|n| n * n).sum::<f64>().sqrt();
    let length = if length == 0. { 1. } else { length };
    a.map(|n| n / length)
}

pub struct Frame {
    points: Vec<f32>,
    normals: Vec<f32>,
    tangents: Vec<f32>,
    lookup: HashMap<usize, usize>,
}
impl Frame {
    pub fn new(head: &Value, cancel: &AtomicBool) -> Result<Self> {
        let points = floats(&head["positions"])?;
        let uv = floats(&head["uvs"])?;
        let indices = uints(&head["indices"])?;
        let ids = uints(&head["sourceVertexIds"])?;
        ensure!(
            points.len() == ids.len() * 3 && uv.len() == ids.len() * 2,
            "Head attribute length mismatch"
        );
        let mut normals = vec![0f32; points.len()];
        let mut tangents = normals.clone();
        for triangle in indices.chunks_exact(3) {
            codec::cancelled(cancel)?;
            let (a, b, c) = (triangle[0], triangle[1], triangle[2]);
            ensure!(
                [a, b, c].iter().all(|n| *n < ids.len()),
                "Head triangle outside vertices"
            );
            let ab = std::array::from_fn(|axis| {
                points[b * 3 + axis] as f64 - points[a * 3 + axis] as f64
            });
            let ac = std::array::from_fn(|axis| {
                points[c * 3 + axis] as f64 - points[a * 3 + axis] as f64
            });
            let normal = cross(ab, ac);
            let (du_ab, dv_ab) = (
                uv[b * 2] as f64 - uv[a * 2] as f64,
                uv[b * 2 + 1] as f64 - uv[a * 2 + 1] as f64,
            );
            let (du_ac, dv_ac) = (
                uv[c * 2] as f64 - uv[a * 2] as f64,
                uv[c * 2 + 1] as f64 - uv[a * 2 + 1] as f64,
            );
            let div = du_ab * dv_ac - dv_ab * du_ac;
            let tangent = if div.abs() > 1e-10 {
                std::array::from_fn(|axis| (ab[axis] * dv_ac - ac[axis] * dv_ab) / div)
            } else {
                ab
            };
            for vertex in [a, b, c] {
                for axis in 0..3 {
                    normals[vertex * 3 + axis] =
                        (normals[vertex * 3 + axis] as f64 + normal[axis]) as f32;
                    tangents[vertex * 3 + axis] =
                        (tangents[vertex * 3 + axis] as f64 + tangent[axis]) as f32;
                }
            }
        }
        for vertex in 0..ids.len() {
            for values in [&mut normals, &mut tangents] {
                let n = normalize(std::array::from_fn(|axis| values[vertex * 3 + axis] as f64));
                for axis in 0..3 {
                    values[vertex * 3 + axis] = n[axis] as f32;
                }
            }
        }
        let lookup: HashMap<usize, usize> = ids
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index))
            .collect();

        Ok(Self {
            points,
            normals,
            tangents,
            lookup,
        })
    }
    pub fn bytes(&self) -> usize {
        (self.points.len() + self.normals.len() + self.tangents.len()) * 4 + self.lookup.len() * 32
    }
    pub fn project(&self, mesh: &mut Value, cancel: &AtomicBool) -> Result<usize> {
        let anchors = uints(&mesh["tangentAnchorIds"])?;
        let offsets = floats(&mesh["tangentOffsets"])?;
        ensure!(
            offsets.len() == anchors.len() * 3 && !anchors.is_empty(),
            "Invalid tangent offset/anchor lengths"
        );
        let mut out = Vec::with_capacity(offsets.len());
        for (vertex, id) in anchors.iter().enumerate() {
            codec::cancelled(cancel)?;
            let index = *self.lookup.get(id).context(format!(
                "Attachment anchor {id} is not in the reference head; refusing distorted geometry"
            ))?;
            let n = std::array::from_fn(|axis| self.normals[index * 3 + axis] as f64);
            let t = std::array::from_fn(|axis| self.tangents[index * 3 + axis] as f64);
            let b = normalize(cross(n, t));
            for axis in 0..3 {
                out.push(
                    (self.points[index * 3 + axis] as f64
                        + t[axis] * offsets[vertex * 3] as f64 * 64.
                        + b[axis] * offsets[vertex * 3 + 1] as f64 * 64.
                        + n[axis] * offsets[vertex * 3 + 2] as f64 * 64.)
                        as f32,
                );
            }
        }
        mesh["positions"] = json!(out);
        mesh["tangentProjected"] = json!(true);
        mesh["tangentProjectionMode"] = json!("anchor-tangent-frame");
        Ok(anchors.len())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_anchor_ids_and_neutral_projection_are_reusable() {
        let head = json!({"positions":[0,0,0,1,0,0,0,1,0],"uvs":[0,0,1,0,0,1],"indices":[0,1,2],"sourceVertexIds":[7,8,9]});
        let frame = Frame::new(&head, &AtomicBool::new(false)).unwrap();
        let mut mesh = json!({"tangentAnchorIds":[7],"tangentOffsets":[0.01,0.02,0.03]});
        assert_eq!(
            frame.project(&mut mesh, &AtomicBool::new(false)).unwrap(),
            1
        );
        let before = mesh["positions"].clone();
        assert!((before[0].as_f64().unwrap() - 0.64).abs() < 1e-6);
        assert!((before[1].as_f64().unwrap() - 1.28).abs() < 1e-6);
        assert!((before[2].as_f64().unwrap() - 1.92).abs() < 1e-6);
        frame.project(&mut mesh, &AtomicBool::new(false)).unwrap();
        assert_eq!(mesh["positions"], before);
        mesh["tangentAnchorIds"] = json!([99]);
        assert!(frame.project(&mut mesh, &AtomicBool::new(false)).is_err());
    }
}
