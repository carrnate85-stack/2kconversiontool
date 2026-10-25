//! Version 2 browser roster Save & verify policy: pure plans, write-profile
//! verification, durable intent and guarded rollback. No process discovery,
//! handles or browser addresses; roster_v2_access supplies one bounded record
//! lease per save. Only layouts proven from the exact Version 2 image are
//! writable (assets/roster-v2-field-proof.json for player bits and
//! assets/roster-v2-write-proof.json for team text, uniform controls and
//! uniform slots). Everything else is refused before any intent or write.
use crate::{
    backend,
    blacktop_native::Memory,
    player_changes, roster_snapshot, roster_v2_profile as profile, roster_v2_readonly as model,
    session_safety::{self, Identity},
    team_changes, uniform_changes,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};

const PROOF_SCHEMA: &str = "nba2k27-v2-roster-write-proof-v1";
const INTENT_SCHEMA: &str = "nba2k27-roster-v2-write-intent-v1";
const IMAGE_HASH: &str = "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B";
/// FirstLastNameSID: signed scalar; -1 disables the override. Other positive
/// values are IDs, not proof that a corresponding name clip exists in a bank.
pub(crate) const FULL_NAME_SID: usize = 0x2E8;
const AUDIO_ANCHORS: &[(u64, &str)] = &[
    (
        0x6b2eab,
        "0fbfafe8020000c74424208d000000488d159af120064c89e14531c041b91c463ccbe84e9d53044885c0740a4889c189eae80fa45304",
    ),
    (
        0x6968f5,
        "4883ec204889f9bac8581e094531c0e8b77e55044883c420668986e8020000",
    ),
];
const AUDIO_METHODS: &[(u64, usize, &str)] = &[
    (
        0x4bed2f0,
        75,
        "3a13c19b60bdaba3ca33478c459d821935b49044af0ba67a5c0c1bab06899ca4",
    ),
    (
        0x4bee7c0,
        349,
        "3dfdf2dbf8051874963baeb6ae5b64859aacdeafa78607862238f1cb6982b1c7",
    ),
];
// Exact QA082 evidence tag. Accepted ONLY for stored journal inspection, never
// as live write authority. Its Player mask excludes the new audio field.
const QA082_JOURNAL_PROOF: &str =
    "74ebb737c3b60e164f3157b6ae2531974c7cc612369d1e7a02b8e79d59517061";
pub(crate) const TEAM_TYPE_BITS: Bits = (0x123C, 26, 6);
pub(crate) const TEAM_CLASSIC_YEAR_BITS: Bits = (0x1548, 19, 8);
const TEAM_METADATA_SPANS: &[(u64, usize, &str)] = &[
    (
        0x1ad4360,
        2469,
        "7df0339fb007e534fd2bef8b4cf13bfa41ba02833f7e68008adcfee79ee4d18f",
    ),
    (
        0x22ff0,
        67,
        "90a674f8860ed699ad406d8164f671838364e347b66029986fa65537ddf5c1a7",
    ),
    (
        0x2f8b120,
        746,
        "8dee9d04e6ca45940bbba5d3d7299db6d950019393d31440229df5c33f1d9e1b",
    ),
    (
        0x3071750,
        329,
        "8c3d03dc8c7ecaa7557e8ab544a691087520e1ed6a0c25f29d5b20970bef2a18",
    ),
    (
        0x22e60,
        33,
        "bf7905349d00d15c297a19fd32e931eb58783b5a25136aa6e6719c451957d8e5",
    ),
    (
        0x3a554e0,
        24931,
        "abe5b75ab41118dbe13310c3bfe98ec762c138af538600bf6af9ed212e9629f4",
    ),
    (
        0x89be940,
        32,
        "374d6e34bb06cafa8d7ac03fd6b1179f7be2cb2e7a1e0c18c23e4f3a271325b4",
    ),
    (
        0x89bfae8,
        32,
        "9740a637900b6920be7967067f487b26f028c9f3362426439b9582854caf17e3",
    ),
    (
        0x89bd140,
        432,
        "784e26f832f64cbe27f9e5ca1b5272aafd74c58c0caacfcfd92268cf8716175f",
    ),
    (
        0x2f86bf0,
        10,
        "db3ae5223afca4f522f91cff30ca71d02504fe91de43a8ba8d7a645d805fcb14",
    ),
];
// Native descriptor traversal/store and signed-32 type selection. These prove
// the only runtime descriptor transition normalized by team_metadata_bytes.
const TEAM_METADATA_INITIALIZER: &[(u64, usize, &str)] = &[
    (0x4b8612b, 85, "7869d490f347c2b0a8392ce4016f0d000680b7b4b654dec47a4f5bb835554b50"),
    (0x4b86226, 49, "f69dd88cca579605b4628bea99dc6f7c21dd00b011e61434a9ea518048b24e2b"),
];
const UNIFORM_SERIALIZER: u64 = 0x0068_7590;
const UNIFORM_RECORD_BYTES: usize = 6411;
const RDATA: std::ops::Range<i64> = 0x05cf_3000..0x0898_c000;
const JOURNAL_LIMIT: u64 = 262_144;
/// (browser key, record offset, legacy/WPF buffer bytes, proven V2 capacity in UTF-16 units)
pub(crate) const TEAM_TEXT: [(&str, usize, usize, usize); 5] = [
    ("name", 0x2FA, 48, 0x19),
    ("cityName", 0x32C, 36, 0x12),
    ("cityShort", 0x350, 14, 7),
    ("stateShort", 0x35E, 14, 0xA),
    ("mainLogo", 0x1AE, 72, 0x26),
];
pub(crate) const UNIFORM_SLOTS: usize = 0xDE0;
pub(crate) const UNIFORM_SLOT_COUNT: usize = 32;
pub(crate) const TEAM_ID: usize = 0xF20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Player,
    Team,
    Uniform,
    /// Team staff record (owner+0x180 table), WPF TeamStaffPatch masks.
    Staff,
    /// 64-byte season stat record (owner+0x1D8 table), WPF season masks.
    Season,
}
impl Kind {
    pub fn stride(self) -> usize {
        match self {
            Kind::Player => profile::PLAYER_STRIDE,
            Kind::Team => profile::TEAM_STRIDE,
            Kind::Uniform => profile::UNIFORM_STRIDE,
            Kind::Staff => crate::roster_v2_services::STAFF_STRIDE,
            Kind::Season => crate::roster_v2_services::SEASON_STRIDE,
        }
    }
    fn span_limit(self) -> usize {
        match self {
            Kind::Player | Kind::Staff => 40,
            Kind::Team => 72,
            Kind::Uniform => 4,
            Kind::Season => 64,
        }
    }
    fn counts(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Kind::Player => 100..=8192,
            Kind::Team => 1..=1024,
            Kind::Uniform | Kind::Staff => 1..=8192,
            Kind::Season => 1..=1_000_000,
        }
    }
}

// ---------------------------------------------------------------- proof ----
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Proof {
    schema: String,
    image_sha256: String,
    game_process_access: bool,
    game_files_changed: bool,
    uniform: UniformProof,
    team_text: Vec<TeamText>,
    uniform_slots: Slots,
    anchors: Vec<Anchor>,
    generator_sha256: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UniformProof {
    serializer_rva: u64,
    record_bytes: usize,
    record_sha256: String,
    controls: Vec<Control>,
    identity: Vec<IdentityField>,
    mismatches: Vec<Value>,
    display_fields: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Control {
    request: String,
    serializer_key: String,
    offset: usize,
    shift: u32,
    mask: u32,
    name_rva: u64,
    matches: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityField {
    property: String,
    offset: usize,
    bytes: usize,
    name_rva: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamText {
    key: String,
    offset: usize,
    capacity_units: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Slots {
    offset: usize,
    count: usize,
    stride: usize,
    uniform_table: usize,
    uniform_stride: usize,
}
#[derive(Deserialize)]
struct Anchor {
    rva: u64,
    hex: String,
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    ensure!(
        !s.is_empty() && s.len() % 2 == 0 && s.len() <= 512,
        "Invalid write-proof anchor"
    );
    (0..s.len())
        .step_by(2)
        .map(|i| Ok(u8::from_str_radix(&s[i..i + 2], 16)?))
        .collect()
}
fn check_proof(p: &Proof) -> Result<()> {
    ensure!(
        p.schema == PROOF_SCHEMA
            && p.image_sha256 == IMAGE_HASH
            && !p.game_process_access
            && !p.game_files_changed
            && p.generator_sha256.len() == 64,
        "Unsupported Version 2 write proof"
    );
    let u = &p.uniform;
    ensure!(
        u.serializer_rva == UNIFORM_SERIALIZER
            && u.record_bytes == UNIFORM_RECORD_BYTES
            && u.record_sha256.len() == 64
            && u.mismatches.is_empty()
            && u.display_fields.len() == 81,
        "Incomplete Version 2 uniform proof"
    );
    let legacy = uniform_changes::fields();
    ensure!(
        u.controls.len() == 52 && legacy.len() == 52,
        "Uniform control proof is incomplete"
    );
    for (c, f) in u.controls.iter().zip(legacy) {
        ensure!(
            c.matches
                && c.request == f.request
                && c.offset == f.offset
                && c.shift == f.shift
                && c.mask == f.mask
                && !c.serializer_key.is_empty()
                && (UNIFORM_SERIALIZER..UNIFORM_SERIALIZER + UNIFORM_RECORD_BYTES as u64)
                    .contains(&c.name_rva),
            "Uniform control {} is not the proven writer layout",
            f.request
        );
    }
    let identity: Vec<_> = u
        .identity
        .iter()
        .map(|i| (i.property.as_str(), i.offset, i.bytes))
        .collect();
    ensure!(
        identity
            == [
                ("ID", 0x50, 2),
                ("TeamID", 0x52, 2),
                ("BaseFilename", 8, 64)
            ]
            && u.identity.iter().all(|i| {
                (UNIFORM_SERIALIZER..UNIFORM_SERIALIZER + UNIFORM_RECORD_BYTES as u64)
                    .contains(&i.name_rva)
            }),
        "Uniform identity proof differs"
    );
    let text: Vec<_> = p
        .team_text
        .iter()
        .map(|t| (t.key.as_str(), t.offset, t.capacity_units))
        .collect();
    let expected: Vec<_> = TEAM_TEXT.iter().map(|t| (t.0, t.1, t.3)).collect();
    ensure!(text == expected, "Team text proof differs");
    for (_, _, bytes, capacity) in TEAM_TEXT {
        ensure!(bytes <= capacity * 2, "Team text exceeds proven buffer");
    }
    let s = &p.uniform_slots;
    ensure!(
        (
            s.offset,
            s.count,
            s.stride,
            s.uniform_table,
            s.uniform_stride
        ) == (
            UNIFORM_SLOTS,
            UNIFORM_SLOT_COUNT,
            8,
            0x298,
            profile::UNIFORM_STRIDE
        ) && UNIFORM_SLOTS + UNIFORM_SLOT_COUNT * 8 == 0xEE0,
        "Uniform slot proof differs"
    );
    ensure!(p.anchors.len() == 7, "Write-proof anchors are incomplete");
    for a in &p.anchors {
        unhex(&a.hex)?;
        ensure!(
            (0x1000..0x05cf_3000).contains(&a.rva),
            "Write-proof anchor is outside code"
        );
    }
    Ok(())
}
fn proof() -> Result<&'static Proof> {
    static P: OnceLock<std::result::Result<Proof, String>> = OnceLock::new();
    P.get_or_init(|| {
        serde_json::from_str::<Proof>(include_str!("../assets/roster-v2-write-proof.json"))
            .map_err(|e| e.to_string())
            .and_then(|p| check_proof(&p).map(|_| p).map_err(|e| format!("{e:#}")))
    })
    .as_ref()
    .map_err(|e| anyhow::anyhow!("Version 2 write proof unavailable: {e}"))
}
fn check_audio_proof(p: &Value) -> Result<()> {
    ensure!(
        p["schema"] == "nba2k27-v2-player-full-name-audio-v1"
            && p["imageSha256"] == IMAGE_HASH
            && p["field"] == "FirstLastNameSID"
            && p["offset"] == FULL_NAME_SID
            && p["bytes"] == 2
            && p["signed"] == true
            && p["requestMinimum"] == -1
            && p["requestMaximum"] == 32767
            && p["adjacentFieldOffset"] == FULL_NAME_SID + 2
            && p["propertyNameRva"] == 0x6b2ebau64,
        "Unsupported full-name audio scalar proof"
    );
    let anchors = p["anchors"].as_array().context("Missing audio anchors")?;
    ensure!(
        anchors.len() == AUDIO_ANCHORS.len(),
        "Incomplete audio anchors"
    );
    for (a, &(rva, hex)) in anchors.iter().zip(AUDIO_ANCHORS) {
        ensure!(
            a["rva"] == rva && a["hex"] == hex,
            "Audio field binding differs"
        );
    }
    let methods = p["methods"].as_array().context("Missing audio methods")?;
    ensure!(
        methods.len() == AUDIO_METHODS.len(),
        "Incomplete audio methods"
    );
    for (m, &(rva, len, sha)) in methods.iter().zip(AUDIO_METHODS) {
        ensure!(
            m["rva"] == rva && m["bytes"] == len && m["sha256"] == sha,
            "Audio signed numeric method differs"
        );
    }
    Ok(())
}
fn audio_proof() -> Result<()> {
    static AUDIO: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    AUDIO
        .get_or_init(|| {
            serde_json::from_str::<Value>(include_str!("../assets/roster-v2-audio-proof.json"))
                .map_err(|e| e.to_string())
                .and_then(|p| check_audio_proof(&p).map_err(|e| format!("{e:#}")))
        })
        .as_ref()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Audio proof unavailable: {e}"))
}
fn check_team_metadata_proof(p: &Value) -> Result<()> {
    ensure!(
        p["schema"] == "nba2k27-v2-team-type-classic-year-v1"
            && p["imageSha256"] == IMAGE_HASH
            && p["type"]["offset"] == 0x123C
            && p["type"]["shift"] == 26
            && p["type"]["bits"] == 6
            && p["type"]["mask"] == 0xfc000000u64
            && p["type"]["knownOriginalMaximum"] == 35
            && p["type"]["targetMaximum"] == 35
            && p["classicYear"]["offset"] == 0x1548
            && p["classicYear"]["shift"] == 19
            && p["classicYear"]["bits"] == 8
            && p["classicYear"]["mask"] == 0x7f80000u64
            && p["classicYear"]["knownOriginalMaximum"] == 127
            && p["classicYear"]["canonicalTargetMaximum"] == 100
            && p["classicYear"]["legacyHistoricYearAlwaysNeutral"] == true
            && p["enumDescriptorRva"] == 0x89be960u64
            && p["enumTargetRva"] == 0xa1e7cd0u64,
        "Unsupported native team metadata proof"
    );
    let spans = p["spans"]
        .as_array()
        .context("Missing native team metadata spans")?;
    ensure!(
        spans.len() == TEAM_METADATA_SPANS.len(),
        "Incomplete native team metadata evidence"
    );
    for (s, &(rva, len, hash)) in spans.iter().zip(TEAM_METADATA_SPANS) {
        ensure!(
            s["rva"] == rva && s["bytes"] == len && s["sha256"] == hash,
            "Native team metadata span differs"
        );
    }
    Ok(())
}
fn team_enum_binding(base: u64, pointer: u64) -> Result<()> {
    ensure!(
        base.checked_add(0x0A1E7CD0)
            .is_some_and(|expected| expected == pointer),
        "Native team Type descriptor enum owner differs"
    );
    Ok(())
}
fn team_metadata_proof() -> Result<()> {
    static P: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    P.get_or_init(|| {
        serde_json::from_str::<Value>(include_str!("../assets/roster-v2-team-metadata-proof.json"))
            .map_err(|e| e.to_string())
            .and_then(|p| check_team_metadata_proof(&p).map_err(|e| format!("{e:#}")))
    })
    .as_ref()
    .map(|_| ())
    .map_err(|e| anyhow::anyhow!("Native team metadata proof unavailable: {e}"))
}
// The native reflection initializer replaces the ClassicYear descriptor's
// provisional type hash with the signed-32 hash. All field-layout bytes stay
// exact. Accept only these two proven states, never an arbitrary masked value.
fn team_metadata_bytes(rva: u64, mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    if rva == 0x089BFAE8 {
        ensure!(bytes.len() == 32, "Incomplete classic-year descriptor");
        let kind = u32::from_le_bytes(bytes[4..8].try_into()?);
        ensure!(matches!(kind, 0x12D46C62 | 0x1451DAB1),
            "Native classic-year descriptor type differs");
        bytes[4..8].copy_from_slice(&0x12D46C62u32.to_le_bytes());
    }
    Ok(bytes)
}
/// Content tag for the exact field, structural-write and audio proofs. Every intent records it; a journal written
/// under different evidence is never treated as this writer's.
pub(crate) fn proof_tag() -> &'static str {
    static T: OnceLock<String> = OnceLock::new();
    T.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(include_str!("../assets/roster-v2-field-proof.json"));
        h.update(include_str!("../assets/roster-v2-write-proof.json"));
        h.update(include_str!("../assets/roster-v2-audio-proof.json"));
        h.update(include_str!("../assets/roster-v2-team-metadata-proof.json"));
        h.update(b"classic-year-type-initialization-v1:12d46c62:1451dab1");
        for &(rva, len, sha) in TEAM_METADATA_INITIALIZER {
            h.update(rva.to_le_bytes());
            h.update((len as u64).to_le_bytes());
            h.update(sha.as_bytes());
        }
        h.update(include_str!("../assets/roster-v2-arena-proof.json"));
        h.update(include_str!("../assets/roster-v2-origins-proof.json"));
        format!("{:x}", h.finalize())
    })
}
fn read(m: &impl Memory, at: u64, len: usize) -> Result<Vec<u8>> {
    m.checkpoint()?;
    let bytes = m.read(at, len)?;
    ensure!(bytes.len() == len, "Partial write-profile read");
    Ok(bytes)
}
fn name_ref(m: &impl Memory, rva: u64, key: &str) -> Result<()> {
    let bytes = read(m, m.base() + rva, 7)?;
    ensure!(
        bytes[..3] == [0x48, 0x8d, 0x15],
        "Uniform property-name reference differs"
    );
    let target = rva as i64 + 7 + i64::from(i32::from_le_bytes(bytes[3..].try_into()?));
    ensure!(
        RDATA.contains(&target),
        "Uniform property name is outside read-only data"
    );
    let mut expected = key.as_bytes().to_vec();
    expected.push(0);
    ensure!(
        read(m, m.base() + target as u64, expected.len())? == expected,
        "Uniform serializer property name differs"
    );
    Ok(())
}
/// One bounded check per save: exact build identity, code anchors for team
/// text/uniform slots, the uniform serializer body and its property names.
/// Player bitfields are verified by roster_v2_readonly::verify_profile.
pub(crate) fn verify_write_profile(m: &impl Memory) -> Result<()> {
    profile::verify_identity_profile(m)?;
    let p = proof()?;
    for a in &p.anchors {
        let expected = unhex(&a.hex)?;
        ensure!(
            read(m, m.base() + a.rva, expected.len())? == expected,
            "Version 2 roster write anchor differs"
        );
    }
    let mut hash = Sha256::new();
    for offset in (0..UNIFORM_RECORD_BYTES).step_by(4096) {
        hash.update(read(
            m,
            m.base() + UNIFORM_SERIALIZER + offset as u64,
            (UNIFORM_RECORD_BYTES - offset).min(4096),
        )?);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == p.uniform.record_sha256,
        "Version 2 uniform serializer differs"
    );
    for c in &p.uniform.controls {
        name_ref(m, c.name_rva, &c.serializer_key)?;
    }
    for i in &p.uniform.identity {
        name_ref(m, i.name_rva, &i.property)?;
    }
    audio_proof()?;
    for &(rva, hex) in AUDIO_ANCHORS {
        let expected = unhex(hex)?;
        ensure!(
            read(m, m.base() + rva, expected.len())? == expected,
            "Full-name audio field binding differs"
        );
    }
    for &(rva, len, sha) in AUDIO_METHODS {
        ensure!(
            format!("{:x}", Sha256::digest(read(m, m.base() + rva, len)?)) == sha,
            "Full-name audio signed numeric method differs"
        );
    }
    name_ref(m, 0x006B_2EBA, "FirstLastNameSID")?;
    team_metadata_proof()?;
    for &(rva, len, expected) in TEAM_METADATA_INITIALIZER {
        ensure!(format!("{:x}", Sha256::digest(read(m, m.base() + rva, len)?)) == expected,
            "Native team metadata initializer differs at {rva:#x}");
    }
    for &(rva, len, expected) in TEAM_METADATA_SPANS {
        let mut hash = Sha256::new();
        for offset in (0..len).step_by(4096) {
            hash.update(team_metadata_bytes(rva, read(
                m,
                m.base() + rva + offset as u64,
                (len - offset).min(4096),
            )?)?);
        }
        ensure!(
            format!("{:x}", hash.finalize()) == expected,
            "Native team metadata code/field evidence differs at {rva:#x}"
        );
    }
    let enum_pointer =
        u64::from_le_bytes(read(m, m.base() + 0x089BE960, 8)?.as_slice().try_into()?);
    team_enum_binding(m.base(), enum_pointer)?;
    crate::roster_v2_arena::verify_profile(m)?;
    crate::roster_v2_origins::verify_profile(m)?;
    // Player weight: the serializer names the float it loads from +0x108.
    name_ref(m, 0x006A_E7DF, "Weight")?;
    m.checkpoint()
}

// ------------------------------------------------------------ bitfields ----
pub(crate) type Bits = (usize, u32, u32);
fn bio(key: &str) -> Result<Bits> {
    model::BIO
        .iter()
        .find(|b| b.0 == key)
        .map(|b| (b.1, b.2, b.3))
        .with_context(|| format!("Missing proven player field {key}"))
}
fn rating(key: &str) -> Result<&'static roster_snapshot::RatingField> {
    roster_snapshot::rating_fields()
        .iter()
        .find(|f| f.key == key)
        .with_context(|| format!("Missing proven player field {key}"))
}
pub(crate) fn get(row: &[u8], (offset, shift, bits): Bits) -> Result<u64> {
    ensure!(
        (1..=32).contains(&bits) && shift + bits <= 64,
        "Invalid field width"
    );
    let start = offset * 8 + shift as usize;
    ensure!(
        (start + bits as usize).div_ceil(8) <= row.len(),
        "Field is outside the record"
    );
    Ok((0..bits as usize).fold(0, |v, i| {
        let g = start + i;
        v | (u64::from((row[g / 8] >> (g % 8)) & 1) << i)
    }))
}
fn utf16(row: &[u8], at: usize, len: usize) -> Result<String> {
    let words: Vec<_> = row
        .get(at..at + len)
        .context("Incomplete record text")?
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|n| *n != 0)
        .collect();
    Ok(String::from_utf16_lossy(&words).trim().to_owned())
}
fn fixed_text(s: &str, len: usize) -> Result<Vec<u8>> {
    ensure!(
        !s.chars().any(char::is_control) && s.encode_utf16().count() < len / 2,
        "Text contains control characters or exceeds its buffer"
    );
    let mut b = vec![0; len];
    for (i, n) in s.encode_utf16().enumerate() {
        b[i * 2..i * 2 + 2].copy_from_slice(&n.to_le_bytes());
    }
    Ok(b)
}

// ---------------------------------------------------------------- plans ----
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    pub offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub mask: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub kind: Kind,
    pub index: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub patches: Vec<Patch>,
    pub guards: Vec<(usize, usize)>,
    pub changed: Vec<String>,
}
impl Plan {
    pub fn expected(&self, applied: usize) -> Result<Vec<u8>> {
        ensure!(applied <= self.patches.len(), "Invalid transaction step");
        let mut raw = self.before.clone();
        for p in self.patches.iter().take(applied) {
            raw[p.offset..p.offset + p.after.len()].copy_from_slice(&p.after);
        }
        Ok(raw)
    }
    pub fn matches(&self, raw: &[u8], expected: &[u8]) -> Result<bool> {
        ensure!(
            raw.len() == self.kind.stride() && expected.len() == self.kind.stride(),
            "Incomplete record buffer"
        );
        Ok(self
            .guards
            .iter()
            .all(|&(o, n)| raw[o..o + n] == expected[o..o + n]))
    }
}
struct Builder {
    kind: Kind,
    index: usize,
    before: Vec<u8>,
    after: Vec<u8>,
    mask: Vec<u8>,
    spans: Vec<(usize, usize)>,
    changed: Vec<String>,
}
impl Builder {
    fn new(kind: Kind, index: usize, raw: &[u8]) -> Result<Self> {
        ensure!(raw.len() == kind.stride(), "Incomplete record buffer");
        Ok(Self {
            kind,
            index,
            before: raw.to_vec(),
            after: raw.to_vec(),
            mask: vec![0; raw.len()],
            spans: vec![],
            changed: vec![],
        })
    }
    fn note(&mut self, first: usize, last: usize, name: &str, changed: bool) {
        if changed {
            self.spans.push((first, last - first + 1));
            if !self.changed.iter().any(|s| s == name) {
                self.changed.push(name.to_owned());
            }
        }
    }
    fn field(&mut self, (offset, shift, bits): Bits, value: u64, name: &str) -> Result<()> {
        ensure!(
            (1..=32).contains(&bits) && value < (1u64 << bits),
            "{name} is outside its proven bit width"
        );
        let start = offset * 8 + shift as usize;
        ensure!(
            (start + bits as usize).div_ceil(8) <= self.before.len(),
            "{name} is outside the record"
        );
        let mut changed = false;
        for i in 0..bits as usize {
            let (byte, bit) = ((start + i) / 8, 1u8 << ((start + i) % 8));
            let want = (value >> i) & 1 == 1;
            ensure!(
                self.mask[byte] & bit == 0 || ((self.after[byte] & bit != 0) == want),
                "Conflicting field masks"
            );
            self.mask[byte] |= bit;
            if want {
                self.after[byte] |= bit;
            } else {
                self.after[byte] &= !bit;
            }
            changed |= (self.before[byte] & bit != 0) != want;
        }
        let (first, last) = (start / 8, (start + bits as usize - 1) / 8);
        self.note(first, last, name, changed);
        Ok(())
    }
    fn bytes(&mut self, offset: usize, data: &[u8], name: &str) -> Result<()> {
        let end = offset + data.len();
        ensure!(
            !data.is_empty() && end <= self.before.len(),
            "{name} is outside the record"
        );
        ensure!(
            self.mask[offset..end].iter().all(|m| *m == 0),
            "Conflicting field masks"
        );
        self.mask[offset..end].fill(255);
        self.after[offset..end].copy_from_slice(data);
        let changed = self.before[offset..end] != *data;
        self.note(offset, end - 1, name, changed);
        Ok(())
    }
    fn finish(mut self, identity: &[(usize, usize)]) -> Result<Plan> {
        // Logical fields stay whole (names are never split per byte). Touching
        // or overlapping spans merge while they fit one bounded write.
        self.spans.sort();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        for (start, len) in self.spans {
            if let Some((s, n)) = spans.last_mut() {
                if start <= *s + *n {
                    let total = (start + len).max(*s + *n) - *s;
                    if total <= self.kind.span_limit() {
                        *n = total;
                        continue;
                    }
                    ensure!(start == *s + *n, "Overlapping patch exceeds its bound");
                }
            }
            spans.push((start, len));
        }
        let patches: Vec<_> = spans
            .iter()
            .map(|&(o, n)| Patch {
                offset: o,
                before: self.before[o..o + n].to_vec(),
                after: self.after[o..o + n].to_vec(),
                mask: self.mask[o..o + n].to_vec(),
            })
            .filter(|p| p.before != p.after)
            .collect();
        let mut guards = identity.to_vec();
        guards.extend(patches.iter().map(|p| (p.offset, p.after.len())));
        let plan = Plan {
            kind: self.kind,
            index: self.index,
            before: self.before,
            after: self.after,
            patches,
            guards,
            changed: self.changed,
        };
        ensure!(
            plan.expected(plan.patches.len())? == plan.after,
            "Incomplete write plan"
        );
        Ok(plan)
    }
}
fn neutral(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::Number(n)) => n.as_f64() == Some(0.),
        Some(Value::String(s)) => s.trim().is_empty(),
        _ => false,
    }
}
const UNMAPPED: &str =
    "is not mapped for Version 2 saves. Leave it unchanged; no game write was attempted.";
/// WPF request validation, after refusing every field Version 2 cannot prove.
/// The flag says whether the request carries a real weight (WPF form value).
pub(crate) fn validate_player(input: &Value) -> Result<(player_changes::Edit, bool)> {
    ensure!(input.is_object(), "Expected a player-edit object");
    for (key, label) in [("fromIndex", "From/college")] {
        ensure!(neutral(input.get(key)), "{label} {UNMAPPED}");
    }
    let weight = !neutral(input.get("weightLbs"));
    for (key, label) in [("heightCm", "Height"), ("wingspanCm", "Wingspan")] {
        ensure!(
            matches!(input.get(key), None | Some(Value::Null)),
            "{label} {UNMAPPED}"
        );
    }
    let mut v = input.clone();
    for key in ["fromIndex"] {
        v.as_object_mut().unwrap().remove(key);
    }
    // Placeholders satisfy the original validator only; they are never planned.
    if !weight {
        v["weightLbs"] = json!(200);
    }
    v["heightCm"] = json!(200);
    v["wingspanCm"] = json!(200);
    Ok((player_changes::validate(&v)?, weight))
}
/// Weight f32 at player+0x108: V2 serializer `vmovss xmm6,[rdi+0x108]` ->
/// property "Weight" (float writer 0x4BECFA0), inside the hashed body.
pub(crate) const WEIGHT: usize = 0x108;
pub(crate) fn weight_display(raw: &[u8]) -> Option<f64> {
    let w = f32::from_le_bytes(raw.get(WEIGHT..WEIGHT + 4)?.try_into().ok()?);
    (w.is_finite() && (70.0..=500.0).contains(&w)).then(|| (w as f64 * 100.).round() / 100.)
}
fn cached_u64(cached: &Value, key: &str) -> Result<u64> {
    cached[key]
        .as_u64()
        .with_context(|| format!("The original {key} is unavailable"))
}
pub(crate) const PLAYER_GUARDS: [(usize, usize); 11] = [
    (0xB0, 8),
    (0, 80),
    (WEIGHT, 4),
    (FULL_NAME_SID, 2),
    (0x110, 4),
    (0x114, 4),
    (0x128, 2),
    (0x12C, 4),
    (0x36A, 2),
    (0x3E0, 4),
    (0x45C, 4),
];
pub(crate) fn plan_player(input: &Value, cached: &Value, raw: &[u8]) -> Result<Plan> {
    ensure!(
        input.get("nativeOriginIndex").is_none() && input.get("nativeOriginCatalogId").is_none(),
        "Native origin request requires a verified catalog selection"
    );
    plan_player_inner(input, cached, raw, None)
}
/// Internal native-derived selection only: IDs/catalog tokens are browser data;
/// original and target addresses come exclusively from the retained native table.
pub(crate) fn plan_player_with_origin(
    input: &Value,
    cached: &Value,
    raw: &[u8],
    selection: Option<&crate::roster_v2_origins::Selection>,
) -> Result<Plan> {
    match selection {
        None => plan_player(input, cached, raw),
        Some(selection) => {
            let index = input
                .get("nativeOriginIndex")
                .and_then(Value::as_u64)
                .context("Expected an explicit native origin index")?;
            let catalog = input
                .get("nativeOriginCatalogId")
                .and_then(Value::as_str)
                .context("Expected an explicit native origin catalog identity")?;
            selection.validate_catalog(catalog)?;
            let pointer = selection.validate_request(index, raw)?;
            let mut normalized = input.clone();
            let object = normalized
                .as_object_mut()
                .context("Expected a player-edit object")?;
            object.remove("nativeOriginIndex");
            object.remove("nativeOriginCatalogId");
            plan_player_inner(&normalized, cached, raw, Some(pointer))
        }
    }
}
fn plan_player_inner(
    input: &Value,
    cached: &Value,
    raw: &[u8],
    origin: Option<u64>,
) -> Result<Plan> {
    ensure!(
        raw.len() == profile::PLAYER_STRIDE,
        "Incomplete player record"
    );
    // Preserve an exact unchanged native echo even if it is an unknown negative
    // sentinel; only newly requested values enter the -1..32767 write domain.
    let mut normalized = input.clone();
    if let Some(sent) = input.get("fullNameId").filter(|v| !v.is_null()) {
        let sent = sent
            .as_i64()
            .or_else(|| sent.as_str().and_then(|s| s.trim().parse::<i64>().ok()));
        let live = i64::from(i16::from_le_bytes(
            raw[FULL_NAME_SID..FULL_NAME_SID + 2].try_into()?,
        ));
        if sent.is_some() && sent == cached["audio"]["fullNameId"].as_i64() && sent == Some(live) {
            normalized
                .as_object_mut()
                .context("Expected a player-edit object")?
                .remove("fullNameId");
        }
    }
    let (edit, weight) = validate_player(&normalized)?;
    ensure!(
        cached["source"] == "live-roster"
            && cached["recordIndex"].as_u64() == Some(edit.record_index as u64)
            && cached["firstName"] == edit.expected_first_name
            && cached["lastName"] == edit.expected_last_name
            && cached["cyberfaceId"].as_u64() == Some(edit.expected_cyberface_id as u64),
        "The selected player no longer matches this editor snapshot"
    );
    let graphic = bio("GraphicId")?;
    ensure!(
        utf16(raw, 40, 40)? == edit.expected_first_name
            && utf16(raw, 0, 40)? == edit.expected_last_name
            && get(raw, graphic)? == edit.expected_cyberface_id as u64,
        "The live player identity changed before Save"
    );
    let potential = rating("PotentialRaw")?;
    let potential = (potential.offset, potential.shift, potential.bits);
    let position = bio("Position")?;
    let secondary = bio("SecondaryPosition")?;
    let jersey = bio("JerseyNumber")?;
    for (key, bits) in [
        ("positionId", position),
        ("secondaryPositionId", secondary),
        ("jerseyNumberRaw", jersey),
        ("potentialRaw", potential),
    ] {
        ensure!(
            get(raw, bits)? == cached_u64(cached, key)?,
            "The live {key} changed before Save"
        );
    }
    let mut b = Builder::new(Kind::Player, edit.record_index, raw)?;
    if let Some(pointer) = origin {
        crate::roster_v2_origins::proof()?;
        b.bytes(0xB0, &pointer.to_le_bytes(), "nativeOriginIndex")?;
    }
    if let Some(id) = edit.full_name_id {
        audio_proof()?;
        let expected = cached["audio"]["fullNameId"]
            .as_i64()
            .filter(|n| (i16::MIN as i64..=i16::MAX as i64).contains(n))
            .context("The original full-name audio ID is unavailable")?;
        ensure!(
            expected >= -1,
            "Unknown negative full-name audio baseline is read-only; preserve it unchanged"
        );
        ensure!(
            i64::from(i16::from_le_bytes(
                raw[FULL_NAME_SID..FULL_NAME_SID + 2].try_into()?
            )) == expected,
            "The live full-name audio ID changed before Save"
        );
        b.bytes(FULL_NAME_SID, &id.to_le_bytes(), "fullNameId")?;
    }
    if edit.first_name != edit.expected_first_name {
        b.bytes(40, &fixed_text(&edit.first_name, 40)?, "firstName")?;
    }
    if edit.last_name != edit.expected_last_name {
        b.bytes(0, &fixed_text(&edit.last_name, 40)?, "lastName")?;
    }
    b.field(graphic, edit.cyberface_id as u64, "cyberfaceId")?;
    b.field(position, edit.position_id as u64, "positionId")?;
    b.field(
        secondary,
        edit.secondary_position_id as u64,
        "secondaryPositionId",
    )?;
    b.field(jersey, edit.jersey_number_raw as u64, "jerseyNumberRaw")?;
    let raw_potential = if edit.potential as u64 == cached_u64(cached, "potential")? {
        cached_u64(cached, "potentialRaw")?
    } else if edit.potential <= 25 {
        0
    } else {
        255u64.min((edit.potential as u64 - 25) * 3 - 1)
    };
    b.field(potential, raw_potential, "potential")?;
    if weight {
        // WPF: retain the exact original float when the displayed value is unchanged.
        let display = weight_display(raw).context("The live weight value is unavailable")?;
        ensure!(
            cached["weightLbs"].as_f64() == Some(display),
            "The live weight changed before Save"
        );
        if edit.weight_lbs != display {
            b.bytes(WEIGHT, &(edit.weight_lbs as f32).to_le_bytes(), "weightLbs")?;
        }
    }
    if !edit.rating_edits.is_empty() {
        ensure!(
            cached["ratingsRaw"]
                .as_array()
                .is_some_and(|a| a.len() == 455),
            "The original rating layout is unavailable"
        );
    }
    for change in &edit.rating_edits {
        let f = &roster_snapshot::rating_fields()[change.index];
        ensure!(
            f.maximum >= 0 && change.expected <= i32::MAX as u32,
            "This player field is read-only"
        );
        let bits = (f.offset, f.shift, f.bits);
        ensure!(
            cached["ratingsRaw"][change.index].as_u64() == Some(change.expected as u64)
                && get(raw, bits)? == change.expected as u64,
            "A selected player field changed before Save"
        );
        b.field(bits, change.value as u64, &f.key)?;
    }
    if let Some(birthday) = &edit.birth_date {
        ensure!(
            cached["bioLayoutVerified"] == true,
            "The birthday layout is unavailable"
        );
        let (year, month, day) = player_changes::date(birthday)?;
        for (key, field, expected) in [
            (
                "birthYear",
                "BirthYear",
                edit.expected_birth_year.map(u64::from),
            ),
            (
                "birthMonth",
                "BirthMonth",
                edit.expected_birth_month.map(u64::from),
            ),
            (
                "birthDay",
                "BirthDay",
                edit.expected_birth_day.map(u64::from),
            ),
        ] {
            let expected = expected.context("Missing original birthday")?;
            ensure!(
                cached[key].as_u64() == Some(expected) && get(raw, bio(field)?)? == expected,
                "The original birthday changed before Save"
            );
        }
        b.field(bio("BirthYear")?, year as u64, "birthDate")?;
        b.field(bio("BirthMonth")?, month as u64, "birthDate")?;
        b.field(bio("BirthDay")?, day as u64, "birthDate")?;
    }
    if let Some(years) = edit.years_pro {
        let expected = edit
            .expected_years_pro
            .context("Missing original years pro")?;
        ensure!(
            cached["bioLayoutVerified"] == true
                && cached["yearsPro"].as_u64() == Some(expected as u64)
                && get(raw, bio("YearsPro")?)? == expected as u64,
            "The original years pro changed before Save"
        );
        b.field(bio("YearsPro")?, years as u64, "yearsPro")?;
    }
    b.finish(&PLAYER_GUARDS)
}
/// Native metadata requests are explicit scalars; legacy historicYearId stays
/// neutral and never acquires a writable mask.
fn metadata_request(input: &Value, key: &str, max: u64) -> Result<Option<u64>> {
    input
        .get(key)
        .map(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
                .filter(|n| n.is_finite() && n.fract() == 0. && *n >= 0. && *n <= max as f64)
                .map(|n| n as u64)
                .context("Invalid native team metadata request")
        })
        .transpose()
}
pub(crate) fn plan_team(input: &Value, cached: &Value, raw: &[u8]) -> Result<Plan> {
    ensure!(
        input.get("nativeArenaId").is_none() && input.get("nativeArenaCatalogId").is_none(),
        "Native arena request requires a verified catalog selection"
    );
    plan_team_inner(input, cached, raw, None)
}
/// Only the native immutable catalog may construct this arena selection. Browser
/// input supplies an ID, never an address; the adapter retains both full rows.
pub(crate) fn plan_team_with_arena(
    input: &Value,
    cached: &Value,
    raw: &[u8],
    selection: Option<&crate::roster_v2_arena::Selection>,
) -> Result<Plan> {
    match selection {
        None => plan_team(input, cached, raw),
        Some(selection) => {
            let requested = input
                .get("nativeArenaId")
                .and_then(Value::as_u64)
                .context("Expected an explicit native arena ID")?;
            let catalog = input
                .get("nativeArenaCatalogId")
                .and_then(Value::as_str)
                .context("Expected an explicit native arena catalog identity")?;
            selection.validate_catalog(catalog)?;
            let pointer = selection.validate_request(requested, raw)?;
            let mut normalized = input.clone();
            normalized
                .as_object_mut()
                .context("Expected a team-edit object")?
                .remove("nativeArenaId");
            normalized
                .as_object_mut()
                .unwrap()
                .remove("nativeArenaCatalogId");
            plan_team_inner(&normalized, cached, raw, Some(pointer))
        }
    }
}
fn plan_team_inner(input: &Value, cached: &Value, raw: &[u8], arena: Option<u64>) -> Result<Plan> {
    ensure!(raw.len() == profile::TEAM_STRIDE, "Incomplete team record");
    team_metadata_proof()?;
    let kind = cached["rosterTypeId"]
        .as_u64()
        .filter(|v| *v <= 63)
        .context("The original native team type is unavailable")?;
    ensure!(
        get(raw, TEAM_TYPE_BITS)? == kind,
        "The native team type changed before Save"
    );
    let desired_kind = metadata_request(input, "rosterTypeId", 63)?.unwrap_or(kind);
    if desired_kind != kind {
        ensure!(
            kind <= 35 && desired_kind <= 35,
            "Unknown native team type is read-only"
        );
    }
    let mut desired_year = metadata_request(input, "classicTeamEndYearRaw", 255)?;
    let year = cached["classicTeamEndYearRaw"].as_u64();
    if let Some(year) = year {
        ensure!(
            year <= 255 && get(raw, TEAM_CLASSIC_YEAR_BITS)? == year,
            "The native classic-team year changed before Save"
        );
        if let Some(desired) = desired_year {
            if desired != year {
                ensure!(
                    year <= 127 && desired <= 100,
                    "Unknown native classic-year is read-only; changed years require canonical0..100"
                );
                let calendar = |raw: u64| {
                    if raw == 0 {
                        None
                    } else {
                        Some(if raw < 60 { 2000 + raw } else { 1900 + raw })
                    }
                };
                if desired != 0 && calendar(desired) == calendar(year) {
                    desired_year = Some(year); // Preserve exact native duplicate encoding.
                }
            }
        }
    } else {
        ensure!(
            desired_year.is_none(),
            "Refresh native team metadata before saving"
        );
    }
    let mut normalized = input.clone();
    normalized
        .as_object_mut()
        .context("Expected a team-edit object")?
        .insert("rosterTypeId".into(), json!(0));
    let edit = team_changes::validate(&normalized)?;
    ensure!(edit.historic_year_id == 0, "Historic year {UNMAPPED}");
    ensure!(
        edit.arena_id == 0 && edit.floor_file.is_none(),
        "Arena and floor {UNMAPPED}"
    );
    ensure!(
        cached["source"] == "live-team"
            && cached["recordIndex"].as_u64() == Some(edit.record_index as u64)
            && cached["teamId"].as_u64() == Some(edit.team_id as u64)
            && cached["name"] == edit.expected_name,
        "The selected team no longer matches this editor snapshot"
    );
    ensure!(
        get(raw, (TEAM_ID, 0, 16))? == edit.team_id as u64
            && utf16(raw, 0x2FA, 48)? == edit.expected_name,
        "The live team identity changed before Save"
    );
    let values = [
        &edit.name,
        &edit.city_name,
        &edit.city_short,
        &edit.state_short,
        &edit.main_logo,
    ];
    let mut b = Builder::new(Kind::Team, edit.record_index, raw)?;
    if let Some(pointer) = arena {
        crate::roster_v2_arena::proof()?;
        b.bytes(0xEE0, &pointer.to_le_bytes(), "nativeArenaId")?;
    }
    b.field(TEAM_TYPE_BITS, desired_kind, "rosterTypeId")?;
    if let Some(year) = desired_year {
        b.field(TEAM_CLASSIC_YEAR_BITS, year, "classicTeamEndYearRaw")?;
    }
    for ((key, offset, len, _), value) in TEAM_TEXT.iter().zip(values) {
        ensure!(
            cached[*key].as_str() == Some(utf16(raw, *offset, *len)?.as_str()),
            "The team's {key} changed. Refresh teams before saving."
        );
        b.bytes(*offset, &fixed_text(value, *len)?, key)?;
    }
    b.finish(&[
        (TEAM_ID, 2),
        (0x2FA, 48),
        (0x123C, 4),
        (0x1548, 4),
        (0xEE0, 8),
    ])
}
pub(crate) fn uniform_address(table: u64, count: usize, index: usize) -> Result<u64> {
    ensure!(index < count, "Uniform index is outside its directory");
    table
        .checked_add((index * profile::UNIFORM_STRIDE) as u64)
        .context("Uniform address overflow")
}
pub(crate) fn uniform_index(table: u64, count: usize, address: u64) -> Option<usize> {
    let offset = address.checked_sub(table)?;
    let stride = profile::UNIFORM_STRIDE as u64;
    (offset % stride == 0 && offset / stride < count as u64).then_some((offset / stride) as usize)
}
pub(crate) fn plan_uniform(
    input: &Value,
    cached_team: &Value,
    cached_uniform: &Value,
    team_raw: &[u8],
    uniform_raw: &[u8],
    address: u64,
    index: usize,
) -> Result<(uniform_changes::Edit, Plan)> {
    let edit = uniform_changes::validate(input)?;
    ensure!(
        cached_team["source"] == "live-team",
        "The selected team no longer matches this editor snapshot"
    );
    let p = uniform_changes::plan(
        &edit,
        cached_team,
        cached_uniform,
        team_raw,
        uniform_raw,
        address,
    )?;
    let mut guards = vec![(8, 64), (0x50, 4)];
    guards.extend(p.patches.iter().map(|q| (q.offset, 4)));
    let plan = Plan {
        kind: Kind::Uniform,
        index,
        before: p.before,
        after: p.after,
        patches: p
            .patches
            .into_iter()
            .map(|q| Patch {
                offset: q.offset,
                before: q.before,
                after: q.after,
                mask: q.mask,
            })
            .collect(),
        guards,
        changed: p.changed_controls,
    };
    ensure!(
        plan.expected(plan.patches.len())? == plan.after,
        "Incomplete uniform plan"
    );
    Ok((edit, plan))
}

/// WPF validateTeamUniformLinkInput().
pub(crate) struct UniformLink {
    pub record_index: usize,
    pub team_id: u16,
    pub expected_team_name: String,
    pub slot: usize,
    pub uniform_record_index: usize,
    pub uniform_id: u16,
    pub expected_base_filename: String,
}
pub(crate) fn validate_uniform_link(v: &Value) -> Result<UniformLink> {
    let integer = |key: &str, max: u64| -> Result<u64> {
        let n = match &v[key] {
            Value::Number(n) => n.as_f64(),
            Value::String(s) if !s.trim().is_empty() => s.trim().parse::<f64>().ok(),
            _ => None,
        };
        n.filter(|n| n.is_finite() && n.fract() == 0. && *n >= 0. && *n <= max as f64)
            .map(|n| n as u64)
            .with_context(|| format!("{key} must be a whole number from 0 to {max}."))
    };
    let text = |key: &str, max: usize, label: &str| -> Result<String> {
        let s = match &v[key] {
            Value::Null => String::new(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let s = s.trim_matches(player_changes::js_space).to_owned();
        ensure!(
            !s.is_empty() && s.encode_utf16().count() <= max && !s.chars().any(|c| c <= '\u{1f}'),
            "{label} must contain 1–{max} valid characters."
        );
        Ok(s)
    };
    Ok(UniformLink {
        record_index: integer("recordIndex", 1023)? as usize,
        team_id: integer("teamId", 65535)? as u16,
        expected_team_name: text("expectedTeamName", 23, "Expected team name")?,
        slot: integer("slot", 31)? as usize,
        uniform_record_index: integer("uniformRecordIndex", 8191)? as usize,
        uniform_id: integer("uniformId", 65535)? as u16,
        expected_base_filename: text("expectedBaseFilename", 63, "Uniform filename")?,
    })
}
/// WPF linkTeamUniform + RunTeamUniformLinkTest(--link-team-uniform): write
/// exactly one 8-byte pointer into an empty slot of this team. `referenced`
/// must come from a live scan of every team slot array.
pub(crate) fn plan_uniform_link(
    edit: &UniformLink,
    cached_team: &Value,
    candidate: &Value,
    team_raw: &[u8],
    uniform_raw: &[u8],
    address: u64,
    referenced: bool,
    stock_art: bool,
) -> Result<Plan> {
    ensure!(
        team_raw.len() == profile::TEAM_STRIDE && uniform_raw.len() == profile::UNIFORM_STRIDE,
        "Incomplete uniform link buffers"
    );
    ensure!(
        cached_team["source"] == "live-team"
            && cached_team["recordIndex"].as_u64() == Some(edit.record_index as u64)
            && cached_team["teamId"].as_u64() == Some(edit.team_id as u64),
        "That team is retained from another roster. Load its roster and refresh before adding a uniform."
    );
    ensure!(
        cached_team["name"] == edit.expected_team_name.as_str(),
        "The selected team changed in the launcher. Refresh and try again."
    );
    ensure!(
        cached_team["emptyUniformSlots"]
            .as_array()
            .is_some_and(|s| s.contains(&json!(edit.slot))),
        "The selected team slot is no longer empty. Refresh and try again."
    );
    ensure!(
        candidate["recordIndex"].as_u64() == Some(edit.uniform_record_index as u64)
            && candidate["uniformId"].as_u64() == Some(edit.uniform_id as u64)
            && candidate["teamId"].as_u64() == Some(edit.team_id as u64)
            && candidate["baseFilename"] == edit.expected_base_filename.as_str(),
        "That uniform record is no longer an unreferenced same-team candidate. Refresh and try again."
    );
    ensure!(
        stock_art,
        "The stock art resource for {} is unavailable; the jersey was not linked.",
        edit.expected_base_filename
    );
    ensure!(
        get(team_raw, (TEAM_ID, 0, 16))? == edit.team_id as u64
            && utf16(team_raw, 0x2FA, 48)? == edit.expected_team_name,
        "The selected team changed before the link; refresh and try again."
    );
    let at = UNIFORM_SLOTS + edit.slot * 8;
    ensure!(
        team_raw[at..at + 8] == [0; 8],
        "The selected uniform slot is no longer empty."
    );
    ensure!(
        get(uniform_raw, (0x50, 0, 16))? == edit.uniform_id as u64
            && get(uniform_raw, (0x52, 0, 16))? == edit.team_id as u64
            && uniform_changes::ascii(uniform_raw, 8, 64)? == edit.expected_base_filename,
        "The candidate uniform changed before the link; refresh and try again."
    );
    ensure!(
        !referenced,
        "The candidate uniform is already referenced by another team slot."
    );
    roster_snapshot::checked_pointer(address)?;
    let mut b = Builder::new(Kind::Team, edit.record_index, team_raw)?;
    b.bytes(at, &address.to_le_bytes(), "uniformSlot")?;
    b.finish(&[(TEAM_ID, 2), (0x2FA, 48)])
}

// ---------------------------------------------------------- ownership ----
/// Stable identity bytes that no allowed patch can change.
pub(crate) fn identity(kind: Kind, row: &[u8]) -> Result<String> {
    ensure!(row.len() == kind.stride(), "Incomplete record buffer");
    Ok(match kind {
        Kind::Player => format!("player:{}", get(row, (0x128, 0, 16))?),
        Kind::Team => format!("team:{}", get(row, (TEAM_ID, 0, 16))?),
        Kind::Uniform => format!(
            "uniform:{}:{}:{}",
            get(row, (0x50, 0, 16))?,
            get(row, (0x52, 0, 16))?,
            uniform_changes::ascii(row, 8, 64)?
        ),
        // Unique id and owning-team link are outside every staff mask.
        Kind::Staff => format!(
            "staff:{}:{:x}",
            get(row, (0x120, 0, 16))?,
            u64::from_le_bytes(row[0x18..0x20].try_into()?)
        ),
        // Team pointers and year are outside the season masks.
        Kind::Season => format!(
            "season:{}:{:x}:{:x}",
            row[0x11],
            u64::from_le_bytes(row[0..8].try_into()?),
            u64::from_le_bytes(row[8..16].try_into()?)
        ),
    })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Owner {
    pub game: Identity,
    pub session: String,
    pub image: u64,
    pub root: u64,
    pub owner: u64,
    pub region_base: u64,
    pub region_len: u64,
    pub kind: Kind,
    pub table: u64,
    pub count: usize,
    pub index: usize,
    pub identity: String,
    pub proof: String,
}
impl Owner {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.game.pid > 0
                && self.game.started.parse::<u64>().is_ok_and(|n| n > 0)
                && Path::new(&self.game.path).is_absolute()
                && Path::new(&self.game.path)
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe"))
                && self.session.len() == 48
                && self.session.bytes().all(|b| b.is_ascii_hexdigit()),
            "Unsupported Version 2 roster transaction identity"
        );
        for pointer in [self.image, self.root, self.owner, self.region_base] {
            roster_snapshot::checked_pointer(pointer)?;
        }
        let stride = self.kind.stride() as u64;
        ensure!(
            self.region_base % 0x1000 == 0
                && (0x80_0000..=0x800_0000).contains(&self.region_len)
                && self.table >= self.region_base + 0x680
                && self.kind.counts().contains(&self.count)
                && self.index < self.count
                && (self.count as u64)
                    .checked_mul(stride)
                    .and_then(|n| self.table.checked_add(n))
                    .is_some_and(|end| end <= self.region_base + self.region_len),
            "Unsupported Version 2 roster directory"
        );
        ensure!(
            !self.identity.is_empty()
                && self.identity.len() <= 160
                && !self.identity.chars().any(char::is_control)
                && self.proof == proof_tag(),
            "Unsupported Version 2 roster layout"
        );
        Ok(())
    }
}
pub(crate) trait Access {
    /// One-shot independently bounded retained-identity recovery; never forward permission.
    fn begin_recovery(&self) -> Result<()> {
        bail!("This adapter cannot enter bounded recovery")
    }
    fn owner(&self) -> Result<Owner>;
    fn capture(&self) -> Result<Vec<u8>>;
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<()>;
    /// Cancellation stops new work here. The other methods stay usable for
    /// bounded guarded rollback on the retained lease.
    fn checkpoint(&self) -> Result<()>;
}

// -------------------------------------------------------------- journal ----
pub(crate) fn allowed(kind: Kind) -> Result<Vec<u8>> {
    match kind {
        Kind::Staff => return Ok(crate::roster_v2_services::staff_masks().to_vec()),
        Kind::Season => return Ok(crate::roster_v2_services::season_masks()),
        _ => {}
    }
    let mut masks = vec![0u8; kind.stride()];
    let mut bits = |(offset, shift, width): Bits| {
        for i in 0..width as usize {
            let g = offset * 8 + shift as usize + i;
            masks[g / 8] |= 1 << (g % 8);
        }
    };
    match kind {
        Kind::Player => {
            for key in [
                "GraphicId",
                "Position",
                "SecondaryPosition",
                "JerseyNumber",
                "BirthYear",
                "BirthMonth",
                "BirthDay",
                "YearsPro",
            ] {
                bits(bio(key)?);
            }
            let p = rating("PotentialRaw")?;
            bits((p.offset, p.shift, p.bits));
            for f in roster_snapshot::rating_fields()
                .iter()
                .filter(|f| f.maximum >= 0)
            {
                bits((f.offset, f.shift, f.bits));
            }
            masks[..80].fill(255);
            masks[WEIGHT..WEIGHT + 4].fill(255);
            crate::roster_v2_origins::proof()?;
            masks[0xB0..0xB8].fill(255);
            audio_proof()?;
            masks[FULL_NAME_SID..FULL_NAME_SID + 2].fill(255);
        }
        Kind::Team => {
            team_metadata_proof()?;
            crate::roster_v2_arena::proof()?;
            bits(TEAM_TYPE_BITS);
            bits(TEAM_CLASSIC_YEAR_BITS);
            for (_, offset, len, _) in TEAM_TEXT {
                masks[offset..offset + len].fill(255);
            }
            // Existing arena pointer only; typed native catalog selection, never an Arena record.
            masks[0xEE0..0xEE8].fill(255);
            // Uniform slot pointers: only plan_uniform_link writes one, 0 -> owned row.
            masks[UNIFORM_SLOTS..UNIFORM_SLOTS + UNIFORM_SLOT_COUNT * 8].fill(255);
            // Advanced rotation cells (48 x 5 u16 UniqueIds); only plan_rotations.
            masks[0x1360..0x1540].fill(255);
        }
        Kind::Staff | Kind::Season => unreachable!(),
        Kind::Uniform => {
            for (offset, mask) in uniform_changes::masks() {
                for (i, b) in mask.to_le_bytes().into_iter().enumerate() {
                    masks[offset + i] |= b;
                }
            }
        }
    }
    Ok(masks)
}
fn validate_patches(owner: &Owner, patches: &[Patch]) -> Result<()> {
    owner.validate()?;
    ensure!(patches.len() <= 1280, "Too many record patches");
    let masks = allowed(owner.kind)?;
    let mut seen = HashSet::new();
    for p in patches {
        ensure!(
            !p.before.is_empty()
                && p.before.len() <= owner.kind.span_limit()
                && p.before.len() == p.after.len()
                && p.mask.len() == p.after.len()
                && p.before != p.after
                && p.offset
                    .checked_add(p.after.len())
                    .is_some_and(|end| end <= masks.len()),
            "Invalid record patch range"
        );
        for i in 0..p.after.len() {
            ensure!(
                seen.insert(p.offset + i)
                    && p.mask[i] & !masks[p.offset + i] == 0
                    && (p.before[i] ^ p.after[i]) & !p.mask[i] == 0,
                "Unowned, overlapping or out-of-mask record patch"
            );
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Intent {
    pub schema: String,
    pub id: String,
    pub owner: Owner,
    pub patches: Vec<Patch>,
    pub pending: bool,
    pub result: String,
}
impl Intent {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == INTENT_SCHEMA
                && self.id.len() == 48
                && self.id.bytes().all(|b| b.is_ascii_hexdigit())
                && self.result.len() <= 512,
            "Unsupported Version 2 roster journal; left untouched"
        );
        validate_patches(&self.owner, &self.patches)
    }
    fn validate_stored(&self) -> Result<()> {
        if self.owner.proof == proof_tag() {
            return self.validate();
        }
        ensure!(
            self.owner.proof == QA082_JOURNAL_PROOF,
            "Unknown legacy roster journal proof; left untouched"
        );
        // Validate all identity, bounds, ID, span and structural requirements.
        // The temporary tag never escapes this inspection and grants no lease.
        let mut inspection = self.clone();
        inspection.owner.proof = proof_tag().into();
        inspection.validate()?;
        if self.owner.kind == Kind::Player {
            for patch in &self.patches {
                for (i, mask) in patch.mask.iter().enumerate() {
                    if (FULL_NAME_SID..FULL_NAME_SID + 2).contains(&(patch.offset + i))
                        || (0xB0..0xB8).contains(&(patch.offset + i))
                    {
                        ensure!(
                            *mask == 0,
                            "Legacy journal cannot own new audio/origin fields"
                        );
                    }
                }
            }
        }
        if self.owner.kind == Kind::Team {
            for patch in &self.patches {
                for (i, mask) in patch.mask.iter().enumerate() {
                    let forbidden = match patch.offset + i {
                        0x123F => 0xFC,
                        0x154A => 0xF8,
                        0x154B => 0x07,
                        0xEE0..=0xEE7 => 0xFF,
                        _ => 0,
                    };
                    ensure!(
                        *mask & forbidden == 0,
                        "Legacy journal cannot own new native team metadata"
                    );
                }
            }
        }
        Ok(())
    }
}
pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("Data/roster-v2-write-journal.json")
}
fn read_intent(path: &Path) -> Result<Intent> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= JOURNAL_LIMIT,
        "Version 2 roster journal exceeds limits; left untouched"
    );
    let i: Intent = serde_json::from_slice(&bytes)?;
    i.validate_stored()?;
    Ok(i)
}
pub(crate) fn pending(root: &Path) -> Result<bool> {
    let p = path(root);
    if !p.try_exists()? {
        return Ok(false);
    }
    Ok(read_intent(&p)?.pending)
}
pub(crate) fn save(root: &Path, intent: &Intent) -> Result<()> {
    intent.validate()?;
    ensure!(
        serde_json::to_vec_pretty(intent)?.len() as u64 <= JOURNAL_LIMIT,
        "Version 2 roster journal exceeds limits before write"
    );
    backend::atomic_json(&path(root), intent)
}
pub(crate) fn resolve_ended(root: &Path, rows: &[session_safety::Process]) -> Result<()> {
    ensure!(
        !rows
            .iter()
            .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")),
        "Close NBA 2K27 before resolving a Version 2 roster transaction"
    );
    crate::roster_v2_arena_clone::resolve_ended(root, rows)?;
    crate::roster_v2_appearance::journal::resolve_ended(root, rows)?;
    crate::roster_v2_access::resolve_structural_ended(root, rows)?;
    let p = path(root);
    if p.try_exists()? {
        let mut i = read_intent(&p)?;
        if i.pending || i.owner.proof == QA082_JOURNAL_PROOF {
            i.pending = false;
            i.result = "game-ended; memory changes cannot be replayed".into();
            // Stored evidence was validated above; only the closed-game path
            // migrates its receipt tag. No memory bytes are adopted or replayed.
            i.owner.proof = proof_tag().into();
            save(root, &i)?;
        }
    }
    Ok(())
}
/// Every native roster journal, legacy and Version 2. Any pending record
/// blocks all further roster writes until the game ends and it is resolved.
pub(crate) fn any_pending(root: &Path) -> Result<bool> {
    Ok(pending(root)?
        || crate::roster_v2_arena_clone::pending(root)?
        || crate::roster_v2_appearance::journal::pending(root)?
        || crate::roster_v2_access::structural_pending(root)?
        || crate::player_transaction::pending(root)?
        || crate::team_transaction::pending(root)?
        || crate::uniform_transaction::pending(root)?
        || crate::game_rules::pending(root)?)
}

// ---------------------------------------------------------- transaction ----
fn check(m: &impl Access, owner: &Owner, plan: &Plan, expected: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        m.owner()? == *owner,
        "Roster database or process ownership changed"
    );
    let raw = m.capture()?;
    ensure!(
        plan.matches(&raw, expected)?,
        "Record values or unchanged neighboring bytes changed"
    );
    Ok(raw)
}
pub(crate) struct Commit {
    pub report: Value,
    pub read_back: Vec<u8>,
}
/// Persist before writing, verify every step, and reverse only exact owned
/// states. Partial or foreign values are held, never overwritten or retried.
pub(crate) fn apply(
    m: &impl Access,
    owner: &Owner,
    plan: &Plan,
    mut persist: impl FnMut(&Intent) -> Result<()>,
) -> Result<Commit> {
    validate_patches(owner, &plan.patches)?;
    ensure!(
        owner.kind == plan.kind
            && owner.index == plan.index
            && plan.before.len() == plan.kind.stride()
            && plan.after.len() == plan.kind.stride()
            && identity(plan.kind, &plan.before)? == owner.identity
            && identity(plan.kind, &plan.after)? == owner.identity
            && plan.expected(plan.patches.len())? == plan.after,
        "Write plan belongs to another record or is incomplete"
    );
    m.checkpoint()?;
    let original = check(m, owner, plan, &plan.before)?;
    let report = |writes: usize| {
        json!({"schema":"nba2k27-v2-roster-edit-v1","ok":true,"kind":plan.kind,"gameCodeHooks":0,"writes":writes,
            "readBackVerified":true,"rollbackOnFailure":true,"fields":plan.changed,"noChanges":writes==0})
    };
    if plan.patches.is_empty() {
        return Ok(Commit {
            report: report(0),
            read_back: original,
        });
    }
    let mut intent = Intent {
        schema: INTENT_SCHEMA.into(),
        id: backend::unique_id(),
        owner: owner.clone(),
        patches: plan.patches.clone(),
        pending: true,
        result: "intent saved before write".into(),
    };
    intent.validate()?;
    persist(&intent)?;
    let mut attempted = 0;
    let outcome = (|| -> Result<Vec<u8>> {
        for (index, p) in plan.patches.iter().enumerate() {
            m.checkpoint()?;
            check(m, owner, plan, &plan.expected(index)?)?;
            attempted = index + 1; // A reported failure may nevertheless have changed bytes.
            m.write(p.offset, &p.after)?;
            check(m, owner, plan, &plan.expected(index + 1)?)?;
        }
        let read_back = check(m, owner, plan, &plan.after)?;
        ensure!(
            read_back == plan.after,
            "Read-back differs from the planned record"
        );
        intent.pending = false;
        intent.result = "applied and read back verified".into();
        persist(&intent)?;
        Ok(read_back)
    })();
    match outcome {
        Ok(read_back) => Ok(Commit {
            report: report(plan.patches.len()),
            read_back,
        }),
        Err(error) => {
            let rollback = (|| -> Result<()> {
                m.begin_recovery()?;
                for index in (0..attempted).rev() {
                    ensure!(m.owner()? == *owner, "Rollback owner changed");
                    let current = m.capture()?;
                    let before = plan.expected(index)?;
                    let after = plan.expected(index + 1)?;
                    if plan.matches(&current, &before)? {
                        continue;
                    }
                    ensure!(
                        plan.matches(&current, &after)?,
                        "Partial or foreign values prevent safe rollback"
                    );
                    let p = &plan.patches[index];
                    m.write(p.offset, &p.before)?;
                    check(m, owner, plan, &before)?;
                }
                Ok(())
            })();
            intent.pending = rollback.is_err();
            intent.result = if rollback.is_ok() {
                "failed; original owned values verified"
            } else {
                "failed; recovery held for changed owner or foreign/partial values"
            }
            .into();
            let recorded = persist(&intent);
            bail!(
                "{error:#}. {}{}",
                if rollback.is_ok() {
                    "All attempted changes are verified rolled back."
                } else {
                    "Some changes may remain. Do not retry; close NBA 2K27 before clearing the recovery record."
                },
                if recorded.is_err() {
                    " Recovery record could not be completed; further writes must remain held."
                } else {
                    ""
                }
            )
        }
    }
}

#[cfg(test)]
#[path = "roster_v2_write_tests.rs"]
mod tests;
