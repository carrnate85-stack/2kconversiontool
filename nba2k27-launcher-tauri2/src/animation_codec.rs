//! Rust port of the WPF player animation preview decoder (PlayerAnimationPreview.exe,
//! nba2k27-animation-lab codec.py, multiple.py, skeletons.py, trajectory.py,
//! prop_math.py, hand_pose.py, shot_events.py, archive.py, character_mesh.py and
//! animation_lab.decode_loaded_clip). Arithmetic follows the Python order of
//! operations (f64 with explicit float32 steps) so payloads match the Python
//! runtime. Read-only: archives are extracted copies in the launcher cache.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

pub type Quat = [f64; 4];
pub type Vec3 = [f64; 3];

/// Native human sampler table at RVA 0x7c0f080: (track type, normal, mirrored).
const HUMAN_MIRROR: [usize; 22] = [0, 1, 2, 3, 4, 5, 10, 11, 12, 13, 6, 7, 8, 9, 18, 19, 20, 21, 14, 15, 16, 17];
const HUMAN_PARENTS: [i64; 22] = [-1, 0, 1, 2, 3, 4, 3, 6, 7, 8, 3, 10, 11, 12, 0, 14, 15, 16, 0, 18, 19, 20];
const CROWD_PARENTS: [i64; 24] = [-1, 0, 1, 2, 3, 4, 3, 6, 7, 8, 9, 3, 11, 12, 13, 14, 0, 16, 17, 18, 0, 20, 21, 22];
const CROWD_MIRROR: [usize; 24] = [0, 1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 6, 7, 8, 9, 10, 20, 21, 22, 23, 16, 17, 18, 19];
const BONE_NAMES: [&str; 22] = [
    "pelvis", "spine_1", "spine_2", "spine_3", "neck", "head", "clavicle_R", "upperarm_R", "forearm_R", "hand_R",
    "clavicle_L", "upperarm_L", "forearm_L", "hand_L", "thigh_R", "shin_R", "foot_R", "toe_R", "thigh_L", "shin_L",
    "foot_L", "toe_L",
];

fn f32r(v: f64) -> f64 {
    v as f32 as f64
}
fn u16_at(b: &[u8], p: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(b.get(p..p + 2).context("Truncated motion data")?.try_into()?))
}
fn i16_at(b: &[u8], p: usize) -> Result<i16> {
    Ok(i16::from_le_bytes(b.get(p..p + 2).context("Truncated motion data")?.try_into()?))
}
fn u32_at(b: &[u8], p: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(b.get(p..p + 4).context("Truncated motion data")?.try_into()?))
}
fn i32_at(b: &[u8], p: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(b.get(p..p + 4).context("Truncated motion data")?.try_into()?))
}
fn u64_at(b: &[u8], p: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(b.get(p..p + 8).context("Truncated motion data")?.try_into()?))
}
fn f32_at(b: &[u8], p: usize) -> Result<f64> {
    Ok(f32::from_le_bytes(b.get(p..p + 4).context("Truncated motion data")?.try_into()?) as f64)
}
fn at(base: usize, delta: i64) -> Result<usize> {
    usize::try_from(base as i64 + delta).ok().context("Motion offset is out of range")
}
fn signed(value: u64, bits: u32) -> i64 {
    let v = value & ((1u64 << bits) - 1);
    if v & (1 << (bits - 1)) != 0 { v as i64 - (1i64 << bits) } else { v as i64 }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub fn mirror_rotation(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], q[3]]
}
fn mirror_translation(t: Vec3) -> Vec3 {
    [-t[0], t[1], t[2]]
}
/// Hopf quaternion, native RVA 0x3c15d10; not smallest-three encoding.
pub fn decode_rotation(word: u32) -> Quat {
    let w = word as u128;
    let phase_a = ((((w * 4192101509 + 0x5a827999fcef3242) >> 32) + w * 3037000499) & 0xffffffff) as f64;
    let phase_b = ((((w * 2444892694 + 0x53749ce6c8dd0d0b) >> 32) + w * 2800302541) & 0xffffffff) as f64;
    let scale = std::f64::consts::PI / 2147483648.0;
    let a = phase_a * scale;
    let b = phase_b * scale;
    let radius = word as f64 / 4294967296.0 + 1.0 / 8589934592.0;
    [
        radius.sqrt() * a.sin(),
        radius.sqrt() * a.cos(),
        (1.0 - radius).sqrt() * b.sin(),
        (1.0 - radius).sqrt() * b.cos(),
    ]
}
fn decode_translation(word: u32, exponent: i32) -> Vec3 {
    let s = 2f64.powi(-exponent);
    let w = word as u64;
    [signed(w, 10) as f64 * s, signed(w >> 10, 12) as f64 * s, signed(w >> 22, 10) as f64 * s]
}
fn sum_sq(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |s, x| s + x * x)
}
pub fn normalize(q: Quat) -> Quat {
    let s = sum_sq(&q).sqrt();
    [q[0] / s, q[1] / s, q[2] / s, q[3] / s]
}
fn dot4(a: &Quat, b: &Quat) -> f64 {
    a.iter().zip(b).fold(0.0, |s, (x, y)| s + x * y)
}
/// y+(z-y)*h+(z-x)*a+(w-y)*b, left to right as in Python.
fn curve1(x: f64, y: f64, z: f64, w: f64, h: f64, a: f64, b: f64) -> f64 {
    y + (z - y) * h + (z - x) * a + (w - y) * b
}
fn bisect_left_f(values: &[f64], target: f64, lo: usize) -> usize {
    let (mut lo, mut hi) = (lo, values.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        if values[mid] < target { lo = mid + 1 } else { hi = mid }
    }
    lo
}

fn bind_parents(bind: &[(f64, f64, f64, u32)]) -> Result<Vec<i64>> {
    let mut parents = Vec::with_capacity(bind.len());
    for (i, record) in bind.iter().enumerate() {
        let relative = signed((record.3 & 4095) as u64, 12);
        let mut parent = i as i64 + relative.div_euclid(4);
        if parent == i as i64 {
            parent = -1;
        }
        ensure!(relative % 4 == 0 && parent < i as i64 && parent >= -1, "Unsupported serialized joint hierarchy");
        parents.push(parent);
    }
    Ok(parents)
}

#[derive(Clone, Copy)]
struct Window {
    i: usize,
    j: usize,
    f: f64,
    prev: f64,
    dt: f64,
    following: f64,
    h: f64,
    a: f64,
    b: f64,
}

pub struct SingleClip {
    pub data: Vec<u8>,
    pub name: String,
    pub source_data: Vec<u8>,
    pub resolved_name: String,
    pub flags: u32,
    pub rate: u32,
    pub key_count: usize,
    pub mirrored: bool,
    pub speed: f64,
    pub bone_count: usize,
    rotation_count: usize,
    translation_count: usize,
    pub payload: usize,
    pub rotation_mask: u64,
    layout: u64,
    translation_exponent: i32,
    root_translation_scale: f64,
    pub prop_count: usize,
    pub scalar_count: usize,
    pub times_offset: usize,
    pub frames_offset: usize,
    pub stride: usize,
    rotation_bones: Vec<usize>,
    translation_bones: Vec<usize>,
    pub ticks: Vec<f64>,
    deltas: Vec<u8>,
    pub bind_offset: usize,
    pub bind: Vec<(f64, f64, f64, u32)>,
    pub skeleton_kind: &'static str,
    pub parents: Vec<i64>,
    pub mirror_map: Vec<usize>,
}

impl SingleClip {
    pub fn new(data: Vec<u8>, name: &str) -> Result<Self> {
        ensure!(data.len() >= 40, "Truncated SingleMoCap header");
        let flags = u32_at(&data, 4)?;
        let rate = flags & 127;
        let key_count = (u16_at(&data, 8)? & 32767) as usize;
        let mirrored = data[9] & 128 != 0;
        let speed = f32_at(&data, 16)?;
        let reference = u32_at(&data, 24)?;
        let bone_count = ((flags >> 7) & 63) as usize;
        let rotation_count = ((flags >> 13) & 63) as usize;
        let translation_count = ((flags >> 19) & 63) as usize;
        ensure!(reference == 0, "Reference clip must be resolved through its archive before decoding");
        ensure!(data.len() >= 72, "Missing motion payload");
        let rel = u64_at(&data, 32)?;
        ensure!(rel & 1 != 0, "Unsupported motion payload pointer");
        let payload = at(32, (rel & !1) as i64)?;
        let rotation_mask = u64_at(&data, payload)?;
        let translation_mask = u64_at(&data, payload + 8)?;
        let layout = u64_at(&data, payload + 16)?;
        let mode = data[12] & 3;
        let translation_exponent = (data[15] >> 4) as i32;
        let root_translation_scale = 2f64.powi(translation_exponent - (data[15] & 15) as i32);
        let prop_count = ((flags >> 28) & 15) as usize;
        let scalar_count = ((flags >> 25) & 7) as usize;
        ensure!(mode == 0, "Unsupported motion encoding {mode}");
        let times_offset = payload + ((layout >> 11) & 0x3ffe) as usize;
        let frames_offset = payload + 2 * (layout >> 41) as usize;
        let stride = 4 * (rotation_count + translation_count);
        let rotation_bones: Vec<usize> = (0..bone_count).filter(|i| rotation_mask >> i & 1 != 0).collect();
        let translation_bones: Vec<usize> = (0..bone_count).filter(|i| translation_mask >> i & 1 != 0).collect();
        ensure!(
            rotation_bones.len() == rotation_count && translation_bones.len() == translation_count,
            "Channel masks and counts disagree"
        );
        ensure!(
            frames_offset >= stride && frames_offset + key_count * stride + stride <= data.len(),
            "Truncated packed control frames"
        );
        ensure!(times_offset + key_count <= data.len(), "Truncated key times");
        ensure!(key_count >= 1 && times_offset >= 1, "Motion has no key times");
        let deltas = data[times_offset..times_offset + key_count].to_vec();
        let mut ticks = vec![0.0];
        for d in &deltas[..deltas.len() - 1] {
            let last = *ticks.last().unwrap();
            ticks.push(last + *d as f64 + 1.0);
        }
        // Native bind lookup (RVA 0x3c18910): scalar offset words precede the aligned bind array.
        let bind_offset = (payload + 31 + 2 * scalar_count) & !3;
        let mut bind = Vec::with_capacity(bone_count);
        for i in 0..bone_count {
            let p = bind_offset + i * 16;
            bind.push((f32_at(&data, p)?, f32_at(&data, p + 4)?, f32_at(&data, p + 8)?, u32_at(&data, p + 12)?));
        }
        let mut parents = bind_parents(&bind)?;
        let (skeleton_kind, mirror_map): (&'static str, Vec<usize>) = if bone_count == 22 && parents == HUMAN_PARENTS {
            ("human", HUMAN_MIRROR.to_vec())
        } else if bone_count == 24 && parents == CROWD_PARENTS {
            ("crowd", CROWD_MIRROR.to_vec())
        } else {
            ("generic", (0..bone_count).collect())
        };
        if mirrored {
            bind = mirror_map.iter().map(|&i| (-bind[i].0, bind[i].1, bind[i].2, bind[i].3)).collect();
            let mut inverse = HashMap::new();
            for (destination, source) in mirror_map.iter().enumerate() {
                inverse.insert(*source as i64, destination as i64);
            }
            parents = mirror_map.iter().map(|&i| if parents[i] < 0 { -1 } else { inverse[&parents[i]] }).collect();
        }
        Ok(Self {
            source_data: data.clone(),
            resolved_name: name.to_owned(),
            data,
            name: name.to_owned(),
            flags,
            rate,
            key_count,
            mirrored,
            speed,
            bone_count,
            rotation_count,
            translation_count,
            payload,
            rotation_mask,
            layout,
            translation_exponent,
            root_translation_scale,
            prop_count,
            scalar_count,
            times_offset,
            frames_offset,
            stride,
            rotation_bones,
            translation_bones,
            ticks,
            deltas,
            bind_offset,
            bind,
            skeleton_kind,
            parents,
            mirror_map,
        })
    }

    pub fn control(&self, index: i64) -> Result<(Vec<Quat>, Vec<Vec3>)> {
        if !(-1..=self.key_count as i64).contains(&index) {
            bail!("Motion control index out of range");
        }
        let base = at(self.frames_offset, index * self.stride as i64)?;
        let mut q = vec![[0.0, 0.0, 0.0, 1.0]; self.bone_count];
        let mut t = vec![[0.0; 3]; self.bone_count];
        for (k, &bone) in self.rotation_bones.iter().enumerate() {
            q[bone] = decode_rotation(u32_at(&self.data, base + 4 * k)?);
        }
        for (k, &bone) in self.translation_bones.iter().enumerate() {
            t[bone] = decode_translation(u32_at(&self.data, base + 4 * (self.rotation_count + k))?, self.translation_exponent);
        }
        if self.mirrored {
            q = self.mirror_map.iter().map(|&i| mirror_rotation(q[i])).collect();
            t = self.mirror_map.iter().map(|&i| mirror_translation(t[i])).collect();
        }
        Ok((q, t))
    }

    fn sample_window(&self, seconds: f64) -> Window {
        // Native sample time and both clock multiplies are float32.
        let rate_speed = f32r(self.rate as f64 * self.speed);
        let last = *self.ticks.last().unwrap();
        let tick = f32r(f32r(seconds) * rate_speed).max(0.0).min(last);
        let j = (self.key_count - 1).min(bisect_left_f(&self.ticks, tick, 1).max(1));
        let i = j.saturating_sub(1);
        let f = (tick - self.ticks[i]) / (self.ticks[j] - self.ticks[i]).max(1.0);
        let dt = self.deltas[i] as f64 + 1.0;
        let prev = if i > 0 { self.deltas[i - 1] as f64 + 1.0 } else { self.data[self.times_offset - 1] as f64 + 1.0 };
        let following = self.deltas[j] as f64 + 1.0;
        let h = f * f * (3.0 - 2.0 * f);
        let a = dt / (dt + prev) * f * (f * (f - 2.0) + 1.0);
        let b = dt / (dt + following) * f * f * (f - 1.0);
        Window { i, j, f, prev, dt, following, h, a, b }
    }

    /// Native normalized nonuniform cubic interpolation for ordinary body tracks (RVA 0x3c143a0).
    pub fn sample(&self, seconds: f64) -> Result<(Vec<Quat>, Vec<Vec3>)> {
        let w = self.sample_window(seconds);
        let (i, j) = (w.i as i64, w.j as i64);
        let controls = [self.control(i - 1)?, self.control(i)?, self.control(j)?, self.control(j + 1)?];
        let mut qs = Vec::with_capacity(self.bone_count);
        let mut ts = Vec::with_capacity(self.bone_count);
        for bone in 0..self.bone_count {
            let mut values: [Quat; 4] = std::array::from_fn(|k| controls[k].0[bone]);
            for k in 1..4 {
                if dot4(&values[k - 1], &values[k]) < 0.0 {
                    values[k] = values[k].map(|v| -v);
                }
            }
            let q: Quat = std::array::from_fn(|c| curve1(values[0][c], values[1][c], values[2][c], values[3][c], w.h, w.a, w.b));
            qs.push(normalize(q));
            let tv: [Vec3; 4] = std::array::from_fn(|k| controls[k].1[bone]);
            ts.push(std::array::from_fn(|c| curve1(tv[0][c], tv[1][c], tv[2][c], tv[3][c], w.h, w.a, w.b)));
        }
        // Native sampler RVA 0x3c14f8f rescales only the root translation.
        if let Some(root) = ts.first_mut() {
            *root = root.map(|v| v * self.root_translation_scale);
        }
        Ok((qs, ts))
    }

    fn prop_control(&self, prop: usize, index: i64) -> Result<(u32, Quat, Vec3)> {
        let table = self.payload + 2 * (self.layout & 4095) as usize;
        let name_hash = u32_at(&self.data, table + prop * 8)?;
        let metadata = u32_at(&self.data, table + prop * 8 + 4)?;
        let start = at(table + 2 * (metadata >> 12) as usize, index * 12)?;
        let xyz = [i16_at(&self.data, start)?, i16_at(&self.data, start + 2)?, i16_at(&self.data, start + 4)?]
            .map(|v| v as f64 / 32767.0);
        let scale = 2.0 / (1.0 + sum_sq(&xyz));
        let mut q = [xyz[0] * scale, xyz[1] * scale, xyz[2] * scale, scale - 1.0];
        let mut t = [i16_at(&self.data, start + 6)?, i16_at(&self.data, start + 8)?, i16_at(&self.data, start + 10)?]
            .map(|v| v as f64 / 16.0);
        if self.mirrored {
            q = mirror_rotation(q);
            t = mirror_translation(t);
        }
        Ok((name_hash, q, t))
    }

    fn prop_attachment(&self, prop: usize) -> Result<i64> {
        let table = self.payload + 2 * (self.layout & 4095) as usize;
        let metadata = u32_at(&self.data, table + prop * 8 + 4)?;
        let mut joint = signed((metadata & 4095) as u64, 12);
        if self.mirrored && joint >= 0 {
            let mut mapping: Vec<usize> = self.mirror_map.clone();
            if matches!(self.skeleton_kind, "human" | "crowd") {
                mapping.extend([self.bone_count + 1, self.bone_count]);
            }
            ensure!((joint as usize) < mapping.len(), "Prop attachment is outside the spatial track mapping");
            joint = mapping[joint as usize] as i64;
        }
        Ok(joint)
    }

    /// Native type-0 cumulative quaternion cubic and nonuniform position cubic.
    pub fn sample_props(&self, seconds: f64) -> Result<Vec<Value>> {
        let w = self.sample_window(seconds);
        let mut result = Vec::with_capacity(self.prop_count);
        for prop in 0..self.prop_count {
            let (i, j) = (w.i as i64, w.j as i64);
            let values = [self.prop_control(prop, i - 1)?, self.prop_control(prop, i)?, self.prop_control(prop, j)?, self.prop_control(prop, j + 1)?];
            let qs: [Quat; 4] = std::array::from_fn(|k| values[k].1);
            let ts: [Vec3; 4] = std::array::from_fn(|k| values[k].2);
            let (q, t) = prop_cubic(&qs, &ts, (w.h, w.a, w.b));
            result.push(json!({
                "name_hash": format!("{:08x}", values[1].0),
                "rotation_xyzw": q,
                "translation_cm": t,
                "attachment_joint": self.prop_attachment(prop)?,
                "interpolation": "native cumulative quaternion cubic / nonuniform position cubic",
            }));
        }
        Ok(result)
    }
}

// prop_math.py
fn qmul(a: Quat, b: Quat) -> Quat {
    let [x, y, z, w] = a;
    let [xx, yy, zz, ww] = b;
    [
        w * xx + x * ww + y * zz - z * yy,
        w * yy - x * zz + y * ww + z * xx,
        w * zz + x * yy - y * xx + z * ww,
        w * ww - x * xx - y * yy - z * zz,
    ]
}
fn qrelative(a: Quat, b: Quat) -> Quat {
    let q = qmul(a, [-b[0], -b[1], -b[2], b[3]]);
    if q[3] < 0.0 { q.map(|v| -v) } else { q }
}
fn qlog(q: Quat) -> Vec3 {
    let length = sum_sq(&q[..3]).sqrt();
    if length < 1e-15 {
        return [0.0; 3];
    }
    let scale = length.atan2(q[3]) / length;
    [q[0] * scale, q[1] * scale, q[2] * scale]
}
fn qexp(v: Vec3) -> Quat {
    let length = sum_sq(&v).sqrt();
    let scale = if length > 1e-15 { length.sin() / length } else { 1.0 };
    [v[0] * scale, v[1] * scale, v[2] * scale, length.cos()]
}
fn prop_cubic(qs: &[Quat; 4], ts: &[Vec3; 4], (h, a, b): (f64, f64, f64)) -> (Quat, Vec3) {
    let first = qrelative(qs[1], qs[0]);
    let middle = qrelative(qs[2], qs[1]);
    let last = qrelative(qs[3], qs[2]);
    let logs = [qlog(middle), qlog(qmul(middle, first)), qlog(qmul(last, middle))];
    let powers: Vec<Quat> = [h, a, b].iter().zip(logs).map(|(w, v)| qexp(v.map(|x| w * x))).collect();
    let q = normalize(qmul(qmul(powers[2], powers[1]), qmul(powers[0], qs[1])));
    let t = std::array::from_fn(|c| curve1(ts[0][c], ts[1][c], ts[2][c], ts[3][c], h, a, b));
    (q, t)
}

// trajectory.py
const STRIDES: [usize; 4] = [6, 4, 12, 8];
fn trajectory_control(clip: &SingleClip, index: i64) -> Result<Vec3> {
    let mode = (u32_at(&clip.data, 8)? >> 30) as usize;
    let start = clip.payload + ((clip.layout >> 24) & 0x1fffe) as usize;
    let stride = STRIDES[mode];
    ensure!((-1..=clip.key_count as i64).contains(&index), "Trajectory control index out of range");
    let p = at(start, index * stride as i64)?;
    ensure!(p + stride <= clip.data.len(), "Truncated motion trajectory controls");
    let d = &clip.data;
    let v = match mode {
        0 => [i16_at(d, p)? as f64 / 8.0, i16_at(d, p + 2)? as f64 / 8.0, i16_at(d, p + 4)? as f64 * 8.0],
        2 => [f32_at(d, p)?, f32_at(d, p + 4)?, f32_at(d, p + 8)?],
        3 => [f32_at(d, p)?, f32_at(d, p + 4)?, 0.0],
        _ => [i16_at(d, p)? as f64 / 8.0, i16_at(d, p + 2)? as f64 / 8.0, 0.0],
    };
    Ok(if clip.mirrored { [-v[0], v[1], -v[2]] } else { v })
}
/// (x cm, z cm, heading units) at a playback time; native RVA 0x3c11750.
pub fn sample_trajectory(clip: &SingleClip, seconds: f64) -> Result<(f64, f64, i64)> {
    let w = clip.sample_window(seconds);
    let (i, j) = (w.i as i64, w.j as i64);
    let c = [trajectory_control(clip, i - 1)?, trajectory_control(clip, i)?, trajectory_control(clip, j)?, trajectory_control(clip, j + 1)?];
    let out: Vec3 = std::array::from_fn(|k| curve1(c[0][k], c[1][k], c[2][k], c[3][k], w.h, w.a, w.b));
    // The native sampler converts heading to int32 before applying mirroring.
    Ok((out[0], out[1], out[2].trunc() as i64))
}

// shot_events.py
pub fn shot_events(clip: &SingleClip) -> Result<Vec<Value>> {
    let start = clip.bind_offset + clip.bone_count * 16;
    // Events precede the other payload tables. Never scan packed controls.
    let mut limits = vec![clip.data.len(), clip.times_offset, clip.frames_offset - clip.stride];
    for i in 0..clip.scalar_count {
        let target = clip.payload + u16_at(&clip.data, clip.payload + 28 + i * 2)? as usize * 2;
        if target > start {
            limits.push(target);
        }
    }
    let end = limits.into_iter().filter(|x| *x >= start).min().context("Shot event table has no bounded terminator")?;
    let mut result = Vec::new();
    let mut offset = start;
    while offset + 7 < end {
        let kind = i16_at(&clip.data, offset)?;
        let flags = u16_at(&clip.data, offset + 2)?;
        let seconds = f32_at(&clip.data, offset + 4)?;
        if flags == 1 {
            return Ok(result);
        }
        ensure!(seconds.is_finite(), "Nonfinite shot event time");
        result.push(json!({"type":kind,"flags":flags,"source_seconds":seconds,"time":seconds / clip.speed}));
        offset += 8;
    }
    bail!("Shot event table has no bounded terminator")
}

// multiple.py
pub struct MultipleClip {
    pub data: Vec<u8>,
    pub name: String,
    pub source_data: Vec<u8>,
    pub resolved_name: String,
    pub identifier: u32,
    pub actor_count: usize,
    pub reference: u32,
    pub headers: usize,
    pub actors: Vec<Actor>,
}
#[derive(Clone)]
pub struct Actor {
    pub actor_index: usize,
    pub header_offset: usize,
    pub id: u32,
    pub joint_count: u32,
    pub mode: u8,
    pub position_xz_raw: (f64, f64),
    pub orientation_word_raw: u16,
}
impl Actor {
    pub fn to_json(&self) -> Value {
        json!({
            "actor_index": self.actor_index,
            "header_offset": self.header_offset,
            "id": format!("{:08x}", self.id),
            "joint_count": self.joint_count,
            "mode": self.mode,
            "position_xz_raw": [self.position_xz_raw.0, self.position_xz_raw.1],
            "orientation_word_raw": self.orientation_word_raw,
            "placement_status": "Native planar actor placement decoded; numerical validation report required",
        })
    }
}
impl MultipleClip {
    pub fn new(data: Vec<u8>, name: &str) -> Result<Self> {
        ensure!(data.len() >= 24, "Truncated MultipleMoCap header");
        let identifier = u32_at(&data, 0)?;
        let actor_count = u32_at(&data, 4)? as usize;
        let reference = u32_at(&data, 8)?;
        let headers = 16 + u16_at(&data, 16)? as usize;
        ensure!((1..=64).contains(&actor_count), "Unrecognized actor header layout");
        let expected = (28 + 12 * (actor_count - 1) + 7) / 8 * 8;
        ensure!(headers == expected, "Unrecognized actor header layout");
        ensure!(headers + 40 * actor_count <= data.len(), "Truncated actor headers");
        let mut actors = Vec::with_capacity(actor_count);
        for actor in 0..actor_count {
            let start = headers + actor * 40;
            let id = u32_at(&data, start)?;
            let flags = u32_at(&data, start + 4)?;
            let back = ((u32_at(&data, start + 8)? >> 21) & 511) as usize * 8;
            ensure!(back == start, "Actor back-reference disagrees with its container offset");
            let record = 16 + actor * 12;
            let relative = u16_at(&data, record)? as usize;
            let orientation = u16_at(&data, record + 2)?;
            let (x, z) = (f32_at(&data, record + 4)?, f32_at(&data, record + 8)?);
            ensure!(record + relative == start, "Actor table and contiguous headers disagree");
            ensure!(x.is_finite() && z.is_finite(), "Nonfinite actor placement");
            actors.push(Actor {
                actor_index: actor,
                header_offset: start,
                id,
                joint_count: (flags >> 7) & 63,
                mode: data[start + 12] & 3,
                position_xz_raw: (x, z),
                orientation_word_raw: orientation,
            });
        }
        Ok(Self {
            source_data: data.clone(),
            resolved_name: name.to_owned(),
            data,
            name: name.to_owned(),
            identifier,
            actor_count,
            reference,
            headers,
            actors,
        })
    }
    pub fn actor(&self, index: usize) -> Result<SingleClip> {
        ensure!(index < self.actor_count, "Actor index is outside this container");
        let start = self.actors[index].header_offset;
        let mut clip = SingleClip::new(self.data[start..].to_vec(), &format!("{} actor {index}", self.name))?;
        clip.source_data = self.source_data.clone();
        clip.resolved_name = self.resolved_name.clone();
        Ok(clip)
    }
}

// archive.py + archive_cache.py: one extracted IFF (ZIP), motion IDs indexed once.
pub struct Archive {
    zip: zip::ZipArchive<BufReader<File>>,
    single: HashMap<u32, String>,
    multiple: HashMap<u32, String>,
}
const MAX_MEMBER: u64 = 64 * 1024 * 1024;
fn read_member_bounded(member: impl Read, expected:u64, limit:u64)->Result<Vec<u8>> {
    ensure!(limit<=MAX_MEMBER && expected<=limit,"Animation member exceeds its byte budget");
    let mut bytes=Vec::new();member.take(limit+1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64==expected && bytes.len() as u64<=limit,"Animation member expanded size differs");Ok(bytes)
}
impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        let zip = zip::ZipArchive::new(BufReader::new(File::open(path)?))?;
        Ok(Self { zip, single: HashMap::new(), multiple: HashMap::new() })
    }
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let mut member = self.zip.by_name(name).with_context(|| format!("Animation member is missing: {name}"))?;
        ensure!(member.size() <= MAX_MEMBER, "Animation member exceeds its byte budget");
        let expected=member.size();
        read_member_bounded(member,expected,MAX_MEMBER)
    }
    /// archive_cache Entry.index: motion headers keyed by stored ID.
    pub fn index(&mut self, cancelled: &dyn Fn() -> bool) -> Result<usize> {
        if !self.single.is_empty() || !self.multiple.is_empty() {
            return Ok(self.single.len() + self.multiple.len());
        }
        let (mut single, mut multiple) = (HashMap::new(), HashMap::new());
        for i in 0..self.zip.len() {
            if i % 512 == 0 {
                ensure!(!cancelled(), "Animation indexing cancelled.");
            }
            let mut member = self.zip.by_index(i)?;
            let name = member.name().to_owned();
            let is_single = name.ends_with(".SingleMoCap");
            if !is_single && !name.ends_with(".MultipleMoCap") {
                continue;
            }
            let mut header = [0u8; 4];
            member.read_exact(&mut header).with_context(|| format!("Truncated motion header: {name}"))?;
            let identifier = u32::from_le_bytes(header);
            let ids = if is_single { &mut single } else { &mut multiple };
            ensure!(ids.insert(identifier, name).is_none(), "Ambiguous motion ID");
        }
        self.single = single;
        self.multiple = multiple;
        Ok(self.single.len() + self.multiple.len())
    }
    /// AnimationArchive.load: follow reference records to their payload.
    pub fn load(&mut self, name: &str) -> Result<SingleClip> {
        ensure!(name.ends_with(".SingleMoCap"), "Only SingleMoCap clips are currently supported");
        let original = self.read(name)?;
        let mut data = original.clone();
        let mut resolved = name.to_owned();
        let mut seen = std::collections::HashSet::new();
        let mut followed = false;
        loop {
            ensure!(data.len() >= 40, "Truncated motion header");
            let identifier = u32_at(&data, 0)?;
            let reference = u32_at(&data, 24)?;
            ensure!(seen.insert(identifier), "Cyclic motion references");
            if reference == 0 {
                break;
            }
            if self.single.is_empty() {
                self.index(&|| false)?;
            }
            resolved = self.single.get(&reference).with_context(|| format!("Missing referenced clip ID {reference:08x}"))?.clone();
            data = self.read(&resolved)?;
            followed = true;
        }
        if followed {
            // A reference owns timing/mirror flags; it shares the target payload.
            data[..32].copy_from_slice(&original[..32]);
            data[24..28].copy_from_slice(&0u32.to_le_bytes());
        }
        let mut clip = SingleClip::new(data, name)?;
        clip.source_data = original;
        clip.resolved_name = resolved;
        Ok(clip)
    }
    /// multiple.load_multiple.
    pub fn load_multiple(&mut self, name: &str) -> Result<MultipleClip> {
        ensure!(name.ends_with(".MultipleMoCap"), "Expected MultipleMoCap member");
        let original = self.read(name)?;
        let mut data = original.clone();
        let mut resolved = name.to_owned();
        let mut seen = std::collections::HashSet::new();
        let mut followed = false;
        loop {
            let header = MultipleClip::new(data.clone(), &resolved)?;
            ensure!(seen.insert(header.identifier), "Cyclic MultipleMoCap reference");
            if header.reference == 0 {
                break;
            }
            if self.multiple.is_empty() {
                self.index(&|| false)?;
            }
            resolved = self.multiple.get(&header.reference).context("Missing MultipleMoCap target")?.clone();
            data = self.read(&resolved)?;
            followed = true;
        }
        if followed {
            let source = MultipleClip::new(original.clone(), name)?;
            let target = MultipleClip::new(data.clone(), &resolved)?;
            ensure!(
                source.actor_count == target.actor_count && source.headers == target.headers,
                "Reference actor layout differs from target"
            );
            let mut merged = data;
            merged[..source.headers].copy_from_slice(&original[..source.headers]);
            for actor in &source.actors {
                let start = actor.header_offset;
                merged[start..start + 32].copy_from_slice(&original[start..start + 32]);
            }
            merged[8..12].copy_from_slice(&0u32.to_le_bytes());
            data = merged;
        }
        let mut clip = MultipleClip::new(data, name)?;
        clip.source_data = original;
        clip.resolved_name = resolved;
        Ok(clip)
    }
}

// hand_pose.py
const HAND_NODES: [&str; 17] = [
    "Hand", "thumb_1", "thumb_2", "thumb_3", "finger_1_1", "finger_1_2", "finger_1_3", "finger_2_1", "finger_2_2",
    "finger_2_3", "hand_edge", "finger_3_1", "finger_3_2", "finger_3_3", "finger_4_1", "finger_4_2", "finger_4_3",
];
const HAND_PARENTS: [i64; 17] = [-1, 0, 1, 2, 0, 4, 5, 0, 7, 8, 0, 10, 11, 12, 10, 14, 15];
/// Polynomial time correction of the ordinary quaternion blend (RVA 0x3c12470).
const BLEND_COEFFICIENTS: [f64; 6] =
    [0.9223620295524597, -2.3059051036834717, 2.8912253379821777, -2.030932903289795, 0.5227404832839966, 0.0002550582867115736];

pub struct HandLibrary {
    pub source_sha256: String,
    pub poses: Vec<Vec<Quat>>,
}
impl HandLibrary {
    /// load_library(path, 'male') from the extracted hand pose clip bytes.
    pub fn from_clip_bytes(bytes: Vec<u8>) -> Result<Self> {
        let clip = SingleClip::new(bytes, "hand_pose_male.SingleMoCap")?;
        ensure!(
            clip.bone_count == 17 && clip.rotation_mask == 0x1fffe && clip.translation_count == 0,
            "Unrecognized hand pose library layout"
        );
        let parents: Vec<i64> = clip
            .bind
            .iter()
            .enumerate()
            .map(|(i, b)| if i == 0 { -1 } else { i as i64 + signed(b.3 as u64, 12).div_euclid(4) })
            .collect();
        ensure!(parents == HAND_PARENTS, "Hand bind metadata disagrees with source finger hierarchy");
        let poses = (0..clip.key_count as i64).map(|i| clip.control(i).map(|c| c.0)).collect::<Result<_>>()?;
        Ok(Self { source_sha256: sha256_hex(&clip.source_data), poses })
    }
    pub fn source_json(&self) -> Value {
        json!({"schema":"nba2k27-hand-library-v1","sex":"male","source_sha256":self.source_sha256,
               "pose_count":self.poses.len(),"side_order":["source_left","source_right"],
               "status":"Native pose selectors and 17-joint quaternion blending verified offline"})
    }
}
fn blend_rotation(a: Quat, mut b: Quat, weight: f64) -> Quat {
    let dot = dot4(&a, &b);
    if dot < 0.0 {
        b = b.map(|v| -v);
    }
    let mut polynomial = BLEND_COEFFICIENTS[0];
    for c in &BLEND_COEFFICIENTS[1..] {
        polynomial = polynomial * weight + c;
    }
    let fraction = weight + polynomial * (1.0 - dot.abs());
    normalize(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * fraction))
}
fn pose_rotations(library: &HandLibrary, selector: [i64; 4], mirrored: bool) -> Result<Vec<Quat>> {
    let identity = vec![[0.0, 0.0, 0.0, 1.0]; 17];
    if selector[0] == 0 && selector[2] == 0 {
        return Ok(identity);
    }
    ensure!(selector[0] + selector[2] == 255, "Hand weights must sum to 255");
    let mut result = identity;
    let mut total = 0;
    for (weight, pose) in [(selector[0], selector[1]), (selector[2], selector[3])] {
        if weight == 0 {
            break;
        }
        let rotations = library.poses.get(pose as usize).context("Hand pose ID exceeds source library")?;
        total += weight;
        result = result
            .iter()
            .zip(rotations)
            .map(|(a, b)| blend_rotation(*a, if mirrored { mirror_rotation(*b) } else { *b }, weight as f64 / total as f64))
            .collect();
    }
    Ok(result)
}
struct HandSelectors {
    streams: Vec<Option<Vec<(i64, i64)>>>,
    speed: f64,
    mirrored: bool,
}
impl HandSelectors {
    fn new(clip: &SingleClip) -> Result<Self> {
        ensure!(clip.bone_count == 22, "Hand selectors require the human track map");
        let count = (clip.flags >> 25) & 7;
        let end = (300.0 * clip.ticks.last().unwrap() / clip.rate as f64).ceil() as i64;
        let mut streams = Vec::new();
        for i in 0..count.min(2) as usize {
            let offset = u16_at(&clip.data, clip.payload + 28 + 2 * i)? as usize;
            if offset == 0 {
                streams.push(None);
                continue;
            }
            let start = clip.payload + 2 * offset;
            let mut events = Vec::new();
            let mut previous: Option<i64> = None;
            let mut pos = start;
            let mut ended = false;
            // Native streams end with a sentinel at/after playback end.
            while pos + 3 < clip.data.len() {
                let word = i32_at(&clip.data, pos)?;
                let tick = (word >> 8) as i64;
                ensure!(previous.is_none_or(|p| tick >= p), "Hand event clock is not ordered");
                events.push((tick, (word & 255) as i64));
                previous = Some(tick);
                if tick >= end && events.len() >= 2 {
                    ended = true;
                    break;
                }
                pos += 4;
            }
            ensure!(ended, "Hand event stream has no end sentinel");
            streams.push(Some(events));
        }
        while streams.len() < 2 {
            streams.push(None);
        }
        Ok(Self { streams, speed: clip.speed, mirrored: clip.mirrored })
    }
    fn sample(&self, seconds: f64) -> Result<Vec<[i64; 4]>> {
        ensure!(seconds.is_finite() && seconds >= 0.0, "Invalid hand sample time");
        let tick = f32r(f32r(f32r(seconds) * self.speed) * 300.0).round_ties_even() as i64;
        let mut result = Vec::new();
        for events in &self.streams {
            let Some(events) = events else {
                result.push([0, 0, 0, 0]);
                continue;
            };
            let (mut lo, mut hi) = (1usize, events.len());
            while lo < hi {
                let mid = (lo + hi) / 2;
                if events[mid].0 < tick { lo = mid + 1 } else { hi = mid }
            }
            let j = lo.min(events.len() - 1).max(1);
            let (a, b) = (events[j - 1], events[j]);
            if a.1 == b.1 {
                result.push([255, a.1, 0, 0]);
                continue;
            }
            ensure!(b.0 > a.0, "Changing hand poses share the same event tick");
            let fraction = f32r((tick - a.0) as f64 / (b.0 - a.0) as f64).min(1.0).max(0.0);
            let smooth = f32r(f32r(fraction * fraction) * f32r(3.0 - f32r(fraction + fraction)));
            let weight = f32r(smooth * 255.0).round_ties_even() as i64;
            result.push(if weight <= 127 { [255 - weight, a.1, weight, b.1] } else { [weight, b.1, 255 - weight, a.1] });
        }
        if self.mirrored {
            result.reverse();
        }
        Ok(result)
    }
}

fn finite(value: &Value) -> bool {
    match value {
        Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
        Value::Array(a) => a.iter().all(finite),
        Value::Object(o) => o.values().all(finite),
        _ => true,
    }
}

/// animation_lab.decode_loaded_clip for one resolved record.
pub fn decode_loaded_clip(
    clip: &SingleClip,
    archive: &str,
    name: &str,
    fps: u32,
    container: Option<(&MultipleClip, usize)>,
    hand_library: Option<&HandLibrary>,
) -> Result<Map<String, Value>> {
    ensure!(fps > 0, "FPS must be positive");
    ensure!((1..=63).contains(&clip.bone_count), "Spatial clip needs between 1 and 63 joints");
    let hand_library = if clip.skeleton_kind == "human" { hand_library } else { None };
    ensure!(clip.rate > 0 && clip.speed > 0.0 && clip.speed.is_finite(), "Clip has invalid playback timing");
    let selectors = hand_library.map(|_| HandSelectors::new(clip)).transpose()?;
    // Nominal duration is descriptive; source key times and speed set actual playback.
    let seconds = clip.ticks.last().unwrap() / (clip.rate as f64 * clip.speed);
    let count = (seconds * fps as f64).ceil() as usize + 1;
    let mut frames = Vec::with_capacity(count);
    for f in 0..count {
        let time = (f as f64 / fps as f64).min(seconds);
        let (q, t) = clip.sample(time)?;
        let (x, z, heading) = sample_trajectory(clip, time)?;
        let mut frame = json!({"frame":f + 1,"time":time,"rotations_xyzw":q,"translations_cm":t,
            "props":clip.sample_props(time)?,"trajectory":{"position_xz_cm":[x, z],"heading_units":heading}});
        if let (Some(library), Some(selectors)) = (hand_library, &selectors) {
            let selection = selectors.sample(time)?;
            let rotations = selection
                .iter()
                .enumerate()
                .map(|(side, s)| pose_rotations(library, *s, side == 0))
                .collect::<Result<Vec<_>>>()?;
            frame["hand_selectors"] = json!(selection);
            frame["hand_rotations_xyzw"] = json!(rotations);
        }
        frames.push(frame);
    }
    let human = clip.skeleton_kind == "human";
    let mut obj = json!({
        "schema": if human { "nba2k27-body-clip-v1" } else { "nba2k27-spatial-clip-v1" },
        "name": clip.name, "source_archive": archive, "skeleton_kind": clip.skeleton_kind,
        "mirror_map": clip.mirror_map, "source_sha256": sha256_hex(&clip.source_data), "fps": fps,
        "duration": seconds, "resolved_motion_sha256": sha256_hex(&clip.data), "source_clip": name,
        "resolved_clip": clip.resolved_name, "mirrored": clip.mirrored,
        "status": "Native ordinary body, prop and movement cubic sampling, root scaling, and human mirroring verified offline",
        "bone_names": BONE_NAMES, "parents": clip.parents,
        "hierarchy_status": "Anatomical mapping inferred from 22 source bind offsets; player mesh retargeting pending",
        "bind_offsets_cm": clip.bind.iter().map(|b| [b.0, b.1, b.2]).collect::<Vec<_>>(),
        "key_ticks": clip.ticks.iter().map(|t| *t as i64).collect::<Vec<_>>(), "frames": frames,
    });
    let map = obj.as_object_mut().unwrap();
    if !human {
        map.insert("bone_names".into(), json!((0..clip.bone_count).map(|i| format!("joint_{i}")).collect::<Vec<_>>()));
        map.insert("hierarchy_status".into(), json!("Serialized bind hierarchy; native crowd or default spatial destination map"));
        map.insert(
            "status".into(),
            json!("Native ordinary spatial/movement sampling verified offline; markers represent tracks without source geometry"),
        );
    }
    if let Some(library) = hand_library {
        map.insert("hand_pose_source".into(), library.source_json());
    }
    if let Some((container, actor)) = container {
        map.insert("container_type".into(), json!("MultipleMoCap"));
        map.insert("actor_index".into(), json!(actor));
        map.insert("actor_count".into(), json!(container.actor_count));
        map.insert("container_actors".into(), Value::Array(container.actors.iter().map(Actor::to_json).collect()));
        map.insert(
            "placement_status".into(),
            json!("Native actor origin/heading composition verified offline; player hierarchy inferred"),
        );
        map.insert("container_placement".into(), container.actors[actor].to_json());
        map.insert(
            "packed_export_status".into(),
            json!("Private actor payload reconstruction and container export supported offline"),
        );
    }
    ensure!(finite(&obj), "Motion contains nonfinite values");
    let Value::Object(map) = obj else { unreachable!() };
    Ok(map)
}

// character_mesh.py: an insertion-ordered JSON reader for SCNE text (bone
// order is the Transform key order, which serde_json's map does not keep).
#[derive(Clone, Debug)]
pub enum Node {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Node>),
    Obj(Vec<(String, Node)>),
}
impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn num(&self) -> Option<f64> {
        if let Node::Num(n) = self { Some(*n) } else { None }
    }
    fn str(&self) -> Option<&str> {
        if let Node::Str(s) = self { Some(s) } else { None }
    }
    fn int(&self, key: &str, default: i64) -> Result<i64> {
        match self.get(key) {
            None => Ok(default),
            Some(Node::Num(n)) if n.fract() == 0.0 => Ok(*n as i64),
            _ => bail!("Character scene field {key} is not an integer"),
        }
    }
    fn nums(&self, key: &str) -> Option<Vec<f64>> {
        match self.get(key)? {
            Node::List(items) => items.iter().map(Node::num).collect(),
            _ => None,
        }
    }
}
struct Parser<'a> {
    b: &'a [u8],
    p: usize,
}
impl Parser<'_> {
    fn ws(&mut self) {
        while self.p < self.b.len() && matches!(self.b[self.p], b' ' | b'\t' | b'\r' | b'\n') {
            self.p += 1;
        }
    }
    fn value(&mut self, depth: usize) -> Result<Node> {
        ensure!(depth < 128, "Character scene nesting is too deep");
        self.ws();
        let c = *self.b.get(self.p).context("Truncated character scene")?;
        Ok(match c {
            b'{' => {
                self.p += 1;
                let mut items: Vec<(String, Node)> = Vec::new();
                self.ws();
                if self.b.get(self.p) == Some(&b'}') {
                    self.p += 1;
                    return Ok(Node::Obj(items));
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    ensure!(self.b.get(self.p) == Some(&b':'), "Invalid character scene object");
                    self.p += 1;
                    let value = self.value(depth + 1)?;
                    // Python dict semantics: a repeated key keeps its first position.
                    match items.iter_mut().find(|(k, _)| *k == key) {
                        Some(slot) => slot.1 = value,
                        None => items.push((key, value)),
                    }
                    self.ws();
                    match self.b.get(self.p) {
                        Some(b',') => self.p += 1,
                        Some(b'}') => {
                            self.p += 1;
                            break;
                        }
                        _ => bail!("Invalid character scene object"),
                    }
                }
                Node::Obj(items)
            }
            b'[' => {
                self.p += 1;
                let mut items = Vec::new();
                self.ws();
                if self.b.get(self.p) == Some(&b']') {
                    self.p += 1;
                    return Ok(Node::List(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.p) {
                        Some(b',') => self.p += 1,
                        Some(b']') => {
                            self.p += 1;
                            break;
                        }
                        _ => bail!("Invalid character scene list"),
                    }
                }
                Node::List(items)
            }
            b'"' => Node::Str(self.string()?),
            b't' if self.b[self.p..].starts_with(b"true") => {
                self.p += 4;
                Node::Bool(true)
            }
            b'f' if self.b[self.p..].starts_with(b"false") => {
                self.p += 5;
                Node::Bool(false)
            }
            b'n' if self.b[self.p..].starts_with(b"null") => {
                self.p += 4;
                Node::Null
            }
            _ => {
                let start = self.p;
                while self.p < self.b.len() && matches!(self.b[self.p], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
                    self.p += 1;
                }
                let text = std::str::from_utf8(&self.b[start..self.p])?;
                Node::Num(text.parse().with_context(|| format!("Invalid character scene number {text:?}"))?)
            }
        })
    }
    fn string(&mut self) -> Result<String> {
        ensure!(self.b.get(self.p) == Some(&b'"'), "Expected a character scene string");
        let start = self.p;
        self.p += 1;
        while self.p < self.b.len() && self.b[self.p] != b'"' {
            if self.b[self.p] == b'\\' {
                self.p += 1;
            }
            self.p += 1;
        }
        ensure!(self.p < self.b.len(), "Truncated character scene string");
        self.p += 1;
        Ok(serde_json::from_slice(&self.b[start..self.p])?)
    }
}
pub fn parse_ordered(text: &str) -> Result<Node> {
    let mut parser = Parser { b: text.as_bytes(), p: 0 };
    let value = parser.value(0)?;
    parser.ws();
    ensure!(parser.p == text.len(), "Trailing data in character scene");
    Ok(value)
}

const MAIN_JOINTS: [&str; 22] = [
    "def_hips", "def_spine", "def_spine1", "def_spine2", "def_neck", "def_head", "def_leftShoulder", "def_leftArm",
    "def_leftForearm", "def_leftHand", "def_rightShoulder", "def_rightArm", "def_rightForearm", "def_rightHand",
    "def_leftUpLeg", "def_leftLeg", "def_leftFoot", "def_leftToeBase", "def_rightUpLeg", "def_rightLeg",
    "def_rightFoot", "def_rightToeBase",
];
pub struct Character {
    pub positions_cm: Vec<Vec3>,
    pub skin: Vec<Vec<(usize, f64)>>,
    pub triangles: Vec<[u16; 3]>,
    pub materials: Vec<String>,
    pub bone_names: Vec<String>,
    pub bone_parents: Vec<i64>,
    pub bone_rest_cm: Vec<Vec3>,
    pub main_joint_indices: Vec<usize>,
    pub source_sha256: String,
}
/// read_character: the extracted png04090 IFF (SCNE as text, buffers as .bin).
pub fn read_character(path: &Path) -> Result<Character> {
    let bytes = std::fs::read(path)?;
    let source_sha256 = sha256_hex(&bytes);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let files: HashMap<String, String> = names.iter().map(|n| (n.to_lowercase(), n.clone())).collect();
    let scene = names.iter().find(|n| n.to_lowercase() == "hihead.scne").context("Character IFF has no hihead.SCNE")?.clone();
    let mut read = |name: &str| -> Result<Vec<u8>> {
        let mut member = archive.by_name(name)?;
        ensure!(member.size() <= MAX_MEMBER, "Character member exceeds its byte budget");
        let expected=member.size();
        read_member_bounded(member,expected,MAX_MEMBER)
    };
    let raw = read(&scene)?;
    let text = String::from_utf8(raw)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let text: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
    let text = text.join("\n");
    let document = parse_ordered(&if text.trim_start().starts_with('{') { text } else { format!("{{{text}}}") })?;
    let Node::Obj(top) = &document else { bail!("Character scene has no model") };
    let root = top.iter().map(|(_, v)| v).find(|v| v.get("Model").is_some()).context("Character scene has no model")?;
    let Some(Node::Obj(models)) = root.get("Model") else { bail!("Character scene has no model") };
    let model = models
        .iter()
        .map(|(_, v)| v)
        .find(|v| v.get("Transform").is_some() && v.get("MatrixWeightsBuffer").is_some())
        .context("Character scene has no skinned model")?;
    let mut buffer = |info: &Node| -> Result<Vec<u8>> {
        let name = info.get("Binary").and_then(Node::str).context("Missing character buffer name")?;
        let bin = format!("{}.bin", name.rsplit_once('.').map(|p| p.0).unwrap_or(name));
        let candidate = files
            .get(&name.to_lowercase())
            .or_else(|| files.get(&bin.to_lowercase()))
            .with_context(|| format!("Missing character buffer {name}"))?
            .clone();
        let mut value = read(&candidate)?;
        if value.starts_with(b"\x1f\x8b") {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(&value[..]).take(MAX_MEMBER + 1).read_to_end(&mut out)?;
            value = out;
        }
        ensure!(
            value.len() as i64 == info.int("Size", -1)?,
            "Character buffer size disagrees with metadata: {name}"
        );
        Ok(value)
    };
    let formats = model.get("VertexFormat").context("Character has no vertex format")?;
    let streams = match model.get("VertexStream") {
        Some(Node::List(s)) => s,
        _ => bail!("Character has no vertex streams"),
    };
    let position = formats.get("POSITION0").context("Character has no positions")?;
    let weights = formats.get("WEIGHTDATA0").context("Character has no skin weights")?;
    ensure!(
        position.get("Format").and_then(Node::str) == Some("R21G21B22_UINT")
            && weights.get("Format").and_then(Node::str) == Some("R32_UINT")
            && model.int("WeightBits", 0)? == 16,
        "Character preview requires the verified packed position/16-bit weight format"
    );
    let stream = |format: &Node| -> Result<&Node> {
        streams.get(format.int("Stream", 0)? as usize).context("Invalid character vertex stream")
    };
    let pstream = stream(position)?;
    let pdata = buffer(pstream)?;
    let stride = pstream.int("Stride", 0)? as usize;
    let poff = position.int("ByteOffset", 0)? as usize;
    ensure!(stride >= poff + 8 && stride > 0 && pdata.len() % stride == 0, "Invalid character position stream");
    let count = pdata.len() / stride;
    let lod_verts = match model.get("Lods") {
        Some(Node::List(l)) => l.first().map(|l| l.int("LodVerts", -1)).transpose()?.unwrap_or(-1),
        _ => -1,
    };
    ensure!(count as i64 == lod_verts, "Unexpected character LOD vertex layout");
    let wstream = stream(weights)?;
    let wdata = buffer(wstream)?;
    let wstride = wstream.int("Stride", 0)? as usize;
    let woff = weights.int("ByteOffset", 0)? as usize;
    ensure!(wstride >= woff + 4 && wdata.len() == count * wstride, "Invalid character weight stream");
    let matrix = buffer(model.get("MatrixWeightsBuffer").unwrap())?;
    let Some(Node::Obj(transforms)) = model.get("Transform") else { bail!("Character has no joints") };
    let names: Vec<String> = transforms.iter().map(|(k, _)| k.clone()).collect();
    let mut rest: Vec<Vec3> = Vec::new();
    let mut parents = Vec::new();
    for (i, (_, bone)) in transforms.iter().enumerate() {
        let parent = bone.int("Parent", -1)?;
        ensure!(parent < i as i64 && parent >= -1, "Character hierarchy is not ordered");
        let local = bone.nums("Translate").unwrap_or_else(|| vec![0.0; 3]);
        ensure!(local.len() == 3 && local.iter().all(|v| v.is_finite()), "Invalid character joint translation");
        ensure!(
            ["Rotate", "Rotation", "Scale", "Matrix"].iter().all(|k| bone.get(k).is_none()),
            "Character preview requires a translation-only bind hierarchy"
        );
        let base = if parent >= 0 { rest[parent as usize] } else { [0.0; 3] };
        // Python: v + (rest[parent][k] if parent>=0 else 0); 0 is int, same value.
        rest.push(std::array::from_fn(|k| local[k] + base[k]));
        parents.push(parent);
    }
    let main: Vec<usize> = MAIN_JOINTS
        .iter()
        .map(|n| names.iter().position(|m| m == n).context("Character is missing a required human joint"))
        .collect::<Result<_>>()?;
    let pscale = position.nums("Scale").filter(|v| v.len() >= 3).context("Invalid character position scale")?;
    let poffset = position.nums("Offset").filter(|v| v.len() >= 3).context("Invalid character position offset")?;
    let mut positions = Vec::with_capacity(count);
    let mut skin = Vec::with_capacity(count);
    for i in 0..count {
        let word = u64_at(&pdata, i * stride + poff)?;
        let raw = [word & 0x1fffff, (word >> 21) & 0x1fffff, (word >> 42) & 0x3fffff];
        let p: Vec3 = std::array::from_fn(|k| raw[k] as f64 * pscale[k] + poffset[k]);
        ensure!(p.iter().all(|v| v.is_finite()), "Nonfinite character vertex");
        positions.push(p);
        let word = u32_at(&wdata, i * wstride + woff)?;
        let (n, index) = ((word & 255) as usize, (word >> 8) as usize);
        let values: Vec<(usize, f64)> = if n == 0 {
            vec![(index, 1.0)]
        } else {
            ensure!((index + n + 1) * 4 <= matrix.len(), "Skin weight pointer is out of range");
            let packed: Vec<u32> = (0..=n).map(|k| u32_at(&matrix, (index + k) * 4)).collect::<Result<_>>()?;
            ensure!(packed.iter().map(|v| (v & 65535) as u64).sum::<u64>() == 65535, "Packed skin weights do not sum to 65535");
            packed.iter().filter(|v| *v & 65535 != 0).map(|v| ((v >> 16) as usize, (v & 65535) as f64 / 65535.0)).collect()
        };
        ensure!(!values.is_empty() && values.iter().all(|(j, _)| *j < names.len()), "Invalid character skin joint");
        skin.push(values);
    }
    let index_info = model.get("IndexBuffer").context("Character has no index buffer")?;
    ensure!(index_info.get("Format").and_then(Node::str) == Some("R16_UINT"), "Unsupported character index format");
    let index_data = buffer(index_info)?;
    let mut triangles = Vec::new();
    let mut materials = Vec::new();
    let mut last_material = "skin".to_owned();
    let prims = match model.get("Prim") {
        Some(Node::List(p)) => p,
        _ => bail!("Character has no primitives"),
    };
    for prim in prims {
        if let Some(m) = prim.get("Material").and_then(Node::str) {
            last_material = m.to_owned();
        }
        ensure!(
            prim.get("Type").and_then(Node::str).unwrap_or("TRIANGLE_LIST") == "TRIANGLE_LIST",
            "Unsupported character primitive"
        );
        let (start, n) = (prim.int("Start", -1)?, prim.int("Count", -1)?);
        ensure!(
            start >= 0 && n >= 0 && n % 3 == 0 && ((start + n) * 2) as usize <= index_data.len(),
            "Invalid character primitive index range"
        );
        // Keep body surfaces hidden by the game's clothes; alpha-tested hair cards need source shaders.
        if matches!(last_material.as_str(), "eyelashes_shader" | "l_armpitHair_shader" | "r_armpitHair_shader") {
            continue;
        }
        let mut at = start as usize;
        while at < (start + n) as usize {
            let tri = [u16_at(&index_data, at * 2)?, u16_at(&index_data, at * 2 + 2)?, u16_at(&index_data, at * 2 + 4)?];
            ensure!(tri.iter().all(|v| (*v as usize) < count), "Character triangle has an invalid vertex");
            triangles.push(tri);
            materials.push(last_material.clone());
            at += 3;
        }
    }
    Ok(Character {
        positions_cm: positions,
        skin,
        triangles,
        materials,
        bone_names: names,
        bone_parents: parents,
        bone_rest_cm: rest,
        main_joint_indices: main,
        source_sha256,
    })
}

/// engine.character(): finger_rig + the neutral preview payload.
pub fn character_payload(source: &Character) -> Result<Value> {
    let names = &source.bone_names;
    let parents = &source.bone_parents;
    let mut native: Vec<usize> = source.main_joint_indices.clone();
    let mut lookup: HashMap<usize, usize> = native.iter().enumerate().map(|(i, j)| (*j, i)).collect();
    let mut rig_names: Vec<String> = MAIN_JOINTS.iter().map(|s| s.to_string()).collect();
    let mut rig_parents: Vec<i64> = HUMAN_PARENTS.to_vec();
    let mut hands = Vec::new();
    for (side, prefix) in [(0, "def_l_"), (1, "def_r_")] {
        for (joint, name) in names.iter().enumerate() {
            if !name.starts_with(prefix) || !["thumb_", "finger_", "hand_edge"].iter().any(|s| name.contains(s)) {
                continue;
            }
            let parent = usize::try_from(parents[joint]).ok().and_then(|p| lookup.get(&p).copied());
            let parent = parent.context("Finger parent is missing from the retained rig")?;
            let index = native.len();
            lookup.insert(joint, index);
            native.push(joint);
            rig_names.push(name.clone());
            rig_parents.push(parent as i64);
            let pose_node = HAND_NODES.iter().enumerate().skip(1).find(|(_, n)| format!("{prefix}{n}") == *name).map(|(i, _)| i);
            hands.push(json!({"joint":index,"side":side,"pose_node":pose_node}));
        }
    }
    ensure!(hands.len() == 42, "Expected 21 retained finger/edge joints per hand");
    let mut mapped = Vec::with_capacity(names.len());
    for joint in 0..names.len() {
        let mut current = joint as i64;
        while !lookup.contains_key(&(current as usize)) {
            current = parents[current as usize];
            ensure!(current >= 0, "Character joint has no retained ancestor");
        }
        mapped.push(lookup[&(current as usize)]);
    }
    let mut skin_index = Vec::new();
    let mut skin_weight = Vec::new();
    for influences in &source.skin {
        // dict merge in first-seen order, then sorted by joint (Python sorted(items)).
        let mut merged: Vec<(usize, f64)> = Vec::new();
        for (joint, weight) in influences {
            let m = mapped[*joint];
            match merged.iter_mut().find(|(j, _)| *j == m) {
                Some(slot) => slot.1 += weight,
                None => merged.push((m, *weight)),
            }
        }
        merged.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        ensure!(merged.len() <= 4, "Character skin requires an unsupported number of influences");
        for k in 0..4 {
            skin_index.push(merged.get(k).map(|m| m.0).unwrap_or(0));
            skin_weight.push(merged.get(k).map(|m| m.1).unwrap_or(0.0));
        }
    }
    let mut groups: Vec<Value> = Vec::new();
    let mut indices = Vec::with_capacity(source.triangles.len() * 3);
    let mut last = None;
    for (triangle, shader) in source.triangles.iter().zip(&source.materials) {
        let mat = match shader.as_str() {
            "torso_shader" | "arms_shader" | "legs_shader" | "hidden_shader" => 1,
            "l_eye_shader" | "r_eye_shader" => 2,
            "mouth_shader" => 3,
            _ => 0,
        };
        if Some(mat) != last {
            groups.push(json!({"start":indices.len(),"count":0,"material":mat}));
            last = Some(mat);
        }
        indices.extend_from_slice(triangle);
        let group = groups.last_mut().unwrap();
        group["count"] = json!(group["count"].as_u64().unwrap() + 3);
    }
    let payload = json!({
        "rig_names": rig_names, "rig_parents": rig_parents,
        "rig_rest_cm": native.iter().map(|i| source.bone_rest_cm[*i]).collect::<Vec<_>>(),
        "hand_joints": hands,
        "schema": "nba2k27-preview-character-v1", "name": "Neutral player preview",
        "source_sha256": source.source_sha256,
        "positions_cm": source.positions_cm.iter().flatten().collect::<Vec<_>>(),
        "main_rest_cm": source.main_joint_indices.iter().map(|i| source.bone_rest_cm[*i]).collect::<Vec<_>>(),
        "skin_index": skin_index, "skin_weight": skin_weight, "indices": indices, "groups": groups,
        "vertex_count": source.positions_cm.len(), "triangle_count": source.triangles.len(),
        "status": "Neutral body with authored hand poses; approximate helper adapter",
    });
    ensure!(finite(&payload), "Character contains nonfinite values");
    Ok(payload)
}

#[cfg(test)]
mod tests {
    #[test]
    fn archive_member_reads_cap_expansion_and_reject_declared_size_drift() {
        use super::read_member_bounded;
        assert_eq!(read_member_bounded(&b"abc"[..],3,4).unwrap(),b"abc");
        assert!(read_member_bounded(&b"abc"[..],2,4).is_err());
        assert!(read_member_bounded(&b"a"[..],2,4).is_err());
        let mut source=std::io::Cursor::new([0;12]);assert!(read_member_bounded(&mut source,4,4).is_err());assert_eq!(source.position(),5);
    }
    use super::*;
    #[test]
    fn rotation_words_are_unit_quaternions() {
        for word in [0u32, 1, 0x7fffffff, 0x80000000, 0xffffffff, 0x12345678] {
            let q = decode_rotation(word);
            assert!((sum_sq(&q) - 1.0).abs() < 1e-12, "{word:#x}");
        }
    }
    #[test]
    fn signed_fields_and_translation() {
        assert_eq!(signed(0x3ff, 10), -1);
        assert_eq!(signed(0x1ff, 10), 511);
        assert_eq!(decode_translation(0x3ff, 0), [-1.0, 0.0, 0.0]);
    }
    #[test]
    fn ordered_scene_keeps_key_order_and_first_position() {
        let node = parse_ordered(r#"{"b":1,"a":{"z":[1,2.5e1,-3],"y":"x\"q"},"b":2}"#).unwrap();
        let Node::Obj(items) = &node else { panic!() };
        assert_eq!(items.iter().map(|i| i.0.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(items[0].1.num(), Some(2.0));
        assert_eq!(node.get("a").unwrap().nums("z").unwrap(), vec![1.0, 25.0, -3.0]);
        assert_eq!(node.get("a").unwrap().get("y").unwrap().str(), Some("x\"q"));
    }
    #[test]
    fn hand_blend_matches_identity_for_full_weight() {
        let q = normalize([0.1, 0.2, 0.3, 0.9]);
        let r = blend_rotation([0.0, 0.0, 0.0, 1.0], q, 1.0);
        for k in 0..4 {
            assert!((r[k] - q[k]).abs() < 1e-3);
        }
    }
}
