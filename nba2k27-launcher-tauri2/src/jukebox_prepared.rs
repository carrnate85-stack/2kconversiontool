//! Validated prepared-playlist cache for Jukebox Play preparation.
//!
//! Stored at `Cache/Jukebox/Prepared/prepared-v1.json`, separate from saved
//! library audio (`Library/Jukebox/Audio` is user content, never a cache).
//! It records only what an earlier launch fully proved:
//! - the exact retained game image hashed to the pinned Version 2 SHA and the
//!   exact manifest named the stock music.iff resource (`StockProof`);
//! - each playlist source file's kernel identity, size and last-write time
//!   when its bytes were fully hashed.
//! Reuse is per file and all-or-nothing: any schema, build, identity, size or
//! time mismatch falls back to the full read. The cache never supplies bytes;
//! every published payload is still hashed while it is staged for this launch,
//! so a stale entry can at worst fail preparation, never publish wrong audio.
//! Session receipts and owned publications stay unique to each launch.
#![allow(dead_code)]
use crate::{
    jukebox_catalog as catalog, jukebox_local,
    jukebox_v2_resources::{IMAGE_SHA, Seen, StockProof},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const SCHEMA: &str = "nba2k27-rust-jukebox-prepared-v1";
const FILE: &str = "prepared-v1.json";
const MAX_RECORD: u64 = 1024 * 1024;
const MAX_SOURCES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VerifiedFile {
    pub sha256: String,
    pub size: usize,
    pub seen: Seen,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema: String,
    image_sha256: String,
    stock: Option<StockProof>,
    sources: Vec<VerifiedFile>,
}
impl Record {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.image_sha256 == IMAGE_SHA,
            "Prepared Jukebox cache has another schema/build"
        );
        ensure!(
            self.sources.len() <= MAX_SOURCES
                && self.sources.iter().all(|s| catalog::hex(&s.sha256, 64)
                    && s.sha256 == s.sha256.to_ascii_lowercase()
                    && (54..=crate::jukebox_audio::MAX_AUDIO).contains(&s.size)
                    && s.seen.size == s.size as u64),
            "Prepared Jukebox cache entries are invalid"
        );
        Ok(())
    }
}

/// Launch-scoped view of the cache. `matches` is only consulted with a
/// retained deny-write handle; `record` is called after a full hash.
#[derive(Default)]
pub(crate) struct Verified {
    known: RefCell<BTreeMap<String, VerifiedFile>>,
    used: RefCell<BTreeMap<String, VerifiedFile>>,
    pub reused: Cell<usize>,
    pub hashed: Cell<usize>,
}
impl Verified {
    fn new(sources: Vec<VerifiedFile>) -> Self {
        Self {
            known: RefCell::new(sources.into_iter().map(|s| (s.sha256.clone(), s)).collect()),
            ..Self::default()
        }
    }
    pub fn matches(&self, file: &File, size: usize, sha: &str) -> Result<bool> {
        let sha = sha.to_ascii_lowercase();
        let Some(entry) = self.known.borrow().get(&sha).cloned() else {
            return Ok(false);
        };
        if entry.size != size || entry.seen != Seen::read(file)? {
            return Ok(false);
        }
        self.reused.set(self.reused.get() + 1);
        self.used.borrow_mut().insert(sha, entry);
        Ok(true)
    }
    pub fn record(&self, file: &File, size: usize, sha: &str) -> Result<()> {
        let entry = VerifiedFile {
            sha256: sha.to_ascii_lowercase(),
            size,
            seen: Seen::read(file)?,
        };
        self.hashed.set(self.hashed.get() + 1);
        self.known
            .borrow_mut()
            .insert(entry.sha256.clone(), entry.clone());
        self.used.borrow_mut().insert(entry.sha256.clone(), entry);
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub stock_proof_reused: bool,
    pub sources_reused: usize,
    pub sources_hashed: usize,
    pub saved: bool,
}

pub(crate) struct Cache {
    stock: Option<StockProof>,
    pub verified: Verified,
}
fn path(portable: &Path) -> PathBuf {
    portable.join("Cache/Jukebox/Prepared").join(FILE)
}
fn plain_dirs(portable: &Path) -> Result<bool> {
    let mut p = portable.to_path_buf();
    for part in ["Cache", "Jukebox", "Prepared"] {
        p.push(part);
        match fs::symlink_metadata(&p) {
            Ok(m) => ensure!(
                m.is_dir() && m.file_attributes() & 0x400 == 0,
                "Prepared Jukebox cache folder is linked/nonordinary"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}
fn read_record(portable: &Path) -> Result<Option<Record>> {
    if !plain_dirs(portable)? {
        return Ok(None);
    }
    let file = match fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path(portable))
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let m = file.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= MAX_RECORD,
        "Prepared Jukebox cache record is nonordinary or oversized"
    );
    let mut bytes = vec![];
    file.take(MAX_RECORD + 1).read_to_end(&mut bytes)?;
    let record: Record = serde_json::from_slice(&bytes)?;
    record.validate()?;
    Ok(Some(record))
}
impl Cache {
    /// Missing, unreadable, foreign-schema or other-build records are ignored
    /// (a full rebuild follows); they are never partially trusted.
    pub fn load(portable: &Path) -> Self {
        let record = read_record(portable).ok().flatten();
        Self {
            stock: record.as_ref().and_then(|r| r.stock.clone()),
            verified: Verified::new(record.map(|r| r.sources).unwrap_or_default()),
        }
    }
    pub fn stock(&self) -> Option<&StockProof> {
        self.stock.as_ref()
    }
    /// Persist only this launch's proofs (bounded to its sources). Called after
    /// a successful preparation; failure here never fails a launch.
    pub fn save(&self, portable: &Path, stock: StockProof, nonce: &str) -> Result<()> {
        ensure!(catalog::hex(nonce, 48), "Invalid prepared cache nonce");
        let record = Record {
            schema: SCHEMA.into(),
            image_sha256: IMAGE_SHA.into(),
            stock: Some(stock),
            sources: self.verified.used.borrow().values().cloned().collect(),
        };
        record.validate()?;
        let portable = portable.canonicalize()?;
        let mut dir = portable.clone();
        for part in ["Cache", "Jukebox", "Prepared"] {
            dir = jukebox_local::ensure_child(&dir, part)?;
        }
        ensure!(
            plain_dirs(&portable)?,
            "Prepared Jukebox cache folder changed"
        );
        let target = dir.join(FILE);
        if let Ok(m) = fs::symlink_metadata(&target) {
            ensure!(
                m.is_file() && m.file_attributes() & 0x400 == 0,
                "Prepared Jukebox cache record is nonordinary; left untouched"
            );
        }
        let stage = dir.join(format!(".prepared-{nonce}.tmp"));
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)?;
        f.write_all(&serde_json::to_vec(&record)?)?;
        f.sync_all()?;
        drop(f);
        let replaced = fs::rename(&stage, &target);
        if replaced.is_err() {
            let _ = fs::remove_file(&stage);
        }
        Ok(replaced?)
    }
    pub fn report(&self, stock_proof_reused: bool, saved: bool) -> Report {
        Report {
            stock_proof_reused,
            sources_reused: self.verified.reused.get(),
            sources_hashed: self.verified.hashed.get(),
            saved,
        }
    }
}
/// Remove only the cache record (never library audio). Callers must ensure no
/// owned launch is preparing; the record holds no publication authority.
pub(crate) fn clear(portable: &Path) -> Result<bool> {
    if !plain_dirs(portable)? {
        return Ok(false);
    }
    let target = path(portable);
    match fs::symlink_metadata(&target) {
        Ok(m) => {
            ensure!(
                m.is_file() && m.file_attributes() & 0x400 == 0,
                "Prepared Jukebox cache record is nonordinary; left untouched"
            );
            fs::remove_file(target)?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
#[path = "jukebox_prepared_tests.rs"]
mod tests;
