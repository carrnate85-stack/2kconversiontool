use super::*;
use crate::{arena_session::Access as _, ball_session::Access as _, blacktop_arena::Inspect};
use std::cell::{Cell, RefCell};
enum Data {
    Arena(crate::arena_v2::tests::Fake),
    Ball(crate::ball_v2::tests::Fake),
}
struct Fake {
    data: RefCell<Data>,
    receipt: RefCell<Vec<u8>>,
    receipt_file: Option<std::path::PathBuf>,
    manifest: Vec<u8>,
    id: Identity,
    writes: Cell<usize>,
    waits: Cell<usize>,
    parent: Cell<bool>,
    protected: Cell<bool>,
    busy_on_wait: Cell<bool>,
    partial: Cell<bool>,
    clock: Cell<u64>,
    fast_samples: Cell<bool>,
}
fn selected_ball() -> &'static str {
    &ball::choices()
        .iter()
        .find(|c| !ball::current(&c.id))
        .unwrap()
        .id
}
impl Fake {
    fn arena(id: &str) -> Self {
        let f = Self::new(
            Data::Arena(crate::arena_v2::tests::fixture()),
            b"levels/arena_brickleyfacility_int.iff\nlevels/arena_brickleyfacility_int_floor.iff\n"
                .to_vec(),
        );
        let p = arena_v2::prepare(&f, id, &arena::STOCK, Some(&f.manifest)).unwrap();
        *f.receipt.borrow_mut() = serde_json::to_vec(&ArenaIntent {
            schema: "nba2k27-native-arena-transaction-v1".into(),
            session: f.session().into(),
            game: f.id.clone(),
            image: f.base(),
            selected: id.into(),
            mapping: p.target_mapping,
            record: p.snapshot.record,
            replacement: p.template,
        })
        .unwrap();
        f
    }
    fn ball() -> Self {
        let f = Self::new(
            Data::Ball(crate::ball_v2::tests::fixture()),
            format!("{}\n", selected_ball()).into_bytes(),
        );
        let p = ball_v2::prepare(
            &f,
            &ball_v2::capture(&f).unwrap(),
            selected_ball(),
            &f.manifest,
        )
        .unwrap();
        *f.receipt.borrow_mut() = serde_json::to_vec(&BallIntent {
            schema: "nba2k27-native-ball-transaction-v1".into(),
            session: f.session().into(),
            game: f.id.clone(),
            image: f.base(),
            selected: selected_ball().into(),
            source: p.source.model,
            lease: p.original.lease,
            replacement: p.replacement,
        })
        .unwrap();
        f
    }
    fn new(data: Data, manifest: Vec<u8>) -> Self {
        Self {
            data: RefCell::new(data),
            receipt: RefCell::new(Vec::new()),
            receipt_file: None,
            manifest,
            id: Identity {
                pid: 123,
                started: "133000000000000000".into(),
                path: r"C:\Fixture\NBA2K27.exe".into(),
            },
            writes: Cell::new(0),
            waits: Cell::new(0),
            parent: Cell::new(true),
            protected: Cell::new(false),
            busy_on_wait: Cell::new(false),
            partial: Cell::new(false),
            clock: Cell::new(0),
            fast_samples: Cell::new(false),
        }
    }
    fn patch(&self, at: u64, bytes: &[u8]) {
        match &mut *self.data.borrow_mut() {
            Data::Arena(f) => f.patch(at, bytes),
            Data::Ball(f) => f.patch(at, bytes),
        }
    }
    fn menu(&self, mode: u32, stage: u32) {
        self.patch(self.base() + 0xAD07408 + 64, &mode.to_le_bytes());
        self.patch(self.base() + 0xAF46870 + 0xEA8, &stage.to_le_bytes());
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        match &*self.data.borrow() {
            Data::Arena(f) => f.base(),
            Data::Ball(f) => f.base(),
        }
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        match &*self.data.borrow() {
            Data::Arena(f) => f.read(at, len),
            Data::Ball(f) => f.read(at, len),
        }
    }
}
impl Inspect for Fake {
    fn region(&self, at: u64) -> Result<arena::Region> {
        match &*self.data.borrow() {
            Data::Arena(f) => f.region(at),
            Data::Ball(f) => f.region(at),
        }
    }
}
impl Host for Fake {
    fn kind(&self) -> Kind {
        match &*self.data.borrow() {
            Data::Arena(_) => Kind::Arena,
            Data::Ball(_) => Kind::Ball,
        }
    }
    fn identity(&self) -> &Identity {
        &self.id
    }
    fn session(&self) -> &str {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }
    fn guard(&self, parent_required: bool) -> Result<()> {
        ensure!(!self.protected.get(), "Protected process present");
        ensure!(
            !parent_required || self.parent.get(),
            "Request parent exited"
        );
        Ok(())
    }
    fn receipt(&self) -> Result<Vec<u8>> {
        if let Some(path) = &self.receipt_file {
            return Ok(std::fs::read(path)?);
        }
        Ok(self.receipt.borrow().clone())
    }
    fn manifest(&self) -> Result<Vec<u8>> {
        Ok(self.manifest.clone())
    }
    fn next_sample(&self) -> Result<()> {
        self.waits.set(self.waits.get() + 1);
        if !self.fast_samples.get() {
            self.clock.set(self.clock.get() + 100);
        }
        if self.busy_on_wait.get() {
            self.patch(
                self.base() + ball_v2::REFERENCES + 0x18,
                &1u32.to_le_bytes(),
            );
        }
        Ok(())
    }
    fn clock_ms(&self) -> Result<u64> {
        Ok(self.clock.get())
    }
    fn publish(
        &self,
        p: &Permit,
        before: &[u8],
        after: &[u8],
        parent_required: bool,
        verify: impl Fn() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            p.valid(self.base()) && p.kind() == self.kind(),
            "Fake publication permit escaped capability"
        );
        self.guard(parent_required)?;
        verify()?;
        assert_eq!(self.read(p.at(), p.len())?, before);
        self.guard(parent_required)?;
        verify()?;
        self.writes.set(self.writes.get() + 1);
        if self.partial.get() {
            self.patch(p.at(), &after[..1]);
            anyhow::bail!("Synthetic uncertain partial write");
        }
        self.patch(p.at(), after);
        ensure!(
            self.read(p.at(), p.len())? == after,
            "Fake readback uncertain"
        );
        Ok(())
    }
}

struct SessionFiles(std::path::PathBuf);
impl SessionFiles {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("v2-asset-session-{}", crate::backend::unique_id()));
        std::fs::create_dir_all(root.join("Data")).unwrap();
        std::fs::create_dir(root.join("Library")).unwrap();
        std::fs::write(root.join("Library/keep"), b"preserve library").unwrap();
        Self(root)
    }
    fn host(&self, mut host: Fake) -> Fake {
        host.receipt_file = Some(self.0.join("Data").join(host.kind().marker()));
        host
    }
}
impl Drop for SessionFiles {
    fn drop(&mut self) {
        assert!(
            self.0
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("v2-asset-session-")
        );
        assert_eq!(
            std::fs::read(self.0.join("Library/keep")).unwrap(),
            b"preserve library"
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn v2_ball_actual_engine_receipt_and_runner_rearm_through_quick_play_main_and_owner_close() {
    let files = SessionFiles::new();
    let access = BallAccess {
        host: files.host(Fake::ball()),
    };
    let mut runner =
        crate::ball_runtime::Runner::new(&files.0, access.host.session(), access.host.identity())
            .unwrap();
    let queue = crate::ball_runtime::Queue::new();
    let original = access.capture().unwrap();
    assert!(
        original.can_prepare(),
        "Version 2 context must reach the actual Engine without forging root_tag"
    );
    queue.request(selected_ball(), || Ok(())).unwrap();
    runner.tick(&queue, &access, true).unwrap();
    runner.tick(&queue, &access, true).unwrap();
    assert_eq!(queue.view().unwrap().phase, "ready");
    assert_eq!(access.host.writes.get(), 1);
    assert!(files.0.join("Data/blacktop-ball-session.json").is_file());
    access.host.menu(0xB4D37B22, 1);
    access.host.patch(
        access.host.base() + ball_v2::REFERENCES + 0x18,
        &1u32.to_le_bytes(),
    );
    runner.tick(&queue, &access, true).unwrap();
    assert_eq!(
        access.host.writes.get(),
        1,
        "No filename restoration while a ball is loaded"
    );
    access.host.menu(0x079964C0, 1);
    access.host.patch(
        access.host.base() + ball_v2::REFERENCES + 0x18,
        &0u32.to_le_bytes(),
    );
    runner.tick(&queue, &access, true).unwrap();
    assert_eq!(queue.view().unwrap().phase, "restored");
    assert_eq!(access.host.writes.get(), 2);
    assert!(!files.0.join("Data/blacktop-ball-session.json").exists());
    for menu in [0x6509837B, 0x079964C0] {
        access.host.menu(0x92DF06DE, 3);
        runner.tick(&queue, &access, true).unwrap();
        runner.tick(&queue, &access, true).unwrap();
        assert_eq!(queue.view().unwrap().phase, "ready");
        access.host.menu(menu, 1);
        runner.tick(&queue, &access, true).unwrap();
        assert_eq!(queue.view().unwrap().phase, "restored");
    }
    access.host.menu(0x92DF06DE, 3);
    runner.tick(&queue, &access, true).unwrap();
    runner.tick(&queue, &access, true).unwrap();
    access.host.parent.set(false);
    runner.tick(&queue, &access, false).unwrap();
    assert_eq!(runner.engine.selected(), "current");
    assert_eq!(queue.view().unwrap().requested_id, "current");
    assert_eq!(access.capture().unwrap().record, original.record);
    runner.engine.game_ended(|| Ok(())).unwrap();
}

#[test]
#[cfg(feature = "dev-model-preview")]
fn v2_custom_arena_rechecks_roster_refresh_without_new_selection_request() {
    use crate::{custom_blacktop as custom, provider_assets::Service};
    use std::{fs, sync::{Arc, atomic::AtomicBool}};
    let files = SessionFiles::new();
    fs::create_dir(files.0.join("levels")).unwrap();
    fs::write(files.0.join("levels/custom.iff"), b"fixture").unwrap();
    let entry = custom::request(&files.0, &files.0, &serde_json::json!({"action":"add","name":"Adapter custom","baseId":"blacktop-stock","arenaSource":"levels/custom.iff","floorSource":null})).unwrap();
    let choice = entry["id"].as_str().unwrap();
    let assets = Service::open(&files.0, &[], &[], Arc::new(AtomicBool::new(false))).unwrap();
    custom::publish(&files.0, &files.0, &assets, Some(choice)).unwrap();
    let access = ArenaAccess { host: files.host(Fake::arena(choice)), profiled: Default::default() };
    let before = access.capture(&arena::choice(choice).unwrap(), &arena::STOCK).unwrap().record.unwrap();
    let mut runner = crate::arena_runtime::Runner::new(&files.0, access.host.session(), access.host.identity()).unwrap();
    let queue = crate::arena_runtime::Queue::new(); queue.request(choice, || Ok(())).unwrap();
    runner.tick(&queue, &access, Some("selection"), true).unwrap();
    assert_eq!(access.host.writes.get(), 2);
    access.host.patch(before.address + 0x7E, &before.fields);
    runner.tick(&queue, &access, Some("match"), true).unwrap();
    assert_eq!(access.host.writes.get(), 2, "never refresh filenames in a match");
    runner.tick(&queue, &access, Some("selection"), true).unwrap();
    assert_eq!(queue.view().unwrap().phase, "ready");
    assert_eq!(access.host.writes.get(), 5);
    // Another refresh during the same selection visit is caught as well.
    access.host.patch(before.address + 0x7E, &before.fields);
    runner.tick(&queue, &access, Some("selection"), true).unwrap();
    assert_eq!(access.host.writes.get(), 8);
    runner.tick(&queue, &access, Some("selection"), true).unwrap();
    assert_eq!(access.host.writes.get(), 8, "unchanged fields are read-only");
    access.host.patch(before.address + 0x7E, b"x");
    runner.tick(&queue, &access, Some("selection"), true).unwrap();
    assert_eq!(access.host.writes.get(), 8, "foreign filename changes must never be overwritten");
    assert_ne!(queue.view().unwrap().phase, "ready");
    runner.engine.game_ended(|| Ok(())).unwrap();
}
#[test]
fn v2_arena_actual_engine_receipt_preserves_mapping_across_menus_and_restores_at_selection() {
    let files = SessionFiles::new();
    let choice = "arena_brickleyfacility_int";
    let access = ArenaAccess {
        host: files.host(Fake::arena(choice)),
        profiled: Default::default(),
    };
    let mut runner =
        crate::arena_runtime::Runner::new(&files.0, access.host.session(), access.host.identity())
            .unwrap();
    let queue = crate::arena_runtime::Queue::new();
    queue.request(choice, || Ok(())).unwrap();
    for _ in 0..2 {
        runner
            .tick(&queue, &access, Some(&access.boundary().unwrap()), true)
            .unwrap();
    }
    assert_eq!(queue.view().unwrap().phase, "ready");
    assert_eq!(access.host.writes.get(), 2);
    for menu in [0x079964C0, 0x6509837B] {
        access.host.menu(menu, 1);
        runner
            .tick(&queue, &access, Some(&access.boundary().unwrap()), true)
            .unwrap();
        access.host.menu(0x92DF06DE, 3);
        for _ in 0..2 {
            runner
                .tick(&queue, &access, Some(&access.boundary().unwrap()), true)
                .unwrap();
        }
        assert_eq!(queue.view().unwrap().phase, "ready");
        assert_eq!(
            access.host.writes.get(),
            2,
            "An owned arena is revalidated, not republished each match"
        );
    }
    queue.request("default", || Ok(())).unwrap();
    for _ in 0..2 {
        runner
            .tick(&queue, &access, Some(&access.boundary().unwrap()), true)
            .unwrap();
    }
    assert_eq!(runner.engine.selected(), "default");
    assert_eq!(access.host.writes.get(), 4);
    assert!(!files.0.join("Data/blacktop-arena-session.json").exists());
    runner.engine.game_ended(|| Ok(())).unwrap();
}

#[test]
fn v2_ball_actual_engine_defers_only_before_publication_and_holds_uncertain_writes() {
    let files = SessionFiles::new();
    let access = BallAccess {
        host: files.host(Fake::ball()),
    };
    let mut engine =
        crate::ball_session::Engine::new(&files.0, access.host.session(), access.host.identity())
            .unwrap();
    access.host.busy_on_wait.set(true);
    let error = engine.apply(&access, selected_ball()).unwrap_err();
    assert!(error.is::<crate::ball_session::Deferred>());
    assert_eq!(access.host.writes.get(), 0);
    assert!(engine.halted().is_none());
    assert!(!files.0.join("Data/blacktop-ball-session.json").exists());
    access.host.busy_on_wait.set(false);
    access.host.patch(
        access.host.base() + ball_v2::REFERENCES + 0x18,
        &0u32.to_le_bytes(),
    );
    access.host.partial.set(true);
    assert!(engine.apply(&access, selected_ball()).is_err());
    assert!(engine.halted().is_some());
    assert_eq!(access.host.writes.get(), 1);
    assert!(files.0.join("Data/blacktop-ball-session.json").exists());
    assert!(engine.apply(&access, selected_ball()).is_err());
    assert_eq!(access.host.writes.get(), 1);
    engine.game_ended(|| Ok(())).unwrap();
    assert!(!files.0.join("Data/blacktop-ball-session.json").exists());
}
#[test]
fn v2_adapter_arena_real_access_trait_orders_exact_template_then_mapping_then_reverse() {
    let access = ArenaAccess {
        host: Fake::arena("arena_brickleyfacility_int"),
        profiled: Default::default(),
    };
    let intent = ArenaIntent::read(&access.host).unwrap();
    let r = intent.record.as_ref().unwrap();
    let next = intent.replacement.as_ref().unwrap();
    access
        .assets(&arena::choice(&intent.selected).unwrap())
        .unwrap();
    access.template(r, &r.fields, next).unwrap();
    access.mapping(&arena::STOCK, &intent.mapping).unwrap();
    assert_eq!(access.host.writes.get(), 2);
    access.host.menu(0x079964C0, 1);
    assert!(access.mapping(&intent.mapping, &arena::STOCK).is_err());
    access.host.menu(0x92DF06DE, 3);
    access.mapping(&intent.mapping, &arena::STOCK).unwrap();
    access.template(r, next, &r.fields).unwrap();
    assert_eq!(access.host.writes.get(), 4);
    access.verify(r, &r.fields).unwrap();
    assert_eq!(
        access.host.receipt.borrow().len(),
        serde_json::to_vec(&intent).unwrap().len()
    );
}
#[test]
fn v2_adapter_ball_real_access_trait_preserves_context_and_four_samples_before_filename_only_write()
{
    let access = BallAccess { host: Fake::ball() };
    let intent = BallIntent::read(&access.host).unwrap();
    let observed = access.capture().unwrap();
    assert_eq!(observed.context.root_tag, access.host.base() + 0x0791E158);
    assert_eq!(
        access.resolve(&observed, selected_ball()).unwrap().0,
        intent.source
    );
    access
        .write(&intent.lease, &intent.replacement, false)
        .unwrap();
    assert_eq!(access.host.waits.get(), 3);
    assert_eq!(access.host.writes.get(), 1);
    access.host.menu(0xB4D37B22, 1);
    access.host.patch(
        access.host.base() + ball_v2::REFERENCES + 0x18,
        &1u32.to_le_bytes(),
    );
    let error = access
        .write(&intent.lease, &intent.replacement, true)
        .unwrap_err();
    assert!(error.is::<crate::ball_session::Deferred>());
    assert_eq!(access.host.writes.get(), 1);
    access.host.menu(0x079964C0, 1);
    access.host.patch(
        access.host.base() + ball_v2::REFERENCES + 0x18,
        &0u32.to_le_bytes(),
    );
    access.host.parent.set(false);
    access
        .write(&intent.lease, &intent.replacement, true)
        .unwrap();
    assert_eq!(access.host.writes.get(), 2);
    assert_eq!(
        access
            .host
            .read(intent.lease.address, ball::STRIDE)
            .unwrap(),
        intent.lease.original
    );
}
#[test]
fn v2_adapter_missing_wrong_birth_session_or_changed_intent_prevents_all_publication() {
    for mutation in 0..6 {
        let access = BallAccess { host: Fake::ball() };
        let intent = BallIntent::read(&access.host).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&access.host.receipt.borrow()).unwrap();
        match mutation {
            0 => *access.host.receipt.borrow_mut() = Vec::new(),
            1 => value["game"]["started"] = serde_json::json!("133000000000000001"),
            2 => value["session"] = serde_json::json!("b".repeat(48)),
            3 => value["source"] = serde_json::json!(ball::NORMAL),
            4 => value["selected"] = serde_json::json!("current"),
            _ => value["replacement"][0] = serde_json::json!(99),
        }
        if mutation > 0 {
            *access.host.receipt.borrow_mut() = serde_json::to_vec(&value).unwrap();
        }
        let error = access
            .write(&intent.lease, &intent.replacement, false)
            .unwrap_err();
        assert!(!error.is::<crate::ball_session::Deferred>());
        assert_eq!(
            access.host.writes.get(),
            0,
            "Published altered intent {mutation}"
        );
    }
}
#[test]
fn v2_adapter_protected_owner_exit_busy_transition_and_uncertain_write_are_distinct() {
    for mutation in 0..5 {
        let access = BallAccess { host: Fake::ball() };
        let intent = BallIntent::read(&access.host).unwrap();
        match mutation {
            0 => access.host.protected.set(true),
            1 => access.host.parent.set(false),
            2 => access.host.busy_on_wait.set(true),
            3 => access.host.partial.set(true),
            _ => access.host.fast_samples.set(true),
        }
        let error = access
            .write(&intent.lease, &intent.replacement, false)
            .unwrap_err();
        assert_eq!(
            error.is::<crate::ball_session::Deferred>(),
            mutation == 2 || mutation == 4
        );
        assert_eq!(access.host.writes.get(), usize::from(mutation == 3));
        assert!(!access.host.receipt.borrow().is_empty());
    }
}
#[test]
fn v2_adapter_mapping_and_template_require_matching_durable_receipt_and_fixed_spans() {
    let access = ArenaAccess {
        host: Fake::arena("arena_594_int"),
        profiled: Default::default(),
    };
    let intent = ArenaIntent::read(&access.host).unwrap();
    assert!(
        access
            .mapping(&arena::STOCK, &arena::mapping(Some(595)).unwrap())
            .is_err()
    );
    assert!(
        access
            .template(intent.record.as_ref().unwrap(), &vec![0; 78], &vec![1; 78])
            .is_err()
    );
    assert_eq!(access.host.writes.get(), 0);
    access.mapping(&arena::STOCK, &intent.mapping).unwrap();
    assert_eq!(access.host.writes.get(), 1);
    let image = Permit {
        area: Area::ArenaMapping,
        at: access.host.base() + arena_v2::MAPPING,
        len: 8,
        allocation: access.host.base(),
    };
    assert!(image.valid(access.host.base()));
    for (at, len, allocation) in [
        (image.at + 1, 8, image.allocation),
        (image.at, 20, image.allocation),
        (image.at, 8, 0x60000000000),
    ] {
        assert!(
            !Permit {
                area: Area::ArenaMapping,
                at,
                len,
                allocation
            }
            .valid(access.host.base())
        );
    }
}
#[test]
fn v2_adapter_exact_protection_state_machine_restores_after_each_fault_without_hiding_both_errors()
{
    struct Operation {
        old: u32,
        fail_write: bool,
        fail_restore: bool,
        writes: usize,
        restores: Vec<u32>,
    }
    impl ProtectedData for Operation {
        fn make_writable(&mut self) -> Result<u32> {
            Ok(self.old)
        }
        fn write_verify(&mut self) -> Result<()> {
            self.writes += 1;
            ensure!(!self.fail_write, "write failure");
            Ok(())
        }
        fn restore(&mut self, old: u32) -> Result<()> {
            self.restores.push(old);
            ensure!(!self.fail_restore, "restore failure");
            Ok(())
        }
    }
    for old in [2, 4, 0x20] {
        for write in [false, true] {
            for restore in [false, true] {
                let mut op = Operation {
                    old,
                    fail_write: write,
                    fail_restore: restore,
                    writes: 0,
                    restores: Vec::new(),
                };
                let result = protected_write(&mut op);
                assert_eq!(result.is_ok(), old == 2 && !write && !restore);
                assert_eq!(op.restores, vec![old]);
                assert_eq!(op.writes, usize::from(old == 2));
                if write && restore && old == 2 {
                    let msg = format!("{:#}", result.unwrap_err());
                    assert!(msg.contains("write failure") && msg.contains("restore failure"));
                }
            }
        }
    }
}
