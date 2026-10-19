//! Read-only Version 2 Jukebox resource proof. No native game-memory pointers,
//! running-process inspection, Steam dispatch, or game-folder mutation.
#![allow(dead_code)]
use crate::{jukebox_audio, jukebox_catalog, jukebox_package};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};
pub(crate) const IMAGE_SHA: &str =
    "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1";
const MAX_INDEX: usize = 128 * 1024 * 1024;
const CODEC_ANCHORS: &[(u32, &[u8])] = &[
    (0x05308401, &[0x41, 0x81, 0xf8, 0x7e, 0xa1, 0xcf, 0xbb]),
    (
        0x0530fac3,
        &[
            0x3d, 0xbd, 0x43, 0x35, 0x06, 0x0f, 0x84, 0x82, 0, 0, 0, 0x3d, 0xf9, 0x95, 0x81, 0x75,
            0x0f, 0x84, 0xdb, 0, 0, 0,
        ],
    ),
    (
        0x05317223,
        &[
            0xc7, 0x01, 0xbd, 0x43, 0x35, 0x06, 0x89, 0x51, 0x04, 0x44, 0x89, 0x41, 0x08,
        ],
    ),
    (
        0x0531723d,
        &[
            0x66, 0xc7, 0x41, 0x1c, 0x01, 0, 0x66, 0x89, 0x51, 0x1e, 0x44, 0x89, 0x41, 0x20,
        ],
    ),
    (
        0x0532248a,
        &[
            0xc4, 0xc1, 0x52, 0x2a, 0x40, 0x0c, 0x41, 0x8b, 0x50, 0x08, 0xc5, 0xd2, 0x2a, 0xca,
            0xc4, 0xc1, 0x52, 0x2a, 0x58, 0x04,
        ],
    ),
];
fn plain(path: &Path) -> Result<File> {
    let f = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0,
        "Jukebox stock resource is linked/nonordinary"
    );
    Ok(f)
}
struct Pins(Vec<File>);
impl Pins {
    fn pin(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Absolute game path required");
        let (mut p, mut pins) = (PathBuf::new(), Vec::new());
        for c in path.components() {
            ensure!(
                !matches!(c, Component::ParentDir | Component::CurDir),
                "Unnormalized game path"
            );
            p.push(c.as_os_str());
            if matches!(c, Component::Prefix(_)) {
                continue;
            }
            let f = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&p)?;
            let m = f.metadata()?;
            ensure!(
                m.is_dir() && m.file_attributes() & 0x400 == 0,
                "Linked game folder is unsupported"
            );
            pins.push(f);
        }
        Ok(Self(pins))
    }
}
fn stable(before: &fs::Metadata, after: &fs::Metadata) -> Result<()> {
    ensure!(
        before.len() == after.len() && before.last_write_time() == after.last_write_time(),
        "Stock resource changed during read"
    );
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Resource {
    pub archive: String,
    pub offset: u64,
    pub size: usize,
}
impl Resource {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.archive.len() == 2
                && self.archive.bytes().all(|b| b.is_ascii_alphanumeric())
                && (24..=jukebox_catalog::MAX_IFF).contains(&self.size)
                && self.offset.checked_add(self.size as u64).is_some(),
            "Invalid stock music archive range"
        );
        Ok(())
    }
    fn read(&self, root: &Path) -> Result<Vec<u8>> {
        self.validate()?;
        let mut f = plain(&root.join(&self.archive))?;
        let before = f.metadata()?;
        ensure!(
            self.offset + self.size as u64 <= before.len(),
            "Stock music range exceeds archive"
        );
        f.seek(SeekFrom::Start(self.offset))?;
        let mut b = vec![0; self.size];
        f.read_exact(&mut b)?;
        stable(&before, &f.metadata()?)?;
        Ok(b)
    }
}
fn lookup(mut manifest: impl BufRead) -> Result<Resource> {
    let (mut line, mut total, mut found) = (Vec::new(), 0usize, None);
    for _ in 0..2_000_000 {
        line.clear();
        let n = manifest.by_ref().take(8193).read_until(b'\n', &mut line)?;
        if n == 0 {
            return found.context("Stock music.iff is absent");
        }
        total += n;
        ensure!(
            n <= 8192 && total <= 256 * 1024 * 1024,
            "Stock manifest exceeds bounds"
        );
        let text = std::str::from_utf8(&line)?.trim_end_matches(['\r', '\n']);
        let fields: Vec<_> = text.split(',').collect();
        if fields.len() == 4
            && fields[0]
                .replace('\\', "/")
                .eq_ignore_ascii_case("music.iff")
        {
            let r = Resource {
                archive: fields[1].into(),
                offset: fields[2].parse()?,
                size: fields[3].parse()?,
            };
            r.validate()?;
            ensure!(
                found.replace(r).is_none(),
                "Ambiguous stock music.iff manifest"
            );
        }
    }
    anyhow::bail!("Stock manifest exceeds row limit")
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
pub(crate) struct Stock {
    pub resource: Resource,
    pub packed_sha256: String,
    pub decoded_sha256: String,
    pub music: jukebox_catalog::Music,
}
/// Hash every byte through a retained deny-write handle. This is read-only work,
/// not publication: refresh caller-supplied cancellation/source-seal checks in
/// bounded 16 MiB batches instead of checking every 1 MiB read. Always recheck
/// at EOF; callers separately prove process ownership immediately before every
/// mutation.
fn checked_image_hash(
    mut input: impl Read,
    expected_size: u64,
    check: impl Fn() -> Result<()>,
) -> Result<String> {
    const CHECK_BYTES: u64 = 16 * 1024 * 1024;
    check()?;
    let mut sha = Sha256::new();
    let mut block = vec![0; 1024 * 1024];
    let (mut total, mut since_check) = (0u64, 0u64);
    loop {
        let n = input.read(&mut block)?;
        if n == 0 {
            break;
        }
        total = total
            .checked_add(n as u64)
            .context("Image hash length overflow")?;
        ensure!(total <= expected_size, "Game image grew during hashing");
        sha.update(&block[..n]);
        since_check += n as u64;
        if since_check >= CHECK_BYTES {
            check()?;
            since_check = 0;
        }
    }
    check()?;
    ensure!(
        total == expected_size,
        "Game image was truncated during hashing"
    );
    Ok(format!("{:X}", sha.finalize()))
}
/// Kernel identity plus size and last-write time of a retained handle. A
/// Steam update or any rewrite of the file changes at least one field.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Seen {
    pub id: crate::file_identity::Stable,
    pub size: u64,
    pub written: u64,
}
impl Seen {
    pub(crate) fn read(file: &File) -> Result<Self> {
        let m = file.metadata()?;
        Ok(Self {
            id: crate::file_identity::Stable::read(file)?,
            size: m.len(),
            written: m.last_write_time(),
        })
    }
}
/// What a previous full read proved: this exact retained image hashed to
/// IMAGE_SHA, and this exact manifest named `resource` for music.iff. Only
/// those two expensive proofs are reused; the music resource itself is
/// always re-read, CRC-checked and decoded.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StockProof {
    pub image: Seen,
    pub manifest: Seen,
    pub resource: Resource,
}
pub(crate) fn read_stock(root: &Path, check: impl Fn() -> Result<()>) -> Result<Stock> {
    Ok(read_stock_with(root, check, None)?.0)
}
/// `proof` reuse is all-or-nothing per file: any identity/size/time mismatch
/// falls back to the full hash or manifest scan. Returns the fresh proof and
/// whether the image hash was reused.
pub(crate) fn read_stock_with(
    root: &Path,
    check: impl Fn() -> Result<()>,
    proof: Option<&StockProof>,
) -> Result<(Stock, StockProof, bool)> {
    check()?;
    let _pins = Pins::pin(root)?;
    let mut image = plain(&root.join("NBA2K27.exe"))?;
    let meta = image.metadata()?;
    ensure!(
        meta.len() == 870548760,
        "Unknown game image size for Jukebox profile"
    );
    let image_seen = Seen::read(&image)?;
    let reused = proof.is_some_and(|p| p.image == image_seen);
    if !reused {
        ensure!(
            checked_image_hash(&mut image, meta.len(), &check)? == IMAGE_SHA,
            "Jukebox requires the independently pinned Version 2 game build"
        );
    }
    stable(&meta, &image.metadata()?)?;
    ensure!(
        Seen::read(&image)? == image_seen,
        "Game image changed during Jukebox preparation"
    );
    // These sites were located by independent codec/magic references, then
    // disassembled in Version2. They prove container/header dispatch and PCM
    // channel/rate/scalar-duration semantics, not an offset delta or write gate.
    for (rva, expected) in CODEC_ANCHORS {
        image.seek(SeekFrom::Start(u64::from(*rva) - 0x1000 + 0x600))?;
        let mut data = vec![0; expected.len()];
        image.read_exact(&mut data)?;
        ensure!(
            data == *expected,
            "Version2 Jukebox codec anchor changed at {rva:08X}"
        );
    }
    let manifest = plain(&root.join("manifest"))?;
    let before = manifest.metadata()?;
    ensure!(
        before.len() <= 256 * 1024 * 1024,
        "Stock manifest is oversized"
    );
    let manifest_seen = Seen::read(&manifest)?;
    let resource = match proof.filter(|p| p.manifest == manifest_seen) {
        Some(p) => {
            p.resource.validate()?;
            p.resource.clone()
        }
        None => lookup(BufReader::new(&manifest))?,
    };
    stable(&before, &manifest.metadata()?)?;
    ensure!(
        Seen::read(&manifest)? == manifest_seen,
        "Stock manifest changed during read"
    );
    let packed = resource.read(root)?;
    ensure!(
        packed.len() >= 25
            && packed[..4] == [0x1f, 0x8b, 0x21, 4]
            && packed[10..12] == [4, 0]
            && &packed[12..16] == b"VCZ\0",
        "Missing authored stock music VCZ trailer"
    );
    let end = packed.len();
    let size = u32::from_le_bytes(packed[end - 4..].try_into()?) as usize;
    let crc = u32::from_le_bytes(packed[end - 8..end - 4].try_into()?);
    ensure!(
        (22..=jukebox_catalog::MAX_IFF).contains(&size),
        "Decoded stock music exceeds bounds"
    );
    let dll = root.join("data/oodle/oo2core_9_win64.dll");
    let _dllpins = Pins::pin(dll.parent().unwrap())?;
    let _dll = plain(&dll)?;
    let dll = dll.canonicalize()?;
    // Exactly the established native exporter ABI; absolute installed DLL only,
    // dependency search restricted to that DLL's directory and System32.
    let library: libloading::Library = unsafe {
        libloading::os::windows::Library::load_with_flags(dll.as_os_str(), 0x100 | 0x800)?
    }
    .into();
    let decode = *unsafe { library.get::<Decode>(b"OodleLZ_Decompress\0") }?;
    let mut decoded = vec![0; size];
    check()?;
    let count = unsafe {
        decode(
            packed[16..end - 8].as_ptr(),
            (end - 24) as i64,
            decoded.as_mut_ptr(),
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
    check()?;
    ensure!(
        count == size as i64 && crc32fast::hash(&decoded) == crc,
        "Stock music authored length/CRC differs"
    );
    let music = jukebox_package::read_music(&decoded)?;
    check()?;
    let proof = StockProof {
        image: image_seen,
        manifest: manifest_seen,
        resource: resource.clone(),
    };
    Ok((
        Stock {
            resource,
            packed_sha256: jukebox_catalog::digest(&packed),
            decoded_sha256: jukebox_catalog::digest(&decoded),
            music,
        },
        proof,
        reused,
    ))
}
pub(crate) fn archive_key(name: &str) -> u64 {
    let mut crc = u64::MAX;
    for b in name.to_ascii_uppercase().bytes() {
        crc ^= u64::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xc96c5795d7870f42 } else { 0 };
        }
    }
    crc ^ u64::MAX
}
/// Independently validates the first stock main-track container from the V2
/// archive index. Exact key uniqueness and whole-file range containment required.
pub(crate) fn read_stock_sample(
    root: &Path,
    bank: u16,
) -> Result<(Resource, jukebox_audio::AudioInfo)> {
    ensure!(
        (1..=71).contains(&bank),
        "Only stock main soundtrack sample lookup is allowed"
    );
    let (r, b) = read_bank(root, bank)?;
    Ok((r, jukebox_audio::inspect(&b)?))
}
/// Only a key present in the pinned stock catalog may address a native audio
/// archive. New Version2 stock72..93 are copied into free proven slots, never
/// presumed to be missing library customs. Metadata/playlist remains unchanged.
pub(crate) fn read_stock_native(
    root: &Path,
    stock: &jukebox_catalog::Music,
    key: &str,
) -> Result<(Resource, Vec<u8>)> {
    stock.validate()?;
    let t = stock
        .tracks
        .iter()
        .find(|t| t.key() == key)
        .context("Stock audio identity is absent")?;
    ensure!(
        t.collection == 1 && (1..999).contains(&t.bank),
        "Only stock main soundtrack audio is supported"
    );
    read_bank(root, t.bank)
}
fn read_bank(root: &Path, bank: u16) -> Result<(Resource, Vec<u8>)> {
    let _pins = Pins::pin(root)?;
    let mut index = plain(&root.join("0A"))?;
    let before = index.metadata()?;
    ensure!(
        before.len() <= MAX_INDEX as u64,
        "Stock index exceeds audit bound"
    );
    let mut bytes = Vec::with_capacity(before.len() as usize);
    index
        .by_ref()
        .take(MAX_INDEX as u64 + 1)
        .read_to_end(&mut bytes)?;
    stable(&before, &index.metadata()?)?;
    let needle = archive_key(&jukebox_catalog::audio_path(bank)).to_le_bytes();
    let mut hits = bytes
        .windows(8)
        .enumerate()
        .filter_map(|(i, w)| (w == needle).then_some(i));
    let at = hits.next().context("Stock Jukebox bank key is absent")?;
    ensure!(
        hits.next().is_none() && at + 24 <= bytes.len(),
        "Ambiguous/truncated stock Jukebox bank key"
    );
    let size = u32::from_le_bytes(bytes[at + 8..at + 12].try_into()?) as usize;
    let position = u64::from_le_bytes(bytes[at + 16..at + 24].try_into()?) >> 23;
    ensure!(
        (54..=80 * 1024 * 1024).contains(&size),
        "Stock audio sample exceeds bounds"
    );
    let mut ranges = fs::read_dir(root)?
        .filter_map(|r| r.ok())
        .filter(|e| {
            let n = e.file_name();
            let s = n.to_string_lossy();
            s.len() == 2 && s.as_bytes()[0].is_ascii_digit() && s.as_bytes()[1].is_ascii_uppercase()
        })
        .map(|e| e.path())
        .collect::<Vec<_>>();
    ranges.sort();
    let mut start = 0u64;
    for path in ranges {
        let mut file = plain(&path)?;
        let before = file.metadata()?;
        let end = start
            .checked_add(before.len())
            .context("Archive range overflow")?;
        if position >= start && position.checked_add(size as u64).is_some_and(|v| v <= end) {
            file.seek(SeekFrom::Start(position - start))?;
            let mut payload = vec![0; size];
            file.read_exact(&mut payload)?;
            stable(&before, &file.metadata()?)?;
            jukebox_audio::inspect(&payload)?;
            return Ok((
                Resource {
                    archive: path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .context("Invalid archive name")?
                        .into(),
                    offset: position - start,
                    size,
                },
                payload,
            ));
        }
        start = end;
    }
    anyhow::bail!("Stock audio range is not contained in one ordinary archive")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_hash_checks_are_bounded_not_per_read_and_keep_all_bytes() {
        use std::cell::Cell;
        let count = Cell::new(0);
        let bytes = vec![0x5a; 33 * 1024 * 1024 + 9];
        let expected = format!("{:X}", Sha256::digest(&bytes));
        let actual = checked_image_hash(Cursor::new(&bytes), bytes.len() as u64, || {
            count.set(count.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(count.get(), 4, "initial, two bounded batches, final");
        assert!(
            checked_image_hash(Cursor::new(&bytes), bytes.len() as u64 - 1, || Ok(())).is_err()
        );
        assert!(
            checked_image_hash(Cursor::new(&bytes), bytes.len() as u64 + 1, || Ok(())).is_err()
        );
    }
    #[test]
    fn image_hash_honors_initial_batch_and_final_refusal() {
        use std::cell::Cell;
        for fail_at in 1..=3 {
            let calls = Cell::new(0);
            let result = checked_image_hash(
                std::io::repeat(0).take(17 * 1024 * 1024),
                17 * 1024 * 1024,
                || {
                    calls.set(calls.get() + 1);
                    ensure!(calls.get() != fail_at, "cancelled");
                    Ok(())
                },
            );
            assert!(result.is_err());
            assert_eq!(calls.get(), fail_at);
        }
    }
    #[test]
    fn lookup_is_fixed_unique_and_bounded() {
        let r = lookup(Cursor::new(b"other,0A,1,24\nmusic.iff,0B,983242178,3850\n")).unwrap();
        assert_eq!(r.offset, 983242178);
        for text in [
            "music.iff,../0B,0,24\n",
            "music.iff,0B,0,0\n",
            "music.iff,0B,0,24\nMUSIC.IFF,0B,1,24\n",
            "other,0B,0,24\n",
        ] {
            assert!(lookup(Cursor::new(text)).is_err());
        }
        assert!(lookup(Cursor::new(vec![b'x'; 8193])).is_err());
    }
    use std::io::Cursor;
    #[test]
    #[ignore = "Explicit installed Version 2 disk audit; no game/process access"]
    fn actual_version2_stock_music_and_native_container() {
        let root = Path::new("C:/Program Files (x86)/Steam/steamapps/common/NBA 2K27");
        let stock = read_stock(root, || Ok(())).unwrap();
        assert!(!stock.music.tracks.is_empty());
        let (r, a) = read_stock_sample(root, 1).unwrap();
        let key = stock
            .music
            .tracks
            .iter()
            .filter(|t| t.collection == 1)
            .max_by_key(|t| t.bank)
            .unwrap()
            .key();
        let (tail, tail_bytes) = read_stock_native(root, &stock.music, &key).unwrap();
        let tail_audio = jukebox_audio::inspect(&tail_bytes).unwrap();
        println!(
            "{}",
            serde_json::json!({"schema":"nba2k27-v2-jukebox-disk-proof-v1","imageSha256":IMAGE_SHA,"musicResource":stock.resource,"packedSha256":stock.packed_sha256,"decodedSha256":stock.decoded_sha256,"tracks":stock.music.tracks.len(),"mainBanks":stock.music.tracks.iter().filter(|t|t.collection==1).map(|t|t.bank).collect::<Vec<_>>(),"collections":stock.music.tracks.iter().fold(std::collections::BTreeMap::<u16,usize>::new(),|mut m,t|{*m.entry(t.collection).or_default()+=1;m}),"stockAudioResource":r,"stockAudioFrames":a.frames,"stockAudioCodec":format!("{:?}",a.codec),"stockTailKey":key,"stockTailResource":tail,"stockTailCodec":format!("{:?}",tail_audio.codec),"gameProcessAccess":false,"gameFilesChanged":false,"liveValidated":false})
        );
    }
}
