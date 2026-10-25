use super::super::{cached_runtime, runtime_lease, stamp};
use super::*;
use sha2::Digest;
use std::{collections::BTreeMap, fs, sync::mpsc};

struct Fixture {
    root: PathBuf,
    tasks: Vec<Maintenance>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cmt-runtime-maintenance-{}", stamp()));
        fs::create_dir_all(root.join("Runtime")).unwrap();
        fs::write(root.join("settings.json"), b"{}").unwrap();
        Self {
            root,
            tasks: Vec::new(),
        }
    }
    fn cache(&self) -> PathBuf {
        self.root.join("Runtime")
    }
    fn runtime(&self, letter: char) -> (PathBuf, String) {
        let hash = letter.to_string().repeat(64);
        let path = self.cache().join(format!("engine-{}", &hash[..16]));
        let mut files = BTreeMap::new();
        for &name in super::super::BOOTSTRAP_FILES {
            let file = path.join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, b"fixture").unwrap();
            files.insert(
                name.into(),
                cache::FileRecord {
                    bytes: 7,
                    sha256: format!("{:x}", sha2::Sha256::digest(b"fixture")),
                },
            );
        }
        cache::write_inventory(&path, &hash, files).unwrap();
        fs::write(path.join("ready.sha256"), &hash).unwrap();
        (path, hash)
    }
    fn ready(&mut self, hash: &str) -> super::super::PreparedRuntime {
        let ready = cached_runtime(&self.cache(), hash).expect("Ready runtime was not reused");
        self.tasks.push(ready.maintenance.clone());
        ready
    }
}
fn wait(task: &Maintenance, expected: &[&str]) {
    let started = Instant::now();
    loop {
        let report = task.report();
        if expected.contains(&report["status"].as_str().unwrap()) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "Maintenance did not settle: {report}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            wait(task, &["complete", "deferred"]);
        }
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("cmt-runtime-maintenance-")
        );
        for attempt in 0..100 {
            match fs::remove_dir_all(&self.root) {
                Ok(()) => return,
                Err(_) if attempt < 99 => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => panic!("Owned fixture cleanup failed: {error}"),
            }
        }
    }
}

#[test]
fn ready_runtime_does_not_wait_for_another_preparation_lock() {
    let mut fixture = Fixture::new();
    let (_, hash) = fixture.runtime('a');
    let lock = preparation_lock(&fixture.cache()).unwrap();
    let cache = fixture.cache();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || sender.send(cached_runtime(&cache, &hash)).unwrap());
    let ready = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("Cached startup waited for maintenance")
        .unwrap();
    fixture.tasks.push(ready.maintenance.clone());
    assert_eq!(ready.maintenance_report()["status"], "waiting");
    drop(lock);
    wait(&ready.maintenance, &["complete"]);
}

#[test]
fn two_cached_readers_start_while_maintenance_waits_on_settings() {
    let mut fixture = Fixture::new();
    let (_, hash) = fixture.runtime('a');
    let settings = settings_write_guard(&fixture.root).unwrap();
    let first = fixture.ready(&hash);
    wait(&first.maintenance, &["waiting-for-settings"]);
    let second = fixture.ready(&hash);
    assert!(runtime_lease(&fixture.cache(), &hash, true).is_err());
    drop(settings);
    wait(&first.maintenance, &["complete"]);
    wait(&second.maintenance, &["complete"]);
}

#[test]
fn background_lease_survives_the_last_ready_owner_until_maintenance_finishes() {
    let mut fixture = Fixture::new();
    let (_, hash) = fixture.runtime('a');
    let settings = settings_write_guard(&fixture.root).unwrap();
    let ready = fixture.ready(&hash);
    let task = ready.maintenance.clone();
    wait(&task, &["waiting-for-settings"]);
    drop(ready);
    assert!(runtime_lease(&fixture.cache(), &hash, true).is_err());
    drop(settings);
    wait(&task, &["complete"]);
    let start = Instant::now();
    while runtime_lease(&fixture.cache(), &hash, true).is_err() {
        assert!(start.elapsed() < Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn repair_exclusion_prevents_fast_reuse() {
    let fixture = Fixture::new();
    let (_, hash) = fixture.runtime('a');
    let _repair = runtime_lease(&fixture.cache(), &hash, true).unwrap();
    assert!(cached_runtime(&fixture.cache(), &hash).is_none());
}

#[test]
fn protected_settings_reject_older_writers_and_atomic_replacement() {
    let fixture = Fixture::new();
    let path = fixture.root.join("settings.json");
    let protected = protect_settings(&fixture.root).unwrap().unwrap();
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    let replacement = fixture.root.join("settings-new.json");
    fs::write(&replacement, b"{\"new\":true}").unwrap();
    assert!(fs::rename(&replacement, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{}");
    drop(protected);
    fs::rename(&replacement, &path).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"{\"new\":true}");
}

#[test]
fn settings_save_waits_for_cleanup_protection_then_resumes() {
    let fixture = Fixture::new();
    let protected = protect_settings(&fixture.root).unwrap().unwrap();
    let root = fixture.root.clone();
    let (sender, receiver) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        let _guard = settings_write_guard(&root).unwrap();
        sender.send(()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(70)).is_err());
    drop(protected);
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    writer.join().unwrap();
}

#[test]
fn missing_settings_defer_cleanup_without_creating_or_overwriting_preferences() {
    let mut fixture = Fixture::new();
    let (_, hash) = fixture.runtime('a');
    fs::remove_file(fixture.root.join("settings.json")).unwrap();
    let ready = fixture.ready(&hash);
    wait(&ready.maintenance, &["deferred"]);
    assert!(!fixture.root.join("settings.json").exists());
    assert_eq!(ready.maintenance_report()["warnings"], false);
}

#[test]
fn invalid_settings_preserve_all_runtime_trees() {
    let mut fixture = Fixture::new();
    let (older, _) = fixture.runtime('c');
    let (_, hash) = fixture.runtime('a');
    fs::write(fixture.root.join("settings.json"), b"[]").unwrap();
    let ready = fixture.ready(&hash);
    wait(&ready.maintenance, &["deferred"]);
    assert!(older.exists());
    assert_eq!(ready.maintenance_report()["warnings"], true);
}

#[test]
fn configured_candidate_is_preserved_during_background_retention() {
    let mut fixture = Fixture::new();
    let (old, _) = fixture.runtime('c');
    std::thread::sleep(Duration::from_millis(5));
    fixture.runtime('b');
    let (_, hash) = fixture.runtime('a');
    fs::write(
        fixture.root.join("settings.json"),
        serde_json::to_vec(&json!({"output_dir":old.join("exports")})).unwrap(),
    )
    .unwrap();
    let ready = fixture.ready(&hash);
    wait(&ready.maintenance, &["complete"]);
    assert!(old.exists());
}

#[test]
fn maintenance_reports_are_bounded_and_drain_without_losing_health_history() {
    let task = Maintenance(Arc::new(Mutex::new(State {
        status: "running",
        started: Instant::now(),
        milliseconds: None,
        warnings: false,
        history: VecDeque::new(),
        pending: VecDeque::new(),
    })));
    for _ in 0..100 {
        task.message(&format!("Preserved {}", "x".repeat(10000)));
    }
    assert_eq!(task.events().len(), 16);
    assert!(task.events().is_empty());
    let report = task.report();
    assert_eq!(report["messages"].as_array().unwrap().len(), 16);
    assert_eq!(report["warnings"], true);
    assert!(
        report["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|message| message.as_str().unwrap().chars().count() <= 4096)
    );
}

#[test]
fn synchronous_scratch_cleanup_waits_for_settings_and_preserves_new_reference() {
    let fixture = Fixture::new();
    let lock = preparation_lock(&fixture.cache()).unwrap();
    let hash = "d".repeat(64);
    let mut files = BTreeMap::new();
    files.insert(
        "Backend/test.bin".into(),
        cache::FileRecord {
            bytes: 7,
            sha256: format!("{:x}", sha2::Sha256::digest(b"fixture")),
        },
    );
    let plan = cache::ScratchPlan::new(&hash, files, Default::default()).unwrap();
    let scratch = cache::create_scratch(&lock, plan).unwrap();
    fs::create_dir(scratch.join("Backend")).unwrap();
    fs::write(scratch.join("Backend/test.bin"), b"fixture").unwrap();
    let writer = settings_write_guard(&fixture.root).unwrap();
    let root = fixture.root.clone();
    let (sender, receiver) = mpsc::channel();
    let cleanup = std::thread::spawn(move || {
        let mut events = Vec::new();
        retry_scratch(&lock, &root, &mut |message| events.push(message.to_owned()));
        sender.send(events).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_millis(70)).is_err());
    fs::write(
        fixture.root.join("settings.json"),
        serde_json::to_vec(&json!({"output_dir":scratch.join("exports")})).unwrap(),
    )
    .unwrap();
    drop(writer);
    let messages = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    cleanup.join().unwrap();
    assert!(scratch.join("Backend/test.bin").is_file());
    assert!(
        messages
            .iter()
            .any(|message| message.contains("referenced by settings"))
    );
}

#[test]
fn synchronous_scratch_cleanup_without_settings_is_deferred() {
    let fixture = Fixture::new();
    let lock = preparation_lock(&fixture.cache()).unwrap();
    fs::remove_file(fixture.root.join("settings.json")).unwrap();
    let mut messages = Vec::new();
    retry_scratch(&lock, &fixture.root, &mut |message| {
        messages.push(message.to_owned())
    });
    assert!(!fixture.root.join("settings.json").exists());
    assert!(messages.iter().any(|message| message.contains("deferred")));
}
