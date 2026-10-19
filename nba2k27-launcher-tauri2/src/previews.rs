//! Optional 256 px renders stay in a persistent ZIP; only requested thumbnails are cached.
use crate::backend::atomic_json;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use zip::ZipArchive;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    name: String,
    count: usize,
    bytes: u64,
}
struct Active {
    manifest: Manifest,
    zip: ZipArchive<File>,
}
pub struct Previews {
    root: PathBuf,
    active: Mutex<Option<Active>>,
    problem: Mutex<Option<String>>,
    importing: Mutex<()>,
}
fn valid_key(key: &str) -> bool {
    let parts: Vec<_> = key.split('/').collect();
    parts.len() == 3
        && matches!(
            parts[0],
            "blacktop-top-previews"
                | "blacktop-bottom-previews"
                | "blacktop-shoe-previews"
                | "blacktop-accessory-previews"
                | "blacktop-sock-previews"
                | "blacktop-ball-previews"
        )
        && parts[1] == "thumbnails"
        && parts[2].ends_with(".png")
        && !parts[2].contains("..")
        && parts[2]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn png(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() >= 33
            && bytes[..8] == [137, 80, 78, 71, 13, 10, 26, 10]
            && &bytes[12..16] == b"IHDR",
        "Not a PNG thumbnail"
    );
    ensure!(
        u32::from_be_bytes(bytes[16..20].try_into()?) == 256
            && u32::from_be_bytes(bytes[20..24].try_into()?) == 256,
        "Preview must be exactly 256 × 256"
    );
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 8 * 1024 * 1024,
    });
    let mut reader = decoder.read_info()?;
    let size = reader
        .output_buffer_size()
        .context("PNG output size is invalid")?;
    ensure!(size <= 1024 * 1024, "PNG output exceeds limit");
    reader.next_frame(&mut vec![0; size])?;
    Ok(())
}
fn open_zip(path: &Path) -> Result<ZipArchive<File>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    ensure!(
        (22..=2 * 1024 * 1024 * 1024).contains(&len),
        "Preview ZIP size is invalid"
    );
    let tail_len = len.min(65557) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail)?;
    let index = (0..=tail_len - 22)
        .rev()
        .find(|&i| {
            tail[i..i + 4] == [80, 75, 5, 6]
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail_len
        })
        .context("ZIP directory is missing")?;
    let end = &tail[index..];
    ensure!(
        end[4..8] == [0, 0, 0, 0],
        "Multi-disk ZIPs are not supported"
    );
    let count = u16::from_le_bytes([end[10], end[11]]);
    ensure!(
        count > 0 && count <= 40000 && end[8..10] == end[10..12],
        "ZIP entry count is invalid or requires ZIP64"
    );
    let size = u32::from_le_bytes(end[12..16].try_into()?) as u64;
    let offset = u32::from_le_bytes(end[16..20].try_into()?) as u64;
    ensure!(
        size <= 12 * 1024 * 1024
            && offset
                .checked_add(size)
                .is_some_and(|v| v <= len - tail_len as u64 + index as u64),
        "ZIP directory exceeds safety limits"
    );
    file.seek(SeekFrom::Start(0))?;
    Ok(ZipArchive::new(file)?)
}
fn read_image(zip: &mut ZipArchive<File>, index: usize) -> Result<Vec<u8>> {
    let mut entry = zip.by_index(index)?;
    ensure!(
        entry.size() > 0 && entry.size() <= 4 * 1024 * 1024,
        "Preview exceeds 4 MB"
    );
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 == entry.size(),
        "Preview size or ZIP CRC is invalid"
    );
    png(&bytes)?;
    Ok(bytes)
}
fn digest_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn validate(source: &Path, progress: impl Fn(usize, usize, &str)) -> Result<Value> {
    let mut zip = open_zip(source)?;
    ensure!(
        zip.len() > 0 && zip.len() <= 40000,
        "Preview ZIP entry count is invalid"
    );
    let total = zip.len();
    let mut seen = HashSet::new();
    let mut count = 0;
    let mut expanded = 0u64;
    for index in 0..total {
        let entry = zip.by_index(index)?;
        let key = entry.name().to_string();
        if entry.is_dir() {
            ensure!(
                !key.contains("..") && !key.contains('\\') && !key.starts_with('/'),
                "Unsafe ZIP directory"
            );
            continue;
        }
        ensure!(valid_key(&key), "Unsupported or unsafe preview path: {key}");
        ensure!(
            seen.insert(key.to_lowercase()),
            "Duplicate preview ID: {key}"
        );
        expanded += entry.size();
        ensure!(
            expanded <= 2 * 1024 * 1024 * 1024,
            "Expanded previews exceed 2 GB"
        );
        drop(entry);
        read_image(&mut zip, index).with_context(|| format!("Invalid thumbnail: {key}"))?;
        count += 1;
        if index % 100 == 0 || index + 1 == total {
            progress(index + 1, total, "Checking 256 px thumbnails");
        }
    }
    ensure!(count > 0, "ZIP contains no previews");
    Ok(json!({"count":count,"entries":total,"expandedBytes":expanded}))
}
impl Previews {
    pub fn new(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("Library/Previews"))?;
        fs::create_dir_all(root.join("Cache/PreviewImages"))?;
        let p = root.join("Data/preview-pack.json");
        let loaded = (|| -> Result<Option<Active>> {
            Ok(if p.exists() {
                let m: Manifest = serde_json::from_slice(&fs::read(p)?)
                    .context("Preview pack manifest is invalid")?;
                ensure!(
                    !m.id.is_empty()
                        && m.id.len() <= 64
                        && m.id.bytes().all(|b| b.is_ascii_hexdigit()),
                    "Preview pack ID is invalid"
                );
                let file = root.join("Library/Previews").join(format!("{}.zip", m.id));
                // Missing optional content should not prevent the launcher starting.
                if file.is_file() {
                    Some(Active {
                        manifest: m,
                        zip: open_zip(&file)?,
                    })
                } else {
                    None
                }
            } else {
                None
            })
        })();
        let (active, problem) = match loaded {
            Ok(active) => (active, None),
            Err(e) => (None, Some(format!("{e:#}"))),
        };
        Ok(Self {
            root: root.into(),
            active: Mutex::new(active),
            problem: Mutex::new(problem),
            importing: Mutex::new(()),
        })
    }
    pub fn status(&self) -> Result<Value> {
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview pack unavailable"))?;
        let problem = self
            .problem
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview status unavailable"))?;
        Ok(if let Some(e) = problem.as_ref() {
            json!({"name":"Pack unavailable","count":0,"error":e})
        } else {
            active.as_ref().map(|a|json!({"name":a.manifest.name,"count":a.manifest.count,"bytes":a.manifest.bytes})).unwrap_or(json!(null))
        })
    }
    pub fn import(&self, source: &Path, progress: impl Fn(usize, usize, &str)) -> Result<Value> {
        let _guard = self
            .importing
            .try_lock()
            .map_err(|_| anyhow::anyhow!("A preview import is already running"))?;
        ensure!(
            source
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip")),
            "Select a preview ZIP"
        );
        let bytes = fs::metadata(source)?.len();
        ensure!(
            bytes > 0 && bytes <= 2 * 1024 * 1024 * 1024,
            "Preview ZIP must be between 1 byte and 2 GB"
        );
        let id = crate::backend::unique_id();
        let temp = self
            .root
            .join("Cache/Imports")
            .join(format!("preview-{id}.importing"));
        let result = (|| -> Result<Value> {
            progress(0, 1, "Copying ZIP into portable library");
            let mut input = File::open(source)?;
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            let mut copied = 0;
            let mut hash = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                let n = input.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                out.write_all(&buffer[..n])?;
                hash.update(&buffer[..n]);
                copied += n as u64;
                ensure!(copied <= bytes, "Source changed during import");
                if copied % (16 * 1024 * 1024) == 0 {
                    progress(
                        copied as usize,
                        bytes as usize,
                        "Copying ZIP into portable library",
                    );
                }
            }
            ensure!(copied == bytes, "Source changed during import");
            out.sync_all()?;
            drop(out);
            drop(input);
            let verified = validate(&temp, &progress)?;
            let count = verified["count"].as_u64().unwrap() as usize;
            let total = verified["entries"].as_u64().unwrap() as usize;
            let mut id = format!("{:x}", hash.finalize());
            let mut destination = self.root.join("Library/Previews").join(format!("{id}.zip"));
            let reused = destination.is_file() && digest_file(&destination)? == id;
            if reused {
                fs::remove_file(&temp)?;
            } else {
                if destination.exists() {
                    id = crate::backend::unique_id();
                    destination = self.root.join("Library/Previews").join(format!("{id}.zip"));
                }
                fs::rename(&temp, &destination)?;
            }
            let manifest = Manifest {
                id,
                name: source
                    .file_name()
                    .context("Filename missing")?
                    .to_string_lossy()
                    .into(),
                count,
                bytes,
            };
            let zip = open_zip(&destination)?;
            let mut active = self
                .active
                .lock()
                .map_err(|_| anyhow::anyhow!("Preview pack unavailable"))?;
            atomic_json(&self.root.join("Data/preview-pack.json"), &manifest)?;
            *active = Some(Active { manifest, zip });
            *self
                .problem
                .lock()
                .map_err(|_| anyhow::anyhow!("Preview status unavailable"))? = None;
            progress(total, total, "Preview pack ready");
            Ok(json!({"count":count,"bytes":bytes,"reused":reused}))
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
    pub fn image(&self, key: &str) -> Result<Option<PathBuf>> {
        if !valid_key(key) {
            return Ok(None);
        }
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Preview pack unavailable"))?;
        let Some(active) = active.as_mut() else {
            return Ok(None);
        };
        let path = self
            .root
            .join("Cache/PreviewImages")
            .join(&active.manifest.id)
            .join(key);
        if path.is_file() {
            return Ok(Some(path));
        }
        let Some(index) = active.zip.index_for_name(key) else {
            return Ok(None);
        };
        let bytes = match read_image(&mut active.zip, index) {
            Ok(bytes) => bytes,
            Err(e) => {
                *self
                    .problem
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Preview status unavailable"))? =
                    Some(format!("{e:#}"));
                return Ok(None);
            }
        };
        fs::create_dir_all(path.parent().unwrap())?;
        let temp = path.with_extension("importing");
        fs::write(&temp, bytes)?;
        fs::rename(temp, &path)?;
        Ok(Some(path))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_and_dimensions() {
        assert!(valid_key("blacktop-top-previews/thumbnails/a.png"));
        for key in [
            "../a.png",
            "blacktop-top-previews/thumbnails/../a.png",
            "blacktop-top-previews/thumbnails/C:a.png",
            "images/a.png",
        ] {
            assert!(!valid_key(key))
        }
        assert!(png(b"not an image").is_err());
    }
    #[test]
    fn damaged_optional_zip_does_not_block_startup() {
        let root = std::env::temp_dir().join(format!(
            "tauri-preview-test-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir_all(root.join("Data")).unwrap();
        fs::create_dir_all(root.join("Library/Previews")).unwrap();
        fs::write(root.join("Library/Previews/abcdef.zip"), b"bad zip").unwrap();
        atomic_json(
            &root.join("Data/preview-pack.json"),
            &Manifest {
                id: "abcdef".into(),
                name: "broken.zip".into(),
                count: 1,
                bytes: 7,
            },
        )
        .unwrap();
        let previews = Previews::new(&root).unwrap();
        assert!(previews.status().unwrap()["error"].is_string());
        assert!(
            previews
                .image("blacktop-top-previews/thumbnails/a.png")
                .unwrap()
                .is_none()
        );
        drop(previews);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn import_persists_and_rejects_bad_pack() {
        let root = std::env::temp_dir().join(format!(
            "tauri-preview-test-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir_all(root.join("Data")).unwrap();
        fs::create_dir_all(root.join("Cache/Imports")).unwrap();
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 256, 256);
            encoder.set_color(png::ColorType::Rgba);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![0; 256 * 256 * 4])
                .unwrap();
        }
        let source = root.join("test.zip");
        let mut writer = zip::ZipWriter::new(File::create(&source).unwrap());
        writer
            .start_file(
                "blacktop-top-previews/thumbnails/a.png",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(&bytes).unwrap();
        writer.finish().unwrap();
        let previews = Previews::new(&root).unwrap();
        previews.import(&source, |_, _, _| {}).unwrap();
        assert_eq!(
            previews.import(&source, |_, _, _| {}).unwrap()["reused"],
            true
        );
        assert_eq!(
            fs::read_dir(root.join("Library/Previews")).unwrap().count(),
            1
        );
        let image = previews
            .image("blacktop-top-previews/thumbnails/a.png")
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(image).unwrap(), bytes);
        let manifest = fs::read(root.join("Data/preview-pack.json")).unwrap();
        drop(previews);
        assert_eq!(Previews::new(&root).unwrap().status().unwrap()["count"], 1);
        let bad = root.join("bad.zip");
        let mut writer = zip::ZipWriter::new(File::create(&bad).unwrap());
        writer
            .start_file("../escape.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
        writer.finish().unwrap();
        let previews = Previews::new(&root).unwrap();
        assert!(previews.import(&bad, |_, _, _| {}).is_err());
        assert_eq!(
            fs::read(root.join("Data/preview-pack.json")).unwrap(),
            manifest
        );
        drop(previews);
        fs::remove_dir_all(root).unwrap();
    }
}
