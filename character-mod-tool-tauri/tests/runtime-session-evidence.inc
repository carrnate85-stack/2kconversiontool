use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cmt-session-evidence-{}", stamp()));
        fs::create_dir(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }

    fn dead(&self, timestamp: u64) -> PathBuf {
        let session = self
            .0
            .join("Sessions")
            .join(format!("4000000000-{timestamp}"));
        fs::create_dir_all(&session).unwrap();
        session
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
fn nested_accessory_journal_rollback_and_unknown_blend_survive_dead_cleanup() {
    let f = Fixture::new();
    let dead = f.dead(123);
    let transaction = dead.join("extracted/png06827/staged/.cmt-accessory-png06827-test");
    fs::create_dir_all(transaction.join("rollback")).unwrap();
    fs::create_dir(transaction.join("evidence")).unwrap();
    let journal = br#"{"version":1,"owner":"cmt-accessory-install","target":"png06827.iff","state":"rolled_back","records":[]}"#;
    fs::write(transaction.join("journal.json"), journal).unwrap();
    fs::write(
        transaction.join("rollback/png06827.iff"),
        b"retained rollback",
    )
    .unwrap();
    fs::write(
        transaction.join("evidence/unknown.blend"),
        b"unrecognized user scene",
    )
    .unwrap();
    let unrelated = f.dead(124);
    fs::write(unrelated.join("cache.bin"), b"disposable session cache").unwrap();
    assert!(session_has_accessory_evidence(&dead));
    assert!(session_has_accessory_evidence(&dead.join("extracted")));
    for _ in 0..2 {
        cleanup_dead_sessions(&f.0).unwrap();
        assert_eq!(fs::read(transaction.join("journal.json")).unwrap(), journal);
        assert_eq!(
            fs::read(transaction.join("rollback/png06827.iff")).unwrap(),
            b"retained rollback"
        );
        assert_eq!(
            fs::read(transaction.join("evidence/unknown.blend")).unwrap(),
            b"unrecognized user scene"
        );
        assert!(!unrelated.exists());
        assert!(!f.0.join("Blender Recovery").exists());
    }
}

#[test]
fn incomplete_accessory_namespace_is_retained_without_trusting_journal_names() {
    let f = Fixture::new();
    let dead = f.dead(123);
    let transaction = dead.join("players/.CMT-ACCESSORY-not-a-valid-player-incomplete");
    fs::create_dir_all(&transaction).unwrap();
    fs::write(
        transaction.join("unknown.blend"),
        b"user work before journal flush",
    )
    .unwrap();
    assert!(session_has_accessory_evidence(&dead));
    cleanup_dead_sessions(&f.0).unwrap();
    assert_eq!(
        fs::read(transaction.join("unknown.blend")).unwrap(),
        b"user work before journal flush"
    );
}

#[test]
fn accessory_owner_marker_protects_renamed_transaction_folders() {
    let f = Fixture::new();
    let dead = f.dead(123);
    let renamed = dead.join("player/retained-evidence");
    fs::create_dir_all(&renamed).unwrap();
    fs::write(
        renamed.join("journal.json"),
        br#"{"owner":"cmt-accessory-install","state":"complete"}"#,
    )
    .unwrap();
    fs::write(renamed.join("unknown.blend"), b"retained scene").unwrap();
    assert!(session_has_accessory_evidence(&dead));
    cleanup_dead_sessions(&f.0).unwrap();
    assert_eq!(
        fs::read(renamed.join("unknown.blend")).unwrap(),
        b"retained scene"
    );
}

#[test]
fn unreadable_malformed_and_oversized_journals_fail_closed() {
    let f = Fixture::new();
    let malformed = f.dead(123);
    fs::write(malformed.join("journal.json"), b"{interrupted").unwrap();
    let oversized = f.dead(124);
    fs::write(oversized.join("journal.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    let unreadable = f.dead(125);
    fs::write(
        unreadable.join("journal.json"),
        br#"{"owner":"cmt-accessory-install"}"#,
    )
    .unwrap();
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(unreadable.join("journal.json"))
        .unwrap();
    for dead in [&malformed, &oversized, &unreadable] {
        assert!(session_has_accessory_evidence(dead));
    }
    cleanup_dead_sessions(&f.0).unwrap();
    for dead in [&malformed, &oversized, &unreadable] {
        assert!(dead.join("journal.json").exists());
    }
    drop(locked);
}

#[test]
fn ordinary_session_cleanup_and_blender_return_recovery_still_work() {
    let f = Fixture::new();
    let ordinary = f.dead(123);
    fs::write(
        ordinary.join("journal.json"),
        br#"{"owner":"unrelated-test-cache"}"#,
    )
    .unwrap();
    fs::write(ordinary.join("cache.bin"), b"cache").unwrap();
    assert!(!session_has_accessory_evidence(&ordinary));
    let blender = f.dead(124);
    fs::create_dir(blender.join("blender-return")).unwrap();
    fs::write(
        blender.join("blender-return/player.iff"),
        b"unapplied Blender export",
    )
    .unwrap();
    assert!(!session_has_accessory_evidence(&blender));
    cleanup_dead_sessions(&f.0).unwrap();
    assert!(!ordinary.exists() && !blender.exists());
    let recovered: Vec<_> = fs::read_dir(f.0.join("Blender Recovery"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "iff"))
        .collect();
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        fs::read(&recovered[0]).unwrap(),
        b"unapplied Blender export"
    );
}

#[test]
fn evidence_guard_is_conservative_for_files_missing_paths_and_scan_limits() {
    let f = Fixture::new();
    let marker_file = f.0.join(".cmt-accessory-unverified");
    fs::write(&marker_file, b"unverified entry").unwrap();
    assert!(session_has_accessory_evidence(&marker_file));
    let ordinary = f.0.join("cache.bin");
    fs::write(&ordinary, b"cache").unwrap();
    assert!(!session_has_accessory_evidence(&ordinary));
    assert!(session_has_accessory_evidence(&f.0.join("unavailable")));
    let dead = f.dead(123);
    let mut deep = dead.clone();
    for _ in 0..65 {
        deep = deep.join("d");
    }
    fs::create_dir_all(&deep).unwrap();
    assert!(session_has_accessory_evidence(&dead));
    cleanup_dead_sessions(&f.0).unwrap();
    assert!(deep.exists());
}

#[test]
fn reparse_subtrees_are_not_followed_and_prevent_dead_session_deletion() {
    use std::{os::windows::process::CommandExt, process::Command};
    let f = Fixture::new();
    let dead = f.dead(123);
    let external = f.0.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("unknown.blend"), b"outside user scene").unwrap();
    let link = dead.join("linked-evidence");
    let output = Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&external)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(session_has_accessory_evidence(&dead));
    assert!(session_has_accessory_evidence(&link));
    cleanup_dead_sessions(&f.0).unwrap();
    assert!(dead.exists());
    assert_eq!(
        fs::read(external.join("unknown.blend")).unwrap(),
        b"outside user scene"
    );
    fs::remove_dir(&link).unwrap();
    assert!(!session_has_accessory_evidence(&dead));
    cleanup_dead_sessions(&f.0).unwrap();
    assert!(!dead.exists() && external.join("unknown.blend").exists());
}
