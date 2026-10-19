//! End-to-end V2 durable selection -> native transport -> match confirmation.
//! All memory is sparse fake data. Files exist only in new workspace fixtures.
use super::*;
use crate::outfit_journal::runtime::{Boundary, Driver as MatchDriver, Status};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;

impl MatchDriver for FakeDriver {
    fn check(&self) -> Result<()> {
        v2::verify_profile(&*self.memory.borrow())
    }
    fn boundary(&self) -> Result<Boundary> {
        self.check()?;
        let c = crate::rules_v2::context(&*self.memory.borrow())?;
        Ok(match crate::rules_v2::boundary(&c, BASE) {
            "selection" => Boundary::Selection,
            "match" => Boundary::Match,
            "main-menu" | "quick-play" => Boundary::Menu,
            _ => Boundary::Other,
        })
    }
    fn lineup(&self) -> Result<Vec<u64>> {
        crate::outfit_carry_v2::lineup(&*self.memory.borrow())
    }
    fn ready(
        &self,
        c: &[crate::blacktop_outfit_match::Choice],
    ) -> Result<Vec<crate::blacktop_outfit_match::MatchPlayer>> {
        crate::outfit_carry_v2::ready(&*self.memory.borrow(), c)
    }
}
fn seed_match(f: &mut Fake) {
    let p: serde_json::Value =
        serde_json::from_str(include_str!("../assets/outfit-match-v2-profile.json")).unwrap();
    for row in p["methods"].as_array().unwrap() {
        let b = row["hex"]
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        f.put(BASE + row["rva"].as_u64().unwrap(), &b);
    }
    for t in p["virtualTables"].as_array().unwrap() {
        for s in t["slots"].as_array().unwrap() {
            f.q(
                BASE + t["rva"].as_u64().unwrap() + s["offset"].as_u64().unwrap(),
                BASE + s["method"].as_u64().unwrap(),
            );
        }
    }
    f.d(BASE + 0xAD07408 + 64, 0xB4D37B22);
    f.d(BASE + v2::ROOT + 0xEA8, 1);
    f.put(BASE + 0xAB5D790, &[1]);
    f.ranges
        .push((BASE + crate::outfit_match_v2::TABLE, 30 * 0x50));
    let row = BASE + crate::outfit_match_v2::TABLE;
    let loaded = 0xA00000000;
    let actor = 0xB00000000;
    let primary = 0xC00000000;
    let state = 0xD00000000;
    let desc = actor + 0xF30;
    let owner = 0xE00000000;
    let array = owner + 0x1000;
    let owned = owner + 0x2000;
    f.q(row, BASE + crate::outfit_match_v2::WRAPPER);
    // Live V2 layout: the table and roster hold the manager interface base.
    f.q(row + 0x10, primary + 0x78);
    f.q(row + 0x18, state);
    f.put(row + 0x20, &[1]);
    f.q(row + 0x28, loaded);
    f.q(loaded + 0x500, ROSTER);
    f.q(loaded + 0x518, primary + 0x78);
    f.q(loaded + 0x520, state);
    f.ranges.push((primary, 0x148));
    f.q(primary, BASE + crate::outfit_match_v2::MANAGER);
    let active = BASE + crate::outfit_match_v2::ACTIVE;
    f.q(active, BASE + 0x687E1E8);
    f.q(active + 8, primary);
    f.q(active + 0x10, primary);
    f.q(primary + 8, active);
    f.q(primary + 0x10, active);
    let queue = BASE + crate::outfit_match_v2::QUEUE;
    let qhead = BASE + crate::outfit_match_v2::QUEUE_SENTINEL;
    f.q(queue, BASE + 0x68B86F8);
    f.q(qhead, BASE + 0x6860B08);
    f.q(qhead + 8, qhead);
    f.q(qhead + 0x10, qhead);
    f.put(queue + 0x5A, &[1]);
    f.d(BASE + crate::outfit_match_v2::MAIN_THREAD, 999);
    f.q(primary + 0x78, BASE + crate::outfit_match_v2::SECONDARY);
    f.q(primary + 0x110, actor);
    f.d(primary + 0x90, 1);
    let sentinel = primary + 0xF8;
    let node = primary + 0x1000;
    f.q(sentinel, BASE + crate::outfit_match_v2::SENTINEL);
    f.q(sentinel + 8, node);
    f.q(sentinel + 0x10, node);
    f.ranges.push((node, 0x40));
    f.q(node, BASE + crate::outfit_match_v2::MANAGED_INSTANCE);
    f.q(node + 8, sentinel);
    f.q(node + 0x10, sentinel);
    f.d(node + 0x18, 42);
    f.q(node + 0x20, actor);
    f.q(actor, BASE + crate::outfit_match_v2::PLAYER);
    f.ranges.push((desc, 0x2970));
    f.q(desc, BASE + crate::outfit_match_v2::PRIVATE_DESCRIPTION);
    f.q(desc + 0xB8, desc + 0x378);
    f.q(desc + 0xC8, array);
    f.d(desc + 0xD0, 2);
    f.d(desc + 0x88, 42);
    f.q(desc + 0x368, owner);
    f.q(owner, BASE + crate::outfit_match_v2::VARIABLE_OUTFIT);
    f.d(owner + 8, 2);
    f.q(owner + 0x10, array);
    f.q(owner + 0x20, owned);
    for (i, name) in ["tshirt_ss_team_016_orl", "basketball_shorts_black"]
        .iter()
        .enumerate()
    {
        let string = owner + 0x3000 + i as u64 * 512;
        f.ranges.push((string, 512));
        f.text(string, name);
        f.q(array + i as u64 * 8, string);
        f.q(owned + i as u64 * 8, string);
    }
}
fn mode(f: &mut Fake, hash: u32, stage: u32, active: u8) {
    f.d(BASE + 0xAD07408 + 64, hash);
    f.d(BASE + v2::ROOT + 0xEA8, stage);
    f.put(BASE + 0xAB5D790, &[active]);
}
fn path(root: &std::path::Path, number: u32, ext: &str) -> std::path::PathBuf {
    root.join("Logs/game-session").join(format!(
        "nba2k27_saved_outfits_{}{}.{ext}",
        game().pid,
        if number == 1 {
            String::new()
        } else {
            format!("_match{number}")
        }
    ))
}
struct Report(File);
impl Report {
    fn new(root: &std::path::Path, number: u32) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(1)
            .create_new(true)
            .open(path(root, number, "log"))?;
        let mut result = Self(file);
        result.update("ARMED", 0)?;
        Ok(result)
    }
    fn update(&mut self, state: &str, count: u32) -> Result<()> {
        self.0.seek(SeekFrom::Start(0))?;
        let text = format!(
            "SCHEMA=blacktop-saved-outfits-v1\nPID={}\nSTATE={state}\nREASON=V2 offline worker fixture\nPLAYERS_REBUILT={count}\nGAME_CODE_HOOKS=0\nOBJECT_HOOKS=0\n",
            game().pid
        );
        self.0.write_all(text.as_bytes())?;
        self.0.set_len(text.len() as u64)?;
        self.0.sync_all()?;
        Ok(())
    }
}
pub(crate) fn run() -> Result<()> {
    use crate::outfit_carry_v2::{Service as Carry, capabilities};
    let root = directory();
    fs::create_dir_all(root.join("App"))?;
    fs::create_dir_all(root.join("Library"))?;
    fs::create_dir_all(root.join("Logs/game-session"))?;
    let memory = Rc::new(RefCell::new(Fake::selection()));
    let desired = outfit(&memory.borrow());
    let mut service = Carry::create(
        &root,
        &"a".repeat(48),
        FakeDriver {
            memory: memory.clone(),
            game: game(),
            fail_publish: false,
        },
    )?;
    let binding = service.list(0)?.players[0].binding.clone();
    service.apply(&binding, &desired, 10)?;
    ensure!(
        matches!(service.tick(20)?, Status::SelectionRefreshing),
        "Refresh must complete before match intent"
    );
    memory.borrow_mut().put(CONTROLLER + 0x30, &[0]);
    ensure!(
        matches!(
            service.tick(30)?,
            Status::SelectionUpdated {
                carry_over: true,
                ..
            }
        ),
        "Confirmed choice not recorded"
    );
    ensure!(
        !path(&root, 1, "trigger").exists(),
        "Premature match trigger"
    );
    let mut report = Report::new(&root, 1)?;
    seed_match(&mut memory.borrow_mut());
    let row = BASE + crate::outfit_match_v2::TABLE;
    let choice = crate::blacktop_outfit_match::Choice {
        choice_id: "a".repeat(48),
        roster: ROSTER,
        slot: 0,
        name: "V2 fixture".into(),
        items: vec!["top".into(), "bottom".into(), "sock".into()],
    };
    ensure!(
        crate::outfit_carry_v2::ready(&*memory.borrow(), &[choice.clone()])?.len() == 1,
        "V2 readiness rejected"
    );
    let mut duplicate = memory.borrow().clone();
    duplicate.q(BASE + v2::LINEUP + 0xC8, ROSTER);
    ensure!(
        crate::outfit_carry_v2::lineup(&duplicate).is_err(),
        "Duplicate lineup accepted"
    );
    let mut alias = memory.borrow().clone();
    alias.q(row + 0x28, 0);
    ensure!(
        crate::outfit_carry_v2::ready(&alias, &[choice]).is_err(),
        "Missing loaded roster accepted"
    );
    ensure!(
        matches!(service.tick(100)?, Status::WaitingForMatch),
        "First match sample"
    );
    ensure!(
        matches!(service.tick(350)?, Status::WaitingForMatch),
        "Second match sample"
    );
    ensure!(
        matches!(
            service.tick(600)?,
            Status::Applying {
                number: 1,
                rebuilt: 0
            }
        ),
        "Selection choice lost at match handoff"
    );
    let first = fs::read(path(&root, 1, "plan"))?;
    ensure!(
        first.starts_with(&0x33504F42u32.to_le_bytes())
            && fs::read(path(&root, 1, "trigger"))? == b"SAVED_OUTFITS_V1",
        "Exact BOP3/trigger transport"
    );
    fs::remove_file(path(&root, 1, "trigger"))?;
    report.update("REBUILT", 1)?;
    ensure!(
        matches!(
            service.tick(800)?,
            Status::Applied {
                number: 1,
                players: 1
            }
        ),
        "Native confirmation not accepted"
    );
    ensure!(
        matches!(service.tick(900)?, Status::Rearming { number: 2 }),
        "Duplicate handoff was not blocked"
    );
    mode(&mut memory.borrow_mut(), 0x079964C0, 1, 0);
    service.tick(1000)?;
    ensure!(
        service.observed_boundary() == Boundary::Menu,
        "Quick Play is not a restore/rearm boundary"
    );
    mode(&mut memory.borrow_mut(), 0x92DF06DE, 3, 0);
    for now in [1500, 2000, 2500, 3000] {
        ensure!(
            matches!(service.tick(now)?, Status::Rearming { number: 2 }),
            "Worker2 absent must not arm"
        );
    }
    let mut report2 = Report::new(&root, 2)?;
    ensure!(
        matches!(service.tick(3500)?, Status::Selecting),
        "Fresh native ARMED and stable selection did not rearm"
    );
    let again = service.list(3600)?.players[0].binding.clone();
    service.apply(&again, &desired, 3610)?;
    memory.borrow_mut().put(CONTROLLER + 0x30, &[0]);
    ensure!(
        matches!(
            service.tick(3620)?,
            Status::SelectionUpdated {
                carry_over: true,
                ..
            }
        ),
        "New choice not recorded"
    );
    mode(&mut memory.borrow_mut(), 0xB4D37B22, 1, 1);
    for now in [3700, 3950] {
        ensure!(
            matches!(service.tick(now)?, Status::WaitingForMatch),
            "New match needs stability"
        );
    }
    ensure!(
        matches!(service.tick(4200)?, Status::Applying { number: 2, .. }),
        "Second unique native submission"
    );
    fs::remove_file(path(&root, 2, "trigger"))?;
    report2.update("REBUILT", 1)?;
    ensure!(
        matches!(
            service.tick(4400)?,
            Status::Applied {
                number: 2,
                players: 1
            }
        ),
        "Second match confirmation"
    );
    mode(&mut memory.borrow_mut(), 0x92DF06DE, 3, 0);
    let _report3 = Report::new(&root, 3)?;
    for now in [5000, 5500, 6000] {
        service.tick(now)?;
    }
    ensure!(
        matches!(service.tick(6500)?, Status::Selecting),
        "Third match rearm"
    );
    let restore = service.list(6600)?.players[0].binding.clone();
    service.restore(&restore, 6610)?;
    memory.borrow_mut().put(CONTROLLER + 0x30, &[0]);
    ensure!(
        matches!(
            service.tick(6620)?,
            Status::SelectionUpdated {
                carry_over: false,
                ..
            }
        ),
        "Restore created a carry choice"
    );
    mode(&mut memory.borrow_mut(), 0xB4D37B22, 1, 1);
    ensure!(
        matches!(service.tick(6700)?, Status::NoChoices) && !path(&root, 3, "plan").exists(),
        "Restored outfit was replayed"
    );
    mode(&mut memory.borrow_mut(), 0x6509837B, 1, 0);
    ensure!(
        matches!(service.tick(6800)?, Status::Menu),
        "Main menu boundary"
    );
    ensure!(
        service
            .finish_ended(|| anyhow::bail!("game still alive"))
            .is_err(),
        "Cleanup bypassed retained liveness"
    );
    service.finish_ended(|| Ok(()))?;
    ensure!(
        !crate::outfit_journal::pending(&root)?,
        "Ended journal not resolved"
    );
    let c = capabilities();
    ensure!(
        c.match_carry_over
            && !c.gameplay_verified
            && !c.automatic_randomization
            && c.minimum_carry_pieces == 3,
        "Capabilities overstated"
    );
    println!(
        "PASS V2 durable selection -> BOP3 once-only handoff -> match confirmation -> Quick Play -> rearm -> second match -> stock restore -> ended cleanup"
    );
    println!(
        "PASS V2 loaded-roster/lineup ownership and legacy gate independence; automatic randomization and gameplayVerified remain false"
    );
    lease_tests()?;
    loader_tests()?;
    crate::outfit_runtime::queue_fixture(desired)?;
    Ok(())
}
fn lease_tests() -> Result<()> {
    use crate::outfit_worker_v2_lease::{self as lease, Lease};
    let root = directory();
    fs::create_dir_all(root.join("App"))?;
    fs::create_dir_all(root.join("Library"))?;
    let target = std::path::Path::new(r"C:\Fixture\NBA2K27.exe");
    let mut worker = game();
    worker.pid = std::process::id();
    worker.path = root.join("App/qa.exe").to_string_lossy().into_owned();
    let mut parent = worker.clone();
    parent.pid += 1;
    parent.started = "134366400000000001".into();
    let mut g = game();
    g.pid = worker.pid + 2;
    let session = crate::backend::unique_id();
    let mut l = Lease::prepare(&root, &session, &worker, &parent, target, || Ok(()))?;
    ensure!(
        l.event_state()? == (false, false) && lease::pending(&root)?,
        "Private native admission defaults"
    );
    ensure!(
        Lease::prepare(&root, &session, &worker, &parent, target, || Ok(())).is_err(),
        "Preexisting events adopted"
    );
    l.bind(&g, || Ok(()))?;
    let file = root
        .join("Data")
        .join(format!("blacktop-outfit-v2-{session}.lease"));
    let body = fs::read(&file)?;
    ensure!(
        body.len() == 1632 && &body[..8] == b"2KOV2L01" && &body[48..96] == session.as_bytes(),
        "Exact native lease ABI"
    );
    ensure!(
        l.admission().is_err(),
        "Admission minted before explicit permission"
    );
    l.allow(|| Ok(()))?;
    ensure!(
        l.event_state()? == (false, true),
        "Native permission not enabled"
    );
    let admission = l.admission()?;
    admission.check(&root, &session, &g, &parent)?;
    ensure!(
        admission
            .check(&root, &"f".repeat(48), &g, &parent)
            .is_err(),
        "Admission crossed sessions"
    );
    ensure!(
        l.allow(|| anyhow::bail!("ownership lost")).is_err() && l.event_state()? == (true, false),
        "Lost owner did not cancel admission"
    );
    ensure!(
        admission.check(&root, &session, &g, &parent).is_err(),
        "Cancelled admission remained usable"
    );
    drop(admission);
    ensure!(l.allow(|| Ok(())).is_err(), "Cancelled lease rearmed");
    ensure!(
        l.finish_ended(|| anyhow::bail!("game running")).is_err() && file.exists(),
        "Live cleanup allowed"
    );
    l.finish_ended(|| Ok(()))?;
    ensure!(!lease::pending(&root)?, "Owned ended files left behind");
    drop(l);
    let session = crate::backend::unique_id();
    let mut l = Lease::prepare(&root, &session, &worker, &parent, target, || Ok(()))?;
    l.bind(&g, || Ok(()))?;
    drop(l);
    ensure!(lease::pending(&root)?, "Crash journal absent");
    let file = root
        .join("Data")
        .join(format!("blacktop-outfit-v2-{session}.lease"));
    let original = fs::read(&file)?;
    let mut altered = original.clone();
    altered[0] ^= 1;
    fs::write(&file, &altered)?;
    ensure!(
        lease::resolve_ended(&root, &session, || Ok(())).is_err() && file.exists(),
        "Foreign lease content removed"
    );
    fs::write(&file, &original)?;
    lease::resolve_ended(&root, &session, || Ok(()))?;
    ensure!(!lease::pending(&root)?, "Ended crash recovery failed");
    println!(
        "PASS actual current-user-only native events, exact lease ABI, no event adoption, cancellation, foreign-file hold and closed-game-only recovery"
    );
    Ok(())
}
fn loader_tests() -> Result<()> {
    use crate::{outfit_worker_v2_lease as lease, outfit_worker_v2_loader as loader};
    let root = directory();
    fs::create_dir_all(root.join("App"))?;
    fs::create_dir_all(root.join("Library"))?;
    fs::write(root.join("Data/settings.json"), b"{}")?;
    let game = root.join("Game");
    fs::create_dir(&game)?;
    let endpoint = format!(
        "{}{}",
        crate::provider_protocol::PIPE_PREFIX,
        "a".repeat(64)
    );
    let base = crate::loader_files::configuration(
        &endpoint,
        "loc/english.iff",
        7,
        &"B".repeat(64),
        &root,
        &[],
        false,
    )?;
    let cfg = lease::configuration(&base, &"c".repeat(48))?;
    ensure!(
        cfg.ends_with(format!("OUTFIT_V2_SESSION={}\r\n", "c".repeat(48)).as_bytes()),
        "V2 config key missing"
    );
    ensure!(
        lease::configuration(&cfg, &"c".repeat(48)).is_err(),
        "Duplicate configuration admitted"
    );
    ensure!(
        lease::configuration(b"FREESTYLE_EVENT_TEST=992\r\n", &"c".repeat(48)).is_err(),
        "Old Freestyle VM route enabled"
    );
    let freestyle = format!(
        "PIPE=x\r\nFREESTYLE_PLAYLIST=992\r\nFREESTYLE_SELECTION={}\r\n",
        "1".repeat(58) + &"0".repeat(13)
    );
    ensure!(
        lease::configuration(freestyle.as_bytes(), &"c".repeat(48))?.ends_with(
            format!("FREESTYLE_SELECTION={}\r\nOUTFIT_V2_SESSION={}\r\n", "1".repeat(58) + &"0".repeat(13), "c".repeat(48)).as_bytes()
        ),
        "V2 Freestyle playlist lines were not carried"
    );
    ensure!(
        lease::configuration(&vec![b'A'; 2047], &"c".repeat(48)).is_err(),
        "Oversized config admitted"
    );
    for n in 0..3 {
        let session = format!("{n:048x}");
        let mut s = loader::Store::create(&root, &game, &session, &cfg, || Ok(()))?;
        s.install_loader(&cfg, || Ok(()))?;
        s.trigger(|| Ok(()))?;
        ensure!(
            crate::loader_files::digest(&fs::read(game.join("version.dll"))?) == loader::PROXY_SHA,
            "Wrong payload installed"
        );
        ensure!(
            loader::pending(&root)? && s.restore(|| anyhow::bail!("Game alive")).is_err(),
            "Live payload cleanup admitted"
        );
        drop(s);
        let mut recovered = loader::Store::recover(&root, &game)?
            .context("Separate loader receipt not recovered")?;
        recovered.restore(|| Ok(()))?;
        ensure!(
            !loader::pending(&root)? && !game.join("version.dll").exists(),
            "Owned payload restore failed"
        );
    }
    fs::write(game.join("version.dll"), b"foreign")?;
    ensure!(
        loader::Store::create(&root, &game, &"d".repeat(48), &cfg, || Ok(())).is_err()
            && fs::read(game.join("version.dll"))? == b"foreign",
        "Foreign version DLL overwritten"
    );
    ensure!(
        crate::loader_files::PROXY_SHA
            == "ECEF8C6C856A60AF3387BE50D76BD30B53A80F80F404476F5C31D5A5453E91DF"
            && loader::PROXY_SHA != crate::loader_files::PROXY_SHA,
        "Global loader admission weakened"
    );
    println!(
        "PASS separate V2 payload hash/journal, three actual fresh file sessions, owned crash recovery, foreign DLL refusal and bounded config; original loader unchanged"
    );
    Ok(())
}
