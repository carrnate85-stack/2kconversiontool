use super::*;

#[test]
fn bundled_mappings_verify_and_cover_wpf_fields() {
    let b = bundle().unwrap();
    let fields = b.catalog["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 61);
    // WPF animation-preview-fields.js: 61 catalog keys plus the jump-shot settings.
    for key in [
        "ShotJumper",
        "ShotRelease",
        "ShotRelease2",
        "FreeThrow",
        "DunkPackage15",
        "ShotStepThru",
    ] {
        assert!(fields.iter().any(|f| f["key"] == key), "{key}");
    }
    assert_eq!(b.timing["choices"].as_array().unwrap().len(), 5);
    for name in MOTION_ARCHIVES
        .iter()
        .chain(&MAPPING_SOURCES)
        .chain(&["hands.iff", "character.iff"])
    {
        asset_meta(name).unwrap();
    }
    // Every catalog archive reference stays inside the verified motion set.
    for f in fields {
        for c in f["choices"].as_array().unwrap() {
            for v in Engine::variants(c).unwrap() {
                if let Some(a) = v["archive"].as_str() {
                    let name = Path::new(a).file_name().unwrap().to_string_lossy();
                    assert!(MOTION_ARCHIVES.contains(&name.as_ref()), "{a}");
                }
            }
        }
    }
}

#[test]
fn sessions_tokens_and_owners() {
    let root = std::env::temp_dir().join(format!("c27-anim-{}", random_hex(6).unwrap()));
    let service = Service::new(&root, Arc::new(|| bail!("no game in this test")));
    let owner = "0f8f6a3c-1d2e-4b5a-9c7d-123456789abc";
    assert!(service.open("../x").is_err());
    let session = service.open(owner).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(service.progress("other-owner", &session).is_err());
    assert_eq!(
        service.progress(owner, &session).unwrap()["session"],
        session.as_str()
    );
    // Catalog needs the game folder; the error is controlled, nothing is created.
    assert!(
        service
            .catalog(owner, &session)
            .unwrap_err()
            .to_string()
            .contains("no game")
    );
    assert!(!root.exists());
    assert!(
        service
            .request(owner, &json!({"op":"export","args":{"session":session}}))
            .is_err()
    );
    service.state().unwrap().tokens.insert(
        "a".repeat(48),
        Token {
            owner: owner.into(),
            session: session.clone(),
            bytes: Arc::new(b"{}".to_vec()),
            expires: Instant::now() + TOKEN_TTL,
        },
    );
    assert!(service.payload("someone-else", &"a".repeat(48)).is_none());
    assert_eq!(
        service.payload(owner, &"a".repeat(48)).unwrap().as_slice(),
        b"{}"
    );
    assert!(service.close("someone-else", &session).is_err());
    service.close(owner, &session).unwrap();
    assert!(
        service.payload(owner, &"a".repeat(48)).is_none(),
        "closing revokes payload tokens"
    );
    assert!(service.progress(owner, &session).is_err());
    assert_eq!(service.close(owner, &session).unwrap()["closed"], true);
    for _ in 0..MAX_SESSIONS {
        service.open(owner).unwrap();
    }
    assert!(service.open(owner).is_err());
    service.shutdown();
}

fn live_root() -> Option<PathBuf> {
    std::env::var_os("C27_LIVE_GAME_DIR").map(PathBuf::from)
}

/// Rust raw extraction vs WPF's Nba2kVczDecoder: byte-identical for every
/// source the 2026-10-16 update left unchanged (sha256 from revision.json).
#[test]
#[ignore]
fn live_raw_sources_match_wpf_decoder() {
    let root = live_root().expect("C27_LIVE_GAME_DIR");
    let fit = FitAssets::default();
    let game = fit.game(&root).unwrap();
    for name in MOTION_ARCHIVES
        .iter()
        .chain(&MAPPING_SOURCES)
        .chain(&["hands.iff"])
    {
        let started = Instant::now();
        let (bytes, _) = game
            .export_animation_source(resource(name).unwrap(), &|| Ok(()))
            .unwrap();
        let only = (*name == "hands.iff").then_some(HAND_CLIP);
        let digest = member_digest(&bytes, only).unwrap();
        let verified = member_digests().unwrap()["assets"][*name].as_str().unwrap();
        println!(
            "{name}: {} bytes {} ms members {digest} verified {}",
            bytes.len(),
            started.elapsed().as_millis(),
            digest == verified
        );
        // Unchanged by the 2026-10-16 update (WPF decoder output re-checked on that game).
        if MAPPING_SOURCES.contains(name)
            || [
                "animations_pregame_single.iff",
                "animations_pregame_multi.iff",
                "animations_slamball.iff",
            ]
            .contains(name)
        {
            assert_eq!(digest, verified, "{name}");
        }
    }
}

/// Prepare the profile from the live game into C27_ANIM_LAUNCHER_ROOT and, when
/// C27_ANIM_REF holds Python reference payloads (tools/animation_preview_reference.py),
/// compare every decoded clip and the character with the Python runtime.
#[test]
#[ignore]
fn live_decode_matches_python_runtime() {
    let root = live_root().expect("C27_LIVE_GAME_DIR");
    let launcher =
        PathBuf::from(std::env::var("C27_ANIM_LAUNCHER_ROOT").expect("C27_ANIM_LAUNCHER_ROOT"));
    let game_root = root.clone();
    let service = Service::new(&launcher, Arc::new(move || Ok(game_root.clone())));
    let owner = "0f8f6a3c-1d2e-4b5a-9c7d-123456789abc";
    let session = service.open(owner).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let started = Instant::now();
    let catalog = service.catalog(owner, &session).unwrap();
    println!(
        "prepared in {} ms; revision {}",
        started.elapsed().as_millis(),
        catalog["revision"]
    );
    let engine = service.state().unwrap().engine.clone().unwrap().1;
    println!("profile {}", engine.dir.display());
    let Some(reference) = std::env::var_os("C27_ANIM_REF").map(PathBuf::from) else {
        return;
    };
    let mut compared = 0;
    let mut worst = 0f64;
    for entry in fs::read_dir(&reference).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let expected: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let request = &expected["request"];
        let actual: Value = match request["op"].as_str().unwrap() {
            "character" => serde_json::from_slice(&engine.character().unwrap()).unwrap(),
            op => {
                let raw = engine.decode(
                    &session,
                    request["field"].as_str().unwrap(),
                    request["value"].as_i64().unwrap(),
                    request["variant"].as_i64().unwrap_or(0),
                    op == "shot-part",
                );
                match raw {
                    Ok(raw) => serde_json::from_slice(&raw).unwrap(),
                    Err(error) => json!({"error": error.to_string()}),
                }
            }
        };
        let mut diff = 0f64;
        let mut problems = Vec::new();
        if expected["payload"].get("error").is_some() || actual.get("error").is_some() {
            // Both runtimes must refuse the same selections (messages differ).
            if expected["payload"].get("error").is_none() || actual.get("error").is_none() {
                problems.push(format!(
                    "refusal differs: python {} rust {}",
                    expected["payload"]["error"], actual["error"]
                ));
            }
        } else {
            compare("", &expected["payload"], &actual, &mut diff, &mut problems);
        }
        println!(
            "{}: max |diff| {diff:e} {}",
            path.file_name().unwrap().to_string_lossy(),
            problems.len()
        );
        assert!(
            problems.is_empty(),
            "{}: {:?}",
            path.display(),
            &problems[..problems.len().min(8)]
        );
        assert!(diff <= 1e-9, "{}: numeric drift {diff}", path.display());
        worst = worst.max(diff);
        compared += 1;
    }
    println!("compared {compared} payloads, worst |diff| {worst:e}");
    assert!(compared > 0);
}

/// Structural equality; numbers within 1e-9.
fn compare(
    path: &str,
    expected: &Value,
    actual: &Value,
    diff: &mut f64,
    problems: &mut Vec<String>,
) {
    // The Python runtime writes its resolved Windows path; both read the same file.
    if path == "/source_archive" {
        return;
    }
    match (expected, actual) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            *diff = diff.max((a - b).abs());
            if (a - b).abs() > 1e-9 {
                problems.push(format!("{path}: {a} != {b}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                problems.push(format!("{path}: length {} != {}", a.len(), b.len()));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                compare(&format!("{path}/{i}"), x, y, diff, problems);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()) {
                if !a.contains_key(key) || !b.contains_key(key) {
                    problems.push(format!("{path}/{key}: missing on one side"));
                }
            }
            for (key, x) in a {
                if let Some(y) = b.get(key) {
                    compare(&format!("{path}/{key}"), x, y, diff, problems);
                }
            }
        }
        (a, b) if a == b => {}
        (a, b) => problems.push(format!("{path}: {a} != {b}")),
    }
}

// Lifecycle fixtures never resolve a real game root or prepare/decode assets.
fn lifecycle_service() -> (Arc<Service>, Arc<std::sync::atomic::AtomicU64>) {
    let ticks = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let base = Instant::now();
    let clock_ticks = ticks.clone();
    let service = Service::with_clock(
        &std::env::temp_dir().join("c27-lifecycle-only"),
        Arc::new(|| bail!("no game in lifecycle fixture")),
        Arc::new(move || base + Duration::from_millis(clock_ticks.load(Ordering::SeqCst))),
        false,
    );
    (service, ticks)
}
fn fixture_engine() -> Arc<Engine> {
    Arc::new(Engine {
        profile_id: "fixture".into(),
        dir: PathBuf::new(),
        profile: json!({}),
        fields: HashMap::new(),
        library: HandLibrary {
            source_sha256: String::new(),
            poses: vec![],
        },
        stamps: vec![],
        work: Mutex::new(Work {
            archives: HashMap::new(),
            cache: VecDeque::new(),
            cache_bytes: 0,
        }),
        closed: Mutex::new(VecDeque::new()),
    })
}
const OWNER_A: &str = "11111111-1111-1111-1111-111111111111";
const OWNER_B: &str = "22222222-2222-2222-2222-222222222222";
#[test]
fn lifecycle_known_heartbeat_expires_without_reopening_or_registering() {
    let (s, t) = lifecycle_service();
    s.heartbeat(OWNER_A);
    assert!(s.state().unwrap().owners.is_empty());
    let a = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = s.open(OWNER_B).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    t.store(119_999, Ordering::SeqCst);
    s.heartbeat(OWNER_A);
    t.store(120_000, Ordering::SeqCst);
    s.maintain();
    assert!(s.progress(OWNER_B, &b).is_err());
    assert!(s.progress(OWNER_A, &a).is_ok());
    t.store(240_000, Ordering::SeqCst);
    s.heartbeat(OWNER_A);
    assert!(s.state().unwrap().owners.is_empty());
    assert!(s.progress(OWNER_A, &a).is_err());
    let replacement = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(replacement, a);
    assert!(s.progress(OWNER_A, &a).is_err());
}
#[test]
fn lifecycle_token_expiry_reaps_bytes_independently_of_live_owner() {
    let (s, t) = lifecycle_service();
    let session = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let bytes = Arc::new(vec![1, 2, 3]);
    let weak = Arc::downgrade(&bytes);
    s.insert_token(OWNER_A, &session, "a".repeat(48), bytes)
        .unwrap();
    t.store(119_000, Ordering::SeqCst);
    s.heartbeat(OWNER_A);
    assert!(weak.upgrade().is_some());
    t.store(120_000, Ordering::SeqCst);
    s.maintain();
    assert!(s.progress(OWNER_A, &session).is_ok());
    assert!(s.state().unwrap().tokens.is_empty());
    assert!(weak.upgrade().is_none());
}
#[test]
fn lifecycle_close_and_expiry_deny_late_or_foreign_token_insertion() {
    let (s, t) = lifecycle_service();
    let session = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        s.insert_token(OWNER_B, &session, "a".repeat(48), Arc::new(vec![]))
            .is_err()
    );
    s.close(OWNER_A, &session).unwrap();
    assert!(
        s.insert_token(OWNER_A, &session, "a".repeat(48), Arc::new(vec![]))
            .is_err()
    );
    assert!(s.state().unwrap().owners.is_empty());
    let session = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    t.store(120_000, Ordering::SeqCst);
    assert!(
        s.insert_token(OWNER_A, &session, "a".repeat(48), Arc::new(vec![]))
            .is_err()
    );
    assert!(s.state().unwrap().tokens.is_empty());
    s.close(OWNER_B, "unknown").unwrap();
    assert!(s.state().unwrap().owners.is_empty());
}
#[test]
fn lifecycle_sweep_revokes_every_owner_before_nonblocking_engine_bookkeeping() {
    let (s, t) = lifecycle_service();
    let engine = fixture_engine();
    let a = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let b = s.open(OWNER_B).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    s.publish_engine(PathBuf::new(), engine.clone()).unwrap();
    s.insert_token(OWNER_A, &a, "a".repeat(48), Arc::new(vec![1]))
        .unwrap();
    s.insert_token(OWNER_B, &b, "b".repeat(48), Arc::new(vec![2]))
        .unwrap();
    let work = engine.work.lock().unwrap(); // Simulates a decoder owning the work mutex.
    t.store(120_000, Ordering::SeqCst);
    let (x, y) = std::sync::mpsc::channel();
    let other = s.clone();
    let join = std::thread::spawn(move || {
        other.maintain();
        x.send(()).unwrap();
    });
    let completed = y.recv_timeout(Duration::from_secs(1)).is_ok();
    drop(work);
    join.join().unwrap();
    assert!(completed, "cleanup must not wait behind decode");
    let state = s.state().unwrap();
    assert!(
        state.sessions.is_empty()
            && state.warm.is_empty()
            && state.tokens.is_empty()
            && state.owners.is_empty()
    );
    drop(state);
    let closed = engine.closed.lock().unwrap();
    assert!(closed.contains(&a) && closed.contains(&b));
}
#[test]
fn lifecycle_idle_and_shutdown_share_publication_admission() {
    let (s, t) = lifecycle_service();
    let engine = fixture_engine();
    let a = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    s.publish_engine(PathBuf::new(), engine.clone()).unwrap();
    s.close(OWNER_A, &a).unwrap();
    t.store(299_999, Ordering::SeqCst);
    s.maintain();
    assert!(s.state().unwrap().engine.is_some());
    let b = s.open(OWNER_B).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    t.store(300_000, Ordering::SeqCst);
    s.maintain();
    assert!(
        s.state().unwrap().engine.is_some(),
        "new interest cancels old idle decision"
    );
    s.close(OWNER_B, &b).unwrap();
    t.store(600_000, Ordering::SeqCst);
    s.maintain();
    assert!(s.state().unwrap().engine.is_none());
    s.open(OWNER_A).unwrap();
    s.shutdown();
    assert!(
        s.publish_engine(PathBuf::new(), engine).is_err(),
        "late producer cannot publish after shutdown"
    );
    s.publish(json!({"phase":"ready"}));
    assert!(s.state().unwrap().engine.is_none());
}
#[test]
fn lifecycle_distinct_caps_and_empty_owner_cleanup_remain_bounded() {
    let (s, _) = lifecycle_service();
    let session = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 1..MAX_SESSIONS {
        s.open(OWNER_A).unwrap();
    }
    assert!(s.open(OWNER_B).is_err());
    s.close(OWNER_B, "unknown").unwrap();
    assert!(!s.state().unwrap().owners.contains_key(OWNER_B));
    s.close(OWNER_A, &session).unwrap();
    assert!(
        s.state().unwrap().owners.contains_key(OWNER_A),
        "other sessions still own interest"
    );
    s.shutdown();
    assert!(s.state().unwrap().owners.is_empty());
}
#[test]
fn lifecycle_repeated_warm_coalesces_while_preparation_is_blocked() {
    use std::sync::{Condvar, atomic::AtomicUsize};
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let g = gate.clone();
    let c = calls.clone();
    let s = Service::with_clock(
        &std::env::temp_dir().join("c27-coalesced-warm"),
        Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
            started_tx.send(()).unwrap();
            let (mut opened, cv) = (g.0.lock().unwrap(), &g.1);
            while !*opened {
                opened = cv.wait(opened).unwrap();
            }
            bail!("controlled preparation failure; no game read")
        }),
        Arc::new(Instant::now),
        false,
    );
    s.warm(OWNER_A, true).unwrap();
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    for _ in 0..32 {
        s.warm(OWNER_A, true).unwrap();
    }
    s.warm(OWNER_B, true).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.state().unwrap().warm.len(), 2);
    for i in 0..MAX_WARM - 2 {
        s.warm(&format!("aaaaaaaa-aaaa-aaaa-aaaa-{i:012x}"), true)
            .unwrap();
    }
    assert!(
        s.warm("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", true)
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1, "all owners share one task");
    s.shutdown();
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    let deadline = Instant::now() + Duration::from_secs(1);
    while s.state().unwrap().warm_running && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(!s.state().unwrap().warm_running);
    assert!(s.state().unwrap().engine.is_none());
}

#[test]
fn lifecycle_empty_owner_removed_only_after_last_interest_is_gone() {
    let (s, _) = lifecycle_service();
    let session = s.open(OWNER_A).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_owned();
    s.update(|state, now| {
        state.warm.insert(OWNER_A.into());
        state.owners.insert(OWNER_A.into(), now);
        Ok(())
    })
    .unwrap();
    s.close(OWNER_A, &session).unwrap();
    assert!(s.state().unwrap().owners.contains_key(OWNER_A));
    s.warm(OWNER_A, false).unwrap();
    assert!(s.state().unwrap().owners.is_empty());
    s.heartbeat(OWNER_A);
    assert!(s.state().unwrap().owners.is_empty());
}

#[test]
fn lifecycle_new_interest_after_cancelled_warm_restarts_once_without_parallel_task() {
    use std::sync::{Condvar, atomic::AtomicUsize};
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = std::sync::mpsc::channel();
    let g = gate.clone();
    let c = calls.clone();
    let service = Service::with_clock(
        &std::env::temp_dir().join("c27-warm-replacement"),
        Arc::new(move || {
            let n = c.fetch_add(1, Ordering::SeqCst);
            tx.send(n).unwrap();
            if n == 0 {
                let mut open = g.0.lock().unwrap();
                while !*open {
                    open = g.1.wait(open).unwrap();
                }
            }
            bail!("controlled no-game result")
        }),
        Arc::new(Instant::now),
        false,
    );
    service.warm(OWNER_A, true).unwrap();
    assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), 0);
    service.warm(OWNER_A, false).unwrap();
    service.warm(OWNER_B, true).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    assert_eq!(rx.recv_timeout(Duration::from_secs(1)).unwrap(), 1);
    let deadline = Instant::now() + Duration::from_secs(1);
    while service.state().unwrap().warm_running && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(!service.state().unwrap().warm_running);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(!service.state().unwrap().owners.contains_key(OWNER_A));
    service.shutdown();
}
