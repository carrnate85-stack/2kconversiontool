use super::*;
use sha2::Digest;
use std::{cell::Cell, os::windows::fs::OpenOptionsExt};

const TOOL: &str = "Backend/blender_tools/NBA_Character_HeadSwap/__init__.py";
const PAYLOAD: [(&str, &[u8]); 3] = [
    ("Backend/a.py", b"original Python source"),
    ("Backend/python_runtime/python.exe", b"inert Python marker"),
    (TOOL, b"original Blender tool"),
];

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cmt-runtime-retirement-{}", stamp()));
        fs::create_dir(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }

    fn runtime(&self, digit: char, modified: u64) -> (PathBuf, Inventory) {
        use std::{
            fs::FileTimes,
            time::{Duration, UNIX_EPOCH},
        };
        let hash = digit.to_string().repeat(64);
        let root = self.0.join(format!("engine-{}", &hash[..16]));
        let mut files = BTreeMap::new();
        for (name, bytes) in PAYLOAD {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
            files.insert(
                name.into(),
                FileRecord {
                    bytes: bytes.len() as u64,
                    sha256: format!("{:x}", sha2::Sha256::digest(bytes)),
                },
            );
        }
        write_inventory(&root, &hash, files).unwrap();
        fs::write(root.join("ready.sha256"), &hash).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(root.join("ready.sha256"))
            .unwrap()
            .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(modified)))
            .unwrap();
        let snapshot = inventory(&root, Some(&hash)).unwrap();
        (root, snapshot)
    }

    fn retired(&self) -> (PathBuf, Inventory) {
        let (root, snapshot) = self.runtime('a', 100);
        verify_tree(&root, &self.0, &snapshot, true).unwrap();
        let retired = self
            .0
            .join(format!(".retired-{}-{}", &snapshot.payload[..16], stamp()));
        fs::rename(root, &retired).unwrap();
        (retired, snapshot)
    }

    fn intact(&self, root: &Path) {
        for (name, bytes) in PAYLOAD {
            assert_eq!(fs::read(root.join(name)).unwrap(), bytes);
        }
        assert!(root.join("inventory.json").is_file());
        assert!(root.join("ready.sha256").is_file());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(
            fs::canonicalize(std::env::temp_dir().join(self.0.file_name().unwrap())).unwrap(),
            self.0
        );
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn retired_verified_payload_and_metadata_delete_successfully() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    remove_retired(&root, &f.0, &snapshot).unwrap();
    assert!(!root.exists());
}

#[test]
fn retired_changed_metadata_is_rejected_before_any_payload_deletion() {
    let f = Fixture::new();
    for name in ["inventory.json", "ready.sha256"] {
        let (root, snapshot) = f.retired();
        let path = root.join(name);
        if name == "inventory.json" {
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b' '); // Still valid JSON, but not the captured byte identity.
            fs::write(&path, bytes).unwrap();
        } else {
            fs::write(&path, "b".repeat(64)).unwrap();
        }
        assert!(remove_retired(&root, &f.0, &snapshot).is_err());
        for (name, bytes) in PAYLOAD {
            assert_eq!(fs::read(root.join(name)).unwrap(), bytes);
        }
        assert!(path.exists());
    }
}

#[test]
fn retired_same_size_modified_tool_and_missing_inventory_are_preserved() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let modified = vec![b'x'; PAYLOAD[2].1.len()];
    fs::write(root.join(TOOL), &modified).unwrap();
    assert!(remove_retired(&root, &f.0, &snapshot).is_err());
    assert_eq!(fs::read(root.join(TOOL)).unwrap(), modified);
    assert!(root.join(PAYLOAD[0].0).exists());
    let (missing, snapshot) = f.retired();
    fs::remove_file(missing.join("inventory.json")).unwrap();
    assert!(remove_retired(&missing, &f.0, &snapshot).is_err());
    for (name, bytes) in PAYLOAD {
        assert_eq!(fs::read(missing.join(name)).unwrap(), bytes);
    }
}

#[test]
fn retired_windows_locks_block_all_deletion_until_retry() {
    let f = Fixture::new();
    for name in [TOOL, "inventory.json", "ready.sha256"] {
        let (root, snapshot) = f.retired();
        let active = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(root.join(name))
            .unwrap();
        assert!(remove_retired(&root, &f.0, &snapshot).is_err());
        f.intact(&root);
        drop(active);
        remove_retired(&root, &f.0, &snapshot).unwrap();
        assert!(!root.exists());
    }
}

#[test]
fn retired_verified_handles_deny_changes_replacement_and_rename() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let replacement = f.0.join("replacement");
    fs::write(&replacement, b"foreign replacement").unwrap();
    let result = remove_retired_with(
        &root,
        &f.0,
        &snapshot,
        || {
            for name in [TOOL, "inventory.json", "ready.sha256"] {
                assert!(fs::write(root.join(name), b"modified").is_err());
                assert!(fs::remove_file(root.join(name)).is_err());
                assert!(fs::rename(root.join(name), root.join("moved")).is_err());
                assert!(fs::rename(&replacement, root.join(name)).is_err());
            }
            assert!(fs::rename(&root, f.0.join("moved-runtime")).is_err());
            assert!(fs::rename(root.join("Backend"), root.join("moved-backend")).is_err());
            Err(anyhow::anyhow!("stop after mutation probe"))
        },
        |_| Ok(()),
    );
    assert!(result.unwrap_err().to_string().contains("mutation probe"));
    f.intact(&root);
    assert_eq!(fs::read(&replacement).unwrap(), b"foreign replacement");
    remove_retired(&root, &f.0, &snapshot).unwrap();
}

#[test]
fn retired_foreign_entry_injected_after_verification_blocks_first_delete() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let foreign = root.join("unknown.blend");
    let result = remove_retired_with(
        &root,
        &f.0,
        &snapshot,
        || {
            fs::write(&foreign, b"new user scene")?;
            Ok(())
        },
        |_| Ok(()),
    );
    assert!(result.unwrap_err().to_string().contains("unverified entry"));
    f.intact(&root);
    assert_eq!(fs::read(foreign).unwrap(), b"new user scene");
}

#[test]
fn retired_alternate_streams_on_payload_metadata_and_directories_are_preserved() {
    let f = Fixture::new();
    for name in [TOOL, "inventory.json", "ready.sha256", "Backend", ""] {
        let (root, snapshot) = f.retired();
        let path = if name.is_empty() {
            root.clone()
        } else {
            root.join(name)
        };
        let stream = PathBuf::from(format!("{}:user-notes", path.display()));
        fs::write(&stream, b"unknown alternate stream").unwrap();
        assert!(remove_retired(&root, &f.0, &snapshot).is_err());
        f.intact(&root);
        assert_eq!(fs::read(&stream).unwrap(), b"unknown alternate stream");
        fs::remove_file(stream).unwrap();
        remove_retired(&root, &f.0, &snapshot).unwrap();
    }
}

#[test]
fn retired_ads_created_after_verification_blocks_first_deletion() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let stream = PathBuf::from(format!("{}:user-notes", root.join(TOOL).display()));
    let result = remove_retired_with(
        &root,
        &f.0,
        &snapshot,
        || {
            fs::write(&stream, b"late alternate stream")?;
            Ok(())
        },
        |_| Ok(()),
    );
    assert!(result.is_err());
    f.intact(&root);
    assert_eq!(fs::read(stream).unwrap(), b"late alternate stream");
}

#[test]
fn retired_generated_bytecode_rules_reject_unknown_files_and_directories() {
    let f = Fixture::new();
    for name in [
        "unknown.blend",
        "Backend/__pycache__/user_work.cpython-312.pyc",
        "Backend/__pycache__/unknown.blend",
        "empty-user-directory",
    ] {
        let (root, snapshot) = f.retired();
        let path = root.join(name);
        if name == "empty-user-directory" {
            fs::create_dir(&path).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"unknown user content").unwrap();
        }
        assert!(remove_retired(&root, &f.0, &snapshot).is_err());
        f.intact(&root);
        assert!(path.exists());
    }
    let (root, snapshot) = f.retired();
    fs::create_dir(root.join("Backend/__pycache__")).unwrap();
    fs::write(
        root.join("Backend/__pycache__/a.cpython-312.pyc"),
        b"generated cache",
    )
    .unwrap();
    remove_retired(&root, &f.0, &snapshot).unwrap();
    assert!(!root.exists());
}

#[test]
fn retired_partial_deletion_retries_but_preserves_new_unknown_blend() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let deleted = Cell::new(0);
    let result = remove_retired_with(
        &root,
        &f.0,
        &snapshot,
        || Ok(()),
        |_| {
            deleted.set(deleted.get() + 1);
            Err(anyhow::anyhow!("simulated interruption"))
        },
    );
    assert!(result.is_err());
    assert_eq!(deleted.get(), 1);
    assert!(root.join("inventory.json").exists() && root.join("ready.sha256").exists());
    let remaining: Vec<_> = PAYLOAD
        .iter()
        .map(|(name, _)| root.join(name))
        .filter(|path| path.exists())
        .collect();
    assert_eq!(remaining.len(), PAYLOAD.len() - 1);
    let foreign = root.join("unknown.blend");
    fs::write(&foreign, b"retained after interrupted retirement").unwrap();
    let retry = inventory(&root, None).unwrap();
    assert!(remove_retired(&root, &f.0, &retry).is_err());
    for path in remaining {
        assert!(path.exists());
    }
    assert_eq!(
        fs::read(&foreign).unwrap(),
        b"retained after interrupted retirement"
    );
    fs::remove_file(foreign).unwrap();
    fs::remove_file(root.join("ready.sha256")).unwrap();
    remove_retired(&root, &f.0, &retry).unwrap();
    assert!(!root.exists());
}

#[test]
fn retired_late_foreign_file_preserves_metadata_after_payload_deletion() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let foreign = root.join("unknown.blend");
    let injected = Cell::new(false);
    let result = remove_retired_with(
        &root,
        &f.0,
        &snapshot,
        || Ok(()),
        |_| {
            if !injected.replace(true) {
                fs::write(&foreign, b"late user scene")?;
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(root.join("inventory.json").exists() && root.join("ready.sha256").exists());
    assert_eq!(fs::read(&foreign).unwrap(), b"late user scene");
    fs::remove_file(foreign).unwrap();
    let retry = inventory(&root, None).unwrap();
    remove_retired(&root, &f.0, &retry).unwrap();
}

#[test]
fn retired_late_metadata_ads_is_preserved_with_retry_proof() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let stream = PathBuf::from(format!(
        "{}:user-notes",
        root.join("inventory.json").display()
    ));
    let injected = Cell::new(false);
    let result = remove_retired_with(
        &root,
        &f.0,
        &snapshot,
        || Ok(()),
        |_| {
            if !injected.replace(true) {
                fs::write(&stream, b"late metadata stream")?;
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert!(root.join("inventory.json").exists() && root.join("ready.sha256").exists());
    assert_eq!(fs::read(&stream).unwrap(), b"late metadata stream");
    fs::remove_file(stream).unwrap();
    let retry = inventory(&root, None).unwrap();
    remove_retired(&root, &f.0, &retry).unwrap();
}

#[test]
fn retired_delete_failure_keeps_retry_proof() {
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    let tool = root.join(TOOL);
    let mut permissions = fs::metadata(&tool).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&tool, permissions).unwrap();
    assert!(remove_retired(&root, &f.0, &snapshot).is_err());
    assert!(root.join("inventory.json").exists());
    let mut permissions = fs::metadata(&tool).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&tool, permissions).unwrap();
    let retry = inventory(&root, None).unwrap();
    remove_retired(&root, &f.0, &retry).unwrap();
}

#[test]
fn retired_links_and_outside_paths_are_never_deleted() {
    use std::{os::windows::process::CommandExt, process::Command};
    let f = Fixture::new();
    let (root, snapshot) = f.retired();
    assert!(remove_retired(&root, &f.0.join("wrong-cache"), &snapshot).is_err());
    let outside = f.0.join("outside-tools");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("unknown.blend"), b"external scene").unwrap();
    let link = root.join("Backend/linked-tools");
    let output = Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(remove_retired(&root, &f.0, &snapshot).is_err());
    f.intact(&root);
    assert_eq!(
        fs::read(outside.join("unknown.blend")).unwrap(),
        b"external scene"
    );
    fs::remove_dir(link).unwrap();
    remove_retired(&root, &f.0, &snapshot).unwrap();
    let (root, snapshot) = f.retired();
    let saved = f.0.join("saved-retirement");
    fs::rename(&root, &saved).unwrap();
    let output = Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&root)
        .arg(&outside)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(remove_retired(&root, &f.0, &snapshot).is_err());
    assert_eq!(
        fs::read(outside.join("unknown.blend")).unwrap(),
        b"external scene"
    );
    fs::remove_dir(&root).unwrap();
    fs::rename(saved, &root).unwrap();
    remove_retired(&root, &f.0, &snapshot).unwrap();
}

#[test]
fn retired_inventory_byte_identity_supports_existing_json_formatting() {
    let f = Fixture::new();
    let (original, _) = f.runtime('a', 100);
    let path = original.join("inventory.json");
    let mut bytes = fs::read(&path).unwrap();
    bytes.extend_from_slice(b"\n  ");
    fs::write(path, bytes).unwrap();
    let snapshot = inventory(&original, None).unwrap();
    verify_tree(&original, &f.0, &snapshot, true).unwrap();
    let root =
        f.0.join(format!(".retired-{}-{}", &snapshot.payload[..16], stamp()));
    fs::rename(original, &root).unwrap();
    remove_retired(&root, &f.0, &snapshot).unwrap();
}

#[test]
fn retired_retry_honors_active_runtime_lease() {
    let f = Fixture::new();
    let (_, current) = f.runtime('b', 300);
    let (root, snapshot) = f.retired();
    let active = runtime_lease(&f.0, &snapshot.payload, false).unwrap();
    assert_eq!(
        prune(&f.0, &current.payload, &f.0.join("settings.json")).unwrap(),
        0
    );
    f.intact(&root);
    drop(active);
    assert_eq!(
        prune(&f.0, &current.payload, &f.0.join("settings.json")).unwrap(),
        1
    );
    assert!(!root.exists());
}

#[test]
fn retired_rechecks_changes_in_existing_pre_rename_path_window() {
    let f = Fixture::new();
    let (original, snapshot) = f.runtime('a', 100);
    verify_tree(&original, &f.0, &snapshot, true).unwrap();
    fs::write(original.join(TOOL), b"user change after preflight").unwrap();
    let root =
        f.0.join(format!(".retired-{}-{}", &snapshot.payload[..16], stamp()));
    fs::rename(original, &root).unwrap();
    assert!(remove_retired(&root, &f.0, &snapshot).is_err());
    assert_eq!(
        fs::read(root.join(TOOL)).unwrap(),
        b"user change after preflight"
    );
    assert!(root.join(PAYLOAD[0].0).exists());
}
