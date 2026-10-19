//! Version 2 session Access implementations. Existing Engines remain
//! responsible for creating/removing their immutable receipts. This adapter
//! additionally reads and verifies that exact durable intent before allowing
//! its fixed native publication operations. No context/root tags are forged.
#![allow(dead_code)]
use crate::{
    arena_v2, ball_v2, blacktop_arena as arena, blacktop_ball as ball, blacktop_native::Memory,
    rules_v2, session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Arena,
    Ball,
}
impl Kind {
    pub(crate) fn marker(self) -> &'static str {
        match self {
            Self::Arena => "blacktop-arena-session.json",
            Self::Ball => "blacktop-ball-session.json",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    ArenaMapping,
    ArenaNames,
    BallName,
}
/// Only this module constructs publication permits. The native child adapter
/// receives getters, not a constructor accepting arbitrary addresses.
pub(crate) struct Permit {
    area: Area,
    at: u64,
    len: usize,
    allocation: u64,
}
impl Permit {
    pub(crate) fn kind(&self) -> Kind {
        if self.area == Area::BallName {
            Kind::Ball
        } else {
            Kind::Arena
        }
    }
    pub(crate) fn at(&self) -> u64 {
        self.at
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    pub(crate) fn allocation(&self) -> u64 {
        self.allocation
    }
    pub(crate) fn image(&self) -> bool {
        self.area == Area::ArenaMapping
    }
    pub(crate) fn valid(&self, base: u64) -> bool {
        self.at >= 0x10000
            && self
                .at
                .checked_add(self.len as u64)
                .is_some_and(|end| end < 0x7fff00000000)
            && self.allocation >= 0x10000
            && self.allocation <= self.at
            && match self.area {
                Area::ArenaMapping => {
                    self.allocation == base
                        && base.checked_add(arena_v2::MAPPING) == Some(self.at)
                        && self.len == 8
                }
                Area::ArenaNames => self.len == 78,
                Area::BallName => (1..=128).contains(&self.len),
            }
    }
}
/// Error context for a failure proven to precede any page-protection change or
/// data write. Only such failures (re-verified against the original) retry.
#[derive(Debug)]
pub(crate) struct NotPublished;
impl std::fmt::Display for NotPublished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Not published; nothing was written")
    }
}
pub(crate) fn not_published(e: &anyhow::Error) -> bool {
    e.downcast_ref::<NotPublished>().is_some()
}
pub(crate) trait Host: arena::Inspect {
    fn kind(&self) -> Kind;
    fn identity(&self) -> &Identity;
    fn session(&self) -> &str;
    fn guard(&self, parent_required: bool) -> Result<()>;
    fn receipt(&self) -> Result<Vec<u8>>;
    fn manifest(&self) -> Result<Vec<u8>>;
    /// Called only on the provider worker, never on a UI or IPC callback.
    fn next_sample(&self) -> Result<()>;
    fn clock_ms(&self) -> Result<u64>;
    /// Must recheck before writer handle acquisition AND immediately before
    /// writing (after any protection transition), then verify exact readback.
    fn publish(
        &self,
        permit: &Permit,
        before: &[u8],
        after: &[u8],
        parent_required: bool,
        verify: impl Fn() -> Result<()>,
    ) -> Result<()>;
}
fn ordinary_receipt<H: Host>(host: &H) -> Result<Vec<u8>> {
    let bytes = host.receipt()?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 16384,
        "Native session receipt size is invalid"
    );
    Ok(bytes)
}
fn owner<H: Host>(host: &H, session: &str, game: &Identity, image: u64) -> Result<()> {
    ensure!(
        session.len() == 48
            && session.bytes().all(|b| b.is_ascii_hexdigit())
            && session == host.session()
            && *game == *host.identity()
            && image == host.base(),
        "Durable native intent belongs to another game/session/image"
    );
    Ok(())
}
fn field_text(bytes: &[u8]) -> Result<&str> {
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .context("Unterminated intent filename")?;
    ensure!(bytes[..end].is_ascii(), "Non-ASCII intent filename");
    Ok(std::str::from_utf8(&bytes[..end])?)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ArenaIntent {
    schema: String,
    session: String,
    game: Identity,
    image: u64,
    selected: String,
    mapping: [u8; 20],
    record: Option<arena::Record>,
    replacement: Option<Vec<u8>>,
}
impl ArenaIntent {
    fn read<H: Host>(host: &H) -> Result<Self> {
        ensure!(host.kind() == Kind::Arena, "Wrong arena capability");
        let value: Self = serde_json::from_slice(&ordinary_receipt(host)?)?;
        ensure!(
            value.schema == "nba2k27-native-arena-transaction-v1",
            "Unsupported arena intent"
        );
        owner(host, &value.session, &value.game, value.image)?;
        let c = arena::choice(&value.selected)?;
        ensure!(
            c.id != "default" && value.mapping == arena::mapping(c.arena_id)?,
            "Arena intent differs from catalog"
        );
        let r = value
            .record
            .as_ref()
            .context("Arena intent has no native record")?;
        ensure!(
            Some(u32::from(r.id)) == c.arena_id && r.header.len() == 16 && r.fields.len() == 78,
            "Arena intent record differs from catalog"
        );
        if c.template_arena_id.is_some() {
            ensure!(
                field_text(&r.fields[..38])? == "arena_700_int"
                    && field_text(&r.fields[38..])? == "arena_700_int_floor",
                "Arena intent original template fields differ"
            );
            ensure!(
                value.replacement.as_deref()
                    == Some(&arena::template_fields(
                        c.file.as_deref().unwrap(),
                        c.floor_file.as_deref().unwrap()
                    )?),
                "Arena intent filenames differ from exact catalog"
            );
        } else {
            ensure!(
                value.replacement.is_none(),
                "Non-template intent changes arena filenames"
            );
            ensure!(
                c.file.as_deref() == Some(field_text(&r.fields[..38])?)
                    && c.floor_file.as_deref().is_none_or(
                        |name| field_text(&r.fields[38..]).is_ok_and(|actual| actual == name)
                    ),
                "Arena intent original filenames differ from its exact catalog choice"
            );
        }
        Ok(value)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BallIntent {
    schema: String,
    session: String,
    game: Identity,
    image: u64,
    selected: String,
    source: u32,
    lease: ball::Lease,
    replacement: Vec<u8>,
}
impl BallIntent {
    fn read<H: Host>(host: &H) -> Result<Self> {
        ensure!(host.kind() == Kind::Ball, "Wrong ball capability");
        let value: Self = serde_json::from_slice(&ordinary_receipt(host)?)?;
        ensure!(
            value.schema == "nba2k27-native-ball-transaction-v1",
            "Unsupported ball intent"
        );
        owner(host, &value.session, &value.game, value.image)?;
        ball::choice(&value.selected)?;
        ensure!(
            !ball::current(&value.selected)
                && value.source != 0
                && value.source != ball::NORMAL
                && value
                    .selected
                    .strip_prefix("balls/")
                    .is_some_and(|name| ball::filename(&value.replacement)
                        .is_ok_and(|actual| name.eq_ignore_ascii_case(&actual))),
            "Ball intent differs from exact catalog"
        );
        ball::plan(&value.lease, &value.replacement)?;
        Ok(value)
    }
}
pub(crate) struct ArenaAccess<H: Host> {
    pub(crate) host: H,
    /// Static code-anchor proof done once per retained process; every
    /// prepare path (capture/revalidate/publish) re-verifies it before writes.
    pub(crate) profiled: std::cell::Cell<bool>,
}
impl<H: Host> ArenaAccess<H> {
    pub(crate) fn boundary(&self) -> Result<String> {
        self.host.guard(false)?;
        if !self.profiled.get() {
            arena_v2::verify_profile(&self.host)?;
            self.profiled.set(true);
        }
        Ok(rules_v2::boundary(&rules_v2::context(&self.host)?, self.host.base()).into())
    }
}
impl<H: Host> crate::arena_session::Access for ArenaAccess<H> {
    fn capture(&self, c: &arena::Choice, expected: &[u8; 20]) -> Result<arena::Snapshot> {
        self.host.guard(false)?;
        arena_v2::capture(&self.host, c, expected)
    }
    fn verify(&self, r: &arena::Record, expected: &[u8]) -> Result<()> {
        self.host.guard(false)?;
        arena_v2::revalidate(&self.host, r, expected)
    }
    fn assets(&self, c: &arena::Choice) -> Result<()> {
        self.host.guard(true)?;
        let bytes = if c.template_arena_id.is_some() {
            Some(self.host.manifest()?)
        } else {
            None
        };
        arena_v2::verify_assets(c, bytes.as_deref())
    }
    fn mapping(&self, expected: &[u8; 20], next: &[u8; 20]) -> Result<()> {
        let intent = ArenaIntent::read(&self.host)?;
        let restore = expected == &intent.mapping && next == &arena::STOCK;
        ensure!(
            restore || (expected == &arena::STOCK && next == &intent.mapping),
            "Arena mapping write has no exact durable intent"
        );
        let r = intent.record.as_ref().unwrap();
        let fields = intent.replacement.as_deref().unwrap_or(&r.fields);
        // Full initial capture uses a read-only handle before any VM_WRITE.
        let snapshot = (|| -> Result<_> {
            self.host.guard(!restore)?;
            let snapshot = arena_v2::capture(&self.host, arena::choice("default")?, expected)?;
            arena_v2::revalidate(&self.host, r, fields)?;
            Ok(snapshot)
        })()
        .map_err(|e| e.context(NotPublished))?;
        let permit = Permit {
            area: Area::ArenaMapping,
            at: self.host.base() + arena_v2::MAPPING,
            len: 8,
            allocation: self.host.base(),
        };
        self.host
            .publish(&permit, &expected[..8], &next[..8], !restore, || {
                let current = ArenaIntent::read(&self.host)?;
                ensure!(
                    current.selected == intent.selected
                        && current.mapping == intent.mapping
                        && current.record == intent.record
                        && current.replacement == intent.replacement,
                    "Arena durable intent changed before publication"
                );
                arena_v2::verify_profile(&self.host)?;
                ensure!(
                    rules_v2::context(&self.host)? == snapshot.context
                        && self.host.read(permit.at(), 20)? == expected,
                    "Arena mapping/context changed before publication"
                );
                arena_v2::revalidate(&self.host, r, fields)
            })?;
        arena_v2::capture(&self.host, arena::choice("default")?, next)?;
        arena_v2::revalidate(&self.host, r, fields)
    }
    fn template(&self, record: &arena::Record, expected: &[u8], next: &[u8]) -> Result<()> {
        let intent = ArenaIntent::read(&self.host)?;
        let r = intent
            .record
            .as_ref()
            .context("Missing arena template receipt")?;
        let replacement = intent
            .replacement
            .as_deref()
            .context("This arena has no filename-write intent")?;
        let restore = expected == replacement && next == r.fields;
        ensure!(
            r == record && (restore || (expected == r.fields && next == replacement)),
            "Arena filename write has no exact durable intent"
        );
        ensure!(
            r.id == 700
                && arena::valid_template_fields(r, expected)
                && arena::valid_template_fields(r, next),
            "Arena template write is not exact allowlisted filename data"
        );
        let snapshot = (|| -> Result<_> {
            self.host.guard(!restore)?;
            let snapshot =
                arena_v2::capture(&self.host, arena::choice("default")?, &arena::STOCK)?;
            arena_v2::revalidate(&self.host, r, expected)?;
            if !restore {
                arena_v2::verify_assets(
                    arena::choice(&intent.selected)?,
                    Some(&self.host.manifest()?),
                )?;
            }
            Ok(snapshot)
        })()
        .map_err(|e| e.context(NotPublished))?;
        let permit = Permit {
            area: Area::ArenaNames,
            at: r.address + 0x7E,
            len: 78,
            allocation: r.allocation,
        };
        self.host.publish(&permit, expected, next, !restore, || {
            let current = ArenaIntent::read(&self.host)?;
            ensure!(
                current.record == intent.record
                    && current.replacement == intent.replacement
                    && current.selected == intent.selected,
                "Arena template intent changed"
            );
            arena_v2::verify_profile(&self.host)?;
            ensure!(
                rules_v2::context(&self.host)? == snapshot.context
                    && self.host.read(self.host.base() + arena_v2::MAPPING, 20)? == arena::STOCK,
                "Arena changed before template filename publication"
            );
            arena_v2::revalidate(&self.host, r, expected)
        })?;
        arena_v2::revalidate(&self.host, r, next)
    }
}
pub(crate) struct BallAccess<H: Host> {
    pub(crate) host: H,
}
fn legacy(s: ball_v2::Snapshot) -> ball::Snapshot {
    ball::Snapshot {
        image: s.image,
        context: s.context,
        lease: s.lease,
        record: s.record,
        references: s.references,
        resource: s.resource,
        native_ball: s.native_ball,
    }
}
impl<H: Host> crate::ball_session::Access for BallAccess<H> {
    fn capture(&self) -> Result<ball::Snapshot> {
        self.host.guard(false)?;
        Ok(legacy(ball_v2::capture(&self.host)?))
    }
    fn resolve(&self, s: &ball::Snapshot, id: &str) -> Result<(u32, String)> {
        self.host.guard(true)?;
        let fresh = ball_v2::capture(&self.host)?;
        fresh.owns(&s.lease, &s.record)?;
        ensure!(
            fresh.context == s.context && fresh.can_prepare(),
            "Ball source selection changed"
        );
        let p = ball_v2::prepare(&self.host, &fresh, id, &self.host.manifest()?)?;
        Ok((p.source.model, p.source.filename))
    }
    fn write(&self, lease: &ball::Lease, replacement: &[u8], restore: bool) -> Result<()> {
        let intent = BallIntent::read(&self.host)?;
        ensure!(
            intent.lease == *lease && intent.replacement == replacement,
            "Ball filename write has no exact durable intent"
        );
        let (expected, next) = if restore {
            (replacement, lease.original.as_slice())
        } else {
            (lease.original.as_slice(), replacement)
        };
        let mut stable = ball_v2::Stable::default();
        let action = if restore {
            ball_v2::Action::Restore
        } else {
            ball_v2::Action::Prepare
        };
        let mut context = None;
        for i in 0..4 {
            self.host.guard(!restore)?;
            let snapshot = ball_v2::capture(&self.host)?;
            snapshot.owns(lease, expected)?;
            ensure!(
                if restore {
                    snapshot.can_restore()
                } else {
                    snapshot.can_prepare()
                },
                crate::ball_session::Deferred(
                    "Ball is busy or outside a verified safe menu; no filename publication entered"
                        .into()
                )
            );
            let ready = stable.observe(&snapshot, action, self.host.clock_ms()?)?;
            context = Some(snapshot.context);
            if i == 3 {
                ensure!(
                    ready,
                    crate::ball_session::Deferred(
                        "Ball changed during four unloaded observations".into()
                    )
                );
            } else {
                self.host.next_sample()?;
            }
        }
        let permit = Permit {
            area: Area::BallName,
            at: lease.address + ball::FIELD as u64,
            len: ball::field_len(&lease.original, replacement)?,
            allocation: lease.allocation,
        };
        let verify = || -> Result<()> {
            let current = BallIntent::read(&self.host)?;
            ensure!(
                current.lease == intent.lease
                    && current.replacement == intent.replacement
                    && current.source == intent.source
                    && current.selected == intent.selected,
                "Ball durable intent changed"
            );
            let snapshot = ball_v2::capture(&self.host)?;
            snapshot.owns(lease, expected)?;
            ensure!(
                context.as_ref() == Some(&snapshot.context)
                    && if restore {
                        snapshot.can_restore()
                    } else {
                        snapshot.can_prepare()
                    },
                crate::ball_session::Deferred(
                    "Ball became busy or changed safe menu before filename publication".into()
                )
            );
            if !restore {
                let p = ball_v2::prepare(
                    &self.host,
                    &snapshot,
                    &intent.selected,
                    &self.host.manifest()?,
                )?;
                ensure!(
                    p.source.model == intent.source && p.replacement == replacement,
                    "Ball live source no longer matches durable intent"
                );
            }
            Ok(())
        };
        // Errors up to here have not entered a write. Preserve unsupported,
        // foreign-owner or invalid-intent errors as held, never as Deferred.
        verify()?;
        self.host.publish(
            &permit,
            &expected[ball::FIELD..ball::FIELD + permit.len],
            &next[ball::FIELD..ball::FIELD + permit.len],
            !restore,
            verify,
        )?;
        ball_v2::capture(&self.host)?.owns(lease, next)
    }
}

/// Exact same fallible protection state machine is used by Win32 and faults.
pub(crate) trait ProtectedData {
    fn make_writable(&mut self) -> Result<u32>;
    fn write_verify(&mut self) -> Result<()>;
    fn restore(&mut self, original: u32) -> Result<()>;
}
pub(crate) fn protected_write(op: &mut impl ProtectedData) -> Result<()> {
    let original = op.make_writable()?;
    let operation = if original == 2 {
        op.write_verify()
    } else {
        Err(anyhow::anyhow!(
            "Arena page protection changed before transition"
        ))
    };
    let restored = op.restore(original);
    match (operation, restored) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => {
            Err(error.context("Arena mapping was written but protection restoration failed"))
        }
        (Err(error), Err(restore)) => anyhow::bail!(
            "Arena transaction held: {error:#}; protection restoration also failed: {restore:#}"
        ),
    }
}

#[cfg(test)]
#[path = "arena_ball_v2_adapter_tests.rs"]
mod tests;
