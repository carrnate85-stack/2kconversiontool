//! Version 2 in-game Jukebox cover art, route B of
//! artifacts\jukebox-covers-v2-route-20261017.md (verifier
//! tools\jukebox_v2_cover_route_audit.py). File-only and read-only on the game:
//! the stock album-cover container is retargeted in memory, and each custom
//! cover is published in stock texture form at a fresh content-hash path that
//! the stock-first provider serves because stock lacks it. The container keeps
//! its stock name, so it takes one forced OVERRIDE slot. No game memory, no
//! native change.
//!
//! WPF (core\jukebox-artwork-session.js) borrowed each song's stock cover slot
//! and rewrote its `Cover Name`; that rule is kept. Only the delivery differs,
//! because V2 is stock-first and stores covers as planar BC1 + Oodle Leviathan.
use crate::jukebox_catalog::Music;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::OnceLock,
};

/// `alb_cvr_cont_{crc32("jukebox"):08x}.iff`, the "jukebox" cover context.
pub(crate) const CONTAINER: &str = "alb_cvr_cont_fcb780e2.iff";
pub(crate) const MAX_CONTAINER: usize = 1024 * 1024;
/// Stock-form 256x256 BC1 cover: 32-byte header plus a payload < 32768 bytes.
pub(crate) const MAX_TEXTURE: usize = 32 + RAW;
pub(crate) const MAX_COVERS: usize = 71;
const MAX_PACKED: usize = 1024 * 1024;
const OODLE_SHA: &str = "9A66B713306E504863646969A59FA435C3EAC01457925F25024A4883803892F5";
const TXTR_MAGIC: [u8; 4] = [0x91, 0xf8, 0x1f, 0xe0];
const RAW: usize = 32768;
const BLOCKS: usize = 4096;
/// `TLD `, 0, `05 21 47 01` (method 0x21, DXGI BC1_UNORM), flags 31, 256x256, 1/1.
const PREFIX: [u8; 24] = [
    0x54, 0x4c, 0x44, 0x20, 0, 0, 0, 0, 0x05, 0x21, 0x47, 0x01, 0x1f, 0, 0, 0, 0, 0x01, 0, 0x01,
    0x01, 0, 0x01, 0,
];
const LEVIATHAN: i32 = 13;
const LEVEL_OPTIMAL1: i32 = 5;
/// Stock 255x256 cover, never a slot.
const ODD_COVER: &str = "071_ruff_ryders_anthem";

fn crc64_table() -> &'static [u64; 256] {
    static TABLE: OnceLock<[u64; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u64; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let mut c = i as u64;
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    (c >> 1) ^ 0xC96C_5795_D787_0F42
                } else {
                    c >> 1
                };
            }
            *slot = c;
        }
        table
    })
}
/// CRC-64/XZ of the whole stored TLD; V2 names shared textures by it.
pub(crate) fn crc64_xz(data: &[u8]) -> u64 {
    let table = crc64_table();
    let mut c = !0u64;
    for &b in data {
        c = table[((c ^ u64::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}
fn hex16(value: u64) -> String {
    format!("{value:016x}")
}
fn le16(data: &[u8], at: usize) -> Result<usize> {
    Ok(u16::from_le_bytes(
        data.get(at..at + 2)
            .context("Cover container is truncated")?
            .try_into()?,
    ) as usize)
}
fn le32(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        data.get(at..at + 4)
            .context("Cover container is truncated")?
            .try_into()?,
    ))
}
/// Names that are safe both as a provider key and as a Mods file name.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
/// `shared/<hh>/<cover>.<16hex>.tld` where hh is the hash's first byte.
pub(crate) fn texture_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    let [shared, folder, leaf] = parts.as_slice() else {
        return false;
    };
    let pieces = leaf.split('.').collect::<Vec<_>>();
    let hex = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    *shared == "shared"
        && folder.len() == 2
        && hex(folder)
        && pieces.len() == 3
        && plain_name(pieces[0])
        && pieces[1].len() == 16
        && hex(pieces[1])
        && pieces[1].starts_with(folder)
        && pieces[2] == "tld"
}
pub(crate) fn texture_logical(cover: &str, texture: &[u8]) -> String {
    let hash = hex16(crc64_xz(texture));
    format!("shared/{}/{cover}.{hash}.tld", &hash[..2])
}

// ------------------------------------------------------------------ texture
/// Library covers are stored as WPF's interleaved 32800-byte TLD. V2 stock
/// covers hold 4096 endpoint dwords followed by 4096 index dwords.
fn planar_split(interleaved: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        interleaved.len() == RAW && RAW / 8 == BLOCKS,
        "Cover BC1 data must be 32768 bytes"
    );
    let mut out = Vec::with_capacity(RAW);
    for block in interleaved.chunks_exact(8) {
        out.extend_from_slice(&block[..4]);
    }
    for block in interleaved.chunks_exact(8) {
        out.extend_from_slice(&block[4..]);
    }
    Ok(out)
}
#[cfg(test)]
fn planar_join(planar: &[u8]) -> Vec<u8> {
    let half = RAW / 2;
    (0..BLOCKS)
        .flat_map(|i| {
            planar[i * 4..i * 4 + 4]
                .iter()
                .chain(&planar[half + i * 4..half + i * 4 + 4])
                .copied()
                .collect::<Vec<_>>()
        })
        .collect()
}
/// Stock form: byte 9 0x21 pairs with a real Oodle payload, never stored == raw.
pub(crate) fn validate_texture(texture: &[u8]) -> Result<()> {
    ensure!(
        texture.len() > 33
            && texture.len() <= MAX_TEXTURE
            && texture[..24] == PREFIX
            && le32(texture, 24)? as usize == RAW
            && le32(texture, 28)? as usize == texture.len() - 32
            && texture.len() - 32 < RAW
            && texture[32] == 0x8c,
        "Cover texture is not the Version 2 stock form (planar BC1, Oodle Leviathan)"
    );
    Ok(())
}
pub(crate) fn stock_texture(native: &[u8], oodle: &Oodle) -> Result<Vec<u8>> {
    crate::jukebox_artwork::validate_native(native)?;
    let planar = planar_split(&native[32..])?;
    let payload = oodle.compress(&planar)?;
    ensure!(
        oodle.decompress(&payload, RAW)? == planar,
        "Cover texture Oodle round trip failed"
    );
    let mut out = PREFIX.to_vec();
    out.extend_from_slice(&(RAW as u32).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    validate_texture(&out)?;
    Ok(out)
}

// ------------------------------------------------------------------ Oodle
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
type Compress = unsafe extern "C" fn(
    i32,
    *const u8,
    i64,
    *mut u8,
    i32,
    *const u8,
    *const u8,
    *const u8,
    *mut u8,
    i64,
) -> i64;
fn plain(path: &Path) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0,
        "Cover route game file is linked/nonordinary"
    );
    Ok(f)
}
/// The game's own Oodle runtime, SHA-256 pinned, held deny-write while loaded.
pub(crate) struct Oodle {
    decode: Decode,
    compress: Compress,
    _library: libloading::Library,
    _pin: File,
}
impl Oodle {
    pub fn load(game: &Path) -> Result<Self> {
        let path = game.join("data/oodle/oo2core_9_win64.dll");
        let mut pin = plain(&path)?;
        ensure!(
            pin.metadata()?.len() <= 16 * 1024 * 1024,
            "Oodle runtime is oversized"
        );
        let mut bytes = Vec::new();
        pin.read_to_end(&mut bytes)?;
        ensure!(
            format!("{:X}", Sha256::digest(&bytes)) == OODLE_SHA,
            "Oodle runtime is not the recorded Version 2 DLL"
        );
        let path = path.canonicalize()?;
        // Absolute installed DLL only; dependency search limited to its folder
        // and System32, exactly as jukebox_v2_resources loads it.
        let library: libloading::Library = unsafe {
            libloading::os::windows::Library::load_with_flags(path.as_os_str(), 0x100 | 0x800)?
        }
        .into();
        let decode = *unsafe { library.get::<Decode>(b"OodleLZ_Decompress\0") }?;
        let compress = *unsafe { library.get::<Compress>(b"OodleLZ_Compress\0") }?;
        Ok(Self {
            decode,
            compress,
            _library: library,
            _pin: pin,
        })
    }
    fn decompress(&self, payload: &[u8], size: usize) -> Result<Vec<u8>> {
        let mut out = vec![0; size];
        let n = unsafe {
            (self.decode)(
                payload.as_ptr(),
                payload.len() as i64,
                out.as_mut_ptr(),
                size as i64,
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
        ensure!(n == size as i64, "Oodle decode length differs");
        Ok(out)
    }
    fn compress(&self, raw: &[u8]) -> Result<Vec<u8>> {
        let mut out = vec![0; raw.len() + 65536];
        let n = unsafe {
            (self.compress)(
                LEVIATHAN,
                raw.as_ptr(),
                raw.len() as i64,
                out.as_mut_ptr(),
                LEVEL_OPTIMAL1,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            )
        };
        ensure!(n > 0 && n as usize <= out.len(), "Oodle compression failed");
        out.truncate(n as usize);
        Ok(out)
    }
}
fn vcz_decode(packed: &[u8], oodle: &Oodle) -> Result<Vec<u8>> {
    ensure!(
        packed.len() > 24
            && packed[..4] == [0x1f, 0x8b, 0x21, 0x04]
            && packed[10..16] == *b"\x04\x00VCZ\x00",
        "Stock cover container is not an unencrypted VCZ entry"
    );
    let end = packed.len();
    let crc = le32(packed, end - 8)?;
    let size = le32(packed, end - 4)? as usize;
    ensure!(
        (22..=MAX_CONTAINER).contains(&size),
        "Stock cover container exceeds bounds"
    );
    let out = oodle.decompress(&packed[16..end - 8], size)?;
    ensure!(
        crc32fast::hash(&out) == crc,
        "Stock cover container CRC differs"
    );
    Ok(out)
}

// ------------------------------------------------------------------ container
#[derive(Clone, Debug)]
struct Member {
    name: String,
    start: usize,
    size: usize,
    local: usize,
    central: usize,
}
/// Strict STORED zip: one disk, no encryption/descriptors, sizes and CRCs exact.
fn members(data: &[u8]) -> Result<Vec<Member>> {
    ensure!(
        (22..=MAX_CONTAINER).contains(&data.len()),
        "Cover container exceeds bounds"
    );
    let eocd = data
        .windows(4)
        .rposition(|w| w == b"PK\x05\x06")
        .context("Cover container has no zip directory")?;
    ensure!(
        eocd + 22 + le16(data, eocd + 20)? == data.len()
            && le16(data, eocd + 4)? == 0
            && le16(data, eocd + 6)? == 0
            && le16(data, eocd + 8)? == le16(data, eocd + 10)?,
        "Cover container zip directory is unsupported"
    );
    let count = le16(data, eocd + 10)?;
    let offset = le32(data, eocd + 16)? as usize;
    ensure!(
        offset + le32(data, eocd + 12)? as usize == eocd && count <= 512,
        "Cover container zip directory is inconsistent"
    );
    let (mut at, mut out, mut names) = (offset, Vec::new(), BTreeSet::new());
    for _ in 0..count {
        ensure!(
            data.get(at..at + 4) == Some(b"PK\x01\x02"),
            "Bad cover container directory entry"
        );
        let (flags, method) = (le16(data, at + 8)?, le16(data, at + 10)?);
        let (crc, packed, size) = (
            le32(data, at + 16)?,
            le32(data, at + 20)?,
            le32(data, at + 24)?,
        );
        let (n, extra, comment) = (
            le16(data, at + 28)?,
            le16(data, at + 30)?,
            le16(data, at + 32)?,
        );
        let local = le32(data, at + 42)? as usize;
        let name = std::str::from_utf8(data.get(at + 46..at + 46 + n).context("Truncated name")?)?;
        ensure!(
            flags & 0x9 == 0 && method == 0 && packed == size,
            "Cover container members must be STORED"
        );
        ensure!(
            data.get(local..local + 4) == Some(b"PK\x03\x04")
                && le16(data, local + 6)? & 0x9 == 0
                && le16(data, local + 8)? == 0
                && le32(data, local + 14)? == crc
                && data.get(local + 30..local + 30 + le16(data, local + 26)?)
                    == Some(name.as_bytes()),
            "Cover container local header differs"
        );
        let start = local + 30 + le16(data, local + 26)? + le16(data, local + 28)?;
        let size = size as usize;
        ensure!(
            start + size <= offset
                && crc32fast::hash(&data[start..start + size]) == crc
                && names.insert(name.to_owned()),
            "Cover container member CRC/range differs"
        );
        out.push(Member {
            name: name.into(),
            start,
            size,
            local,
            central: at,
        });
        at += 46 + n + extra + comment;
        ensure!(at <= eocd, "Cover container directory overruns");
    }
    ensure!(at == eocd, "Cover container directory has trailing data");
    Ok(out)
}
#[derive(Clone, Debug)]
struct Txtr {
    name: String,
    hash_at: usize,
    width: Option<u32>,
    height: Option<u32>,
}
fn field(data: &[u8], name: &str) -> Option<u32> {
    let key = crc32fast::hash(name.as_bytes()).to_le_bytes();
    let at = data.windows(4).position(|w| w == key)?;
    (data.get(at + 4..at + 8)? == [6, 0, 0, 0x40])
        .then(|| le32(data, at + 16).ok())
        .flatten()
}
/// One descriptor: CRC-32(name) at +0x18, `Binary` = `<name>.<16hex>.tld`.
fn txtr(data: &[u8]) -> Result<Txtr> {
    ensure!(
        data.len() > 0x1c && data[..4] == TXTR_MAGIC && data.last() == Some(&0),
        "Bad cover TXTR descriptor"
    );
    let table = le32(data, 8)? as usize;
    ensure!(
        table > 1 && table < data.len(),
        "Bad cover TXTR string table"
    );
    let strings = data[data.len() - table..data.len() - 1]
        .split(|b| *b == 0)
        .collect::<Vec<_>>();
    let name = std::str::from_utf8(strings.first().context("TXTR name missing")?)?;
    let leaf = std::str::from_utf8(strings.last().context("TXTR leaf missing")?)?;
    ensure!(
        le32(data, 0x18)? == crc32fast::hash(name.as_bytes())
            && strings.contains(&b"Binary".as_slice()),
        "Cover TXTR identity differs"
    );
    let hash = leaf
        .strip_prefix(name)
        .and_then(|r| r.strip_prefix('.'))
        .and_then(|r| r.strip_suffix(".tld"))
        .context("Cover TXTR Binary leaf does not match its name")?;
    ensure!(
        hash.len() == 16
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Cover TXTR Binary hash is invalid"
    );
    Ok(Txtr {
        name: name.into(),
        hash_at: data.len() - 1 - leaf.len() + name.len() + 1,
        width: field(data, "Width"),
        height: field(data, "Height"),
    })
}
pub(crate) struct Container {
    bytes: Vec<u8>,
    members: Vec<Member>,
    covers: BTreeMap<String, (usize, Txtr)>,
}
impl Container {
    pub fn parse(bytes: Vec<u8>) -> Result<Self> {
        let members = members(&bytes)?;
        let mut covers = BTreeMap::new();
        for (i, m) in members.iter().enumerate() {
            let Some(stem) = m.name.strip_suffix(".TXTR") else {
                continue;
            };
            let t = txtr(&bytes[m.start..m.start + m.size])?;
            ensure!(t.name == stem, "Cover TXTR name differs from its member");
            ensure!(
                covers.insert(stem.to_owned(), (i, t)).is_none(),
                "Duplicate cover TXTR"
            );
        }
        ensure!(
            !covers.is_empty(),
            "Cover container has no TXTR descriptors"
        );
        Ok(Self {
            bytes,
            members,
            covers,
        })
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.covers.keys().map(String::as_str)
    }
    /// 256x256, plain name, not the odd 255-wide cover or playlist art.
    pub fn slot(&self, name: &str) -> bool {
        self.covers.get(name).is_some_and(|(_, t)| {
            t.width == Some(256)
                && t.height == Some(256)
                && plain_name(name)
                && name != ODD_COVER
                && !name.starts_with("2k27_playlistart_")
        })
    }
    /// Same-size rewrite of the 16 hash digits, then that member's CRCs.
    pub fn retarget(&mut self, name: &str, hash: &str) -> Result<()> {
        ensure!(
            hash.len() == 16
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Cover hash must be 16 lowercase hex digits"
        );
        let (i, t) = self
            .covers
            .get(name)
            .context("Cover slot is not in the container")?;
        let m = &self.members[*i];
        let at = m.start + t.hash_at;
        self.bytes[at..at + 16].copy_from_slice(hash.as_bytes());
        let crc = crc32fast::hash(&self.bytes[m.start..m.start + m.size]).to_le_bytes();
        self.bytes[m.local + 14..m.local + 18].copy_from_slice(&crc);
        self.bytes[m.central + 16..m.central + 20].copy_from_slice(&crc);
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<u8>> {
        Self::parse(self.bytes.clone())?;
        Ok(self.bytes)
    }
    #[cfg(test)]
    fn hash(&self, name: &str) -> Option<String> {
        let (i, t) = self.covers.get(name)?;
        let at = self.members[*i].start + t.hash_at;
        String::from_utf8(self.bytes[at..at + 16].to_vec()).ok()
    }
}

// ------------------------------------------------------------------ game reads
/// One bounded manifest pass: the container's row, plus the shared texture
/// rows of the given cover stems (for the new-path collision refusal).
fn scan_manifest(
    game: &Path,
    stems: &BTreeSet<String>,
    check: &dyn Fn() -> Result<()>,
) -> Result<((String, u64, usize), BTreeSet<String>)> {
    let file = plain(&game.join("manifest"))?;
    let before = file.metadata()?;
    ensure!(
        before.len() <= 256 * 1024 * 1024,
        "Stock manifest is oversized"
    );
    let identity = crate::file_identity::Stable::read(&file)?;
    let mut reader = BufReader::new(&file);
    let (mut line, mut total) = (Vec::new(), 0usize);
    let (mut row, mut shared) = (None, BTreeSet::new());
    for n in 0..2_000_000usize {
        if n % 65536 == 0 {
            check()?;
        }
        line.clear();
        let read = reader.by_ref().take(8193).read_until(b'\n', &mut line)?;
        if read == 0 {
            let after = file.metadata()?;
            ensure!(
                after.len() == before.len()
                    && after.last_write_time() == before.last_write_time()
                    && crate::file_identity::Stable::read(&file)? == identity,
                "Stock manifest changed during the cover scan"
            );
            return Ok((row.context("Stock cover container is absent")?, shared));
        }
        total += read;
        ensure!(
            read <= 8192 && total <= 256 * 1024 * 1024,
            "Stock manifest exceeds bounds"
        );
        let text = String::from_utf8_lossy(&line);
        let fields = text
            .trim_end_matches(['\r', '\n'])
            .split(',')
            .collect::<Vec<_>>();
        let Some(path) = fields.first() else { continue };
        let path = path.replace('\\', "/").to_ascii_lowercase();
        if path == CONTAINER {
            ensure!(
                fields.len() == 4
                    && fields[1].len() == 2
                    && fields[1].bytes().all(|b| b.is_ascii_alphanumeric()),
                "Invalid stock cover container row"
            );
            let found = (fields[1].to_owned(), fields[2].parse()?, fields[3].parse()?);
            ensure!(
                row.replace(found).is_none(),
                "Ambiguous stock cover container"
            );
        } else if let Some(leaf) = path.strip_prefix("shared/").and_then(|p| p.get(3..)) {
            if leaf
                .split('.')
                .next()
                .is_some_and(|stem| stems.contains(stem))
            {
                shared.insert(path);
            }
        }
    }
    anyhow::bail!("Stock manifest exceeds row limit")
}
fn read_container(game: &Path, row: &(String, u64, usize), oodle: &Oodle) -> Result<Vec<u8>> {
    let (archive, offset, size) = row;
    ensure!(
        (25..=MAX_PACKED).contains(size),
        "Stock cover container range exceeds bounds"
    );
    let mut f = plain(&game.join(archive))?;
    let before = f.metadata()?;
    ensure!(
        offset
            .checked_add(*size as u64)
            .is_some_and(|end| end <= before.len()),
        "Stock cover container range exceeds its archive"
    );
    f.seek(SeekFrom::Start(*offset))?;
    let mut packed = vec![0; *size];
    f.read_exact(&mut packed)?;
    let after = f.metadata()?;
    ensure!(
        after.len() == before.len() && after.last_write_time() == before.last_write_time(),
        "Stock archive changed during the cover read"
    );
    vcz_decode(&packed, oodle)
}

// ------------------------------------------------------------------ plan
pub(crate) struct Route {
    /// Retargeted container, published at `CONTAINER`.
    pub container: Vec<u8>,
    /// `(shared/<hh>/<slot>.<crc64>.tld, stock-form texture)`.
    pub textures: Vec<(String, Vec<u8>)>,
    /// Songs with artwork that found no free slot (they keep their cover).
    pub skipped: usize,
}
impl Route {
    /// Container + textures as `(logical, bytes)`, container first.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        std::iter::once((CONTAINER.to_owned(), self.container.clone()))
            .chain(self.textures.iter().cloned())
            .collect()
    }
}
/// Picks slots exactly as WPF did (the song's own bank cover first, never a
/// cover name still shown by a song without new art), then a spare cover no
/// published song uses. `art[i]` belongs to `music.tracks[i]`; on success the
/// chosen slot becomes that track's `Cover Name`.
pub(crate) fn choose(
    names: &[String],
    stock: &Music,
    music: &Music,
    art: &[bool],
    slot: impl Fn(&str) -> bool,
) -> Result<Vec<Option<String>>> {
    ensure!(
        art.len() == music.tracks.len(),
        "Cover art list differs from the playlist"
    );
    let published = music
        .tracks
        .iter()
        .map(|t| t.cover.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let preserved = music
        .tracks
        .iter()
        .zip(art)
        .filter(|(_, a)| !**a)
        .map(|(t, _)| t.cover.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut spare = names
        .iter()
        .filter(|n| !published.contains(&n.to_ascii_lowercase()) && slot(n))
        .cloned()
        .collect::<Vec<_>>()
        .into_iter();
    let mut used = BTreeSet::new();
    let mut out = Vec::new();
    for (t, has) in music.tracks.iter().zip(art) {
        if !has {
            out.push(None);
            continue;
        }
        let own = stock
            .tracks
            .iter()
            .find(|s| s.collection == 1 && s.bank == t.bank)
            .map(|s| s.cover.clone())
            .filter(|c| {
                slot(c) && !preserved.contains(&c.to_ascii_lowercase()) && !used.contains(c)
            });
        let chosen = own.or_else(|| {
            spare
                .by_ref()
                .find(|c| !preserved.contains(&c.to_ascii_lowercase()) && !used.contains(c))
        });
        if let Some(c) = &chosen {
            used.insert(c.clone());
        }
        out.push(chosen);
    }
    ensure!(used.len() <= MAX_COVERS, "Too many cover slots");
    Ok(out)
}
/// Read-only on the game. `music` is the playlist music.iff about to be
/// published; tracks given art get their slot as `Cover Name`.
pub(crate) fn plan(
    game: &Path,
    stock: &Music,
    music: &mut Music,
    art: &[Option<Vec<u8>>],
    check: &dyn Fn() -> Result<()>,
) -> Result<Option<Route>> {
    ensure!(
        art.len() == music.tracks.len(),
        "Cover art list differs from the playlist"
    );
    if art.iter().all(Option::is_none) {
        return Ok(None);
    }
    check()?;
    let _pins = crate::loader_files::Directories::pin(game)?;
    let oodle = Oodle::load(game)?;
    let stems = stock
        .tracks
        .iter()
        .map(|t| t.cover.to_ascii_lowercase())
        .chain(std::iter::once("051_2ksoundeffects".to_owned()))
        .collect::<BTreeSet<_>>();
    let (row, existing) = scan_manifest(game, &stems, check)?;
    let mut container = Container::parse(read_container(game, &row, &oodle)?)?;
    check()?;
    let names = container.names().map(str::to_owned).collect::<Vec<_>>();
    let flags = art.iter().map(Option::is_some).collect::<Vec<_>>();
    let slots = choose(&names, stock, music, &flags, |n| container.slot(n))?;
    let (mut textures, mut skipped) = (Vec::new(), 0);
    for (i, slot) in slots.iter().enumerate() {
        let Some(native) = &art[i] else { continue };
        let Some(slot) = slot else {
            skipped += 1;
            continue;
        };
        check()?;
        let texture = stock_texture(native, &oodle)?;
        let logical = texture_logical(slot, &texture);
        ensure!(
            texture_path(&logical) && !existing.contains(&logical),
            "A new cover path already exists in the stock manifest"
        );
        container.retarget(slot, &hex16(crc64_xz(&texture)))?;
        music.tracks[i].cover = slot.clone();
        textures.push((logical, texture));
    }
    if textures.is_empty() {
        return Ok(None);
    }
    let container = container.finish()?;
    ensure!(
        container.len() <= MAX_CONTAINER,
        "Cover container exceeds bounds"
    );
    Ok(Some(Route {
        container,
        textures,
        skipped,
    }))
}

/// Test fixture for other modules: a one-cover container and a stock-form
/// texture header (not decodable; publication only checks its form).
#[cfg(test)]
pub(crate) fn fixture_files() -> Vec<(String, Vec<u8>)> {
    let mut texture = PREFIX.to_vec();
    texture.extend_from_slice(&(RAW as u32).to_le_bytes());
    texture.extend_from_slice(&3u32.to_le_bytes());
    texture.extend_from_slice(&[0x8c, 0x0c, 0]);
    vec![
        (
            CONTAINER.into(),
            tests::container(&[("001_helicopter", 256)]),
        ),
        (texture_logical("001_helicopter", &texture), texture),
    ]
}
#[cfg(test)]
#[path = "jukebox_cover_route_tests.rs"]
mod tests;
