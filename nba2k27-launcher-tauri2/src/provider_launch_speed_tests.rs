//! QA 0.53 launch-speed fixtures: parallel prepare ownership/cleanup proofs
//! and an ignored offline bench for the player-folder sub-steps. Fixtures live
//! under the OS temp folder only; no game, Steam or process is touched.
use super::*;
use std::sync::atomic::AtomicU64;

struct Fixture {
    base: PathBuf,
    portable: PathBuf,
    mods: PathBuf,
    session: String,
}
impl Fixture {
    fn new(tag: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("rust-launch-speed-{tag}-{}", backend::unique_id()));
        let portable = base.join("portable");
        let mods = base.join("mods");
        fs::create_dir_all(portable.join("Data")).unwrap();
        fs::create_dir(&mods).unwrap();
        Self {
            base,
            portable,
            mods,
            session: format!("{}{}", backend::unique_id(), backend::unique_id())[..48]
                .to_ascii_uppercase(),
        }
    }
    fn players(&self, folders: usize, per_folder: usize) {
        for f in 0..folders {
            let dir = self.mods.join(format!("players/Player {f:04}"));
            fs::create_dir_all(&dir).unwrap();
            for i in 0..per_folder {
                fs::write(
                    dir.join(format!("png{f:04}{i:03}.iff")),
                    [f as u8, i as u8, 7],
                )
                .unwrap();
            }
        }
    }
    fn service(&self) -> Service {
        Service::open(
            &self.mods,
            &[],
            &[],
            std::sync::Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    fn marker(&self) -> PathBuf {
        self.portable
            .join("Data")
            .join(crate::player_folders::MARKER)
    }
    fn sig_names(&self) -> Vec<String> {
        match fs::read_dir(self.mods.join("char/sig")) {
            Ok(rows) => rows
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

/// The exact restore the launch cleanup performs for players (owned Store or
/// recovery of an interrupted receipt), without any game/process gate.
fn cleanup_players(f: &Fixture, players: &mut Option<crate::player_folders::Store>) {
    if let Some(s) = players {
        s.restore(|| Ok(())).unwrap();
    } else if let Some(mut interrupted) =
        crate::player_folders::Store::recover(&f.portable, &f.mods, &f.session).unwrap()
    {
        interrupted.restore(|| Ok(())).unwrap();
    }
}

#[test]
fn side_lane_failure_stops_players_and_cleanup_leaves_no_receipt_or_link() {
    let f = Fixture::new("side-fail");
    f.players(4, 40);
    let assets = f.service();
    let stop = AtomicBool::new(false);
    let mut players = None;
    let checks = AtomicU64::new(0);
    let lanes = parallel::run(
        &stop,
        |sibling| {
            prepare_player_links(
                &assets,
                &f.portable,
                &f.mods,
                &f.session,
                &mut players,
                &|| {
                    sibling.check()?;
                    // Give the side lane time to fail mid-activation.
                    if checks.fetch_add(1, Ordering::AcqRel) > 4 {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Ok(())
                },
                &|_, _, _| {},
            )
        },
        Some(|_sibling: &parallel::Sibling| -> Result<()> {
            thread::sleep(Duration::from_millis(5));
            anyhow::bail!("fixture playlist failure")
        }),
        None::<fn(&parallel::Sibling) -> Result<()>>,
    );
    let error = lanes
        .first_error()
        .expect("a hard failure must be reported");
    assert_eq!(format!("{error:#}"), "fixture playlist failure");
    assert!(
        lanes
            .main
            .as_ref()
            .err()
            .is_none_or(parallel::is_sibling_stop),
        "players stopped only because its sibling failed"
    );
    cleanup_players(&f, &mut players);
    assert!(
        !f.marker().exists(),
        "receipt must be restored, not stranded"
    );
    assert!(
        f.sig_names().is_empty(),
        "no owned link may remain: {:?}",
        f.sig_names()
    );
    assert_eq!(
        fs::read(f.mods.join("players/Player 0000/png0000000.iff")).unwrap(),
        [0, 0, 7]
    );
}

#[test]
fn players_failure_stops_the_side_lane_and_reports_the_original_cause() {
    let f = Fixture::new("main-fail");
    f.players(2, 10);
    let assets = f.service();
    let stop = AtomicBool::new(false);
    let mut players = None;
    let side_saw_stop = AtomicBool::new(false);
    let lanes = parallel::run(
        &stop,
        |sibling| {
            prepare_player_links(
                &assets,
                &f.portable,
                &f.mods,
                &f.session,
                &mut players,
                &|| {
                    sibling.check()?;
                    if players_marker_exists(&f) {
                        anyhow::bail!("fixture game started")
                    }
                    Ok(())
                },
                &|_, _, _| {},
            )
        },
        Some(|sibling: &parallel::Sibling| -> Result<u32> {
            for _ in 0..2_000 {
                if let Err(e) = sibling.check() {
                    side_saw_stop.store(true, Ordering::Release);
                    return Err(e);
                }
                thread::sleep(Duration::from_millis(1));
            }
            Ok(1)
        }),
        None::<fn(&parallel::Sibling) -> Result<()>>,
    );
    assert!(side_saw_stop.load(Ordering::Acquire));
    let error = lanes.first_error().unwrap();
    assert_eq!(
        format!("{error:#}"),
        "Publishing the selected player-folder links: fixture game started"
    );
    // The receipt was written before the failing check: cleanup owns it.
    cleanup_players(&f, &mut players);
    assert!(!f.marker().exists());
    assert!(f.sig_names().is_empty());
}
fn players_marker_exists(f: &Fixture) -> bool {
    f.marker().exists()
}

#[test]
fn cancel_before_dispatch_stops_every_lane_and_all_owned_state_restores() {
    let f = Fixture::new("cancel");
    f.players(3, 30);
    let assets = f.service();
    let stop = AtomicBool::new(false);
    let cancelled = AtomicBool::new(false);
    let mut players = None;
    let gate = || -> Result<()> {
        ensure!(!cancelled.load(Ordering::Acquire), "Launch was cancelled");
        Ok(())
    };
    let side_journal = f.base.join("side-journal.json");
    let lanes = parallel::run(
        &stop,
        |sibling| {
            prepare_player_links(
                &assets,
                &f.portable,
                &f.mods,
                &f.session,
                &mut players,
                &|| {
                    sibling.check()?;
                    gate()?;
                    thread::sleep(Duration::from_micros(300));
                    Ok(())
                },
                &|_, _, _| {},
            )
        },
        Some(|sibling: &parallel::Sibling| -> Result<()> {
            // A side lane that owns a durable journal records it before work
            // and leaves it for the launch's recovery path on any error.
            fs::write(&side_journal, b"owned").unwrap();
            cancelled.store(true, Ordering::Release);
            loop {
                sibling.check()?;
                gate()?;
                thread::sleep(Duration::from_millis(1));
            }
        }),
        Some(|_sibling: &parallel::Sibling| -> Result<u8> { Ok(3) }),
    );
    let error = lanes.first_error().unwrap();
    assert_eq!(format!("{error:#}"), "Launch was cancelled");
    assert_eq!(lanes.third.as_ref().unwrap().as_ref().unwrap(), &3);
    cleanup_players(&f, &mut players);
    // Launch cleanup recovers the side journal (jukebox_launch::recover_ended
    // in production); this fixture only proves it survived for recovery.
    assert!(side_journal.exists());
    assert!(!f.marker().exists());
    assert!(f.sig_names().is_empty());
}

#[test]
fn successful_parallel_prepare_publishes_routes_identically_to_sequential() {
    let sequential = Fixture::new("seq");
    sequential.players(3, 12);
    let assets_a = sequential.service();
    let mut a = None;
    prepare_player_links(
        &assets_a,
        &sequential.portable,
        &sequential.mods,
        &sequential.session,
        &mut a,
        &|| Ok(()),
        &|_, _, _| {},
    )
    .unwrap();
    let parallel_fixture = Fixture::new("par");
    parallel_fixture.players(3, 12);
    let assets_b = parallel_fixture.service();
    let mut b = None;
    let stop = AtomicBool::new(false);
    let steps = Mutex::new(Vec::new());
    let lanes = parallel::run(
        &stop,
        |sibling| {
            prepare_player_links(
                &assets_b,
                &parallel_fixture.portable,
                &parallel_fixture.mods,
                &parallel_fixture.session,
                &mut b,
                &|| sibling.check(),
                &|name, _, ok| steps.lock().unwrap().push((name.to_owned(), ok)),
            )
        },
        Some(|_s: &parallel::Sibling| -> Result<()> { Ok(()) }),
        Some(|_s: &parallel::Sibling| -> Result<()> { Ok(()) }),
    );
    assert!(lanes.first_error().is_none());
    assert_eq!(
        steps.into_inner().unwrap(),
        [
            ("players-create".to_owned(), true),
            ("players-activate".to_owned(), true),
            ("players-index".to_owned(), true)
        ]
    );
    let (a, b) = (a.as_mut().unwrap(), b.as_mut().unwrap());
    assert_eq!(a.routes(), b.routes());
    assert_eq!(assets_a.counts().unwrap(), assets_b.counts().unwrap());
    for (target, _) in b.routes() {
        assert_eq!(assets_b.query(&target).unwrap().size, 3);
    }
    a.restore(|| Ok(())).unwrap();
    b.restore(|| Ok(())).unwrap();
    assert!(sequential.sig_names().is_empty() && parallel_fixture.sig_names().is_empty());
}

#[test]
fn a_panicking_side_lane_is_joined_and_reported_without_stranding_players() {
    let f = Fixture::new("panic");
    f.players(2, 8);
    let assets = f.service();
    let stop = AtomicBool::new(false);
    let mut players = None;
    let lanes = parallel::run(
        &stop,
        |sibling| {
            prepare_player_links(
                &assets,
                &f.portable,
                &f.mods,
                &f.session,
                &mut players,
                &|| {
                    thread::sleep(Duration::from_micros(200));
                    sibling.check()
                },
                &|_, _, _| {},
            )
        },
        Some(|_s: &parallel::Sibling| -> Result<()> { panic!("fixture worker panic") }),
        None::<fn(&parallel::Sibling) -> Result<()>>,
    );
    let error = format!("{:#}", lanes.first_error().unwrap());
    assert!(error.contains("stopped unexpectedly"), "{error}");
    cleanup_players(&f, &mut players);
    assert!(!f.marker().exists());
    assert!(f.sig_names().is_empty());
}

/// Ignored offline bench. Real process-scan safety checks, no game present.
/// cargo test --release --offline --locked launch_speed_player_bench -- --ignored --nocapture
#[test]
#[ignore]
fn launch_speed_player_bench() {
    let scans = 40;
    let started = Instant::now();
    for _ in 0..scans {
        let _ = session_safety::process_scan().unwrap();
    }
    let scan_ms = started.elapsed().as_secs_f64() * 1000.0 / scans as f64;
    println!("BENCH process_scan avg {scan_ms:.2} ms");
    for (folders, per) in [(25, 20), (100, 20), (200, 20)] {
        let f = Fixture::new("bench");
        f.players(folders, per);
        let assets = f.service();
        let calls = AtomicU64::new(0);
        let check = || -> Result<()> {
            calls.fetch_add(1, Ordering::AcqRel);
            let _ = session_safety::process_scan()?;
            Ok(())
        };
        let steps = Mutex::new(Vec::new());
        let mut players = None;
        let all = Instant::now();
        prepare_player_links(
            &assets,
            &f.portable,
            &f.mods,
            &f.session,
            &mut players,
            &check,
            &|name, started: Instant, ok| {
                steps.lock().unwrap().push(format!(
                    "{name}={}ms ok={ok}",
                    started.elapsed().as_millis()
                ))
            },
        )
        .unwrap();
        let total = all.elapsed().as_millis();
        let restore = Instant::now();
        players.as_mut().unwrap().restore(|| Ok(())).unwrap();
        println!(
            "BENCH files={} total={total}ms closedCalls={} steps={:?} restore(no-scan)={}ms",
            folders * per,
            calls.load(Ordering::Acquire),
            steps.into_inner().unwrap(),
            restore.elapsed().as_millis()
        );
    }
}

/// Ignored offline bench: two independent owned-file lanes (players links with
/// real per-mutation process scans, standing in for players + Jukebox) run
/// sequentially vs on parallel::run. Measures scan contention honestly.
#[test]
#[ignore]
fn launch_speed_parallel_bench() {
    let scan = || -> Result<()> {
        let _ = session_safety::process_scan()?;
        Ok(())
    };
    let run_one = |f: &Fixture, assets: &Service, check: &dyn Fn() -> Result<()>| {
        let mut players = None;
        prepare_player_links(
            assets,
            &f.portable,
            &f.mods,
            &f.session,
            &mut players,
            check,
            &|_, _, _| {},
        )
        .unwrap();
        players.as_mut().unwrap().restore(|| Ok(())).unwrap();
    };
    let (a, b) = (Fixture::new("pa"), Fixture::new("pb"));
    a.players(15, 20);
    b.players(10, 20);
    let (sa, sb) = (a.service(), b.service());
    let started = Instant::now();
    run_one(&a, &sa, &scan);
    run_one(&b, &sb, &scan);
    let sequential = started.elapsed().as_millis();
    let (c, d) = (Fixture::new("pc"), Fixture::new("pd"));
    c.players(15, 20);
    d.players(10, 20);
    let (sc, sd) = (c.service(), d.service());
    let stop = AtomicBool::new(false);
    let started = Instant::now();
    let lanes = parallel::run(
        &stop,
        |sibling| {
            run_one(&c, &sc, &|| {
                sibling.check()?;
                scan()
            });
            Ok(())
        },
        Some(|sibling: &parallel::Sibling| -> Result<()> {
            run_one(&d, &sd, &|| {
                sibling.check()?;
                scan()
            });
            Ok(())
        }),
        None::<fn(&parallel::Sibling) -> Result<()>>,
    );
    assert!(lanes.first_error().is_none());
    let parallel = started.elapsed().as_millis();
    println!(
        "BENCH lanes 300+200 files: sequential={sequential}ms parallel={parallel}ms saved={}ms",
        sequential as i128 - parallel as i128
    );
}
