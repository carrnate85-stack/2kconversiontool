//! V2 appearance capability/transaction policy with a separately typed native
//! atomic32 adapter. Never widen the generic record write lease.
#![allow(dead_code)]
use crate::{
    blacktop_arena::Inspect,
    blacktop_native::Memory,
    roster_v2_write::{self as writer, Kind, Owner},
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Generation {
    owner: Owner,
    stamp: [u8; 32],
    known_owner_spans: Vec<(u64, usize)>,
}
impl Generation {
    /// Only a future native adapter may mint this from retained owner inspection.
    /// `known_owner_spans` must contain ALL other directory/owner metadata spans,
    /// never a browser-provided or optional subset. Stamp identifies the same
    /// captured directory generation; cancellation is separate from retention.
    pub(crate) fn from_native(
        owner: Owner,
        stamp: [u8; 32],
        known_owner_spans: Vec<(u64, usize)>,
    ) -> Result<Self> {
        owner.validate()?;
        ensure!(
            owner.kind == Kind::Player && stamp != [0; 32] && !known_owner_spans.is_empty(),
            "Appearance generation is incomplete"
        );
        for &(at, len) in &known_owner_spans {
            span(at, len)?;
        }
        Ok(Self {
            owner,
            stamp,
            known_owner_spans,
        })
    }
}
fn span(at: u64, len: usize) -> Result<()> {
    ensure!(
        at >= 0x10000
            && len > 0
            && at
                .checked_add(len as u64)
                .is_some_and(|end| end < 0x7fff00000000),
        "Invalid appearance span"
    );
    Ok(())
}
fn overlap(a: u64, n: usize, b: u64, m: usize) -> bool {
    a < b.saturating_add(m as u64) && b < a.saturating_add(n as u64)
}
fn read(m: &impl Access, at: u64, len: usize) -> Result<Vec<u8>> {
    span(at, len)?;
    ensure!(len <= 4096, "Appearance bounded read");
    m.retained_check()?;
    let value = m.read(at, len)?;
    ensure!(value.len() == len, "Partial appearance read");
    Ok(value)
}
fn q(m: &impl Access, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(read(m, at, 8)?.as_slice().try_into()?))
}
/// All methods must derive authority from retained native identities. In
/// particular reads and retained_check must remain usable for conditional
/// rollback after cancellation; checkpoint stops only NEW forward work.
pub(crate) trait Access: Inspect {
    fn generation(&self) -> Result<Generation>;
    /// All native PERSONDATA appearance references (owner+180/+188, stride1C8,
    /// QWORD+0), from the same retained owner/header generation. Not optional:
    /// scanning only PLAYER+78 would miss appearance shared with staff.
    fn other_appearance_pointers(&self) -> Result<Vec<u64>>;
    /// Native transport additionally seals the full owner header through3D0
    /// and untouched100B of the selected104B pool row. Pure policy fixtures may
    /// omit it; the actual in-process transport refuses an absent seal.
    fn native_metadata(
        &self,
        _generation: &Generation,
        _appearance: u64,
    ) -> Result<Option<[u8; 32]>> {
        Ok(None)
    }
    fn retained_check(&self) -> Result<()>;
    /// Separate cancellation/new-operation authority. Memory::checkpoint and
    /// retained_check must prove only the retained source, including rollback.
    fn forward_check(&self) -> Result<()>;
    fn verify_native_profile(&self) -> Result<()>
    where
        Self: Sized,
    {
        verify_profile(self)
    }
    /// Future adapter must compare the exact four-byte preimage and atomically
    /// publish only Permit::mask bits. There is deliberately no raw-address write.
    fn compare_exchange_masked(&self, permit: &Permit<'_>) -> Result<CasOutcome>;
}
/// The adapter must distinguish an atomic conflict from its own successful CAS.
/// An error or matching after-image alone never supplies rollback authority.
pub(crate) enum CasOutcome {
    NoEffect,
    Applied(EffectReceipt),
    FailedAfterApplied(EffectReceipt, anyhow::Error),
    Uncertain,
}
pub(crate) struct EffectReceipt {
    origin: Snapshot,
    after: [u8; 4],
    undo: bool,
}
pub(crate) fn verify_profile(m: &impl Memory) -> Result<()> {
    writer::verify_write_profile(m)?;
    let proof: Value = serde_json::from_str(include_str!("../assets/appearance-v2-proof.json"))?;
    ensure!(
        proof["schema"] == "qa066-v2-appearance-disk-proof-v1"
            && proof["imageSha256"]
                == "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1",
        "Appearance reference mismatch"
    );
    for code in proof["nativeCodeProofs"]
        .as_array()
        .context("Missing appearance anchors")?
    {
        let rva = u64::from_str_radix(
            code["rva"]
                .as_str()
                .context("Missing appearance RVA")?
                .trim_start_matches("0x"),
            16,
        )?;
        let len = code["bytes"].as_u64().context("Missing appearance span")? as usize;
        ensure!((1..=4096).contains(&len), "Appearance anchor bounds");
        m.checkpoint()?;
        let actual = m.read(m.base() + rva, len)?;
        ensure!(
            actual.len() == len
                && format!("{:x}", Sha256::digest(&actual))
                    == code["sha256"].as_str().unwrap_or(""),
            "Appearance native anchor changed"
        );
    }
    for code in proof["setters"]
        .as_array()
        .context("Missing appearance setters")?
    {
        let rva = |key: &str| {
            u64::from_str_radix(
                code[key].as_str().unwrap_or("").trim_start_matches("0x"),
                16,
            )
        };
        let (begin, end) = (rva("begin")?, rva("end")?);
        ensure!(
            end > begin && end - begin <= 4096,
            "Appearance setter bounds"
        );
        m.checkpoint()?;
        let actual = m.read(m.base() + begin, (end - begin) as usize)?;
        ensure!(
            actual.len() as u64 == end - begin
                && format!("{:x}", Sha256::digest(actual)) == code["sha256"].as_str().unwrap_or(""),
            "Appearance full setter changed"
        );
    }
    for (instruction, target, name) in [
        (
            0x2E24B3Au64,
            0x68ED802u64,
            "APPEARANCEDATA::SetHeightInTenthsOfMM",
        ),
        (
            0x2E24C2E,
            0x68ED8E0,
            "APPEARANCEDATA::SetWingspanInTenthsOfMM",
        ),
    ] {
        m.checkpoint()?;
        let code = m.read(m.base() + instruction, 7)?;
        ensure!(
            code.len() == 7
                && code[..3] == [0x4c, 0x8d, 0x0d]
                && instruction as i64 + 7 + i32::from_le_bytes(code[3..].try_into()?) as i64
                    == target as i64
                && m.read(m.base() + target, name.len() + 1)? == [name.as_bytes(), &[0]].concat(),
            "Appearance named setter reference changed"
        );
    }
    m.checkpoint()?;
    ensure!(
        m.read(m.base() + 0x5D0E560, 4)? == 0.01f32.to_le_bytes(),
        "Appearance cm scale changed"
    );
    m.checkpoint()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    generation: Generation,
    row: Vec<u8>,
    pointers: Vec<u64>,
    other_pointers: Vec<u64>,
    native_metadata: Option<[u8; 32]>,
    appearance: u64,
    allocation: u64,
    words: [u8; 4],
}
fn pointers(m: &impl Access, g: &Generation) -> Result<Vec<u64>> {
    (0..g.owner.count)
        .map(|index| {
            q(
                m,
                g.owner.table + index as u64 * g.owner.kind.stride() as u64 + 0x78,
            )
        })
        .collect()
}
fn region(m: &impl Access, at: u64) -> Result<u64> {
    span(at, 4)?;
    let p = m.region(at)?;
    ensure!(
        p.committed
            && p.allocation >= 0x10000
            && p.kind == 0x20000
            && p.protection == 4
            && at >= p.start
            && at + 4
                <= p.start
                    .checked_add(p.len)
                    .context("Appearance region overflow")?,
        "Appearance must remain plain private RW"
    );
    Ok(p.allocation)
}
impl Snapshot {
    pub(crate) fn verify_before(&self, m: &impl Access) -> Result<()> {
        self.guard(m, &self.words)
    }
    pub(crate) fn verify_after(&self, m: &impl Access, p: &Plan) -> Result<()> {
        ensure!(p.origin == *self, "Appearance after-plan origin differs");
        self.guard(m, &p.after)
    }
    pub(crate) fn capture(m: &impl Access, expected: &Generation) -> Result<Self> {
        expected.owner.validate()?;
        ensure!(
            m.base() == expected.owner.image,
            "Appearance image binding changed"
        );
        m.verify_native_profile()?;
        m.retained_check()?;
        ensure!(
            m.generation()? == *expected,
            "Appearance source generation changed"
        );
        let row_at = expected.owner.table
            + expected.owner.index as u64 * expected.owner.kind.stride() as u64;
        let row = read(m, row_at, expected.owner.kind.stride())?;
        ensure!(
            writer::identity(Kind::Player, &row)? == expected.owner.identity,
            "Appearance row identity changed"
        );
        let peers = pointers(m, expected)?;
        let other_pointers = m.other_appearance_pointers()?;
        ensure!(
            other_pointers.len() <= 8192,
            "Appearance other-owner projection exceeds bound"
        );
        let appearance = peers[expected.owner.index];
        ensure!(
            u64::from_le_bytes(row[0x78..0x80].try_into()?) == appearance,
            "Appearance pointer changed during row capture"
        );
        ensure!(
            appearance % 4 == 0,
            "Appearance masked CAS requires4bytealignment"
        );
        span(appearance, 4)?;
        ensure!(
            !overlap(
                appearance,
                4,
                expected.owner.table,
                expected.owner.count * expected.owner.kind.stride()
            ) && expected
                .known_owner_spans
                .iter()
                .all(|(at, len)| !overlap(appearance, 4, *at, *len))
                && peers
                    .iter()
                    .enumerate()
                    .all(|(index, peer)| index == expected.owner.index
                        || *peer == 0
                        || !overlap(appearance, 4, *peer, 4)),
            "Appearance aliases another owner"
        );
        ensure!(
            other_pointers
                .iter()
                .all(|p| *p == 0 || !overlap(appearance, 4, *p, 4)),
            "Appearance aliases PERSONDATA"
        );
        let allocation = region(m, appearance)?;
        let words: [u8; 4] = read(m, appearance, 4)?.as_slice().try_into()?;
        let height = u16::from_le_bytes(words[..2].try_into()?) & 0x7fff;
        let wingspan = u16::from_le_bytes(words[2..].try_into()?) & 0x7fff;
        ensure!(
            (12000..=26000).contains(&height) && (12000..=32000).contains(&wingspan),
            "Appearance source measurements are uninitialized or implausible"
        );
        let native_metadata = m.native_metadata(expected, appearance)?;
        let snapshot = Self {
            generation: expected.clone(),
            row,
            pointers: peers,
            other_pointers,
            native_metadata,
            appearance,
            allocation,
            words,
        };
        snapshot.guard(m, &snapshot.words)?;
        Ok(snapshot)
    }
    fn guard(&self, m: &impl Access, words: &[u8; 4]) -> Result<()> {
        m.retained_check()?;
        m.verify_native_profile()?;
        let owner = &self.generation.owner;
        ensure!(
            m.generation()? == self.generation
                && region(m, self.appearance)? == self.allocation
                && pointers(m, &self.generation)? == self.pointers
                && m.other_appearance_pointers()? == self.other_pointers
                && m.native_metadata(&self.generation, self.appearance)? == self.native_metadata
                && read(
                    m,
                    owner.table + owner.index as u64 * owner.kind.stride() as u64,
                    owner.kind.stride()
                )? == self.row
                && read(m, self.appearance, 4)?.as_slice() == words.as_slice()
                && m.generation()? == self.generation
                && q(
                    m,
                    owner.table + owner.index as u64 * owner.kind.stride() as u64 + 0x78
                )? == self.appearance,
            "Appearance owner/preimage changed"
        );
        m.retained_check()
    }
    /// Typed composite handoff AFTER the ordinary record transaction verified
    /// its exact after-image. This is read-only: appearance bytes, aliases,
    /// owner generation and allocation must still be the original preimage.
    pub(crate) fn after_record(&self, m: &impl Access, record: &writer::Plan) -> Result<Self> {
        ensure!(
            record.kind == Kind::Player
                && record.index == self.generation.owner.index
                && record.before == self.row
                && record.after.len() == Kind::Player.stride()
                && record.expected(record.patches.len())? == record.after
                && writer::identity(Kind::Player, &record.after)? == self.generation.owner.identity
                && record.after[0x78..0x80] == self.row[0x78..0x80],
            "Appearance composite record handoff differs"
        );
        let mut next = self.clone();
        next.row = record.after.clone();
        next.guard(m, &self.words)?;
        Ok(next)
    }
    pub(crate) fn cm(&self) -> [f64; 2] {
        [
            u16::from_le_bytes(self.words[..2].try_into().unwrap()) & 0x7fff,
            u16::from_le_bytes(self.words[2..].try_into().unwrap()) & 0x7fff,
        ]
        .map(|v| v as f64 / 100.0)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    // A masked plan belongs to the exact captured owner/generation/preimage,
    // including aliases. It cannot be moved to a different player's snapshot.
    origin: Snapshot,
    after: [u8; 4],
    mask: [u8; 4],
}
impl Plan {
    pub(crate) fn has_changes(&self) -> bool {
        self.after != self.origin.words
    }
    /// Keep the exact requested masked values while rebinding only the verified
    /// ordinary-record after-image. Never changes appearance source words.
    pub(crate) fn after_record(&self, snapshot: &Snapshot) -> Result<Self> {
        ensure!(
            snapshot.generation == self.origin.generation
                && snapshot.pointers == self.origin.pointers
                && snapshot.other_pointers == self.origin.other_pointers
                && snapshot.appearance == self.origin.appearance
                && snapshot.allocation == self.origin.allocation
                && snapshot.words == self.origin.words
                && snapshot.native_metadata == self.origin.native_metadata,
            "Appearance composite preimage changed"
        );
        let mut next = self.clone();
        next.origin = snapshot.clone();
        Ok(next)
    }
    pub(crate) fn new(
        snapshot: &Snapshot,
        height: Option<f64>,
        wingspan: Option<f64>,
    ) -> Result<Self> {
        let mut after = snapshot.words;
        let mut mask = [0; 4];
        for (index, value, max) in [(0, height, 260.0), (1, wingspan, 320.0)] {
            if let Some(value) = value {
                ensure!(
                    value.is_finite() && (120.0..=max).contains(&value),
                    "Appearance measurement outside supported cm range"
                );
                let raw = (value * 100.0).round() as u16;
                ensure!(raw <= 0x7fff, "Appearance measurement exceeds15bits");
                let offset = index * 2;
                let old = u16::from_le_bytes(after[offset..offset + 2].try_into()?);
                after[offset..offset + 2].copy_from_slice(&((old & 0x8000) | raw).to_le_bytes());
                mask[offset..offset + 2].copy_from_slice(&[0xff, 0x7f]);
            }
        }
        Ok(Self {
            origin: snapshot.clone(),
            after,
            mask,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Prepared,
    Intent,
    Applied,
    Restored,
    Held,
}
/// Durable adapter must Flush and read back exact snapshot+plan+phase before
/// returning. No production journal adapter exists yet.
pub(crate) trait Journal {
    fn persist(&mut self, snapshot: &Snapshot, plan: &Plan, phase: Phase) -> Result<()>;
    fn verify(&self, snapshot: &Snapshot, plan: &Plan, phase: Phase) -> Result<()>;
}
pub(crate) struct Permit<'a> {
    snapshot: &'a Snapshot,
    plan: &'a Plan,
    undo: bool,
}
impl Permit<'_> {
    pub(crate) fn is_undo(&self) -> bool {
        self.undo
    }
    /// Only a trusted atomic adapter, after an actual successful CAS, may mint
    /// this receipt. Observing desired bytes is explicitly insufficient.
    pub(crate) fn applied_receipt(&self) -> EffectReceipt {
        EffectReceipt {
            origin: self.snapshot.clone(),
            after: self.plan.after,
            undo: self.undo,
        }
    }
    fn accepts(&self, receipt: &EffectReceipt) -> bool {
        receipt.origin == *self.snapshot
            && receipt.after == self.plan.after
            && receipt.undo == self.undo
    }
    pub(crate) fn address(&self) -> u64 {
        self.snapshot.appearance
    }
    pub(crate) fn before(&self) -> [u8; 4] {
        if self.undo {
            self.plan.after
        } else {
            self.snapshot.words
        }
    }
    pub(crate) fn after(&self) -> [u8; 4] {
        if self.undo {
            self.snapshot.words
        } else {
            self.plan.after
        }
    }
    pub(crate) fn mask(&self) -> [u8; 4] {
        self.plan.mask
    }
    pub(crate) fn verify(&self, m: &impl Access, journal: &impl Journal) -> Result<()> {
        let phase = if self.undo {
            Phase::Held
        } else {
            Phase::Intent
        };
        journal.verify(self.snapshot, self.plan, phase)?;
        self.snapshot.guard(m, &self.before())?;
        ensure!(
            self.before()
                .iter()
                .zip(self.after())
                .zip(self.mask())
                .all(|((a, b), mask)| (*a ^ b) & !mask == 0),
            "Appearance plan escapes15bitmask"
        );
        Ok(())
    }
}
pub(crate) struct Transaction {
    snapshot: Snapshot,
    plan: Plan,
    phase: Phase,
    owned: Option<EffectReceipt>,
}
impl Transaction {
    pub(crate) fn new(snapshot: Snapshot, plan: Plan) -> Result<Self> {
        ensure!(
            plan.origin == snapshot,
            "Appearance plan belongs to a different captured owner or preimage"
        );
        Ok(Self {
            snapshot,
            plan,
            phase: Phase::Prepared,
            owned: None,
        })
    }
    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }
    pub(crate) fn apply(&mut self, m: &impl Access, journal: &mut impl Journal) -> Result<()> {
        ensure!(
            self.phase == Phase::Prepared,
            "Appearance transaction never retries forward work"
        );
        m.forward_check()?;
        self.snapshot.guard(m, &self.snapshot.words)?;
        if self.plan.after == self.snapshot.words {
            self.phase = Phase::Restored;
            return Ok(());
        }
        self.phase = Phase::Held;
        journal.persist(&self.snapshot, &self.plan, Phase::Intent)?;
        self.phase = Phase::Intent;
        let permit = Permit {
            snapshot: &self.snapshot,
            plan: &self.plan,
            undo: false,
        };
        permit.verify(m, journal)?;
        // Retain uncertainty before the first possible byte effect.
        self.phase = Phase::Held;
        // Journal I/O and native profile/ownership revalidation can take time.
        // Cancellation during those steps must still forbid the forward CAS.
        // Undo deliberately uses only retained ownership in rollback().
        m.forward_check()?;
        let (receipt, error) = match m.compare_exchange_masked(&permit)? {
            CasOutcome::Applied(receipt) => (receipt, None),
            CasOutcome::FailedAfterApplied(receipt, error) => (receipt, Some(error)),
            CasOutcome::NoEffect => {
                if self.snapshot.guard(m, &self.snapshot.words).is_ok() {
                    journal.persist(&self.snapshot, &self.plan, Phase::Restored)?;
                    self.phase = Phase::Restored;
                }
                anyhow::bail!("Appearance CAS made no effect; no rollback authority was acquired");
            }
            CasOutcome::Uncertain => anyhow::bail!(
                "Appearance CAS outcome uncertain; transaction held without undo authority"
            ),
        };
        ensure!(
            permit.accepts(&receipt),
            "Appearance effect receipt belongs to another operation"
        );
        self.owned = Some(receipt);
        self.snapshot.guard(m, &self.plan.after)?;
        if let Some(error) = error {
            self.rollback(m, journal)?;
            return Err(error);
        }
        journal.persist(&self.snapshot, &self.plan, Phase::Applied)?;
        self.phase = Phase::Applied;
        Ok(())
    }
    pub(crate) fn rollback(&mut self, m: &impl Access, journal: &mut impl Journal) -> Result<()> {
        ensure!(
            matches!(self.phase, Phase::Held | Phase::Applied),
            "Appearance rollback has no owned effect"
        );
        ensure!(
            self.owned
                .as_ref()
                .is_some_and(|r| r.origin == self.snapshot
                    && r.after == self.plan.after
                    && !r.undo),
            "Appearance rollback lacks an owned atomic effect receipt"
        );
        self.phase = Phase::Held;
        self.snapshot.guard(m, &self.plan.after)?;
        journal.persist(&self.snapshot, &self.plan, Phase::Held)?;
        let permit = Permit {
            snapshot: &self.snapshot,
            plan: &self.plan,
            undo: true,
        };
        permit.verify(m, journal)?;
        // Reverse authority is single-attempt. An uncertain undo, or a later
        // read/journal failure, must never reuse a receipt after foreign ABA.
        self.owned = None;
        let outcome = m.compare_exchange_masked(&permit)?;
        match outcome {
            CasOutcome::Applied(receipt) if permit.accepts(&receipt) => {}
            _ => anyhow::bail!("Appearance undo was not confirmed by its atomic adapter; held"),
        }
        self.snapshot.guard(m, &self.snapshot.words)?;
        journal.persist(&self.snapshot, &self.plan, Phase::Restored)?;
        self.phase = Phase::Restored;
        self.owned = None;
        Ok(())
    }
}
#[cfg(test)]
#[path = "roster_v2_appearance_tests.rs"]
mod tests;

#[path = "roster_v2_appearance_journal.rs"]
pub(crate) mod journal;
#[path = "roster_v2_appearance_native.rs"]
pub(crate) mod native;
#[path = "roster_v2_appearance_source.rs"]
pub(crate) mod source;
