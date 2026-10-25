use super::{no_reparse, runtime_lease, stamp, stream_hash};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    os::windows::fs::MetadataExt,
    path::{Component, Path, PathBuf, Prefix},
};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct FileRecord {
    pub(super) bytes: u64,
    pub(super) sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ScratchPlan {
    format: u32,
    directory: String,
    payload: String,
    files: BTreeMap<String, FileRecord>,
    directories: BTreeSet<String>,
}

fn scratch_name(name: &str) -> bool {
    let Some(stamp) = name
        .strip_prefix(".preparing-")
        .or_else(|| name.strip_prefix(".broken-"))
    else {
        return false;
    };
    let parts: Vec<_> = stamp.split('-').collect();
    parts.len() == 3
        && parts[..2]
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && hexadecimal(parts[2], 64)
}

fn payload_path(name: &str) -> bool {
    name.starts_with("Backend/")
        && name.split('/').count() <= 128
        && name.split('/').all(|part| {
            let stem = part
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || ["COM", "LPT"].iter().any(|prefix| {
                    stem.strip_prefix(prefix).is_some_and(|suffix| {
                        matches!(
                            suffix,
                            "1" | "2"
                                | "3"
                                | "4"
                                | "5"
                                | "6"
                                | "7"
                                | "8"
                                | "9"
                                | "\u{b9}"
                                | "\u{b2}"
                                | "\u{b3}"
                        )
                    })
                });
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !device
                && !part
                    .chars()
                    .any(|ch| ch.is_control() || "\\:<>\"|?*".contains(ch))
        })
}

impl ScratchPlan {
    fn identity(&self) -> Result<String> {
        use sha2::Digest;
        Ok(format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&(
                self.format,
                &self.payload,
                &self.files,
                &self.directories
            ))?)
        ))
    }

    pub(super) fn new(
        hash: &str,
        files: BTreeMap<String, FileRecord>,
        mut directories: BTreeSet<String>,
    ) -> Result<Self> {
        ensure!(
            hexadecimal(hash, 64) && !files.is_empty() && files.len() < 20000,
            "Invalid scratch payload inventory"
        );
        let mut names = HashSet::new();
        for (name, record) in &files {
            ensure!(
                payload_path(name)
                    && names.insert(name.to_lowercase())
                    && record.bytes <= 512 * 1024 * 1024
                    && hexadecimal(&record.sha256, 64),
                "Invalid scratch payload entry"
            );
            for parent in Path::new(name)
                .ancestors()
                .skip(1)
                .filter(|path| !path.as_os_str().is_empty())
            {
                directories.insert(parent.to_string_lossy().replace('\\', "/"));
            }
        }
        for directory in directories.clone() {
            for parent in Path::new(&directory)
                .ancestors()
                .skip(1)
                .filter(|path| !path.as_os_str().is_empty())
            {
                directories.insert(parent.to_string_lossy().replace('\\', "/"));
            }
        }
        for directory in &directories {
            ensure!(
                (directory == "Backend" || payload_path(directory))
                    && names.insert(directory.to_lowercase()),
                "Invalid scratch payload directory"
            );
        }
        ensure!(
            directories.len() + files.len() < 20000,
            "Oversized scratch inventory"
        );
        let inventory = serde_json::to_vec(&Inventory {
            payload: hash.into(),
            files: files.clone(),
            metadata: None,
        })?;
        let mut plan = Self {
            format: 1,
            directory: String::new(),
            payload: hash.into(),
            files,
            directories,
        };
        for (name, bytes) in [
            ("inventory.json", inventory.as_slice()),
            ("ready.sha256", hash.as_bytes()),
        ] {
            use sha2::Digest;
            plan.files.insert(
                name.into(),
                FileRecord {
                    bytes: bytes.len() as u64,
                    sha256: format!("{:x}", sha2::Sha256::digest(bytes)),
                },
            );
        }
        Ok(plan)
    }

    fn validate(&self, name: &str) -> Result<()> {
        ensure!(
            self.format == 1 && self.directory == name && scratch_name(name),
            "Unrecognized scratch ownership"
        );
        ensure!(
            name.ends_with(&format!("-{}", self.identity()?)),
            "Scratch ownership journal changed"
        );
        let mut files = self.files.clone();
        let inventory = files
            .remove("inventory.json")
            .context("Missing scratch inventory proof")?;
        let ready = files
            .remove("ready.sha256")
            .context("Missing scratch ready proof")?;
        let expected = Self::new(&self.payload, files, self.directories.clone())?;
        for (actual, expected) in [
            (&inventory, &expected.files["inventory.json"]),
            (&ready, &expected.files["ready.sha256"]),
        ] {
            ensure!(
                actual.bytes == expected.bytes && actual.sha256 == expected.sha256,
                "Scratch metadata proof differs"
            );
        }
        Ok(())
    }
}

fn scratch_journal(cache: &Path, name: &str) -> PathBuf {
    cache.join(format!("{name}.scratch.json"))
}

fn write_scratch_plan(lock: &super::PreparationLock, plan: &ScratchPlan) -> Result<()> {
    use std::io::Write;
    plan.validate(&plan.directory)?;
    let bytes = serde_json::to_vec(plan)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Oversized scratch ownership journal"
    );
    let path = scratch_journal(&lock.cache, &plan.directory);
    no_reparse(&path)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn create_scratch(
    lock: &super::PreparationLock,
    mut plan: ScratchPlan,
) -> Result<PathBuf> {
    plan.directory = format!(".preparing-{}-{}", stamp(), plan.identity()?);
    let root = lock.cache.join(&plan.directory);
    write_scratch_plan(lock, &plan)?;
    fs::create_dir(&root)?;
    Ok(root)
}

// Pin verified entries against writes/replacement, then delete the exact handle,
// not a pathname that could have been replaced after verification.
fn pinned(path: &Path) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    no_reparse(path)?;
    let file = fs::OpenOptions::new()
        .access_mode(0x80000000 | 0x00010000)
        .share_mode(1)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)?;
    ensure!(
        file.metadata()?.file_attributes() & 0x400 == 0,
        "Scratch cleanup refuses links"
    );
    default_streams_only(path)?;
    Ok(file)
}

fn default_streams_only(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[repr(C)]
    struct StreamData {
        bytes: i64,
        name: [u16; 296],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn FindFirstStreamW(
            path: *const u16,
            level: u32,
            data: *mut StreamData,
            flags: u32,
        ) -> *mut std::ffi::c_void;
        fn FindNextStreamW(handle: *mut std::ffi::c_void, data: *mut StreamData) -> i32;
        fn FindClose(handle: *mut std::ffi::c_void) -> i32;
    }
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut data = StreamData {
        bytes: 0,
        name: [0; 296],
    };
    let handle = unsafe { FindFirstStreamW(path.as_ptr(), 0, &mut data, 0) };
    if handle == -1_isize as *mut std::ffi::c_void {
        let error = std::io::Error::last_os_error();
        ensure!(
            error.raw_os_error() == Some(38),
            "Cannot verify scratch streams: {error}"
        );
        return Ok(());
    }
    let result = (|| -> Result<()> {
        loop {
            let end = data
                .name
                .iter()
                .position(|ch| *ch == 0)
                .context("Invalid scratch stream name")?;
            ensure!(
                String::from_utf16(&data.name[..end])? == "::$DATA",
                "Scratch contains an unknown alternate data stream"
            );
            if unsafe { FindNextStreamW(handle, &mut data) } == 0 {
                let error = std::io::Error::last_os_error();
                ensure!(
                    error.raw_os_error() == Some(38),
                    "Cannot verify scratch streams: {error}"
                );
                break;
            }
        }
        Ok(())
    })();
    unsafe {
        FindClose(handle);
    }
    result
}

fn delete_pinned(file: fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            class: u32,
            information: *const std::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    let delete = 1_i32;
    let result = unsafe {
        SetFileInformationByHandle(file.as_raw_handle(), 4, (&delete as *const i32).cast(), 4)
    };
    ensure!(
        result != 0,
        "Scratch deletion failed: {}",
        std::io::Error::last_os_error()
    );
    drop(file);
    Ok(())
}

fn read_scratch_plan(cache: &Path, name: &str) -> Result<(ScratchPlan, fs::File)> {
    use std::io::Read;
    let mut file = pinned(&scratch_journal(cache, name))?;
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= 4 * 1024 * 1024,
        "Unrecognized scratch ownership journal"
    );
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let plan: ScratchPlan = serde_json::from_slice(&bytes)?;
    plan.validate(name)?;
    Ok((plan, file))
}

pub(super) fn record_broken(
    lock: &super::PreparationLock,
    temporary: &Path,
    destination: &Path,
    backup: &Path,
) -> Result<()> {
    let name = temporary
        .file_name()
        .context("Missing scratch name")?
        .to_str()
        .context("Invalid scratch name")?;
    let Ok((mut plan, _journal)) = read_scratch_plan(&lock.cache, name) else {
        return Ok(());
    };
    // Older/uninventoried or changed inventories cannot establish backup ownership.
    let proof = (|| -> Result<()> {
        let mut file = pinned(&destination.join("inventory.json"))?;
        let expected = &plan.files["inventory.json"];
        ensure!(
            file.metadata()?.len() == expected.bytes,
            "Different runtime inventory"
        );
        let hash: String = stream_hash(&mut file)?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        ensure!(hash == expected.sha256, "Different runtime inventory");
        Ok(())
    })();
    if proof.is_ok() {
        plan.directory = backup
            .file_name()
            .context("Missing backup name")?
            .to_str()
            .context("Invalid backup name")?
            .into();
        write_scratch_plan(lock, &plan)?;
    }
    Ok(())
}

pub(super) fn broken_path(lock: &super::PreparationLock, temporary: &Path) -> Result<PathBuf> {
    let name = temporary
        .file_name()
        .context("Missing scratch name")?
        .to_str()
        .context("Invalid scratch name")?;
    let suffix = match read_scratch_plan(&lock.cache, name) {
        Ok((plan, _journal)) => format!("-{}", plan.identity()?),
        Err(_) => String::new(),
    };
    Ok(lock.cache.join(format!(".broken-{}{suffix}", stamp())))
}

fn remove_scratch(lock: &super::PreparationLock, name: &str) -> Result<()> {
    let (plan, journal) = read_scratch_plan(&lock.cache, name)?;
    let root = lock.cache.join(name);
    match fs::symlink_metadata(&root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return delete_pinned(journal);
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let mut pending = vec![root.clone()];
    while let Some(path) = pending.pop() {
        ensure!(
            files.len() + dirs.len() + pending.len() < 20000,
            "Oversized scratch tree"
        );
        let mut file = pinned(&path)?;
        let metadata = file.metadata()?;
        let relative = relative(&root, &path)?;
        if metadata.is_dir() {
            ensure!(
                relative.is_empty() || plan.directories.contains(&relative),
                "Scratch contains an unknown directory"
            );
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
            dirs.push((path, file));
        } else {
            let expected = plan
                .files
                .get(&relative)
                .context("Scratch contains an unknown file")?;
            ensure!(
                metadata.is_file() && metadata.len() == expected.bytes,
                "Scratch file size changed: {relative}"
            );
            let hash: String = stream_hash(&mut file)?
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            ensure!(
                hash == expected.sha256,
                "Scratch file checksum changed: {relative}"
            );
            files.push(file);
        }
    }
    for file in files {
        delete_pinned(file)?;
    }
    dirs.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (_, directory) in dirs {
        delete_pinned(directory)?;
    }
    delete_pinned(journal)
}

pub(super) fn retry_scratch(
    lock: &super::PreparationLock,
    settings: &Path,
    progress: &mut impl FnMut(&str),
) {
    let result = (|| -> Result<()> {
        no_reparse(&lock.cache)?;
        let references = references(settings)?;
        let mut names = BTreeSet::new();
        for entry in fs::read_dir(&lock.cache)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            let name = name.strip_suffix(".scratch.json").unwrap_or(&name);
            if name.starts_with(".preparing-") || name.starts_with(".broken-") {
                names.insert(name.to_owned());
            }
        }
        for name in names {
            let root = lock.cache.join(&name);
            let result = if references
                .iter()
                .any(|(_, path)| path.starts_with(&root) || root.starts_with(path))
            {
                Err(anyhow::anyhow!("Scratch is referenced by settings"))
            } else {
                remove_scratch(lock, &name)
            };
            match result {
                Ok(()) => progress(&format!(
                    "Removed verified runtime scratch: {}",
                    root.display()
                )),
                Err(error) => progress(&format!(
                    "Preserved runtime scratch: {} ({error:#})",
                    root.display()
                )),
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        progress(&format!(
            "Preserved runtime scratch; cleanup unavailable: {error:#}"
        ));
    }
}
#[derive(Serialize, Deserialize)]
struct Inventory {
    payload: String,
    files: BTreeMap<String, FileRecord>,
    #[serde(skip)]
    metadata: Option<FileRecord>,
}
pub(super) fn write_inventory(
    root: &Path,
    hash: &str,
    files: BTreeMap<String, FileRecord>,
) -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(1)
        .open(root.join("inventory.json"))?;
    serde_json::to_writer(
        &file,
        &Inventory {
            payload: hash.into(),
            files,
            metadata: None,
        },
    )?;
    file.sync_all()?;
    Ok(())
}

fn inventory(root: &Path, hash: Option<&str>) -> Result<Inventory> {
    use sha2::Digest;
    use std::io::Read;
    let file = root.join("inventory.json");
    no_reparse(&file)?;
    ensure!(
        fs::metadata(&file)?.len() <= 4 * 1024 * 1024,
        "Unrecognized runtime inventory"
    );
    let mut bytes = Vec::new();
    fs::File::open(file)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Unrecognized runtime inventory"
    );
    let mut inventory: Inventory = serde_json::from_slice(&bytes)?;
    ensure!(
        hexadecimal(&inventory.payload, 64)
            && hash.is_none_or(|hash| inventory.payload == hash)
            && !inventory.files.is_empty()
            && inventory.files.len() < 20000,
        "Runtime inventory identity differs"
    );
    inventory.metadata = Some(FileRecord {
        bytes: bytes.len() as u64,
        sha256: format!("{:x}", sha2::Sha256::digest(&bytes)),
    });
    Ok(inventory)
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn verify_tree(
    root: &Path,
    cache: &Path,
    snapshot: &Inventory,
    complete: bool,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    confined_tree(root, cache)?;
    let mut expected_dirs = HashSet::from([String::new()]);
    for name in snapshot.files.keys() {
        for parent in Path::new(name).ancestors().skip(1) {
            expected_dirs.insert(parent.to_string_lossy().replace('\\', "/"));
        }
    }
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let mut pending = vec![root.to_path_buf()];
    let mut matched = 0;
    while let Some(path) = pending.pop() {
        let name = relative(root, &path)?;
        if path.is_dir() {
            let generated = path.file_name().is_some_and(|name| name == "__pycache__")
                && expected_dirs.contains(&relative(root, path.parent().unwrap())?);
            ensure!(
                expected_dirs.contains(&name) || generated,
                "Runtime contains an untracked directory"
            );
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
            dirs.push(path);
            continue;
        }
        if matches!(name.as_str(), "ready.sha256" | "inventory.json") {
            continue;
        }
        if let Some(record) = snapshot.files.get(&name) {
            ensure!(
                fs::metadata(&path)?.len() == record.bytes,
                "Runtime file was changed"
            );
            let digest = stream_hash(&mut fs::File::open(&path)?)?;
            let digest: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
            ensure!(digest == record.sha256, "Runtime file checksum changed");
            matched += 1;
        } else {
            let parent = path.parent().unwrap();
            let generated = parent.file_name().is_some_and(|name| name == "__pycache__")
                && path
                    .extension()
                    .is_some_and(|ext| ext == "pyc" || ext == "pyo")
                && fs::metadata(&path)?.len() <= 64 * 1024 * 1024
                && snapshot.files.contains_key(&relative(
                    root,
                    &parent.parent().unwrap().join(format!(
                        "{}.py",
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .split('.')
                            .next()
                            .unwrap()
                    )),
                )?);
            ensure!(generated, "Runtime contains an untracked file");
        }
        files.push(path);
    }
    ensure!(
        !complete || matched == snapshot.files.len(),
        "Runtime files are missing"
    );
    Ok((files, dirs))
}

fn remove_retired(root: &Path, cache: &Path, snapshot: &Inventory) -> Result<()> {
    remove_retired_with(root, cache, snapshot, || Ok(()), |_| Ok(()))
}

fn check_retired_file(file: &mut fs::File, expected: &FileRecord, name: &str) -> Result<()> {
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() == expected.bytes,
        "Retired runtime file size changed: {name}"
    );
    let digest: String = stream_hash(file)?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ensure!(
        digest == expected.sha256,
        "Retired runtime checksum changed: {name}"
    );
    Ok(())
}

fn retired_members_unchanged(
    dirs: &[(PathBuf, fs::File)],
    allowed: &BTreeSet<PathBuf>,
) -> Result<()> {
    for (dir, _) in dirs {
        for entry in fs::read_dir(dir)? {
            ensure!(
                allowed.contains(&entry?.path()),
                "Retired runtime gained an unverified entry"
            );
        }
    }
    Ok(())
}

// Callbacks are no-ops in production and allow tests to exercise the exact
// post-verification boundary without changing cleanup ordering.
fn remove_retired_with(
    root: &Path,
    cache: &Path,
    snapshot: &Inventory,
    after_verified: impl FnOnce() -> Result<()>,
    after_file_deleted: impl Fn(&Path) -> Result<()>,
) -> Result<()> {
    use sha2::Digest;
    ensure!(
        root.parent() == Some(cache),
        "Runtime cleanup escaped its cache"
    );
    no_reparse(cache)?;
    // Validate expected paths without learning ownership from the retired tree.
    let plan = ScratchPlan::new(&snapshot.payload, snapshot.files.clone(), BTreeSet::new())?;
    let metadata = snapshot
        .metadata
        .as_ref()
        .context("Missing original runtime inventory identity")?;
    let ready_record = FileRecord {
        bytes: snapshot.payload.len() as u64,
        sha256: format!("{:x}", sha2::Sha256::digest(snapshot.payload.as_bytes())),
    };
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let (mut ready, mut inventory) = (None, None);
    let mut allowed = BTreeSet::new();
    let mut pending = vec![(root.to_path_buf(), 0)];
    while let Some((path, depth)) = pending.pop() {
        ensure!(
            depth <= 128 && allowed.len() + pending.len() < 20000,
            "Oversized retired runtime tree"
        );
        let mut file = pinned(&path)?;
        let attributes = file.metadata()?;
        let name = relative(root, &path)?;
        allowed.insert(path.clone());
        if attributes.is_dir() {
            let generated = path.file_name().is_some_and(|name| name == "__pycache__")
                && plan.directories.contains(&relative(
                    root,
                    path.parent().context("Missing bytecode parent")?,
                )?);
            ensure!(
                name.is_empty() || plan.directories.contains(&name) || generated,
                "Retired runtime contains an unknown directory"
            );
            for entry in fs::read_dir(&path)? {
                ensure!(
                    allowed.len() + pending.len() < 20000,
                    "Oversized retired runtime tree"
                );
                pending.push((entry?.path(), depth + 1));
            }
            dirs.push((path, file));
        } else if name == "inventory.json" {
            check_retired_file(&mut file, metadata, &name)?;
            inventory = Some(file);
        } else if name == "ready.sha256" {
            check_retired_file(&mut file, &ready_record, &name)?;
            ready = Some(file);
        } else {
            ensure!(attributes.is_file(), "Unrecognized retired runtime entry");
            if let Some(expected) = snapshot.files.get(&name) {
                check_retired_file(&mut file, expected, &name)?;
            } else {
                let parent = path.parent().context("Missing bytecode parent")?;
                let source = parent
                    .parent()
                    .context("Missing bytecode source")?
                    .join(format!(
                        "{}.py",
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .split('.')
                            .next()
                            .unwrap()
                    ));
                ensure!(
                    parent.file_name().is_some_and(|name| name == "__pycache__")
                        && path
                            .extension()
                            .is_some_and(|ext| ext == "pyc" || ext == "pyo")
                        && attributes.len() <= 64 * 1024 * 1024
                        && snapshot.files.contains_key(&relative(root, &source)?),
                    "Retired runtime contains an unknown file"
                );
            }
            files.push((path, file));
        }
    }
    let inventory = inventory.context("Retired runtime inventory is missing")?;
    after_verified()?;
    // Directory pins prevent replacement, but Windows can still add children.
    // Check membership again before deleting any of the verified handles.
    retired_members_unchanged(&dirs, &allowed)?;
    // Sharing restrictions are per stream on Windows. A base-stream pin does
    // not prevent another process from adding an ADS after the first check.
    for (path, _) in files.iter().chain(dirs.iter()) {
        default_streams_only(path)?;
    }
    default_streams_only(&root.join("inventory.json"))?;
    if ready.is_some() {
        default_streams_only(&root.join("ready.sha256"))?;
    }
    for (path, file) in files {
        default_streams_only(&path)?;
        delete_pinned(file)?;
        allowed.remove(&path);
        after_file_deleted(&path)?;
    }
    retired_members_unchanged(&dirs, &allowed)?;
    dirs.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    let root_pin = dirs.pop().context("Retired runtime root is missing")?;
    for (path, directory) in dirs {
        default_streams_only(&path)?;
        delete_pinned(directory)?;
    }
    // Keep the retry proof until payload files and child directories are gone.
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name();
        ensure!(
            name == "inventory.json" || (name == "ready.sha256" && ready.is_some()),
            "Retired runtime gained an unverified entry"
        );
    }
    default_streams_only(root)?;
    default_streams_only(&root.join("inventory.json"))?;
    if let Some(file) = ready {
        default_streams_only(&root.join("ready.sha256"))?;
        delete_pinned(file)?;
    }
    delete_pinned(inventory)?;
    delete_pinned(root_pin.1)?;
    Ok(())
}

fn references(settings: &Path) -> Result<Vec<(String, PathBuf)>> {
    let bytes = match fs::read(settings) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let values: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut paths = Vec::new();
    for key in ["output_dir", "head_swap_tool", "mesh_data_transfer"] {
        let Some(value) = values[key].as_str().filter(|value| !value.is_empty()) else {
            continue;
        };
        let path = PathBuf::from(value);
        ensure!(
            path.is_absolute()
                && !matches!(path.components().next(),
            Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..))),
            "Runtime cleanup cannot safely resolve configured paths"
        );
        let resolved = path.ancestors().find_map(|parent| {
            fs::canonicalize(parent)
                .ok()
                .map(|resolved| resolved.join(path.strip_prefix(parent).unwrap()))
        });
        paths.push((
            key.to_owned(),
            resolved.ok_or_else(|| anyhow::anyhow!("Configured path is unavailable"))?,
        ));
    }
    Ok(paths)
}

fn referenced(root: &Path, paths: &[(String, PathBuf)]) -> bool {
    paths.iter().any(|(key, path)| {
        if !path.starts_with(root) && !root.starts_with(path) {
            return false;
        }
        let managed = path
            .strip_prefix(root)
            .ok()
            .map(|path| path.to_string_lossy().replace('\\', "/").to_lowercase());
        !matches!(
            (key.as_str(), managed.as_deref()),
            (
                "head_swap_tool",
                Some("backend/blender_tools/nba_character_headswap")
            ) | (
                "mesh_data_transfer",
                Some("backend/blender_tools/mesh_data_transfer")
            )
        )
    })
}

fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn retired(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(".retired-") else {
        return false;
    };
    let parts: Vec<_> = rest.split('-').collect();
    parts.len() == 3
        && hexadecimal(parts[0], 16)
        && parts[1..]
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn confined_tree(root: &Path, cache: &Path) -> Result<()> {
    ensure!(
        root.parent() == Some(cache),
        "Runtime cleanup escaped its cache"
    );
    no_reparse(root)?;
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut count = 0;
    while let Some((path, depth)) = pending.pop() {
        count += 1;
        ensure!(
            depth <= 128 && count <= 20000,
            "Unrecognized runtime cleanup tree"
        );
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "Runtime cleanup refuses links"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path)? {
                ensure!(
                    count + pending.len() < 20000,
                    "Unrecognized runtime cleanup inventory"
                );
                pending.push((entry?.path(), depth + 1));
            }
        }
    }
    Ok(())
}

// Called under the preparation lock, after the current runtime is leased.
pub(super) fn prune(cache: &Path, current: &str, settings: &Path) -> Result<usize> {
    ensure!(hexadecimal(current, 64), "Invalid runtime cleanup identity");
    no_reparse(cache)?;
    let cache = fs::canonicalize(cache)?;
    let references = references(settings)?;
    let mut candidates = Vec::new();
    let mut removed = 0;
    for entry in fs::read_dir(&cache)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if retired(&name) {
            if !referenced(&path, &references) {
                if let Ok(snapshot) = inventory(&path, None) {
                    if snapshot
                        .payload
                        .starts_with(&name[".retired-".len()..".retired-".len() + 16])
                    {
                        if let Ok(_lease) = runtime_lease(&cache, &snapshot.payload, true) {
                            if remove_retired(&path, &cache, &snapshot).is_ok() {
                                removed += 1;
                            }
                        }
                    }
                }
            }
            continue;
        }
        let Some(prefix) = name
            .strip_prefix("engine-")
            .filter(|prefix| hexadecimal(prefix, 16))
        else {
            continue;
        };
        let marker = path.join("ready.sha256");
        if no_reparse(&marker).is_err()
            || fs::metadata(&marker).map_or(true, |metadata| metadata.len() != 64)
        {
            continue;
        }
        let Ok(hash) = fs::read_to_string(&marker) else {
            continue;
        };
        if !hexadecimal(&hash, 64)
            || !hash.starts_with(prefix)
            || !path.join("Backend/python_runtime/python.exe").is_file()
            || !path.join("Backend/prototype_bridge.py").is_file()
        {
            continue;
        }
        if hash == current {
            continue;
        }
        if inventory(&path, Some(&hash)).is_err() {
            continue;
        }
        let Ok(modified) = fs::metadata(marker).and_then(|metadata| metadata.modified()) else {
            continue;
        };
        candidates.push((modified, path, hash));
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, path, hash) in candidates.into_iter().skip(1) {
        let Ok(lease) = runtime_lease(&cache, &hash, true) else {
            continue;
        };
        if referenced(&path, &references) {
            continue;
        }
        let Ok(snapshot) = inventory(&path, Some(&hash)) else {
            continue;
        };
        if verify_tree(&path, &cache, &snapshot, true).is_err() {
            continue;
        }
        let destination = cache.join(format!(".retired-{}-{}", &hash[..16], stamp()));
        // Rename first so interrupted deletion can never leave a usable partial runtime.
        if fs::rename(&path, &destination).is_ok()
            && remove_retired(&destination, &cache, &snapshot).is_ok()
        {
            removed += 1;
        }
        drop(lease);
    }
    Ok(removed)
}

#[cfg(test)]
#[path = "../tests/runtime-scratch.inc"]
mod scratch_tests;

#[cfg(test)]
#[path = "../tests/runtime-retirement.inc"]
mod retirement_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;
    use std::{
        fs::{FileTimes, OpenOptions},
        os::windows::fs::OpenOptionsExt,
        time::{Duration, UNIX_EPOCH},
    };

    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn prune(&self, hash: &str) -> usize {
            prune(&self.0, hash, &self.0.join("settings.json")).unwrap()
        }
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("cmt-retention-{}", stamp()));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn engine(&self, digit: char, modified: u64) -> (String, std::path::PathBuf) {
            let hash = digit.to_string().repeat(64);
            let path = self.0.join(format!("engine-{}", &hash[..16]));
            fs::create_dir_all(path.join("Backend/python_runtime")).unwrap();
            fs::write(
                path.join("Backend/python_runtime/python.exe"),
                b"test marker, never executed",
            )
            .unwrap();
            fs::write(
                path.join("Backend/prototype_bridge.py"),
                b"test marker, never executed",
            )
            .unwrap();
            let ready = path.join("ready.sha256");
            fs::write(&ready, &hash).unwrap();
            let mut files = BTreeMap::new();
            for name in [
                "Backend/python_runtime/python.exe",
                "Backend/prototype_bridge.py",
            ] {
                let bytes = fs::read(path.join(name)).unwrap();
                files.insert(
                    name.into(),
                    FileRecord {
                        bytes: bytes.len() as u64,
                        sha256: format!("{:x}", sha2::Sha256::digest(&bytes)),
                    },
                );
            }
            write_inventory(&path, &hash, files).unwrap();
            OpenOptions::new()
                .write(true)
                .open(ready)
                .unwrap()
                .set_times(
                    FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(modified)),
                )
                .unwrap();
            (hash, path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn keeps_current_previous_active_and_unknown_folders() {
        let fixture = Fixture::new();
        let (current, current_path) = fixture.engine('a', 300);
        let (_, previous) = fixture.engine('b', 200);
        let (active_hash, active) = fixture.engine('c', 100);
        let (_, old) = fixture.engine('d', 50);
        let unknown = fixture.0.join("my-blender-tools");
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join("keep"), b"user tools").unwrap();
        let (_, malformed) = fixture.engine('e', 400);
        fs::write(malformed.join("ready.sha256"), b"invalid").unwrap();
        let lease = runtime_lease(&fixture.0, &active_hash, false).unwrap();
        assert_eq!(fixture.prune(&current), 1);
        for path in [&current_path, &previous, &active, &unknown, &malformed] {
            assert!(path.is_dir());
        }
        assert!(!old.exists());
        drop(lease);
        assert_eq!(fixture.prune(&current), 1);
        assert!(!active.exists());
        assert_eq!(fs::read(unknown.join("keep")).unwrap(), b"user tools");
    }

    #[test]
    fn interrupted_retirement_is_retried_without_affecting_live_runtime() {
        let fixture = Fixture::new();
        let (current, live) = fixture.engine('a', 300);
        let pending = fixture.0.join(format!(".retired-{}-1-123", "b".repeat(16)));
        let (_, old) = fixture.engine('b', 100);
        fs::rename(old, &pending).unwrap();
        let file = pending.join("Backend/python_runtime/python.exe");
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(file)
            .unwrap();
        assert_eq!(fixture.prune(&current), 0);
        assert!(pending.is_dir() && live.is_dir());
        drop(lock);
        assert_eq!(fixture.prune(&current), 1);
        assert!(!pending.exists() && live.is_dir());
    }

    #[test]
    fn changed_files_user_exports_empty_directories_and_legacy_caches_are_preserved() {
        let fixture = Fixture::new();
        let (current, _) = fixture.engine('a', 600);
        fixture.engine('b', 500);
        let (_, changed) = fixture.engine('c', 400);
        fs::write(
            changed.join("Backend/prototype_bridge.py"),
            b"user modification",
        )
        .unwrap();
        let (_, export) = fixture.engine('d', 300);
        fs::write(export.join("player.iff"), b"user export").unwrap();
        let (_, empty) = fixture.engine('e', 200);
        fs::create_dir(empty.join("my-work")).unwrap();
        let (_, legacy) = fixture.engine('f', 100);
        fs::remove_file(legacy.join("inventory.json")).unwrap();
        assert_eq!(fixture.prune(&current), 0);
        for path in [&changed, &export, &empty, &legacy] {
            assert!(path.is_dir());
        }
        assert_eq!(fs::read(export.join("player.iff")).unwrap(), b"user export");
    }

    #[test]
    fn generated_bytecode_is_disposable_but_unknown_bytecode_is_preserved() {
        let fixture = Fixture::new();
        let (current, _) = fixture.engine('a', 400);
        fixture.engine('b', 300);
        let (_, generated) = fixture.engine('c', 200);
        fs::create_dir(generated.join("Backend/__pycache__")).unwrap();
        fs::write(
            generated.join("Backend/__pycache__/prototype_bridge.cpython-312.pyc"),
            b"cache",
        )
        .unwrap();
        let (_, unknown) = fixture.engine('d', 100);
        fs::create_dir(unknown.join("Backend/__pycache__")).unwrap();
        fs::write(
            unknown.join("Backend/__pycache__/user_work.cpython-312.pyc"),
            b"user work",
        )
        .unwrap();
        assert_eq!(fixture.prune(&current), 1);
        assert!(!generated.exists());
        assert!(unknown.exists());
    }

    #[test]
    fn configured_outputs_and_custom_tools_prevent_retirement() {
        let fixture = Fixture::new();
        let (current, _) = fixture.engine('a', 400);
        fixture.engine('b', 300);
        let (_, output) = fixture.engine('c', 200);
        let (_, tools) = fixture.engine('d', 100);
        let settings = fixture.0.join("settings.json");
        fs::write(
            &settings,
            serde_json::to_vec(&serde_json::json!({
                "output_dir": output.join("future-exports"),
                "head_swap_tool": tools.join("my-tools")
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(fixture.prune(&current), 0);
        assert!(output.exists() && tools.exists());
        fs::write(&settings, b"invalid").unwrap();
        assert!(prune(&fixture.0, &current, &settings).is_err());
        assert!(output.exists() && tools.exists());
        fs::remove_file(settings).unwrap();
        assert_eq!(fixture.prune(&current), 2);
    }

    #[test]
    fn managed_tool_paths_do_not_pin_old_runtime_but_custom_paths_do() {
        let fixture = Fixture::new();
        let (_, root) = fixture.engine('a', 100);
        let root = fs::canonicalize(root).unwrap();
        assert!(!referenced(
            &root,
            &[(
                "head_swap_tool".into(),
                root.join("Backend/blender_tools/NBA_Character_HeadSwap")
            )]
        ));
        assert!(referenced(
            &root,
            &[(
                "head_swap_tool".into(),
                root.join("Backend/blender_tools/my-tools")
            )]
        ));
        assert!(referenced(
            &root,
            &[("output_dir".into(), root.parent().unwrap().to_path_buf())]
        ));
    }

    #[test]
    fn nested_directory_links_are_never_followed_or_deleted() {
        use std::{os::windows::process::CommandExt, process::Command};
        let fixture = Fixture::new();
        let (current, _) = fixture.engine('a', 300);
        fixture.engine('b', 200);
        let (_, old) = fixture.engine('c', 100);
        let external = fixture.0.join("external-tools");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("keep"), b"external sentinel").unwrap();
        let link = old.join("Backend/linked-tools");
        let status = Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(external.to_string_lossy().replace('/', "\\"))
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(status.status.success(), "{:?}", status);
        assert_eq!(fixture.prune(&current), 0);
        assert!(old.exists());
        assert_eq!(
            fs::read(external.join("keep")).unwrap(),
            b"external sentinel"
        );
        fs::remove_dir(link).unwrap();
        assert_eq!(fixture.prune(&current), 1);
        assert!(external.join("keep").is_file());
    }
}
