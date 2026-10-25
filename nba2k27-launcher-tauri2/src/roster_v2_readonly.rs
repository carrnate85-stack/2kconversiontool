//! Version 2 roster snapshot model. No write/process/file API exists here;
//! roster_v2_write owns Save & verify and hands back verified read-back rows.
//! Field proof is independent serializer dataflow, not a copied old address gate.
#![allow(dead_code)]
use crate::{
    blacktop_native::Memory,
    roster_v2_profile as owner,
    roster_v2_write::{self as writer, Kind},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const START: u64 = 0x006a_e330;
// B673EB04 image: independently re-extracted 455/455 fields. Fingerprint ends
// before the trailing map loop reuses RDI, after all named player fields.
const LENGTH: usize = 54512;
const BODY_HASH: &str = "bea589403432ea7a3de52024502554fda1e3ca842c75e6e01abe8f667d56feb0";
const IMAGE_HASH: &str = "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B";
const TEAM_ANCHORS: &[(u64, &[u8])] = &[
    // PLAYERDATA::GetCachedDescription (0x2E20630): archetype key cached at
    // +0xE8, valid iff +0x418 bit 7 (Description_Internal in the field proof).
    (0x02e2_0643, &[0xf6, 0x81, 0x18, 0x04, 0, 0, 0x80]),
    (0x02e2_06ef, &[0x48, 0x8b, 0x87, 0xe8, 0, 0, 0]),
    // Independent getters and the paired ID/name use from Freestyle preparation.
    (0x02f8_6c40, &[0x0f, 0xb7, 0x81, 0x20, 0x0f, 0, 0, 0xc3]),
    (0x02f8_7cd0, &[0x48, 0x8d, 0x81, 0xfa, 0x02, 0, 0, 0xc3]),
    (
        0x02dd_a733,
        &[
            0x0f, 0xb7, 0x81, 0x20, 0x0f, 0, 0, 0x89, 0x44, 0x24, 0x5c, 0x48, 0x8d, 0x81, 0xfa,
            0x02, 0, 0,
        ],
    ),
    (
        0x027e_7a00,
        &[0x41, 0x8b, 0x86, 0x3c, 0x12, 0, 0, 0xc1, 0xe8, 0x1a],
    ),
    // Caller passes the active owner returned by 211A120 directly to the
    // uniform getter. Therefore +290/+298 are owner-, not allocation-relative.
    (
        0x01cd_d473,
        &[
            0xe8, 0xa8, 0xcc, 0x43, 0, 0x48, 0x89, 0xc1, 0x89, 0xea, 0xe8, 0x4e, 0xf7, 0x1f, 0x01,
        ],
    ),
];
pub(crate) const BIO: &[(&str, usize, u32, u32)] = &[
    ("GraphicId", 0x110, 0, 16),
    ("UniqueId", 0x128, 0, 16),
    ("BirthYear", 0x110, 16, 12),
    ("BirthMonth", 0x36a, 12, 4),
    ("BirthDay", 0x114, 1, 5),
    ("YearsPro", 0x3e0, 17, 5),
    ("Position", 0x12c, 8, 3),
    ("SecondaryPosition", 0x12c, 11, 3),
    ("JerseyNumber", 0x114, 11, 7),
];

#[derive(Clone, Debug)]
struct Field {
    key: String,
    offset: usize,
    shift: u32,
    bits: u32,
    name: u64,
    write: u64,
}
#[derive(Clone, Debug)]
struct Fields {
    packed: Vec<Field>,
    metadata: Vec<(String, u64)>,
}
fn n(value: &Value, key: &str) -> Result<u64> {
    value[key]
        .as_u64()
        .with_context(|| format!("Missing field proof {key}"))
}
fn s<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("Missing field proof {key}"))
}
fn shape(value: &Value) -> Result<(usize, u32, u32)> {
    let offset = usize::try_from(n(value, "offset")?)?;
    let shift = u32::try_from(n(value, "shift")?)?;
    let bits = u32::try_from(n(value, "bits")?)?;
    ensure!(
        offset <= owner::PLAYER_STRIDE - 8
            && (1..=32).contains(&bits)
            && shift.checked_add(bits).is_some_and(|v| v <= 64),
        "Invalid field proof bit range"
    );
    Ok((offset, shift, bits))
}
fn fields_from(proof: &Value, original: &Value) -> Result<Fields> {
    ensure!(
        s(proof, "schema")? == "nba2k27-v2-player-bitfield-proof-v1"
            && s(proof, "imageSha256")? == IMAGE_HASH
            && n(proof, "serializerRva")? == START
            && n(proof, "serializerBytes")? == LENGTH as u64
            && s(proof, "serializerSha256")? == BODY_HASH
            && proof["readOnly"] == true
            && proof["writesSupported"] == false
            && n(proof, "expectedFields")? == 455
            && n(proof, "matchedFields")? == 455
            && proof["missing"].as_array().is_some_and(Vec::is_empty)
            && proof["mismatches"].as_array().is_some_and(Vec::is_empty),
        "Incomplete Version 2 field proof"
    );
    let rows = proof["rows"]
        .as_array()
        .context("Missing player proof rows")?;
    let old = original["fields"]
        .as_array()
        .context("Missing original field policy")?;
    ensure!(
        rows.len() == 455 && old.len() == 455,
        "Incomplete player field policy"
    );
    let mut keys = HashSet::new();
    let mut packed = Vec::with_capacity(455);
    for (row, expected) in rows.iter().zip(old) {
        let key = s(row, "key")?;
        ensure!(
            key == s(expected, "key")? && keys.insert(key) && row["matches"] == true,
            "Duplicate, reordered or unproven player field"
        );
        let (offset, shift, bits) = shape(&row["extracted"])?;
        let (eo, es, eb) = shape(expected)?;
        ensure!(
            offset * 8 + shift as usize == eo * 8 + es as usize && bits == eb,
            "Player bitfield differs from independently extracted evidence"
        );
        let (name, write) = (n(row, "nameRva")?, n(row, "writeRva")?);
        ensure!(
            (START..START + LENGTH as u64).contains(&name)
                && (name..START + LENGTH as u64).contains(&write),
            "Player field proof lies outside fingerprinted body"
        );
        packed.push(Field {
            key: key.into(),
            offset,
            shift,
            bits,
            name,
            write,
        });
    }
    let metadata = proof["metadata"]
        .as_array()
        .context("Missing player metadata proof")?;
    let mut proven = Vec::new();
    for &(key, offset, shift, bits) in BIO {
        let found: Vec<_> = metadata.iter().filter(|r| r["key"] == key).collect();
        ensure!(
            found.len() == 1 && shape(&found[0]["shape"])? == (offset, shift, bits),
            "Player metadata mapping is not proven"
        );
        proven.push((key.into(), n(found[0], "nameRva")?));
    }
    for (key, offset) in [("UnlocalizedLastName", 0), ("UnlocalizedFirstName", 40)] {
        let found: Vec<_> = metadata.iter().filter(|r| r["key"] == key).collect();
        ensure!(
            found.len() == 1
                && found[0]["writer"] == "utf16"
                && n(found[0], "recordOffset")? == offset,
            "Player name buffer is not proven"
        );
        proven.push((key.into(), n(found[0], "nameRva")?));
    }
    ensure!(
        proven
            .iter()
            .all(|(_, at)| (START..START + LENGTH as u64).contains(at)),
        "Metadata name proof lies outside serializer"
    );
    Ok(Fields {
        packed,
        metadata: proven,
    })
}
fn fields() -> Result<Fields> {
    fields_from(
        &serde_json::from_str(include_str!("../assets/roster-v2-field-proof.json"))?,
        &serde_json::from_str(include_str!("../assets/roster-layout.json"))?,
    )
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    ensure!(
        at >= 0x10000
            && (1..=4096).contains(&len)
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end <= 0x7fff_ffff_ffff),
        "Unbounded read-only roster read"
    );
    m.checkpoint()?;
    let bytes = m.read(at, len)?;
    ensure!(bytes.len() == len, "Partial read-only roster read");
    Ok(bytes)
}
fn image(m: &impl Memory, rva: u64) -> Result<u64> {
    m.base().checked_add(rva).context("Image address overflow")
}

/// Pins all 455 independently named/dataflow-reconciled bit mappings. This
/// returns no writer capability and does not relax any legacy editor gate.
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    owner::verify_identity_profile(m)?;
    let f = fields()?;
    let mut hash = Sha256::new();
    for offset in (0..LENGTH).step_by(4096) {
        hash.update(read(
            m,
            image(m, START + offset as u64)?,
            (LENGTH - offset).min(4096),
        )?);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == BODY_HASH,
        "Version 2 player serializer body differs"
    );
    for (key, rva) in f
        .packed
        .iter()
        .map(|r| (&r.key, r.name))
        .chain(f.metadata.iter().map(|(k, r)| (k, *r)))
    {
        let bytes = read(m, image(m, rva)?, 7)?;
        ensure!(
            bytes[..3] == [0x48, 0x8d, 0x15],
            "Player property-name reference differs"
        );
        let target =
            i64::try_from(rva)? + 7 + i64::from(i32::from_le_bytes(bytes[3..].try_into()?));
        ensure!(
            (0x05cf_0000..0x0898_c000).contains(&target),
            "Player property name is outside read-only image data"
        );
        let mut expected = key.as_bytes().to_vec();
        expected.push(0);
        ensure!(
            read(m, image(m, target as u64)?, expected.len())? == expected,
            "Player serializer property name differs"
        );
    }
    for &(rva, expected) in TEAM_ANCHORS {
        ensure!(
            read(m, image(m, rva)?, expected.len())? == expected,
            "Team/owner read-only code anchor differs"
        );
    }
    Ok(())
}
fn uint(row: &[u8], offset: usize, shift: u32, bits: u32) -> Result<u64> {
    ensure!(
        (1..=32).contains(&bits) && shift.checked_add(bits).is_some_and(|v| v <= 64),
        "Invalid packed value width"
    );
    let len = ((shift + bits) as usize).div_ceil(8);
    let mut bytes = [0; 8];
    bytes[..len].copy_from_slice(
        row.get(offset..offset.checked_add(len).context("Player field overflow")?)
            .context("Truncated player field")?,
    );
    Ok((u64::from_le_bytes(bytes) >> shift) & ((1u64 << bits) - 1))
}
fn name(row: &[u8], offset: usize, length: usize) -> Result<String> {
    let bytes = row
        .get(offset..offset.checked_add(length).context("Name overflow")?)
        .context("Truncated roster name")?;
    ensure!(bytes.len() % 2 == 0, "Invalid name width");
    let words: Vec<_> = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .take_while(|v| *v != 0)
        .collect();
    let value = String::from_utf16(&words)?.trim().to_owned();
    ensure!(
        !value.chars().any(char::is_control),
        "Roster name contains control characters"
    );
    Ok(value)
}
fn decode_player(row: &[u8], index: usize) -> Result<Option<Value>> {
    ensure!(row.len() == owner::PLAYER_STRIDE, "Truncated player record");
    let last = name(row, 0, 40)?;
    let first = name(row, 40, 40)?;
    if last.is_empty() && first.is_empty() {
        return Ok(None);
    }
    let bio: Result<serde_json::Map<String, Value>> = BIO
        .iter()
        .map(|&(key, o, s, b)| Ok((key.into(), json!(uint(row, o, s, b)?))))
        .collect();
    let key = u64::from_le_bytes(row[0xe8..0xf0].try_into()?);
    let archetype =
        (row[0x418] & 0x80 != 0 && key != 0 && key != u64::MAX).then(|| format!("{key:016x}"));
    Ok(Some(
        json!({"index":index,"firstName":first,"lastName":last,"name":format!("{first} {last}").trim(),"metadata":bio?,"archetypeKey":archetype,"readOnly":true}),
    ))
}
fn decode_team(row: &[u8], index: usize) -> Result<Option<Value>> {
    ensure!(row.len() == owner::TEAM_STRIDE, "Truncated team record");
    let text = name(row, 0x2fa, 48)?;
    if text.is_empty() {
        return Ok(None);
    }
    // V2 consumes the complete dword>>26 (six bits), not the old decoder's
    // five-bit 123F slice. Expose the raw kind; do not invent enum labels.
    let mut value = json!({"index":index,"id":uint(row,0xf20,0,16)?,"name":text,"kind":uint(row,0x123c,26,6)?,"readOnly":true});
    // Buffers proven by the rebranding copy routine (roster-v2-write-proof).
    for (key, offset, len, _) in &writer::TEAM_TEXT[1..] {
        value[*key] = json!(name(row, *offset, *len)?);
    }
    Ok(Some(value))
}
fn uniform_layout() -> Result<&'static [Value]> {
    static ROWS: std::sync::OnceLock<Option<Vec<Value>>> = std::sync::OnceLock::new();
    ROWS.get_or_init(|| {
        let layout: Value =
            serde_json::from_str(include_str!("../assets/roster-layout.json")).ok()?;
        let rows = layout["uniforms"].as_array()?.clone();
        (rows.len() == 85).then_some(rows)
    })
    .as_deref()
    .context("Incomplete uniform display layout")
}
/// Read-only uniform card values. Every field is proven against the V2
/// uniform serializer by tools/roster_v2_write_audit.py. The custom-uniform
/// pointer at +0 is never dereferenced.
fn decode_uniform(row: &[u8], index: usize, slot: usize) -> Result<Value> {
    ensure!(
        row.len() == owner::UNIFORM_STRIDE,
        "Truncated uniform record"
    );
    let mut out = serde_json::Map::new();
    let mut socks = vec![Value::Null; 5];
    let word = |offset: usize| -> Result<u64> { uint(row, offset, 0, 32) };
    for f in uniform_layout()? {
        let offset = usize::try_from(n(f, "offset")?)?;
        let bytes = usize::try_from(n(f, "bytes")?)?;
        let value = match s(f, "kind")? {
            "ascii" => json!(crate::uniform_changes::ascii(row, offset, bytes)?),
            "signed" => json!(*row.get(offset).context("Truncated uniform field")? as i8),
            "bool" => json!((word(offset)? >> n(f, "shift")?) & n(f, "mask")? != 0),
            "masked" => json!((word(offset)? >> n(f, "shift")?) & n(f, "mask")?),
            "uint" => {
                let mut b = [0; 8];
                b[..bytes].copy_from_slice(
                    row.get(offset..offset + bytes)
                        .context("Truncated uniform field")?,
                );
                let v = u64::from_le_bytes(b);
                if bytes == 8 {
                    json!(v.to_string())
                } else {
                    json!(v)
                }
            }
            _ => anyhow::bail!("Unknown uniform display field"),
        };
        let property = s(f, "property")?;
        if let Some(slot) = property.strip_prefix("SockOption") {
            let slot = slot.parse::<usize>()?;
            ensure!((1..=5).contains(&slot), "Invalid sock slot");
            socks[slot - 1] = value;
        } else {
            out.insert(s(f, "key")?.into(), value);
        }
    }
    out.insert("sockOptionIds".into(), json!(socks));
    out.insert("hasCustomUniformData".into(), json!(false));
    out.insert("recordIndex".into(), json!(index));
    out.insert("recordAddress".into(), Value::Null);
    out.insert("slot".into(), json!(slot));
    Ok(Value::Object(out))
}
fn table(
    m: &impl Memory,
    state: &owner::OwnerSnapshot,
    directory: &owner::Directory,
) -> Result<Vec<u8>> {
    let length = directory
        .count
        .checked_mul(directory.stride)
        .context("Table length overflow")?;
    ensure!(
        (1..=16 * 1024 * 1024).contains(&length),
        "Oversized roster table"
    );
    let mut bytes = Vec::with_capacity(length);
    for offset in (0..length).step_by(4096) {
        if offset % 65536 == 0 {
            owner::revalidate(m, state)?;
        }
        bytes.extend(read(
            m,
            directory
                .pointer
                .checked_add(offset as u64)
                .context("Table address overflow")?,
            (length - offset).min(4096),
        )?);
    }
    Ok(bytes)
}
fn unchanged(
    m: &impl Memory,
    state: &owner::OwnerSnapshot,
    directory: &owner::Directory,
    bytes: &[u8],
) -> Result<()> {
    for (index, expected) in bytes.chunks(4096).enumerate() {
        if index % 16 == 0 {
            owner::revalidate(m, state)?;
        }
        ensure!(
            read(m, directory.pointer + index as u64 * 4096, expected.len())? == expected,
            "Roster rows changed during read-only capture; retry at a stable menu"
        );
    }
    Ok(())
}

/// Native-owned token. Public JSON contains no addresses. Lazy details avoid
/// materializing 455 fields for thousands of players at every directory refresh.
#[derive(Debug)]
pub(crate) struct Snapshot {
    owner: owner::OwnerSnapshot,
    document: Value,
    row_hashes: Vec<[u8; 32]>,
    // Retained immutable bytes from the double-checked capture. The unchanged
    // browser editor expects inline ratings; decode these bytes, never issue
    // hundreds of profile reads again for every player in the directory.
    records: Vec<u8>,
    teams: Vec<u8>,
    team_hashes: Vec<[u8; 32]>,
    uniforms: Vec<u8>,
    uniform_hashes: Vec<[u8; 32]>,
    name_audio: Option<crate::roster_v2_name_audio::Snapshot>,
    arenas: Option<std::sync::Arc<crate::roster_v2_arena::Snapshot>>,
    origins: Option<std::sync::Arc<crate::roster_v2_origins::Snapshot>>,
}
fn hashes(rows: &[u8], stride: usize) -> Vec<[u8; 32]> {
    rows.chunks_exact(stride)
        .map(|r| Sha256::digest(r).into())
        .collect()
}
impl Snapshot {
    pub(crate) fn document(&self) -> &Value {
        &self.document
    }
    pub(crate) fn owner(&self) -> &owner::OwnerSnapshot {
        &self.owner
    }
    pub(crate) fn arenas(&self) -> Option<&std::sync::Arc<crate::roster_v2_arena::Snapshot>> {
        self.arenas.as_ref()
    }
    pub(crate) fn origins(&self) -> Option<&std::sync::Arc<crate::roster_v2_origins::Snapshot>> {
        self.origins.as_ref()
    }
    pub(crate) fn cached_players(&self) -> Result<Vec<Value>> {
        let f = fields()?;
        self.document["players"]
            .as_array()
            .context("Captured players missing")?
            .iter()
            .map(|value| {
                let index = usize::try_from(n(value, "index")?)?;
                let row = self.row(Kind::Player, index)?;
                let mut result = value.clone();
                let raw: Result<Vec<_>> = f
                    .packed
                    .iter()
                    .map(|field| uint(row, field.offset, field.shift, field.bits))
                    .collect();
                result["ratingsRaw"] = json!(raw?);
                result["writeAvailable"] = json!(false);
                Ok(result)
            })
            .collect()
    }
    fn parts(&self, kind: Kind) -> Result<(&[u8], &[[u8; 32]])> {
        Ok(match kind {
            Kind::Player => (&self.records, &self.row_hashes),
            Kind::Team => (&self.teams, &self.team_hashes),
            Kind::Uniform => (&self.uniforms, &self.uniform_hashes),
            Kind::Staff | Kind::Season => {
                anyhow::bail!("Staff and season records are read on demand")
            }
        })
    }
    /// WPF unreferencedPopulatedUniforms: named uniform rows no team slot uses.
    pub(crate) fn unreferenced_uniforms(&self) -> Result<Vec<Value>> {
        let Some(d) = &self.owner.uniforms else {
            return Ok(Vec::new());
        };
        let mut referenced = vec![false; d.count];
        for team in self.teams.chunks_exact(owner::TEAM_STRIDE) {
            for slot in 0..writer::UNIFORM_SLOT_COUNT {
                let at = writer::UNIFORM_SLOTS + slot * 8;
                let pointer = u64::from_le_bytes(team[at..at + 8].try_into()?);
                if let Some(i) = writer::uniform_index(d.pointer, d.count, pointer) {
                    referenced[i] = true;
                }
            }
        }
        let mut out = Vec::new();
        for (i, row) in self
            .uniforms
            .chunks_exact(owner::UNIFORM_STRIDE)
            .enumerate()
        {
            let base = crate::uniform_changes::ascii(row, 8, 64)?;
            if referenced[i] || base.is_empty() {
                continue;
            }
            let v = decode_uniform(row, i, 0)?;
            out.push(json!({"recordIndex":i,"recordAddress":null,"uniformId":v["uniformId"],"teamId":v["teamId"],"baseFilename":base,
                "typeId":v["typeId"],"designationId":v["designationId"],"hasCustomUniformData":false}));
        }
        Ok(out)
    }
    /// One captured record, re-hashed so a corrupted cache is never planned.
    pub(crate) fn row(&self, kind: Kind, index: usize) -> Result<&[u8]> {
        let (rows, hashes) = self.parts(kind)?;
        let stride = kind.stride();
        let row = index
            .checked_mul(stride)
            .and_then(|start| rows.get(start..start + stride))
            .context("Record is outside the captured directory")?;
        ensure!(
            hashes
                .get(index)
                .is_some_and(|h| <[u8; 32]>::from(Sha256::digest(row)) == *h),
            "Cached roster record changed"
        );
        Ok(row)
    }
    /// Adopt a record that the writer read back and verified after Save.
    pub(crate) fn replace(&mut self, kind: Kind, index: usize, row: &[u8]) -> Result<()> {
        ensure!(row.len() == kind.stride(), "Incomplete verified read-back");
        self.row(kind, index)?;
        let start = index * kind.stride();
        let (rows, hashes) = match kind {
            Kind::Player => (&mut self.records, &mut self.row_hashes),
            Kind::Team => (&mut self.teams, &mut self.team_hashes),
            Kind::Uniform => (&mut self.uniforms, &mut self.uniform_hashes),
            Kind::Staff | Kind::Season => {
                anyhow::bail!("Staff and season records are read on demand")
            }
        };
        rows[start..start + row.len()].copy_from_slice(row);
        hashes[index] = Sha256::digest(row).into();
        self.document = build_document(
            &self.owner,
            &self.records,
            &self.teams,
            &self.uniforms,
            self.name_audio.as_ref(),
            self.origins.as_deref(),
        )?;
        Ok(())
    }
}
fn team_uniforms(
    team: &mut Value,
    row: &[u8],
    state: &owner::OwnerSnapshot,
    uniforms: &[u8],
) -> Result<()> {
    let id = uint(row, writer::TEAM_ID, 0, 16)?;
    let (mut slots, mut empty, mut invalid) = (Vec::new(), Vec::new(), 0);
    for slot in 0..writer::UNIFORM_SLOT_COUNT {
        let at = writer::UNIFORM_SLOTS + slot * 8;
        let pointer = u64::from_le_bytes(row[at..at + 8].try_into()?);
        if pointer == 0 {
            empty.push(slot);
            continue;
        }
        let index = state
            .uniforms
            .as_ref()
            .and_then(|d| writer::uniform_index(d.pointer, d.count, pointer));
        match index {
            Some(i) => {
                let u = &uniforms[i * owner::UNIFORM_STRIDE..(i + 1) * owner::UNIFORM_STRIDE];
                if uint(u, 0x52, 0, 16)? == id {
                    slots.push(decode_uniform(u, i, slot)?);
                } else {
                    invalid += 1;
                }
            }
            None => invalid += 1,
        }
    }
    team["uniformSlotCapacity"] = json!(writer::UNIFORM_SLOT_COUNT);
    team["ownedUniformSlotCount"] = json!(slots.len());
    team["invalidUniformSlotCount"] = json!(invalid);
    team["emptyUniformSlots"] = json!(empty);
    team["uniforms"] = json!(slots);
    Ok(())
}
fn project_origin(
    value: &mut Value,
    origins: Option<&crate::roster_v2_origins::Snapshot>,
    row: &[u8],
) -> Result<()> {
    let projected = match origins {
        Some(origins) => origins.project(row)?,
        None => {
            json!({"originChoicesAvailable":false,"nativeOriginIndex":null,"nativeOriginCatalogId":null,"originState":"unavailable","collegeName":null,"originSelectable":false})
        }
    };
    for (key, v) in projected.as_object().context("Origin projection missing")? {
        value[key] = v.clone();
    }
    Ok(())
}
fn build_document(
    state: &owner::OwnerSnapshot,
    players: &[u8],
    teams: &[u8],
    uniforms: &[u8],
    name_audio: Option<&crate::roster_v2_name_audio::Snapshot>,
    origins: Option<&crate::roster_v2_origins::Snapshot>,
) -> Result<Value> {
    let mut player_list = Vec::new();
    for (index, row) in players.chunks_exact(owner::PLAYER_STRIDE).enumerate() {
        if let Some(mut value) = decode_player(row, index)? {
            // player+0x60 = owning team record (live probe 3_player_team_pointer:
            // every named player is 0 or an exact team record start). Display only.
            let pointer = u64::from_le_bytes(row[0x60..0x68].try_into()?);
            let team = pointer
                .checked_sub(state.teams.pointer)
                .filter(|d| pointer != 0 && d % owner::TEAM_STRIDE as u64 == 0)
                .map(|d| (d / owner::TEAM_STRIDE as u64) as usize)
                .filter(|i| *i < state.teams.count);
            value["teamIndex"] = json!(team);
            value["weightLbs"] = json!(writer::weight_display(row));
            // FirstLastNameSID (proven i16 at +0x2E8): the full-name sound ID.
            value["fullNameId"] = json!(i16::from_le_bytes([row[0x2e8], row[0x2e9]]));
            value["audio"] = crate::roster_v2_name_audio::project(name_audio, row)?;
            project_origin(&mut value, origins, row)?;
            value["teamName"] = json!(match team {
                Some(i) => name(
                    &teams[i * owner::TEAM_STRIDE..(i + 1) * owner::TEAM_STRIDE],
                    0x2fa,
                    48
                )?,
                None => String::new(),
            });
            player_list.push(value);
        }
    }
    ensure!(
        player_list.len() >= 100,
        "Fewer than 100 valid named players; roster is not ready"
    );
    let mut team_list = Vec::new();
    for (index, row) in teams.chunks_exact(owner::TEAM_STRIDE).enumerate() {
        if let Some(mut value) = decode_team(row, index)? {
            team_uniforms(&mut value, row, state, uniforms)?;
            team_list.push(value);
        }
    }
    ensure!(!team_list.is_empty(), "No named teams; roster is not ready");
    let f = fields()?;
    Ok(
        json!({"schema":"nba2k27-v2-readonly-roster-v1","source":"Version 2 verified snapshot","readOnly":true,"writeAvailable":false,
        "players":player_list,"teams":team_list,"ratingKeys":f.packed.iter().map(|p|&p.key).collect::<Vec<_>>(),
        "directoryCounts":{"players":state.players.count,"teams":state.teams.count,"arenas":state.arenas.count,"uniforms":state.uniforms.as_ref().map(|u|u.count)},
        "unsupported":["height and wingspan writes","From clearing / new origin allocation","custom arena / floor cloning","structural season allocation / removal / shared-row copying","online ratings page capture"]}),
    )
}
fn capture_verified(m: &impl Memory, region: owner::OwnedRegion) -> Result<Snapshot> {
    let state = owner::inspect_owner(m, region)?;
    let players = table(m, &state, &state.players)?;
    let teams = table(m, &state, &state.teams)?;
    let uniforms = match &state.uniforms {
        Some(d) => table(m, &state, d)?,
        None => Vec::new(),
    };
    m.checkpoint()?;
    let name_audio = crate::roster_v2_name_audio::capture(m, &state)?;
    let arenas = crate::roster_v2_arena::capture(m, &state)?.map(std::sync::Arc::new);
    let origins = crate::roster_v2_origins::capture(m, &state)?.map(std::sync::Arc::new);
    let document = build_document(
        &state,
        &players,
        &teams,
        &uniforms,
        name_audio.as_ref(),
        origins.as_deref(),
    )?;
    owner::revalidate(m, &state)?;
    unchanged(m, &state, &state.players, &players)?;
    unchanged(m, &state, &state.teams, &teams)?;
    if let Some(d) = &state.uniforms {
        unchanged(m, &state, d, &uniforms)?;
    }
    owner::revalidate(m, &state)?;
    if let Some(audio) = &name_audio {
        audio.revalidate(m, &state)?;
    }
    if let Some(origins) = &origins {
        origins.revalidate(m, &state)?;
    }
    if let Some(arenas) = &arenas {
        arenas.revalidate(m, &state)?;
    }
    owner::revalidate(m, &state)?;
    Ok(Snapshot {
        row_hashes: hashes(&players, owner::PLAYER_STRIDE),
        team_hashes: hashes(&teams, owner::TEAM_STRIDE),
        uniform_hashes: hashes(&uniforms, owner::UNIFORM_STRIDE),
        owner: state,
        document,
        records: players,
        teams,
        uniforms,
        name_audio,
        arenas,
        origins,
    })
}
/// Region must be obtained by the retained native reader, never browser input.
pub(crate) fn capture(m: &impl Memory, region: owner::OwnedRegion) -> Result<Snapshot> {
    verify_profile(m)?;
    capture_verified(m, region)
}
fn details_verified(m: &impl Memory, snapshot: &Snapshot, index: usize) -> Result<Value> {
    ensure!(
        index < snapshot.owner.players.count,
        "Player index is outside captured roster"
    );
    owner::revalidate(m, &snapshot.owner)?;
    let at = snapshot
        .owner
        .players
        .pointer
        .checked_add((index * owner::PLAYER_STRIDE) as u64)
        .context("Player address overflow")?;
    let row = read(m, at, owner::PLAYER_STRIDE)?;
    ensure!(
        <[u8; 32]>::from(Sha256::digest(&row)) == snapshot.row_hashes[index],
        "Player changed since directory capture; refresh the roster"
    );
    let mut value = decode_player(&row, index)?.context("Unnamed player is not selectable")?;
    let f = fields()?;
    let raw: Result<Vec<_>> = f
        .packed
        .iter()
        .map(|field| uint(&row, field.offset, field.shift, field.bits))
        .collect();
    value["ratingsRaw"] = json!(raw?);
    value["fullNameId"] = json!(i16::from_le_bytes([row[0x2e8], row[0x2e9]]));
    value["audio"] = crate::roster_v2_name_audio::project(snapshot.name_audio.as_ref(), &row)?;
    project_origin(&mut value, snapshot.origins.as_deref(), &row)?;
    value["writeAvailable"] = json!(false);
    ensure!(
        read(m, at, owner::PLAYER_STRIDE)? == row,
        "Player changed while decoding details"
    );
    owner::revalidate(m, &snapshot.owner)?;
    Ok(value)
}
pub(crate) fn player_details(m: &impl Memory, snapshot: &Snapshot, index: usize) -> Result<Value> {
    verify_profile(m)?;
    details_verified(m, snapshot, index)
}

#[cfg(test)]
#[path = "roster_v2_readonly_tests.rs"]
mod tests;
