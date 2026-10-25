use crate::{
    catalog::{Catalog, safe_relative},
    codec::{self, Archive, Oodle},
};
use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
pub const VERSION: &str = "0.1.27";
pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;
/// Rate-limit conversion/CRC telemetry, not work, cancellation or validation.
/// Emit every changed percentage and a keepalive at least every 50ms while
/// callbacks arrive. Hundreds of thousands of member events otherwise flood IPC.
#[derive(Default)]
struct ExportProgressGate {
    previous: Option<(u64, Instant)>,
}
impl ExportProgressGate {
    fn allow(&mut self, percent: u64, now: Instant) -> bool {
        if self
            .previous
            .is_none_or(|(old, time)| old != percent || now.duration_since(time).as_millis() >= 50)
        {
            self.previous = Some((percent, now));
            true
        } else {
            false
        }
    }
}
pub struct Backend {
    pub catalog: Option<Catalog>,
    pub archive: Option<(usize, Archive)>,
    archive_bytes: usize,
    recipe: Option<(usize, crate::materials::Recipe)>,
    apparel: Option<(usize, crate::apparel::Session)>,
    pub output: Option<PathBuf>,
    pub last_output: Option<PathBuf>,
    pub cancel: Arc<AtomicBool>,
    pub emit: Emit,
}
impl Backend {
    pub fn new(emit: Emit, cancel: Arc<AtomicBool>) -> Self {
        Self {
            catalog: None,
            archive: None,
            archive_bytes: 0,
            recipe: None,
            apparel: None,
            output: None,
            last_output: None,
            cancel,
            emit,
        }
    }
    pub fn event(&self, phase: &str, percent: u64) {
        (self.emit)(
            json!({"kind":"event","channel":"progress","payload":{"phase":phase,"percent":percent}}),
        );
    }
    pub fn catalog(&self) -> Result<&Catalog> {
        self.catalog
            .as_ref()
            .context("Load the installed game first")
    }
    fn load_archive(&mut self, id: usize) -> Result<()> {
        if self
            .archive
            .as_ref()
            .is_some_and(|(cached, _)| *cached == id)
        {
            return Ok(());
        }
        self.archive = None;
        self.archive_bytes = 0;
        self.recipe = None;
        self.apparel = None;
        self.event("Reading archive range", 5);
        let catalog = self.catalog()?;
        let resource = catalog.entry(id)?;
        let packed = codec::range(catalog, resource)?;
        self.event("Native Oodle container decode", 20);
        let oodle = Oodle::load(&self.catalog()?.root)?;
        let bytes = if resource.name.to_lowercase().ends_with(".iff")
            || packed.starts_with(b"PK\x03\x04")
        {
            crate::archive_decode::preview(&oodle, &catalog.root, &packed, &self.cancel)?
        } else {
            crate::exporter::inspection_container(&resource.name, packed, &self.cancel)?
        };
        self.event("Reading IFF directory", 65);
        let size = bytes.len();
        let archive = Archive::open_with_cancel(bytes, &self.cancel)?;
        self.archive_bytes = size;
        self.archive = Some((id, archive));
        Ok(())
    }
    fn source_archive(&self, id: usize) -> Result<Archive> {
        let catalog = self.catalog()?;
        let resource = catalog.entry(id)?;
        let packed = codec::range(catalog, resource)?;
        let oodle = Oodle::load(&catalog.root)?;
        let bytes = if resource.name.to_lowercase().ends_with(".iff")
            || packed.starts_with(b"PK\x03\x04")
        {
            crate::archive_decode::preview(&oodle, &catalog.root, &packed, &self.cancel)?
        } else {
            crate::exporter::inspection_container(&resource.name, packed, &self.cancel)?
        };
        let archive = Archive::open_with_cancel(bytes, &self.cancel)?;
        archive.require_unique_members()?;
        Ok(archive)
    }

    /// Open one loose mod IFF while keeping the installed-game catalog loaded
    /// for referenced textures, materials and companion resources. The launcher
    /// supplies both the selected file and its allowed mod-library root; resolve
    /// both before reading so junctions and traversal cannot escape that root.
    fn load_loose_archive(&mut self, path: &Path, allowed_root: &Path) -> Result<()> {
        let root = allowed_root
            .canonicalize()
            .context("Mod library folder is not accessible")?;
        ensure!(root.is_dir(), "Mod library folder is not a directory");
        let path = path
            .canonicalize()
            .context("Selected mod file is not accessible")?;
        ensure!(
            path.starts_with(&root),
            "Selected mod file is outside the mod library"
        );
        let metadata = path.metadata()?;
        ensure!(metadata.is_file(), "Selected mod resource is not a file");
        ensure!(
            metadata.len() <= crate::container::MAX as u64,
            "Selected mod resource exceeds the 1 GiB preview limit"
        );
        let lower = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        ensure!(
            lower.ends_with(".iff") || lower.ends_with(".iff.bin"),
            "Only loose IFF model resources can be previewed"
        );

        self.archive = None;
        self.archive_bytes = 0;
        self.recipe = None;
        self.apparel = None;
        self.event("Reading selected mod", 5);
        let mut input = fs::File::open(&path)?;
        let mut packed = Vec::with_capacity(metadata.len() as usize);
        let mut chunk = [0u8; 1024 * 1024];
        loop {
            codec::cancelled(&self.cancel)?;
            let read = input.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            packed.extend_from_slice(&chunk[..read]);
        }
        ensure!(
            packed.len() as u64 == metadata.len(),
            "Selected mod changed while it was being read"
        );
        self.event("Decoding selected mod", 20);
        let catalog = self.catalog()?;
        let oodle = Oodle::load(&catalog.root)?;
        let bytes = if packed.starts_with(b"PK\x03\x04") {
            packed
        } else {
            crate::archive_decode::preview(&oodle, &catalog.root, &packed, &self.cancel)?
        };
        self.event("Reading selected mod directory", 65);
        let size = bytes.len();
        let archive = Archive::open_with_cancel(bytes, &self.cancel)?;
        self.archive_bytes = size;
        self.archive = Some((usize::MAX, archive));
        Ok(())
    }
    pub fn dispatch(&mut self, channel: &str, args: &[Value]) -> Result<Value> {
        let a = args.first().cloned().unwrap_or(Value::Null);
        match channel {
            "app:licenses" => Ok(json!(crate::host::license_text())),
            "diagnostics:prepare" => Ok(crate::diagnostics::sanitized(&a)),
            "cache:status" => Ok(
                json!({"bytes":if self.archive.is_some(){self.archive_bytes}else{0},"scope":"source container; renderer/GPU and companion allocations are not included","files":0,"sessions":usize::from(self.archive.is_some()),"diskCache":false,"limitBytes":crate::container::MAX}),
            ),
            "cache:clear" => {
                self.archive = None;
                self.recipe = None;
                self.apparel = None;
                Ok(json!({"cleared":true,"bytes":0,"files":0,"diskCache":false}))
            }
            "app:bootstrap" => Ok(
                json!({"version":VERSION,"defaultGameRoot":"C:\\Program Files (x86)\\Steam\\steamapps\\common\\NBA 2K27","defaultOutputRoot":std::env::var("USERPROFILE").map(|p|PathBuf::from(p).join("Documents/NBA 2K27 Exports")).unwrap_or_default(),"runtime":"Native desktop + shared WebView2; no .NET / Node / Electron","coverage":"Native mod-ready and original-container exports, file inspection, player configurations, shoe/clothing designs, hair and material-linked arena previews."}),
            ),
            "game:load" => {
                self.cancel.store(false, Ordering::Relaxed);
                self.event("Indexing installed manifest", 5);
                let t = Instant::now();
                let catalog = Catalog::load_with_cancel(
                    Path::new(a.as_str().context("Game folder missing")?),
                    &self.cancel,
                )?;
                let archives: std::collections::HashSet<_> =
                    catalog.entries.iter().map(|e| e.archive.clone()).collect();
                let result = json!({"resources":catalog.entries.len(),"archives":archives.len(),"gameRoot":catalog.root,"indexMs":t.elapsed().as_secs_f64()*1000.,"invalidRows":catalog.invalid_rows,"kinds":catalog.kinds()});
                self.catalog = Some(catalog);
                self.archive = None;
                self.recipe = None;
                self.apparel = None;
                self.event("Manifest indexed", 100);
                Ok(result)
            }
            "catalog:search" => {
                self.cancel.store(false, Ordering::Relaxed);
                self.catalog()?.search_with_cancel(&a, &self.cancel)
            }
            "catalog:folders" => self.catalog()?.folder_list(a.as_str().unwrap_or("")),
            "catalog:inspect" => {
                let id = a.as_u64().context("Missing resource id")? as usize;
                let entry = self.catalog()?.entry(id)?;
                use std::io::{Read, Seek, SeekFrom};
                let mut file = fs::File::open(self.catalog()?.root.join(&entry.archive))?;
                ensure!(
                    entry.offset <= file.metadata()?.len()
                        && entry.size <= file.metadata()?.len() - entry.offset,
                    "Resource range outside archive"
                );
                file.seek(SeekFrom::Start(entry.offset))?;
                let mut header = vec![0u8; entry.size.min(64) as usize];
                file.read_exact(&mut header)?;
                let format = if codec::vcz(&header) {
                    "VCZ compressed buffer"
                } else if header.starts_with(b"PK\x03\x04") {
                    "ZIP / IFF container"
                } else if header.starts_with(b"DDS ") {
                    "DirectDraw texture"
                } else if header.starts_with(b"TLD") {
                    "2K texture (exports as DDS)"
                } else {
                    "Direct archive resource"
                };
                let mut metadata = serde_json::to_value(entry)?;
                metadata["format"] = json!(format);
                metadata["kind"] = json!(crate::catalog::classify(&entry.name));
                metadata["signature"] = json!(
                    header
                        .iter()
                        .take(16)
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                Ok(metadata)
            }
            "catalog:resolve-resource" => {
                let name = a.as_str().context("Resource path missing")?;
                let catalog = self.catalog()?;
                let entry = catalog.resolve(name).or_else(|original| {
                    if name.to_lowercase().ends_with(".dds") {
                        catalog.resolve(&format!("{}.tld", &name[..name.len() - 4]))
                    } else {
                        Err(original)
                    }
                })?;
                Ok(serde_json::to_value(entry)?)
            }
            "file:inspect" => {
                self.cancel.store(false, Ordering::Relaxed);
                let id = a.as_u64().context("Missing resource id")? as usize;
                self.load_archive(id)?;
                let archive = &mut self.archive.as_mut().unwrap().1;
                let mut members = archive.names()?;
                for value in &mut members {
                    codec::cancelled(&self.cancel)?;
                    let member = archive
                        .member(value["id"].as_u64().context("Missing member ID")? as usize)?;
                    let metadata = crate::inspection::member_summary(
                        &member.name,
                        &member.compression,
                        member.encrypted,
                    );
                    for field in ["kind", "compression", "status"] {
                        value[field] = metadata[field].clone();
                    }
                    value["dataOffset"] = json!(member.data_offset);
                }
                self.event("IFF ready", 100);
                Ok(json!({"id":id,"members":members}))
            }
            "file:preview-member" => {
                self.cancel.store(false, Ordering::Relaxed);
                let index = a.as_u64().context("Missing member id")? as usize;
                let (resource_id, archive) =
                    self.archive.as_mut().context("Inspect an IFF first")?;
                let (name, source_bytes) = {
                    let member = archive.member(index)?;
                    (member.name, member.size)
                };
                if source_bytes > crate::inspection::MEMBER_MAX as u64 {
                    return Ok(crate::inspection::notice(&name, source_bytes));
                }
                let bytes =
                    archive.read_with_cancel(index, crate::inspection::MEMBER_MAX, &self.cancel)?;
                let ext = Path::new(&name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(ext.as_str(), "scne" | "txtr" | "json") {
                    let bytes = if codec::vcz(&bytes) {
                        let oodle =
                            Oodle::load(&self.catalog.as_ref().context("Catalog unloaded")?.root)?;
                        match crate::buffer_decode::decode(&bytes, None, &oodle, &self.cancel) {
                            Ok(decoded) => decoded,
                            Err(e) => {
                                codec::cancelled(&self.cancel)?;
                                let mut view =
                                    crate::inspection::preview(&name, &bytes, source_bytes);
                                view["notice"] = json!(format!(
                                    "Document decode unavailable: {e:#}. Save original preserves the complete source."
                                ));
                                return Ok(view);
                            }
                        }
                    } else {
                        bytes
                    };
                    Ok(crate::inspection::preview(&name, &bytes, source_bytes))
                } else if bytes.starts_with(b"DDS ") || bytes.starts_with(b"TLD") {
                    ensure!(
                        bytes.len() <= 32 * 1024 * 1024,
                        "Texture member exceeds 32 MiB preview budget; Save original is unaffected"
                    );
                    let direct_dds = bytes.starts_with(b"DDS ");
                    let dds = if direct_dds {
                        ensure!(
                            bytes.len() <= 16 * 1024 * 1024,
                            "Texture exceeds 16 MiB viewing budget; Save original is unaffected"
                        );
                        crate::validation::dds(&bytes[..bytes.len().min(148)], bytes.len() as u64)?;
                        bytes
                    } else {
                        crate::textures::one(
                            archive,
                            self.catalog.as_ref().context("Catalog unloaded")?,
                            &name,
                            &self.cancel,
                        )?
                    };
                    let mut view = texture(&name, &dds);
                    if direct_dds {
                        view["sourceMember"] = json!({"resourceId":resource_id,"index":index});
                    }
                    Ok(view)
                } else {
                    Ok(crate::inspection::preview(&name, &bytes, source_bytes))
                }
            }
            "file:save-member" => {
                let index = a.as_u64().context("Missing member id")? as usize;
                let decoded = args.get(1).and_then(Value::as_bool).unwrap_or(false);
                let (resource, archive) = self.archive.as_mut().context("Inspect an IFF first")?;
                let mut name = archive.member(index)?.name;
                if decoded
                    && (name.to_lowercase().ends_with(".scne")
                        || name.to_lowercase().ends_with(".txtr"))
                {
                    name.push_str(".json");
                }
                Ok(
                    json!({"nativeSave":true,"nativeMember":true,"name":name,"index":index,"resourceId":resource,"decoded":decoded,"gameRoot":self.catalog()?.root}),
                )
            }
            "file:publish-member" => self.publish_member(&a),
            "preview:load" => {
                self.cancel.store(false, Ordering::Relaxed);
                let id = a.as_u64().context("Missing resource id")? as usize;
                let selected_config = args
                    .get(1)
                    .map(|v| v.as_str().context("Player configuration must be a string"))
                    .transpose()?;
                self.load_archive(id)?;
                self.archive.as_ref().unwrap().1.require_unique_members()?;
                self.event("Decoding native geometry", 70);
                let catalog = self.catalog.as_ref().context("Catalog unloaded")?;
                let name = catalog.entry(id)?.name.clone();
                let oodle = Oodle::load(&catalog.root)?;
                let mut out = crate::preview::build(
                    &mut self.archive.as_mut().unwrap().1,
                    catalog,
                    &oodle,
                    &name,
                    &self.cancel,
                )?;
                crate::tangent::apply(&mut out, catalog, &oodle, &name, &self.cancel)?;
                self.event("Loading player appearance and textures", 85);
                crate::player::apply(
                    &mut out,
                    &mut self.archive.as_mut().unwrap().1,
                    catalog,
                    &oodle,
                    &name,
                    selected_config,
                    None,
                    &self.cancel,
                )?;
                let recipe = crate::materials::Recipe::read(
                    &mut self.archive.as_mut().unwrap().1,
                    &self.cancel,
                )?;
                recipe.decorate(&mut out);
                if out["arena"] != true && out["player"].is_null() {
                    let textures = recipe.batch(
                        &mut self.archive.as_mut().unwrap().1,
                        catalog,
                        &oodle,
                        0,
                        &self.cancel,
                    )?;
                    let existing = out["textures"]
                        .as_array_mut()
                        .context("Texture list missing")?;
                    for item in textures["textures"].as_array().unwrap() {
                        if !existing.iter().any(|t| t["name"] == item["name"]) {
                            existing.push(item.clone());
                        }
                    }
                    if let Some(skipped) = out["skipped"].as_array_mut() {
                        skipped.extend(textures["skipped"].as_array().unwrap().iter().cloned());
                    }
                }
                self.apparel = None;
                match crate::apparel::Session::create(
                    &mut out,
                    catalog,
                    &oodle,
                    &name,
                    &self.cancel,
                ) {
                    Ok(Some(session)) => self.apparel = Some((id, session)),
                    Ok(None) => {}
                    Err(e) => {
                        if codec::cancelled(&self.cancel).is_err() {
                            return Err(e);
                        }
                        out["skipped"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!({"name":name,"reason":format!("Apparel preview: {e:#}")}));
                        if name.contains("shoes_") {
                            out["shoe"] = json!({"selected":"neutral","variants":[]});
                        }
                    }
                }
                recipe.decorate(&mut out);
                self.recipe = Some((id, recipe));
                self.event("Model ready", 100);
                Ok(out)
            }
            "preview:load-loose" => {
                self.cancel.store(false, Ordering::Relaxed);
                let source = Path::new(a.as_str().context("Selected mod path missing")?);
                let allowed_root = Path::new(
                    args.get(1)
                        .and_then(Value::as_str)
                        .context("Mod library root missing")?,
                );
                let virtual_name = args
                    .get(2)
                    .and_then(Value::as_str)
                    .context("Mod virtual path missing")?
                    .replace('\\', "/");
                crate::catalog::safe_relative(&virtual_name)?;
                ensure!(
                    virtual_name.to_ascii_lowercase().ends_with(".iff")
                        || virtual_name.to_ascii_lowercase().ends_with(".iff.bin"),
                    "Mod virtual path must name an IFF resource"
                );
                let selected_config = args
                    .get(3)
                    .map(|v| v.as_str().context("Player configuration must be a string"))
                    .transpose()?;
                self.load_loose_archive(source, allowed_root)?;
                self.archive.as_ref().unwrap().1.require_unique_members()?;
                self.event("Decoding native geometry", 70);
                let catalog = self.catalog.as_ref().context("Catalog unloaded")?;
                // The loose file remains the geometry/source of truth. Use a
                // canonical catalog context when one exists; player configs and
                // item IFFs also resolve beside the selected loose file below.
                let leaf = virtual_name.rsplit('/').next().unwrap_or(&virtual_name);
                let lower_leaf = leaf.to_ascii_lowercase();
                let player_stem = lower_leaf.trim_end_matches(".iff");
                let player_context = player_stem
                    .strip_prefix("png")
                    .filter(|number| {
                        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
                    })
                    .map(|_| format!("char/sig/{leaf}"));
                let dependency_name = player_context
                    .as_deref()
                    .and_then(|candidate| catalog.resolve(candidate).ok())
                    .or_else(|| catalog.resolve(&virtual_name).ok())
                    .map(|resource| resource.name.clone())
                    .unwrap_or_else(|| virtual_name.clone());
                let oodle = Oodle::load(&catalog.root)?;
                let mut out = crate::preview::build(
                    &mut self.archive.as_mut().unwrap().1,
                    catalog,
                    &oodle,
                    &dependency_name,
                    &self.cancel,
                )?;
                crate::tangent::apply(&mut out, catalog, &oodle, &dependency_name, &self.cancel)?;
                self.event("Loading player appearance and textures", 85);
                crate::player::apply(
                    &mut out,
                    &mut self.archive.as_mut().unwrap().1,
                    catalog,
                    &oodle,
                    &dependency_name,
                    selected_config,
                    source.parent(),
                    &self.cancel,
                )?;
                let recipe = crate::materials::Recipe::read(
                    &mut self.archive.as_mut().unwrap().1,
                    &self.cancel,
                )?;
                recipe.decorate(&mut out);
                if out["arena"] != true && out["player"].is_null() {
                    let textures = recipe.batch(
                        &mut self.archive.as_mut().unwrap().1,
                        catalog,
                        &oodle,
                        0,
                        &self.cancel,
                    )?;
                    let existing = out["textures"]
                        .as_array_mut()
                        .context("Texture list missing")?;
                    for item in textures["textures"].as_array().unwrap() {
                        if !existing
                            .iter()
                            .any(|texture| texture["name"] == item["name"])
                        {
                            existing.push(item.clone());
                        }
                    }
                    if let Some(skipped) = out["skipped"].as_array_mut() {
                        skipped.extend(textures["skipped"].as_array().unwrap().iter().cloned());
                    }
                }
                self.apparel = None;
                match crate::apparel::Session::create(
                    &mut out,
                    catalog,
                    &oodle,
                    &dependency_name,
                    &self.cancel,
                ) {
                    Ok(Some(session)) => self.apparel = Some((usize::MAX, session)),
                    Ok(None) => {}
                    Err(error) => {
                        if codec::cancelled(&self.cancel).is_err() {
                            return Err(error);
                        }
                        out["skipped"].as_array_mut().unwrap().push(json!({
                            "name": virtual_name,
                            "reason": format!("Apparel preview: {error:#}")
                        }));
                        if virtual_name.contains("shoes_") {
                            out["shoe"] = json!({"selected":"neutral","variants":[]});
                        }
                    }
                }
                recipe.decorate(&mut out);
                self.recipe = Some((usize::MAX, recipe));
                out["source"] = json!({
                    "kind": "loose-mod",
                    "path": virtual_name,
                    "dependencyContext": dependency_name,
                    "bytes": source.metadata()?.len()
                });
                self.event("Model ready", 100);
                Ok(out)
            }
            "preview:material-textures" => {
                let id = a.as_u64().context("Missing resource id")? as usize;
                ensure!(
                    self.archive
                        .as_ref()
                        .is_some_and(|(cached, _)| *cached == id),
                    "Model archive session changed; reopen preview"
                );
                let recipe = &self
                    .recipe
                    .as_ref()
                    .filter(|(cached, _)| *cached == id)
                    .context("Reopen model preview before loading materials")?
                    .1;
                let cursor = args.get(1).and_then(Value::as_u64).unwrap_or(0) as usize;
                self.cancel.store(false, Ordering::Relaxed);
                self.event("Loading linked material textures; retaining model", 10);
                let catalog = self.catalog.as_ref().context("Catalog unloaded")?;
                let oodle = Oodle::load(&catalog.root)?;
                let result = recipe.batch(
                    &mut self.archive.as_mut().unwrap().1,
                    catalog,
                    &oodle,
                    cursor,
                    &self.cancel,
                )?;
                self.event("Material textures ready", 100);
                Ok(result)
            }
            "preview:change-design" => {
                let id = a.as_u64().context("Missing resource id")? as usize;
                let choice = args
                    .get(1)
                    .and_then(Value::as_str)
                    .context("Missing design choice")?;
                let session = &self
                    .apparel
                    .as_ref()
                    .filter(|(cached, _)| *cached == id)
                    .context("Model session changed; reopen preview")?
                    .1;
                self.cancel.store(false, Ordering::Relaxed);
                self.event("Replacing color textures; retaining geometry", 20);
                let result = session.change(
                    self.catalog()?,
                    &Oodle::load(&self.catalog()?.root)?,
                    choice,
                    &self.cancel,
                )?;
                self.event("Design ready", 100);
                Ok(result)
            }
            "preview:save-texture" => {
                let id = a.as_u64().context("Missing resource id")? as usize;
                self.catalog()?.entry(id)?;
                let name = args
                    .get(1)
                    .and_then(Value::as_str)
                    .context("Missing texture name")?;
                if let Some(source) = args.get(2).filter(|value| !value.is_null()) {
                    ensure!(
                        source["resourceId"].as_u64() == Some(id as u64),
                        "Texture source changed; reopen the gallery"
                    );
                    let index = source["index"]
                        .as_u64()
                        .context("Texture member ID missing")?
                        as usize;
                    let (cached_id, archive) =
                        self.archive.as_mut().context("Inspect an IFF first")?;
                    ensure!(
                        *cached_id == id,
                        "Texture source changed; reopen the gallery"
                    );
                    let member = archive.member(index)?;
                    ensure!(
                        member.name == name,
                        "Texture member changed; reopen the gallery"
                    );
                    self.cancel.store(false, Ordering::Relaxed);
                    let header = archive.prefix(index, 148, &self.cancel)?;
                    crate::validation::dds(&header, member.size)?;
                    // Use the existing occurrence-addressed, CRC-checked atomic Save.
                    // Reconstructed TLD textures never carry this direct DDS identity.
                    return Ok(
                        json!({"nativeSave":true,"nativeMember":true,"name":name,"resourceId":id,"index":index,"decoded":false,"gameRoot":self.catalog()?.root}),
                    );
                }
                let filename = name
                    .strip_suffix(".tld")
                    .map(|s| format!("{s}.dds"))
                    .unwrap_or(name.into());
                Ok(
                    json!({"nativeSave":true,"nativeTexture":true,"name":filename,"resourceId":id,"textureName":name}),
                )
            }
            "preview:publish-texture" => {
                self.cancel.store(false, Ordering::Relaxed);
                let id = a["resourceId"]
                    .as_u64()
                    .context("Texture resource missing")? as usize;
                let name = a["textureName"].as_str().context("Texture name missing")?;
                let target = self.save_target(&a)?;
                let mut temporary = if self
                    .archive
                    .as_ref()
                    .is_some_and(|(cached, _)| *cached == id)
                {
                    None
                } else {
                    Some(self.source_archive(id)?)
                };
                let archive = temporary
                    .as_mut()
                    .unwrap_or_else(|| &mut self.archive.as_mut().unwrap().1);
                let dds = crate::textures::one(
                    archive,
                    self.catalog.as_ref().context("Catalog unloaded")?,
                    name,
                    &self.cancel,
                )?;
                crate::validation::dds(&dds[..dds.len().min(148)], dds.len() as u64)?;
                codec::cancelled(&self.cancel)?;
                write_new_atomic(&target, &dds)?;
                Ok(json!({"target":target,"bytes":dds.len()}))
            }
            "preview:textures" => {
                self.cancel.store(false, Ordering::Relaxed);
                let id = a.as_u64().context("Missing resource id")? as usize;
                let mut temporary = if self
                    .archive
                    .as_ref()
                    .is_some_and(|(cached, _)| *cached == id)
                {
                    None
                } else {
                    Some(self.source_archive(id)?)
                };
                let archive = temporary
                    .as_mut()
                    .unwrap_or_else(|| &mut self.archive.as_mut().unwrap().1);
                let result = crate::textures::gallery(
                    archive,
                    self.catalog.as_ref().context("Catalog unloaded")?,
                    &self.cancel,
                    |name, info, archive, item| {
                        item["range"] = crate::player::range(&crate::player::preview_metadata(
                            archive, name, info,
                        ));
                    },
                )?;
                self.event("Texture gallery ready", 100);
                Ok(result)
            }
            "export:set-root" => {
                let root = PathBuf::from(a.as_str().context("Missing export folder")?);
                ensure!(root.is_absolute(), "Export folder must be absolute");
                self.output = Some(root);
                Ok(a)
            }
            "export:start" => self.export(&a),
            "export:textures" => {
                self.export_textures(a.as_u64().context("Missing resource id")? as usize)
            }
            "file:close" => {
                self.archive = None;
                self.recipe = None;
                self.apparel = None;
                Ok(json!(true))
            }
            _ => bail!("Unsupported exporter command: '{channel}'"),
        }
    }
    pub fn member_bytes(&mut self, index: usize, decoded: bool) -> Result<(String, Vec<u8>)> {
        let archive = &mut self.archive.as_mut().context("Inspect an IFF first")?.1;
        let name = archive.member(index)?.name;
        let bytes = archive.read_with_cancel(index, codec::MAX, &self.cancel)?;
        if decoded
            && matches!(
                Path::new(&name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str(),
                "scne" | "txtr" | "json"
            )
        {
            let bytes = if codec::vcz(&bytes) {
                let oodle = Oodle::load(&self.catalog()?.root)?;
                crate::buffer_decode::decode(&bytes, None, &oodle, &self.cancel)?
            } else {
                bytes
            };
            let output_name = if name.to_ascii_lowercase().ends_with(".json") {
                name
            } else {
                format!("{name}.json")
            };
            return Ok((output_name, crate::inspection::decoded_json(&bytes)?));
        }
        Ok((name, bytes))
    }
    fn save_target(&self, request: &Value) -> Result<PathBuf> {
        let target = PathBuf::from(
            request["target"]
                .as_str()
                .context("Missing Save destination")?,
        );
        ensure!(target.is_absolute(), "Save destination must be absolute");
        let parent = target
            .parent()
            .context("Missing Save parent")?
            .canonicalize()?;
        ensure!(
            !within_game(&parent, &self.catalog()?.root),
            "Save outside the game folder"
        );
        Ok(parent.join(target.file_name().context("Missing Save filename")?))
    }
    fn publish_member(&mut self, request: &Value) -> Result<Value> {
        let resource = request["resourceId"]
            .as_u64()
            .context("Missing inspection resource")? as usize;
        ensure!(
            self.archive.as_ref().is_some_and(|(id, _)| *id == resource),
            "Inspection session changed; reopen the member"
        );
        let index = request["index"].as_u64().context("Missing member index")? as usize;
        let target = self.save_target(request)?;
        self.cancel.store(false, Ordering::Relaxed);
        if request["decoded"] == true {
            let (_, bytes) = self.member_bytes(index, true)?;
            codec::cancelled(&self.cancel)?;
            write_new_atomic(&target, &bytes)?;
            return Ok(json!({"target":target,"bytes":bytes.len()}));
        }
        let archive = &mut self.archive.as_mut().unwrap().1;
        let bytes = archive.member(index)?.size;
        write_stream_atomic(&target, |output| {
            archive.copy_member(index, codec::MAX, &self.cancel, output)?;
            Ok(())
        })?;
        Ok(json!({"target":target,"bytes":bytes}))
    }
    fn checked_output(&self) -> Result<PathBuf> {
        let output = self
            .output
            .as_ref()
            .context("Choose an export folder first")?;
        ensure!(
            output.is_dir(),
            "Choose an existing export folder owned by you"
        );
        let output = output.canonicalize()?;
        ensure!(
            !within_game(&output, &self.catalog()?.root),
            "Export outside the game folder to protect installed archives"
        );
        Ok(output)
    }
    fn export_textures(&mut self, id: usize) -> Result<Value> {
        let output = self.checked_output()?;
        self.cancel.store(false, Ordering::Relaxed);
        self.event("Reading source textures", 0);
        let catalog = self.catalog()?;
        let resource = catalog.entry(id)?;
        let oodle = Oodle::load(&catalog.root)?;
        let packed = codec::range(catalog, resource)?;
        let source = if codec::vcz(&packed) {
            crate::archive_decode::strict(&oodle, &catalog.root, &packed, &self.cancel)?
        } else {
            packed
        };
        let source = if source.starts_with(b"PK\x03\x04") {
            source
        } else {
            crate::exporter::inspection_container(&resource.name, source, &self.cancel)?
        };
        let mut archive = Archive::open_with_cancel(source, &self.cancel)?;
        archive.require_unique_members()?;
        let descriptors = crate::textures::descriptors(&mut archive, &self.cancel)?;
        ensure!(
            !descriptors.is_empty(),
            "No DDS/TLD source textures were described by this resource"
        );
        ensure!(
            descriptors.len() <= 10000,
            "Texture export exceeds 10,000-resource safety limit"
        );
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let job = output.join(format!("Textures-{stamp}-{}", std::process::id()));
        fs::create_dir(&job)?;
        let mut results = Vec::new();
        let mut failures = Vec::new();
        let mut names = std::collections::HashSet::new();
        for (i, (name, info)) in descriptors.iter().enumerate() {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            self.event(
                &format!("Exporting DDS {} of {}", i + 1, descriptors.len()),
                (i * 100 / descriptors.len()) as u64,
            );
            let result = (|| -> Result<Value> {
                let dds = crate::textures::reconstruct(
                    &mut archive,
                    catalog,
                    &oodle,
                    name,
                    info,
                    &self.cancel,
                    codec::MAX,
                )?;
                let filename = if name.to_lowercase().ends_with(".tld") {
                    format!("{}.dds", &name[..name.len() - 4])
                } else {
                    name.clone()
                };
                ensure!(
                    names.insert(filename.to_lowercase()),
                    "Duplicate DDS output path"
                );
                let target = job.join(safe_relative(&filename)?);
                fs::create_dir_all(target.parent().unwrap())?;
                codec::cancelled(&self.cancel)?;
                write_new_atomic(&target, &dds)?;
                Ok(json!({"name":name,"target":target,"bytes":dds.len()}))
            })();
            match result {
                Ok(value) => results.push(value),
                Err(e) => failures.push(json!({"name":name,"error":format!("{e:#}")})),
            }
        }
        let cancelled = self.cancel.load(Ordering::Relaxed);
        if !results.is_empty() {
            self.last_output = Some(job.clone());
        }
        if !cancelled {
            self.event("DDS export finished", 100);
        }
        Ok(
            json!({"folder":job,"results":results,"failures":failures,"cancelled":cancelled,"notice":"Full-resolution source DDS textures. No preview ranges, palette colors, downsampling or report files are baked into exports."}),
        )
    }
    fn export(&mut self, args: &Value) -> Result<Value> {
        let mut ids = args
            .as_array()
            .or_else(|| args["ids"].as_array())
            .context("Select at least one resource")?
            .clone();
        let mode = args["mode"].as_str().unwrap_or("original");
        ensure!(
            matches!(mode, "original" | "mod-ready"),
            "Unknown export mode"
        );
        ensure!(
            !ids.is_empty() && ids.len() <= 5000,
            "Select 1–5,000 resources per export job"
        );
        let mut unique = std::collections::HashSet::new();
        ids.retain(|id| unique.insert(id.clone()));
        for id in &ids {
            self.catalog()?
                .entry(id.as_u64().context("Bad resource id")? as usize)?;
        }
        if mode == "mod-ready" {
            let catalog = self.catalog()?;
            let selected = ids.clone();
            for id in selected {
                let name = &catalog
                    .entry(id.as_u64().context("Bad resource id")? as usize)?
                    .name;
                if !name.contains("hair") && !name.contains("beard") {
                    continue;
                }
                for path in crate::player::hair_sources(name) {
                    if let Ok(companion) = catalog.resolve(&path) {
                        let id = json!(companion.id);
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
            ensure!(
                ids.len() <= 5000,
                "Hair companion expansion exceeds 5,000-resource batch limit; export a smaller selection"
            );
        }
        let output = self.checked_output()?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let job = output.join(format!("Export-{stamp}-{}", std::process::id()));
        fs::create_dir(&job)?;
        self.cancel.store(false, Ordering::Relaxed);
        (self.emit)(json!({"kind":"export-job","folder":job,"ids":ids,"mode":mode}));
        let mut results = Vec::new();
        let mut failures = Vec::new();
        let mut manifest = crate::exporter::DependencyResolver::new(self.catalog()?, &self.cancel)?;
        for (index, id) in ids.iter().enumerate() {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            self.event(
                &format!("Exporting {} of {}", index + 1, ids.len()),
                (index * 100 / ids.len()) as u64,
            );
            let result = (|| -> Result<Value> {
                let id = id.as_u64().context("Bad resource id")? as usize;
                let c = self.catalog()?;
                let e = c.entry(id)?.clone();
                if crate::raw_export::eligible_name(&e.name, mode) {
                    let mut source = crate::raw_export::Source::open(c, &e, &self.cancel)?;
                    if source.is_plain() {
                        let stats = if mode == "mod-ready" {
                            json!({"warnings":[]})
                        } else {
                            Value::Null
                        };
                        let validation = crate::validation::streamed_file(
                            &e.name,
                            &source.header,
                            e.size,
                            mode,
                            &stats,
                        )?;
                        if mode == "mod-ready" {
                            ensure!(
                                validation["status"] != "failed",
                                "Export validation failed: {}",
                                validation["errors"]
                            );
                        }
                        let target = job.join(safe_relative(&e.name)?);
                        fs::create_dir_all(target.parent().unwrap())?;
                        let mut gate = ExportProgressGate::default();
                        write_stream_atomic(&target, |file| {
                            source.copy_to(file, &self.cancel, |done, total| {
                                let fraction = if total == 0 {
                                    0.0
                                } else {
                                    done as f64 / total as f64
                                };
                                let percent = ((index as f64 + fraction * 0.94) * 100.0
                                    / ids.len() as f64)
                                    as u64;
                                if gate.allow(percent, Instant::now()) {
                                    self.event("Copying archive data", percent)
                                }
                            })
                        })?;
                        return Ok(
                            json!({"id":id,"name":e.name,"target":target,"bytes":e.size,"members":0,"mode":mode,"conversion":stats,"validation":validation}),
                        );
                    }
                }
                let packed = codec::range(c, &e)?;
                self.event(
                    "Native container decode",
                    ((index as f64 + 0.1) * 100. / ids.len() as f64) as u64,
                );
                let oodle = Oodle::load(&c.root)?;
                let bytes = if codec::vcz(&packed) && e.name.to_lowercase().ends_with(".iff") {
                    crate::archive_decode::strict(&oodle, &c.root, &packed, &self.cancel)?
                } else {
                    packed
                };
                self.event(
                    "Checking member sizes and CRCs",
                    ((index as f64 + if mode == "mod-ready" { 0.15 } else { 0.4 }) * 100.
                        / ids.len() as f64) as u64,
                );
                let mut stats = Value::Null;
                let (name, bytes) = if mode == "mod-ready" {
                    if bytes.starts_with(b"PK\x03\x04") {
                        let mut highest = 0.0f64;
                        let mut progress_gate = ExportProgressGate::default();
                        let (converted, report) = crate::exporter::convert(
                            bytes,
                            c,
                            &oodle,
                            &self.cancel,
                            &mut manifest,
                            |phase, p| {
                                highest = highest.max(p);
                                let percent = ((index as f64 + 0.15 + highest * 0.75) * 100.
                                    / ids.len() as f64)
                                    as u64;
                                if progress_gate.allow(percent, Instant::now()) {
                                    self.event(phase, percent);
                                }
                            },
                        )?;
                        stats = report;
                        (e.name.clone(), converted)
                    } else {
                        let (name, data, warnings) =
                            crate::exporter::single(&e.name, bytes, c, &oodle, &self.cancel)?;
                        stats = json!({"warnings":warnings});
                        (name, data)
                    }
                } else {
                    (e.name.clone(), bytes)
                };
                let (members, validation, bytes) = if bytes.starts_with(b"PK\x03\x04")
                    || bytes.starts_with(b"PK\x05\x06")
                {
                    let mut archive = Archive::open_with_cancel(bytes, &self.cancel)?;
                    let mut validation_progress_gate = ExportProgressGate::default();
                    let report = |p: f64| {
                        let percent = ((index as f64
                            + if mode == "mod-ready" {
                                0.9 + p * 0.04
                            } else {
                                0.4 + p * 0.5
                            })
                            * 100.
                            / ids.len() as f64) as u64;
                        if validation_progress_gate.allow(percent, Instant::now()) {
                            self.event("Checking member sizes and CRCs", percent);
                        }
                    };
                    let count = if mode == "mod-ready" {
                        crate::exporter::validate(&mut archive, &self.cancel, report)?
                    } else {
                        archive.validate_with(&self.cancel, report)?
                    };
                    (
                        count,
                        crate::validation::archive(&mut archive, mode, &stats, &self.cancel)?,
                        archive.into_bytes(),
                    )
                } else {
                    (
                        0,
                        crate::validation::file(&name, &bytes, mode, &stats),
                        bytes,
                    )
                };
                if mode == "mod-ready" {
                    ensure!(
                        validation["status"] != "failed",
                        "Export validation failed: {}",
                        validation["errors"]
                    );
                }
                codec::cancelled(&self.cancel)?;
                self.event(
                    "Publishing checked export",
                    ((index as f64 + 0.95) * 100. / ids.len() as f64) as u64,
                );
                let target = job.join(safe_relative(&name)?);
                fs::create_dir_all(target.parent().unwrap())?;
                write_new_atomic(&target, &bytes)?;
                Ok(
                    json!({"id":id,"name":e.name,"target":target,"bytes":bytes.len(),"members":members,"mode":mode,"conversion":stats,"validation":validation}),
                )
            })();
            match result {
                Ok(r) => {
                    (self.emit)(json!({"kind":"export-item","value":r}));
                    results.push(r);
                }
                Err(error) => {
                    let failure = json!({"id":id,"name":self.catalog()?.entry(id.as_u64().unwrap() as usize)?.name,"error":format!("{error:#}")});
                    (self.emit)(json!({"kind":"export-failure","value":failure}));
                    failures.push(failure);
                }
            }
        }
        let cancelled = self.cancel.load(Ordering::Relaxed);
        let pending: Vec<_> = ids
            .iter()
            .filter(|id| {
                !results.iter().any(|r| &r["id"] == *id)
                    && !failures.iter().any(|r| &r["id"] == *id)
            })
            .cloned()
            .collect();
        if !results.is_empty() {
            self.last_output = Some(job.clone());
        }
        if !cancelled {
            self.event("Export finished", 100);
        }
        Ok(
            json!({"folder":job,"results":results,"failures":failures,"pending":pending,"mode":mode,"cancelled":cancelled,"notice":if mode=="mod-ready"{"Native mod-ready conversion: unsupported assets preserved; review in-app warnings before game use. No preview-generated colors or geometry are exported."}else{"Original decoded container: every member payload unchanged. No preview-generated data is exported."}}),
        )
    }
}
pub fn texture(name: &str, dds: &[u8]) -> Value {
    let fourcc = dds
        .get(84..88)
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("DDS");
    let format = if fourcc == "DX10" {
        format!("DXGI {}", codec::u32_at(dds, 128).unwrap_or(0))
    } else {
        fourcc.into()
    };
    json!({"name":name,"data":base64::engine::general_purpose::STANDARD.encode(dds),"width":codec::u32_at(dds,16).unwrap_or(0),"height":codec::u32_at(dds,12).unwrap_or(0),"mips":codec::u32_at(dds,28).unwrap_or(1).max(1),"format":format})
}
pub fn write_new_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    write_stream_atomic(target, |file| {
        file.write_all(bytes)?;
        Ok(())
    })
}
fn write_stream_atomic(
    target: &Path,
    write: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
    ensure!(
        !target.exists(),
        "Target already exists; the exporter does not overwrite files"
    );
    let temp = target.with_file_name(format!(
        ".rust-export-{}-{}.tmp",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    write_stream_atomic_at(target, &temp, write)
}
fn write_stream_atomic_at(
    target: &Path,
    temp: &Path,
    write: impl FnOnce(&mut fs::File) -> Result<()>,
) -> Result<()> {
    // Creation must succeed before cleanup owns this path. A name collision or
    // permission error must never remove an existing, unowned temporary file.
    let mut file = OpenOptions::new().write(true).create_new(true).open(temp)?;
    let result = (|| -> Result<()> {
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        // std::fs::rename replaces existing files, including on Windows.
        // MoveFileW (without REPLACE_EXISTING) makes the no-overwrite check atomic.
        use std::os::windows::ffi::OsStrExt;
        use windows::{Win32::Storage::FileSystem::MoveFileW, core::PCWSTR};
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe { MoveFileW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr())) }
            .context("Could not publish export without overwriting an existing file")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
// Inputs are canonical paths. Component comparisons handle drive roots and
// avoid accepting a game descendant or rejecting a similarly named sibling.
fn within_game(path: &Path, game: &Path) -> bool {
    PathBuf::from(path.to_string_lossy().to_lowercase())
        .starts_with(PathBuf::from(game.to_string_lossy().to_lowercase()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_boundary_handles_roots_case_siblings_and_unc() {
        for (path, game, inside) in [
            (r"\\?\D:\Exports", r"\\?\d:\", true),
            (r"\\?\C:\Games\NBA\exports", r"\\?\c:\games\nba", true),
            (r"\\?\C:\Games\NBA", r"\\?\C:\Games\NBA", true),
            (r"\\?\C:\Games\NBA-other", r"\\?\C:\Games\NBA", false),
            (
                r"\\?\UNC\server\share\game\out",
                r"\\?\UNC\SERVER\share\game",
                true,
            ),
            (
                r"\\?\UNC\server\share\outside",
                r"\\?\UNC\server\share\game",
                false,
            ),
        ] {
            assert_eq!(
                within_game(Path::new(path), Path::new(game)),
                inside,
                "{path} / {game}"
            );
        }
    }
    struct RawFixture {
        root: PathBuf,
        game: PathBuf,
        output: PathBuf,
    }
    impl RawFixture {
        fn new() -> Self {
            let artifacts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("artifacts");
            fs::create_dir_all(&artifacts).unwrap();
            let root = artifacts.join(format!(
                "raw-unit-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            let game = root.join("game");
            let output = root.join("output");
            fs::create_dir(&game).unwrap();
            fs::create_dir(&output).unwrap();
            Self {
                root: root.canonicalize().unwrap(),
                game: game.canonicalize().unwrap(),
                output: output.canonicalize().unwrap(),
            }
        }
        fn backend(
            &self,
            name: &str,
            offset: u64,
            size: u64,
            emit: Emit,
            cancel: Arc<AtomicBool>,
        ) -> Backend {
            let mut b = Backend::new(emit, cancel);
            b.catalog = Some(
                Catalog::from_entries(
                    self.game.clone(),
                    vec![crate::catalog::Resource {
                        id: 0,
                        name: name.into(),
                        archive: "0A".into(),
                        offset,
                        size,
                    }],
                    0,
                    &AtomicBool::new(false),
                )
                .unwrap(),
            );
            b.output = Some(self.output.clone());
            b
        }
    }
    impl Drop for RawFixture {
        fn drop(&mut self) {
            // Only the exclusively created, project-local test tree is owned.
            let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("artifacts")
                .canonicalize()
                .unwrap();
            assert!(self.root.starts_with(&parent));
            assert!(
                self.root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("raw-unit-")
            );
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn stream_hash(path: &Path, offset: u64, size: u64) -> String {
        use sha2::{Digest, Sha256};
        use std::io::{Read, Seek, SeekFrom};
        let mut file = fs::File::open(path).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        let mut reader = file.take(size);
        let mut hash = Sha256::new();
        let mut buffer = vec![0; 1024 * 1024];
        loop {
            let n = reader.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        format!("{:X}", hash.finalize())
    }
    #[test]
    fn large_raw_range_exports_both_modes_without_decoder_or_full_buffer() {
        use std::io::{Seek, SeekFrom};
        let f = RawFixture::new();
        let source = f.game.join("0A");
        let offset = 73u64;
        let size = 513 * 1024 * 1024 + 37u64;
        let mut file = fs::File::create(&source).unwrap();
        file.set_len(offset + size + 8).unwrap();
        file.write_all(b"prefix outside selected range").unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.write_all(b"selected resource begins here").unwrap();
        file.seek(SeekFrom::Start(offset + size - 8)).unwrap();
        file.write_all(b"END-MARKNEIGHBOR").unwrap();
        drop(file);
        let original = stream_hash(&source, 0, offset + size + 8);
        let expected = stream_hash(&source, offset, size);
        for mode in ["original", "mod-ready"] {
            let events = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured = events.clone();
            let mut b = f.backend(
                "resource.bin",
                offset,
                size,
                Arc::new(move |v| captured.lock().unwrap().push(v)),
                Arc::new(AtomicBool::new(false)),
            );
            let result = b.export(&json!({"ids":[0],"mode":mode})).unwrap();
            assert_eq!(result["failures"].as_array().unwrap().len(), 0, "{result}");
            let item = &result["results"][0];
            assert_eq!(item["bytes"], size);
            assert_eq!(item["validation"]["status"], "ready");
            let target = Path::new(item["target"].as_str().unwrap());
            assert_eq!(fs::metadata(target).unwrap().len(), size);
            assert_eq!(stream_hash(target, 0, size), expected);
            assert_eq!(
                b.last_output.as_ref().unwrap(),
                Path::new(result["folder"].as_str().unwrap())
            );
            let events = events.lock().unwrap();
            let progress: Vec<_> = events
                .iter()
                .filter(|v| v["channel"] == "progress")
                .filter_map(|v| v["payload"]["percent"].as_u64())
                .collect();
            assert!(progress.windows(2).all(|p| p[0] <= p[1]));
            assert!(events.iter().any(|v| {
                v["payload"]["phase"] == "Copying archive data"
                    && v["payload"]["percent"]
                        .as_u64()
                        .is_some_and(|n| n > 0 && n < 94)
            }));
        }
        assert_eq!(stream_hash(&source, 0, offset + size + 8), original);
    }
    #[test]
    fn raw_bounds_cancel_and_zip_disguise_do_not_publish_partial_success() {
        let f = RawFixture::new();
        fs::write(f.game.join("0A"), vec![8; 3 * 1024 * 1024]).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let mut b = f.backend(
            "resource.bin",
            0,
            3 * 1024 * 1024,
            Arc::new(move |v| {
                if v["payload"]["phase"] == "Copying archive data"
                    && v["payload"]["percent"].as_u64().unwrap_or(0) > 0
                {
                    signal.store(true, Ordering::Relaxed)
                }
            }),
            cancel,
        );
        let result = b.export(&json!({"ids":[0],"mode":"original"})).unwrap();
        assert_eq!(result["cancelled"], true);
        assert!(result["results"].as_array().unwrap().is_empty());
        assert_eq!(
            fs::read_dir(result["folder"].as_str().unwrap())
                .unwrap()
                .count(),
            0
        );
        for (offset, size) in [
            (3 * 1024 * 1024 + 1, 1),
            (u64::MAX, 2),
            (0, 3 * 1024 * 1024 + 1),
        ] {
            let mut b = f.backend(
                "resource.bin",
                offset,
                size,
                Arc::new(|_| {}),
                Arc::new(AtomicBool::new(false)),
            );
            let result = b.export(&json!({"ids":[0],"mode":"mod-ready"})).unwrap();
            assert!(result["results"].as_array().unwrap().is_empty());
            assert_eq!(result["failures"].as_array().unwrap().len(), 1);
            assert_eq!(
                fs::read_dir(result["folder"].as_str().unwrap())
                    .unwrap()
                    .count(),
                0
            );
        }
        fs::write(f.game.join("0A"), b"PK\x03\x04corrupt container").unwrap();
        let mut b = f.backend(
            "disguised.bin",
            0,
            21,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
        );
        let result = b.export(&json!({"ids":[0],"mode":"mod-ready"})).unwrap();
        assert!(result["results"].as_array().unwrap().is_empty());
        assert_eq!(result["failures"].as_array().unwrap().len(), 1);
    }
    #[test]
    fn atomic_temp_collision_preserves_unowned_bytes() {
        let f = RawFixture::new();
        let target = f.output.join("result.bin");
        let temp = f.output.join("existing.tmp");
        fs::write(&temp, b"unowned bytes").unwrap();
        assert!(
            write_stream_atomic_at(&target, &temp, |file| {
                file.write_all(b"replacement")?;
                Ok(())
            })
            .is_err()
        );
        assert!(!target.exists());
        assert_eq!(fs::read(&temp).unwrap(), b"unowned bytes");
    }
    #[test]
    fn raw_source_truncation_and_write_faults_remove_only_owned_partial_output() {
        let f = RawFixture::new();
        let path = f.game.join("0A");
        fs::write(&path, vec![9; 2 * 1024 * 1024]).unwrap();
        let b = f.backend(
            "resource.bin",
            0,
            2 * 1024 * 1024,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
        );
        let mut source = crate::raw_export::Source::open(
            b.catalog().unwrap(),
            b.catalog().unwrap().entry(0).unwrap(),
            &b.cancel,
        )
        .unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(1024)
            .unwrap();
        let target = f.output.join("truncated.bin");
        assert!(
            write_stream_atomic(&target, |output| source.copy_to(
                output,
                &b.cancel,
                |_, _| {}
            ))
            .is_err()
        );
        assert_eq!(fs::read_dir(&f.output).unwrap().count(), 0);
        fs::write(&path, vec![9; 2 * 1024 * 1024]).unwrap();
        struct Fault<'a> {
            out: &'a mut fs::File,
            left: usize,
        }
        impl std::io::Write for Fault<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.left == 0 {
                    return Err(std::io::Error::from_raw_os_error(112));
                }
                let n = self.left.min(bytes.len());
                let n = self.out.write(&bytes[..n])?;
                self.left -= n;
                Ok(n)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                self.out.flush()
            }
        }
        let mut source = crate::raw_export::Source::open(
            b.catalog().unwrap(),
            b.catalog().unwrap().entry(0).unwrap(),
            &b.cancel,
        )
        .unwrap();
        assert!(
            write_stream_atomic(&target, |output| source.copy_to(
                &mut Fault {
                    out: output,
                    left: 5000
                },
                &b.cancel,
                |_, _| {}
            ))
            .is_err()
        );
        assert_eq!(fs::read_dir(&f.output).unwrap().count(), 0);
        assert_eq!(fs::metadata(path).unwrap().len(), 2 * 1024 * 1024);
    }
    #[test]
    fn export_progress_gate_bounds_repeated_member_events_without_losing_percentages() {
        let now = Instant::now();
        let mut gate = ExportProgressGate::default();
        assert!(gate.allow(15, now));
        for _ in 0..300_000 {
            assert!(!gate.allow(15, now));
        }
        for percent in 16..=100 {
            assert!(gate.allow(percent, now));
        }
    }
    #[test]
    fn export_progress_gate_retains_periodic_watchdog_keepalives() {
        use std::time::Duration;
        let now = Instant::now();
        let mut gate = ExportProgressGate::default();
        assert!(gate.allow(40, now));
        assert!(!gate.allow(40, now + Duration::from_millis(49)));
        assert!(gate.allow(40, now + Duration::from_millis(50)));
        assert!(!gate.allow(40, now + Duration::from_millis(99)));
        assert!(gate.allow(40, now + Duration::from_millis(100)));
        assert!(gate.allow(41, now + Duration::from_millis(101)));
        assert!(gate.allow(90, now + Duration::from_millis(102)));
    }
    #[test]
    fn decoded_member_save_preserves_full_width_numbers_and_source() {
        let source = crate::inspection::tests::wide_integer_tree();
        let cancel = Arc::new(AtomicBool::new(false));
        let container =
            crate::exporter::inspection_container("level.SCNE", source.clone(), &cancel).unwrap();
        let mut backend = Backend::new(Arc::new(|_| {}), cancel);
        backend.archive = Some((0, Archive::open(container).unwrap()));
        let (name, output) = backend.member_bytes(0, true).unwrap();
        assert_eq!(name, "level.SCNE.json");
        let parsed: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(parsed["root"][0].as_u64(), Some(u64::MAX));
        assert_eq!(parsed["root"][1].as_u64(), Some(9_007_199_254_740_993));
        assert_eq!(backend.member_bytes(0, false).unwrap().1, source);
    }
    #[test]
    fn dds_gallery_save_keeps_occurrence_identity_and_rejects_stale_or_wrong_members() {
        let fixture = RawFixture::new();
        let dds = |pixel: u32| {
            let mut bytes = vec![0; 132];
            bytes[..4].copy_from_slice(b"DDS ");
            for (offset, value) in [
                (4, 124u32),
                (12, 1),
                (16, 1),
                (28, 1),
                (76, 32),
                (80, 0x41),
                (88, 32),
                (128, pixel),
            ] {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            bytes
        };
        let first = dds(0xff0000ff);
        let second = dds(0xff00ff00);
        let source = crate::iff_directory::tests::fixture(&[
            ("same.dds", &first),
            ("same.dds", &second),
            ("not.dds", b"not a texture"),
        ]);
        let mut backend = fixture.backend(
            "duplicate.iff",
            0,
            source.len() as u64,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
        );
        backend.archive = Some((0, Archive::open(source.clone()).unwrap()));
        for (index, expected) in [&first, &second].iter().enumerate() {
            let view = backend
                .dispatch("file:preview-member", &[json!(index)])
                .unwrap();
            let identity = json!({"resourceId":0,"index":index});
            assert_eq!(view["sourceMember"], identity);
            let mut plan = backend
                .dispatch(
                    "preview:save-texture",
                    &[json!(0), json!("same.dds"), identity],
                )
                .unwrap();
            assert_eq!(plan["nativeMember"], true);
            assert!(plan.get("nativeTexture").is_none());
            assert_eq!(plan["index"], index);
            let target = fixture.output.join(format!("gallery{index}.dds"));
            plan["target"] = json!(target);
            backend
                .dispatch("file:publish-member", std::slice::from_ref(&plan))
                .unwrap();
            assert_eq!(fs::read(&target).unwrap(), **expected);
            assert!(backend.dispatch("file:publish-member", &[plan]).is_err());
            assert_eq!(fs::read(&target).unwrap(), **expected);
        }
        for (id, name, identity) in [
            (0, "wrong.dds", json!({"resourceId":0,"index":0})),
            (0, "same.dds", json!({"resourceId":1,"index":0})),
            (0, "same.dds", json!({"resourceId":0,"index":99})),
            (0, "not.dds", json!({"resourceId":0,"index":2})),
        ] {
            assert!(
                backend
                    .dispatch("preview:save-texture", &[json!(id), json!(name), identity])
                    .is_err()
            );
        }
        // Normal reconstructed gallery saves remain name-based, never raw TLD publication.
        let normal = backend
            .dispatch(
                "preview:save-texture",
                &[json!(0), json!("color.tld"), Value::Null],
            )
            .unwrap();
        assert_eq!(normal["nativeTexture"], true);
        assert_eq!(normal["name"], "color.dds");
        assert!(normal.get("nativeMember").is_none());
        backend.archive.as_mut().unwrap().0 = 1;
        assert!(
            backend
                .dispatch(
                    "preview:save-texture",
                    &[
                        json!(0),
                        json!("same.dds"),
                        json!({"resourceId":0,"index":0})
                    ]
                )
                .is_err()
        );
        assert_eq!(backend.archive.take().unwrap().1.into_bytes(), source);
    }
    #[test]
    fn duplicate_member_preview_and_native_publication_use_occurrence_not_name() {
        let fixture = RawFixture::new();
        let first = b"{\"marker\":1,\"wide\":18446744073709551615}";
        let second = b"{\"marker\":2}";
        let source =
            crate::iff_directory::tests::fixture(&[("same.json", first), ("same.json", second)]);
        let mut backend = fixture.backend(
            "duplicate.iff",
            0,
            source.len() as u64,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
        );
        backend.archive = Some((0, Archive::open(source.clone()).unwrap()));
        for (i, payload) in [first.as_slice(), second.as_slice()].iter().enumerate() {
            let view = backend
                .dispatch("file:preview-member", &[json!(i)])
                .unwrap();
            let tree: Value = serde_json::from_str(view["text"].as_str().unwrap()).unwrap();
            assert_eq!(tree["marker"], i + 1);
            let plan = backend
                .dispatch("file:save-member", &[json!(i), json!(false)])
                .unwrap();
            assert_eq!(plan["index"], i);
            assert_eq!(plan["resourceId"], 0);
            let target = fixture.output.join(format!("occurrence{i}.json"));
            let request = json!({"resourceId":0,"index":i,"target":target,"decoded":false});
            backend
                .dispatch("file:publish-member", std::slice::from_ref(&request))
                .unwrap();
            assert_eq!(fs::read(&target).unwrap(), *payload);
            assert!(backend.dispatch("file:publish-member", &[request]).is_err());
            assert_eq!(fs::read(&target).unwrap(), *payload);
        }
        let decoded = fixture.output.join("decoded.json");
        backend
            .dispatch(
                "file:publish-member",
                &[json!({"resourceId":0,"index":0,"target":decoded,"decoded":true})],
            )
            .unwrap();
        let tree: Value = serde_json::from_slice(&fs::read(decoded).unwrap()).unwrap();
        assert_eq!(tree["wide"].as_u64(), Some(u64::MAX));
        let stale = fixture.output.join("stale.json");
        assert!(
            backend
                .dispatch(
                    "file:publish-member",
                    &[json!({"resourceId":1,"index":0,"target":stale})]
                )
                .is_err()
        );
        assert!(!stale.exists());
        assert_eq!(backend.archive.take().unwrap().1.into_bytes(), source);
    }
    #[test]
    fn corrupt_occurrence_save_does_not_publish_or_block_other_members() {
        let fixture = RawFixture::new();
        let mut source =
            crate::iff_directory::tests::fixture(&[("same.txt", b"bad"), ("same.txt", b"good")]);
        source[30 + 8] ^= 1;
        let mut backend = fixture.backend(
            "duplicate.iff",
            0,
            source.len() as u64,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
        );
        backend.archive = Some((0, Archive::open(source.clone()).unwrap()));
        let bad = fixture.output.join("bad.txt");
        assert!(
            backend
                .dispatch(
                    "file:publish-member",
                    &[json!({"resourceId":0,"index":0,"target":bad})]
                )
                .is_err()
        );
        assert!(!bad.exists());
        assert_eq!(fs::read_dir(&fixture.output).unwrap().count(), 0);
        let good = fixture.output.join("good.txt");
        backend
            .dispatch(
                "file:publish-member",
                &[json!({"resourceId":0,"index":1,"target":good})],
            )
            .unwrap();
        assert_eq!(fs::read(good).unwrap(), b"good");
        assert_eq!(backend.archive.take().unwrap().1.into_bytes(), source);
    }
    #[test]
    fn partial_disk_full_permission_and_cancel_failures_leave_no_output_or_temp() {
        let dir = std::env::temp_dir().join(format!(
            "rust-exporter-io-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let neighbor = dir.join("user.bin");
        fs::write(&neighbor, b"user data").unwrap();
        for code in [112, 5, 1223] {
            // Windows disk full, access denied, cancelled.
            let target = dir.join(format!("fault-{code}.bin"));
            assert!(
                write_stream_atomic(&target, |file| {
                    file.write_all(b"partial bytes")?;
                    Err(std::io::Error::from_raw_os_error(code).into())
                })
                .is_err()
            );
            assert!(!target.exists());
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
            assert_eq!(fs::read(&neighbor).unwrap(), b"user data");
        }
        let target = dir.join("concurrent.bin");
        assert!(
            write_stream_atomic(&target, |file| {
                file.write_all(b"export")?;
                fs::write(&target, b"concurrent user output")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"concurrent user output");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_file(target).unwrap();
        fs::remove_file(neighbor).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn existing_outputs_never_overwritten() {
        let dir = std::env::temp_dir().join(format!(
            "rust-exporter-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("fixture.bin");
        write_new_atomic(&path, b"original").unwrap();
        assert!(write_new_atomic(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn invalid_export_root_is_rejected() {
        let mut backend = Backend::new(Arc::new(|_| {}), Arc::new(AtomicBool::new(false)));
        assert!(
            backend
                .dispatch("export:set-root", &[json!("relative/output")])
                .is_err()
        );
    }
    #[test]
    fn unsupported_methods_are_explicit() {
        let mut backend = Backend::new(Arc::new(|_| {}), Arc::new(AtomicBool::new(false)));
        assert!(backend.dispatch("pretend-full-export", &[]).is_err());
    }
    #[test]
    fn public_branding_is_plain_product_and_version() {
        let mut backend = Backend::new(Arc::new(|_| {}), Arc::new(AtomicBool::new(false)));
        let value = backend.dispatch("app:bootstrap", &[]).unwrap();
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        let root = value["defaultOutputRoot"].as_str().unwrap();
        assert!(!root.contains("Rust"));
        assert!(!value["coverage"].as_str().unwrap().contains("Migration"));
    }
}
