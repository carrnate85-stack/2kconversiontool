use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 16] = b"CMT-RUST-PAYLOAD";
// Fixed bootstrap surface, not the full inventory. Cached launches only stat
// these files; payload/member hashes remain on the verified repair path.
const BOOTSTRAP_FILES: &[&str] = &[
    "Backend/python_runtime/python.exe",
    "Backend/python_runtime/python312.dll",
    "Backend/python_runtime/vcruntime140.dll",
    "Backend/python_runtime/Lib/encodings/__init__.py",
    "Backend/prototype_bridge.py",
    "Backend/prototype_services.py",
    "Backend/prototype_accessories.py",
    "Backend/prototype_blender_snapshot.py",
    "Backend/prototype_blender_paths.py",
    "Backend/prototype_seams.py",
    "Backend/prototype_staging.py",
    "Backend/prototype_player_writes.py",
    "Backend/python_bridge.py",
    "Backend/app_settings.py",
    "Backend/character_manifest.py",
    "Backend/character_mod_tool.py",
    "Backend/nba2k25_support.py",
];
#[path = "runtime_cache.rs"]
mod cache;
#[path = "runtime_maintenance.rs"]
mod maintenance;
pub use maintenance::settings_write_guard;

pub struct PreparedRuntime {
    pub backend: PathBuf,
    _lease: File,
    maintenance: maintenance::Maintenance,
}

fn prepared(
    cache: &Path,
    hash: &str,
    destination: &Path,
    _progress: &mut impl FnMut(&str),
) -> Result<PreparedRuntime> {
    let lease = runtime_lease(cache, hash, false)?;
    let maintenance = maintenance::start(
        cache
            .parent()
            .context("Missing runtime data folder")?
            .to_path_buf(),
        hash.to_owned(),
        lease.try_clone()?,
    );
    Ok(PreparedRuntime {
        backend: destination.join("Backend"),
        _lease: lease,
        maintenance,
    })
}

impl PreparedRuntime {
    pub fn maintenance_events(&self) -> Vec<String> {
        self.maintenance.events()
    }
    pub fn maintenance_report(&self) -> serde_json::Value {
        self.maintenance.report()
    }
}

fn cached_runtime(cache: &Path, hash: &str) -> Option<PreparedRuntime> {
    let destination = cache.join(format!("engine-{}", &hash[..16]));
    if !usable(&destination, hash) {
        return None;
    }
    let lease = runtime_lease(cache, hash, false).ok()?;
    if !usable(&destination, hash) {
        return None;
    }
    let maintenance = maintenance::start(
        cache.parent()?.to_path_buf(),
        hash.to_owned(),
        lease.try_clone().ok()?,
    );
    Some(PreparedRuntime {
        backend: destination.join("Backend"),
        _lease: lease,
        maintenance,
    })
}

fn no_reparse(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.exists() {
            ensure!(
                fs::symlink_metadata(ancestor)?.file_attributes() & 0x400 == 0,
                "Runtime cache links are not supported: {}",
                ancestor.display()
            );
        }
    }
    Ok(())
}

struct PreparationLock {
    cache: PathBuf,
    _file: File,
}

fn preparation_lock(cache: &Path) -> Result<PreparationLock> {
    no_reparse(cache)?;
    let cache = fs::canonicalize(cache)?;
    let path = cache.join(".prepare.lock");
    no_reparse(&path)?;
    let start = Instant::now();
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&path)
        {
            Ok(file) => return Ok(PreparationLock { cache, _file: file }),
            Err(error)
                if error.raw_os_error() == Some(32)
                    && start.elapsed() < Duration::from_secs(30) =>
            {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                return Err(error).context("Another instance is preparing the conversion engine");
            }
        }
    }
}

fn bootstrap_files_ready(destination: &Path) -> bool {
    BOOTSTRAP_FILES.iter().all(|relative| {
        let path = destination.join(relative);
        no_reparse(&path).is_ok()
            && fs::symlink_metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.len() > 0 && metadata.file_attributes() & 0x400 == 0
            })
    })
}

fn usable(destination: &Path, hash: &str) -> bool {
    let ready = || -> Result<bool> {
        let path = destination.join("ready.sha256");
        no_reparse(&path)?;
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file()
                && metadata.file_attributes() & 0x400 == 0
                && metadata.len() == 64
                && hash.len() == 64,
            "Invalid runtime ready marker"
        );
        let mut marker = [0_u8; 64];
        file.read_exact(&mut marker)?;
        Ok(marker == hash.as_bytes())
    };
    ready().unwrap_or(false) && bootstrap_files_ready(destination)
}

fn runtime_lease(cache: &Path, hash: &str, exclusive: bool) -> Result<File> {
    let path = cache.join(format!(".engine-{}.lease", &hash[..16]));
    no_reparse(&path)?;
    if exclusive {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(path)
            .context("This runtime is in use. Close its other app windows before repairing it")
    } else {
        if !path.exists() {
            File::create(&path)?;
        }
        OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(path)
            .context("Could not lease the conversion runtime")
    }
}

fn install_prepared(lock: &PreparationLock, temporary: &Path, destination: &Path) -> Result<()> {
    let cache = &lock.cache;
    ensure!(
        temporary.parent() == Some(cache) && destination.parent() == Some(cache),
        "Invalid runtime installation boundary"
    );
    no_reparse(temporary)?;
    no_reparse(destination)?;
    let backup = cache::broken_path(lock, temporary)?;
    let previous = destination.exists();
    if previous {
        cache::record_broken(lock, temporary, destination, &backup)?;
        fs::rename(destination, &backup).context("Could not move the incomplete runtime aside")?;
    }
    if let Err(error) = fs::rename(temporary, destination) {
        if previous {
            fs::rename(&backup, destination)
                .context("Could not restore the previous runtime after failed repair")?;
        }
        return Err(error).context("Could not install the prepared runtime");
    }
    Ok(())
}

pub fn stamp() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

pub fn data_root() -> Result<PathBuf> {
    let test_mode = std::env::args().nth(1).is_some_and(|arg| {
        matches!(
            arg.as_str(),
            "--serve-rpc" | "--script" | "--ui-test" | "--smoke"
        )
    });
    if test_mode {
        if let Some(root) = std::env::var_os("CMT_TEST_DATA") {
            let root = PathBuf::from(root);
            ensure!(root.is_absolute(), "Test storage must be an absolute path");
            fs::create_dir_all(&root)?;
            return Ok(root);
        }
    }
    let local = std::env::var_os("LOCALAPPDATA").context("Windows Local AppData is unavailable")?;
    let root = PathBuf::from(local).join("CharacterModToolTauri");
    fs::create_dir_all(&root)?;
    Ok(root)
}

/// A retention guard, not permission to delete a tree. Inaccessible, linked or
/// oversized trees remain in place because their accessory evidence is unknown.
pub fn session_has_accessory_evidence(path: &Path) -> bool {
    fn scan(root: &Path) -> Result<bool> {
        no_reparse(root)?;
        let mut pending = vec![(root.to_path_buf(), 0_u32)];
        let mut visited = 0_usize;
        while let Some((path, depth)) = pending.pop() {
            visited += 1;
            ensure!(
                depth <= 64 && visited <= 20000,
                "Session evidence scan limit reached"
            );
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if name.starts_with(".cmt-accessory-") {
                // Even an incomplete/renamed journal or foreign file in this
                // namespace is retained; names never authorize deletion.
                return Ok(true);
            }
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Session evidence scan refuses links"
            );
            if metadata.is_dir() {
                for entry in fs::read_dir(&path)? {
                    ensure!(
                        visited + pending.len() < 20000,
                        "Session evidence scan limit reached"
                    );
                    pending.push((entry?.path(), depth + 1));
                }
            } else {
                ensure!(metadata.is_file(), "Unrecognized session entry");
                if name == "journal.json" {
                    let file = OpenOptions::new()
                        .read(true)
                        .share_mode(1)
                        .custom_flags(0x00200000)
                        .open(&path)?;
                    let metadata = file.metadata()?;
                    ensure!(
                        metadata.is_file()
                            && metadata.file_attributes() & 0x400 == 0
                            && metadata.len() <= 1024 * 1024,
                        "Unrecognized session journal"
                    );
                    let mut bytes = Vec::new();
                    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
                    ensure!(bytes.len() <= 1024 * 1024, "Oversized session journal");
                    let journal: serde_json::Value = serde_json::from_slice(&bytes)?;
                    if journal["owner"] == "cmt-accessory-install" {
                        return Ok(true);
                    }
                    ensure!(
                        journal["owner"].is_string(),
                        "Unrecognized session journal owner"
                    );
                }
            }
        }
        Ok(false)
    }
    scan(path).unwrap_or(true)
}

pub fn cleanup_dead_sessions(root: &Path) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    let sessions = root.join("Sessions");
    fs::create_dir_all(&sessions)?;
    for entry in fs::read_dir(sessions)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || entry.metadata()?.file_attributes() & 0x400 != 0 {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some((pid, timestamp)) = name.split_once('-') else {
            continue;
        };
        if timestamp.is_empty() || !timestamp.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        if pid == 0 {
            continue;
        }
        let dead = unsafe {
            match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                Ok(handle) => {
                    let _ = CloseHandle(handle);
                    false
                }
                Err(error) => error.code().0 as u32 == 0x80070057,
            }
        };
        if dead
            && !session_has_accessory_evidence(&entry.path())
            && crate::blender_recovery::recover_session(root, &entry.path()).is_ok()
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

struct SliceReader {
    file: File,
    offset: u64,
    length: u64,
    position: u64,
}
impl Read for SliceReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let limit = output
            .len()
            .min(self.length.saturating_sub(self.position) as usize);
        let read = self.file.read(&mut output[..limit])?;
        self.position += read as u64;
        Ok(read)
    }
}
impl Seek for SliceReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => value as i128,
            SeekFrom::End(value) => self.length as i128 + value as i128,
            SeekFrom::Current(value) => self.position as i128 + value as i128,
        };
        if next < 0 || next > self.length as i128 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Payload seek out of bounds",
            ));
        }
        self.position = next as u64;
        self.file
            .seek(SeekFrom::Start(self.offset + self.position))?;
        Ok(self.position)
    }
}

pub fn prepare(root: &Path, mut progress: impl FnMut(&str)) -> Result<PreparedRuntime> {
    let executable = std::env::current_exe()?;
    let mut file = File::open(&executable)?;
    let size = file.metadata()?.len();
    ensure!(size >= 64, "This EXE has no bundled conversion engine");
    file.seek(SeekFrom::End(-64))?;
    let mut footer = [0_u8; 64];
    file.read_exact(&mut footer)?;
    ensure!(
        &footer[..16] == MAGIC,
        "This is a development binary. Build the sharing EXE first."
    );
    let offset = u64::from_le_bytes(footer[16..24].try_into()?);
    let length = u64::from_le_bytes(footer[24..32].try_into()?);
    ensure!(
        offset.checked_add(length) == Some(size - 64),
        "Invalid bundled payload boundaries"
    );
    let hash = footer[32..]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let cache = root.join("Runtime");
    no_reparse(&cache)?;
    fs::create_dir_all(&cache)?;
    if let Some(runtime) = cached_runtime(&cache, &hash) {
        return Ok(runtime);
    }
    let preparation = preparation_lock(&cache)?;
    let cache = &preparation.cache;
    maintenance::retry_scratch(&preparation, root, &mut progress);
    let destination = cache.join(format!("engine-{}", &hash[..16]));
    no_reparse(&destination)?;
    if usable(&destination, &hash) {
        return prepared(&cache, &hash, &destination, &mut progress);
    }
    let repair = runtime_lease(&cache, &hash, true)?;
    progress(if destination.exists() {
        "Repairing the bundled conversion engine..."
    } else {
        "Preparing the bundled conversion engine..."
    });
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = SliceReader {
        file,
        offset,
        length,
        position: 0,
    };
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    ensure!(
        format!("{:x}", digest.finalize()) == hash,
        "The bundled conversion engine failed its checksum"
    );
    reader.seek(SeekFrom::Start(0))?;
    fs::create_dir_all(&cache)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut inventory = std::collections::BTreeMap::new();
    let mut directories = std::collections::BTreeSet::new();
    ensure!(
        archive.len() < 20000,
        "Unexpectedly large runtime inventory"
    );
    // Inventory the checksummed payload before any extractable scratch exists.
    for index in 0..archive.len() {
        let mut member = archive.by_index(index)?;
        let relative = member
            .enclosed_name()
            .context("Unsafe bundled runtime path")?;
        ensure!(
            relative.starts_with("Backend") && !member.is_symlink(),
            "Invalid bundled runtime path"
        );
        let key = relative.to_string_lossy().replace('\\', "/");
        if member.is_dir() {
            directories.insert(key.trim_end_matches('/').to_owned());
            continue;
        }
        ensure!(
            member.size() <= 512 * 1024 * 1024,
            "Oversized runtime member"
        );
        let record = cache::FileRecord {
            bytes: member.size(),
            sha256: stream_hash(&mut member)?
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        };
        ensure!(
            inventory.insert(key, record).is_none(),
            "Duplicate runtime member"
        );
    }
    for relative in BOOTSTRAP_FILES {
        ensure!(
            inventory
                .get(*relative)
                .is_some_and(|record| record.bytes > 0),
            "Bundled conversion engine is missing a bootstrap file: {relative}"
        );
    }
    let plan = cache::ScratchPlan::new(&hash, inventory.clone(), directories)?;
    let temporary = cache::create_scratch(&preparation, plan)?;
    let result = (|| -> Result<()> {
        ensure!(
            archive.len() < 20000,
            "Unexpectedly large runtime inventory"
        );
        for index in 0..archive.len() {
            let mut member = archive.by_index(index)?;
            let relative = member
                .enclosed_name()
                .context("Unsafe bundled runtime path")?;
            ensure!(relative.starts_with("Backend"), "Unexpected bundled path");
            ensure!(!member.is_symlink(), "Runtime links are not supported");
            ensure!(
                member.size() <= 512 * 1024 * 1024,
                "Oversized runtime member"
            );
            let key = relative.to_string_lossy().replace('\\', "/");
            let output = temporary.join(relative);
            no_reparse(&output)?;
            if member.is_dir() {
                fs::create_dir_all(&output)?;
                continue;
            }
            fs::create_dir_all(output.parent().context("Missing runtime parent")?)?;
            no_reparse(&output)?;
            let mut destination = OpenOptions::new()
                .write(true)
                .create_new(true)
                .share_mode(1)
                .open(output)?;
            let mut digest = Sha256::new();
            let mut written = 0_u64;
            let mut buffer = [0_u8; 65536];
            loop {
                let count = member.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                written += count as u64;
                ensure!(
                    written <= member.size(),
                    "Runtime member exceeds its declared size"
                );
                destination.write_all(&buffer[..count])?;
                digest.update(&buffer[..count]);
            }
            ensure!(written == member.size(), "Runtime member is incomplete");
            ensure!(
                format!("{:x}", digest.finalize()) == inventory[&key].sha256,
                "Runtime member changed during extraction"
            );
            destination.sync_all()?;
        }
        ensure!(
            bootstrap_files_ready(&temporary),
            "Prepared conversion engine has incomplete bootstrap files"
        );
        cache::write_inventory(&temporary, &hash, inventory)?;
        let mut ready = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(1)
            .open(temporary.join("ready.sha256"))?;
        ready.write_all(hash.as_bytes())?;
        ready.sync_all()?;
        drop(ready);
        install_prepared(&preparation, &temporary, &destination)?;
        Ok(())
    })();
    maintenance::retry_scratch(&preparation, root, &mut progress);
    result?;
    drop(repair);
    progress("Conversion engine ready.");
    prepared(&cache, &hash, &destination, &mut progress)
}

fn stream_hash(reader: &mut impl Read) -> Result<Vec<u8>> {
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let size = reader.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(hash.finalize().to_vec())
}

fn verify_members(reader: impl Read + Seek, root: &Path) -> Result<usize> {
    let root = std::path::absolute(root)?;
    no_reparse(&root)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut checked = 0;
    for index in 0..archive.len() {
        let mut member = archive.by_index(index)?;
        let relative = member.enclosed_name().context("Unsafe runtime member")?;
        ensure!(
            relative.starts_with("Backend") && !member.is_symlink(),
            "Invalid runtime member"
        );
        if member.is_dir() {
            continue;
        }
        // Package virtualization can resolve individual files to a different
        // physical root. Enclosed ZIP paths plus ancestor link checks keep the
        // logical runtime confined without rejecting that Windows overlay.
        let file = root.join(&relative);
        no_reparse(&file)?;
        let mut cached = File::open(&file)
            .with_context(|| format!("Missing runtime file: {}", relative.display()))?;
        ensure!(
            cached.metadata()?.len() == member.size(),
            "Runtime file changed: {}",
            relative.display()
        );
        ensure!(
            stream_hash(&mut cached)? == stream_hash(&mut member)?,
            "Runtime file checksum failed: {}",
            relative.display()
        );
        checked += 1;
    }
    Ok(checked)
}

pub fn verify(engine: &Path) -> Result<usize> {
    let mut file = File::open(std::env::current_exe()?)?;
    let size = file.metadata()?.len();
    ensure!(size >= 64, "Missing runtime payload");
    file.seek(SeekFrom::End(-64))?;
    let mut footer = [0_u8; 64];
    file.read_exact(&mut footer)?;
    ensure!(&footer[..16] == MAGIC, "Missing sharing EXE payload");
    let offset = u64::from_le_bytes(footer[16..24].try_into()?);
    let length = u64::from_le_bytes(footer[24..32].try_into()?);
    ensure!(
        offset.checked_add(length) == Some(size - 64),
        "Invalid runtime boundaries"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = SliceReader {
        file,
        offset,
        length,
        position: 0,
    };
    ensure!(
        stream_hash(&mut reader)? == footer[32..],
        "Embedded runtime checksum failed"
    );
    reader.seek(SeekFrom::Start(0))?;
    verify_members(
        reader,
        engine.parent().context("Missing runtime directory")?,
    )
}

#[cfg(test)]
#[path = "../tests/runtime-critical.inc"]
mod critical_tests;

#[cfg(test)]
#[path = "../tests/runtime-session-evidence.inc"]
mod session_evidence_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_runtime_install_is_replaced_within_owned_cache() {
        let cache = std::env::temp_dir().join(format!("cmt-runtime-repair-{}", stamp()));
        fs::create_dir_all(&cache).unwrap();
        let lock = preparation_lock(&cache).unwrap();
        let cache = &lock.cache;
        let old = cache.join("engine-test");
        let new = cache.join(".preparing-test");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("partial"), b"old").unwrap();
        fs::write(new.join("ready.sha256"), b"new").unwrap();
        fs::write(cache.join("settings.json"), b"preserved").unwrap();
        install_prepared(&lock, &new, &old).unwrap();
        assert_eq!(fs::read(old.join("ready.sha256")).unwrap(), b"new");
        assert!(!old.join("partial").exists());
        assert_eq!(fs::read(cache.join("settings.json")).unwrap(), b"preserved");
        let backup = fs::read_dir(cache)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".broken-")
            })
            .unwrap();
        assert_eq!(fs::read(backup.join("partial")).unwrap(), b"old");
        assert!(install_prepared(&lock, &new, &cache.parent().unwrap().join("outside")).is_err());
        let root = cache.clone();
        drop(lock);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_repair_refuses_active_shared_leases() {
        let cache = std::env::temp_dir().join(format!("cmt-runtime-lease-{}", stamp()));
        fs::create_dir_all(&cache).unwrap();
        let hash = "0123456789abcdef0123456789abcdef";
        let first = runtime_lease(&cache, hash, false).unwrap();
        let second = runtime_lease(&cache, hash, false).unwrap();
        assert!(runtime_lease(&cache, hash, true).is_err());
        drop(first);
        assert!(runtime_lease(&cache, hash, true).is_err());
        drop(second);
        let repair = runtime_lease(&cache, hash, true).unwrap();
        assert!(runtime_lease(&cache, hash, false).is_err());
        drop(repair);
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn integrity_detects_changed_cached_files() {
        let root = std::env::temp_dir().join(format!("cmt-integrity-{}", stamp()));
        fs::create_dir_all(root.join("Backend")).unwrap();
        fs::write(root.join("Backend/test.py"), b"original").unwrap();
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file("Backend/test.py", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(b"original").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert_eq!(
            verify_members(std::io::Cursor::new(&bytes), &root).unwrap(),
            1
        );
        fs::write(root.join("Backend/test.py"), b"modified").unwrap();
        assert!(verify_members(std::io::Cursor::new(&bytes), &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn slice_cannot_escape_payload() {
        let path = std::env::temp_dir().join(format!("cmt-rust-slice-{}", stamp()));
        fs::write(&path, b"prefixDATAfooter").unwrap();
        let mut file = File::open(&path).unwrap();
        file.seek(SeekFrom::Start(6)).unwrap();
        let mut reader = SliceReader {
            file,
            offset: 6,
            length: 4,
            position: 0,
        };
        let mut data = Vec::new();
        reader.read_to_end(&mut data).unwrap();
        assert_eq!(data, b"DATA");
        assert!(reader.seek(SeekFrom::Start(5)).is_err());
        assert!(reader.seek(SeekFrom::End(-5)).is_err());
        drop(reader);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn abandoned_session_cleanup_preserves_live_and_unknown_folders() {
        let root = std::env::temp_dir().join(format!("cmt-rust-cleanup-{}", stamp()));
        let live = root
            .join("Sessions")
            .join(format!("{}-123", std::process::id()));
        let unknown = root.join("Sessions/other-files");
        let dead = root.join("Sessions/4000000000-123");
        fs::create_dir_all(&live).unwrap();
        fs::create_dir_all(&unknown).unwrap();
        fs::create_dir_all(&dead).unwrap();
        cleanup_dead_sessions(&root).unwrap();
        assert!(live.exists());
        assert!(unknown.exists());
        assert!(!dead.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn abandoned_blender_returns_survive_cleanup_and_storage_failure() {
        let root = std::env::temp_dir().join(format!("cmt-rust-return-cleanup-{}", stamp()));
        let dead = root.join("Sessions/4000000000-123");
        let folder = dead.join("blender-return");
        fs::create_dir_all(&folder).unwrap();
        let returned = folder.join("player.iff");
        fs::write(&returned, b"unapplied user edits").unwrap();
        fs::write(root.join("Blender Recovery"), b"occupied").unwrap();
        cleanup_dead_sessions(&root).unwrap();
        assert!(
            returned.is_file(),
            "Failed recovery must keep the original session"
        );
        fs::remove_file(root.join("Blender Recovery")).unwrap();
        cleanup_dead_sessions(&root).unwrap();
        assert!(!dead.exists());
        let files: Vec<_> = fs::read_dir(root.join("Blender Recovery"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "iff"))
            .collect();
        assert_eq!(files.len(), 1);
        assert_eq!(fs::read(&files[0]).unwrap(), b"unapplied user edits");
        fs::remove_dir_all(root).unwrap();
    }
}
