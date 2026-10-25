//! Mod installer, ported from the WPF reference (core/mod-installer.js and
//! core/mod-import-source.js).
//!
//! Sources are loose files, one folder, ZIP archives (extracted in-process
//! with the bundled `zip` crate) or RAR archives (inspected and extracted with
//! Windows' own `System32\tar.exe`, exactly as WPF did). Archives are expanded
//! into the portable `Data\mod-install` staging area only. Every file is then
//! routed to a manifest-verified logical path inside the active mods folder;
//! nothing is ever written to the game folder. Existing mod files are never
//! replaced without an explicit per-file Replace or Keep decision, and the
//! commit stages a sibling temporary copy before an atomic rename, rolling
//! back every committed file if any step fails.
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    mem::size_of,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant, SystemTime},
};

pub const MAX_ENTRIES: usize = 4096;
const MAX_MANIFEST_BYTES: u64 = 512 << 20;
const MAX_MANIFEST_ENTRIES: usize = 2_000_000;
const MAX_DEPTH: usize = 32;
const STAGE_PREFIX: &str = "stage-";
const TAR_TIMEOUT: Duration = Duration::from_secs(120);
pub(crate) fn recovery_artifact(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(".mod-install-") else {
        return false;
    };
    let Some(id) = rest
        .strip_suffix(".bak")
        .or_else(|| rest.strip_suffix(".tmp"))
    else {
        return false;
    };
    id.len() == 48 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Installer safety limits (WPF values by default; tests shrink them).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub entries: usize,
    pub file_bytes: u64,
    pub total_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: MAX_ENTRIES,
            file_bytes: 2 << 30,
            total_bytes: 4 << 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Files,
    Folder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Archive {
    Zip,
    Rar,
}
fn archive_type(path: &Path) -> Option<Archive> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "zip" => Some(Archive::Zip),
        "rar" => Some(Archive::Rar),
        _ => None,
    }
}

/// WPF limit wording: "4 GB", or exact bytes for test-sized limits.
fn size_text(bytes: u64) -> String {
    if bytes % (1 << 30) == 0 {
        format!("{} GB", bytes >> 30)
    } else {
        format!("{} byte", thousands(bytes as usize))
    }
}
/// `toLocaleString()` grouping used in WPF messages ("4,096").
fn thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
fn safe_part(part: &str) -> bool {
    if part.is_empty() || part == "." || part == ".." || part.len() > 255 {
        return false;
    }
    if part
        .chars()
        .any(|c| c < '\u{20}' || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        || part.ends_with('.')
        || part.ends_with(' ')
    {
        return false;
    }
    // Windows device names cannot be ordinary files, with or without extension.
    let stem = part
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let device = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.as_bytes()[3].is_ascii_digit());
    !device
        && !["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|n| matches!(n, "¹" | "²" | "³"))
        })
}
/// Relative archive/package entry path (WPF `safeEntry`): no absolute or
/// drive paths, no `.`/`..` segments, no device names or reserved characters.
pub fn safe_entry(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    let normalized = normalized.strip_prefix("./").unwrap_or(&normalized);
    let normalized = normalized.trim_end_matches('/');
    let bytes = normalized.as_bytes();
    if normalized.is_empty()
        || normalized.starts_with('/')
        || (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
    {
        return None;
    }
    normalized
        .split('/')
        .all(safe_part)
        .then(|| normalized.to_string())
}
/// Manifest logical path (WPF `safeLogical`).
pub fn safe_logical(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    let normalized = normalized.trim_matches('/');
    (!normalized.is_empty() && normalized.split('/').all(safe_part)).then(|| normalized.to_string())
}
fn leaf(logical: &str) -> &str {
    logical.rsplit('/').next().unwrap_or(logical)
}
fn parent_leaf(logical: &str) -> &str {
    logical
        .rsplit_once('/')
        .map(|(dir, _)| leaf(dir))
        .unwrap_or("")
}

/// Stock manifest leaf-name index (WPF `manifestLeafIndex`).
pub struct ManifestIndex {
    leaves: HashMap<String, Vec<String>>,
    roots: HashSet<String>,
}
impl ManifestIndex {
    pub fn load(game_root: &Path) -> Result<Self> {
        let path = game_root.join("manifest");
        let usable = fs::symlink_metadata(&path).is_ok_and(|m| {
            m.is_file()
                && !m.file_type().is_symlink()
                && m.len() > 0
                && m.len() <= MAX_MANIFEST_BYTES
        });
        ensure!(
            usable,
            "The selected game folder has no usable stock manifest."
        );
        let file = fs::File::open(&path).context("The stock manifest could not be opened")?;
        Self::parse(BufReader::new(file.take(MAX_MANIFEST_BYTES + 1)))
    }
    pub fn parse(reader: impl BufRead) -> Result<Self> {
        let (mut leaves, mut roots) = (HashMap::<String, Vec<String>>::new(), HashSet::new());
        let mut entries = 0usize;
        for line in reader.split(b'\n') {
            let line = line?;
            let line = String::from_utf8_lossy(&line);
            let Some(comma) = line.find(',').filter(|&c| c > 0) else {
                continue;
            };
            entries += 1;
            ensure!(
                entries <= MAX_MANIFEST_ENTRIES,
                "The stock manifest exceeds the installer safety limit."
            );
            let Some(logical) = safe_logical(&line[..comma]) else {
                continue;
            };
            roots.insert(logical.split('/').next().unwrap_or("").to_lowercase());
            let values = leaves.entry(leaf(&logical).to_lowercase()).or_default();
            if !values.iter().any(|v| v.eq_ignore_ascii_case(&logical)) {
                values.push(logical);
            }
        }
        Ok(Self { leaves, roots })
    }
}
fn manifest_match(matches: &[String], source: &Path) -> Option<String> {
    match matches.len() {
        0 => return None,
        1 => return Some(matches[0].clone()),
        _ => {}
    }
    let parent = source
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let narrowed: Vec<_> = matches
        .iter()
        .filter(|m| parent_leaf(m).to_lowercase() == parent)
        .collect();
    (narrowed.len() == 1).then(|| narrowed[0].clone())
}
/// WPF `/^png\d{4,6}(?=[_.]|$).*\.iff$/i` → `char/sig/<name>`.
fn loose_player_logical(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix("png")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let after = &rest[digits..];
    ((4..=6).contains(&digits)
        && (after.is_empty() || after.starts_with('_') || after.starts_with('.'))
        && lower.ends_with(".iff")
        && safe_part(name))
    .then(|| format!("char/sig/{name}"))
}
fn rooted_package_logical(relative: Option<&str>, roots: &HashSet<String>) -> Option<String> {
    let logical = safe_logical(relative?)?;
    let parts: Vec<&str> = logical.split('/').collect();
    (0..parts.len())
        .find(|&i| roots.contains(&parts[i].to_lowercase()))
        .and_then(|i| safe_logical(&parts[i..].join("/")))
}
/// Programs and scripts are never installed into a mods folder.
pub fn executable(name: &str) -> bool {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "exe"
            | "dll"
            | "asi"
            | "com"
            | "scr"
            | "bat"
            | "cmd"
            | "ps1"
            | "psm1"
            | "vbs"
            | "vbe"
            | "js"
            | "jse"
            | "wsf"
            | "wsh"
            | "msi"
            | "msp"
            | "lnk"
            | "url"
            | "reg"
            | "hta"
            | "cpl"
            | "sys"
            | "jar"
            | "py"
            | "pif"
    )
}

fn linked(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}
fn plain_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !linked(&m))
}
/// Refuses any symbolic link or junction along the path (WPF `plainChain`).
fn plain_chain(path: &Path, message: &str) -> Result<()> {
    let absolute = std::path::absolute(path)?;
    let mut current = PathBuf::new();
    for component in absolute.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            continue;
        }
        if let Ok(meta) = fs::symlink_metadata(&current) {
            ensure!(!linked(&meta), "{message}");
        }
    }
    Ok(())
}
fn key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    text.trim_end_matches('\\').to_lowercase()
}
fn within(parent: &Path, child: &Path) -> bool {
    let (parent, child) = (key(parent), key(child));
    child == parent || child.starts_with(&format!("{parent}\\"))
}
fn canonical_or_absolute(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    Ok(fs::canonicalize(&absolute)
        .map(|p| {
            let text = p.to_string_lossy().into_owned();
            match text.strip_prefix(r"\\?\UNC\") {
                Some(unc) => PathBuf::from(format!(r"\\{unc}")),
                None => PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text)),
            }
        })
        .unwrap_or(absolute))
}

#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub relative: Option<String>,
    pub package: bool,
}

/// Recursively lists a plain folder (WPF `collectFolder`).
pub fn collect_folder(dir: &Path, include_root_name: bool, limits: &Limits) -> Result<Vec<Source>> {
    let root = std::path::absolute(dir)?;
    ensure!(
        plain_dir(&root),
        "The selected mod folder must be a plain local directory."
    );
    plain_chain(&root, "Mod packages cannot be inside linked folders.")?;
    let prefix = if include_root_name {
        root.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let (mut pending, mut sources) = (vec![(root, prefix, 0usize)], Vec::new());
    let (mut total, mut visited) = (0u64, 0usize);
    while let Some((dir, relative, depth)) = pending.pop() {
        let mut entries = fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            visited += 1;
            ensure!(
                visited <= limits.entries * 4,
                "The mod package exceeds the {} file safety limit.",
                thousands(limits.entries)
            );
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let meta = fs::symlink_metadata(&path)?;
            ensure!(
                !linked(&meta),
                "Mod packages cannot contain linked files or folders."
            );
            let child = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            if meta.is_dir() {
                ensure!(
                    depth < MAX_DEPTH,
                    "The mod package folder tree is too deep."
                );
                pending.push((path, child, depth + 1));
                continue;
            }
            ensure!(
                meta.is_file(),
                "Mod packages may contain only plain files and folders."
            );
            if meta.len() == 0 {
                continue;
            }
            ensure!(
                sources.len() < limits.entries,
                "The mod package exceeds the {} file safety limit.",
                thousands(limits.entries)
            );
            total += meta.len();
            ensure!(
                total <= limits.total_bytes,
                "The mod package exceeds the {} safety limit.",
                size_text(limits.total_bytes)
            );
            let relative =
                safe_entry(&child).context("The mod package contains an unsafe path.")?;
            sources.push(Source {
                path,
                relative: Some(relative),
                package: true,
            });
        }
    }
    Ok(sources)
}

fn archive_file(file: &Path, limits: &Limits) -> Result<()> {
    let meta = fs::symlink_metadata(file).context("The selected archive no longer exists.")?;
    ensure!(
        meta.is_file() && !linked(&meta),
        "The selected archive must be a plain local file."
    );
    ensure!(
        meta.len() > 0 && meta.len() <= limits.total_bytes,
        "The selected archive exceeds the {} safety limit.",
        size_text(limits.total_bytes)
    );
    Ok(())
}
fn zip_error(error: zip::result::ZipError) -> anyhow::Error {
    anyhow!("Could not read this ZIP archive: {error}")
}
/// Validates every entry, then extracts a ZIP into `out` (an empty staging
/// folder). Paths are re-checked by `safe_entry`; links, encrypted entries,
/// unsupported compression, duplicate Windows paths and oversize content
/// are refused before anything is written.
pub fn extract_zip(file: &Path, out: &Path, limits: &Limits) -> Result<()> {
    archive_file(file, limits)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(fs::File::open(file)?)).map_err(zip_error)?;
    ensure!(zip.len() > 0, "The selected archive is empty.");
    ensure!(
        zip.len() <= limits.entries,
        "The archive exceeds the {} entry safety limit.",
        thousands(limits.entries)
    );
    let (mut seen, mut parents, mut files) = (HashSet::new(), HashSet::new(), Vec::new());
    let mut total = 0u64;
    for index in 0..zip.len() {
        let entry = zip.by_index_raw(index).map_err(zip_error)?;
        let path = safe_entry(entry.name()).context("The archive contains an unsafe path.")?;
        let mode = entry.unix_mode().unwrap_or(0) & 0o170000;
        ensure!(
            !entry.is_symlink() && matches!(mode, 0 | 0o100000 | 0o040000),
            "Archive links and special entries are not supported."
        );
        ensure!(
            !entry.encrypted(),
            "Password-protected ZIP archives are not supported."
        );
        let lower = path.to_lowercase();
        ensure!(
            seen.insert(lower.clone()),
            "The archive contains duplicate Windows paths."
        );
        let mut ancestors = lower.as_str();
        while let Some((dir, _)) = ancestors.rsplit_once('/') {
            parents.insert(dir.to_string());
            ancestors = dir;
        }
        if entry.is_dir() {
            continue;
        }
        if !matches!(
            entry.compression(),
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            // Nothing has been written yet; the caller hands the archive to
            // Windows' tar.exe, which WPF used for every ZIP.
            return Err(anyhow::Error::new(UnsupportedZipCompression(
                entry.compression().to_string(),
            )));
        }
        let size = entry.size();
        total = total.saturating_add(size);
        ensure!(
            size <= limits.file_bytes && total <= limits.total_bytes,
            "The archive expands beyond the {} safety limit.",
            size_text(limits.total_bytes)
        );
        files.push((index, path, lower, size));
    }
    ensure!(
        files
            .iter()
            .all(|(_, _, lower, _)| !parents.contains(lower)),
        "The archive contains duplicate Windows paths."
    );
    for (index, path, _, size) in files {
        let target = path
            .split('/')
            .fold(out.to_path_buf(), |p, part| p.join(part));
        fs::create_dir_all(target.parent().context("Archive entry parent missing")?)?;
        let mut entry = zip.by_index(index).map_err(zip_error)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .context("The archive contains duplicate Windows paths.")?;
        // Never trust the declared size: read at most one byte more and
        // require an exact match (the zip reader also verifies the CRC).
        let written = std::io::copy(&mut (&mut entry).take(size + 1), &mut output)
            .map_err(|e| anyhow!("Could not read this ZIP archive: {e}"))?;
        ensure!(
            written == size,
            "The archive file list could not be verified."
        );
    }
    Ok(())
}

/// A ZIP entry uses a method the bundled decoder lacks (Deflate64, bzip2,
/// LZMA…). Raised before any byte is extracted.
#[derive(Debug)]
pub struct UnsupportedZipCompression(pub String);
impl std::fmt::Display for UnsupportedZipCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This ZIP uses {} compression, which the launcher cannot extract.",
            self.0
        )
    }
}
impl std::error::Error for UnsupportedZipCompression {}
/// WPF extracted every ZIP with tar.exe; the bundled decoder handles the
/// common Stored/Deflate case and tar.exe the remaining methods.
pub fn extract_zip_or_tar(file: &Path, out: &Path, tar: &Path, limits: &Limits) -> Result<()> {
    match extract_zip(file, out, limits) {
        Err(error) if error.downcast_ref::<UnsupportedZipCompression>().is_some() => {
            ensure!(
                fs::read_dir(out)?.next().is_none(),
                "The archive staging folder is not empty."
            );
            extract_with_tar(file, out, tar, limits).map_err(|tar_error| {
                if fs::symlink_metadata(tar).is_ok_and(|m| m.is_file()) {
                    tar_error
                } else {
                    anyhow!("{error} Re-pack it as a standard (Deflate) ZIP.")
                }
            })
        }
        result => result,
    }
}
pub fn tar_executable() -> PathBuf {
    let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(system).join("System32").join("tar.exe")
}
fn run_tar(executable: &Path, args: &[&OsStr]) -> Result<String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .context("Could not start the Windows archive tool")?;
    let mut stdout = child.stdout.take().context("Archive tool output missing")?;
    let mut stderr = child.stderr.take().context("Archive tool output missing")?;
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut stdout).take(32 << 20).read_to_end(&mut bytes);
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = (&mut stderr).take(1 << 20).read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + TAR_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Could not read this archive: the archive tool timed out.");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let (out, err) = (
        out.join().unwrap_or_default(),
        err.join().unwrap_or_default(),
    );
    if !status.success() {
        let text = String::from_utf8_lossy(&err);
        let detail = text
            .trim()
            .lines()
            .last()
            .unwrap_or("Archive extraction failed.");
        bail!("Could not read this archive: {detail}");
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}
/// WPF `inspectVerboseListing`: only plain files and directories, total size
/// within the limit. Returns (records, total bytes).
pub fn inspect_verbose_listing(text: &str, limits: &Limits) -> Result<(usize, u64)> {
    static ROW: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let row = ROW.get_or_init(|| regex::Regex::new(r"^\S+\s+\d+\s+\d+\s+\d+\s+(\d+)\s+").unwrap());
    let (mut total, mut records) = (0u64, 0usize);
    for line in text.lines().filter(|l| !l.is_empty()) {
        ensure!(
            matches!(line.as_bytes()[0], b'-' | b'd'),
            "Archive links and special entries are not supported."
        );
        let size: u64 = row
            .captures(line)
            .and_then(|c| c[1].parse().ok())
            .context("The archive file list could not be verified.")?;
        ensure!(
            size <= limits.file_bytes,
            "The archive expands beyond the {} safety limit.",
            size_text(limits.total_bytes)
        );
        total = total.saturating_add(size);
        ensure!(
            total <= limits.total_bytes,
            "The archive expands beyond the {} safety limit.",
            size_text(limits.total_bytes)
        );
        records += 1;
    }
    Ok((records, total))
}
/// WPF `inspectArchive` + extraction through Windows' bsdtar. Used for RAR;
/// works for any format that tar.exe reads.
pub fn extract_with_tar(file: &Path, out: &Path, executable: &Path, limits: &Limits) -> Result<()> {
    ensure!(
        fs::symlink_metadata(executable).is_ok_and(|m| m.is_file()),
        "RAR archives need the Windows archive tool (System32\\tar.exe), which is not available on this PC. Extract the RAR yourself and install the folder instead."
    );
    archive_file(file, limits)?;
    let file = file.as_os_str();
    let list = run_tar(executable, &[OsStr::new("-tf"), file])?;
    let verbose = run_tar(executable, &[OsStr::new("-tvf"), file])?;
    let entries: Vec<&str> = list.lines().filter(|l| !l.is_empty()).collect();
    ensure!(!entries.is_empty(), "The selected archive is empty.");
    ensure!(
        entries.len() <= limits.entries,
        "The archive exceeds the {} entry safety limit.",
        thousands(limits.entries)
    );
    let mut seen = HashSet::new();
    for entry in &entries {
        let safe = safe_entry(entry).context("The archive contains an unsafe path.")?;
        ensure!(
            seen.insert(safe.to_lowercase()),
            "The archive contains duplicate Windows paths."
        );
    }
    let (records, _) = inspect_verbose_listing(&verbose, limits)?;
    ensure!(
        records == entries.len(),
        "The archive file list changed while it was being checked."
    );
    run_tar(
        executable,
        &[
            OsStr::new("--no-same-owner"),
            OsStr::new("--no-same-permissions"),
            OsStr::new("-xf"),
            file,
            OsStr::new("-C"),
            out.as_os_str(),
        ],
    )?;
    Ok(())
}

/// Sources ready for routing, plus the staging folder that owns any
/// extracted archive content.
pub struct Prepared {
    pub staging: Option<PathBuf>,
    pub sources: Vec<Source>,
    pub archives: usize,
    pub folders: usize,
}
impl Prepared {
    pub fn cleanup(&mut self) {
        if let Some(staging) = self.staging.take() {
            let _ = fs::remove_dir_all(staging);
        }
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        self.cleanup();
    }
}
/// Removes staging folders left by an earlier, interrupted install.
pub fn purge_staging(parent: &Path) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(STAGE_PREFIX) && plain_dir(&entry.path()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}
pub fn prepare(
    paths: &[PathBuf],
    mode: Mode,
    staging_parent: &Path,
    tar: &Path,
    limits: &Limits,
) -> Result<Prepared> {
    let mut selected: Vec<PathBuf> = Vec::new();
    for path in paths {
        let path = std::path::absolute(path)?;
        if !selected.iter().any(|p| key(p) == key(&path)) {
            selected.push(path);
        }
    }
    ensure!(
        !selected.is_empty(),
        "Choose a mod folder, archive, or file."
    );
    ensure!(
        selected.len() <= limits.entries,
        "Choose between 1 and {} mod files.",
        limits.entries
    );
    let mut prepared = Prepared {
        staging: None,
        sources: Vec::new(),
        archives: 0,
        folders: 0,
    };
    let result = (|| -> Result<()> {
        if mode == Mode::Folder {
            ensure!(selected.len() == 1, "Choose one mod folder at a time.");
            prepared.sources = collect_folder(&selected[0], true, limits)?;
            prepared.folders = 1;
        } else {
            for path in &selected {
                let Some(kind) = archive_type(path) else {
                    prepared.sources.push(Source {
                        path: path.clone(),
                        relative: None,
                        package: false,
                    });
                    continue;
                };
                if prepared.staging.is_none() {
                    fs::create_dir_all(staging_parent)?;
                    plain_chain(
                        staging_parent,
                        "The launcher Data folder must not be linked.",
                    )?;
                    let staging = staging_parent
                        .join(format!("{STAGE_PREFIX}{}", crate::backend::unique_id()));
                    fs::create_dir(&staging)?;
                    prepared.staging = Some(staging);
                }
                let out = prepared
                    .staging
                    .as_ref()
                    .context("Staging folder missing")?
                    .join(format!("archive-{}", prepared.archives));
                prepared.archives += 1;
                fs::create_dir(&out)?;
                match kind {
                    Archive::Zip => extract_zip_or_tar(path, &out, tar, limits)?,
                    Archive::Rar => extract_with_tar(path, &out, tar, limits)?,
                }
                prepared
                    .sources
                    .extend(collect_folder(&out, false, limits)?);
            }
        }
        ensure!(
            !prepared.sources.is_empty(),
            "No files were found in this mod package."
        );
        ensure!(
            prepared.sources.len() <= limits.entries,
            "The mod package exceeds the {} file safety limit.",
            thousands(limits.entries)
        );
        Ok(())
    })();
    if let Err(error) = result {
        prepared.cleanup();
        return Err(error);
    }
    Ok(prepared)
}

#[derive(Debug, Clone)]
pub struct Item {
    pub source: PathBuf,
    pub name: String,
    pub logical: String,
    pub destination: PathBuf,
    pub routing: &'static str,
    pub size: u64,
    pub modified: SystemTime,
    pub existing: Option<(u64, SystemTime)>,
}
#[derive(Debug, Clone)]
pub struct Note {
    pub name: String,
    pub reason: String,
}
pub struct Plan {
    pub id: String,
    pub root: PathBuf,
    pub items: Vec<Item>,
    pub issues: Vec<Note>,
    pub ignored: Vec<Note>,
    pub prepared: Prepared,
}
impl Plan {
    pub fn replacements(&self) -> usize {
        self.items.iter().filter(|i| i.existing.is_some()).count()
    }
    pub fn to_json(&self) -> Value {
        let ms = |t: SystemTime| {
            t.duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        };
        let notes = |n: &[Note]| -> Vec<Value> {
            n.iter()
                .map(|n| json!({"name":n.name,"reason":n.reason}))
                .collect()
        };
        json!({
            "cancelled": false,
            "planId": self.id,
            "root": self.root,
            "items": self.items.iter().map(|i| json!({
                "name": i.name,
                "logicalPath": i.logical,
                "routing": i.routing,
                "size": i.size,
                "existing": i.existing.map(|(size, modified)| json!({"size":size,"modifiedMs":ms(modified)})),
            })).collect::<Vec<_>>(),
            "issues": notes(&self.issues),
            "ignored": notes(&self.ignored),
            "totalBytes": self.items.iter().map(|i| i.size).sum::<u64>(),
            "replacements": self.replacements(),
            "archives": self.prepared.archives,
            "folders": self.prepared.folders,
        })
    }
}
fn note(name: &str, reason: &str) -> Note {
    Note {
        name: name.into(),
        reason: reason.into(),
    }
}
fn check_mods_root(game_root: &Path, mods_root: &Path) -> Result<PathBuf> {
    ensure!(
        !mods_root.as_os_str().is_empty() && plain_dir(mods_root),
        "The active mods folder is not a plain local directory."
    );
    plain_chain(mods_root, "Mod installation refuses linked folders.")?;
    let root = canonical_or_absolute(mods_root)?;
    let game = canonical_or_absolute(game_root)?;
    ensure!(
        !within(&root, &game),
        "The active mods folder must be a dedicated mods folder, not the game folder or one of its parents."
    );
    Ok(root)
}
/// Routes every prepared source (WPF `planModInstall`).
pub fn plan(
    game_root: &Path,
    mods_root: &Path,
    mut prepared: Prepared,
    limits: &Limits,
) -> Result<Plan> {
    let routed = (|| -> Result<(PathBuf, Vec<Item>, Vec<Note>, Vec<Note>)> {
        let root = check_mods_root(game_root, mods_root)?;
        let index = ManifestIndex::load(game_root)?;
        let (items, issues, ignored) = route(&index, &root, &prepared.sources, limits);
        Ok((root, items, issues, ignored))
    })();
    match routed {
        Ok((root, items, issues, ignored)) => Ok(Plan {
            id: crate::backend::unique_id(),
            root,
            items,
            issues,
            ignored,
            prepared,
        }),
        Err(error) => {
            prepared.cleanup();
            Err(error)
        }
    }
}
fn route(
    index: &ManifestIndex,
    root: &Path,
    sources: &[Source],
    limits: &Limits,
) -> (Vec<Item>, Vec<Note>, Vec<Note>) {
    let (mut items, mut issues, mut ignored) = (Vec::new(), Vec::new(), Vec::new());
    let (mut destinations, mut total) = (HashSet::new(), 0u64);
    for source in sources {
        let name = source
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if executable(&name) {
            let reason = "Programs and scripts are never installed into a mods folder.";
            if source.package {
                ignored.push(note(&name, reason));
            } else {
                issues.push(note(&name, reason));
            }
            continue;
        }
        let meta = match fs::symlink_metadata(&source.path) {
            Ok(meta) => meta,
            Err(_) => {
                issues.push(note(&name, "File no longer exists."));
                continue;
            }
        };
        if !meta.is_file() || linked(&meta) {
            issues.push(note(&name, "Source must be a plain local file."));
            continue;
        }
        total = total.saturating_add(meta.len());
        if meta.len() < 1 || meta.len() > limits.file_bytes || total > limits.total_bytes {
            issues.push(note(
                &name,
                "File selection exceeds the installer size limit.",
            ));
            continue;
        }
        let lower = name.to_lowercase();
        let matches = index.leaves.get(&lower).map(Vec::as_slice).unwrap_or(&[]);
        let structured = [".iff", ".bin", ".dds"]
            .iter()
            .any(|e| lower.ends_with(e))
            .then(|| rooted_package_logical(source.relative.as_deref(), &index.roots))
            .flatten();
        let manifest = structured
            .is_none()
            .then(|| manifest_match(matches, &source.path))
            .flatten();
        let player = (structured.is_none() && manifest.is_none() && matches.is_empty())
            .then(|| loose_player_logical(&name))
            .flatten();
        let routing = if structured.is_some() {
            "package-path"
        } else if manifest.is_some() {
            "manifest"
        } else {
            "player-char-sig"
        };
        let Some(logical) = structured.or(manifest).or(player) else {
            if source.package && !lower.ends_with(".iff") {
                ignored.push(note(&name, "Package documentation or preview file."));
            } else if matches.is_empty() {
                issues.push(note(&name, "Filename is not listed in the stock manifest and is not a recognized PNG player file."));
            } else {
                issues.push(note(&name, "Filename is ambiguous in the stock manifest."));
            }
            continue;
        };
        let destination = logical
            .split('/')
            .fold(root.to_path_buf(), |p, part| p.join(part));
        if !within(root, &destination) || key(root) == key(&destination) {
            issues.push(note(
                &name,
                "Resolved destination is outside the mods folder.",
            ));
            continue;
        }
        if !destinations.insert(key(&destination)) {
            issues.push(note(
                &name,
                "Another selected file resolves to the same destination.",
            ));
            continue;
        }
        if plain_chain(
            destination.parent().unwrap_or(root),
            "Mod installation refuses linked destination folders.",
        )
        .is_err()
        {
            issues.push(note(&name, "Destination folder is a link or junction."));
            continue;
        }
        let existing = match fs::symlink_metadata(&destination) {
            Ok(m) if !m.is_file() || linked(&m) => {
                issues.push(note(&name, "Destination is not a plain file."));
                continue;
            }
            Ok(m) => Some((m.len(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH))),
            Err(_) => None,
        };
        items.push(Item {
            source: source.path.clone(),
            name,
            logical,
            destination,
            routing,
            size: meta.len(),
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            existing,
        });
    }
    if items.is_empty() && issues.is_empty() {
        issues.push(note("Package", "No installable mod files were found."));
    }
    (items, issues, ignored)
}

fn metadata_matches(path: &Path, size: u64, modified: SystemTime) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| {
        m.is_file()
            && !linked(&m)
            && m.len() == size
            && m.modified().unwrap_or(SystemTime::UNIX_EPOCH) == modified
    })
}
struct Staged<'a> {
    item: &'a Item,
    file: fs::File,
    backup: Option<(PathBuf, fs::File)>,
    published: bool,
    _pins: crate::loader_files::Directories,
}
fn create_owned(path: &Path) -> Result<fs::File> {
    Ok(fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .access_mode(0x80000000 | 0x40000000 | 0x10000)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?)
}
fn same_file(file: &fs::File, path: &Path) -> Result<bool> {
    use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
    let other = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x00200000)
        .open(path)?;
    let mut a = BY_HANDLE_FILE_INFORMATION::default();
    let mut b = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut a)?;
        GetFileInformationByHandle(HANDLE(other.as_raw_handle()), &mut b)?;
    }
    Ok(a.dwVolumeSerialNumber == b.dwVolumeSerialNumber
        && a.nFileIndexHigh == b.nFileIndexHigh
        && a.nFileIndexLow == b.nFileIndexLow)
}
fn rename_owned(file: &fs::File, target: &Path, replace: bool) -> Result<()> {
    use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
    let name = std::path::absolute(target)?
        .as_os_str()
        .encode_wide()
        .collect::<Vec<_>>();
    ensure!(
        !name.is_empty() && name.len() <= 32760 && !name.contains(&0),
        "Mod path exceeds bounds"
    );
    let size = size_of::<FILE_RENAME_INFO>() + (name.len() + 1) * 2;
    let mut buffer = vec![0u64; size.div_ceil(8)];
    let info = buffer.as_mut_ptr() as *mut FILE_RENAME_INFO;
    unsafe {
        // FileRenameInfoEx REPLACE_IF_EXISTS | POSIX_SEMANTICS permits
        // atomic replacement while our original read/DELETE-sharing handle
        // stays open. It continues to deny concurrent writes through that
        // handle. No readonly-attribute bypass or nonatomic fallback.
        (*info).Anonymous.Flags = if replace { 0x1 | 0x2 } else { 0 };
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            if replace {
                FileRenameInfoEx
            } else {
                FileRenameInfo
            },
            info as _,
            size as u32,
        )?;
    }
    Ok(())
}
/// Installs a plan (WPF `installModPlan`) with per-file conflict decisions.
/// `replace` and `keep` hold lowercase logical paths; every existing
/// destination must appear in exactly one of them.
pub fn install(plan: &Plan, replace: &HashSet<String>, keep: &HashSet<String>) -> Result<Value> {
    install_recoverable(plan, replace, keep, None, None)
}
fn file_hash(file: &fs::File) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:X}", hash.finalize()))
}
fn install_recoverable(
    plan: &Plan,
    replace: &HashSet<String>,
    keep: &HashSet<String>,
    recovery: Option<&Path>,
    expected: Option<&HashMap<String, String>>,
) -> Result<Value> {
    let mut receipt =
        json!({"schema":1,"id":plan.id,"root":plan.root,"state":"preparing","items":[]});
    if let Some(path) = recovery {
        crate::backend::atomic_json(path, &receipt)?;
    }
    ensure!(
        plan.issues.is_empty(),
        "Resolve every mod routing issue before installing."
    );
    for item in plan.items.iter().filter(|i| i.existing.is_some()) {
        let k = item.logical.to_lowercase();
        ensure!(
            replace.contains(&k) != keep.contains(&k),
            "Choose Replace or Keep for every existing mod file."
        );
    }
    plain_chain(&plan.root, "Mod installation refuses linked folders.")?;
    ensure!(
        plain_dir(&plan.root),
        "The active mods folder is not a plain local directory."
    );
    let chosen: Vec<&Item> = plan
        .items
        .iter()
        .filter(|i| i.existing.is_none() || replace.contains(&i.logical.to_lowercase()))
        .collect();
    let kept = plan.items.len() - chosen.len();
    // Keep exact owned handles until commit/rollback completes. Each replace
    // is one atomic handle rename; the old destination never disappears first.
    let mut staged: Vec<Staged<'_>> = Vec::new();
    let stage = (|| -> Result<()> {
        for item in &chosen {
            ensure!(
                metadata_matches(&item.source, item.size, item.modified),
                "{} changed after it was selected.",
                item.name
            );
            let parent = item
                .destination
                .parent()
                .context("Missing destination folder")?;
            ensure!(
                parent.starts_with(&plan.root),
                "Destination is outside the mods folder."
            );
            fs::create_dir_all(parent)?;
            plain_chain(
                parent,
                "Mod installation refuses linked destination folders.",
            )?;
            let pins = crate::loader_files::Directories::pin(parent)?;
            let path = parent.join(format!(".mod-install-{}.tmp", crate::backend::unique_id()));
            let file = create_owned(&path)?;
            staged.push(Staged {
                item,
                file,
                backup: None,
                published: false,
                _pins: pins,
            });
            let output = &mut &staged.last().unwrap().file;
            let mut input = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x00200000)
                .open(&item.source)?;
            ensure!(
                !linked(&input.metadata()?),
                "Linked mod input is unsupported"
            );
            let copied = std::io::copy(&mut (&mut input).take(item.size + 1), output)?;
            output.sync_all()?;
            ensure!(
                copied == item.size && metadata_matches(&item.source, item.size, item.modified),
                "{} changed after it was selected.",
                item.name
            );
        }
        Ok(())
    })();
    if let Err(error) = stage {
        for entry in &staged {
            let _ = crate::loader_files::unlink(&entry.file);
        }
        return Err(error);
    }
    let commit = (|| -> Result<()> {
        for entry in &mut staged {
            let item = entry.item;
            if let Some((size, modified)) = item.existing {
                // Deny writes while making a durable original copy. DELETE
                // sharing is necessary for atomic replacement, not for cleanup.
                let input = fs::OpenOptions::new()
                    .read(true)
                    .share_mode(5)
                    .custom_flags(0x00200000)
                    .open(&item.destination)?;
                ensure!(
                    metadata_matches(&item.destination, size, modified)
                        && !linked(&input.metadata()?),
                    "{} changed while the install was being prepared.",
                    item.logical
                );
                let path = item
                    .destination
                    .with_file_name(format!(".mod-install-{}.bak", crate::backend::unique_id()));
                let backup = create_owned(&path)?;
                entry.backup = Some((path, backup));
                if let Some(expected) = expected {
                    ensure!(
                        expected.get(&item.logical.to_lowercase()) == Some(&file_hash(&input)?),
                        "{} changed before restore. Newer edits were preserved.",
                        item.logical
                    );
                    (&input).seek(SeekFrom::Start(0))?;
                }
                let copied = std::io::copy(
                    &mut (&input).take(size + 1),
                    &mut &entry.backup.as_ref().unwrap().1,
                )?;
                entry.backup.as_ref().unwrap().1.sync_all()?;
                ensure!(
                    copied == size
                        && same_file(&input, &item.destination)?
                        && metadata_matches(&item.destination, size, modified),
                    "Original mod changed before atomic replacement"
                );
                if let Some(path) = recovery {
                    let (backup_path, backup_file) = entry.backup.as_ref().unwrap();
                    receipt["items"].as_array_mut().unwrap().push(json!({
                        "logical":item.logical,
                        "backup":backup_path.file_name().unwrap().to_string_lossy(),
                        "originalHash":file_hash(backup_file)?,
                        "installedHash":file_hash(&entry.file)?
                    }));
                    // Recovery evidence must reach disk before replacing this file.
                    crate::backend::atomic_json(path, &receipt)?;
                }
                rename_owned(&entry.file, &item.destination, true)?;
            } else {
                // No replacement authority for a newly appeared destination.
                rename_owned(&entry.file, &item.destination, false)?;
            }
            entry.published = true;
            entry.file.sync_all()?;
        }
        Ok(())
    })();
    if let Err(error) = commit {
        let mut held = Vec::new();
        for entry in staged.into_iter().rev() {
            // Handle-based unlink cannot delete an unrelated path replacement.
            let removed = crate::loader_files::unlink(&entry.file);
            drop(entry.file);
            if entry.published {
                if let Some((path, file)) = &entry.backup {
                    if removed.is_err()
                        || rename_owned(file, &entry.item.destination, false).is_err()
                    {
                        held.push(path.display().to_string());
                    }
                }
            } else if let Some((_, file)) = &entry.backup {
                let _ = crate::loader_files::unlink(file);
            }
        }
        ensure!(
            held.is_empty(),
            "Mod installation failed: {error:#}. Original backups preserved for recovery: {}",
            held.join(", ")
        );
        return Err(error);
    }
    let mut cleanup_warnings = Vec::new();
    for entry in &staged {
        if let Some((path, file)) = &entry.backup {
            if recovery.is_none()
                && let Err(error) = crate::loader_files::unlink(file)
            {
                cleanup_warnings.push(format!("Backup preserved at {}: {error:#}", path.display()));
            }
        }
    }
    if let Some(path) = recovery {
        receipt["state"] = json!("complete");
        if let Err(error) = crate::backend::atomic_json(path, &receipt) {
            cleanup_warnings.push(format!(
                "Files installed; recovery receipt needs review: {error:#}"
            ));
        }
    }
    Ok(json!({
        "recoveryId": recovery.map(|_| &plan.id),
        "cleanupWarnings": cleanup_warnings,
        "cancelled": false,
        "installed": staged.len(),
        "replaced": staged.iter().filter(|entry| entry.item.existing.is_some()).count(),
        "kept": kept,
        "ignored": plan.ignored.len(),
        "archives": plan.prepared.archives,
        "folders": plan.prepared.folders,
        "items": staged.iter().map(|entry| { let i=entry.item; json!({"name":i.name,"logicalPath":i.logical,"size":i.size}) }).collect::<Vec<_>>(),
    }))
}

/// Holds at most one reviewed plan between the conflict review and commit.
pub struct Installer {
    staging: PathBuf,
    recovery: PathBuf,
    pending: Mutex<Option<Plan>>,
}
impl Installer {
    pub fn new(data_root: &Path) -> Self {
        Self {
            staging: data_root.join("Data").join("mod-install"),
            recovery: data_root.join("Recovery").join("ModInstalls"),
            pending: Mutex::new(None),
        }
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Option<Plan>>> {
        self.pending
            .lock()
            .map_err(|_| anyhow!("Mod installer unavailable"))
    }
    pub fn cancel(&self) -> Result<()> {
        if let Some(mut plan) = self.lock()?.take() {
            plan.prepared.cleanup();
        }
        purge_staging(&self.staging);
        Ok(())
    }
    fn read_receipt(&self, path: &Path) -> Result<Value> {
        plain_chain(&self.recovery, "Recovery refuses linked folders")?;
        let meta = fs::symlink_metadata(path)?;
        ensure!(
            meta.is_file() && !linked(&meta) && meta.len() <= 8 * 1024 * 1024,
            "Invalid recovery receipt"
        );
        Ok(serde_json::from_reader(fs::File::open(path)?)?)
    }
    pub fn history(&self, mods_root: &Path) -> Result<Value> {
        let _lock = self.lock()?;
        let root = canonical_or_absolute(mods_root)?;
        let mut rows = Vec::new();
        if !self.recovery.exists() {
            return Ok(json!(rows));
        }
        plain_chain(&self.recovery, "Recovery refuses linked folders")?;
        for entry in fs::read_dir(&self.recovery)?.take(10000) {
            let entry = entry?;
            if entry.path().extension() != Some(OsStr::new("json")) {
                continue;
            }
            let Ok(r) = self.read_receipt(&entry.path()) else {
                continue;
            };
            if r["schema"] != 1
                || r["state"] != "complete"
                || r["root"]
                    .as_str()
                    .is_none_or(|p| key(Path::new(p)) != key(&root))
            {
                continue;
            }
            let count = r["items"].as_array().map_or(0, Vec::len);
            if count > 0 {
                rows.push(json!({"id":r["id"],"files":count,"updatedMs": entry.metadata()?.modified()?.duration_since(SystemTime::UNIX_EPOCH)?.as_millis() as u64}));
            }
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r["updatedMs"].as_u64().unwrap_or(0)));
        Ok(json!(rows))
    }
    pub fn restore(&self, mods_root: &Path, id: &str) -> Result<Value> {
        let pending = self.lock()?;
        ensure!(
            pending.is_none(),
            "Close the install review before restoring files"
        );
        ensure!(
            id.len() == 48 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid recovery ID"
        );
        let path = self.recovery.join(format!("{id}.json"));
        let _recovery_pin = crate::loader_files::Directories::pin(&self.recovery)?;
        let mut receipt = self.read_receipt(&path)?;
        let root = canonical_or_absolute(mods_root)?;
        ensure!(
            receipt["schema"] == 1 && receipt["id"] == id && receipt["state"] == "complete",
            "This recovery is no longer available"
        );
        ensure!(
            receipt["root"]
                .as_str()
                .is_some_and(|p| key(Path::new(p)) == key(&root)),
            "Select the original mod profile before restoring"
        );
        let rows = receipt["items"]
            .as_array()
            .context("Invalid recovery entries")?;
        ensure!(
            !rows.is_empty() && rows.len() <= MAX_ENTRIES,
            "Invalid recovery size"
        );
        let mut items = Vec::new();
        let mut held_files = Vec::new();
        let mut held_dirs = Vec::new();
        let mut seen = HashSet::new();
        let mut expected = HashMap::new();
        for row in rows {
            let logical = row["logical"]
                .as_str()
                .and_then(safe_logical)
                .context("Invalid recovery destination")?;
            ensure!(
                seen.insert(logical.to_lowercase()),
                "Duplicate recovery destination"
            );
            let destination = root.join(&logical);
            expected.insert(
                logical.to_lowercase(),
                row["installedHash"].as_str().unwrap_or("").to_owned(),
            );
            let parent = destination.parent().context("Missing recovery parent")?;
            plain_chain(parent, "Recovery refuses linked folders")?;
            held_dirs.push(crate::loader_files::Directories::pin(parent)?);
            let name = row["backup"].as_str().context("Invalid backup name")?;
            ensure!(
                name.starts_with(".mod-install-")
                    && name.ends_with(".bak")
                    && !name.contains(['/', '\\', ':'])
                    && name.len() < 100,
                "Invalid backup path"
            );
            let source = parent.join(name);
            let current = fs::OpenOptions::new()
                .read(true)
                .share_mode(5)
                .custom_flags(0x00200000)
                .open(&destination)?;
            let original = fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x00200000)
                .open(&source)?;
            let cm = current.metadata()?;
            let om = original.metadata()?;
            ensure!(
                cm.is_file() && om.is_file() && !linked(&cm) && !linked(&om),
                "Linked recovery files refused"
            );
            ensure!(
                file_hash(&current)? == row["installedHash"].as_str().unwrap_or(""),
                "{} changed since installation. Restore stopped to preserve newer edits.",
                logical
            );
            ensure!(
                file_hash(&original)? == row["originalHash"].as_str().unwrap_or(""),
                "Backup verification failed for {}",
                logical
            );
            items.push(Item {
                source,
                name: logical.clone(),
                logical,
                destination,
                routing: "recovery",
                size: om.len(),
                modified: om.modified()?,
                existing: Some((cm.len(), cm.modified()?)),
            });
            held_files.push(current);
            held_files.push(original);
        }
        let plan = Plan {
            id: crate::backend::unique_id(),
            root,
            items,
            issues: vec![],
            ignored: vec![],
            prepared: Prepared {
                staging: None,
                sources: vec![],
                archives: 0,
                folders: 0,
            },
        };
        // Consume restore authority durably before effects; an interrupted restore is never replayed.
        receipt["state"] = json!("restoring");
        crate::backend::atomic_json(&path, &receipt)?;
        let result = install_recoverable(
            &plan,
            &seen,
            &HashSet::new(),
            Some(&self.recovery.join(format!("{}.json", plan.id))),
            Some(&expected),
        )?;
        receipt["state"] = json!("restored");
        crate::backend::atomic_json(&path, &receipt)?;
        Ok(result)
    }
    /// Prepares and routes a selection. Plans with routing issues install
    /// nothing (as in WPF) and are discarded immediately.
    pub fn plan(
        &self,
        game_root: &Path,
        mods_root: &Path,
        paths: &[PathBuf],
        mode: Mode,
        tar: &Path,
        limits: &Limits,
    ) -> Result<Value> {
        let mut pending = self.lock()?;
        if let Some(mut old) = pending.take() {
            old.prepared.cleanup();
        }
        purge_staging(&self.staging);
        let prepared = prepare(paths, mode, &self.staging, tar, limits)?;
        let mut plan = plan(game_root, mods_root, prepared, limits)?;
        let mut value = plan.to_json();
        if plan.issues.is_empty() {
            *pending = Some(plan);
        } else {
            plan.prepared.cleanup();
            value["planId"] = Value::Null;
        }
        Ok(value)
    }
    /// Commits the pending plan with `{planId, replace:[...], keep:[...]}`.
    /// The plan is consumed and its staging removed whatever the outcome.
    pub fn commit(&self, mods_root: &Path, args: &Value) -> Result<Value> {
        // Held for the whole install: a concurrent cancel or new plan would
        // otherwise purge the staging folder this install is still copying
        // from, possibly part-way through the commit renames.
        let mut pending = self.lock()?;
        let mut plan = pending
            .take()
            .context("This install review expired; choose the mods again.")?;
        let result = (|| -> Result<Value> {
            ensure!(
                args["planId"].as_str() == Some(plan.id.as_str()),
                "This install review expired; choose the mods again."
            );
            ensure!(
                key(&canonical_or_absolute(mods_root)?) == key(&plan.root),
                "The active mods profile changed; choose the mods again."
            );
            let set = |name: &str| -> Result<HashSet<String>> {
                let values = args[name].as_array().map(Vec::as_slice).unwrap_or(&[]);
                ensure!(values.len() <= MAX_ENTRIES, "Too many install decisions");
                values
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_lowercase)
                            .context("Invalid install decision")
                    })
                    .collect()
            };
            fs::create_dir_all(&self.recovery)?;
            plain_chain(&self.recovery, "Recovery refuses linked folders")?;
            let _pin = crate::loader_files::Directories::pin(&self.recovery)?;
            install_recoverable(
                &plan,
                &set("replace")?,
                &set("keep")?,
                Some(&self.recovery.join(format!("{}.json", plan.id))),
                None,
            )
        })();
        plan.prepared.cleanup();
        drop(pending);
        result
    }
}

#[cfg(test)]
#[path = "mod_install_tests.rs"]
mod tests;
