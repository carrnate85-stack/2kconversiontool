use super::*;
use crate::{
    blacktop_outfits::tests::{Fixture, game},
    outfit_journal::Witness,
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
const OWNER: &str = "abcdef0123456789abcdef0123456789abcdef0123456789";
#[derive(Clone)]
struct Mock {
    memory: Rc<RefCell<Fixture>>,
    events: Rc<RefCell<Vec<String>>>,
    fail: Rc<Cell<&'static str>>,
    game: Identity,
}
enum Cap {
    Existing(Ready),
    Changed(Submission),
}
struct Confirmation {
    cap: Cap,
    player: Player,
}
impl Confirmation {
    fn proof(&self) -> &Witness {
        match &self.cap {
            Cap::Existing(r) => r.witness(),
            Cap::Changed(s) => s.witness(),
        }
    }
}
impl Driver for Mock {
    type Receipt = Confirmation;
    fn identity(&self) -> &Identity {
        &self.game
    }
    fn capture(&self, pages: &[Page]) -> Result<Selection> {
        outfits::capture(&*self.memory.borrow(), &self.game, pages)
    }
    fn prepare(
        &self,
        r: Reservation,
        p: &Player,
        c: &Composed,
        pages: &[Page],
    ) -> Result<Prepared> {
        {
            let _pin = r.witness().pin()?;
            self.events
                .borrow_mut()
                .push("allocation-after-durable-reserve".into());
        }
        outfits::revalidate(&*self.memory.borrow(), &self.game, p, pages)?;
        ensure!(self.fail.get() != "prepare", "mock allocation failure");
        let mut f = self.memory.borrow_mut();
        let original_bytes = f.bytes.clone();
        let original_regions = f.regions.clone();
        let mut page = f.private(&self.game, p, c);
        // Subsequent preparations reserve a different synthetic page. Nothing
        // in this mock opens/writes a process or allocates native memory.
        let n = self
            .events
            .borrow()
            .iter()
            .filter(|e| e.as_str() == "allocation-after-durable-reserve")
            .count();
        if n > 1 {
            let mut region = *f.regions.get(&0x800000000).unwrap();
            f.bytes = original_bytes;
            f.regions = original_regions;
            page.allocation += ((n - 1) * 4096) as u64;
            let data = outfits::page_bytes(
                &self.game,
                &p.key,
                p.original_pointer,
                page.allocation,
                c.street_index,
                c.roster_shoes,
                &c.items,
            )?;
            f.put(page.allocation, &data);
            region.start = page.allocation;
            region.allocation = page.allocation;
            f.regions.insert(page.allocation, region);
            page.sha256 = crate::loader_files::digest(&data);
        }
        f.q(p.key.description + 0xc8, p.source.pointer);
        f.d(p.key.description + 0xd0, p.source.count);
        r.prepared(page)
    }
    fn unchanged(
        &self,
        r: Ready,
        p: &Player,
        _: &Composed,
        pages: &[Page],
    ) -> Result<Self::Receipt> {
        r.witness().pin()?;
        outfits::revalidate(&*self.memory.borrow(), &self.game, p, pages)?;
        self.events
            .borrow_mut()
            .push("unchanged-no-source-write".into());
        Ok(Confirmation {
            cap: Cap::Existing(r),
            player: p.clone(),
        })
    }
    fn publish(&self, s: Submission, p: &Player, pages: &[Page]) -> Result<Self::Receipt> {
        {
            let _pin = s.witness().pin()?;
            self.events
                .borrow_mut()
                .push("source-after-durable-attempt".into());
        }
        outfits::revalidate(&*self.memory.borrow(), &self.game, p, pages)?;
        let target = s.intent().target(s.page())?;
        let after = Player {
            source: target.clone(),
            items: s.intent().items.clone(),
            ..p.clone()
        };
        let mut f = self.memory.borrow_mut();
        f.q(p.key.description + 0xc8, target.pointer);
        f.d(p.key.description + 0xd0, target.count);
        f.put(p.key.controller + 0x30 + p.key.slot as u64 * 0x7b0, &[1]);
        ensure!(
            self.fail.get() != "publish",
            "mock failure after source write"
        );
        Ok(Confirmation {
            cap: Cap::Changed(s),
            player: after,
        })
    }
    fn consumed(&self, r: &Self::Receipt, pages: &[Page]) -> Result<bool> {
        r.proof().pin()?;
        ensure!(self.fail.get() != "poll", "mock failed confirmation");
        let f = self.memory.borrow();
        outfits::revalidate(&*f, &self.game, &r.player, pages)?;
        let v = outfits::dirty(&*f, &r.player.key)?;
        ensure!(v <= 1, "bad refresh");
        self.events
            .borrow_mut()
            .push("read-only-refresh-poll".into());
        Ok(v == 0)
    }
}
impl outfit_journal::runtime::Driver for Mock {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn boundary(&self) -> Result<outfit_journal::runtime::Boundary> {
        let f = self.memory.borrow();
        let c = crate::blacktop_native::context(&*f)?;
        use outfit_journal::runtime::Boundary;
        Ok(
            match crate::blacktop_native::boundary(&c, crate::blacktop_native::Memory::base(&*f)) {
                "selection" => Boundary::Selection,
                "match" => Boundary::Match,
                "main-menu" | "quick-play" => Boundary::Menu,
                _ => Boundary::Other,
            },
        )
    }
    fn lineup(&self) -> Result<Vec<u64>> {
        crate::blacktop_outfit_match::lineup(&*self.memory.borrow())
    }
    fn ready(
        &self,
        choices: &[crate::blacktop_outfit_match::Choice],
    ) -> Result<Vec<crate::blacktop_outfit_match::MatchPlayer>> {
        ensure!(
            outfit_journal::runtime::Driver::boundary(self)?
                == outfit_journal::runtime::Boundary::Match,
            "fixture match is not ready"
        );
        Ok(choices
            .iter()
            .map(|c| crate::blacktop_outfit_match::MatchPlayer {
                roster: c.roster,
                record: 0x300000000 + c.slot as u64 * 0x50,
                roster_copy: 0x400000000 + c.slot as u64 * 0x1000,
                manager: 0x500000000 + c.slot as u64 * 0x1000,
                player: 0x600000000 + c.slot as u64 * 0x1000,
                description: 0x600000f20 + c.slot as u64 * 0x1000,
                array: 0x800000000 + c.slot as u64 * 0x1000,
                items: ["stock_top".into(), "stock_bottom".into()],
            })
            .collect())
    }
}
fn setup() -> (PathBuf, Mock, Session<Mock>, backend::Outfit) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(format!("outfit-session-{}", backend::unique_id()));
    std::fs::create_dir_all(root.join("Data")).unwrap();
    let d = Mock {
        memory: Rc::new(RefCell::new(Fixture::selection())),
        events: Rc::new(RefCell::new(Vec::new())),
        fail: Rc::new(Cell::new("")),
        game: game(),
    };
    let catalog = crate::outfits::catalog().unwrap();
    let outfit = catalog
        .presets
        .iter()
        .filter(|p| p.pool.as_deref() == Some("male") && p.separable && !p.manual_only)
        .find_map(|p| {
            let o = catalog.original_outfit(&p.id, "Session fixture").ok()?;
            (catalog.compose(&catalog.selection(&o).ok()?).ok()?.len() >= 3).then_some(o)
        })
        .unwrap();
    let s = Session::create(&root, OWNER, d.clone()).unwrap();
    (root, d, s, outfit)
}
fn complete(d: &Mock, s: &Session<Mock>) {
    let f = d.capture(s.journal().pages()).unwrap();
    for p in f.players {
        d.memory
            .borrow_mut()
            .put(p.key.controller + 0x30 + p.key.slot as u64 * 0x7b0, &[0]);
    }
}
#[test]
fn apply_wait_confirm_unchanged_and_restore_are_nonblocking_and_durable() {
    let (root, d, mut s, o) = setup();
    let list = s.list(100).unwrap();
    assert_eq!(list.players.len(), 4);
    let token = list.players[0].token.clone();
    let binding = list.players[0].binding.clone();
    let json = serde_json::to_string(&list).unwrap();
    for key in [
        "roster",
        "description",
        "controller",
        "pointer",
        "groupBasis",
    ] {
        assert!(!json.contains(key));
    }
    s.apply(&token, &o, 110).unwrap();
    assert!(s.busy());
    assert!(s.journal().ledger().choices.is_empty());
    assert!(s.apply(&token, &o, 111).is_err());
    assert!(s.list(112).is_err());
    assert_eq!(s.poll(120).unwrap(), Poll::Waiting);
    assert!(s.journal().ledger().choices.is_empty());
    complete(&d, &s);
    assert!(matches!(
        s.poll(130).unwrap(),
        Poll::Confirmed {
            slot: 0,
            carry_over: true,
            ..
        }
    ));
    assert_eq!(s.journal().ledger().choices.len(), 1);
    assert_eq!(s.journal().pages().len(), 1);
    let id = s.journal().ledger().choices[0].choice_id.clone();
    assert_eq!(s.poll(140).unwrap(), Poll::Idle);
    assert!(s.apply(&token, &o, 145).is_err());
    let t = s.list(150).unwrap().players[0].token.clone();
    assert_eq!(s.list(150).unwrap().players[0].binding, binding);
    s.apply(&t, &o, 160).unwrap();
    assert!(matches!(
        s.poll(170).unwrap(),
        Poll::Confirmed {
            carry_over: true,
            ..
        }
    ));
    assert_eq!(s.journal().pages().len(), 1);
    assert_ne!(s.journal().ledger().choices[0].choice_id, id);
    let t = s.list(180).unwrap().players[0].token.clone();
    s.restore(&t, 190).unwrap();
    assert_eq!(s.poll(200).unwrap(), Poll::Waiting);
    complete(&d, &s);
    assert!(matches!(
        s.poll(210).unwrap(),
        Poll::Confirmed {
            carry_over: false,
            ..
        }
    ));
    assert!(s.journal().ledger().choices.is_empty());
    assert_eq!(
        &d.events.borrow()[..3],
        &[
            "allocation-after-durable-reserve",
            "source-after-durable-attempt",
            "read-only-refresh-poll"
        ]
    );
    assert!(
        d.events
            .borrow()
            .iter()
            .any(|e| e == "unchanged-no-source-write")
    );
    s.finish(|| Ok(())).unwrap();
    assert!(!outfit_journal::pending(&root).unwrap());
    assert!(s.list(220).is_err());
    s.finish(|| Ok(())).unwrap(); // Downstream cleanup retries do not reactivate this session.
    assert!(s.finish(|| anyhow::bail!("new game appeared")).is_err());
}
#[test]
fn tokens_expire_are_single_use_and_never_follow_a_reused_slot() {
    let (_, d, mut s, o) = setup();
    let t = s.list(10).unwrap().players[0].token.clone();
    assert!(s.apply(&t, &o, 3010).is_err());
    assert!(d.events.borrow().is_empty());
    let t = s.list(4000).unwrap().players[0].token.clone();
    assert!(s.apply(&t, &o, 3999).is_err());
    assert!(d.events.borrow().is_empty());
    let t = s.list(5000).unwrap().players[0].token.clone();
    let p = d.capture(&[]).unwrap().players.remove(0);
    d.memory
        .borrow_mut()
        .q(p.key.description + 0xb8, p.key.roster + 0x4f8);
    assert!(s.apply(&t, &o, 5010).is_err());
    assert!(d.events.borrow().is_empty());
    assert!(s.held().is_none());
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn failures_never_replay_uncertain_allocations_or_source_writes() {
    for failure in ["prepare", "publish", "poll"] {
        let (root, d, mut s, o) = setup();
        let t = s.list(100).unwrap().players[0].token.clone();
        d.fail.set(failure);
        let applied = s.apply(&t, &o, 110);
        if failure == "poll" {
            applied.unwrap();
            assert!(s.poll(120).is_err());
        } else {
            assert!(applied.is_err());
        }
        assert!(s.held().is_some());
        assert!(s.journal().ledger().choices.is_empty());
        let n = d.events.borrow().len();
        assert!(s.apply(&t, &o, 130).is_err());
        assert!(s.list(140).is_err());
        assert_eq!(d.events.borrow().len(), n);
        assert!(s.finish(|| anyhow::bail!("game alive")).is_err());
        assert!(outfit_journal::pending(&root).unwrap());
        s.finish(|| Ok(())).unwrap();
        assert!(!outfit_journal::pending(&root).unwrap());
    }
}
#[test]
fn refresh_timeout_backward_clock_and_leaving_before_confirmation_hold_the_session() {
    for mode in [0, 1, 2] {
        let (_, d, mut s, o) = setup();
        let t = s.list(100).unwrap().players[0].token.clone();
        s.apply(&t, &o, 110).unwrap();
        match mode {
            0 => {
                assert!(s.poll(3111).is_err());
            }
            1 => {
                assert!(s.poll(109).is_err());
            }
            _ => {
                assert!(s.leave_blacktop().is_err());
            }
        }
        assert!(s.held().is_some());
        assert!(s.journal().ledger().choices.is_empty());
        complete(&d, &s);
        assert!(s.poll(3200).is_err());
        s.finish(|| Ok(())).unwrap();
    }
}
#[test]
fn leaving_blacktop_clears_choices_and_tokens_but_keeps_immutable_pages_until_exit() {
    let (_, d, mut s, o) = setup();
    let t = s.list(100).unwrap().players[0].token.clone();
    s.apply(&t, &o, 110).unwrap();
    complete(&d, &s);
    s.poll(120).unwrap();
    let t = s.list(130).unwrap().players[0].token.clone();
    let old_binding = s.list(130).unwrap().players[0].binding.clone();
    s.leave_blacktop().unwrap();
    assert!(s.journal().ledger().choices.is_empty());
    assert_eq!(s.journal().pages().len(), 1);
    assert!(s.held().is_none());
    assert!(s.apply(&t, &o, 140).is_err());
    let t = s.list(150).unwrap().players[0].token.clone();
    assert_ne!(s.list(150).unwrap().players[0].binding, old_binding);
    s.apply(&t, &o, 160).unwrap();
    s.poll(170).unwrap();
    assert_eq!(s.journal().ledger().choices.len(), 1);
    assert_eq!(s.journal().pages().len(), 1);
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn multiple_players_keep_independent_pages_and_replacing_one_never_displaces_another() {
    let (_, d, mut s, o) = setup();
    let t = s.list(100).unwrap().players[0].token.clone();
    s.apply(&t, &o, 110).unwrap();
    complete(&d, &s);
    s.poll(120).unwrap();
    let t = s.list(130).unwrap().players[1].token.clone();
    s.apply(&t, &o, 140).unwrap();
    complete(&d, &s);
    s.poll(150).unwrap();
    assert_eq!(s.journal().pages().len(), 2);
    assert_eq!(s.journal().ledger().choices.len(), 2);
    let second = s
        .journal()
        .ledger()
        .choices
        .iter()
        .find(|c| c.slot == 1)
        .unwrap()
        .clone();
    let p = d.capture(s.journal().pages()).unwrap().players.remove(0);
    let catalog = crate::outfits::catalog().unwrap();
    let alternate = catalog
        .presets
        .iter()
        .filter(|v| v.separable && !v.manual_only && v.pool == p.pool)
        .find_map(|v| {
            let next = catalog.original_outfit(&v.id, "Replacement").ok()?;
            (outfits::compose(&p, &next).ok()?.items != p.items).then_some(next)
        })
        .unwrap();
    let t = s.list(160).unwrap().players[0].token.clone();
    s.apply(&t, &alternate, 170).unwrap();
    complete(&d, &s);
    s.poll(180).unwrap();
    assert_eq!(s.journal().pages().len(), 3);
    assert_eq!(s.journal().ledger().choices.len(), 2);
    assert!(s.journal().ledger().choices.contains(&second));
    assert_eq!(d.capture(s.journal().pages()).unwrap().players.len(), 4);
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn a_game_appearing_during_final_close_keeps_the_receipt_and_disables_further_requests() {
    let (root, _, mut s, _) = setup();
    let t = s.list(100).unwrap().players[0].token.clone();
    let calls = Cell::new(0);
    assert!(
        s.finish(|| {
            let n = calls.get();
            calls.set(n + 1);
            ensure!(n == 0, "game appeared");
            Ok(())
        })
        .is_err()
    );
    assert!(s.held().is_some());
    assert!(outfit_journal::pending(&root).unwrap());
    assert!(s.restore(&t, 120).is_err());
    assert!(s.list(130).is_err());
    s.finish(|| Ok(())).unwrap();
    assert!(!outfit_journal::pending(&root).unwrap());
}
#[test]
fn combined_selection_refresh_match_handoff_native_result_and_cleanup_keep_one_journal_owner() {
    use outfit_journal::runtime::Status;
    let (root, d, mut s, o) = setup();
    drop(crate::loader_files::prepare_reports(&root).unwrap());
    let prefix = root
        .join("Logs/game-session")
        .join(format!("nba2k27_saved_outfits_{}", game().pid));
    let report = |state: &str, n: u32| {
        std::fs::write(prefix.with_extension("log"),format!("SCHEMA=blacktop-saved-outfits-v1\r\nPID={}\r\nSTATE={state}\r\nREASON=Combined fixture\r\nPLAYERS_REBUILT={n}\r\nGAME_CODE_HOOKS=0\r\nOBJECT_HOOKS=0\r\n",game().pid)).unwrap()
    };
    report("ARMED", 0);
    assert_eq!(s.tick(10).unwrap(), Status::Selecting);
    let token = s.list(100).unwrap().players[0].token.clone();
    s.apply(&token, &o, 110).unwrap();
    assert_eq!(s.tick(120).unwrap(), Status::SelectionRefreshing);
    complete(&d, &s);
    assert!(matches!(
        s.tick(130).unwrap(),
        Status::SelectionUpdated {
            slot: 0,
            carry_over: true,
            ..
        }
    ));
    d.memory.borrow_mut().d(
        crate::blacktop_outfits::tests::BASE + 0xAC77660 + 64,
        0xB4D37B22,
    );
    d.memory
        .borrow_mut()
        .d(crate::blacktop_outfits::tests::BASE + 0xAEB69D0 + 0xEA8, 1);
    assert_eq!(s.tick(200).unwrap(), Status::WaitingForMatch);
    assert_eq!(s.tick(450).unwrap(), Status::WaitingForMatch);
    assert_eq!(
        s.tick(700).unwrap(),
        Status::Applying {
            number: 1,
            rebuilt: 0
        }
    );
    assert!(s.busy());
    assert!(s.list(710).is_err());
    assert!(s.finish(|| anyhow::bail!("game is alive")).is_err());
    assert!(s.busy());
    assert!(prefix.with_extension("trigger").is_file());
    std::fs::remove_file(prefix.with_extension("trigger")).unwrap();
    report("REBUILT", 1);
    assert_eq!(
        s.tick(800).unwrap(),
        Status::Applied {
            number: 1,
            players: 1
        }
    );
    assert!(!s.busy());
    assert_eq!(s.journal().ledger().consumed.len(), 1);
    s.finish(|| Ok(())).unwrap();
    assert!(!prefix.with_extension("plan").exists());
    assert!(prefix.with_extension("log").exists());
    assert!(!outfit_journal::pending(&root).unwrap());
    s.finish(|| Ok(())).unwrap();
}

#[test]
fn automatic_batch_preserves_verified_owned_choices_and_manual_override() {
    let (root, d, mut s, outfit) = setup();
    let token = s.list(100).unwrap().players[0].token.clone();
    s.apply(&token, &outfit, 110).unwrap();
    complete(&d, &s);
    s.poll(120).unwrap();
    let queued = s.journal().ledger().choices.clone();
    let pages = s.journal().pages().len();
    let effects = d.events.borrow().len();
    let journal_bytes = std::fs::metadata(root.join("Data").join(crate::outfit_journal::MARKER))
        .unwrap()
        .len();
    let token = s.list(130).unwrap().players[0].token.clone();
    assert_eq!(
        s.apply_automatic_many(&[(token, outfit.clone())], 140)
            .unwrap(),
        0
    );
    assert!(!s.busy());
    assert_eq!(s.journal().pages().len(), pages);
    assert_eq!(d.events.borrow().len(), effects);
    assert_eq!(s.journal().ledger().choices, queued);
    assert_eq!(
        std::fs::metadata(root.join("Data").join(crate::outfit_journal::MARKER))
            .unwrap()
            .len(),
        journal_bytes
    );
    // A newly selected/uncovered player still applies in the same automatic batch.
    let view = s.list(150).unwrap();
    let items = vec![
        (view.players[0].token.clone(), outfit.clone()),
        (view.players[1].token.clone(), outfit.clone()),
    ];
    assert_eq!(s.apply_automatic_many(&items, 160).unwrap(), 1);
    assert_eq!(s.journal().pages().len(), pages + 1);
    complete(&d, &s);
    s.poll(170).unwrap();
    assert_eq!(s.journal().ledger().choices.len(), 2);
    assert!(s.journal().ledger().choices.contains(&queued[0]));
    // Manual requests bypass only automatic suppression, never native guards.
    let token = s.list(180).unwrap().players[0].token.clone();
    s.apply_many(&[(token, outfit.clone())], 190).unwrap();
    assert!(s.busy());
    complete(&d, &s);
    s.poll(200).unwrap();
    assert!(!s.journal().ledger().choices.contains(&queued[0]));
    // A real visit end clears pending choices, so the next automatic fit is allowed.
    s.leave_blacktop().unwrap();
    let token = s.list(210).unwrap().players[0].token.clone();
    assert_eq!(s.apply_automatic_many(&[(token, outfit)], 220).unwrap(), 1);
    complete(&d, &s);
    s.poll(230).unwrap();
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn automatic_suppression_requires_verified_unconsumed_full_page_ownership() {
    let (_, d, mut s, outfit) = setup();
    let token = s.list(100).unwrap().players[0].token.clone();
    s.apply(&token, &outfit, 110).unwrap();
    assert!(s.journal().ledger().choices.is_empty());
    assert!(
        s.apply_automatic_many(&[(token, outfit.clone())], 111)
            .is_err(),
        "unverified refresh stays busy"
    );
    complete(&d, &s);
    s.poll(120).unwrap();
    let player = d.capture(s.journal().pages()).unwrap().players.remove(0);
    assert!(s.owns_queued_choice(&player).unwrap());
    for field in ["model", "description", "pointer", "count", "items"] {
        let mut changed = player.clone();
        match field {
            "model" => changed.key.model += 8,
            "description" => changed.key.description += 8,
            "pointer" => changed.source.pointer = changed.original_pointer,
            "count" => changed.source.count += 1,
            _ => changed.items.push("foreign_item".into()),
        }
        assert!(
            s.owns_queued_choice(&changed).is_err(),
            "cannot downgrade {field}"
        );
    }
    let frozen = s.journal().ledger().choices.clone();
    s.store.consume(&frozen).unwrap();
    assert!(
        !s.owns_queued_choice(&player).unwrap(),
        "consumed choice cannot suppress next match"
    );
    s.finish(|| Ok(())).unwrap();
}
#[test]
fn automatic_and_manual_never_bypass_an_uncertain_selection_write() {
    let (_, d, mut s, outfit) = setup();
    let token = s.list(100).unwrap().players[0].token.clone();
    d.fail.set("publish");
    assert!(s.apply(&token, &outfit, 110).is_err());
    assert!(s.held().is_some());
    let effects = d.events.borrow().len();
    assert!(
        s.apply_automatic_many(&[(token.clone(), outfit.clone())], 120)
            .is_err()
    );
    assert!(s.apply_many(&[(token, outfit)], 130).is_err());
    assert_eq!(d.events.borrow().len(), effects);
    s.finish(|| Ok(())).unwrap();
}
