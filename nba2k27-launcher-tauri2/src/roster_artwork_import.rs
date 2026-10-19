//! Native manual artwork preparation. Paths never cross the browser boundary.
//! A chooser prepares one expiring candidate; only a matching, still-current
//! editor commit publishes the WPF-compatible local library. No game access.
use crate::{file_identity::Stable, loader_files::Directories};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
const INPUT: usize = 16 * 1024 * 1024;
const LIBRARY: usize = 32 * 1024 * 1024;
const URL: usize = 2 * 1024 * 1024;
#[derive(Clone, PartialEq)]
struct Owner {
    tab: String,
    kind: String,
    key: String,
    destination: Value,
}
impl Owner {
    fn parse(tab: &str, v: &Value) -> Result<Self> {
        ensure!(
            tab.len() == 36
                && tab
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }),
            "Invalid artwork editor identity"
        );
        let o = v.as_object().context("Invalid artwork request")?;
        ensure!(
            o.keys()
                .all(|k| ["kind", "key", "choose", "action", "id", "destination"]
                    .contains(&k.as_str())),
            "Unsupported artwork request option"
        );
        let kind = v["kind"]
            .as_str()
            .filter(|s| matches!(*s, "portrait" | "logo"))
            .context("Invalid artwork kind")?;
        let key = v["key"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256 && !s.contains('\0'))
            .context("Invalid artwork key")?;
        let d = v["destination"]
            .as_object()
            .context("Artwork requires a live player destination")?;
        ensure!(
            d.len() == 4
                && d.keys()
                    .all(|k| ["recordIndex", "firstName", "lastName", "cyberfaceId"]
                        .contains(&k.as_str())),
            "Invalid artwork destination"
        );
        ensure!(
            d["recordIndex"].as_u64().is_some_and(|n| n < 65536)
                && d["cyberfaceId"].as_u64().is_some_and(|n| n <= 65535),
            "Invalid artwork player index"
        );
        for k in ["firstName", "lastName"] {
            ensure!(
                d[k].as_str()
                    .is_some_and(|s| s.len() <= 256 && !s.contains('\0')),
                "Invalid artwork player name"
            );
        }
        Ok(Self {
            tab: tab.to_owned(),
            kind: kind.into(),
            key: key.into(),
            destination: v["destination"].clone(),
        })
    }
}
#[derive(PartialEq)]
struct Witness {
    id: Stable,
    bytes: Vec<u8>,
}
struct Pending {
    id: String,
    owner: Owner,
    path: PathBuf,
    before: Option<Witness>,
    value: String,
    since: Instant,
    _dirs: Directories,
}
#[derive(Default)]
pub(crate) struct Store {
    pending: Mutex<Option<Pending>>,
}
fn read(path: &Path, limit: usize, check: &dyn Fn() -> Result<()>) -> Result<(File, Witness)> {
    check()?;
    let _dirs = Directories::pin(path.parent().context("Artwork source parent missing")?)?;
    let mut f = OpenOptions::new()
        .read(true)
        .share_mode(1 | 2 | 4)
        .custom_flags(0x00200000)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= limit as u64,
        "Artwork must be a bounded ordinary file"
    );
    let id = Stable::read(&f)?;
    let mut a = Vec::new();
    (&mut f).take(limit as u64 + 1).read_to_end(&mut a)?;
    ensure!(
        a.len() == m.len() as usize,
        "Artwork source was partially read"
    );
    check()?;
    f.seek(SeekFrom::Start(0))?;
    let mut b = Vec::new();
    (&mut f).take(limit as u64 + 1).read_to_end(&mut b)?;
    let end = f.metadata()?;
    ensure!(
        a == b
            && end.len() == m.len()
            && end.last_write_time() == m.last_write_time()
            && Stable::read(&f)? == id,
        "Artwork source changed during verification"
    );
    check()?;
    Ok((f, Witness { id, bytes: a }))
}
fn library(path: &Path, check: &dyn Fn() -> Result<()>) -> Result<Option<Witness>> {
    match read(path, LIBRARY, check) {
        Ok((_, w)) => Ok(Some(w)),
        Err(e)
            if e.downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}
fn image(input: &[u8], check: &dyn Fn() -> Result<()>) -> Result<String> {
    use windows::{
        Win32::{Graphics::Imaging::*, System::Com::*},
        core::HRESULT,
    };
    ensure!(
        (12..=INPUT).contains(&input.len()),
        "Choose an image of at most 16 MiB"
    );
    let webp = &input[..4] == b"RIFF" && &input[8..12] == b"WEBP";
    ensure!(
        input.starts_with(b"\x89PNG\r\n\x1a\n") || input.starts_with(b"\xff\xd8\xff") || webp,
        "Choose a PNG, JPEG or WebP image"
    );
    check()?;
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() }
            }
        }
    }
    let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    ensure!(
        status.is_ok() || status == HRESULT(0x80010106u32 as i32),
        "Artwork decoder apartment initialization failed"
    );
    let _apartment = Apartment(status.is_ok());
    let (w, h, rgba) = unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(input)?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .context("Image decoder unavailable or image invalid; try PNG or JPEG")?;
        ensure!(decoder.GetFrameCount()? == 1, "Choose a single-frame image");
        let frame = decoder.GetFrame(0)?;
        let (mut w, mut h) = (0, 0);
        frame.GetSize(&mut w, &mut h)?;
        ensure!(
            (1..=8192).contains(&w)
                && (1..=8192).contains(&h)
                && u64::from(w) * u64::from(h) <= 16_777_216,
            "Artwork exceeds the safe 16-megapixel limit"
        );
        let (tw, th) = dimensions(w, h);
        check()?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(&converter, tw, th, WICBitmapInterpolationModeFant)?;
        let mut rgba = vec![0; (tw * th * 4) as usize];
        scaler.CopyPixels(std::ptr::null(), tw * 4, &mut rgba)?;
        (tw, th, rgba)
    };
    check()?;
    let png = crate::roster_artwork::png(w as usize, h as usize, &rgba)?;
    let value = format!(
        "data:image/png;base64,{}",
        crate::roster_artwork::base64(&png)
    );
    ensure!(value.len() <= URL, "Prepared artwork exceeds 2 MiB");
    check()?;
    Ok(value)
}
fn dimensions(w: u32, h: u32) -> (u32, u32) {
    if w.max(h) <= 512 {
        (w, h)
    } else if w >= h {
        (
            512,
            ((u64::from(h) * 512 + u64::from(w) / 2) / u64::from(w)).max(1) as u32,
        )
    } else {
        (
            ((u64::from(w) * 512 + u64::from(h) / 2) / u64::from(h)).max(1) as u32,
            512,
        )
    }
}
impl Store {
    pub(crate) fn request(
        &self,
        root: &Path,
        tab: &str,
        v: &Value,
        check: &dyn Fn() -> Result<()>,
        choose: &mut dyn FnMut() -> Result<Option<PathBuf>>,
    ) -> Result<Value> {
        check()?;
        let owner = Owner::parse(tab, v)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| anyhow::anyhow!("Artwork preparation unavailable"))?;
        let action = if v["choose"] == true {
            ensure!(
                v.get("action").is_none() && v.get("id").is_none(),
                "Invalid artwork preparation"
            );
            "prepare"
        } else {
            v["action"].as_str().context("Invalid artwork action")?
        };
        if action == "prepare" {
            // Hold the serialized candidate slot through the owned dialog. Closing
            // the page cannot publish; a stale prepare result is canceled by UI.
            *pending = None;
            let Some(source) = choose()? else {
                return Ok(Value::Null);
            };
            check()?;
            let (_file, source) = read(&source, INPUT, check)?;
            let value = image(&source.bytes, check)?;
            let dir = root.join("Data");
            match fs::create_dir(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
            let dirs = Directories::pin(&dir)?;
            let path = dir.join("player-editor-artwork.json");
            let before = library(&path, check)?;
            if let Some(w) = &before {
                ensure!(
                    serde_json::from_slice::<Value>(&w.bytes)?.is_object(),
                    "Artwork library is invalid"
                );
            }
            check()?;
            let id = crate::backend::unique_id();
            let receipt = json!({"id":id,"dataUrl":value});
            *pending = Some(Pending {
                id,
                owner,
                path,
                before,
                value,
                since: Instant::now(),
                _dirs: dirs,
            });
            return Ok(receipt);
        }
        ensure!(
            matches!(action, "commit" | "cancel") && v.get("choose").is_none(),
            "Unsupported artwork action"
        );
        let id = v["id"]
            .as_str()
            .context("Artwork preparation identity missing")?;
        let p = pending.as_ref().context("Artwork preparation expired")?;
        ensure!(
            p.id == id
                && p.owner == owner
                && p.path == root.join("Data/player-editor-artwork.json"),
            "Artwork preparation belongs to another editor"
        );
        if action == "cancel" {
            *pending = None;
            return Ok(json!({"canceled":true}));
        }
        ensure!(
            p.since.elapsed() < Duration::from_secs(60),
            "Artwork preparation expired; choose the image again"
        );
        check()?;
        ensure!(
            library(&p.path, check)? == p.before,
            "Artwork library changed. Choose the image again"
        );
        let mut document = match &p.before {
            Some(w) => serde_json::from_slice::<Value>(&w.bytes)?,
            None => json!({}),
        };
        let key = format!(
            "{:x}",
            Sha256::digest(format!("{}:{}", p.owner.kind, p.owner.key).as_bytes())
        );
        document
            .as_object_mut()
            .context("Invalid artwork library")?
            .insert(key, Value::String(p.value.clone()));
        ensure!(
            serde_json::to_vec_pretty(&document)?.len() <= LIBRARY,
            "Artwork library exceeds 32 MiB"
        );
        check()?;
        // Namespace race qualification: this is the existing settings atomic-
        // replace model, not cross-process compare/exchange. Check immediately
        // before replacement and retain pinned ancestors throughout publication.
        ensure!(
            library(&p.path, check)? == p.before,
            "Artwork library changed. Choose the image again"
        );
        check()?;
        crate::backend::atomic_json(&p.path, &document)?;
        let value = p.value.clone();
        *pending = None;
        Ok(Value::String(value))
    }
}
#[cfg(test)]
#[path = "roster_artwork_import_tests.rs"]
mod tests;
