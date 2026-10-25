//! Backend-owned immutable inputs, independent of the evictable preview cache.
//! Capture/validation workers use the short mutation read lock; packet sealing,
//! browser decoding, and Blender supervision do not hold that gate.
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LIMIT: usize = 8;
const FILE_LIMIT: usize = 1024;
const BYTE_LIMIT: u64 = 1024 * 1024 * 1024;
const TTL: Duration = Duration::from_secs(10 * 60);
const URI: &str = "cmt-preview://blender-snapshot/";
static NEXT: AtomicU64 = AtomicU64::new(1);

fn identity(payload: &Value) -> Result<Value> {
    let target = PathBuf::from(
        payload["target"]
            .as_str()
            .context("Choose a staged player")?,
    )
    .canonicalize()?;
    let owner = payload["handoffId"]
        .as_str()
        .context("Missing Blender snapshot owner")?;
    ensure!(
        !owner.is_empty() && owner.len() <= 128,
        "Invalid Blender snapshot owner"
    );
    Ok(
        json!({"target":target.to_string_lossy().to_lowercase(), "handoffId":owner,
        "config":payload["config"], "includeHair":payload["includeHair"] == true,
        "includeFacialHair":payload["includeFacialHair"] == true,
        "includeTextures":payload["includeTextures"] == true,
        "includeJersey":payload["includeJersey"] == true, "fitChest":payload["fitChest"] == true}),
    )
}

struct Resource {
    path: PathBuf,
    digest: String,
    size: u64,
    archive: bool,
    packet: bool,
}
impl Resource {
    fn read(&self) -> Result<Vec<u8>> {
        let metadata = fs::symlink_metadata(&self.path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == self.size
                && metadata.len() <= BYTE_LIMIT,
            "Blender snapshot resource changed"
        );
        let bytes = fs::read(&self.path)?;
        ensure!(
            format!("{:x}", Sha256::digest(&bytes)) == self.digest,
            "Blender snapshot resource changed"
        );
        Ok(bytes)
    }
}

pub struct Snapshot {
    root: PathBuf,
    fallback: Value,
    resources: HashMap<String, Resource>,
    created: Instant,
}
impl Snapshot {
    pub fn worker_payload(&self, request: &Value) -> Value {
        let mut payload = request.clone();
        // Never accept an input path or digest supplied by the browser.
        payload["_blenderSnapshot"] = json!(self.root);
        payload
    }
    fn verify(&self) -> Result<()> {
        for resource in self.resources.values() {
            resource.read()?;
        }
        Ok(())
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        // root is generated here, never taken from a client request.
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Entry {
    identity: Value,
    snapshot: Option<Arc<Snapshot>>,
}
#[derive(Default)]
pub struct Snapshots {
    entries: Mutex<HashMap<String, Entry>>,
    active: Mutex<HashMap<String, Weak<Snapshot>>>,
}

impl Snapshots {
    pub fn owns_uri(path: &str) -> bool {
        path.starts_with(URI)
    }

    pub fn prepare(
        &self,
        session: &Path,
        data: &Path,
        request: &Value,
        mut worker: impl FnMut(&str, Value) -> Result<Value>,
    ) -> Result<Value> {
        let identity = identity(request)?;
        let id = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        {
            let mut entries = self.entries.lock().unwrap();
            let mut active = self.active.lock().unwrap();
            active.retain(|_, snapshot| snapshot.strong_count() > 0);
            entries.retain(|_, entry| {
                entry
                    .snapshot
                    .as_ref()
                    .is_none_or(|s| s.created.elapsed() < TTL)
            });
            ensure!(
                entries.len() + active.len() < LIMIT,
                "Too many Blender preparations; finish or close an existing preview"
            );
            ensure!(
                !active.contains_key(identity["target"].as_str().unwrap()),
                "This staged player already has an open Blender handoff"
            );
            ensure!(
                !entries
                    .values()
                    .any(|entry| entry.identity["target"] == identity["target"]),
                "This staged player already has a Blender preparation"
            );
            entries.insert(
                id.clone(),
                Entry {
                    identity: identity.clone(),
                    snapshot: None,
                },
            );
        }
        let result = (|| -> Result<(Snapshot, Value)> {
            let root = session.join(format!("blender-snapshot-{id}"));
            fs::create_dir(&root)?;
            let mut snapshot = Snapshot {
                root,
                fallback: Value::Null,
                resources: HashMap::new(),
                created: Instant::now(),
            };
            let internal = snapshot.worker_payload(request);
            let fallback = worker("blender-snapshot-prepare", internal.clone())?;
            let native = snapshot.seal_native(data, request, &fallback, &id);
            if native.is_err() {
                snapshot.resources.clear();
                let _ = fs::remove_dir_all(snapshot.root.join("native"));
            }
            snapshot.fallback = snapshot.seal_fallback(fallback, &id)?;
            snapshot.fallback["snapshotId"] = json!(id);
            let mut resources = match native {
                Ok(resources) => resources,
                Err(error) => {
                    let mut resources = snapshot.fallback.clone();
                    resources["warnings"]
                        .as_array_mut()
                        .context("Invalid snapshot warnings")?
                        .push(json!(format!(
                            "Native Blender snapshot fallback: {error:#}"
                        )));
                    resources
                }
            };
            worker("blender-snapshot-validate", internal)?;
            resources["snapshotId"] = json!(id);
            Ok((snapshot, resources))
        })();
        let mut entries = self.entries.lock().unwrap();
        match result {
            Ok((snapshot, resources)) => {
                entries
                    .get_mut(&id)
                    .context("Blender preparation was released")?
                    .snapshot = Some(Arc::new(snapshot));
                Ok(resources)
            }
            Err(error) => {
                entries.remove(&id);
                Err(error)
            }
        }
    }

    pub fn release(&self, payload: &Value) -> Result<()> {
        let id = payload["snapshotId"]
            .as_str()
            .context("Missing Blender snapshot lease")?;
        let owner = payload["handoffId"]
            .as_str()
            .context("Missing Blender snapshot owner")?;
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(id) {
            ensure!(
                entry.identity["handoffId"] == owner,
                "Blender snapshot belongs to another preparation"
            );
            ensure!(
                entry.snapshot.is_some(),
                "Blender snapshot is still preparing"
            );
            entries.remove(id);
        }
        Ok(())
    }

    pub fn claim(&self, payload: &Value) -> Result<Arc<Snapshot>> {
        let id = payload["snapshotId"]
            .as_str()
            .context("Prepare an immutable Blender snapshot first")?;
        let identity = identity(payload)?;
        let snapshot = {
            let mut entries = self.entries.lock().unwrap();
            let entry = entries
                .get(id)
                .context("Blender snapshot expired or was already opened")?;
            ensure!(
                entry.identity == identity,
                "Blender snapshot belongs to a different request"
            );
            ensure!(
                entry.snapshot.is_some(),
                "Blender snapshot is still preparing"
            );
            let snapshot = entries.remove(id).unwrap().snapshot.unwrap();
            self.active.lock().unwrap().insert(
                identity["target"].as_str().unwrap().to_owned(),
                Arc::downgrade(&snapshot),
            );
            snapshot
        };
        ensure!(
            snapshot.created.elapsed() < TTL,
            "Blender snapshot expired; reopen the preview"
        );
        snapshot.verify()?;
        Ok(snapshot)
    }

    pub fn fallback(&self, payload: &Value) -> Result<Value> {
        let id = payload["snapshotId"]
            .as_str()
            .context("Missing Blender snapshot lease")?;
        let identity = identity(payload)?;
        let snapshot = {
            let entries = self.entries.lock().unwrap();
            let entry = entries.get(id).context("Blender snapshot expired")?;
            ensure!(
                entry.identity == identity,
                "Blender snapshot belongs to a different request"
            );
            entry
                .snapshot
                .clone()
                .context("Blender snapshot is still preparing")?
        };
        ensure!(snapshot.created.elapsed() < TTL, "Blender snapshot expired");
        snapshot.verify()?;
        Ok(snapshot.fallback.clone())
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap()
            .retain(|_, e| e.snapshot.is_none());
    }

    fn resource(&self, uri: &str) -> Result<(Arc<Snapshot>, String)> {
        let suffix = uri
            .strip_prefix(URI)
            .context("Invalid Blender snapshot resource")?;
        let (id, _) = suffix
            .split_once('/')
            .context("Invalid Blender snapshot resource")?;
        let entries = self.entries.lock().unwrap();
        let snapshot = entries
            .get(id)
            .and_then(|e| e.snapshot.clone())
            .context("Blender snapshot resource expired")?;
        ensure!(
            snapshot.created.elapsed() < TTL,
            "Blender snapshot resource expired"
        );
        ensure!(
            snapshot.resources.contains_key(uri),
            "Unowned Blender snapshot resource"
        );
        Ok((snapshot, uri.to_owned()))
    }

    pub fn read(&self, command: &str, uri: &str, binary: bool) -> Result<ValueOrBytes> {
        let (snapshot, key) = self.resource(uri)?;
        let resource = &snapshot.resources[&key];
        let bytes = resource.read()?;
        if command == "file-stat" && !binary {
            return Ok(ValueOrBytes::Json(
                json!({"path":uri,"size":resource.size,"mtimeMs":0}),
            ));
        }
        if command == "read-archive" {
            ensure!(
                resource.archive && resource.packet,
                "Use the frozen compatibility archive reader"
            );
            return if binary {
                Ok(ValueOrBytes::Bytes(bytes))
            } else {
                Ok(ValueOrBytes::Json(crate::preview_packet::archive_json(
                    &bytes,
                )?))
            };
        }
        ensure!(
            command == "read-file" && !resource.packet,
            "Unsupported Blender snapshot read"
        );
        if binary {
            Ok(ValueOrBytes::Bytes(crate::preview_packet::file(&bytes)?))
        } else {
            Ok(ValueOrBytes::Json(
                json!({"data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
            ))
        }
    }
}

pub enum ValueOrBytes {
    Json(Value),
    Bytes(Vec<u8>),
}

impl Snapshot {
    fn add(&mut self, id: &str, path: PathBuf, archive: bool, packet: bool) -> Result<String> {
        ensure!(
            path.canonicalize()?.starts_with(self.root.canonicalize()?),
            "Unowned Blender snapshot path"
        );
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() <= 512 * 1024 * 1024,
            "Invalid or oversized Blender snapshot input"
        );
        let bytes = fs::read(&path)?;
        let size = bytes.len() as u64;
        ensure!(
            self.resources.len() < FILE_LIMIT
                && size <= 512 * 1024 * 1024
                && self.resources.values().map(|r| r.size).sum::<u64>() + size <= BYTE_LIMIT,
            "Blender snapshot exceeds its resource budget"
        );
        let uri = format!(
            "{URI}{id}/{}/{}",
            self.resources.len(),
            path.file_name()
                .context("Missing snapshot resource name")?
                .to_string_lossy()
        );
        self.resources.insert(
            uri.clone(),
            Resource {
                path,
                digest: format!("{:x}", Sha256::digest(&bytes)),
                size,
                archive,
                packet,
            },
        );
        Ok(uri)
    }

    fn seal_fallback(&mut self, resources: Value, id: &str) -> Result<Value> {
        let main = PathBuf::from(
            resources["filePath"]
                .as_str()
                .context("Missing snapshot main")?,
        );
        self.add(id, main, true, false)?;
        for (name, archive) in [("companionPaths", true), ("externalTextures", false)] {
            if let Some(rows) = resources["options"][name].as_array() {
                for row in rows {
                    let path = PathBuf::from(
                        row["path"]
                            .as_str()
                            .context("Missing snapshot dependency")?,
                    );
                    self.add(id, path, archive, false)?;
                }
            }
        }
        Ok(resources)
    }

    fn seal_native(
        &mut self,
        data: &Path,
        request: &Value,
        fallback: &Value,
        id: &str,
    ) -> Result<Value> {
        let mut assets = crate::assets::Assets::new(Arc::new(AtomicBool::new(false)));
        assets.configure(data)?;
        let mut input = request.clone();
        input["target"] = fallback["filePath"].clone();
        let mut resources = assets.prepare(&input)?;
        let directory = self.root.join("native");
        fs::create_dir(&directory)?;
        let mut capture = |snapshot: &mut Self, source: &str, archive: bool| -> Result<String> {
            let leaf = source
                .rsplit(['/', '\\'])
                .next()
                .context("Missing native resource name")?;
            let folder = directory.join(snapshot.resources.len().to_string());
            fs::create_dir(&folder)?;
            let path = folder.join(leaf);
            let bytes = if archive {
                assets.binary_archive(source)?
            } else {
                base64::engine::general_purpose::STANDARD.decode(
                    assets.read_file(source)?["data"]
                        .as_str()
                        .context("Invalid native snapshot texture")?,
                )?
            };
            ensure!(
                bytes.len() as u64 <= BYTE_LIMIT,
                "Blender snapshot packet exceeds budget"
            );
            fs::write(&path, bytes)?;
            snapshot.add(id, path, archive, archive)
        };
        let main = resources["filePath"]
            .as_str()
            .context("Missing native snapshot main")?
            .to_owned();
        resources["filePath"] = json!(capture(self, &main, true)?);
        for (name, archive) in [("companionPaths", true), ("externalTextures", false)] {
            if let Some(rows) = resources["options"][name].as_array_mut() {
                for row in rows {
                    let source = row["path"]
                        .as_str()
                        .context("Missing native snapshot dependency")?
                        .to_owned();
                    row["path"] = json!(capture(self, &source, archive)?);
                }
            }
        }
        resources
            .as_object_mut()
            .context("Invalid native snapshot resources")?
            .remove("resourceLease");
        Ok(resources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "cmt-blender-snapshot-rust-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn lexical_target_aliases_share_identity_but_settings_and_flags_do_not() {
        let root = temp();
        fs::write(root.join("main.iff"), b"fixture").unwrap();
        fs::create_dir(root.join("child")).unwrap();
        let a = json!({"target":root.join("main.iff"),"handoffId":"owner","config":"custom"});
        let mut b = a.clone();
        b["target"] = json!(root.join("child/.././main.iff"));
        assert_eq!(identity(&a).unwrap(), identity(&b).unwrap());
        b["includeTextures"] = json!(true);
        assert_ne!(identity(&a).unwrap(), identity(&b).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn release_ownership_claim_once_integrity_and_cleanup() {
        let root = temp();
        let source = root.join("main.iff");
        fs::write(&source, b"snapshot").unwrap();
        let payload = json!({"target":source,"handoffId":"owner","snapshotId":"fixture"});
        let identity = identity(&payload).unwrap();
        let mut snapshot = Snapshot {
            root: root.clone(),
            fallback: Value::Null,
            resources: HashMap::new(),
            created: Instant::now(),
        };
        let uri = snapshot
            .add("fixture", source.clone(), false, false)
            .unwrap();
        let leases = Snapshots::default();
        leases.entries.lock().unwrap().insert(
            "fixture".into(),
            Entry {
                identity,
                snapshot: Some(Arc::new(snapshot)),
            },
        );
        assert!(
            leases
                .release(&json!({"snapshotId":"fixture","handoffId":"other"}))
                .is_err()
        );
        assert!(
            leases
                .read("read-file", &(uri.clone() + "/../x"), false)
                .is_err()
        );
        let owned = leases.claim(&payload).unwrap();
        assert!(leases.claim(&payload).is_err());
        leases.release(&payload).unwrap();
        assert!(source.is_file());
        fs::write(&source, b"replaced").unwrap();
        assert!(owned.verify().is_err());
        drop(owned);
        assert!(!root.exists());
    }

    #[test]
    fn failed_preparation_releases_reservation_and_only_its_own_files() {
        let root = temp();
        let target = root.join("main.iff");
        fs::write(&target, b"fixture").unwrap();
        let payload = json!({"target":target,"handoffId":"owner"});
        let leases = Snapshots::default();
        for _ in 0..2 {
            assert!(
                leases
                    .prepare(&root, &root, &payload, |_, payload| {
                        let directory =
                            PathBuf::from(payload["_blenderSnapshot"].as_str().unwrap());
                        fs::write(directory.join("partial"), b"partial").unwrap();
                        anyhow::bail!("held preparation failed")
                    })
                    .is_err()
            );
            assert!(leases.entries.lock().unwrap().is_empty());
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_target_alias_reservation_and_resource_limits_are_bounded() {
        let root = temp();
        let target = root.join("main.iff");
        fs::write(&target, b"fixture").unwrap();
        let payload = json!({"target":target,"handoffId":"owner"});
        let leases = Snapshots::default();
        let entry_identity = identity(&payload).unwrap();
        leases.entries.lock().unwrap().insert(
            "pending".into(),
            Entry {
                identity: entry_identity,
                snapshot: None,
            },
        );
        let alias = json!({"target":root.join("./main.iff"),"handoffId":"other"});
        assert!(
            leases
                .prepare(&root, &root, &alias, |_, _| unreachable!())
                .is_err()
        );
        leases.entries.lock().unwrap().clear();
        for id in 0..LIMIT {
            leases.entries.lock().unwrap().insert(
                id.to_string(),
                Entry {
                    identity: json!({"target":id}),
                    snapshot: None,
                },
            );
        }
        assert!(
            leases
                .prepare(&root, &root, &payload, |_, _| unreachable!())
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn expired_claim_drops_files_and_request_config_cannot_change_ownership() {
        let root = temp();
        let target = root.join("main.iff");
        fs::write(&target, b"fixture").unwrap();
        let payload =
            json!({"target":target,"handoffId":"owner","snapshotId":"old","config":"custom"});
        let snapshot = Arc::new(Snapshot {
            root: root.clone(),
            fallback: Value::Null,
            resources: HashMap::new(),
            created: Instant::now() - TTL - Duration::from_secs(1),
        });
        let leases = Snapshots::default();
        leases.entries.lock().unwrap().insert(
            "old".into(),
            Entry {
                identity: identity(&payload).unwrap(),
                snapshot: Some(snapshot),
            },
        );
        let mut wrong = payload.clone();
        wrong["config"] = json!("other");
        assert!(leases.claim(&wrong).is_err());
        assert!(root.exists());
        assert!(leases.claim(&payload).is_err());
        assert!(!root.exists());
        assert!(leases.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn native_packets_survive_original_replacement_without_a_preview_cache_or_game() {
        let root = temp();
        let original = root.join("main.iff");
        fs::write(&original, b"original").unwrap();
        let identity = identity(&json!({"target":original,"handoffId":"owner"})).unwrap();
        let bytes = crate::preview_packet::archive(
            &std::collections::BTreeMap::from([("main.scne".into(), b"frozen scene".to_vec())]),
            "geometry",
            "content",
            &[],
        )
        .unwrap();
        let packet = root.join("packet.bin");
        fs::write(&packet, &bytes).unwrap();
        let mut snapshot = Snapshot {
            root: root.clone(),
            fallback: Value::Null,
            resources: HashMap::new(),
            created: Instant::now(),
        };
        let uri = snapshot.add("packet", packet, true, true).unwrap();
        let leases = Snapshots::default();
        leases.entries.lock().unwrap().insert(
            "packet".into(),
            Entry {
                identity,
                snapshot: Some(Arc::new(snapshot)),
            },
        );
        fs::write(original, b"full swap replacement").unwrap();
        match leases.read("read-archive", &uri, true).unwrap() {
            ValueOrBytes::Bytes(actual) => assert_eq!(actual, bytes),
            _ => panic!("Expected native packet"),
        }
        match leases.read("read-archive", &uri, false).unwrap() {
            ValueOrBytes::Json(value) => assert_eq!(value["geometryKey"], "geometry"),
            _ => panic!("Expected JSON archive"),
        }
        leases.clear();
        assert!(!root.exists());
    }

    #[test]
    fn claim_keeps_canonical_target_owned_until_supervision_ends() {
        let root = temp();
        let target = root.join("main.iff");
        fs::write(&target, b"fixture").unwrap();
        let payload = json!({"target":target,"handoffId":"owner","snapshotId":"live"});
        let leases = Snapshots::default();
        leases.entries.lock().unwrap().insert(
            "live".into(),
            Entry {
                identity: identity(&payload).unwrap(),
                snapshot: Some(Arc::new(Snapshot {
                    root: root.join("blender-snapshot-live"),
                    fallback: Value::Null,
                    resources: HashMap::new(),
                    created: Instant::now(),
                })),
            },
        );
        let owned = leases.claim(&payload).unwrap();
        leases.release(&payload).unwrap();
        leases.clear();
        let alias = json!({"target":root.join("./main.iff"),"handoffId":"another"});
        assert!(
            leases
                .prepare(&root, &root, &alias, |_, _| unreachable!())
                .unwrap_err()
                .to_string()
                .contains("open Blender")
        );
        drop(owned);
        let mut called = false;
        assert!(
            leases
                .prepare(&root, &root, &alias, |_, _| {
                    called = true;
                    anyhow::bail!("test stop")
                })
                .is_err()
        );
        assert!(called);
        fs::remove_dir_all(root).unwrap();
    }
}
