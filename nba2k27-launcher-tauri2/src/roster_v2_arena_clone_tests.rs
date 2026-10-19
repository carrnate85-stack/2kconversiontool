use super::*;
use std::cell::{Cell, RefCell};
fn utf(row: &mut [u8], at: usize, value: &str) {
    for (i, c) in value.encode_utf16().enumerate() {
        row[at + i * 2..at + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
}
fn fixture() -> (Plan, Edit, Vec<u8>, Vec<u64>) {
    let base: u64 = 0x2_2000_0000;
    let table = base + 0x80000;
    let arena_table = base + 0x180000;
    let mut team = vec![0; profile::TEAM_STRIDE];
    team[writer::TEAM_ID..writer::TEAM_ID + 2].copy_from_slice(&7u16.to_le_bytes());
    utf(&mut team, 0x2fa, "Fixture");
    team[LINK..LINK + 8].copy_from_slice(&arena_table.to_le_bytes());
    let mut rows = vec![0; STRIDE * 3];
    rows[ID..ID + 2].copy_from_slice(&17u16.to_le_bytes());
    utf(&mut rows, 0, "Donor");
    rows[0x7e..0x7e + 9].copy_from_slice(b"arena_017");
    rows[0xa4..0xa4 + 9].copy_from_slice(b"floor_017");
    rows[0x300] = 37;
    rows[STRIDE + ID..STRIDE + ID + 2].copy_from_slice(&999u16.to_le_bytes());
    rows[2 * STRIDE + ID..2 * STRIDE + ID + 2].copy_from_slice(&1000u16.to_le_bytes());
    let links = vec![arena_table; 30];
    let owner = Owner {
        team: writer::Owner {
            game: crate::session_safety::Identity {
                pid: 7,
                started: "42".into(),
                path: r"C:\QA\NBA2K27.exe".into(),
            },
            session: "a".repeat(48),
            image: 0x140000000,
            root: base + 0x50,
            owner: base + 0x100,
            region_base: base,
            region_len: 0x800000,
            kind: writer::Kind::Team,
            table,
            count: 30,
            index: 0,
            identity: writer::identity(writer::Kind::Team, &team).unwrap(),
            proof: writer::proof_tag().into(),
        },
        arena_table,
        arena_count: 3,
        donor: 0,
        target: 1,
        target_id: 999,
        donor_hash: hash(&rows[..STRIDE]),
        ids_hash: ids_hash(&rows).unwrap(),
        other_links_hash: links_hash(&links, 0),
        catalog_id: "c".repeat(64),
        protected_header_hash: "e".repeat(64),
    };
    let edit = Edit {
        record_index: 0,
        team_id: 7,
        expected_team_name: "Fixture".into(),
        donor_arena_id: 17,
        target_arena_id: 999,
        arena_name: "Fixture Court".into(),
        arena_file: "arena_999_int".into(),
        floor_file: "arena_999_int_floor".into(),
        catalog_id: "c".repeat(64),
    };
    (
        plan(owner, &edit, &team, &rows, &links).unwrap(),
        edit,
        rows,
        links,
    )
}
struct Fake {
    plan: Plan,
    raw: RefCell<Raw>,
    writes: Cell<usize>,
    fail: Option<usize>,
    partial: bool,
    foreign: bool,
    cancel: Cell<bool>,
    change_owner: Cell<bool>,
}
impl Fake {
    fn new(plan: &Plan) -> Self {
        Self {
            plan: plan.clone(),
            raw: RefCell::new(plan.before.clone()),
            writes: Cell::new(0),
            fail: None,
            partial: false,
            foreign: false,
            cancel: Cell::new(false),
            change_owner: Cell::new(false),
        }
    }
}
impl Access for Fake {
    fn owner(&self) -> Result<Owner> {
        let mut value = self.plan.owner.clone();
        if self.change_owner.get() {
            value.team.session = "b".repeat(48);
        }
        Ok(value)
    }
    fn capture(&self) -> Result<Raw> {
        Ok(self.raw.borrow().clone())
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(!self.cancel.get(), "cancelled");
        Ok(())
    }
    fn write(&self, area: Area, at: usize, bytes: &[u8]) -> Result<()> {
        let n = self.writes.get();
        self.writes.set(n + 1);
        let mut raw = self.raw.borrow_mut();
        let b = if area == Area::Team {
            &mut raw.team
        } else {
            &mut raw.arena
        };
        if self.fail == Some(n) {
            if self.partial {
                let changed = (0..bytes.len()).find(|i| b[at + i] != bytes[*i]).unwrap();
                b[at + changed] = bytes[changed];
                if self.foreign {
                    b[at + changed] ^= 3;
                }
            } else if self.foreign {
                b[at] ^= 7;
            }
            anyhow::bail!("fixture write failure");
        }
        b[at..at + bytes.len()].copy_from_slice(bytes);
        Ok(())
    }
}
#[test]
fn clone_copies_full_donor_preserves_reserved_id_and_publishes_team_last() {
    let (p, _, _, _) = fixture();
    assert_eq!(word(&p.after.arena, ID).unwrap(), 999);
    assert_eq!(p.after.arena[0x300], 37);
    assert_eq!(text(&p.after.arena, 0, 70).unwrap(), "Fixture Court");
    assert_eq!(
        ascii(&p.after.arena, 0xa4, 40).unwrap(),
        "arena_999_int_floor"
    );
    assert!(
        p.patches[..p.patches.len() - 1]
            .iter()
            .all(|v| v.area == Area::Arena && !(v.offset..v.offset + v.after.len()).contains(&ID))
    );
    assert_eq!(p.patches.last().unwrap().area, Area::Team);
    let f = Fake::new(&p);
    let mut durable = false;
    apply(&f, &p, |i| {
        if i.pending {
            assert_eq!(f.writes.get(), 0);
            durable = true;
        }
        Ok(())
    })
    .unwrap();
    assert!(durable);
    assert_eq!(*f.raw.borrow(), p.after);
    assert_eq!(&p.before.team[..LINK], &p.after.team[..LINK]);
    assert_eq!(&p.before.team[LINK + 8..], &p.after.team[LINK + 8..]);
}
#[test]
fn stale_ambiguous_referenced_populated_and_escaped_targets_refuse() {
    let (p, e, rows, links) = fixture();
    for mut changed in [e.clone(), e.clone(), e.clone()] {
        changed.catalog_id = "d".repeat(64);
        assert!(plan(p.owner.clone(), &changed, &p.before.team, &rows, &links).is_err());
    }
    let mut r = rows.clone();
    r[2 * STRIDE + ID..2 * STRIDE + ID + 2].copy_from_slice(&999u16.to_le_bytes());
    assert!(plan(p.owner.clone(), &e, &p.before.team, &r, &links).is_err());
    let mut r = rows.clone();
    r[STRIDE] = b'X';
    assert!(plan(p.owner.clone(), &e, &p.before.team, &r, &links).is_err());
    let mut l = links.clone();
    l[1] = p.owner.target_address();
    assert!(plan(p.owner.clone(), &e, &p.before.team, &rows, &l).is_err());
    let mut o = p.owner.clone();
    o.arena_table = o.team.table;
    assert!(o.validate().is_err());
    for s in ["../a", "a.iff", "a/b", "a\\b", "a:b", "a b", ""] {
        assert!(stem(s, 39).is_err());
    }
}
#[test]
fn failed_durability_and_cancel_have_no_effect() {
    let (p, _, _, _) = fixture();
    let f = Fake::new(&p);
    assert!(apply(&f, &p, |_| anyhow::bail!("disk full")).is_err());
    assert_eq!(f.writes.get(), 0);
    let f = Fake::new(&p);
    assert!(
        apply(&f, &p, |i| {
            if i.pending {
                f.cancel.set(true);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(f.writes.get(), 0);
}
#[test]
fn every_complete_effect_failure_rolls_back_before_unlinking_dependencies() {
    let (p, _, _, _) = fixture();
    for n in 0..p.patches.len() {
        let mut f = Fake::new(&p);
        f.fail = Some(n);
        let mut final_pending = true;
        assert!(
            apply(&f, &p, |i| {
                final_pending = i.pending;
                Ok(())
            })
            .is_err()
        );
        assert!(!final_pending);
        assert_eq!(*f.raw.borrow(), p.before);
    }
}
#[test]
fn partial_foreign_and_changed_owner_hold_without_forced_restore() {
    let (p, _, _, _) = fixture();
    let mut f = Fake::new(&p);
    f.fail = Some(0);
    f.partial = true;
    f.foreign = true;
    let mut pending = false;
    assert!(
        apply(&f, &p, |i| {
            pending = i.pending;
            Ok(())
        })
        .is_err()
    );
    assert!(pending);
    assert_eq!(f.writes.get(), 1);
    let f = Fake::new(&p);
    assert!(
        apply(&f, &p, |i| {
            if i.pending {
                f.change_owner.set(true);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(f.writes.get(), 0);
}
#[test]
fn final_journal_failure_conditionally_restores_both_records() {
    let (p, _, _, _) = fixture();
    let f = Fake::new(&p);
    assert!(
        apply(&f, &p, |i| {
            if !i.pending && i.result == "applied and read back verified" {
                anyhow::bail!("commit disk full");
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(*f.raw.borrow(), p.before);
}
#[test]
fn forged_maskless_span_or_link_order_cannot_gain_authority() {
    let (p, _, _, _) = fixture();
    let mut q = p.clone();
    q.patches[0].offset = ID;
    assert!(validate_patches(q.mode, &q.owner, &q.patches).is_err());
    let mut q = p.clone();
    q.patches.reverse();
    assert!(validate_patches(q.mode, &q.owner, &q.patches).is_err());
    let mut q = p.clone();
    q.patches.last_mut().unwrap().after = 0x12340000u64.to_le_bytes().to_vec();
    assert!(validate_patches(q.mode, &q.owner, &q.patches).is_err());
}

#[test]
fn saved_profile_populates_even_referenced_empty_target_without_any_team_effect() {
    let (p, e, rows, mut links) = fixture();
    let mut team = p.before.team.clone();
    links[0] = p.owner.target_address();
    links[2] = p.owner.target_address();
    team[LINK..LINK + 8].copy_from_slice(&links[0].to_le_bytes());
    let mut owner = p.owner.clone();
    owner.other_links_hash = links_hash(&links, 0);
    assert!(plan(owner.clone(), &e, &team, &rows, &links).is_err());
    let profile = plan_profile(owner, &e, &team, &rows, &links).unwrap();
    assert_eq!(profile.mode, Mode::PopulateOnly);
    assert_eq!(profile.before.team, profile.after.team);
    assert!(profile.patches.iter().all(|p| p.area == Area::Arena));
    let f = Fake::new(&profile);
    let report = apply(&f, &profile, |_| Ok(())).unwrap();
    assert_eq!(report["mode"], "populateOnly");
    assert_eq!(report["readBackVerified"], true);
    assert_eq!(f.raw.borrow().team, team);
}
#[test]
fn saved_profile_exact_full_desired_noop_and_foreign_populated_refusal() {
    let (p, e, mut rows, links) = fixture();
    rows[STRIDE..2 * STRIDE].copy_from_slice(&p.after.arena);
    let profile = plan_profile(p.owner.clone(), &e, &p.before.team, &rows, &links).unwrap();
    assert!(profile.patches.is_empty());
    assert_eq!(profile.before, profile.after);
    let f = Fake::new(&profile);
    let report = apply(&f, &profile, |_| Ok(())).unwrap();
    assert_eq!(report["noChanges"], true);
    assert_eq!(f.writes.get(), 0);
    for at in [0, 0x300, STRIDE - 1] {
        let mut foreign = rows.clone();
        foreign[STRIDE + at] ^= 1;
        assert!(plan_profile(p.owner.clone(), &e, &p.before.team, &foreign, &links).is_err());
    }
    let mut forged = profile.clone();
    forged.patches.push(p.patches.last().unwrap().clone());
    assert!(validate_patches(forged.mode, &forged.owner, &forged.patches).is_err());
}
#[test]
fn no_effect_metadata_resolution_requires_exact_pending_intent_and_never_claims_graph_restore() {
    let (p, _, _, _) = fixture();
    let root = std::env::current_dir()
        .unwrap()
        .join("artifacts")
        .join(format!("qa084-arena-noeffect-{}", backend::unique_id()));
    std::fs::create_dir_all(root.join("Data")).unwrap();
    let i = prepare_intent(&p).unwrap();
    save(&root, &i).unwrap();
    let mut foreign = i.clone();
    foreign.id = backend::unique_id();
    assert!(resolve_no_effect(&root, &foreign).is_err());
    assert!(pending(&root).unwrap());
    resolve_no_effect(&root, &i).unwrap();
    let closed = load(&root).unwrap();
    assert!(!closed.pending);
    assert!(closed.result.contains("no graph equality claimed"));
    assert!(!closed.result.contains("restored"));
}

#[test]
fn zero_attempt_failure_cannot_claim_original_restored_after_foreign_preimage_change() {
    let (p, _, _, _) = fixture();
    let f = Fake::new(&p);
    let mut pending = false;
    assert!(
        apply(&f, &p, |i| {
            pending = i.pending;
            if i.pending {
                f.raw.borrow_mut().arena[STRIDE - 1] ^= 1;
                f.cancel.set(true);
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(f.writes.get(), 0);
    assert!(pending);
}

#[test]
fn arena_owner_refuses_full_metadata_aliases_and_missing_protected_witness() {
    let (p, _, _, _) = fixture();
    for team in [false, true] {
        for at in [
            p.owner.team.region_base + 0x678,
            p.owner.team.owner + 0x2a0,
            p.owner.team.owner + 0x3c8,
            p.owner.team.root + 0xa8,
        ] {
            let mut o = p.owner.clone();
            if team {
                o.team.table = at;
            } else {
                o.arena_table = at;
            }
            assert!(o.validate().is_err());
        }
    }
    let mut o = p.owner.clone();
    o.protected_header_hash.clear();
    assert!(o.validate().is_err());
    let mut old = serde_json::to_value(&p.owner).unwrap();
    old.as_object_mut().unwrap().remove("protectedHeaderHash");
    assert!(serde_json::from_value::<Owner>(old).is_err());
}
