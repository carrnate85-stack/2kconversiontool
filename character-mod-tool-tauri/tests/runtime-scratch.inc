use super::super::{PreparationLock, install_prepared, preparation_lock};
use super::*;
use sha2::Digest;
use std::os::windows::fs::OpenOptionsExt;

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TOOL: &str = "Backend/blender_tools/NBA_Character_HeadSwap/__init__.py";
const MEMBERS: [(&str, &[u8]); 3] = [
    (
        "Backend/prototype_bridge.py",
        b"bridge marker, never executed",
    ),
    (
        "Backend/python_runtime/python.exe",
        b"python marker, never executed",
    ),
    (TOOL, b"original bundled tool"),
];

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cmt-runtime-scratch-{}", stamp()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Runtime")).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }

    fn lock(&self) -> PreparationLock {
        preparation_lock(&self.0.join("Runtime")).unwrap()
    }

    fn files() -> BTreeMap<String, FileRecord> {
        MEMBERS
            .iter()
            .map(|(name, bytes)| {
                (
                    (*name).into(),
                    FileRecord {
                        bytes: bytes.len() as u64,
                        sha256: format!("{:x}", sha2::Sha256::digest(bytes)),
                    },
                )
            })
            .collect()
    }

    fn scratch(&self, lock: &PreparationLock, complete: bool) -> PathBuf {
        let plan = ScratchPlan::new(HASH, Self::files(), BTreeSet::new()).unwrap();
        let scratch = create_scratch(lock, plan).unwrap();
        for (index, (name, bytes)) in MEMBERS.iter().enumerate() {
            if !complete && index != 0 {
                continue;
            }
            let path = scratch.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        if complete {
            write_inventory(&scratch, HASH, Self::files()).unwrap();
            fs::write(scratch.join("ready.sha256"), HASH).unwrap();
        }
        scratch
    }

    fn retry(&self, lock: &PreparationLock) -> Vec<String> {
        let mut messages = Vec::new();
        retry_scratch(lock, &self.0.join("settings.json"), &mut |message| {
            messages.push(message.to_owned())
        });
        messages
    }

    fn journal(&self, path: &Path) -> PathBuf {
        scratch_journal(
            &self.0.join("Runtime"),
            path.file_name().unwrap().to_str().unwrap(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let expected = std::env::temp_dir().join(self.0.file_name().unwrap());
        assert_eq!(fs::canonicalize(expected).unwrap(), self.0);
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn scratch_crash_between_entries_is_recovered_on_next_startup() {
    let f = Fixture::new();
    let lock = f.lock();
    let scratch = f.scratch(&lock, false);
    let current = lock.cache.join("engine-current");
    fs::create_dir(&current).unwrap();
    fs::write(current.join("keep"), b"working tools").unwrap();
    drop(lock); // Abrupt owner exit: durable journal and only one extracted entry.
    let lock = f.lock();
    let messages = f.retry(&lock);
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("Removed verified"))
    );
    assert!(!scratch.exists() && !f.journal(&scratch).exists());
    assert_eq!(fs::read(current.join("keep")).unwrap(), b"working tools");
}

#[test]
fn scratch_abrupt_process_exit_releases_lock_and_recovery_retries() {
    use std::{os::windows::process::CommandExt, process::Command};
    const ENVIRONMENT: &str = "CMT_RUNTIME_SCRATCH_CRASH_FIXTURE";
    if let Some(root) = std::env::var_os(ENVIRONMENT) {
        let root = PathBuf::from(root);
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cmt-runtime-scratch-")
        );
        assert_eq!(
            fs::canonicalize(std::env::temp_dir().join(root.file_name().unwrap())).unwrap(),
            root
        );
        let f = Fixture(root);
        let lock = f.lock();
        f.scratch(&lock, false);
        std::process::exit(91); // No Rust destructors or normal scratch cleanup.
    }
    let f = Fixture::new();
    let test_name = format!(
        "{}::scratch_abrupt_process_exit_releases_lock_and_recovery_retries",
        module_path!().split_once("::").unwrap().1
    );
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &test_name, "--nocapture"])
        .env(ENVIRONMENT, &f.0)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(91), "{output:?}");
    let lock = f.lock();
    let scratch = fs::read_dir(&lock.cache)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".preparing-")
        })
        .unwrap();
    assert_eq!(fs::read(scratch.join(MEMBERS[0].0)).unwrap(), MEMBERS[0].1);
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.starts_with("Removed verified"))
    );
    assert!(!scratch.exists() && !f.journal(&scratch).exists());
}

#[test]
fn scratch_interrupted_deletion_and_orphan_journal_are_retried() {
    let f = Fixture::new();
    let lock = f.lock();
    let scratch = f.scratch(&lock, true);
    fs::remove_file(scratch.join(MEMBERS[0].0)).unwrap();
    fs::remove_file(scratch.join("ready.sha256")).unwrap();
    f.retry(&lock);
    assert!(!scratch.exists() && !f.journal(&scratch).exists());
    let orphan = create_scratch(
        &lock,
        ScratchPlan::new(HASH, Fixture::files(), BTreeSet::new()).unwrap(),
    )
    .unwrap();
    fs::remove_dir(&orphan).unwrap();
    f.retry(&lock);
    assert!(!f.journal(&orphan).exists());
}

#[test]
fn scratch_foreign_files_and_empty_directories_prevent_all_deletion() {
    let f = Fixture::new();
    let lock = f.lock();
    for directory in [false, true] {
        let scratch = f.scratch(&lock, true);
        let foreign = scratch.join("user-work");
        if directory {
            fs::create_dir(&foreign).unwrap();
        } else {
            fs::write(&foreign, b"user export").unwrap();
        }
        assert!(
            f.retry(&lock)
                .iter()
                .any(|message| message.contains("unknown"))
        );
        for (name, bytes) in MEMBERS {
            assert_eq!(fs::read(scratch.join(name)).unwrap(), bytes);
        }
        assert!(foreign.exists() && f.journal(&scratch).exists());
    }
}

#[test]
fn scratch_modified_blender_tool_and_truncated_file_are_preserved() {
    let f = Fixture::new();
    let lock = f.lock();
    for bytes in [b"modified bundled tool".as_slice(), b"partial".as_slice()] {
        let scratch = f.scratch(&lock, true);
        fs::write(scratch.join(TOOL), bytes).unwrap();
        let messages = f.retry(&lock);
        assert!(messages.iter().any(|message| message.contains("changed")));
        assert_eq!(fs::read(scratch.join(TOOL)).unwrap(), bytes);
        assert!(scratch.join("Backend/prototype_bridge.py").exists());
    }
}

#[test]
fn scratch_old_malformed_and_modified_metadata_are_reported_not_adopted() {
    let f = Fixture::new();
    let lock = f.lock();
    let old = lock.cache.join(".preparing-4000000000-123");
    let broken = lock.cache.join(".broken-4000000000-124");
    for path in [&old, &broken] {
        fs::create_dir(path).unwrap();
        fs::write(path.join("keep"), b"unverified").unwrap();
    }
    let malformed = f.scratch(&lock, true);
    fs::write(f.journal(&malformed), b"{incomplete").unwrap();
    let metadata = f.scratch(&lock, true);
    fs::write(metadata.join("ready.sha256"), "b".repeat(64)).unwrap();
    let messages = f.retry(&lock);
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.starts_with("Preserved"))
            .count(),
        4
    );
    for path in [&old, &broken, &malformed, &metadata] {
        assert!(path.is_dir());
    }
}

#[test]
fn scratch_active_file_and_preparation_locks_prevent_cleanup() {
    let f = Fixture::new();
    let lock = f.lock();
    let scratch = f.scratch(&lock, true);
    assert!(
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(lock.cache.join(".prepare.lock"))
            .is_err()
    );
    let active = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(scratch.join(TOOL))
        .unwrap();
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.starts_with("Preserved"))
    );
    assert!(scratch.join("Backend/prototype_bridge.py").exists());
    drop(active);
    f.retry(&lock);
    assert!(!scratch.exists());
}

#[test]
fn scratch_delete_failure_keeps_journal_and_retries_verified_remainder() {
    let f = Fixture::new();
    let lock = f.lock();
    let scratch = f.scratch(&lock, true);
    let tool = scratch.join(TOOL);
    let mut permissions = fs::metadata(&tool).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&tool, permissions).unwrap();
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.starts_with("Preserved"))
    );
    assert!(f.journal(&scratch).exists() && tool.exists());
    let mut permissions = fs::metadata(&tool).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&tool, permissions).unwrap();
    f.retry(&lock);
    assert!(!scratch.exists() && !f.journal(&scratch).exists());
}

#[test]
fn scratch_settings_reference_and_invalid_settings_prevent_cleanup() {
    let f = Fixture::new();
    let lock = f.lock();
    let scratch = f.scratch(&lock, true);
    let settings = f.0.join("settings.json");
    fs::write(&settings, serde_json::to_vec(&serde_json::json!({"head_swap_tool": scratch.join("Backend/blender_tools/NBA_Character_HeadSwap")})).unwrap()).unwrap();
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.contains("referenced"))
    );
    fs::write(&settings, b"invalid").unwrap();
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.contains("cleanup unavailable"))
    );
    assert!(scratch.join(TOOL).exists());
    fs::remove_file(settings).unwrap();
    f.retry(&lock);
    assert!(!scratch.exists());
}

#[test]
fn scratch_nested_and_root_links_are_preserved_without_following_targets() {
    use std::{os::windows::process::CommandExt, process::Command};
    let f = Fixture::new();
    let lock = f.lock();
    let outside = f.0.join("external-tools");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), b"external sentinel").unwrap();
    for root_link in [false, true] {
        let scratch = f.scratch(&lock, false);
        let link = if root_link {
            fs::remove_file(scratch.join(MEMBERS[0].0)).unwrap();
            fs::remove_dir(scratch.join("Backend")).unwrap();
            fs::remove_dir(&scratch).unwrap();
            scratch.clone()
        } else {
            scratch.join("Backend/linked-tools")
        };
        let output = Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .creation_flags(0x08000000)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            f.retry(&lock)
                .iter()
                .any(|message| message.starts_with("Preserved"))
        );
        assert_eq!(
            fs::read(outside.join("keep")).unwrap(),
            b"external sentinel"
        );
        fs::remove_dir(link).unwrap();
        f.retry(&lock);
        assert!(!scratch.exists());
    }
}

#[test]
fn scratch_broken_backup_only_cleans_matching_unmodified_inventory() {
    let f = Fixture::new();
    let lock = f.lock();
    for modified in [false, true] {
        let old = f.scratch(&lock, true);
        let engine = lock.cache.join(format!("engine-{}", &HASH[..16]));
        fs::rename(&old, &engine).unwrap();
        f.retry(&lock); // Finish the old successful installation's journal.
        if modified {
            fs::write(engine.join(TOOL), b"user edited Blender tool").unwrap();
        }
        let new = f.scratch(&lock, true);
        install_prepared(&lock, &new, &engine).unwrap();
        let backup = fs::read_dir(&lock.cache)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.is_dir()
                    && path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".broken-")
            })
            .unwrap();
        let messages = f.retry(&lock);
        assert_eq!(backup.exists(), modified);
        if modified {
            assert_eq!(
                fs::read(backup.join(TOOL)).unwrap(),
                b"user edited Blender tool"
            );
            assert!(
                messages
                    .iter()
                    .any(|message| message.contains("checksum changed")
                        || message.contains("size changed"))
            );
        }
        for (name, bytes) in MEMBERS {
            assert_eq!(fs::read(engine.join(name)).unwrap(), bytes);
        }
        fs::remove_dir_all(&engine).unwrap();
    }
}

#[test]
fn scratch_bad_plan_paths_and_inconsistent_metadata_are_rejected() {
    let f = Fixture::new();
    let lock = f.lock();
    for name in [
        "Backend/../outside",
        "Backend/C:ads",
        "Backend/linked\\escape",
        "Backend/ends.",
        "Backend/NUL",
        "Backend/COM1.py",
    ] {
        let mut files = Fixture::files();
        files.insert(
            name.into(),
            FileRecord {
                bytes: 0,
                sha256: HASH.into(),
            },
        );
        assert!(ScratchPlan::new(HASH, files, BTreeSet::new()).is_err());
    }
    let scratch = f.scratch(&lock, true);
    let journal = f.journal(&scratch);
    let mut plan: ScratchPlan = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    plan.files.get_mut(TOOL).unwrap().sha256 = "b".repeat(64);
    fs::write(journal, serde_json::to_vec(&plan).unwrap()).unwrap();
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.contains("ownership journal changed"))
    );
    assert!(scratch.join(TOOL).exists());
}

#[test]
fn scratch_failed_install_restores_previous_runtime_and_cleanup_retries() {
    let f = Fixture::new();
    let lock = f.lock();
    let old = f.scratch(&lock, true);
    let engine = lock.cache.join(format!("engine-{}", &HASH[..16]));
    fs::rename(&old, &engine).unwrap();
    f.retry(&lock);
    let new = f.scratch(&lock, true);
    let active = pinned(&new).unwrap();
    assert!(install_prepared(&lock, &new, &engine).is_err());
    assert_eq!(fs::read(engine.join(TOOL)).unwrap(), MEMBERS[2].1);
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.starts_with("Preserved"))
    );
    assert!(new.exists() && f.journal(&new).exists());
    drop(active);
    f.retry(&lock);
    assert!(!new.exists() && !f.journal(&new).exists());
    assert!(engine.join(TOOL).exists());
}

#[test]
fn scratch_self_consistent_journal_edits_cannot_redefine_owned_payload() {
    let f = Fixture::new();
    let lock = f.lock();
    let scratch = f.scratch(&lock, false);
    let journal = f.journal(&scratch);
    let original: ScratchPlan = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    let mut files = Fixture::files();
    files.insert(
        "Backend/my-work.iff".into(),
        FileRecord {
            bytes: 4,
            sha256: format!("{:x}", sha2::Sha256::digest(b"work")),
        },
    );
    let mut altered = ScratchPlan::new(HASH, files, BTreeSet::new()).unwrap();
    altered.directory = original.directory;
    fs::write(scratch.join("Backend/my-work.iff"), b"work").unwrap();
    fs::write(journal, serde_json::to_vec(&altered).unwrap()).unwrap();
    assert!(
        f.retry(&lock)
            .iter()
            .any(|message| message.contains("ownership journal changed"))
    );
    assert_eq!(
        fs::read(scratch.join("Backend/my-work.iff")).unwrap(),
        b"work"
    );
    assert!(scratch.join(MEMBERS[0].0).exists());
}

#[test]
fn scratch_alternate_user_streams_on_tools_and_journals_are_preserved() {
    let f = Fixture::new();
    let lock = f.lock();
    for journal in [false, true] {
        let scratch = f.scratch(&lock, true);
        let file = if journal {
            f.journal(&scratch)
        } else {
            scratch.join(TOOL)
        };
        let stream = PathBuf::from(format!("{}:user-notes", file.display()));
        fs::write(&stream, b"user work in alternate stream").unwrap();
        assert!(
            f.retry(&lock)
                .iter()
                .any(|message| message.contains("alternate data stream"))
        );
        assert_eq!(fs::read(&stream).unwrap(), b"user work in alternate stream");
        assert!(scratch.join(MEMBERS[0].0).exists());
        fs::remove_file(stream).unwrap();
        f.retry(&lock);
        assert!(!scratch.exists());
    }
}
