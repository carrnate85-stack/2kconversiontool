//! Native port of the WPF roster reader. Pure bounded snapshot decoding: no
//! process discovery, writes, hooks, file mutations or frontend-supplied pointers.
use crate::blacktop_native::Memory;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
};

pub(crate) const PLAYER_STRIDE: usize = 0x4F8;
/// Native-only record ownership, captured from the same stable table bytes as
/// the displayed player. Never reconstructed from browser-supplied addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerIdentity {
    pub index: usize,
    pub id: u16,
    pub appearance: u64,
}
#[derive(Clone, Debug)]
pub(crate) struct TeamIdentity {
    pub id: u16,
    pub arena: u64,
    pub uniform_slots: Vec<u64>,
}
#[derive(Clone, Debug)]
pub(crate) struct UniformIdentity {
    pub id: u16,
    pub team_id: u16,
    pub filename: String,
    pub hash: String,
}
#[derive(Clone, Debug)]
pub(crate) struct TeamDirectory {
    pub directory: crate::team_changes::Directory,
    pub teams: Vec<TeamIdentity>,
    pub arena_hashes: Vec<String>,
    pub uniform_header: Vec<u8>,
    pub uniforms: Vec<UniformIdentity>,
}
const TEAM_STRIDE: usize = 0x1550;
const ARENA_STRIDE: usize = 0x1278;
const UNIFORM_STRIDE: usize = 0x170;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Layout {
    schema: String,
    source_sha256: String,
    serializer_sha256: String,
    serializer_rva: u64,
    serializer_bytes: usize,
    fields: Vec<RatingField>,
    uniforms: Vec<UniformField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RatingField {
    pub key: String,
    pub offset: usize,
    pub shift: u32,
    pub bits: u32,
    pub maximum: i64,
}
pub(crate) fn rating_fields() -> &'static [RatingField] {
    &layout().fields
}
pub(crate) fn serializer_fingerprint() -> &'static str {
    &layout().serializer_sha256
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UniformField {
    property: String,
    key: Option<String>,
    kind: String,
    offset: usize,
    bytes: usize,
    #[serde(default)]
    shift: u32,
    #[serde(default)]
    mask: u32,
}
fn layout() -> &'static Layout {
    static L: OnceLock<Layout> = OnceLock::new();
    L.get_or_init(|| {
        let l: Layout = serde_json::from_str(include_str!("../assets/roster-layout.json"))
            .expect("Verified original roster layout");
        assert_eq!(l.schema, "nba2k27-native-roster-layout-v1");
        assert_eq!(l.source_sha256.len(), 64);
        assert_eq!(l.fields.len(), 455);
        assert_eq!(l.uniforms.len(), 85);
        let mut keys = HashSet::new();
        for f in &l.fields {
            assert!(
                keys.insert(&f.key)
                    && f.offset + 4 <= PLAYER_STRIDE
                    && f.bits > 0
                    && f.bits <= 32
                    && f.shift + f.bits <= 32
            );
        }
        for f in &l.uniforms {
            assert!(f.offset + f.bytes <= UNIFORM_STRIDE && f.bytes > 0);
        }
        l
    })
}
fn at(bytes: &[u8], start: usize, len: usize) -> Result<&[u8]> {
    bytes
        .get(start..start.checked_add(len).context("Record offset overflow")?)
        .context("Incomplete roster record")
}
fn uint(bytes: &[u8], start: usize, len: usize) -> Result<u64> {
    ensure!((1..=8).contains(&len), "Invalid integer width");
    let mut b = [0; 8];
    b[..len].copy_from_slice(at(bytes, start, len)?);
    Ok(u64::from_le_bytes(b))
}
fn bits(bytes: &[u8], start: usize, shift: u32, width: u32) -> Result<u64> {
    ensure!(
        width > 0 && width <= 32 && shift + width <= 64,
        "Invalid packed field"
    );
    Ok(
        (uint(bytes, start, ((shift + width) as usize).div_ceil(8))? >> shift)
            & ((1u64 << width) - 1),
    )
}
fn utf16(bytes: &[u8], start: usize, len: usize) -> Result<String> {
    ensure!(len.is_multiple_of(2), "Invalid UTF-16 width");
    let words: Vec<u16> = at(bytes, start, len)?
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .take_while(|v| *v != 0)
        .collect();
    Ok(String::from_utf16_lossy(&words).trim().to_owned())
}
fn ascii(bytes: &[u8], start: usize, len: usize) -> Result<String> {
    Ok(at(bytes, start, len)?
        .iter()
        .take_while(|v| **v != 0)
        .map(|b| if b.is_ascii() { *b as char } else { '?' })
        .collect::<String>()
        .trim()
        .to_owned())
}
fn valid_name(s: &str) -> bool {
    !s.trim().is_empty()
        && s.encode_utf16().count() <= 40
        && !s.contains('\u{fffd}')
        && !s.chars().any(char::is_control)
}
fn hex(value: u64) -> String {
    format!("0x{value:X}")
}
fn optional_hex(value: u64) -> String {
    if value == 0 {
        String::new()
    } else {
        hex(value)
    }
}
pub(crate) fn checked_pointer(value: u64) -> Result<u64> {
    ensure!(
        (0x10000..=0x7fff_ffff_fff8).contains(&value) && value.is_multiple_of(8),
        "Invalid aligned roster object pointer"
    );
    Ok(value)
}
fn read(m: &impl Memory, address: u64, len: usize) -> Result<Vec<u8>> {
    m.checkpoint()?;
    ensure!(
        address >= 0x10000
            && len > 0
            && len <= 16 * 1024 * 1024
            && address
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_ffff_ffff),
        "Invalid roster read span"
    );
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        let n = (len - bytes.len()).min(4096);
        let chunk = m.read(address + bytes.len() as u64, n)?;
        ensure!(chunk.len() == n, "Partial roster read");
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Allocation {
    pub base: u64,
    pub len: u64,
}
impl Allocation {
    pub(crate) fn valid(&self) -> bool {
        self.base >= 0x10000
            && (0x800000..=0x8000000).contains(&self.len)
            && self
                .base
                .checked_add(self.len)
                .is_some_and(|v| v <= 0x7fff_ffff_ffff)
    }
    fn object(&self, pointer: u64, len: usize) -> bool {
        self.valid()
            && len > 0
            && pointer >= self.base
            && pointer.is_multiple_of(8)
            && pointer
                .checked_sub(self.base)
                .and_then(|off| off.checked_add(len as u64))
                .is_some_and(|end| end <= self.len)
    }
    fn table(&self, pointer: u64, count: u64, stride: usize, maximum: u64) -> Result<()> {
        ensure!(
            (1..=maximum).contains(&count)
                && pointer >= self.base + 0x680
                && count.checked_mul(stride as u64).is_some_and(
                    |len| len <= usize::MAX as u64 && self.object(pointer, len as usize)
                ),
            "Roster table exceeds its owned allocation"
        );
        Ok(())
    }
}
struct Table {
    header_address: u64,
    header: Vec<u8>,
    pointer: u64,
    count: usize,
    stride: usize,
    bytes: Vec<u8>,
}
impl Table {
    fn optional(
        m: &impl Memory,
        a: Allocation,
        offset: u64,
        stride: usize,
        max: u64,
    ) -> Result<Self> {
        let header = read(m, a.base + offset, 16)?;
        let count = uint(&header, 0, 8)?;
        let pointer = uint(&header, 8, 8)?;
        if a.table(pointer, count, stride, max).is_err() {
            return Ok(Self {
                header_address: a.base + offset,
                header,
                pointer: 0,
                count: 0,
                stride,
                bytes: vec![],
            });
        }
        Ok(Self {
            header_address: a.base + offset,
            header,
            pointer,
            count: count as usize,
            stride,
            bytes: read(m, pointer, count as usize * stride)?,
        })
    }
    fn load(m: &impl Memory, a: Allocation, offset: u64, stride: usize, max: u64) -> Result<Self> {
        let header = read(m, a.base + offset, 16)?;
        let count = uint(&header, 0, 8)?;
        let pointer = uint(&header, 8, 8)?;
        a.table(pointer, count, stride, max)?;
        Ok(Self {
            header_address: a.base + offset,
            header,
            pointer,
            count: count as usize,
            stride,
            bytes: read(m, pointer, count as usize * stride)?,
        })
    }
    fn address(&self, index: usize) -> u64 {
        self.pointer + (index * self.stride) as u64
    }
    fn row(&self, index: usize) -> &[u8] {
        &self.bytes[index * self.stride..(index + 1) * self.stride]
    }
    fn index(&self, pointer: u64) -> Option<usize> {
        let offset = pointer.checked_sub(self.pointer)?;
        if !offset.is_multiple_of(self.stride as u64) {
            return None;
        }
        let index = usize::try_from(offset / self.stride as u64).ok()?;
        (index < self.count).then_some(index)
    }
    fn stable(&self, m: &impl Memory) -> Result<()> {
        ensure!(
            read(m, self.header_address, 16)? == self.header
                && (self.bytes.is_empty()
                    || read(m, self.pointer, self.bytes.len())? == self.bytes),
            "Roster changed during read; refresh before editing"
        );
        Ok(())
    }
}

/// Constructed only after the exact serializer and table-owner chain validate.
/// Synthetic tests build it directly; no renderer/RPC can supply verification.
pub(crate) struct VerifiedLayout {
    image: u64,
    root: u64,
    owner: u64,
    archetype: bool,
}
impl VerifiedLayout {
    pub(crate) fn verify(m: &impl Memory, player_table: u64) -> Result<Self> {
        crate::blacktop_native::validate_build(m)?;
        let l = layout();
        ensure!(
            format!(
                "{:x}",
                Sha256::digest(read(m, m.base() + l.serializer_rva, l.serializer_bytes)?)
            ) == l.serializer_sha256,
            "Player serializer differs; no roster data published"
        );
        let root = checked_pointer(uint(&read(m, m.base() + 0x9A47EB8, 8)?, 0, 8)?)?;
        let owner = checked_pointer(uint(&read(m, root + 0xA8, 8)?, 0, 8)?)?;
        ensure!(
            uint(&read(m, owner + 0x18, 8)?, 0, 8)? == player_table,
            "Roster owner differs"
        );
        let archetype = read(m, m.base() + 0x2DEB753, 7)? == [0xF6, 0x81, 0x18, 0x04, 0, 0, 0x80]
            && read(m, m.base() + 0x2DEB7FF, 7)? == [0x48, 0x8B, 0x87, 0xE8, 0, 0, 0];
        let result = Self {
            image: m.base(),
            root,
            owner,
            archetype,
        };
        result.stable(m, player_table)?;
        Ok(result)
    }
    fn stable(&self, m: &impl Memory, table: u64) -> Result<()> {
        ensure!(
            uint(&read(m, self.image + 0x9A47EB8, 8)?, 0, 8)? == self.root
                && uint(&read(m, self.root + 0xA8, 8)?, 0, 8)? == self.owner
                && uint(&read(m, self.owner + 0x18, 8)?, 0, 8)? == table,
            "Roster database changed owner"
        );
        Ok(())
    }
}
fn uniform(
    row: &[u8],
    index: usize,
    address: u64,
    a: Allocation,
    m: &impl Memory,
) -> Result<Value> {
    let mut out = Map::new();
    out.insert("recordIndex".into(), json!(index));
    out.insert("recordAddress".into(), json!(hex(address)));
    let mut socks = vec![Value::Null; 5];
    for f in &layout().uniforms {
        let value = match f.kind.as_str() {
            "ascii" => json!(ascii(row, f.offset, f.bytes)?),
            "signed" => json!(row[f.offset] as i8),
            "bool" => json!((uint(row, f.offset, 4)? >> f.shift) & f.mask as u64 != 0),
            "masked" => json!((uint(row, f.offset, 4)? >> f.shift) & f.mask as u64),
            "uint" => {
                let v = uint(row, f.offset, f.bytes)?;
                if f.bytes == 8 {
                    json!(v.to_string())
                } else {
                    json!(v)
                }
            }
            _ => anyhow::bail!("Unknown compiled uniform field"),
        };
        if f.property.starts_with("SockOption") {
            let slot = f.property[10..].parse::<usize>()?;
            ensure!((1..=5).contains(&slot), "Invalid sock slot");
            socks[slot - 1] = value;
        } else {
            out.insert(f.key.clone().context("Missing compiled field key")?, value);
        }
    }
    out.insert("sockOptionIds".into(), json!(socks));
    let custom = uint(row, 0, 8)?;
    let has = a.object(custom, 0x250);
    out.insert("hasCustomUniformData".into(), json!(has));
    if has {
        let b = read(m, custom, 0x250)?;
        let flags = uint(&b, 0x4C, 4)?;
        for (key, value) in [
            ("customUniformAddress", json!(hex(custom))),
            ("customUniformUniqueId", json!(uint(&b, 0, 2)?)),
            ("customUniformSignature", json!(uint(&b, 4, 4)?)),
            ("customUniformName", json!(ascii(&b, 8, 64)?)),
            ("customUniformFontNameToken", json!(uint(&b, 0x48, 4)?)),
            ("customUniformNumberOfLayers", json!(flags & 63)),
            ("customUniformNumberOfOptions", json!((flags >> 6) & 15)),
            ("customUniformInUse", json!((flags >> 10) & 1 != 0)),
            ("customUniformPadding", json!(flags >> 11)),
            ("bakedJerseyFilename", json!(ascii(&b, 0x50, 256)?)),
            ("bakedShortsFilename", json!(ascii(&b, 0x150, 256)?)),
        ] {
            out.insert(key.into(), value);
        }
        ensure!(
            read(m, custom, 0x250)? == b,
            "Custom uniform changed during read"
        );
    }
    Ok(Value::Object(out))
}
fn structurally_blank(uniform: &Value) -> bool {
    uniform.as_object().is_some_and(|o| {
        o.iter().all(|(k, v)| {
            if matches!(k.as_str(), "recordIndex" | "recordAddress" | "slot") {
                return true;
            }
            match v {
                Value::Number(n) => n.as_i64() == Some(0),
                Value::String(s) => {
                    s.is_empty()
                        || matches!(k.as_str(), "localizedNameToken" | "localizedTeamNameToken")
                            && s == "0"
                }
                Value::Bool(b) => !*b,
                Value::Array(a) => a.iter().all(|v| v.as_u64() == Some(0)),
                _ => false,
            }
        })
    })
}
fn position(id: u64) -> &'static str {
    ["PG", "SG", "SF", "PF", "C", "N/A"]
        .get(id as usize)
        .copied()
        .unwrap_or("Unknown")
}
fn decimal(v: f32, min: f32, max: f32, places: i32) -> Value {
    if !v.is_finite() || v < min || v > max {
        Value::Null
    } else {
        let scale = 10f64.powi(places);
        json!((v as f64 * scale).round() / scale)
    }
}
fn float(row: &[u8], offset: usize) -> Result<f32> {
    Ok(f32::from_le_bytes(at(row, offset, 4)?.try_into()?))
}
fn birth_date(year: u64, month: u64, day: u64) -> Value {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && day > 0
        && day <= days[month as usize - 1]
    {
        json!(format!("{year:04}-{month:02}-{day:02}"))
    } else {
        Value::Null
    }
}
fn player(
    row: &[u8],
    index: usize,
    address: u64,
    teams: &Table,
    team_values: &[Value],
    m: &impl Memory,
    verified: Option<&VerifiedLayout>,
) -> Result<Option<Value>> {
    let last = utf16(row, 0, 40)?;
    let first = utf16(row, 40, 40)?;
    if !valid_name(&first) && !valid_name(&last) {
        return Ok(None);
    }
    let team = uint(row, 0x60, 8)?;
    let team_index = teams.index(team);
    let appearance = uint(row, 0x78, 8)?;
    let measure = if appearance != 0 {
        read(m, appearance, 4)
            .ok()
            .filter(|b| read(m, appearance, 4).ok().as_ref() == Some(b))
    } else {
        None
    };
    let height = measure
        .as_ref()
        .and_then(|b| uint(b, 0, 2).ok())
        .filter(|v| (12000..=26000).contains(v));
    let wingspan = measure
        .as_ref()
        .and_then(|b| uint(b, 2, 2).ok())
        .filter(|v| (12000..=32000).contains(v));
    let pos = bits(row, 0x12D, 0, 3)?;
    let secondary = bits(row, 0x12D, 3, 3)?;
    let jersey = bits(row, 0x115, 3, 7)?;
    let potential = bits(row, 0x45D, 0, 8)?;
    let mut clothing = Map::new();
    for (key, offset, shift, width) in [
        ("shoeHome", 0x170, 0, 13),
        ("shoeAway", 0x171, 5, 13),
        ("sockHome", 0x160, 4, 4),
        ("sockAway", 0x161, 0, 3),
        ("undershirt", 0x2F7, 6, 2),
        ("compressionShorts", 0x193, 6, 3),
        ("headband", 0x1B6, 6, 3),
        ("leftArm", 0x180, 6, 5),
        ("rightArm", 0x182, 1, 5),
        ("leftElbow", 0x183, 4, 3),
        ("rightElbow", 0x184, 6, 3),
        ("leftWrist", 0x185, 5, 4),
        ("rightWrist", 0x276, 0, 4),
        ("leftFingers", 0x34A, 7, 4),
        ("rightFingers", 0x34D, 5, 4),
        ("leftLeg", 0x18D, 4, 4),
        ("rightLeg", 0x18E, 6, 4),
        ("leftKnee", 0x2F8, 7, 4),
        ("rightKnee", 0x1C8, 0, 4),
        ("leftAnkle", 0x191, 4, 3),
        ("rightAnkle", 0x192, 5, 3),
    ] {
        clothing.insert(key.into(), json!(bits(row, offset, shift, width)?));
    }
    let overall = decimal(float(row, 0x1AC)?, 20., 120., 4);
    let mut out = json!({"recordIndex":index,"recordAddress":hex(address),"firstName":first,"lastName":last,"name":format!("{first} {last}").trim(),
        "cyberfaceId":uint(row,0x110,2)?,"teamAddress":optional_hex(team),"teamIndex":team_index,"teamName":team_index.map(|i|team_values[i]["name"].as_str().unwrap_or("")).unwrap_or(""),
        "positionId":pos,"position":position(pos),"secondaryPositionId":secondary,"secondaryPosition":position(secondary),
        "jerseyNumberRaw":jersey,"jerseyNumber":if jersey==100{"00".into()}else{jersey.to_string()},"weightLbs":decimal(float(row,0x108)?,70.,500.,2),
        "heightRaw":height,"heightCm":height.map(|v|v as f64/100.),"wingspanRaw":wingspan,"wingspanCm":wingspan.map(|v|v as f64/100.),"potentialRaw":potential,"potential":110u64.min(25+(potential+1)/3),
        "audio":null,"ratingsRaw":null,"yearsPro":null,"bioLayoutVerified":false,"birthDate":null,"birthYear":null,"birthMonth":null,"birthDay":null,"fromIndex":null,"collegeName":null,"archetypeKey":null,
        "cachedOverallRaw":overall,"overall":if overall.is_null(){Value::Null}else{json!((float(row,0x1AC)? as f64).round() as u64)},"overallStatus":"live-cache-candidate","clothing":clothing});
    if let Some(v) = verified {
        let values: Result<Vec<_>> = layout()
            .fields
            .iter()
            .map(|f| bits(row, f.offset, f.shift, f.bits))
            .collect();
        out["ratingsRaw"] = json!(values?);
        let year = bits(row, 0x112, 0, 12)?;
        let month = bits(row, 0x36B, 4, 4)?;
        let day = bits(row, 0x114, 1, 5)?;
        out["yearsPro"] = json!(bits(row, 0x3E2, 1, 5)?);
        out["bioLayoutVerified"] = json!(true);
        out["birthYear"] = json!(year);
        out["birthMonth"] = json!(month);
        out["birthDay"] = json!(day);
        out["birthDate"] = birth_date(year, month, day);
        let key = uint(row, 0xE8, 8)?;
        if v.archetype && row[0x418] & 0x80 != 0 && key != 0 && key != u64::MAX {
            out["archetypeKey"] = json!(format!("{key:016x}"));
        }
    }
    Ok(Some(out))
}

#[cfg(test)]
pub(crate) fn decode(
    m: &impl Memory,
    a: Allocation,
    verified: Option<&VerifiedLayout>,
) -> Result<Value> {
    Ok(decode_owned(m, a, verified)?.0)
}
pub(crate) fn decode_owned(
    m: &impl Memory,
    a: Allocation,
    verified: Option<&VerifiedLayout>,
) -> Result<(Value, Vec<PlayerIdentity>, TeamDirectory)> {
    ensure!(a.valid(), "Invalid roster allocation");
    let players = Table::load(m, a, 0x60, PLAYER_STRIDE, 8192)?;
    ensure!(
        players.pointer == a.base + 0x680 && (100..=8192).contains(&players.count),
        "Player table is not the validated roster directory"
    );
    if let Some(v) = verified {
        v.stable(m, players.pointer)?;
    }
    let arenas = Table::load(m, a, 0xB0, ARENA_STRIDE, 2048)?;
    let teams = Table::optional(m, a, 0xD0, TEAM_STRIDE, 1024)?;
    let uniforms = Table::optional(m, a, 0x290, UNIFORM_STRIDE, 8192)?;
    // Tables may be in any order, but must never alias one another or headers.
    let mut spans: Vec<_> = [&players, &arenas, &teams, &uniforms]
        .into_iter()
        .filter(|t| t.count > 0)
        .collect();
    spans.sort_by_key(|t| t.pointer);
    for w in spans.windows(2) {
        ensure!(
            w[0].pointer + w[0].bytes.len() as u64 <= w[1].pointer,
            "Overlapping roster directories"
        );
    }
    let mut arena_values = Vec::new();
    for i in 0..arenas.count {
        m.checkpoint()?;
        let row = arenas.row(i);
        let name = utf16(row, 0, 66)?;
        let file = ascii(row, 0x7E, 38)?;
        let floor = ascii(row, 0xA4, 40)?;
        arena_values.push(json!({"recordIndex":i,"recordAddress":hex(arenas.address(i)),"arenaId":uint(row,0x792,2)?,"name":name,"arenaFile":file,"floorFile":floor,"emptyCandidate":name.is_empty()&&file.is_empty()&&floor.is_empty()}));
    }
    let mut uniform_values = BTreeMap::new();
    for i in 0..uniforms.count {
        m.checkpoint()?;
        uniform_values.insert(
            uniforms.address(i),
            uniform(uniforms.row(i), i, uniforms.address(i), a, m)?,
        );
    }
    let roster_types = [
        "Normal",
        "Historic",
        "User",
        "Online",
        "All Stars East",
        "All Stars West",
        "Street Home",
        "Street Away",
        "Rookies",
        "Sophomores",
        "Rookie Showcase",
        "MyTeam",
        "Euroleague",
        "Domination",
        "College",
        "2K Sports",
        "Hoops Summit",
        "G-League",
        "College",
        "Unknown",
        "Team USA",
        "Dream Team",
        "National Team",
        "Expansion Slot",
        "Expansion Premade",
        "E-League",
        "All-Time",
        "Unknown",
        "WNBA",
        "Decade",
    ];
    let mut team_values = Vec::new();
    let mut owned = HashSet::new();
    let mut referenced = HashSet::new();
    for i in 0..teams.count {
        m.checkpoint()?;
        let row = teams.row(i);
        let name = utf16(row, 0x2FA, 48)?;
        let name = if valid_name(&name) {
            name
        } else {
            String::new()
        };
        let id = uint(row, 0xF20, 2)?;
        let roster = bits(row, 0x123F, 2, 5)?;
        let historic = bits(row, 0x154A, 3, 7)?;
        let arena = arenas.index(uint(row, 0xEE0, 8)?).map(|n| &arena_values[n]);
        let mut slots = Vec::new();
        let mut empty = Vec::new();
        let mut invalid = 0;
        for slot in 0..32 {
            let pointer = uint(row, 0xDE0 + slot * 8, 8)?;
            if pointer == 0 {
                empty.push(slot);
                continue;
            }
            if let Some(v) = uniform_values.get(&pointer) {
                referenced.insert(pointer);
                if v["teamId"].as_u64() == Some(id) {
                    let mut v = v.clone();
                    v["slot"] = json!(slot);
                    slots.push(v);
                    owned.insert(pointer);
                    continue;
                }
            }
            invalid += 1;
        }
        team_values.push(json!({"recordIndex":i,"recordAddress":hex(teams.address(i)),"name":name,"cityName":utf16(row,0x32C,36)?,"cityShort":utf16(row,0x350,14)?,"stateShort":utf16(row,0x35E,14)?,"mainLogo":utf16(row,0x1AE,72)?,
            "teamId":id,"rosterTypeId":roster,"rosterType":roster_types.get(roster as usize).copied().unwrap_or("Unknown"),"historicYearId":historic,"historicYear":if historic==0{String::new()}else{format!("'{:02}-'{:02}",(historic-1)%100,historic%100)},
            "arenaId":arena.map(|v|&v["arenaId"]),"arenaName":arena.map(|v|&v["name"]).cloned().unwrap_or(json!("")),"arenaFile":arena.map(|v|&v["arenaFile"]).cloned().unwrap_or(json!("")),
            "uniformSlotCapacity":32,"ownedUniformSlotCount":slots.len(),"invalidUniformSlotCount":invalid,"emptyUniformSlots":empty,"uniforms":slots}));
    }
    let mut player_values = Vec::new();
    let mut identities = Vec::new();
    for i in 0..players.count {
        m.checkpoint()?;
        identities.push(PlayerIdentity {
            index: i,
            id: uint(players.row(i), 0x128, 2)? as u16,
            appearance: uint(players.row(i), 0x78, 8)?,
        });
        if let Some(v) = player(
            players.row(i),
            i,
            players.address(i),
            &teams,
            &team_values,
            m,
            verified,
        )? {
            player_values.push(v);
        }
    }
    for table in [&players, &teams, &arenas, &uniforms] {
        table.stable(m)?;
    }
    if let Some(v) = verified {
        v.stable(m, players.pointer)?;
    }
    let unowned: Vec<_> = uniform_values
        .iter()
        .filter(|(p, _)| !owned.contains(p))
        .collect();
    let unreferenced: Vec<_> = uniform_values
        .iter()
        .filter(|(p, _)| !referenced.contains(p))
        .collect();
    let loose: Vec<_> = unreferenced
        .iter()
        .filter(|(_, v)| v["baseFilename"].as_str() != Some(""))
        .map(|(_, v)| {
            let mut o = Map::new();
            for key in [
                "recordIndex",
                "recordAddress",
                "uniformId",
                "teamId",
                "baseFilename",
                "typeId",
                "designationId",
                "hasCustomUniformData",
            ] {
                o.insert(key.into(), v[key].clone());
            }
            Value::Object(o)
        })
        .collect();
    let linked = player_values
        .iter()
        .filter(|v| !v["teamIndex"].is_null())
        .count();
    let nonzero = player_values
        .iter()
        .filter(|v| v["cyberfaceId"].as_u64() != Some(0))
        .count();
    let named = team_values
        .iter()
        .filter(|v| v["name"].as_str() != Some(""))
        .count();
    let arena_linked = team_values
        .iter()
        .filter(|v| !v["arenaId"].is_null())
        .count();
    let mut found = HashSet::new();
    let mut matched = HashSet::new();
    let mut records_found = 0;
    let mut records_matched = 0;
    for v in &player_values {
        for (name, id) in [
            ("Joel Embiid", 4090),
            ("Tyrese Maxey", 8011),
            ("V.J. Edgecombe", 4120),
            ("Cooper Flagg", 4117),
        ] {
            if v["name"].as_str() == Some(name) {
                found.insert(name);
                records_found += 1;
                if v["cyberfaceId"].as_u64() == Some(id) {
                    matched.insert(name);
                    records_matched += 1;
                }
            }
        }
    }
    let team_count = team_values.len();
    team_values.retain(|v| v["name"].as_str() != Some(""));
    Ok((
        json!({"schema":"nba2k27-live-player-research-v1","ok":true,"access":"read-only","writes":0,
        "regionBase":hex(a.base),"regionSize":a.len,"declaredPlayerCount":players.count,"decodedPlayerCount":player_values.len(),"playerTable":hex(players.pointer),"playerStride":"0x4F8",
        "teamTable":optional_hex(teams.pointer),"teamCount":team_count,"namedTeamCount":named,"arenaTable":hex(arenas.pointer),"arenaCount":arena_values.len(),"arenaLinkedTeamCount":arena_linked,
        "uniformRecordCount":uniform_values.len(),"ownedUniformRecordCount":owned.len(),"referencedUniformRecordCount":referenced.len(),"unreferencedUniformRecordCount":unreferenced.len(),"unownedUniformRecordCount":unowned.len(),
        "unownedUniformBlankFilenameCount":unowned.iter().filter(|(_,v)|v["baseFilename"].as_str()==Some("")).count(),"unownedUniformPopulatedFilenameCount":unowned.iter().filter(|(_,v)|v["baseFilename"].as_str()!=Some("")).count(),
        "unownedUniformStructurallyBlankCount":unowned.iter().filter(|(_,v)|structurally_blank(v)).count(),"unreferencedUniformStructurallyBlankCount":unreferenced.iter().filter(|(_,v)|structurally_blank(v)).count(),
        "customUniformDataRecordCount":uniform_values.values().filter(|v|v["hasCustomUniformData"]==true).count(),"unreferencedPopulatedUniforms":loose,
        "verifiedTeamLinks":linked,"nonzeroCyberfaceIds":nonzero,"cyberfaceAnchorValidation":{"expectedNames":4,"namesFound":found.len(),"namesMatched":matched.len(),"recordsFound":records_found,"recordsMatched":records_matched,"passed":found.len()==4&&matched.len()==4&&records_found==records_matched},
        "players":player_values,"teams":team_values,"arenas":arena_values,"origins":[]}),
        identities,
        TeamDirectory {
            directory: crate::team_changes::Directory {
                team_table: teams.pointer,
                team_count: teams.count,
                arena_table: arenas.pointer,
                arena_count: arenas.count,
            },
            teams: (0..teams.count)
                .map(|i| {
                    Ok(TeamIdentity {
                        id: uint(teams.row(i), 0xF20, 2)? as u16,
                        arena: uint(teams.row(i), 0xEE0, 8)?,
                        uniform_slots: (0..32)
                            .map(|slot| uint(teams.row(i), 0xDE0 + slot * 8, 8))
                            .collect::<Result<_>>()?,
                    })
                })
                .collect::<Result<_>>()?,
            arena_hashes: (0..arenas.count)
                .map(|i| crate::team_changes::hash(arenas.row(i)))
                .collect(),
            uniform_header: uniforms.header.clone(),
            uniforms: (0..uniforms.count)
                .map(|i| {
                    Ok(UniformIdentity {
                        id: uint(uniforms.row(i), 0x50, 2)? as u16,
                        team_id: uint(uniforms.row(i), 0x52, 2)? as u16,
                        filename: ascii(uniforms.row(i), 8, 64)?,
                        hash: crate::team_changes::hash(uniforms.row(i)),
                    })
                })
                .collect::<Result<_>>()?,
        },
    ))
}

/// Project the exact verified read-back into the existing WPF response shape.
/// Optional services not verified by this port (audio/origins/seasons) stay
/// unchanged; no optimistic values are copied from the browser request.
pub(crate) fn project_player(cached: &Value, row: &[u8], appearance: &[u8]) -> Result<Value> {
    ensure!(
        row.len() == PLAYER_STRIDE && appearance.len() == 4,
        "Incomplete committed player read-back"
    );
    let mut out = cached.clone();
    let first = utf16(row, 40, 40)?;
    let last = utf16(row, 0, 40)?;
    out["firstName"] = json!(first);
    out["lastName"] = json!(last);
    out["name"] = json!(format!("{first} {last}").trim());
    out["cyberfaceId"] = json!(uint(row, 0x110, 2)?);
    let pos = bits(row, 0x12D, 0, 3)?;
    let secondary = bits(row, 0x12D, 3, 3)?;
    out["positionId"] = json!(pos);
    out["position"] = json!(position(pos));
    out["secondaryPositionId"] = json!(secondary);
    out["secondaryPosition"] = json!(position(secondary));
    let jersey = bits(row, 0x115, 3, 7)?;
    out["jerseyNumberRaw"] = json!(jersey);
    out["jerseyNumber"] = json!(if jersey == 100 {
        "00".into()
    } else {
        jersey.to_string()
    });
    out["weightLbs"] = decimal(float(row, 0x108)?, 70., 500., 2);
    let height = uint(appearance, 0, 2)?;
    let wings = uint(appearance, 2, 2)?;
    out["heightRaw"] = json!(height);
    out["heightCm"] = json!(height as f64 / 100.);
    out["wingspanRaw"] = json!(wings);
    out["wingspanCm"] = json!(wings as f64 / 100.);
    let potential = bits(row, 0x45D, 0, 8)?;
    out["potentialRaw"] = json!(potential);
    out["potential"] = json!(110u64.min(25 + (potential + 1) / 3));
    let ratings: Result<Vec<_>> = rating_fields()
        .iter()
        .map(|f| bits(row, f.offset, f.shift, f.bits))
        .collect();
    out["ratingsRaw"] = json!(ratings?);
    let year = bits(row, 0x112, 0, 12)?;
    let month = bits(row, 0x36B, 4, 4)?;
    let day = bits(row, 0x114, 1, 5)?;
    out["birthYear"] = json!(year);
    out["birthMonth"] = json!(month);
    out["birthDay"] = json!(day);
    out["birthDate"] = birth_date(year, month, day);
    out["yearsPro"] = json!(bits(row, 0x3E2, 1, 5)?);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct Fixture {
        base: u64,
        bytes: Vec<u8>,
        reads: Cell<usize>,
        mutate: Option<u64>,
    }
    impl Memory for Fixture {
        fn base(&self) -> u64 {
            0x140000000
        }
        fn read(&self, address: u64, len: usize) -> Result<Vec<u8>> {
            self.reads.set(self.reads.get() + 1);
            ensure!(len <= 4096, "Unbounded native read");
            let offset = usize::try_from(
                address
                    .checked_sub(self.base)
                    .context("Foreign allocation read")?,
            )?;
            let mut b = at(&self.bytes, offset, len)?.to_vec();
            if self.mutate == Some(address) && self.reads.get() > 20 {
                b[0] ^= 1;
            }
            Ok(b)
        }
    }
    fn fixtures() -> Vec<Value> {
        serde_json::from_str(include_str!("../assets/roster-read-regressions.json")).unwrap()
    }
    fn memory(case: &Value) -> Fixture {
        let hex = case["hex"].as_str().unwrap();
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        Fixture {
            base: case["regionBase"].as_str().unwrap().parse().unwrap(),
            bytes,
            reads: Cell::new(0),
            mutate: None,
        }
    }
    fn numeric_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| numeric_equal(a, b))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, a)| b.get(k).is_some_and(|b| numeric_equal(a, b)))
            }
            _ => a == b,
        }
    }
    #[test]
    fn complete_record_snapshots_match_original_wpf_reader() {
        for case in fixtures() {
            let m = memory(&case);
            let a = Allocation {
                base: m.base,
                len: case["regionSize"].as_u64().unwrap(),
            };
            let mut v = VerifiedLayout {
                image: 0,
                root: 0,
                owner: 0,
                archetype: true,
            };
            // Synthetic verification is explicit and confined to this test.
            // Separate owner-stability tests exercise the real read chain.
            let table = m.base + 0x680;
            let mut data = m.bytes.clone();
            // Use a tiny synthetic image/root/owner tail within the same owned allocation.
            let image = m.base + 0x3A000 - 0x9A47EB8;
            let root = m.base + 0x3B000;
            let owner = m.base + 0x3C000;
            data[0x3A000..0x3A008].copy_from_slice(&root.to_le_bytes());
            data[0x3B0A8..0x3B0B0].copy_from_slice(&owner.to_le_bytes());
            data[0x3C018..0x3C020].copy_from_slice(&table.to_le_bytes());
            v.image = image;
            v.root = root;
            v.owner = owner;
            let m = Fixture { bytes: data, ..m };
            let (result, identities, team_directory) = decode_owned(&m, a, Some(&v)).unwrap();
            assert_eq!(
                team_directory.teams.len(),
                result["teamCount"].as_u64().unwrap() as usize
            );
            assert_eq!(
                team_directory.arena_hashes.len(),
                result["arenaCount"].as_u64().unwrap() as usize
            );
            assert_eq!(
                team_directory.uniforms.len(),
                result["uniformRecordCount"].as_u64().unwrap() as usize
            );
            let uniform_table = uint(&team_directory.uniform_header, 8, 8).unwrap();
            for (i, row) in team_directory.uniforms.iter().enumerate() {
                let raw = m
                    .read(uniform_table + (i * UNIFORM_STRIDE) as u64, UNIFORM_STRIDE)
                    .unwrap();
                assert_eq!(row.id, uint(&raw, 0x50, 2).unwrap() as u16);
                assert_eq!(row.team_id, uint(&raw, 0x52, 2).unwrap() as u16);
                assert_eq!(row.filename, ascii(&raw, 8, 64).unwrap());
                assert_eq!(row.hash, crate::team_changes::hash(&raw));
            }
            for (i, row) in team_directory.teams.iter().enumerate() {
                let raw = m
                    .read(
                        team_directory.directory.team_table + (i * TEAM_STRIDE + 0xDE0) as u64,
                        256,
                    )
                    .unwrap();
                assert_eq!(row.uniform_slots.len(), 32);
                for (slot, address) in row.uniform_slots.iter().enumerate() {
                    assert_eq!(*address, uint(&raw, slot * 8, 8).unwrap());
                }
            }
            assert_eq!(
                identities.len(),
                result["declaredPlayerCount"].as_u64().unwrap() as usize
            );
            for row in &identities {
                let at = 0x680 + row.index * PLAYER_STRIDE;
                assert_eq!(row.id, uint(&m.bytes, at + 0x128, 2).unwrap() as u16);
                assert_eq!(row.appearance, uint(&m.bytes, at + 0x78, 8).unwrap());
            }
            assert!(result.get("nativeOwnership").is_none());
            for (key, expected) in case["expected"].as_object().unwrap() {
                if let Some(actual) = result.get(key) {
                    assert!(
                        numeric_equal(actual, expected),
                        "Original reader mismatch in {key}: first actual={}",
                        actual.to_string().chars().take(300).collect::<String>()
                    );
                }
            }
            for key in [
                "players",
                "teams",
                "arenas",
                "unreferencedPopulatedUniforms",
            ] {
                assert_eq!(
                    result[key].as_array().unwrap().len(),
                    case["expected"][key].as_array().unwrap().len()
                );
            }
            for (actual, expected) in result["players"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["expected"]["players"].as_array().unwrap())
            {
                for (key, value) in expected.as_object().unwrap() {
                    assert!(
                        numeric_equal(&actual[key], value),
                        "Player {} field {key}: {} != {}",
                        actual["recordIndex"],
                        actual[key],
                        value
                    );
                }
            }
        }
    }
    #[test]
    fn bad_tables_optional_sentinels_and_mutation_are_fail_closed() {
        let case = fixtures().remove(0);
        let m = memory(&case);
        let a = Allocation {
            base: m.base,
            len: 0x800000,
        };
        assert!(decode(&m, a, None).is_ok());
        for (pointer, count) in [
            (u64::MAX, 100),
            (m.base + 0x680, 8193),
            (m.base + 0x681, 100),
            (m.base + 0x800000 - 8, 100),
            (m.base, 100),
            (m.base + 0x680, 0),
        ] {
            let mut bytes = m.bytes.clone();
            bytes[0x60..0x68].copy_from_slice(&(count as u64).to_le_bytes());
            bytes[0x68..0x70].copy_from_slice(&pointer.to_le_bytes());
            assert!(
                decode(
                    &Fixture {
                        bytes,
                        reads: Cell::new(0),
                        ..memory(&case)
                    },
                    a,
                    None
                )
                .is_err()
            );
        }
        assert!(!a.object(u64::MAX, 0x250));
        assert!(!a.object(a.base + a.len - 8, 0x250));
        assert!(!a.object(a.base + 3, 0x250));
        let changed = Fixture {
            mutate: Some(a.base + 0x60),
            ..memory(&case)
        };
        assert!(decode(&changed, a, None).is_err());
        let result = decode(&m, a, None).unwrap();
        assert!(result["players"][0]["ratingsRaw"].is_null());
        assert_eq!(result["players"][0]["bioLayoutVerified"], false);
    }
    #[test]
    fn allocation_ranges_and_record_bounds_never_wrap() {
        for pointer in [0, 1, 0x10003, u64::MAX, 0x8000_0000_0000] {
            assert!(checked_pointer(pointer).is_err());
        }
        assert_eq!(checked_pointer(0x10000).unwrap(), 0x10000);
        for a in [
            Allocation {
                base: 0,
                len: 0x800000,
            },
            Allocation {
                base: u64::MAX - 7,
                len: 0x800000,
            },
            Allocation {
                base: 0x10000,
                len: 0x8000001,
            },
        ] {
            assert!(!a.valid());
        }
        assert!(uint(&[0; 3], 0, 4).is_err());
        assert!(utf16(&[0; 4], 0, 3).is_err());
        assert!(bits(&[0; 8], 0, 0, 0).is_err());
        assert_eq!(birth_date(2000, 2, 29), "2000-02-29");
        assert!(birth_date(1900, 2, 29).is_null());
    }
    #[test]
    fn absent_optional_team_uniform_directories_preserve_player_reading() {
        let case = fixtures().remove(0);
        let mut m = memory(&case);
        let a = Allocation {
            base: m.base,
            len: 0x800000,
        };
        for offset in [0xD0, 0x290] {
            m.bytes[offset..offset + 8].copy_from_slice(&0u64.to_le_bytes());
            m.bytes[offset + 8..offset + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        }
        let result = decode(&m, a, None).unwrap();
        assert_eq!(result["decodedPlayerCount"], 100);
        assert_eq!(result["teams"], json!([]));
        assert_eq!(result["uniformRecordCount"], 0);
        assert!(
            result["players"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["teamIndex"].is_null())
        );
    }
    #[test]
    fn cancellation_is_checked_during_cpu_decoding_not_only_process_reads() {
        struct Cancelled {
            inner: Fixture,
            last_reads: Cell<usize>,
            streak: Cell<usize>,
        }
        impl Memory for Cancelled {
            fn base(&self) -> u64 {
                self.inner.base()
            }
            fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
                self.inner.read(at, len)
            }
            fn checkpoint(&self) -> Result<()> {
                let reads = self.inner.reads.get();
                let streak = if self.last_reads.get() == reads {
                    self.streak.get() + 1
                } else {
                    1
                };
                self.last_reads.set(reads);
                self.streak.set(streak);
                ensure!(
                    streak < 2,
                    "Synthetic cancellation between CPU-only records"
                );
                Ok(())
            }
        }
        let case = fixtures().remove(0);
        let m = Cancelled {
            inner: memory(&case),
            last_reads: Cell::new(usize::MAX),
            streak: Cell::new(0),
        };
        let error = decode(
            &m,
            Allocation {
                base: m.inner.base,
                len: 0x800000,
            },
            None,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Synthetic cancellation between CPU-only records"
        );
        assert_eq!(m.streak.get(), 2);
        assert_eq!(m.inner.reads.get(), m.last_reads.get());
    }
}
