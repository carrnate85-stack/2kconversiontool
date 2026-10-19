//! Typed, immutable Jukebox artwork preparation. Nothing is published to disk
//! or the provider here. A selected song's native TLD borrows only an exact
//! stock cover resource proved by the current manifest; unrelated mods, aliases
//! and cover names used by non-overridden songs are never borrowed.
//!
//! Native integration must consume `Prepared` as a separate <=71-artwork /
//! <=142-exact-route capability. Do not convert these paths into generic mutable
//! aliases or broaden the four ordinary OVERRIDE slots. The stock-first loader
//! needs a separately authenticated exact route permit before this is enabled.
#![allow(dead_code)]
use crate::jukebox_catalog::{self as catalog, Music};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
    sync::Arc,
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
};
pub(crate) const MAX_ARTWORK: usize = 71;
pub(crate) const MAX_ROUTES: usize = 142;
pub(crate) const NATIVE_BYTES: usize = 32800;
const MAX_PERMIT: usize = 24 * 1024;
const PERMIT_SCHEMA: &str = "nba2k27-jukebox-artwork-routes-v1";

fn cover(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
/// Exact manifest grammar, not a directory/prefix permission. Full paths and
/// leaf paths are separately enumerated in the final bounded publication.
pub(crate) fn logical_cover(path: &str) -> Option<&str> {
    let p = path.split('/').collect::<Vec<_>>();
    if p.len() != 3
        || p[0] != "shared"
        || p[1].len() != 2
        || !p[1]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let leaf = p[2].split('.').collect::<Vec<_>>();
    if leaf.len() != 3
        || !cover(leaf[0])
        || leaf[1].len() != 16
        || leaf[2] != "tld"
        || !leaf[1]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(leaf[0])
}
pub(crate) fn validate_native(data: &[u8]) -> Result<()> {
    ensure!(
        data.len() == NATIVE_BYTES
            && data.starts_with(b"TLD ")
            && data[10] == 71
            && data[11] == 1
            && u16::from_le_bytes(data[16..18].try_into()?) == 256
            && u16::from_le_bytes(data[18..20].try_into()?) == 256
            && u32::from_le_bytes(data[24..28].try_into()?) == 32768
            && u32::from_le_bytes(data[28..32].try_into()?) == 32768,
        "Jukebox artwork must be a validated 256x256 BC1 TLD"
    );
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot {
    name: String,
    logical: String,
}
pub(crate) struct Slots {
    stock_sha256: String,
    entries: Vec<Slot>,
}
impl Slots {
    pub fn read(game: &Path, stock: &Music, check: impl Fn() -> Result<()>) -> Result<Self> {
        check()?;
        let _pins = crate::loader_files::Directories::pin(game)?;
        let file: File = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(game.join("manifest"))?;
        let before = file.metadata()?;
        let identity = crate::file_identity::Stable::read(&file)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe {
            GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
        }
        ensure!(
            before.is_file()
                && before.file_attributes() & 0x400 == 0
                && info.nNumberOfLinks == 1
                && before.len() <= 256 * 1024 * 1024,
            "Jukebox cover manifest is not a bounded ordinary file"
        );
        let result = Self::parse(BufReader::new(&file), stock, &check)?;
        let after = file.metadata()?;
        ensure!(
            before.len() == after.len()
                && before.last_write_time() == after.last_write_time()
                && identity == crate::file_identity::Stable::read(&file)?,
            "Jukebox cover manifest changed during inspection"
        );
        check()?;
        Ok(result)
    }
    pub fn parse(
        mut reader: impl BufRead,
        stock: &Music,
        check: impl Fn() -> Result<()>,
    ) -> Result<Self> {
        stock.validate()?;
        // Actual stock identities, not the obsolete bank>71 custom heuristic:
        // Version2 has93 main tracks. This list grants no audio-bank authority.
        let names = stock
            .tracks
            .iter()
            .filter(|t| t.collection == 1)
            .map(|t| t.cover.to_ascii_lowercase())
            .filter(|n| cover(n))
            .collect::<BTreeSet<_>>();
        let mut entries = BTreeMap::<String, Slot>::new();
        let (mut total, mut buffer) = (0usize, Vec::new());
        for row in 0..2_000_000 {
            if row % 2048 == 0 {
                check()?;
            }
            buffer.clear();
            let n = reader.by_ref().take(8193).read_until(b'\n', &mut buffer)?;
            if n == 0 {
                return Ok(Self {
                    stock_sha256: catalog::digest(&serde_json::to_vec(stock)?),
                    entries: entries.into_values().collect(),
                });
            }
            total = total.checked_add(n).context("Artwork manifest overflow")?;
            ensure!(
                n <= 8192 && total <= 256 * 1024 * 1024,
                "Artwork manifest row/size limit exceeded"
            );
            let line = std::str::from_utf8(&buffer)?.trim_end_matches(['\r', '\n']);
            let fields = line.split(',').collect::<Vec<_>>();
            let Some(raw) = fields.first() else { continue };
            let logical = raw.replace('\\', "/").to_ascii_lowercase();
            let Some(name) = logical_cover(&logical) else {
                continue;
            };
            if !names.contains(name) {
                continue;
            }
            ensure!(
                fields.len() == 4
                    && fields[1].len() == 2
                    && fields[1].bytes().all(|b| b.is_ascii_alphanumeric())
                    && !fields[2].is_empty()
                    && !fields[3].is_empty()
                    && fields[2].bytes().all(|b| b.is_ascii_digit())
                    && fields[3].bytes().all(|b| b.is_ascii_digit()),
                "Invalid stock cover resource row"
            );
            let offset = fields[2].parse::<u64>()?;
            let size = fields[3].parse::<u64>()?;
            ensure!(
                (24..=20 * 1024 * 1024).contains(&size) && offset.checked_add(size).is_some(),
                "Invalid stock cover resource range"
            );
            let slot = Slot {
                name: name.into(),
                logical: logical.clone(),
            };
            ensure!(
                entries.insert(name.into(), slot).is_none(),
                "Ambiguous duplicate stock cover resource; no artwork was published"
            );
        }
        anyhow::bail!("Artwork manifest row limit exceeded")
    }
    pub fn available(&self) -> usize {
        self.entries.len()
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Binding {
    pub cover: String,
    pub logical: String,
    pub leaf: String,
    pub sha256: String,
    pub size: usize,
    pub track_keys: Vec<String>,
}
pub(crate) struct Prepared {
    plan: catalog::Plan,
    bindings: Vec<Binding>,
    routes: BTreeMap<String, Arc<Vec<u8>>>,
    permit: Vec<u8>,
    permit_sha256: String,
}
impl Prepared {
    pub fn plan(&self) -> &catalog::Plan {
        &self.plan
    }
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
    pub fn routes(&self) -> &BTreeMap<String, Arc<Vec<u8>>> {
        &self.routes
    }
    /// Private loader bootstrap payload. A root-owned authenticated route may
    /// serve these exact bytes; loader config pins its SHA/count. This is NOT
    /// a public mutable asset endpoint or a claim that the loader is integrated.
    pub fn native_permit(&self) -> (&[u8], &str) {
        (&self.permit, &self.permit_sha256)
    }
    pub fn into_parts(
        self,
    ) -> (
        catalog::Plan,
        Vec<Binding>,
        BTreeMap<String, Arc<Vec<u8>>>,
        Vec<u8>,
        String,
    ) {
        (
            self.plan,
            self.bindings,
            self.routes,
            self.permit,
            self.permit_sha256,
        )
    }
}
/// Loader-parser reference/fixture contract. The hash must originate in the
/// frozen backend config, never from the same untrusted payload being parsed.
/// A real native implementation repeats these bounded exact-grammar checks.
pub(crate) fn verify_native_permit(data: &[u8], expected_sha: &str) -> Result<BTreeSet<String>> {
    ensure!(
        data.len() <= MAX_PERMIT
            && data.is_ascii()
            && catalog::hex(expected_sha, 64)
            && catalog::digest(data).eq_ignore_ascii_case(expected_sha),
        "Jukebox artwork permit bytes/hash differ"
    );
    let text = std::str::from_utf8(data)?;
    ensure!(
        text.ends_with('\n') && !text.contains('\r'),
        "Jukebox artwork permit must be canonical"
    );
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some(PERMIT_SCHEMA),
        "Jukebox artwork permit schema differs"
    );
    let count = lines.next().context("Jukebox artwork count is missing")?;
    let n = count.parse::<usize>()?;
    ensure!(
        n <= MAX_ARTWORK && n.to_string() == count,
        "Jukebox artwork count exceeds bounds"
    );
    let mut routes = BTreeSet::new();
    let mut covers = BTreeSet::new();
    let mut rows = 0usize;
    for row in lines {
        rows += 1;
        ensure!(rows <= n, "Jukebox artwork permit has extra rows");
        let (path, sha) = row
            .split_once('|')
            .context("Jukebox artwork permit row is invalid")?;
        let name = logical_cover(path).context("Jukebox artwork permit path is invalid")?;
        ensure!(
            catalog::hex(sha, 64)
                && sha == sha.to_ascii_lowercase()
                && covers.insert(name.to_string())
                && routes.insert(path.to_string())
                && routes.insert(path.rsplit('/').next().unwrap().to_string()),
            "Jukebox artwork permit has duplicate/invalid exact routes"
        );
    }
    ensure!(
        rows == n && routes.len() == n * 2 && routes.len() <= MAX_ROUTES,
        "Jukebox artwork permit route count differs"
    );
    Ok(routes)
}
/// Derives the normal verified audio plan itself, then patches only Cover Name.
/// `artwork` is supplied by Service::artwork_native(key), never renderer bytes.
/// `occupied` must include all provider assets, aliases, mutable and virtual
/// routes; the provider must atomically repeat the complete collision check
/// immediately before inserting the typed immutable group.
pub(crate) fn prepare(
    selected: &Music,
    stock: &Music,
    slots: &Slots,
    artwork: &BTreeMap<String, Vec<u8>>,
    occupied: &BTreeSet<String>,
    check: impl Fn() -> Result<()>,
) -> Result<Prepared> {
    check()?;
    selected.validate()?;
    stock.validate()?;
    ensure!(
        (1..=MAX_ARTWORK).contains(&selected.tracks.len()) && artwork.len() <= MAX_ARTWORK,
        "Jukebox artwork supports1–71 selected songs without truncation"
    );
    ensure!(
        slots.stock_sha256 == catalog::digest(&serde_json::to_vec(stock)?),
        "Artwork slots belong to a different stock catalog"
    );
    let selected_keys = selected
        .tracks
        .iter()
        .map(|t| t.key())
        .collect::<BTreeSet<_>>();
    ensure!(
        artwork.keys().all(|k| selected_keys.contains(k)),
        "Artwork request contains an unselected song"
    );
    let reserved = selected
        .tracks
        .iter()
        .filter(|t| !artwork.contains_key(&t.key()))
        .map(|t| t.cover.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let occupied = occupied
        .iter()
        .map(|s| s.replace('\\', "/").to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let available = slots
        .entries
        .iter()
        .filter(|s| {
            !reserved.contains(&s.name)
                && !occupied.contains(&s.logical)
                && !occupied.contains(s.logical.rsplit('/').next().unwrap())
        })
        .collect::<Vec<_>>();
    let mut plan = catalog::plan_with_stock(selected, stock)?;
    let mut bindings = Vec::<Binding>::new();
    let mut routes = BTreeMap::<String, Arc<Vec<u8>>>::new();
    let mut assigned = BTreeMap::<String, usize>::new();
    for (i, track) in selected.tracks.iter().enumerate() {
        check()?;
        let Some(data) = artwork.get(&track.key()) else {
            continue;
        };
        validate_native(data)?;
        let hash = catalog::digest(data);
        let index = if let Some(index) = assigned.get(&hash) {
            let index = *index;
            ensure!(
                routes[&bindings[index].logical].as_slice() == data,
                "Artwork content fingerprint collision"
            );
            bindings[index].track_keys.push(track.key());
            index
        } else {
            let slot=available.get(bindings.len()).context("Not enough unoccupied stock artwork slots; library artwork was retained and nothing was published")?;
            let leaf = slot.logical.rsplit('/').next().unwrap().to_string();
            let bytes = Arc::new(data.clone());
            ensure!(
                routes.insert(slot.logical.clone(), bytes.clone()).is_none()
                    && routes.insert(leaf.clone(), bytes).is_none(),
                "Artwork exact-route collision"
            );
            let index = bindings.len();
            bindings.push(Binding {
                cover: slot.name.clone(),
                logical: slot.logical.clone(),
                leaf,
                sha256: hash.clone(),
                size: data.len(),
                track_keys: vec![track.key()],
            });
            assigned.insert(hash, index);
            index
        };
        plan.music.tracks[i].cover = bindings[index].cover.clone();
    }
    ensure!(
        bindings.len() <= MAX_ARTWORK
            && routes.len() == bindings.len() * 2
            && routes.len() <= MAX_ROUTES,
        "Artwork publication exceeds typed limits"
    );
    // Canonical ASCII wire payload, bounded by71 fixed-grammar paths. Native
    // parses schema/count/rows strictly and compares this whole-payload hash
    // with trusted config before enabling exact full/leaf stock-first routes.
    let mut permit = format!("{PERMIT_SCHEMA}\n{}\n", bindings.len()).into_bytes();
    for b in &bindings {
        permit.extend(format!("{}|{}\n", b.logical, b.sha256).bytes());
    }
    ensure!(
        permit.len() <= MAX_PERMIT && permit.is_ascii(),
        "Artwork native permit exceeds bounds"
    );
    let permit_sha256 = catalog::digest(&permit);
    ensure!(
        verify_native_permit(&permit, &permit_sha256)?
            .into_iter()
            .eq(routes.keys().cloned()),
        "Generated artwork native permit does not match its immutable publication"
    );
    plan.music.validate()?;
    check()?;
    Ok(Prepared {
        plan,
        bindings,
        routes,
        permit,
        permit_sha256,
    })
}

#[cfg(test)]
#[path = "jukebox_artwork_session_tests.rs"]
mod tests;
