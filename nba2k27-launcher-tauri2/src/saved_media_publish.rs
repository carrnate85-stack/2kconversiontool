//! One-time, fixed-plan saved-media publication. No game access or App swap.
//! New datasets only: never replace, merge, recursively remove or delete any
//! existing Main library. Partial copies/publications are held by a receipt;
//! explicit rollback moves only exact newly-owned units back into Recovery.
#![allow(dead_code)]
#[path = "saved_media_publish_io.rs"]
mod io;
use anyhow::{Context, Result, ensure};
use io::{Id, Owned, Pins, Source, Tree};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
const RECEIPT: &str = "Recovery/rust-saved-media-publication.jsonl";
const TARGETS: [(&str, bool); 5] = [
    ("Library/Jukebox", true),
    ("Data/Jukebox", true),
    ("Data/Rust/LegacyImport", true),
    ("Data/Rust/startup-videos.json", false),
    ("Data/Rust/legacy-media-import-complete.json", false),
];
#[derive(Clone)]
struct Review {
    stage: PathBuf,
    target: PathBuf,
    inventory: String,
    originals: BTreeMap<String, String>,
}
fn reviewed() -> Review {
    Review {
        stage: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts/legacy-media-staging-20261016-native"),
        target: PathBuf::from(
            r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\2k Launcher",
        ),
        inventory: "76b3d222e19e7c6f577452a3aee3f3e8964ab52fb5199b0b69e1778832ca8ef0".into(),
        originals: BTreeMap::from([
            (
                "Data/settings.json".into(),
                "c0a8f9d35e372b181be6136a8fb794a2865138af00d0521f4560f8b275778ced".into(),
            ),
            (
                "Library/Music/archive-library/5410c2c8c76f6bd9/library.json".into(),
                "4e276cf0f3c1f75e11049a6cedd5eb80782f6ccc14a0863773c6f1a0494e30dc".into(),
            ),
            (
                "Library/Videos/mp4-library.json".into(),
                "c22ae74d28924a2b56ceb523e1faa3d073cad979163d277c20a243194affab43".into(),
            ),
        ]),
    }
}
fn nonce() -> Result<String> {
    use windows::Win32::Security::Cryptography::*;
    let mut bytes = [0; 24];
    unsafe {
        BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn equal_path(a: &Path, b: &Path) -> bool {
    let f = |p: &Path| {
        p.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    f(a) == f(b)
}
fn disjoint(a: &Path, b: &Path) -> bool {
    let f = |p: &Path| {
        format!(
            "{}\\",
            p.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('/', "\\")
                .trim_end_matches('\\')
                .to_ascii_lowercase()
        )
    };
    let aa = f(a);
    let bb = f(b);
    !aa.starts_with(&bb) && !bb.starts_with(&aa)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    index: usize,
    target: String,
    directory: bool,
    sources: Vec<String>,
}
fn specs(tree: &Tree) -> Result<Vec<Spec>> {
    let mut covered = BTreeSet::new();
    let mut out = vec![];
    for (index, (target, directory)) in TARGETS.iter().enumerate() {
        let prefix = format!("{target}/");
        let sources = tree
            .files
            .keys()
            .filter(|p| {
                if *directory {
                    p.starts_with(&prefix)
                } else {
                    p.as_str() == *target
                }
            })
            .cloned()
            .collect::<Vec<_>>();
        ensure!(
            !sources.is_empty() && (*directory || sources.len() == 1),
            "Reviewed unit is incomplete"
        );
        covered.extend(sources.iter().cloned());
        out.push(Spec {
            index,
            target: (*target).into(),
            directory: *directory,
            sources,
        });
    }
    for path in tree.files.keys().filter(|p| !covered.contains(*p)) {
        ensure!(
            path.starts_with("Library/Videos/mp4/") && path.ends_with(".mp4"),
            "Unexpected reviewed-stage path"
        );
    }
    Ok(out)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Videos {
    schema: String,
    selected: Option<String>,
    items: Vec<Video>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Video {
    id: String,
    relative_path: String,
    sha256: String,
    bytes: u64,
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Report {
    pub phase: String,
    pub units: usize,
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub reused_videos: usize,
    pub stage_inventory_sha256: String,
    pub originals_unchanged: bool,
    pub settings_preserved: bool,
    pub game_access: bool,
    pub app_changed: bool,
}
struct Prepared {
    review: Review,
    tree: Tree,
    specs: Vec<Spec>,
    originals: BTreeMap<String, Source>,
    videos: BTreeMap<String, Source>,
    settings: Option<Source>,
    root_id: Id,
    parents: BTreeMap<String, Id>,
    _pins: Vec<Pins>,
}
impl Prepared {
    fn open(review: Review) -> Result<Self> {
        ensure!(
            review.inventory.len() == 64 && review.inventory.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid fixed stage review"
        );
        let stage = review.stage.canonicalize()?;
        let target = review.target.canonicalize()?;
        ensure!(disjoint(&stage, &target), "Stage and Main must be disjoint");
        let mut pins = vec![Pins::open(&target)?];
        let root_id = pins[0].identity()?;
        let mut parents = BTreeMap::new();
        for rel in ["Data", "Library", "Recovery"] {
            let pinned = Pins::open(&target.join(rel))?;
            parents.insert(rel.into(), pinned.identity()?);
            pins.push(pinned);
        }
        if !io::missing(&target.join("Data/Rust"))? {
            let pinned = Pins::open(&target.join("Data/Rust"))?;
            parents.insert("Data/Rust".into(), pinned.identity()?);
            pins.push(pinned);
        }
        ensure!(
            io::missing(&target.join("Data/Rust/settings-update.json"))?,
            "Finish settings recovery before publishing media"
        );
        let mut tree = Tree::open(&stage)?;
        ensure!(
            tree.digest() == review.inventory.to_ascii_lowercase(),
            "Frozen staged inventory differs; no promotion"
        );
        let unit_specs = specs(&tree)?;
        let complete: Value = serde_json::from_slice(
            &tree
                .files
                .get_mut("Data/Rust/legacy-media-import-complete.json")
                .unwrap()
                .bytes(1024 * 1024)?,
        )?;
        ensure!(
            complete["schema"] == "nba2k27-legacy-media-staged-v1"
                && complete["activePlaylistId"].is_null()
                && complete["startupVideoEnabled"] == false,
            "Staged completion/default selection differs"
        );
        let library: Value = serde_json::from_slice(
            &tree
                .files
                .get_mut("Data/Jukebox/library.json")
                .unwrap()
                .bytes(16 * 1024 * 1024)?,
        )?;
        ensure!(
            library["schema"] == "nba2k27-rust-jukebox-library-v1"
                && library["activePlaylistId"].is_null(),
            "Staged Jukebox must keep Default selected"
        );
        let mut originals = BTreeMap::new();
        for (rel, sha) in &review.originals {
            io::relative(rel)?;
            let source = Source::open(&target.join(rel))?;
            ensure!(
                source.owned.sha256.eq_ignore_ascii_case(sha),
                "Original settings/catalog changed since review; hold"
            );
            originals.insert(rel.clone(), source);
        }
        let settings = if io::missing(&target.join("Data/Rust/settings.json"))? {
            None
        } else {
            Some(Source::open(&target.join("Data/Rust/settings.json"))?)
        };
        let doc = tree
            .files
            .get_mut("Data/Rust/startup-videos.json")
            .unwrap()
            .bytes(1024 * 1024)?;
        let video_catalog: Videos = serde_json::from_slice(&doc)?;
        ensure!(
            video_catalog.schema == "nba2k27-rust-startup-video-library-v1"
                && video_catalog.items.len() <= 1000,
            "Invalid staged video catalog"
        );
        let mut videos = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for item in video_catalog.items {
            ensure!(
                uuid(&item.id)
                    && ids.insert(item.id.to_ascii_lowercase())
                    && item.relative_path == format!("Library/Videos/mp4/{}.mp4", item.id),
                "Ambiguous staged video path/ID"
            );
            let staged = tree
                .files
                .get(&item.relative_path)
                .context("Missing staged MP4")?;
            ensure!(
                staged.owned.bytes == item.bytes && staged.owned.sha256 == item.sha256,
                "Staged MP4 catalog hash differs"
            );
            let original = Source::open(&target.join(&item.relative_path))?;
            ensure!(
                original.owned.bytes == item.bytes && original.owned.sha256 == item.sha256,
                "Original MP4 no longer equals stage; never replace it"
            );
            videos.insert(item.relative_path, original);
        }
        ensure!(
            video_catalog
                .selected
                .as_ref()
                .is_none_or(|s| ids.contains(&s.to_ascii_lowercase())),
            "Staged selected video missing"
        );
        ensure!(
            tree.files
                .keys()
                .filter(|p| p.starts_with("Library/Videos/"))
                .count()
                == videos.len(),
            "Unreferenced staged MP4"
        );
        Ok(Self {
            review,
            tree,
            specs: unit_specs,
            originals,
            videos,
            settings,
            root_id,
            parents,
            _pins: pins,
        })
    }
    fn original_records(&self) -> BTreeMap<String, Owned> {
        self.originals
            .iter()
            .chain(self.videos.iter())
            .map(|(k, s)| (k.clone(), s.owned.clone()))
            .collect()
    }
    fn report(&self, phase: &str) -> Report {
        Report {
            phase: phase.into(),
            units: 5,
            copied_files: self.specs.iter().map(|s| s.sources.len()).sum(),
            copied_bytes: self
                .specs
                .iter()
                .flat_map(|s| &s.sources)
                .map(|p| self.tree.files[p].owned.bytes)
                .sum(),
            reused_videos: self.videos.len(),
            stage_inventory_sha256: self.review.inventory.clone(),
            originals_unchanged: true,
            settings_preserved: true,
            game_access: false,
            app_changed: false,
        }
    }
    fn unchanged(&mut self) -> Result<()> {
        for s in self.originals.values_mut().chain(self.videos.values_mut()) {
            ensure!(
                io::capture(&mut s.file)? == s.owned,
                "Original retained identity/hash changed"
            );
        }
        if let Some(s) = &mut self.settings {
            ensure!(
                io::capture(&mut s.file)? == s.owned,
                "Namespaced settings changed during publication"
            );
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    schema: String,
    nonce: String,
    root: Id,
    parents: BTreeMap<String, Id>,
    stage: Id,
    inventory: String,
    journal: Id,
    originals: BTreeMap<String, Owned>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Unit {
    index: usize,
    id: Id,
    files: BTreeMap<String, Owned>,
    directories: BTreeMap<String, Id>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", deny_unknown_fields)]
enum Event {
    Header(Header),
    WorkspaceIntent,
    WorkspaceOwned { id: Id },
    RustDirectoryIntent,
    RustDirectoryOwned { id: Id },
    CopyIntent { index: usize },
    Prepared { unit: Unit },
    PublishIntent { index: usize },
    Published { index: usize },
    Complete,
    RollbackIntent { index: usize },
    RolledBackUnit { index: usize },
    RolledBack,
}
struct Journal {
    file: File,
    bytes: Vec<u8>,
    identity: Id,
}
impl Journal {
    fn append(&mut self, event: &Event) -> Result<()> {
        ensure!(
            io::id(&self.file)? == self.identity,
            "Publication receipt identity changed"
        );
        ensure!(
            io::capture(&mut self.file)?.sha256 == io::hash(&self.bytes),
            "Publication receipt bytes changed"
        );
        let mut line = serde_json::to_vec(event)?;
        line.push(b'\n');
        ensure!(
            self.bytes.len() + line.len() <= 4 * 1024 * 1024,
            "Publication receipt exceeds bounds"
        );
        self.file.seek(SeekFrom::End(0))?;
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.bytes.extend(line);
        ensure!(
            io::capture(&mut self.file)?.sha256 == io::hash(&self.bytes),
            "Publication receipt readback differs"
        );
        Ok(())
    }
    fn load(path: &Path, write: bool) -> Result<(Self, Vec<Event>)> {
        let mut file = io::journal_open(path, write)?;
        ensure!(
            file.metadata()?.len() <= 4 * 1024 * 1024,
            "Publication receipt exceeds bounds"
        );
        let identity = io::id(&file)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            !bytes.is_empty() && bytes.last() == Some(&b'\n'),
            "Incomplete publication receipt; hold"
        );
        let events = bytes
            .split(|b| *b == b'\n')
            .filter(|b| !b.is_empty())
            .map(serde_json::from_slice)
            .collect::<std::result::Result<Vec<Event>, _>>()?;
        Ok((
            Self {
                file,
                bytes,
                identity,
            },
            events,
        ))
    }
}
fn workspace(root: &Path, nonce: &str) -> PathBuf {
    root.join("Recovery")
        .join(format!("rust-saved-media-{nonce}"))
}
fn temporary(work: &Path, index: usize) -> PathBuf {
    work.join(format!("unit-{index}"))
}
fn validate_unit(plan: &Prepared, unit: &Unit, path: &Path) -> Result<()> {
    let spec = plan
        .specs
        .get(unit.index)
        .context("Unknown publication unit")?;
    if spec.directory {
        let tree = Tree::open(path)?;
        ensure!(
            tree.identity == unit.id
                && tree.files.len() == unit.files.len()
                && tree.directories == unit.directories,
            "Published directory identity/contents changed"
        );
        for (rel, owned) in &unit.files {
            ensure!(
                tree.files.get(rel).is_some_and(|f| f.owned == *owned),
                "Published owned file identity/hash changed"
            );
        }
    } else {
        let source = Source::open(path)?;
        ensure!(
            source.owned.id == unit.id && unit.files.get("") == Some(&source.owned),
            "Published file changed"
        );
    }
    let expected = spec
        .sources
        .iter()
        .map(|key| {
            let rel = if spec.directory {
                key.strip_prefix(&(spec.target.clone() + "/"))
                    .unwrap()
                    .to_string()
            } else {
                String::new()
            };
            (rel, &plan.tree.files[key].owned)
        })
        .collect::<BTreeMap<_, _>>();
    ensure!(
        expected.len() == unit.files.len()
            && unit.files.iter().all(|(p, v)| expected
                .get(p)
                .is_some_and(|s| s.bytes == v.bytes && s.sha256 == v.sha256)),
        "Owned unit differs from frozen source inventory"
    );
    let mut directories = BTreeSet::new();
    if spec.directory {
        for name in expected.keys() {
            let parts = name.split('/').collect::<Vec<_>>();
            for length in 1..parts.len() {
                directories.insert(parts[..length].join("/"));
            }
        }
    }
    ensure!(
        unit.directories.keys().cloned().collect::<BTreeSet<_>>() == directories,
        "Unexpected/foreign empty directory in owned unit"
    );
    Ok(())
}
fn publish(
    review: Review,
    guard: &impl Fn() -> Result<()>,
    fault: &impl Fn(&str, usize) -> Result<()>,
) -> Result<Report> {
    guard()?;
    let mut plan = Prepared::open(review)?;
    ensure!(
        io::missing(&plan.review.target.join(RECEIPT))?,
        "A publication receipt already exists; inspect before retrying"
    );
    for spec in &plan.specs {
        ensure!(
            io::missing(&plan.review.target.join(&spec.target))?,
            "New dataset target already exists; never merge/replace"
        );
    }
    guard()?;
    let file = io::fresh(&plan.review.target.join(RECEIPT))?;
    let identity = io::id(&file)?;
    let nonce = nonce()?;
    let header = Header {
        schema: "nba2k27-rust-saved-media-publication-v1".into(),
        nonce: nonce.clone(),
        root: plan.root_id.clone(),
        parents: plan.parents.clone(),
        stage: plan.tree.identity.clone(),
        inventory: plan.review.inventory.clone(),
        journal: identity.clone(),
        originals: plan.original_records(),
    };
    let mut journal = Journal {
        file,
        bytes: vec![],
        identity,
    };
    journal.append(&Event::Header(header))?;
    journal.append(&Event::WorkspaceIntent)?;
    guard()?;
    let work = workspace(&plan.review.target, &nonce);
    fs::create_dir(&work)?;
    let work_pin = Pins::open(&work)?;
    journal.append(&Event::WorkspaceOwned {
        id: work_pin.identity()?,
    })?;
    if io::missing(&plan.review.target.join("Data/Rust"))? {
        journal.append(&Event::RustDirectoryIntent)?;
        guard()?;
        fs::create_dir(plan.review.target.join("Data/Rust"))?;
        journal.append(&Event::RustDirectoryOwned {
            id: Pins::open(&plan.review.target.join("Data/Rust"))?.identity()?,
        })?;
    }
    let _rust = Pins::open(&plan.review.target.join("Data/Rust"))?;
    let mut units = vec![];
    for spec in &plan.specs {
        guard()?;
        journal.append(&Event::CopyIntent { index: spec.index })?;
        let temp = temporary(&work, spec.index);
        if spec.directory {
            fs::create_dir(&temp)?;
        }
        let mut files = BTreeMap::new();
        for key in &spec.sources {
            let rel = if spec.directory {
                key.strip_prefix(&(spec.target.clone() + "/"))
                    .unwrap()
                    .to_string()
            } else {
                String::new()
            };
            let to = if spec.directory {
                io::create_parents(&temp, &rel)?;
                temp.join(&rel)
            } else {
                temp.clone()
            };
            guard()?;
            let owned = plan.tree.files.get_mut(key).unwrap().copy_new(&to, guard)
                .with_context(|| format!("Copying reviewed publication unit {} entry {rel}", spec.index))?;
            files.insert(rel, owned);
            fault("copied-file", spec.index)?;
        }
        let directories = if spec.directory {
            Tree::open(&temp)?.directories.clone()
        } else {
            BTreeMap::new()
        };
        let node = io::open_node(&temp, spec.directory, false)?;
        let unit = Unit {
            index: spec.index,
            id: io::id(&node)?,
            files,
            directories,
        };
        drop(node);
        validate_unit(&plan, &unit, &temp)?;
        journal.append(&Event::Prepared { unit: unit.clone() })?;
        units.push(unit);
        fault("prepared", spec.index)?;
    }
    plan.unchanged()?;
    for unit in &units {
        let spec = &plan.specs[unit.index];
        let temp = temporary(&work, unit.index);
        let target = plan.review.target.join(&spec.target);
        guard()?;
        ensure!(
            io::missing(&target)?,
            "Target appeared before publication; foreign path held"
        );
        validate_unit(&plan, unit, &temp)?;
        journal.append(&Event::PublishIntent { index: unit.index })?;
        fault("intent", unit.index)?;
        guard()?;
        let node = io::open_node(&temp, spec.directory, true)?;
        ensure!(
            io::id(&node)? == unit.id,
            "Prepared unit changed before rename"
        );
        io::rename(&node, &target)?;
        drop(node);
        fault("renamed", unit.index)?;
        validate_unit(&plan, unit, &target)?;
        journal.append(&Event::Published { index: unit.index })?;
        fault("published", unit.index)?;
    }
    guard()?;
    plan.unchanged()?;
    journal.append(&Event::Complete)?;
    Ok(plan.report("complete"))
}
struct Recovery {
    header: Header,
    units: BTreeMap<usize, Unit>,
    intents: BTreeSet<usize>,
    work_id: Option<Id>,
    complete: bool,
    rolled_back: bool,
    rollback_intents: BTreeSet<usize>,
    rollback_done: BTreeSet<usize>,
}
fn recovery(plan: &Prepared, journal: &Journal, events: &[Event]) -> Result<Recovery> {
    let Some(Event::Header(header)) = events.first() else {
        anyhow::bail!("Missing publication header")
    };
    ensure!(
        header.schema == "nba2k27-rust-saved-media-publication-v1"
            && header.nonce.len() == 48
            && header.nonce.bytes().all(|b| b.is_ascii_hexdigit())
            && header.root == plan.root_id
            && header.stage == plan.tree.identity
            && header.inventory == plan.review.inventory
            && header.journal == journal.identity,
        "Publication receipt belongs to different storage/review"
    );
    ensure!(
        header.originals == plan.original_records(),
        "Original source identities changed since publication"
    );
    ensure!(
        header
            .parents
            .iter()
            .all(|(rel, id)| plan.parents.get(rel) == Some(id)),
        "Publication storage parent identity changed"
    );
    let mut r = Recovery {
        header: header.clone(),
        units: BTreeMap::new(),
        intents: BTreeSet::new(),
        work_id: None,
        complete: false,
        rolled_back: false,
        rollback_intents: BTreeSet::new(),
        rollback_done: BTreeSet::new(),
    };
    let mut workspace_intent = false;
    let mut copy = BTreeSet::new();
    let mut published = BTreeSet::new();
    let mut rollback_intents = BTreeSet::new();
    let mut rollback_done = BTreeSet::new();
    let mut rust_intent = false;
    let mut rust_owned = None;
    for event in events.iter().skip(1) {
        ensure!(!r.rolled_back, "Unexpected event after rollback");
        match event {
            Event::Header(_) => anyhow::bail!("Duplicate publication header"),
            Event::WorkspaceIntent => {
                ensure!(!workspace_intent, "Duplicate workspace intent");
                workspace_intent = true;
            }
            Event::WorkspaceOwned { id } => {
                ensure!(
                    workspace_intent && r.work_id.is_none(),
                    "Invalid workspace ownership event"
                );
                r.work_id = Some(id.clone());
            }
            Event::RustDirectoryIntent => {
                ensure!(
                    r.work_id.is_some()
                        && copy.is_empty()
                        && !header.parents.contains_key("Data/Rust")
                        && !rust_intent,
                    "Invalid Rust directory intent"
                );
                rust_intent = true;
            }
            Event::RustDirectoryOwned { id } => {
                ensure!(
                    rust_intent && copy.is_empty() && rust_owned.is_none(),
                    "Invalid Rust directory ownership"
                );
                rust_owned = Some(id.clone());
            }
            Event::CopyIntent { index } => {
                ensure!(
                    *index < 5
                        && r.work_id.is_some()
                        && r.intents.is_empty()
                        && copy.insert(*index),
                    "Invalid/duplicate copy intent"
                );
            }
            Event::Prepared { unit } => {
                ensure!(
                    copy.contains(&unit.index)
                        && unit.index < 5
                        && !r.units.contains_key(&unit.index),
                    "Invalid prepared unit"
                );
                r.units.insert(unit.index, unit.clone());
            }
            Event::PublishIntent { index } => {
                ensure!(
                    r.units.len() == 5 && r.units.contains_key(index) && r.intents.insert(*index),
                    "Invalid/duplicate publication intent"
                );
            }
            Event::Published { index } => {
                ensure!(
                    r.intents.contains(index) && published.insert(*index),
                    "Invalid published unit event"
                );
            }
            Event::Complete => {
                ensure!(
                    published.len() == 5 && !r.complete,
                    "Invalid completion event"
                );
                r.complete = true;
            }
            Event::RollbackIntent { index } => {
                ensure!(
                    r.intents.contains(index) && rollback_intents.insert(*index),
                    "Invalid rollback intent"
                );
            }
            Event::RolledBackUnit { index } => {
                ensure!(
                    rollback_intents.contains(index) && rollback_done.insert(*index),
                    "Invalid rollback result"
                );
            }
            Event::RolledBack => {
                ensure!(
                    r.intents
                        .iter()
                        .all(|i| rollback_done.contains(i) || !published.contains(i)),
                    "Rollback did not account for published units"
                );
                r.rolled_back = true;
            }
        }
    }
    let work = workspace(&plan.review.target, &r.header.nonce);
    if let Some(id) = rust_owned {
        ensure!(
            plan.parents.get("Data/Rust") == Some(&id),
            "New Rust directory identity changed after publication"
        );
    }
    if let Some(id) = &r.work_id {
        ensure!(
            Pins::open(&work)?.identity()? == *id,
            "Recovery workspace identity changed"
        );
    }
    r.rollback_intents = rollback_intents;
    r.rollback_done = rollback_done;
    Ok(r)
}
fn recover(review: Review, rollback: bool, guard: &impl Fn() -> Result<()>) -> Result<Report> {
    guard()?;
    let mut plan = Prepared::open(review)?;
    let path = plan.review.target.join(RECEIPT);
    let (mut journal, events) = Journal::load(&path, rollback)?;
    let r = recovery(&plan, &journal, &events)?;
    ensure!(
        !r.rolled_back,
        "Receipt already rolled back; archive requires explicit review"
    );
    let work = workspace(&plan.review.target, &r.header.nonce);
    let mut final_units = vec![];
    let _work_pin = if r.work_id.is_some() {
        Some(Pins::open(&work)?)
    } else {
        None
    };
    // Validate every target before the first rollback mutation. Unprepared or
    // partial workspace bytes are intentionally retained, never recursively deleted.
    for spec in &plan.specs {
        let target = plan.review.target.join(&spec.target);
        let temp = temporary(&work, spec.index);
        if !io::missing(&target)? {
            let unit = r
                .units
                .get(&spec.index)
                .context("Foreign target appeared without owned prepared record")?;
            ensure!(
                r.intents.contains(&spec.index),
                "Target exists without durable publish intent"
            );
            ensure!(
                !r.rollback_done.contains(&spec.index),
                "Target reappeared after owned rollback; foreign path held"
            );
            validate_unit(&plan, unit, &target)?;
            ensure!(
                io::missing(&temp)?,
                "Both prepared and target unit exist; hold"
            );
            final_units.push(unit.clone());
        } else if let Some(unit) = r.units.get(&spec.index) {
            validate_unit(&plan, unit, &temp)?;
        }
    }
    plan.unchanged()?;
    if !rollback {
        return Ok(plan.report(
            if r.complete && final_units.len() == 5 && r.rollback_intents.is_empty() {
                "complete-verified"
            } else {
                "held-partial; originals untouched"
            },
        ));
    }
    for unit in final_units.iter().rev() {
        let spec = &plan.specs[unit.index];
        let target = plan.review.target.join(&spec.target);
        let temp = temporary(&work, unit.index);
        guard()?;
        validate_unit(&plan, unit, &target)?;
        if !r.rollback_intents.contains(&unit.index) {
            journal.append(&Event::RollbackIntent { index: unit.index })?;
        }
        let node = io::open_node(&target, spec.directory, true)?;
        ensure!(
            io::id(&node)? == unit.id,
            "Rollback source identity changed"
        );
        io::rename(&node, &temp)?;
        drop(node);
        validate_unit(&plan, unit, &temp)?;
        journal.append(&Event::RolledBackUnit { index: unit.index })?;
    }
    for index in r.rollback_intents.difference(&r.rollback_done) {
        if !final_units.iter().any(|u| u.index == *index) {
            journal.append(&Event::RolledBackUnit { index: *index })?;
        }
    }
    guard()?;
    plan.unchanged()?;
    journal.append(&Event::RolledBack)?;
    // Archive only this exact retained receipt, without deleting its workspace
    // or any original/user content. A fresh reviewed attempt gets a new nonce.
    let archive = if r.work_id.is_some() {
        work.join("publication-rolled-back.jsonl")
    } else {
        plan.review.target.join("Recovery").join(format!(
            "rust-saved-media-{}-rolled-back.jsonl",
            r.header.nonce
        ))
    };
    ensure!(io::missing(&archive)?, "Rollback receipt archive exists");
    io::rename(&journal.file, &archive)?;
    Ok(plan.report("rolled-back; new dataset retained in Recovery"))
}
pub(crate) fn verify_reviewed_main() -> Result<Report> {
    let mut plan = Prepared::open(reviewed())?;
    ensure!(
        io::missing(&plan.review.target.join(RECEIPT))?,
        "Publication recovery receipt exists; inspect instead"
    );
    for spec in &plan.specs {
        ensure!(
            io::missing(&plan.review.target.join(&spec.target))?,
            "Publication target collision; never merge/replace"
        );
    }
    plan.unchanged()?;
    Ok(plan.report("reviewed; no writes"))
}
pub(crate) fn publish_reviewed_main(guard: impl Fn() -> Result<()>) -> Result<Report> {
    publish(reviewed(), &guard, &|_, _| Ok(()))
}
pub(crate) fn inspect_reviewed_main(guard: impl Fn() -> Result<()>) -> Result<Report> {
    recover(reviewed(), false, &guard)
}
pub(crate) fn rollback_reviewed_main(guard: impl Fn() -> Result<()>) -> Result<Report> {
    recover(reviewed(), true, &guard)
}

#[cfg(saved_media_publish_offline)]
#[path = "saved_media_publish_tests.rs"]
pub(crate) mod tests;
