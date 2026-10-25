//! Decoded NBA 2K27 Version 2 roster IFF (stored zip -> roster.RDAT) codec.
//! Port of tools/rdat_roster.py (artifacts/wnba-merge-20261017.md). Pure data;
//! no game process or file access.
//!
//! RDAT  u32le (len - 0x10), 12 bytes, 'BNH(' u32le (len - 0x18), 16 bytes, then
//!       'BNDL' chunks: 'BNDL', u32le payloadLen, 0x34-byte header, payload,
//!       zero-padded so each chunk is a multiple of 8 bytes.
//! Header (13 x u32le): [0] id, [1] dataLen, [2] auxLen, [4] auxCrc,
//!       [5] crcOffset, [6] crcLen, [7] dataCrc, [11] id2. CRC = crc32(section +
//!       SALT); SALT is 22 bytes in the executable (read by the caller).
//! Payload: MSB-first bit-packed rows, then the big-endian schema (19-word
//!       header: total bits u64, type hash, id, row count at word 15; 19-word
//!       field records). Field type 21 = reference (u32 row, u32 tag).
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;

pub(crate) const DIRECTORY: u32 = 0x8C76_B363;
const HEADER: usize = 0x34;
const RECORD_WORDS: usize = 19;
pub(crate) const REF_TYPE: u32 = 21;
pub(crate) const REF_TAG: u32 = 0x10E7_BC3A;
pub(crate) const NULL_REF: u64 = u64::MAX;
const MAX_IFF: usize = 64 * 1024 * 1024;

pub(crate) fn padded(n: usize) -> usize {
    ((8 + HEADER + n + 7) & !7) - 8 - HEADER
}
fn le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn be(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}
fn crc(section: &[u8], salt: &[u8]) -> u32 {
    let mut h = crc32fast::Hasher::new();
    h.update(section);
    h.update(salt);
    h.finalize()
}

/// Read up to 64 bits at an MSB-first bit offset.
pub(crate) fn get(buf: &[u8], off: usize, width: usize) -> u64 {
    debug_assert!(width <= 64);
    let mut v = 0u64;
    let mut i = 0;
    while i < width {
        let bit = off + i;
        let byte = buf[bit / 8];
        let take = (8 - bit % 8).min(width - i);
        let shift = 8 - bit % 8 - take;
        v = (v << take) | ((byte >> shift) as u64 & ((1u64 << take) - 1));
        i += take;
    }
    v
}
pub(crate) fn put(buf: &mut [u8], off: usize, width: usize, value: u64) {
    debug_assert!(width <= 64);
    let mut i = 0;
    while i < width {
        let bit = off + i;
        let take = (8 - bit % 8).min(width - i);
        let shift = 8 - bit % 8 - take;
        let part = ((value >> (width - i - take)) & ((1u64 << take) - 1)) as u8;
        let mask = (((1u16 << take) - 1) as u8) << shift;
        buf[bit / 8] = (buf[bit / 8] & !mask) | (part << shift);
        i += take;
    }
}
pub(crate) fn wide(row: &[u8], off: usize, chars: usize) -> String {
    let units: Vec<u16> = (0..chars)
        .map(|k| get(row, off + 16 * k, 16) as u16)
        .take_while(|c| *c != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Extract `bits` bits starting at `start` into a fresh left-aligned buffer.
fn extract(data: &[u8], start: usize, bits: usize) -> Vec<u8> {
    let len = bits.div_ceil(8);
    let mut out = vec![0u8; len];
    let shift = start % 8;
    let base = start / 8;
    for (k, o) in out.iter_mut().enumerate() {
        let hi = data.get(base + k).copied().unwrap_or(0) as u16;
        let lo = data.get(base + k + 1).copied().unwrap_or(0) as u16;
        *o = (((hi << 8) | lo) >> (8 - shift)) as u8;
    }
    let tail = bits % 8;
    if tail != 0 {
        out[len - 1] &= 0xFFu8 << (8 - tail);
    }
    out
}
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}
impl BitWriter {
    fn byte(&mut self, b: u8, bits: u32) {
        // Push the top `bits` bits of b.
        self.acc = (self.acc << bits) | ((b as u32) >> (8 - bits));
        self.n += bits;
        while self.n >= 8 {
            self.n -= 8;
            self.out.push((self.acc >> self.n) as u8);
        }
        self.acc &= (1 << self.n) - 1;
    }
    fn row(&mut self, row: &[u8], bits: usize) {
        for (k, b) in row.iter().enumerate() {
            let left = bits - k * 8;
            if left == 0 {
                break;
            }
            self.byte(*b, left.min(8) as u32);
        }
    }
    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push((self.acc << (8 - self.n)) as u8);
        }
        self.out
    }
}
pub(crate) fn pack_rows(rows: &[Vec<u8>], bits: usize) -> Vec<u8> {
    let mut w = BitWriter {
        out: Vec::with_capacity((rows.len() * bits).div_ceil(8)),
        acc: 0,
        n: 0,
    };
    for r in rows {
        w.row(r, bits);
    }
    w.finish()
}
pub(crate) fn unpack_rows(data: &[u8], rows: usize, bits: usize) -> Vec<Vec<u8>> {
    (0..rows).map(|i| extract(data, i * bits, bits)).collect()
}

#[derive(Clone)]
pub(crate) struct Table {
    pub id: u32,
    header: Vec<u8>,
    data: Vec<u8>,
    pub aux: Vec<u8>,
    pub rows: usize,
    pub bits: usize,
    cache: Option<Vec<Vec<u8>>>,
}
impl Table {
    fn parse(chunk: &[u8], rows: usize) -> Result<Self> {
        ensure!(chunk.len() >= HEADER, "Roster chunk truncated");
        let id = le(chunk, 0);
        let (data_len, aux_len) = (le(chunk, 4) as usize, le(chunk, 8) as usize);
        ensure!(
            chunk.len() == HEADER + padded(data_len + aux_len),
            "Roster table {id:08x} has a bad chunk length"
        );
        ensure!(
            chunk[HEADER + data_len + aux_len..].iter().all(|b| *b == 0),
            "Roster table {id:08x} has non-zero padding"
        );
        let data = chunk[HEADER..HEADER + data_len].to_vec();
        let aux = chunk[HEADER + data_len..HEADER + data_len + aux_len].to_vec();
        let mut bits = 0;
        if aux.len() >= RECORD_WORDS * 4 {
            let total = ((be(&aux, 0) as u64) << 32) | be(&aux, 4) as u64;
            ensure!(
                be(&aux, 60) as usize == rows,
                "Roster table {id:08x} schema row count differs from the directory"
            );
            if rows > 0 {
                ensure!(
                    total % rows as u64 == 0 && (total as usize).div_ceil(8) == data_len,
                    "Roster table {id:08x} schema bit total is inconsistent"
                );
                bits = (total / rows as u64) as usize;
            }
        } else {
            ensure!(rows == 0, "Roster table {id:08x} has rows without a schema");
        }
        Ok(Self {
            id,
            header: chunk[..HEADER].to_vec(),
            data,
            aux,
            rows,
            bits,
            cache: None,
        })
    }
    fn h(&self, i: usize) -> u32 {
        le(&self.header, 4 * i)
    }
    pub(crate) fn all_rows(&mut self) -> &[Vec<u8>] {
        if self.cache.is_none() {
            self.cache = Some(unpack_rows(&self.data, self.rows, self.bits));
        }
        self.cache.as_ref().unwrap()
    }
    pub(crate) fn rows_vec(&mut self) -> Vec<Vec<u8>> {
        self.all_rows().to_vec()
    }
    pub(crate) fn set_rows(&mut self, rows: Vec<Vec<u8>>) {
        let total = (rows.len() * self.bits) as u64;
        self.data = pack_rows(&rows, self.bits);
        self.rows = rows.len();
        self.aux[0..4].copy_from_slice(&((total >> 32) as u32).to_be_bytes());
        self.aux[4..8].copy_from_slice(&(total as u32).to_be_bytes());
        self.aux[60..64].copy_from_slice(&(self.rows as u32).to_be_bytes());
        self.cache = Some(rows);
    }
    fn crcs_ok(&self, salt: &[u8]) -> bool {
        let content = [self.data.as_slice(), &self.aux].concat();
        let (off, len) = (self.h(5) as usize, self.h(6) as usize);
        (len == 0
            || (off + len <= content.len() && crc(&content[off..off + len], salt) == self.h(7)))
            && (self.h(2) == 0 || crc(&self.aux, salt) == self.h(4))
    }
    fn chunk(&self, salt: &[u8]) -> Vec<u8> {
        let mut hdr = self.header.clone();
        hdr[4..8].copy_from_slice(&(self.data.len() as u32).to_le_bytes());
        hdr[8..12].copy_from_slice(&(self.aux.len() as u32).to_le_bytes());
        if self.h(6) != 0 || !self.data.is_empty() {
            hdr[20..24].copy_from_slice(&0u32.to_le_bytes());
            hdr[24..28].copy_from_slice(&(self.data.len() as u32).to_le_bytes());
            hdr[28..32].copy_from_slice(&crc(&self.data, salt).to_le_bytes());
        }
        if !self.aux.is_empty() {
            hdr[16..20].copy_from_slice(&crc(&self.aux, salt).to_le_bytes());
        }
        let body_len = self.data.len() + self.aux.len();
        let mut out = hdr;
        out.extend_from_slice(&self.data);
        out.extend_from_slice(&self.aux);
        out.resize(HEADER + padded(body_len), 0);
        out
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub width: usize,
    pub kind: u32,
    pub type_hash: u32,
    pub name_hash: u32,
    pub parent: u32,
    pub count: usize,
    pub offset: usize,
}
pub(crate) fn schema(table: &Table) -> Result<Vec<Field>> {
    let a = &table.aux;
    let n = a.len() / 4;
    let w = |i: usize| be(a, i * 4);
    let mut recs = Vec::new();
    let mut i = RECORD_WORDS;
    while i + RECORD_WORDS <= n && w(i + 6) as usize == recs.len() + 1 {
        recs.push((0..RECORD_WORDS).map(|k| w(i + k)).collect::<Vec<_>>());
        i += RECORD_WORDS;
    }
    let mut offsets: BTreeMap<u32, usize> = BTreeMap::new();
    let mut pending: Vec<&Vec<u32>> = recs.iter().collect();
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|r| {
            if r[5] == 0 || offsets.contains_key(&r[5]) {
                let parent = if r[5] == 0 { 0 } else { offsets[&r[5]] };
                offsets.insert(r[6], r[0] as usize + parent);
                false
            } else {
                true
            }
        });
        ensure!(
            pending.len() < before,
            "Roster table {:08x} has unresolved parent fields",
            table.id
        );
    }
    let fields: Vec<Field> = recs
        .iter()
        .map(|r| Field {
            width: r[1] as usize,
            kind: r[2],
            type_hash: r[3],
            name_hash: r[4],
            parent: r[5],
            count: if r[7] == 1 { r[15] as usize } else { 1 },
            offset: offsets[&r[6]],
        })
        .collect();
    let top = fields.iter().filter(|f| f.parent == 0);
    if table.bits > 0
        && let Some(end) = top.map(|f| f.offset + f.width).max()
    {
        ensure!(
            end == table.bits,
            "Roster table {:08x} schema does not cover its row bits",
            table.id
        );
    }
    Ok(fields)
}
/// Bit offsets of every reference element (the target is the field type hash).
pub(crate) fn ref_slots(table: &Table) -> Result<Vec<(u32, usize)>> {
    let mut out = Vec::new();
    for f in schema(table)? {
        if f.kind != REF_TYPE {
            continue;
        }
        ensure!(
            f.parent == 0 && f.width == 64 * f.count,
            "Roster table {:08x} has an unsupported reference shape",
            table.id
        );
        out.extend((0..f.count).map(|k| (f.type_hash, f.offset + 64 * k)));
    }
    Ok(out)
}
pub(crate) fn field_by_name(table: &Table, name: u32) -> Result<Field> {
    let found: Vec<_> = schema(table)?
        .into_iter()
        .filter(|f| f.name_hash == name && f.parent == 0)
        .collect();
    ensure!(
        found.len() == 1,
        "Roster table {:08x} field {name:08x} is not unique",
        table.id
    );
    Ok(found.into_iter().next().unwrap())
}

#[derive(Clone)]
struct Directory {
    header: Vec<u8>,
    prefix: Vec<u8>,
    ids: Vec<u32>,
    suffix: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct Roster {
    zip_local: Vec<u8>,
    zip_central: Vec<u8>,
    rdat_prefix: Vec<u8>,
    directory: Directory,
    pub order: Vec<u32>,
    pub tables: BTreeMap<u32, Table>,
}
impl Roster {
    pub(crate) fn parse(iff: &[u8]) -> Result<Self> {
        ensure!(
            (64..=MAX_IFF).contains(&iff.len()) && iff[..4] == *b"PK\x03\x04",
            "Roster is not a zip archive"
        );
        let method = u16::from_le_bytes([iff[8], iff[9]]);
        let crc32 = le(iff, 14);
        let (csize, usize_) = (le(iff, 18) as usize, le(iff, 22) as usize);
        let name_len = u16::from_le_bytes([iff[26], iff[27]]) as usize;
        let extra_len = u16::from_le_bytes([iff[28], iff[29]]) as usize;
        let data_at = 30 + name_len + extra_len;
        ensure!(
            method == 0
                && csize == usize_
                && iff.get(30..30 + name_len) == Some(b"roster.RDAT".as_slice())
                && data_at + csize <= iff.len(),
            "Roster zip must hold one stored roster.RDAT member"
        );
        let rdat = &iff[data_at..data_at + csize];
        ensure!(
            crc32fast::hash(rdat) == crc32,
            "Roster zip member CRC differs"
        );
        let central = &iff[data_at + csize..];
        let eocd = central
            .windows(4)
            .rposition(|w| w == b"PK\x05\x06")
            .context("Roster zip end record missing")?;
        ensure!(
            central.starts_with(b"PK\x01\x02")
                && central.len() >= eocd + 22
                && u16::from_le_bytes([central[eocd + 10], central[eocd + 11]]) == 1,
            "Roster zip must have exactly one central entry"
        );
        ensure!(
            rdat.len() >= 0x28
                && le(rdat, 0) as usize == rdat.len() - 0x10
                && rdat[0x10..0x14] == *b"BNH("
                && le(rdat, 0x14) as usize == rdat.len() - 0x18,
            "Roster member is not an RDAT stream"
        );
        let mut chunks = Vec::new();
        let mut p = 0x28;
        while p < rdat.len() {
            ensure!(
                p + 8 + HEADER <= rdat.len() && rdat[p..p + 4] == *b"BNDL",
                "Roster bundle header missing at {p:#x}"
            );
            let size = le(rdat, p + 4) as usize;
            ensure!(
                p + 8 + HEADER + size <= rdat.len(),
                "Roster bundle truncated"
            );
            chunks.push(&rdat[p + 8..p + 8 + HEADER + size]);
            p += 8 + HEADER + size;
        }
        ensure!(
            p == rdat.len() && chunks.len() >= 2,
            "Roster bundle stream is malformed"
        );
        let dir = chunks[0];
        let dh = |i: usize| le(dir, 4 * i);
        ensure!(
            dh(0) == DIRECTORY && dh(1) == 0 && dh(2) == 0 && dh(5) == 0,
            "Roster directory header is unexpected"
        );
        let body_len = dh(6) as usize;
        ensure!(
            dir.len() == HEADER + padded(body_len)
                && dir[HEADER + body_len..].iter().all(|b| *b == 0),
            "Roster directory padding is invalid"
        );
        let body = &dir[HEADER..HEADER + body_len];
        let first = le(chunks[1], 0).to_be_bytes();
        let q = body
            .windows(4)
            .position(|w| w == first)
            .context("Roster directory entries missing")?;
        ensure!(
            matches!((body.len() - q) % 8, 0 | 2),
            "Roster directory entries are misaligned"
        );
        let n = (body.len() - q) / 8;
        let mut ids = Vec::new();
        let mut counts = BTreeMap::new();
        for k in 0..n {
            let id = be(body, q + 8 * k);
            ids.push(id);
            ensure!(
                counts
                    .insert(id, be(body, q + 8 * k + 4) as usize)
                    .is_none(),
                "Roster directory repeats a table"
            );
        }
        let mut order = Vec::new();
        let mut tables = BTreeMap::new();
        for c in &chunks[1..] {
            let id = le(c, 0);
            let rows = *counts
                .get(&id)
                .context("Roster table missing from directory")?;
            order.push(id);
            tables.insert(id, Table::parse(c, rows)?);
        }
        ensure!(
            order == ids,
            "Roster directory order differs from its tables"
        );
        Ok(Self {
            zip_local: iff[..data_at].to_vec(),
            zip_central: central.to_vec(),
            rdat_prefix: rdat[..0x28].to_vec(),
            directory: Directory {
                header: dir[..HEADER].to_vec(),
                prefix: body[..q].to_vec(),
                ids,
                suffix: body[q + 8 * n..].to_vec(),
            },
            order,
            tables,
        })
    }
    pub(crate) fn table(&mut self, id: u32) -> Result<&mut Table> {
        self.tables
            .get_mut(&id)
            .with_context(|| format!("Roster table {id:08x} missing"))
    }
    pub(crate) fn bad_crcs(&self, salt: &[u8]) -> Vec<u32> {
        let mut bad: Vec<u32> = self
            .order
            .iter()
            .filter(|id| !self.tables[id].crcs_ok(salt))
            .copied()
            .collect();
        let body = self.directory_body();
        if crc(&body, salt) != le(&self.directory.header, 28) {
            bad.push(DIRECTORY);
        }
        bad
    }
    /// schema type hash -> table ids
    pub(crate) fn type_map(&self) -> BTreeMap<u32, Vec<u32>> {
        let mut out: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for id in &self.order {
            let t = &self.tables[id];
            if t.aux.len() >= RECORD_WORDS * 4 {
                out.entry(be(&t.aux, 12)).or_default().push(*id);
            }
        }
        out
    }
    fn directory_body(&self) -> Vec<u8> {
        let mut body = self.directory.prefix.clone();
        for id in &self.directory.ids {
            body.extend_from_slice(&id.to_be_bytes());
            body.extend_from_slice(&(self.tables[id].rows as u32).to_be_bytes());
        }
        body.extend_from_slice(&self.directory.suffix);
        body
    }
    fn rdat_bytes(&self, salt: &[u8]) -> Vec<u8> {
        let body = self.directory_body();
        let mut dir = self.directory.header.clone();
        dir[24..28].copy_from_slice(&(body.len() as u32).to_le_bytes());
        dir[28..32].copy_from_slice(&crc(&body, salt).to_le_bytes());
        dir.extend_from_slice(&body);
        dir.resize(HEADER + padded(body.len()), 0);
        let mut out = self.rdat_prefix.clone();
        for chunk in
            std::iter::once(dir).chain(self.order.iter().map(|id| self.tables[id].chunk(salt)))
        {
            out.extend_from_slice(b"BNDL");
            out.extend_from_slice(&((chunk.len() - HEADER) as u32).to_le_bytes());
            out.extend_from_slice(&chunk);
        }
        let total = out.len() as u32;
        out[0..4].copy_from_slice(&(total - 0x10).to_le_bytes());
        out[0x14..0x18].copy_from_slice(&(total - 0x18).to_le_bytes());
        out
    }
    pub(crate) fn iff_bytes(&self, salt: &[u8]) -> Result<Vec<u8>> {
        let rdat = self.rdat_bytes(salt);
        let crc32 = crc32fast::hash(&rdat);
        let mut local = self.zip_local.clone();
        local[14..18].copy_from_slice(&crc32.to_le_bytes());
        local[18..22].copy_from_slice(&(rdat.len() as u32).to_le_bytes());
        local[22..26].copy_from_slice(&(rdat.len() as u32).to_le_bytes());
        let mut central = self.zip_central.clone();
        central[16..20].copy_from_slice(&crc32.to_le_bytes());
        central[20..24].copy_from_slice(&(rdat.len() as u32).to_le_bytes());
        central[24..28].copy_from_slice(&(rdat.len() as u32).to_le_bytes());
        let Some(eocd) = central.windows(4).rposition(|w| w == b"PK\x05\x06") else {
            bail!("Roster zip end record missing");
        };
        central[eocd + 16..eocd + 20]
            .copy_from_slice(&((local.len() + rdat.len()) as u32).to_le_bytes());
        let mut out = local;
        out.extend_from_slice(&rdat);
        out.extend_from_slice(&central);
        ensure!(out.len() <= MAX_IFF, "Merged roster exceeds bounds");
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bit_helpers_round_trip_unaligned_rows() {
        for bits in [1usize, 7, 13, 64, 495, 9721] {
            let rows: Vec<Vec<u8>> = (0..23u64)
                .map(|i| {
                    let mut r = vec![0u8; bits.div_ceil(8)];
                    for b in 0..bits {
                        if (i * 0x9E37_79B9 + b as u64 * 7) % 3 == 0 {
                            put(&mut r, b, 1, 1);
                        }
                    }
                    r
                })
                .collect();
            let packed = pack_rows(&rows, bits);
            assert_eq!(packed.len(), (23 * bits).div_ceil(8));
            assert_eq!(unpack_rows(&packed, 23, bits), rows);
        }
        let mut r = vec![0u8; 13];
        put(&mut r, 37, 64, 0x0123_4567_89AB_CDEF);
        assert_eq!(get(&r, 37, 64), 0x0123_4567_89AB_CDEF);
        put(&mut r, 37, 9, 0x1AB);
        assert_eq!(get(&r, 37, 9), 0x1AB);
    }
    #[test]
    fn chunk_padding_rule_matches_stock_rosters() {
        for (content, extra) in [
            (0, 4),
            (4, 0),
            (1, 3),
            (5, 7),
            (2, 2),
            (6, 6),
            (3, 1),
            (7, 5),
        ] {
            assert_eq!(padded(800 + content) - (800 + content), extra);
        }
    }
}
