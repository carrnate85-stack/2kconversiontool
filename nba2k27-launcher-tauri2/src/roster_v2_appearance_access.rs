//! Retained read authority for the separate in-process appearance CAS. This
//! lease never writes a shared word with WriteProcessMemory.
use super::*;
use crate::{
    blacktop_arena::{Inspect, Region as InspectRegion},
    roster_v2_appearance as appearance,
};
struct RetainedAppearance {
    reader: Reader,
    query: QueryHandle,
    parent: QueryHandle,
    binding: Binding,
    owner: writer::Owner,
    forward_stop: Arc<AtomicBool>,
    forward_started: Instant,
}
impl RetainedAppearance {
    fn open(access: &Access, owner: writer::Owner) -> Result<Self> {
        access.checkpoint()?;
        access.finish()?;
        ensure!(
            owner.kind == Kind::Player
                && owner.game == access.binding.game
                && owner.session == access.binding.session,
            "Appearance owner differs from launch"
        );
        let reader = Reader::open_observer(&owner.game, &access.binding.root)?;
        let query = QueryHandle(unsafe { OpenProcess(RIGHTS, false, owner.game.pid)? });
        handle_matches(query.0, &owner.game)?;
        let parent = QueryHandle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                access.binding.parent.pid,
            )?
        });
        handle_matches(parent.0, &access.binding.parent)?;
        let value = Self {
            reader,
            query,
            parent,
            binding: access.binding.clone(),
            owner,
            forward_stop: access.stop.clone(),
            forward_started: Instant::now(),
        };
        value.retained()?;
        Ok(value)
    }
    fn retained_handles(&self) -> Result<()> {
        ensure!(
            self.forward_started.elapsed() < Duration::from_secs(30),
            "Retained appearance recovery deadline expired"
        );
        for (handle, identity) in [
            (self.query.0, &self.binding.game),
            (self.parent.0, &self.binding.parent),
        ] {
            let (mut c, mut e, mut k, mut u) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            unsafe {
                GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u)?;
            }
            ensure!(
                e.dwLowDateTime == 0
                    && e.dwHighDateTime == 0
                    && identity.started
                        == ((u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime))
                            .to_string(),
                "Retained appearance process ended or changed"
            );
        }
        Ok(())
    }
    fn forward_retained(&self) -> Result<()> {
        ensure!(
            !self.forward_stop.load(Ordering::Acquire) && self.forward_started.elapsed() < BUDGET,
            "Appearance save canceled or timed out"
        );
        self.retained_handles()
    }
    fn retained(&self) -> Result<()> {
        self.retained_handles()?;
        handle_matches(self.query.0, &self.binding.game)?;
        guard(&session_safety::processes()?, &self.binding)?;
        let current = Reader::open_observer(&self.binding.game, &self.binding.root)?;
        ensure!(
            current.base() == self.reader.base() && self.reader.base() == self.owner.image,
            "Appearance image changed"
        );
        Ok(())
    }
    fn query(&self, at: u64) -> Result<Region> {
        ensure!(
            (0x10000..=0x7fff_ffff_ffff).contains(&at),
            "Invalid retained appearance query"
        );
        self.retained_handles()?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let done = unsafe {
            VirtualQueryEx(
                self.query.0,
                Some(at as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            done == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Retained appearance query failed"
        );
        Ok(Region {
            base: info.BaseAddress as u64,
            allocation: info.AllocationBase as u64,
            len: info.RegionSize as u64,
            state: info.State.0,
            kind: info.Type.0,
            protection: info.Protect.0,
        })
    }
}
impl Memory for RetainedAppearance {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn checkpoint(&self) -> Result<()> {
        self.retained_handles()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.checkpoint()?;
        ensure!(
            (1..=4096).contains(&len)
                && at >= 0x10000
                && at
                    .checked_add(len as u64)
                    .is_some_and(|end| end <= 0x7fff_ffff_ffff),
            "Unbounded retained appearance read"
        );
        let r = self.query(at)?;
        let allowed = if r.kind == MEM_IMAGE.0 {
            r.allocation == self.base() && matches!(r.protection, 2 | 4 | 8 | 0x20)
        } else {
            private(r)
        };
        ensure!(
            allowed && contains(r.base, r.len, at, len),
            "Retained appearance source is not readable owned data"
        );
        let bytes = self.reader.read(at, len)?;
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl Inspect for RetainedAppearance {
    fn region(&self, at: u64) -> Result<InspectRegion> {
        let r = self.query(at)?;
        Ok(InspectRegion {
            start: r.base,
            len: r.len,
            allocation: r.allocation,
            kind: r.kind,
            protection: r.protection,
            committed: r.state == MEM_COMMIT.0,
        })
    }
}

struct AppearanceLease<'a> {
    retained: &'a RetainedAppearance,
    generation: appearance::Generation,
    operation: Option<&'a appearance::native::Operation>,
    journal: Option<&'a appearance::journal::Durable>,
}
impl Memory for AppearanceLease<'_> {
    fn base(&self) -> u64 {
        self.retained.base()
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.retained.read(at, len)
    }
    fn checkpoint(&self) -> Result<()> {
        self.retained.checkpoint()
    }
}
impl Inspect for AppearanceLease<'_> {
    fn region(&self, at: u64) -> Result<InspectRegion> {
        Inspect::region(self.retained, at)
    }
}
impl appearance::Access for AppearanceLease<'_> {
    fn generation(&self) -> Result<appearance::Generation> {
        Ok(self.generation.clone())
    }
    fn other_appearance_pointers(&self) -> Result<Vec<u64>> {
        appearance::native::person_pointers(self, &self.retained.owner)
    }
    fn native_metadata(
        &self,
        generation: &appearance::Generation,
        at: u64,
    ) -> Result<Option<[u8; 32]>> {
        Ok(Some(appearance::native::metadata(self, generation, at)?))
    }
    fn retained_check(&self) -> Result<()> {
        self.retained.retained_handles()
    }
    fn forward_check(&self) -> Result<()> {
        self.retained.forward_retained()?;
        self.retained.retained()
    }
    fn compare_exchange_masked(
        &self,
        permit: &appearance::Permit<'_>,
    ) -> Result<appearance::CasOutcome> {
        let operation = self
            .operation
            .context("Appearance native operation was not admitted")?;
        let seal = self
            .journal
            .context("Appearance durable journal was not admitted")?
            .current_seal()?;
        self.retained.retained()?;
        operation.compare(permit, &seal, &|| {
            if permit.is_undo() {
                self.retained_check()
            } else {
                self.retained.forward_retained()
            }
        })
    }
}
fn inverse(record: &writer::Plan) -> writer::Plan {
    writer::Plan {
        kind: record.kind,
        index: record.index,
        before: record.after.clone(),
        after: record.before.clone(),
        patches: record
            .patches
            .iter()
            .rev()
            .map(|p| writer::Patch {
                offset: p.offset,
                before: p.after.clone(),
                after: p.before.clone(),
                mask: p.mask.clone(),
            })
            .collect(),
        guards: record.guards.clone(),
        changed: record.changed.clone(),
    }
}
fn restore_record(
    lease: &WriteLease<'_>,
    owner: &writer::Owner,
    record: &writer::Plan,
    root: &Path,
    journal: &appearance::journal::Durable,
) -> Result<()> {
    // Forward cancellation does not revoke this exact retained, conditional undo.
    // It still has its own bounded reader and every original source dependency.
    let retained = Access::open(&lease.access.binding, Arc::new(AtomicBool::new(false)))?;
    let mut reverse = WriteLease::open(&retained, lease.state, Kind::Player, record.index)?;
    reverse.origin_dependency = lease.origin_dependency.clone();
    reverse.guards = lease.guards.clone();
    ensure!(
        writer::Access::capture(&reverse)? == record.after,
        "Player changed before composite rollback"
    );
    writer::apply(&reverse, owner, &inverse(record), |intent| {
        journal.ordinary(intent)?;
        writer::save(root, intent)
    })?;
    ensure!(
        writer::Access::capture(&reverse)? == record.before,
        "Composite original player was not restored"
    );
    Ok(())
}
pub(super) fn commit(
    root: &Path,
    lease: &WriteLease<'_>,
    owner: &writer::Owner,
    record: &writer::Plan,
    height: Option<f64>,
    wingspan: Option<f64>,
) -> Result<writer::Commit> {
    use appearance::Access as _;
    let retained = RetainedAppearance::open(lease.access, owner.clone())?;
    let generation = appearance::native::generation(&retained, owner)?;
    let probe = AppearanceLease {
        retained: &retained,
        generation: generation.clone(),
        operation: None,
        journal: None,
    };
    let snapshot = appearance::Snapshot::capture(&probe, &generation)?;
    let plan = appearance::Plan::new(&snapshot, height, wingspan)?;
    let current = snapshot.cm();
    if [height, wingspan]
        .iter()
        .enumerate()
        .all(|(i, v)| v.is_none_or(|v| (v * 100.0).round() == (current[i] * 100.0).round()))
    {
        return writer::apply(lease, owner, record, |intent| writer::save(root, intent));
    }
    let journal = appearance::journal::Durable::begin(root, owner, record, &snapshot, &plan)?;
    let operation = appearance::native::Operation::new(root, owner, &journal.id()?)?;
    let native = AppearanceLease {
        retained: &retained,
        generation,
        operation: Some(&operation),
        journal: Some(&journal),
    };
    if let Err(error) = operation.probe(&snapshot, &plan, &|| retained.forward_retained()) {
        let _ = journal.held();
        return Err(error.context("Native appearance probe uncertain; recovery remains held"));
    }
    let ordinary = match writer::apply(lease, owner, record, |intent| {
        journal.ordinary(intent)?;
        writer::save(root, intent)
    }) {
        Ok(value) => value,
        Err(error) => {
            if writer::Access::capture(lease).is_ok_and(|row| row == record.before)
                && appearance::Snapshot::capture(&probe, &probe.generation)
                    .is_ok_and(|s| s == snapshot)
            {
                let _ = journal.restored();
            } else {
                let _ = journal.held();
            }
            return Err(error);
        }
    };
    let initial_snapshot = snapshot.clone();
    let restored = || -> Result<()> {
        restore_record(lease, owner, record, root, &journal)?;
        ensure!(
            appearance::Snapshot::capture(&probe, &probe.generation)? == initial_snapshot,
            "Appearance source changed before composite recovery completed"
        );
        journal.restored()
    };
    let rebased = (|| -> Result<_> {
        let snapshot = snapshot.after_record(&native, record)?;
        let plan = plan.after_record(&snapshot)?;
        Ok((snapshot, plan))
    })();
    let (snapshot, plan) = match rebased {
        Ok(v) => v,
        Err(error) => {
            if restored().is_err() {
                let _ = journal.held();
            }
            return Err(error);
        }
    };
    let mut transaction = appearance::Transaction::new(snapshot.clone(), plan.clone())?;
    let mut phases = journal.clone();
    if let Err(error) = transaction.apply(&native, &mut phases) {
        let appearance_restored = transaction.phase() == appearance::Phase::Restored
            || transaction.rollback(&native, &mut phases).is_ok();
        if !appearance_restored || restored().is_err() {
            let _ = journal.held();
        }
        return Err(
            error.context("Composite save failed; uncertain outcomes remain held without replay")
        );
    }
    let seal = match journal.verified(&snapshot, &plan) {
        Ok(v) => v,
        Err(error) => {
            if transaction.rollback(&native, &mut phases).is_err() || restored().is_err() {
                let _ = journal.held();
            }
            return Err(error);
        }
    };
    // A lost commit acknowledgement can mean native ownership was consumed. Never
    // guess that it is still safe to reverse either half of the composite.
    if let Err(error) = native.forward_check() {
        if transaction.rollback(&native, &mut phases).is_err() || restored().is_err() {
            let _ = journal.held();
        }
        return Err(error);
    }
    if let Err(error) = operation.commit(&snapshot, &plan, &seal, &|| retained.forward_retained()) {
        let _ = journal.held();
        return Err(
            error.context("Appearance commit acknowledgement uncertain; recovery remains held")
        );
    }
    ensure!(
        writer::Access::capture(lease)? == record.after,
        "Player changed before appearance completion"
    );
    snapshot.verify_after(&native, &plan)?;
    journal.complete().context("Both changes were verified; completion evidence failed. Close the game before clearing recovery")?;
    let mut report = ordinary.report;
    report["appearanceReadBackVerified"] = json!(true);
    report["appearanceAtomicMaskedCas"] = json!(true);
    report["noChanges"] = json!(false);
    Ok(writer::Commit {
        report,
        read_back: ordinary.read_back,
    })
}

pub(super) fn details(
    access: &Access,
    state: &profile::OwnerSnapshot,
    index: usize,
    value: &mut Value,
) -> Result<()> {
    let row = read_record(
        access,
        record_address(state, &state.players, Kind::Player, index)?,
        profile::PLAYER_STRIDE,
    )?;
    let owner = writer::Owner {
        game: access.binding.game.clone(),
        session: access.binding.session.clone(),
        image: access.base(),
        root: state.root,
        owner: state.owner,
        region_base: state.region.base,
        region_len: state.region.len,
        kind: Kind::Player,
        table: state.players.pointer,
        count: state.players.count,
        index,
        identity: writer::identity(Kind::Player, &row)?,
        proof: writer::proof_tag().into(),
    };
    let retained = RetainedAppearance::open(access, owner.clone())?;
    let generation = appearance::native::generation(&retained, &owner)?;
    let probe = AppearanceLease {
        retained: &retained,
        generation: generation.clone(),
        operation: None,
        journal: None,
    };
    let snapshot = appearance::Snapshot::capture(&probe, &generation)?;
    let cm = snapshot.cm();
    value["heightCm"] = json!(cm[0]);
    value["wingspanCm"] = json!(cm[1]);
    value["editCapabilities"]["heightCm"] = json!(true);
    value["editCapabilities"]["wingspanCm"] = json!(true);
    Ok(())
}
