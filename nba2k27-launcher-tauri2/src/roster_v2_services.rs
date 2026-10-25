//! Version 2 team staff and player season services, ported from the WPF
//! helper (TeamStaff.cs, TeamStaffPatch.cs, SeasonStats.cs) and its JS
//! callers (core/team-staff.js, core/player-season-stats.js,
//! renderer/player-season-awards.js). Pure decode, validation and planning:
//! native reads and the guarded write lease live in roster_v2_access, the
//! journal and transaction in roster_v2_write.
//!
//! V2 evidence (tools/editor_fields_v2_audit.py, tools/roster_v2_owner_directory_scan.py,
//! live probe artifacts/editor-services-live-20261017-164403.json): the staff
//! serializer, season getters and directory accessors below are pinned by
//! exact bytes before any staff/season read or write.
use crate::{
    blacktop_native::Memory,
    roster_v2_profile as profile,
    roster_v2_write::{Kind, Patch, Plan},
};
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

pub(crate) const STAFF_STRIDE: usize = 0x1C8;
pub(crate) const STAFF_HEADER: u64 = 0x180;
pub(crate) const TEAM_STAFF_SLOTS: usize = 0xF70;
pub(crate) const STAFF_OWNER: usize = 0x18;
pub(crate) const SEASON_STRIDE: usize = 64;
pub(crate) const SEASON_HEADER: u64 = 0x1D0;
pub(crate) const PLAYER_SEASONS: usize = 0x130;
pub(crate) const PLAYER_PRIVATE_SEASONS: usize = 0xE0;
pub(crate) const REGULAR_SEASONS: usize = 25;
pub(crate) const ROTATIONS: usize = 0x1360;
pub(crate) const ROTATION_BYTES: usize = 480;
pub(crate) const ROTATION_FLAG: usize = 0x154C;

/// Exact V2 code bytes for the structures these services touch.
const ANCHORS: &[(u64, &str)] = &[
    (0x2f18cd6, "4180fc20"),                         // 32 team staff slots
    (0x2f18ce5, "4a8b94e0700f0000"),                 // team+0xF70+slot*8
    (0x2f18cfb, "41b8c8010000"),                     // staff record 0x1C8
    (0x2f18d2d, "0fb6868201000083e01f"),             // slot = staff+0x182 & 0x1F
    (0x2f1169d, "4c39a4d0700f0000"),                 // team slot points back to staff
    (0x2f116fe, "498b4424180fb780200f0000"),         // staff+0x18 -> team, team id +0xF20
    (0x2f0b777, "83bec001000000740c488bb6c8010000"), // season pool kind/pool
    (0x2f063b6, "48638ed00100004885c97e4d48c1e106"), // owner+0x1D0 count, 64-byte records
    (
        0x2edbc71,
        "3b9180010000731589d04869c0c801000048038188010000",
    ), // owner+0x180/0x1C8
    (0x2e0a931, "3b91d0010000731289d048c1e006480381d8010000"), // owner+0x1D0/0x40
    (0x2f27bb0, "488d8f60130000488d8660130000"),     // rotations team+0x1360 block
    (0x2f27c34, "483de0010000"),                     // 0x1E0 bytes = 48 x 5 u16
    (0x2f86da0, "0fb6814c1500002401c3"),             // has advanced rotation = +0x154C & 1
];
/// The functions WPF pinned, remapped to V2 with identical lengths.
const SPANS: &[(u64, usize, &str)] = &[
    (
        0x6bba00,
        17580,
        "ece8e2e6fef903eb501f0b6582f1b6d0a0ff9180fa1c877a9ca8393542618c2f",
    ), // StaffData serializer
    (
        0x2e07070,
        561,
        "16611177ce615333caecdc1d61124f7070d75612cd759993fd3a7c6582d11365",
    ), // PlayerStatData_Get
    (
        0x2e072b0,
        577,
        "e0b1c149c0fe9cbdbe2cb87cca3c27c4d111fad36d206695feb528d23a31d654",
    ), // PLAYERSTATDATA::GetByIndex
    (
        0x2e5b520,
        10,
        "e6cb9c82fe8d5804d4ac471688bdb7a91c150b7efe7f46b1845545aa605feb09",
    ), // PLAYERSTATDATA::GetYear
];
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
/// One bounded check per staff/season operation (on top of the roster
/// identity profile): ~20 small reads plus the 17.6 KB staff serializer.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    profile::verify_identity_profile(m)?;
    for &(rva, hex) in ANCHORS {
        let expected = unhex(hex);
        m.checkpoint()?;
        ensure!(
            m.read(m.base() + rva, expected.len())? == expected,
            "This game's staff/season layout is unsupported. No staff or season data was read."
        );
    }
    for &(rva, len, sha) in SPANS {
        let mut hash = Sha256::new();
        for offset in (0..len).step_by(4096) {
            m.checkpoint()?;
            let n = (len - offset).min(4096);
            let bytes = m.read(m.base() + rva + offset as u64, n)?;
            ensure!(bytes.len() == n, "Partial staff/season profile read");
            hash.update(bytes);
        }
        ensure!(
            format!("{:x}", hash.finalize()) == sha,
            "This game's staff/season layout is unsupported. No staff or season data was read."
        );
    }
    m.checkpoint()
}

/// ISO-8601 UTC timestamp (JS `new Date().toISOString()` shape).
pub(crate) fn utc_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (secs, millis) = (d.as_secs() as i64, d.subsec_millis());
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

// ---------------------------------------------------------------- staff ----
#[derive(Deserialize)]
struct Policy {
    schema: String,
    stride: usize,
    masks: Vec<u8>,
    ranges: Vec<(usize, u32, u32, i64, i64, bool, bool)>,
}
fn policy() -> &'static Policy {
    static P: OnceLock<Policy> = OnceLock::new();
    P.get_or_init(|| {
        let p: Policy = serde_json::from_str(include_str!("../assets/staff-patch-policy.json"))
            .expect("WPF staff patch policy");
        assert!(
            p.schema == "nba2k27-staff-patch-policy-v1"
                && p.stride == STAFF_STRIDE
                && p.masks.len() == STAFF_STRIDE
                && p.ranges.len() == 173
        );
        p
    })
}
pub(crate) fn controls() -> &'static [Value] {
    static C: OnceLock<Vec<Value>> = OnceLock::new();
    C.get_or_init(|| {
        let c: Vec<Value> = serde_json::from_str(include_str!("../assets/staff-controls.json"))
            .expect("WPF staff controls");
        assert_eq!(c.len(), 175);
        c
    })
}
pub(crate) fn schema() -> &'static Value {
    static S: OnceLock<Value> = OnceLock::new();
    S.get_or_init(|| {
        let s: Value = serde_json::from_str(include_str!("../assets/staff-field-schema.json"))
            .expect("WPF staff schema");
        assert!(s["roles"].as_array().is_some_and(|r| r.len() == 32));
        s
    })
}
pub(crate) fn staff_masks() -> &'static [u8] {
    &policy().masks
}
const SCALARS: [(&str, usize); 24] = [
    ("BenchDepth", 0x158),
    ("LineupFormFactor", 0x159),
    ("RunPlayTendency", 0x15a),
    ("DoubleTeamTendency", 0x15b),
    ("ZoneUsage", 0x15c),
    ("ShotInsideTendency", 0x15d),
    ("ShotCloseTendency", 0x15e),
    ("ShotMediumTendency", 0x15f),
    ("ShotThreeTendency", 0x160),
    ("DriveTendency", 0x161),
    ("PostUpTendency", 0x162),
    ("AlleyOopTendency", 0x163),
    ("Tempo", 0x164),
    ("HelpDefense", 0x165),
    ("Motivation", 0x166),
    ("Intensity", 0x167),
    ("BasketballIQ", 0x168),
    ("WorkEthic", 0x169),
    ("Offense", 0x16a),
    ("Defense", 0x16b),
    ("Business", 0x16c),
    ("Training", 0x16d),
    ("Charisma", 0x16e),
    ("Analytics", 0x16f),
];
const SYSTEMS: [&str; 8] = [
    "Balanced",
    "Grit and Grind",
    "Pace and Space",
    "Perimeter Centric",
    "Post Centric",
    "Triangle",
    "Seven Seconds",
    "Defense",
];
fn n(f: &Value, key: &str) -> Option<i64> {
    f[key].as_i64()
}
fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
/// WPF name(): UTF-16 terminated inside 40 bytes, no control/replacement chars.
pub(crate) fn staff_name(b: &[u8], at: usize) -> Result<String> {
    let mut end = at;
    while end < at + 40 && u16le(b, end) != 0 {
        end += 2;
    }
    ensure!(end < at + 40, "Unterminated staff name.");
    let words: Vec<u16> = (at..end).step_by(2).map(|i| u16le(b, i)).collect();
    let s = String::from_utf16(&words).map_err(|_| anyhow::anyhow!("Invalid staff name."))?;
    ensure!(
        !s.chars()
            .any(|c| c <= '\u{1f}' || c == '\u{7f}' || c == '\u{fffd}'),
        "Invalid staff name."
    );
    Ok(s)
}
fn bits_value(b: &[u8], offset: usize, shift: u32, bits: u32) -> u64 {
    (0..bits as usize).fold(0, |v, i| {
        let g = offset * 8 + shift as usize + i;
        v | (u64::from((b[g / 8] >> (g % 8)) & 1) << i)
    })
}
fn set_bits(b: &mut [u8], offset: usize, shift: u32, bits: u32, value: u64) {
    for i in 0..bits as usize {
        let g = offset * 8 + shift as usize + i;
        let mask = 1u8 << (g % 8);
        b[g / 8] = (b[g / 8] & !mask) | if (value >> i) & 1 == 1 { mask } else { 0 };
    }
}
fn control_value(b: &[u8], f: &Value) -> Result<Value> {
    let offset = n(f, "offset").context("Invalid staff control")? as usize;
    if f["text"] == true {
        return Ok(json!(staff_name(b, offset)?));
    }
    let shift = n(f, "shift").unwrap_or(0) as u32;
    let bits = n(f, "bits").context("Invalid staff control")? as u32;
    let v = bits_value(b, offset, shift, bits) as i64;
    Ok(json!(if f["signed"] == true && v >= 32768 {
        v - 65536
    } else {
        v
    }))
}
fn label(key: &str) -> String {
    let key = key.strip_suffix("Badge").unwrap_or(key).replace('_', " ");
    let mut out = String::new();
    let mut prev_lower = false;
    for c in key.chars() {
        if prev_lower && c.is_ascii_uppercase() {
            out.push(' ');
        }
        prev_lower = c.is_ascii_lowercase();
        out.push(c);
    }
    out
}
fn display_system(n: u32) -> String {
    if n == 15 {
        "None / unset".into()
    } else if (n as usize) < SYSTEMS.len() && schema()["systems"].get(n as usize).is_some() {
        SYSTEMS[n as usize].into()
    } else {
        format!("Unknown ({n})")
    }
}
/// WPF core/team-staff.js decode(), same output shape for the unchanged renderer.
pub(crate) fn decode_staff(b: &[u8], slot: usize, index: usize) -> Result<Value> {
    ensure!(b.len() == STAFF_STRIDE, "Incomplete staff record.");
    ensure!(
        slot < 32 && index < 8192 && (b[0x182] & 31) as usize == slot,
        "Invalid staff identity."
    );
    let first = staff_name(b, 0x50)?;
    let last = staff_name(b, 0x78)?;
    ensure!(
        !first.is_empty() || !last.is_empty(),
        "Unnamed staff record."
    );
    let system = (u32le(b, 0x130) >> 20) & 15;
    let active = (u32le(b, 0x130) >> 24) & 15;
    let mut values = Map::new();
    for f in controls() {
        values.insert(
            f["key"].as_str().context("Invalid staff control")?.into(),
            control_value(b, f)?,
        );
    }
    let badges: Vec<Value> = schema()["badges"]
        .as_array()
        .context("Staff badges missing")?
        .iter()
        .filter(|f| f["key"] != "NONEBadge")
        .map(|f| {
            let key = f["key"].as_str().unwrap_or("");
            let bits = n(f, "bits").unwrap_or(0) as u32;
            json!({"key":key,"label":label(key),"raw":bits_value(b,n(f,"offset").unwrap_or(0) as usize,n(f,"shift").unwrap_or(0) as u32,bits),"bits":bits})
        })
        .collect();
    let full = [first.as_str(), last.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Ok(
        json!({"slot":slot,"recordIndex":index,"firstName":first,"lastName":last,"name":full,
        "role":schema()["roles"][slot],"values":values,"controls":controls(),"uniqueId":u16le(b,0x120),"faceId":u16le(b,0x12a)&0x7fff,
        "soundId":u16le(b,0x128) as i16,"playerUniqueId":u32le(b,0xa8),"wage":u32le(b,0x124),"growthPotential":b[0x131],
        "yearsInLeague":(u32le(b,0x134)>>7)&127,"yearsWithTeam":b[0x134]&127,"preferredSystem":display_system(system),"activeSystem":display_system(active),
        "attributes":SCALARS.iter().map(|(key,offset)|json!({"key":key,"label":label(key),"raw":b[*offset]})).collect::<Vec<_>>(),
        "proficiency":SYSTEMS.iter().enumerate().map(|(i,l)|json!({"key":schema()["systems"][i],"label":l,"raw":b[0x170+i]})).collect::<Vec<_>>(),
        "badges":badges}),
    )
}
fn control(key: &str) -> Result<&'static Value> {
    controls()
        .iter()
        .find(|f| f["key"] == key)
        .with_context(|| format!("Unsupported staff field: {key}"))
}
/// WPF validateValues().
pub(crate) fn validate_staff_values(values: &Value) -> Result<()> {
    let obj = values
        .as_object()
        .filter(|o| o.len() <= controls().len())
        .context("Invalid staff changes.")?;
    for (key, v) in obj {
        let f = control(key)?;
        if f["text"] == true {
            let s = v.as_str().context("Invalid staff name.")?;
            let units = s.encode_utf16().count();
            ensure!(
                (units as i64) < n(f, "bytes").unwrap_or(0) / 2
                    && !s.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
                "Invalid staff name."
            );
        } else {
            let bits = n(f, "bits").unwrap_or(0) as u32;
            let x = v
                .as_f64()
                .filter(|x| x.is_finite() && x.fract() == 0.)
                .with_context(|| format!("Invalid staff value: {key}"))? as i64;
            let min = n(f, "min").unwrap_or(0);
            let max = n(f, "max").unwrap_or((1i64 << bits) - 1);
            let choices = f["choices"].as_array().map(Vec::len);
            ensure!(
                x >= min
                    && x <= max
                    && choices
                        .is_none_or(|c| (x as usize) < c || (key.ends_with("SystemId") && x == 15)),
                "Invalid staff value: {key}"
            );
        }
    }
    Ok(())
}
/// WPF patch(): apply validated control values to a copy of the snapshot.
pub(crate) fn patch_staff(raw: &[u8], values: &Value) -> Result<Vec<u8>> {
    ensure!(raw.len() == STAFF_STRIDE, "Invalid staff snapshot.");
    validate_staff_values(values)?;
    let mut b = raw.to_vec();
    for (key, v) in values.as_object().unwrap() {
        let f = control(key)?;
        let offset = n(f, "offset").unwrap() as usize;
        if f["text"] == true {
            let bytes = n(f, "bytes").unwrap() as usize;
            b[offset..offset + bytes].fill(0);
            for (i, w) in v.as_str().unwrap().encode_utf16().enumerate() {
                b[offset + i * 2..offset + i * 2 + 2].copy_from_slice(&w.to_le_bytes());
            }
        } else {
            let x = v.as_f64().unwrap() as i64;
            let x = if x < 0 { x + 65536 } else { x } as u64;
            set_bits(
                &mut b,
                offset,
                n(f, "shift").unwrap_or(0) as u32,
                n(f, "bits").unwrap() as u32,
                x,
            );
        }
    }
    ensure!(
        !staff_name(&b, 0x50)?.is_empty() || !staff_name(&b, 0x78)?.is_empty(),
        "A staff member needs a name."
    );
    Ok(b)
}
/// WPF TeamStaffPatch.Validate(): masks, a name, and each edited range.
pub(crate) fn validate_staff_patch(before: &[u8], after: &[u8]) -> Result<()> {
    ensure!(
        before.len() == STAFF_STRIDE && after.len() == STAFF_STRIDE,
        "Invalid staff patch."
    );
    let p = policy();
    for i in 0..STAFF_STRIDE {
        ensure!(
            (before[i] ^ after[i]) & !p.masks[i] == 0,
            "Staff IDs, roles and links are not editable."
        );
    }
    ensure!(
        !staff_name(after, 0x50)?.is_empty() || !staff_name(after, 0x78)?.is_empty(),
        "A staff member needs a name."
    );
    for &(offset, shift, bits, min, max, signed, system) in &p.ranges {
        let value = |b: &[u8]| {
            let v = bits_value(b, offset, shift, bits) as i64;
            if signed && v >= 32768 { v - 65536 } else { v }
        };
        let (a, b) = (value(before), value(after));
        if a != b {
            ensure!(
                b >= min && b <= max && !(system && b > 7 && b != 15),
                "A staff value is outside its supported range."
            );
        }
    }
    Ok(())
}
/// Changed bytes as bounded contiguous patches, each masked by WPF's policy.
fn byte_patches(before: &[u8], after: &[u8], masks: &[u8], limit: usize) -> Vec<Patch> {
    let mut patches = Vec::new();
    let mut i = 0;
    while i < before.len() {
        if before[i] == after[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < before.len() && before[i] != after[i] && i - start < limit {
            i += 1;
        }
        patches.push(Patch {
            offset: start,
            before: before[start..i].to_vec(),
            after: after[start..i].to_vec(),
            mask: masks[start..i].to_vec(),
        });
    }
    patches
}
/// WPF TeamStaffService.save(): the tab's read snapshot must still match the
/// live record and every expected value; only owned (non-shared) records.
pub(crate) fn plan_staff(
    input: &Value,
    snapshot: &[u8],
    live: &[u8],
    index: usize,
    slot: usize,
    owned: bool,
) -> Result<Plan> {
    ensure!(
        snapshot == live,
        "The staff record changed. Refresh before saving."
    );
    ensure!(
        owned,
        "This staff member is shared with another team; save it from the team that owns it. No game write was attempted."
    );
    let current = decode_staff(snapshot, slot, index)?;
    let values = input["values"].clone();
    let expected = &input["expected"];
    ensure!(
        expected.is_object()
            && values
                .as_object()
                .is_some_and(|v| v.keys().all(|k| expected[k] == current["values"][k])),
        "The staff values changed. Refresh before saving."
    );
    let after = patch_staff(snapshot, &values)?;
    validate_staff_patch(snapshot, &after)?;
    let patches = byte_patches(snapshot, &after, staff_masks(), 40);
    let changed = values
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, v)| current["values"][k.as_str()] != **v)
        .map(|(k, _)| k.clone())
        .collect();
    let mut guards = vec![(0, 0x50), (0x120, 2), (0x182, 1)];
    guards.extend(patches.iter().map(|p| (p.offset, p.after.len())));
    Ok(Plan {
        kind: Kind::Staff,
        index,
        before: snapshot.to_vec(),
        after,
        patches,
        guards,
        changed,
    })
}

// ------------------------------------------------------------ rotations ----
/// WPF TeamRotationSnapshot.Members(): players whose +0x60 is this team.
/// Returns (UniqueId, record index, "First Last").
pub(crate) fn rotation_members(team: u64, players: &[u8]) -> Result<Vec<(u16, usize, String)>> {
    ensure!(
        team > 0 && players.len() % profile::PLAYER_STRIDE == 0,
        "Invalid rotation membership snapshot."
    );
    let mut rows = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for (i, r) in players.chunks_exact(profile::PLAYER_STRIDE).enumerate() {
        if u64::from_le_bytes(r[0x60..0x68].try_into()?) != team {
            continue;
        }
        let id = u16le(r, 0x128);
        ensure!(
            ids.insert(id),
            "Team player IDs are ambiguous. Rotation editing is unavailable."
        );
        let last = staff_name(r, 0).map_err(|_| anyhow::anyhow!("Unterminated player name."))?;
        let first = staff_name(r, 40).map_err(|_| anyhow::anyhow!("Unterminated player name."))?;
        ensure!(
            !first.is_empty() || !last.is_empty(),
            "An assigned player is unnamed."
        );
        rows.push((id, i, format!("{first} {last}").trim().to_owned()));
    }
    ensure!(
        (5..=64).contains(&rows.len()),
        "Load a team with at least five verified roster members."
    );
    Ok(rows)
}
pub(crate) fn member_digest(members: &[(u16, usize, String)]) -> String {
    let rows: Vec<Value> = members
        .iter()
        .map(|(id, index, name)| json!({"id":id,"index":index,"name":name}))
        .collect();
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&rows).unwrap_or_default())
    )
}
pub(crate) fn rotation_rows(raw: &[u8]) -> Result<Vec<Vec<u16>>> {
    ensure!(
        raw.len() == ROTATION_BYTES,
        "Invalid advanced rotation data."
    );
    Ok((0..48)
        .map(|i| (0..5).map(|j| u16le(raw, i * 10 + j * 2)).collect())
        .collect())
}
/// WPF validate()/pack(): edited segments need five distinct team members.
pub(crate) fn pack_rotations(
    before: &[u8],
    rows: &Value,
    members: &[(u16, usize, String)],
) -> Result<Vec<u8>> {
    let current = rotation_rows(before)?;
    let rows = rows
        .as_array()
        .filter(|r| r.len() == 48)
        .context("Rotations need 48 segments with five player IDs each.")?;
    let valid: std::collections::HashSet<u16> = members.iter().map(|m| m.0).collect();
    let mut after = vec![0u8; ROTATION_BYTES];
    for (i, row) in rows.iter().enumerate() {
        let ids: Vec<u16> = row
            .as_array()
            .filter(|r| r.len() == 5)
            .and_then(|r| {
                r.iter()
                    .map(|v| v.as_u64().filter(|v| *v <= 65535).map(|v| v as u16))
                    .collect()
            })
            .context("Rotations need 48 segments with five player IDs each.")?;
        if ids != current[i] {
            let unique: std::collections::HashSet<_> = ids.iter().collect();
            ensure!(
                unique.len() == 5 && ids.iter().all(|id| valid.contains(id)),
                "Each edited segment needs five different players from this team."
            );
        }
        for (j, id) in ids.iter().enumerate() {
            after[i * 10 + j * 2..i * 10 + j * 2 + 2].copy_from_slice(&id.to_le_bytes());
        }
    }
    Ok(after)
}
/// One guarded Team plan over the rotation block (2-byte cells, bounded spans).
pub(crate) fn plan_rotations(index: usize, team: &[u8], after_block: &[u8]) -> Result<Plan> {
    ensure!(
        team.len() == profile::TEAM_STRIDE && after_block.len() == ROTATION_BYTES,
        "Incomplete rotation buffers"
    );
    let mut after = team.to_vec();
    after[ROTATIONS..ROTATIONS + ROTATION_BYTES].copy_from_slice(after_block);
    let masks = crate::roster_v2_write::allowed(Kind::Team)?;
    let patches = byte_patches(team, &after, &masks, 72);
    let mut guards = vec![
        (crate::roster_v2_write::TEAM_ID, 2),
        (0x2FA, 48),
        (ROTATION_FLAG, 1),
    ];
    guards.extend(patches.iter().map(|p| (p.offset, p.after.len())));
    Ok(Plan {
        kind: Kind::Team,
        index,
        before: team.to_vec(),
        after,
        patches,
        guards,
        changed: vec!["rotations".into()],
    })
}

// -------------------------------------------------------------- seasons ----
/// WPF player-season-stats.js fields: (key, offset, shift, bits, signed).
pub(crate) const SEASON_FIELDS: [(&str, usize, u32, u32, bool); 20] = [
    ("ftm", 0x20, 0, 11, false),
    ("fta", 0x20, 11, 11, false),
    ("fgm", 0x18, 13, 13, false),
    ("fga", 0x14, 15, 14, false),
    ("threeMade", 0x24, 12, 12, false),
    ("threeAttempted", 0x24, 0, 12, false),
    ("oreb", 0x20, 22, 10, false),
    ("dreb", 0x28, 0, 12, false),
    ("steals", 0x1c, 0, 11, false),
    ("blocks", 0x1c, 11, 11, false),
    ("assists", 0x38, 0, 12, false),
    ("turnovers", 0x28, 12, 11, false),
    ("points", 0x2c, 0, 14, false),
    ("gp", 0x14, 0, 7, false),
    ("gs", 0x38, 12, 7, false),
    ("minutes", 0x18, 0, 13, false),
    ("fouls", 0x1c, 22, 10, false),
    ("doubleDoubles", 0x34, 25, 7, false),
    ("tripleDoubles", 0x14, 8, 7, false),
    ("plusMinus", 0x2c, 14, 15, true),
];
/// WPF player-season-awards.js: (key, offset, bit, width, max); allStar is split.
pub(crate) const AWARDS: [(&str, usize, u32, u32, u8); 11] = [
    ("mvp", 20, 7, 1, 1),
    ("champion", 43, 6, 1, 1),
    ("finalsMvp", 43, 7, 1, 1),
    ("roy", 27, 5, 1, 1),
    ("sixthMan", 27, 6, 1, 1),
    ("dpoy", 27, 7, 1, 1),
    ("mip", 47, 7, 1, 1),
    ("allStar", 39, 7, 2, 2),
    ("allNba", 23, 5, 2, 3),
    ("allDefense", 47, 5, 2, 2),
    ("allRookie", 27, 2, 2, 2),
];
fn season_value(r: &[u8], (_, offset, shift, bits, signed): (&str, usize, u32, u32, bool)) -> i64 {
    let v = ((u32le(r, offset) >> shift) & ((1u32 << bits) - 1)) as i64;
    if signed && v >= 1 << (bits - 1) {
        v - (1 << bits)
    } else {
        v
    }
}
pub(crate) fn decode_totals(r: &[u8]) -> Result<Value> {
    ensure!(r.len() == SEASON_STRIDE, "Invalid season record.");
    Ok(Value::Object(
        SEASON_FIELDS
            .iter()
            .map(|f| (f.0.to_string(), json!(season_value(r, *f))))
            .collect(),
    ))
}
pub(crate) fn decode_awards(r: &[u8]) -> Result<Value> {
    ensure!(r.len() == SEASON_STRIDE, "Invalid season record.");
    Ok(Value::Object(
        AWARDS
            .iter()
            .map(|&(key, offset, bit, width, _)| {
                let v = if key == "allStar" {
                    ((r[39] >> 7) & 1) | (((r[27] >> 4) & 1) << 1)
                } else {
                    (r[offset] >> bit) & ((1u8 << width) - 1)
                };
                (key.to_string(), json!(v))
            })
            .collect(),
    ))
}
pub(crate) fn patch_awards(r: &[u8], changes: &Map<String, Value>) -> Result<Vec<u8>> {
    ensure!(r.len() == SEASON_STRIDE, "Invalid season record.");
    let mut after = r.to_vec();
    for (key, value) in changes {
        let &(_, offset, bit, width, max) = AWARDS
            .iter()
            .find(|a| a.0 == key)
            .with_context(|| format!("Unsupported award or tier: {key}"))?;
        let v = value
            .as_u64()
            .filter(|v| *v <= max as u64)
            .with_context(|| format!("Unsupported award or tier: {key}"))? as u8;
        if key == "allStar" {
            after[39] = (after[39] & 127) | if v == 1 { 128 } else { 0 };
            after[27] = (after[27] & 239) | if v == 2 { 16 } else { 0 };
        } else {
            let mask = ((1u8 << width) - 1) << bit;
            after[offset] = (after[offset] & !mask) | (v << bit);
        }
    }
    Ok(after)
}
pub(crate) fn season_masks() -> Vec<u8> {
    let mut m = vec![0u8; SEASON_STRIDE];
    for &(_, offset, shift, bits, _) in &SEASON_FIELDS {
        for i in 0..bits as usize {
            let g = offset * 8 + shift as usize + i;
            m[g / 8] |= 1 << (g % 8);
        }
    }
    for &(_, offset, bit, width, _) in &AWARDS {
        m[offset] |= (((1u16 << width) - 1) << bit) as u8;
    }
    m[27] |= 16; // All-Star reserve half of the split field
    m
}
pub(crate) fn shared_warning(links: &[usize]) -> Result<Option<String>> {
    ensure!(
        !links.is_empty(),
        "Season ownership is unavailable. Refresh the roster."
    );
    let mut unique = links.to_vec();
    unique.sort();
    unique.dedup();
    Ok((unique.len() > 1).then(|| format!("This season history is shared by {} player entries in the currently loaded roster. Saving will update all {} linked entries. Other roster files will not be changed.",unique.len(),unique.len())))
}
fn range(f: &(&str, usize, u32, u32, bool)) -> (i64, i64) {
    if f.4 {
        (-(1i64 << (f.3 - 1)), (1i64 << (f.3 - 1)) - 1)
    } else {
        (0, (1i64 << f.3) - 1)
    }
}
/// WPF plan() (totals/averages through season_math) plus awards.patch().
/// Returns the planned record and warnings that the browser must confirm.
pub(crate) fn plan_season_record(record: &[u8], changes: &Value) -> Result<(Vec<u8>, Vec<String>)> {
    let changes = changes.as_object().context("Invalid season changes.")?;
    let (mut awards, mut stats) = (Map::new(), Map::new());
    for (k, v) in changes {
        match k.strip_prefix("award:") {
            Some(a) => awards.insert(a.into(), v.clone()),
            None => stats.insert(k.clone(), v.clone()),
        };
    }
    let before = decode_totals(record)?;
    let mut next = before.clone();
    for (key, value) in &stats {
        let x = value
            .as_f64()
            .filter(|x| x.is_finite())
            .with_context(|| format!("Invalid season value: {key}"))?;
        if let Some(f) = SEASON_FIELDS.iter().find(|f| f.0 == key) {
            let (min, max) = range(f);
            ensure!(
                x.fract() == 0. && x as i64 >= min && x as i64 <= max,
                "Season total outside the stored range: {key}"
            );
            next[key.as_str()] = json!(x as i64);
        }
    }
    for (key, value) in &stats {
        if SEASON_FIELDS.iter().any(|f| f.0 == key) {
            continue;
        }
        if let Some(total) = season_total_for(key) {
            ensure!(
                !stats.contains_key(total),
                "Edit the average or its total, not both: {key}"
            );
        }
        next = crate::season_math::change(&next, key, value.as_f64().unwrap())?;
    }
    for f in &SEASON_FIELDS {
        let (min, max) = range(f);
        let v = next[f.0].as_i64().unwrap_or(i64::MIN);
        ensure!(
            v >= min && v <= max,
            "Season total outside the stored range: {}",
            f.0
        );
    }
    let g = |k: &str| next[k].as_i64().unwrap();
    let changed: Vec<_> = SEASON_FIELDS
        .iter()
        .filter(|f| before[f.0] != next[f.0])
        .collect();
    let mut warnings = Vec::new();
    if !changed.is_empty() {
        ensure!(
            g("gs") <= g("gp"),
            "Games started cannot exceed games played."
        );
        ensure!(
            g("doubleDoubles") <= g("gp") && g("tripleDoubles") <= g("gp"),
            "Double/triple-doubles cannot exceed games played."
        );
        for (made, attempted) in [
            ("fgm", "fga"),
            ("threeMade", "threeAttempted"),
            ("ftm", "fta"),
        ] {
            ensure!(
                g(made) <= g(attempted),
                "Made shots cannot exceed attempts."
            );
        }
        ensure!(
            g("threeMade") <= g("fgm") && g("threeAttempted") <= g("fga"),
            "Three-pointers cannot exceed all field goals."
        );
        if g("points") != g("fgm") * 2 + g("threeMade") + g("ftm") {
            warnings.push("Points do not match the shooting totals. Shooting totals have not been changed automatically.".into());
        }
    }
    let mut after = record.to_vec();
    for f in changed {
        let (_, offset, shift, bits, _) = *f;
        let mask = (((1u64 << bits) - 1) << shift) as u32;
        let raw = (g(f.0) & ((1i64 << bits) - 1)) as u32;
        let word = (u32le(&after, offset) & !mask) | (raw << shift);
        after[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok((patch_awards(&after, &awards)?, warnings))
}
fn season_total_for(key: &str) -> Option<&'static str> {
    Some(match key {
        "ppg" => "points",
        "mpg" => "minutes",
        "orpg" => "oreb",
        "apg" => "assists",
        "spg" => "steals",
        "bpg" => "blocks",
        "topg" => "turnovers",
        "fpg" => "fouls",
        "plusMinusAverage" => "plusMinus",
        "fgPercent" => "fgm",
        "threePercent" => "threeMade",
        "ftPercent" => "ftm",
        _ => return None,
    })
}
/// SeasonStats.cs ValidateSeasonPatch().
pub(crate) fn validate_season_patch(before: &[u8], after: &[u8]) -> Result<()> {
    ensure!(
        before.len() == SEASON_STRIDE && after.len() == SEASON_STRIDE,
        "Invalid season snapshot."
    );
    let allowed = season_masks();
    for &(key, offset, bit, width, max) in &AWARDS {
        if key == "allStar" {
            continue;
        }
        let mask = (((1u16 << width) - 1) << bit) as u8;
        if before[offset] & mask != after[offset] & mask {
            ensure!(
                (after[offset] & mask) >> bit <= max,
                "Unsupported award tier."
            );
        }
    }
    if (((before[39] ^ after[39]) & 128) != 0 || ((before[27] ^ after[27]) & 16) != 0)
        && after[39] & 128 != 0
        && after[27] & 16 != 0
    {
        bail!("Choose All-Star starter or reserve, not both.");
    }
    for i in 0..SEASON_STRIDE {
        ensure!(
            (before[i] ^ after[i]) & !allowed[i] == 0,
            "Season edit touches an unsupported field."
        );
    }
    let v = |k: &str| season_value(after, *SEASON_FIELDS.iter().find(|f| f.0 == k).unwrap());
    ensure!(v("gs") <= v("gp"), "Games started exceeds games played.");
    ensure!(
        v("doubleDoubles") <= v("gp") && v("tripleDoubles") <= v("gp"),
        "Double/triple-doubles exceeds games played."
    );
    ensure!(
        v("ftm") <= v("fta")
            && v("fgm") <= v("fga")
            && v("threeMade") <= v("threeAttempted")
            && v("threeMade") <= v("fgm")
            && v("threeAttempted") <= v("fga"),
        "Invalid shooting totals."
    );
    Ok(())
}
pub(crate) fn plan_season(index: usize, before: &[u8], after: Vec<u8>) -> Result<Plan> {
    validate_season_patch(before, &after)?;
    let patches = byte_patches(before, &after, &season_masks(), SEASON_STRIDE);
    let mut guards = vec![(0, 0x14)];
    guards.extend(patches.iter().map(|p| (p.offset, p.after.len())));
    Ok(Plan {
        kind: Kind::Season,
        index,
        before: before.to_vec(),
        changed: SEASON_FIELDS
            .iter()
            .filter(|f| season_value(before, **f) != season_value(&after, **f))
            .map(|f| f.0.to_string())
            .collect(),
        after,
        patches,
        guards,
    })
}
/// Native PLAYERSTATDATA counter is bits13..20 of the final DWORD.
pub(crate) fn season_reference_count(row: &[u8]) -> Result<usize> {
    ensure!(
        row.len() == SEASON_STRIDE,
        "Incomplete season reference count"
    );
    Ok(((u32::from_le_bytes(row[60..64].try_into()?) >> 13) & 255) as usize)
}
pub(crate) fn season_reference_occurrences(players: &[u8], index: usize) -> Result<usize> {
    let stride = crate::roster_v2_write::Kind::Player.stride();
    ensure!(
        !players.is_empty() && players.len() % stride == 0 && index <= i16::MAX as usize,
        "Invalid season membership snapshot"
    );
    let mut count = 0;
    for row in players.chunks_exact(stride) {
        if row[PLAYER_PRIVATE_SEASONS..PLAYER_PRIVATE_SEASONS + 8] != [0; 8] {
            continue;
        }
        for slot in 0..33 {
            let at = PLAYER_SEASONS + slot * 2;
            if i16::from_le_bytes(row[at..at + 2].try_into()?) == index as i16 {
                count += 1;
            }
        }
    }
    Ok(count)
}
/// Read-only season row for the unchanged WPF renderer (player-season-service read()).
pub(crate) fn season_row(
    slot: usize,
    record: &[u8],
    team: Value,
    secondary: Value,
    token: &str,
    links: &[usize],
) -> Result<Value> {
    let totals = decode_totals(record)?;
    let mut averages = crate::season_math::display(&totals)?;
    if let Some(o) = averages.as_object_mut() {
        for f in &SEASON_FIELDS {
            o.remove(f.0);
        }
    }
    Ok(
        json!({"slot":slot,"year":1900 + record[0x11] as u32,"team":team,"secondaryTeam":secondary,"token":token,
        "totals":totals,"awards":decode_awards(record)?,"averages":averages,"sharedWarning":shared_warning(links)?}),
    )
}

#[cfg(test)]
#[path = "roster_v2_services_tests.rs"]
mod tests;
