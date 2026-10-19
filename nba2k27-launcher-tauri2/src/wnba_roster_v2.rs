//! Version 2 mixed NBA + WNBA Freestyle roster, built locally from the user's
//! own installed game files (artifacts/wnba-merge-20261017.md).
//!
//! WPF parity: the WPF-era stock `roster/learn2k_roster.iff` was already a
//! 403-row mixed roster that Freestyle loaded by default; WPF then wrote only
//! the four category-order DWORDs (freestyle_v2.rs). Version 2 ships the NBA
//! and WNBA Learn2K rosters separately, so this module rebuilds the WPF-era
//! roster from them (Rust port of tools/roster_merge_wnba.py) and the provider
//! serves it at `roster/learn2k_roster.iff` through a forced OVERRIDE slot.
//!
//! Integrity: both stock inputs are pinned by SHA-256 of their packed archive
//! entries, every bundle CRC is checked with the executable's salt, and the
//! merged output must equal the offline-verified SHA-256 for its variant.
//! The VCBLOWFISH key is captured read-only from the running game and kept
//! only in `Portable\Data\wnba-roster-v2\` (never logged, bundled or shipped).
use crate::blacktop_native::Memory;
use crate::rdat_roster::{
    self as rdat, NULL_REF, REF_TAG, Roster, field_by_name, get, put, ref_slots, wide,
};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub(crate) const PATH: &str = "roster/learn2k_roster.iff";
const WNBA_PATH: &str = "roster/learn2k_wnba_roster.iff";
const PE_STAMP: u32 = 0x6AC5_3765;
const PE_IMAGE: u32 = 0x35BD_E000;
/// Roster wrapper manager (setter 0x3C78030 copies the key to +0x58 and
/// publishes pointer/length at +0xD8/+0xE0).
const MANAGER_RVA: u64 = 0xBB9_A8E8;
const SALT_RVA: u32 = 0x73B_5180;
const SALT_LEN: usize = 22;
const SALT_CRC: u32 = 0xB300_5D98; // crc32(salt) == the stock empty-table CRC
const STOCK_NBA_SHA256: &str = "69445e272c0c02f6905a99bc0926e2d76e331ab7925f3621b799ac33fd6c28c2";
const STOCK_WNBA_SHA256: &str = "3c021a015880372a3a66dd499388d08b5c079186edde494e1bb0f68f779e4452";
const FOLDER: &str = "wnba-roster-v2";
const KEY_FILE: &str = "roster-key.bin";

// Table ids (directory hashes) and field name hashes of the V2 schema.
const TEAM: u32 = 0xE34F_AD55;
const PLAYER: u32 = 0x72D6_0754;
const STAFF: u32 = 0x4C57_5CCD;
const HEADER_TABLE: u32 = 0xCEF8_4E6A;
const PERSON_TABLES: [u32; 2] = [0xC54B_EB40, 0x66BF_BD42];
const TEAM_TYPES: [u32; 2] = [0x2CEC_F817, 0x0C73_2B9B];
const TEAM_NAME: u32 = 0x292A_0287;
const TEAM_ID: u32 = 0x2ABD_43F2;
const PERSON_ID: u32 = 0x31BD_8AF6;
const PLAYER_NAMES: [u32; 2] = [0x51D5_5A69, 0xF45F_B0F8];
const PLAYER_AFFILIATION: [u32; 7] = [
    0x8EBB_AB92,
    0x29EF_21A7,
    0x028C_93E4,
    0x524E_6767,
    0x2549_57F1,
    0xBB2D_C252,
    0x5EE8_ECF8,
];
const HEADER_SIZE_FIELD: u32 = 0x3187_5000;
const STRIDES: [(u32, usize); 5] = [
    (TEAM, 0x1550),
    (PLAYER, 0x4F8),
    (STAFF, 0x1C8),
    (0xC54B_EB40, 0x3C),
    (0x66BF_BD42, 0x68),
];
const WNBA_TYPE: u64 = 29;
const NATIONAL_TYPE: u64 = 22;
const NATIONAL_IDS: [u64; 2] = [69, 361];
const WPF_INSERT_AFTER_ID: u64 = 477;
const PLACEHOLDER_BASE: u64 = 64000;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Variant {
    /// The WPF-era 403-row structure (default).
    WpfEra,
    /// 15 clubs + WNBA USA appended; passes the launcher's mixed verifier.
    National,
    /// 15 clubs only; no new players. Diagnostic: WPF's verifier needs USA 361.
    Clubs,
}
impl Variant {
    pub(crate) fn parse(name: &str) -> Result<Self> {
        Ok(match name {
            "wpf-era" | "" => Self::WpfEra,
            "national" => Self::National,
            "clubs" => Self::Clubs,
            _ => bail!("Unknown WNBA roster variant"),
        })
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::WpfEra => "wpf-era",
            Self::National => "national",
            Self::Clubs => "clubs",
        }
    }
    /// Offline-verified decoded SHA-256 (tools/roster_merge_wnba.py, 2026-10-17).
    fn expected_sha256(self) -> &'static str {
        match self {
            Self::WpfEra => "8827184c6bf3ec8d7a833db8f71880ee9e9af6c35609d26ae2d0c2559ed89fe5",
            Self::National => "e3d944f4e2876f99f2cedd93a4d3f586f1e29df0a544e74d16e381efe176f95a",
            Self::Clubs => "1530bbec733b8d2437616aad5e6ca34c81fdfbcdabac6e9c2523c9ab02f55b86",
        }
    }
    /// Whether the launcher's strict mixed-roster verifier can pass.
    pub(crate) fn exposes_categories(self) -> bool {
        self != Self::Clubs
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn folder(portable: &Path) -> PathBuf {
    portable.join("Data").join(FOLDER)
}

// ---------------------------------------------------------------- key ----

/// Read-only capture of the roster wrapper key from the verified V2 image.
pub(crate) fn capture_key(m: &impl Memory) -> Result<Vec<u8>> {
    m.checkpoint()?;
    let h = m.read(m.base(), 4096)?;
    ensure!(
        h.len() == 4096 && h[..2] == *b"MZ",
        "Invalid game image header"
    );
    let pe = u32::from_le_bytes(h[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe)
            && h[pe..pe + 4] == *b"PE\0\0"
            && u32::from_le_bytes(h[pe + 8..pe + 12].try_into()?) == PE_STAMP
            && u32::from_le_bytes(h[pe + 80..pe + 84].try_into()?) == PE_IMAGE,
        "Roster key capture requires the verified Version 2 image"
    );
    let manager = m.base() + MANAGER_RVA;
    let fields = m.read(manager + 0xD8, 16)?;
    ensure!(fields.len() == 16, "Roster key manager is not readable");
    let pointer = u64::from_le_bytes(fields[..8].try_into()?);
    let length = u64::from_le_bytes(fields[8..].try_into()?) as usize;
    ensure!(
        pointer == manager + 0x58 && (8..=72).contains(&length),
        "Roster key is not initialized yet"
    );
    let key = m.read(pointer, length)?;
    ensure!(
        key.len() == length && key.iter().any(|b| *b != 0),
        "Roster key is not initialized yet"
    );
    m.checkpoint()?;
    Ok(key)
}
pub(crate) fn load_key(portable: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(folder(portable).join(KEY_FILE)).ok()?;
    (8..=72).contains(&bytes.len()).then_some(bytes)
}
/// Saved only after it decrypted both pinned stock rosters with verified CRCs.
fn save_key(portable: &Path, key: &[u8]) -> Result<()> {
    let dir = folder(portable);
    fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{KEY_FILE}.tmp"));
    fs::write(&tmp, key)?;
    fs::rename(&tmp, dir.join(KEY_FILE))?;
    Ok(())
}

// ------------------------------------------------------- stock inputs ----

fn open_plain(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x0020_0000)
        .open(path)
        .with_context(|| format!("Opening {}", path.display()))?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0,
        "Installed game file is not an ordinary file"
    );
    Ok(file)
}
/// Packed archive entries for `names`, resolved through the V2 archive index
/// `0A` (record: u64 key = archive_key(path), u32 decoded size, u32 packed
/// size << 1, u64 position << 23 across the sorted archives). Each
/// key must be unique; the caller pins the bytes by SHA-256.
fn read_indexed<const N: usize>(
    game: &Path,
    names: [&str; N],
    check: &impl Fn() -> Result<()>,
) -> Result<[Vec<u8>; N]> {
    const MAX_INDEX: u64 = 128 * 1024 * 1024;
    let _pins = crate::loader_files::Directories::pin(game)?;
    let mut index = open_plain(&game.join("0A"))?;
    let before = index.metadata()?;
    ensure!(before.len() <= MAX_INDEX, "Archive index exceeds bounds");
    let mut bytes = Vec::with_capacity(before.len() as usize);
    index.by_ref().take(MAX_INDEX + 1).read_to_end(&mut bytes)?;
    ensure!(
        index.metadata()?.len() == before.len(),
        "Archive index changed during read"
    );
    check()?;
    let mut archives: Vec<PathBuf> = fs::read_dir(game)?
        .filter_map(|r| r.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.len() == 2 && s.as_bytes()[0].is_ascii_digit() && s.as_bytes()[1].is_ascii_uppercase()
        })
        .map(|e| e.path())
        .collect();
    archives.sort();
    let mut out: [Vec<u8>; N] = std::array::from_fn(|_| Vec::new());
    for (slot, name) in names.iter().enumerate() {
        let needle = crate::jukebox_v2_resources::archive_key(name).to_le_bytes();
        let mut hits = bytes
            .windows(8)
            .enumerate()
            .filter_map(|(i, w)| (w == needle).then_some(i));
        let at = hits
            .next()
            .with_context(|| format!("{name} is absent from the archive index"))?;
        ensure!(
            hits.next().is_none() && at + 24 <= bytes.len(),
            "Ambiguous or truncated archive index entry for {name}"
        );
        // +8 is the decoded size; +12 holds packed size << 1 (verified on the
        // seven entries in roster-scratch/asset-provenance.json).
        let doubled = u32::from_le_bytes(bytes[at + 12..at + 16].try_into()?);
        ensure!(
            doubled & 1 == 0,
            "Unexpected archive index flags for {name}"
        );
        let size = (doubled >> 1) as usize;
        let position = u64::from_le_bytes(bytes[at + 16..at + 24].try_into()?) >> 23;
        ensure!(
            (24..=crate::sigconfig::MAX_BYTES).contains(&size),
            "Archive entry {name} exceeds bounds"
        );
        let mut start = 0u64;
        for path in &archives {
            let file = open_plain(path)?;
            let meta = file.metadata()?;
            let end = start
                .checked_add(meta.len())
                .context("Archive range overflow")?;
            if position >= start && position + size as u64 <= end {
                use std::os::windows::fs::FileExt;
                let mut payload = vec![0u8; size];
                let mut read = 0;
                while read < size {
                    check()?;
                    let n = file.seek_read(&mut payload[read..], position - start + read as u64)?;
                    ensure!(n > 0, "Archive entry {name} ended unexpectedly");
                    read += n;
                }
                ensure!(
                    file.metadata()?.len() == meta.len(),
                    "Archive changed during read"
                );
                out[slot] = payload;
                break;
            }
            start = end;
        }
        ensure!(
            !out[slot].is_empty(),
            "Archive entry {name} is not contained in one ordinary archive"
        );
    }
    Ok(out)
}

type Decode = unsafe extern "C" fn(
    *const u8,
    i64,
    *mut u8,
    i64,
    i32,
    i32,
    i32,
    *mut u8,
    i64,
    *mut u8,
    *mut u8,
    *mut u8,
    i64,
    i32,
) -> i64;
struct Oodle {
    _library: crate::trusted_oodle::Library,
    decode: Decode,
}
impl Oodle {
    fn load(game: &Path) -> Result<Self> {
        let library = crate::trusted_oodle::load(game, crate::trusted_oodle::Runtime::Core)?;
        let decode = *unsafe { library.get::<Decode>(b"OodleLZ_Decompress\0") }?;
        Ok(Self {
            _library: library,
            decode,
        })
    }
}
/// VCZ 0x84 wrapper: Blowfish-ECB payload, Oodle stream, authored length/CRC.
fn decode(packed: &[u8], key: &[u8], oodle: &Oodle) -> Result<Vec<u8>> {
    ensure!(
        packed.len() > 24
            && packed[..4] == [0x1F, 0x8B, 0x21, 0x84]
            && packed[10..16] == *b"\x04\0VCZ\0"
            && (packed.len() - 24) % 8 == 0,
        "Stock roster is not an encrypted VCZ wrapper"
    );
    let end = packed.len();
    let crc = u32::from_le_bytes(packed[end - 8..end - 4].try_into()?);
    let size = u32::from_le_bytes(packed[end - 4..].try_into()?) as usize;
    ensure!(
        (64..=64 * 1024 * 1024).contains(&size),
        "Stock roster size exceeds bounds"
    );
    let mut payload = packed[16..end - 8].to_vec();
    crate::vcblowfish::Blowfish::new(key)?.decrypt(&mut payload)?;
    let mut out = vec![0u8; size];
    let count = unsafe {
        (oodle.decode)(
            payload.as_ptr(),
            payload.len() as i64,
            out.as_mut_ptr(),
            out.len() as i64,
            1,
            0,
            0,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            3,
        )
    };
    ensure!(
        count == size as i64 && crc32fast::hash(&out) == crc,
        "Stock roster did not decrypt to its authored length/CRC (wrong or stale key)"
    );
    Ok(out)
}
/// 22-byte bundle CRC salt from the installed executable (read-only).
fn read_salt(exe: &Path) -> Result<Vec<u8>> {
    let mut file = open_plain(exe)?;
    let mut head = vec![0u8; 4096];
    file.read_exact(&mut head)?;
    let pe = u32::from_le_bytes(head[60..64].try_into()?) as usize;
    ensure!(
        (64..=3800).contains(&pe)
            && head[pe..pe + 4] == *b"PE\0\0"
            && u32::from_le_bytes(head[pe + 8..pe + 12].try_into()?) == PE_STAMP
            && u32::from_le_bytes(head[pe + 80..pe + 84].try_into()?) == PE_IMAGE,
        "WNBA roster requires the verified Version 2 executable"
    );
    let sections = u16::from_le_bytes(head[pe + 6..pe + 8].try_into()?) as usize;
    let optional = u16::from_le_bytes(head[pe + 20..pe + 22].try_into()?) as usize;
    let table = pe + 24 + optional;
    ensure!(
        table + 40 * sections <= head.len(),
        "Executable section table exceeds bounds"
    );
    for k in 0..sections {
        let s = &head[table + 40 * k..table + 40 * (k + 1)];
        let word = |at: usize| u32::from_le_bytes(s[at..at + 4].try_into().unwrap());
        let (vsize, va, raw_size, raw) = (word(8), word(12), word(16), word(20));
        if va <= SALT_RVA && SALT_RVA + SALT_LEN as u32 <= va + vsize.max(raw_size) {
            use std::os::windows::fs::FileExt;
            let mut salt = vec![0u8; SALT_LEN];
            let n = file.seek_read(&mut salt, (raw + SALT_RVA - va) as u64)?;
            ensure!(
                n == SALT_LEN && crc32fast::hash(&salt) == SALT_CRC,
                "Executable roster salt differs from the verified Version 2 build"
            );
            return Ok(salt);
        }
    }
    bail!("Executable roster salt is not mapped")
}

// -------------------------------------------------------------- merge ----

struct Merge {
    base: Roster,
    src: Roster,
    variant: Variant,
    types: BTreeMap<u32, u32>,
    slots: BTreeMap<u32, Vec<(u32, usize)>>,
    team_map: BTreeMap<usize, usize>,
    appended: BTreeMap<u32, BTreeMap<usize, usize>>,
    appended_rows: BTreeMap<u32, Vec<Vec<u8>>>,
    players_used: BTreeSet<usize>,
    staff_used: BTreeSet<usize>,
    pid: (usize, Vec<(usize, usize)>),
    sid: usize,
}
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Summary {
    pub variant: Variant,
    pub teams: usize,
    pub inserted_at: usize,
    pub teams_added: usize,
    pub nba: usize,
    pub wnba: usize,
    pub national: Vec<u64>,
    pub players: usize,
    pub players_appended: usize,
    pub sha256: String,
}

impl Merge {
    fn rows(&mut self, base: bool, id: u32) -> Result<&[Vec<u8>]> {
        let roster = if base { &mut self.base } else { &mut self.src };
        Ok(roster.table(id)?.all_rows())
    }
    fn target(&self, type_hash: u32) -> Result<u32> {
        self.types
            .get(&type_hash)
            .copied()
            .with_context(|| format!("Reference type {type_hash:08x} has no unique table"))
    }
    fn same_person(&mut self, i: usize) -> Result<bool> {
        let b = self.rows(true, PLAYER)?.get(i).cloned();
        let s = self.rows(false, PLAYER)?.get(i).cloned();
        let (Some(b), Some(s)) = (b, s) else {
            return Ok(false);
        };
        let (off, names) = &self.pid;
        Ok(get(&b, *off, 16) == get(&s, *off, 16)
            && names
                .iter()
                .all(|(o, c)| wide(&b, *o, *c) == wide(&s, *o, *c)))
    }
    fn staff_in_base(&mut self, i: usize) -> Result<bool> {
        let team_slots = self.team_slots(STAFF)?;
        let b = self.rows(true, STAFF)?.get(i).cloned();
        let s = self.rows(false, STAFF)?.get(i).cloned();
        let (Some(mut b), Some(mut s)) = (b, s) else {
            return Ok(false);
        };
        if get(&b, self.sid, 16) != get(&s, self.sid, 16) {
            return Ok(false);
        }
        for off in team_slots {
            if get(&b, off, 64) != NULL_REF {
                return Ok(false);
            }
            put(&mut b, off, 64, 0);
            put(&mut s, off, 64, 0);
        }
        Ok(b == s)
    }
    fn identical(&mut self, id: u32, i: usize) -> Result<bool> {
        if self.base.tables[&id].bits != self.src.tables[&id].bits {
            return Ok(false);
        }
        let b = self.rows(true, id)?.get(i).cloned();
        let s = self.rows(false, id)?.get(i).cloned();
        Ok(matches!((b, s), (Some(b), Some(s)) if b == s))
    }
    fn team_slots(&self, id: u32) -> Result<Vec<usize>> {
        Ok(self.slots[&id]
            .iter()
            .filter(|(t, _)| self.types.get(t) == Some(&TEAM))
            .map(|(_, o)| *o)
            .collect())
    }
    fn append(&mut self, id: u32, idx: usize) -> Result<usize> {
        if let Some(n) = self.appended.get(&id).and_then(|m| m.get(&idx)) {
            return Ok(*n);
        }
        let slot = self.appended_rows.get(&id).map_or(0, Vec::len);
        let new = self.base.tables[&id].rows + slot;
        self.appended.entry(id).or_default().insert(idx, new);
        self.appended_rows.entry(id).or_default().push(Vec::new());
        let row = self.rows(false, id)?[idx].clone();
        let row = self.remap(id, row, true)?;
        self.appended_rows.get_mut(&id).unwrap()[slot] = row;
        Ok(new)
    }
    fn remap(&mut self, id: u32, mut row: Vec<u8>, copied: bool) -> Result<Vec<u8>> {
        for (type_hash, off) in self.slots[&id].clone() {
            let v = get(&row, off, 64);
            if v == NULL_REF {
                continue;
            }
            let (idx, tag) = ((v >> 32) as usize, v as u32);
            ensure!(tag == REF_TAG, "Unexpected roster reference tag");
            let tg = self.target(type_hash)?;
            let new = if tg == TEAM {
                if let Some(n) = self.team_map.get(&idx) {
                    *n
                } else if copied || id == TEAM {
                    put(&mut row, off, 64, NULL_REF);
                    continue;
                } else {
                    bail!("Roster reference to an unmerged team {idx}");
                }
            } else if tg == PLAYER {
                if self.same_person(idx)? {
                    self.players_used.insert(idx);
                    idx
                } else {
                    self.append(PLAYER, idx)?
                }
            } else if tg == STAFF {
                if self.staff_in_base(idx)? {
                    self.staff_used.insert(idx);
                    idx
                } else {
                    self.append(STAFF, idx)?
                }
            } else if self.identical(tg, idx)? {
                idx
            } else if copied && PERSON_TABLES.contains(&tg) {
                self.append(tg, idx)?
            } else {
                bail!("Roster table {tg:08x} row {idx} differs between the two stock rosters");
            };
            put(&mut row, off, 64, ((new as u64) << 32) | tag as u64);
        }
        Ok(row)
    }
}

fn shift_team_refs(rows: &mut [Vec<u8>], slots: &[usize], at: usize, n: usize) {
    for r in rows.iter_mut() {
        for &off in slots {
            let v = get(r, off, 64);
            if v != NULL_REF && (v >> 32) as usize >= at {
                put(
                    r,
                    off,
                    64,
                    ((((v >> 32) as usize + n) as u64) << 32) | (v & 0xFFFF_FFFF),
                );
            }
        }
    }
}

fn fresh_ids(rows: &mut [Vec<u8>], base: &[Vec<u8>], off: usize) {
    let base_ids: BTreeSet<u64> = base.iter().map(|r| get(r, off, 16)).collect();
    let mut used = base_ids.clone();
    let mut next = 0xFFFEu64;
    for r in rows.iter_mut() {
        let old = get(r, off, 16);
        if used.contains(&old) {
            while base_ids.contains(&next) || used.contains(&next) {
                next -= 1;
            }
            put(r, off, 16, next);
            next -= 1;
        }
        used.insert(get(r, off, 16));
    }
}

pub(crate) fn merge(
    base: Roster,
    src: Roster,
    variant: Variant,
    salt: &[u8],
) -> Result<(Vec<u8>, Summary)> {
    let types: BTreeMap<u32, u32> = src
        .type_map()
        .into_iter()
        .filter(|(_, ids)| ids.len() == 1)
        .map(|(t, ids)| (t, ids[0]))
        .collect();
    let mut slots = BTreeMap::new();
    for id in &src.order {
        slots.insert(*id, ref_slots(&src.tables[id])?);
    }
    for id in [TEAM, PLAYER, STAFF] {
        let (b, s) = (&base.tables[&id], &src.tables[&id]);
        let layout = |t: &rdat::Table| -> Result<Vec<(usize, usize, u32, u32, u32)>> {
            Ok(rdat::schema(t)?
                .into_iter()
                .map(|f| (f.offset, f.width, f.kind, f.type_hash, f.name_hash))
                .collect())
        };
        ensure!(
            b.bits == s.bits && layout(b)? == layout(s)?,
            "Roster table {id:08x} schema differs between the two stock rosters"
        );
    }
    let st = &src.tables[&TEAM];
    let type_f: Vec<_> = TEAM_TYPES
        .iter()
        .map(|h| field_by_name(st, *h))
        .collect::<Result<_>>()?;
    let name_f = field_by_name(st, TEAM_NAME)?;
    let id_f = field_by_name(st, TEAM_ID)?;
    let sp = &src.tables[&PLAYER];
    let pid_f = field_by_name(sp, PERSON_ID)?;
    let names: Vec<(usize, usize)> = PLAYER_NAMES
        .iter()
        .map(|h| field_by_name(sp, *h).map(|f| (f.offset, f.count)))
        .collect::<Result<_>>()?;
    let sid = field_by_name(&src.tables[&STAFF], PERSON_ID)?.offset;
    let affil: Vec<_> = PLAYER_AFFILIATION
        .iter()
        .map(|h| field_by_name(sp, *h))
        .collect::<Result<_>>()?;
    let mut m = Merge {
        base,
        src,
        variant,
        types,
        slots,
        team_map: BTreeMap::new(),
        appended: BTreeMap::new(),
        appended_rows: BTreeMap::new(),
        players_used: BTreeSet::new(),
        staff_used: BTreeSet::new(),
        pid: (pid_f.offset, names),
        sid,
    };
    let kind = |r: &[u8]| get(r, type_f[0].offset, type_f[0].width);
    let team_id = |r: &[u8]| get(r, id_f.offset, id_f.width);
    let tname = |r: &[u8]| wide(r, name_f.offset, name_f.count);
    let base_rows = m.rows(true, TEAM)?.to_vec();
    let src_rows = m.rows(false, TEAM)?.to_vec();
    ensure!(
        !base_rows.iter().any(|r| kind(r) == WNBA_TYPE),
        "The base roster already contains WNBA teams"
    );
    let base_by_id: BTreeMap<u64, usize> = base_rows
        .iter()
        .enumerate()
        .map(|(i, r)| (team_id(r), i))
        .collect();
    ensure!(
        base_by_id.len() == base_rows.len(),
        "Base team IDs are not unique"
    );
    let clubs: Vec<usize> = (0..src_rows.len())
        .filter(|&i| {
            type_f
                .iter()
                .all(|f| get(&src_rows[i], f.offset, f.width) == WNBA_TYPE)
        })
        .collect();
    let mut club_ids: Vec<u64> = clubs.iter().map(|&i| team_id(&src_rows[i])).collect();
    club_ids.sort();
    ensure!(
        club_ids == (300..312).chain([315, 316, 317]).collect::<Vec<u64>>(),
        "The WNBA roster does not hold the 15 expected clubs"
    );
    let selected: Vec<usize> = match variant {
        Variant::Clubs => clubs.clone(),
        Variant::National => clubs
            .iter()
            .copied()
            .chain((0..src_rows.len()).filter(|&i| {
                kind(&src_rows[i]) == NATIONAL_TYPE
                    && NATIONAL_IDS.contains(&team_id(&src_rows[i]))
                    && !base_by_id.contains_key(&team_id(&src_rows[i]))
            }))
            .collect(),
        Variant::WpfEra => (0..src_rows.len())
            .filter(|&i| {
                !tname(&src_rows[i]).is_empty() && !base_by_id.contains_key(&team_id(&src_rows[i]))
            })
            .collect(),
    };
    let n = selected.len();
    let at = match variant {
        Variant::WpfEra => {
            base_by_id
                .get(&WPF_INSERT_AFTER_ID)
                .context("WPF-era insertion anchor team is missing")?
                + 1
        }
        _ => base_rows.len(),
    };
    let base_new = |i: usize| if i < at { i } else { i + n };
    for (k, &s) in selected.iter().enumerate() {
        m.team_map.insert(s, at + k);
    }
    for (i, r) in src_rows.iter().enumerate() {
        if !m.team_map.contains_key(&i)
            && let Some(&b) = base_by_id.get(&team_id(r))
        {
            m.team_map.insert(i, base_new(b));
        }
    }
    let mut inserted = Vec::new();
    for &s in &selected {
        inserted.push(m.remap(TEAM, src_rows[s].clone(), false)?);
    }

    // Shift every base team reference in every table by the insertion.
    let mut out = m.base.clone();
    let base_types = out.type_map();
    if n > 0 && at < base_rows.len() {
        for id in out.order.clone() {
            let table = out.table(id)?;
            if table.rows == 0 {
                continue;
            }
            let team_offsets: Vec<usize> = ref_slots(table)?
                .into_iter()
                .filter(|(t, _)| base_types.get(t).is_some_and(|v| v == &[TEAM]))
                .map(|(_, o)| o)
                .collect();
            if team_offsets.is_empty() {
                continue;
            }
            let mut rows = table.rows_vec();
            shift_team_refs(&mut rows, &team_offsets, at, n);
            table.set_rows(rows);
        }
    }
    let mut teams = out.table(TEAM)?.rows_vec();
    teams.splice(at..at, inserted).for_each(drop);
    if variant == Variant::WpfEra {
        for row in teams.iter_mut().skip(at).take(n) {
            if matches!(kind(row), 4 | 5) {
                for f in &type_f {
                    put(row, f.offset, f.width, 33);
                }
            }
        }
    }
    for (i, row) in teams.iter_mut().enumerate() {
        let id = team_id(row);
        if tname(row).is_empty() && id >= PLACEHOLDER_BASE && id != PLACEHOLDER_BASE + i as u64 {
            put(row, id_f.offset, id_f.width, PLACEHOLDER_BASE + i as u64);
        }
    }
    ensure!(
        teams
            .iter()
            .map(|r| team_id(r))
            .collect::<BTreeSet<_>>()
            .len()
            == teams.len(),
        "Merged team IDs are not unique"
    );

    // Affiliate shared players/staff in place.
    let p_team = m.team_slots(PLAYER)?;
    let shifted_players = out.table(PLAYER)?.rows_vec();
    let mut prows = shifted_players.clone();
    for i in m.players_used.clone() {
        let srow = m.rows(false, PLAYER)?[i].clone();
        let mut row = prows[i].clone();
        for &off in &p_team {
            let v = get(&srow, off, 64);
            let Some(&t) = (v != NULL_REF)
                .then(|| m.team_map.get(&((v >> 32) as usize)))
                .flatten()
            else {
                continue;
            };
            let current = get(&row, off, 64);
            if current != NULL_REF {
                ensure!(
                    (current >> 32) as usize == t,
                    "Player {i}: base team reference is not empty"
                );
                continue;
            }
            put(&mut row, off, 64, ((t as u64) << 32) | (v & 0xFFFF_FFFF));
        }
        if row != prows[i] {
            for f in &affil {
                put(&mut row, f.offset, f.width, get(&srow, f.offset, f.width));
            }
        }
        prows[i] = row;
    }
    let s_team = m.team_slots(STAFF)?;
    let mut srows = out.table(STAFF)?.rows_vec();
    for i in m.staff_used.clone() {
        let source = m.rows(false, STAFF)?[i].clone();
        for &off in &s_team {
            let v = get(&source, off, 64);
            if v != NULL_REF
                && let Some(&t) = m.team_map.get(&((v >> 32) as usize))
            {
                put(
                    &mut srows[i],
                    off,
                    64,
                    ((t as u64) << 32) | (v & 0xFFFF_FFFF),
                );
            }
        }
    }
    let mut new_players = m.appended_rows.remove(&PLAYER).unwrap_or_default();
    let mut new_staff = m.appended_rows.remove(&STAFF).unwrap_or_default();
    fresh_ids(&mut new_players, &m.rows(true, PLAYER)?.to_vec(), m.pid.0);
    fresh_ids(&mut new_staff, &m.rows(true, STAFF)?.to_vec(), m.sid);
    let players_appended = new_players.len();
    prows.extend(new_players);
    srows.extend(new_staff);
    out.table(TEAM)?.set_rows(teams);
    out.table(PLAYER)?.set_rows(prows);
    out.table(STAFF)?.set_rows(srows);
    for id in PERSON_TABLES {
        if let Some(extra) = m.appended_rows.remove(&id)
            && !extra.is_empty()
        {
            let mut rows = out.table(id)?.rows_vec();
            rows.extend(extra);
            out.table(id)?.set_rows(rows);
        }
    }
    let size_f = field_by_name(&out.tables[&HEADER_TABLE], HEADER_SIZE_FIELD)?;
    let align16 = |x: usize| (x + 15) & !15;
    let delta: usize = STRIDES
        .iter()
        .map(|(id, stride)| {
            align16(out.tables[id].rows * stride) - align16(m.base.tables[id].rows * stride)
        })
        .sum();
    let header = out.table(HEADER_TABLE)?;
    let mut row = header.rows_vec();
    let old = get(&row[0], size_f.offset, size_f.width);
    put(&mut row[0], size_f.offset, size_f.width, old + delta as u64);
    header.set_rows(row);

    let bytes = out.iff_bytes(salt)?;
    let summary = verify(&bytes, salt, m.variant, at, n, players_appended)?;
    Ok((bytes, summary))
}

/// Independent re-parse: CRCs, every reference in range, counts, and the
/// offline-verified output hash for this variant.
fn verify(
    bytes: &[u8],
    salt: &[u8],
    variant: Variant,
    at: usize,
    n: usize,
    appended: usize,
) -> Result<Summary> {
    let mut r = Roster::parse(bytes)?;
    ensure!(
        r.bad_crcs(salt).is_empty(),
        "Merged roster CRC check failed"
    );
    let types = r.type_map();
    for id in r.order.clone() {
        let t = r.table(id)?;
        if t.rows == 0 {
            continue;
        }
        let slots = ref_slots(t)?;
        let mut targets = Vec::new();
        for (type_hash, off) in slots {
            let target = types
                .get(&type_hash)
                .filter(|v| v.len() == 1)
                .context("Merged roster reference has no unique target")?[0];
            targets.push((target, off));
        }
        let rows = t.rows_vec();
        for (target, off) in targets {
            let limit = r.tables[&target].rows;
            for row in &rows {
                let v = get(row, off, 64);
                ensure!(
                    v == NULL_REF || ((v as u32) == REF_TAG && ((v >> 32) as usize) < limit),
                    "Merged roster reference out of range"
                );
            }
        }
    }
    let team = r.table(TEAM)?.clone();
    let kind_f = field_by_name(&team, TEAM_TYPES[0])?;
    let id_f = field_by_name(&team, TEAM_ID)?;
    let mut team = team;
    let rows = team.rows_vec();
    let count = |k: u64| {
        rows.iter()
            .filter(|x| get(x, kind_f.offset, kind_f.width) == k)
            .count()
    };
    let mut national: Vec<u64> = rows
        .iter()
        .filter(|x| get(x, kind_f.offset, kind_f.width) == NATIONAL_TYPE)
        .map(|x| get(x, id_f.offset, id_f.width))
        .collect();
    national.sort();
    let (nba, wnba) = (count(0), count(WNBA_TYPE));
    ensure!(
        nba == 30 && wnba == 15,
        "Merged roster must hold 30 NBA and 15 WNBA teams"
    );
    if variant == Variant::WpfEra {
        ensure!(rows.len() == 403, "WPF-era roster must hold 403 teams");
    }
    if variant.exposes_categories() {
        ensure!(
            national == NATIONAL_IDS,
            "Merged roster must hold both national teams"
        );
    }
    let sha = sha256_hex(bytes);
    ensure!(
        sha == variant.expected_sha256(),
        "Merged roster differs from the offline-verified {} build; not served",
        variant.name()
    );
    Ok(Summary {
        variant,
        teams: rows.len(),
        inserted_at: at,
        teams_added: n,
        nba,
        wnba,
        national,
        players: r.tables[&PLAYER].rows,
        players_appended: appended,
        sha256: sha,
    })
}

// ---------------------------------------------------------- prepare ----

pub(crate) struct Prepared {
    pub bytes: Vec<u8>,
    pub summary: Summary,
}

fn cache_path(portable: &Path, variant: Variant) -> PathBuf {
    folder(portable).join(format!("learn2k_roster.{}.iff", variant.name()))
}
/// A cached build is accepted only when it still hashes to the pinned output.
pub(crate) fn cached(portable: &Path, game: &Path, variant: Variant) -> Result<Option<Prepared>> {
    let Ok(bytes) = fs::read(cache_path(portable, variant)) else {
        return Ok(None);
    };
    if sha256_hex(&bytes) != variant.expected_sha256() {
        return Ok(None);
    }
    let salt = read_salt(&game.join("NBA2K27.exe"))?;
    let summary = verify(&bytes, &salt, variant, 0, 0, 0)?;
    Ok(Some(Prepared { bytes, summary }))
}

/// Build from the installed game with an explicit key; saves key + output.
pub(crate) fn build(
    portable: &Path,
    game: &Path,
    variant: Variant,
    key: &[u8],
    check: impl Fn() -> Result<()>,
) -> Result<Prepared> {
    check()?;
    let salt = read_salt(&game.join("NBA2K27.exe"))?;
    let [packed_nba, packed_wnba] = read_indexed(game, [PATH, WNBA_PATH], &check)?;
    ensure!(
        sha256_hex(&packed_nba) == STOCK_NBA_SHA256
            && sha256_hex(&packed_wnba) == STOCK_WNBA_SHA256,
        "Installed Learn2K rosters differ from the verified Version 2 files"
    );
    let oodle = Oodle::load(game)?;
    let base = Roster::parse(&decode(&packed_nba, key, &oodle)?)?;
    let src = Roster::parse(&decode(&packed_wnba, key, &oodle)?)?;
    check()?;
    ensure!(
        base.bad_crcs(&salt).is_empty() && src.bad_crcs(&salt).is_empty(),
        "Stock roster bundle CRCs differ from the verified executable"
    );
    save_key(portable, key)?;
    let (bytes, summary) = merge(base, src, variant, &salt)?;
    check()?;
    let path = cache_path(portable, variant);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, &path)?;
    Ok(Prepared { bytes, summary })
}

#[cfg(test)]
#[path = "wnba_roster_v2_tests.rs"]
mod tests;
