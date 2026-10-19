use super::*;
use serde_json::json;

fn fixture() -> Snapshot {
    let mut player = [0u8; PLAYER_BYTES];
    for s in 0..ALL_SLOTS {
        set_slot(&mut player, s, EMPTY);
    }
    set_slot(&mut player, 0, 0);
    let mut rows = vec![[0u8; STAT_BYTES]; 5];
    rows[0][..8].copy_from_slice(&0x10000u64.to_le_bytes());
    rows[0][17] = 124;
    put_word(&mut rows[0], 18, EMPTY);
    set_refcount(&mut rows[0], 1);
    for (r, row) in rows.iter_mut().enumerate().skip(1) {
        *row = cleared(row, if r == 4 { EMPTY } else { r as u16 + 1 });
    }
    let mut manager = [0; MANAGER_BYTES];
    put_word(&mut manager, 0, 1);
    Snapshot {
        players: vec![player],
        rows,
        manager,
        pool: PoolKind::Player,
        manager_count: 1,
        teams: TeamDirectory {
            base: 0x10000,
            count: 4,
        },
    }
}
fn planned(snapshot: &Snapshot, op: Operation) -> Plan {
    match plan(snapshot, op).unwrap() {
        Outcome::Planned(p) => p,
        Outcome::Unchanged => panic!("expected plan"),
    }
}
fn shared() -> Snapshot {
    let mut s = fixture();
    s.players.push(s.players[0]);
    set_refcount(&mut s.rows[0], 2);
    s
}
fn copy_op() -> Operation {
    Operation::NarrowSharedCopy {
        player: 0,
        slot: 0,
        changes: json!({"gp":10}),
    }
}

#[test]
fn exact_native_extents_and_separate_pools() {
    let s = fixture();
    let players: Vec<u8> = s.players.iter().flatten().copied().collect();
    let rows: Vec<u8> = s.rows.iter().flatten().copied().collect();
    assert_eq!(
        Snapshot::from_bytes(&players, &rows, &s.manager, s.pool, 1, s.teams).unwrap(),
        s
    );
    assert_eq!(
        Snapshot::from_bytes(&players, &rows, &[0; 64], s.pool, 1, s.teams),
        Err(Refusal::Shape)
    );
    for pool in [PoolKind::Staff, PoolKind::Team] {
        assert_eq!(
            Snapshot::from_bytes(&players, &rows, &s.manager, pool, 1, s.teams),
            Err(Refusal::Pool)
        );
    }
    for count in [0, 2, usize::MAX] {
        assert_eq!(
            Snapshot::from_bytes(&players, &rows, &s.manager, s.pool, count, s.teams),
            Err(Refusal::Pool)
        );
    }
    for (p, r) in [
        (&players[..players.len() - 1], rows.as_slice()),
        (players.as_slice(), &rows[..rows.len() - 1]),
        (&[][..], rows.as_slice()),
    ] {
        assert_eq!(
            Snapshot::from_bytes(p, r, &s.manager, s.pool, 1, s.teams),
            Err(Refusal::Shape)
        );
    }
    // No invented padding or manager/table non-overlap rule: exact 56-byte
    // manager can precede the first row immediately in a capture buffer.
    let mut adjacent = s.manager.to_vec();
    adjacent.extend_from_slice(&rows);
    assert!(
        Snapshot::from_bytes(
            &players,
            &adjacent[56..],
            &adjacent[..56],
            s.pool,
            1,
            s.teams
        )
        .is_ok()
    );
}

#[test]
fn all33_occurrences_not_unique_players_and_private_branch() {
    let mut s = fixture();
    set_slot(&mut s.players[0], 25, 0);
    assert_eq!(s.validate().unwrap_err(), Refusal::CountMismatch(0));
    set_refcount(&mut s.rows[0], 2);
    let v = s.validate().unwrap();
    assert_eq!(
        v.references(0).unwrap(),
        &[
            Occurrence { player: 0, slot: 0 },
            Occurrence {
                player: 0,
                slot: 25
            }
        ]
    );
    assert_eq!(
        plan(&s, Operation::RemoveOldest { player: 0 }).unwrap_err(),
        Refusal::SharedRemoval
    );
    s.players.push(s.players[0]);
    s.players[1][0xe0] = 1;
    assert_eq!(s.validate().unwrap_err(), Refusal::PrivateHistory(1));
}

#[test]
fn invalid_reference_and_counter_boundaries() {
    for bad in [0xfffe, 5, 32767] {
        let mut s = fixture();
        set_slot(&mut s.players[0], 32, bad);
        assert_eq!(
            s.validate().unwrap_err(),
            Refusal::Reference {
                player: 0,
                slot: 32
            }
        );
    }
    let mut s = fixture();
    s.players = vec![s.players[0]; 255];
    set_refcount(&mut s.rows[0], 255);
    assert!(s.validate().is_ok());
    s.players.push(s.players[0]);
    assert_eq!(s.validate().unwrap_err(), Refusal::CountOverflow(0));
    let mut s = fixture();
    set_refcount(&mut s.rows[0], 0);
    assert_eq!(s.validate().unwrap_err(), Refusal::CountMismatch(0));
}

#[test]
fn bounded_complete_canonical_free_chain() {
    assert_eq!(fixture().validate().unwrap().free_chain(), &[1, 2, 3, 4]);
    for next in [1, 5, 0] {
        let mut s = fixture();
        put_word(&mut s.rows[1], 18, next);
        assert_eq!(s.validate().unwrap_err(), Refusal::FreeChain);
    }
    let mut s = fixture();
    put_word(&mut s.manager, 0, 5);
    assert_eq!(s.validate().unwrap_err(), Refusal::FreeChain);
    let mut s = fixture();
    put_word(&mut s.rows[1], 18, EMPTY);
    assert_eq!(s.validate().unwrap_err(), Refusal::IncompleteFreeChain);
    let mut s = fixture();
    s.rows[2][31] = 1;
    assert_eq!(s.validate().unwrap_err(), Refusal::NoncanonicalFree(2));
    let mut s = fixture();
    put_word(&mut s.manager, 0, 0);
    assert_eq!(s.validate().unwrap_err(), Refusal::FreeChain);
    let mut s = fixture();
    for at in [48, 56, 60] {
        put_dword(&mut s.rows[1], at, 0xf0000000);
    }
    assert!(s.validate().is_ok());
}

#[test]
fn add_older_preserves_destination_reserved_and_unrelated_bytes() {
    let mut s = fixture();
    s.manager[30] = 99;
    s.players[0][50] = 77;
    for (at, v) in [(48, 0xa0000000), (56, 0xf8000000), (60, 0xb0000000)] {
        put_dword(&mut s.rows[1], at, v);
    }
    let p = planned(&s, Operation::AddOlder { player: 0 });
    assert_eq!(
        p.phases(),
        [Phase::ReserveHead, Phase::InitializeRow, Phase::PublishSlot]
    );
    assert_eq!(slot(&p.after().players[0], 1), 1);
    assert_eq!(p.after().rows[1][17], 123);
    assert_eq!(pointer(&p.after().rows[1], 0), 0x10000);
    assert_eq!(pointer(&p.after().rows[1], 8), 0);
    assert_eq!(word(&p.after().rows[1], 18), EMPTY);
    assert_eq!(refcount(&p.after().rows[1]), 1);
    for (at, mask) in [(48, 0xf0000000), (56, 0xf8000000), (60, 0xf0000000)] {
        assert_eq!(
            dword(&p.after().rows[1], at) & mask,
            dword(&s.rows[1], at) & mask
        );
    }
    assert_eq!(p.after().rows[0], s.rows[0]);
    assert_eq!(p.after().rows[2..], s.rows[2..]);
    assert_eq!(p.after().manager[2..], s.manager[2..]);
    assert_eq!(p.after().players[0][50], 77);
    assert!(p.before().validate().is_ok());
    assert!(p.after().validate().is_ok());
    s.rows[0][17] = 1;
    assert_eq!(p.before().rows[0][17], 124); // owns its input
}

#[test]
fn add_boundaries_and_selected_calendar_only() {
    let mut s = fixture();
    s.rows[0][17] = 0;
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 0 }).unwrap_err(),
        Refusal::YearFloor
    );
    let mut s = fixture();
    set_slot(&mut s.players[0], 0, EMPTY);
    set_refcount(&mut s.rows[0], 0);
    s.rows[0] = cleared(&s.rows[0], 1);
    put_word(&mut s.manager, 0, 0);
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 0 }).unwrap_err(),
        Refusal::EmptyHistory
    );
    let mut s = fixture();
    set_slot(&mut s.players[0], 0, EMPTY);
    set_slot(&mut s.players[0], 24, 0);
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 0 }).unwrap_err(),
        Refusal::FullHistory
    );
    let mut s = fixture();
    s.rows.truncate(1);
    put_word(&mut s.manager, 0, EMPTY);
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 0 }).unwrap_err(),
        Refusal::NoFreeRow
    );
    let mut s = fixture();
    s.rows[0][..8].fill(0);
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 0 }).unwrap_err(),
        Refusal::TeamPointer
    );
    let mut s = fixture();
    // An unrelated player's odd calendar is not part of this operation's scope.
    let mut other = s.players[0];
    set_slot(&mut other, 1, 0);
    s.players.push(other);
    set_refcount(&mut s.rows[0], 3);
    assert!(plan(&s, Operation::AddOlder { player: 0 }).is_ok());
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 1 }).unwrap_err(),
        Refusal::Calendar
    );
}

#[test]
fn calendar_gaps_are_not_compacted() {
    let s = fixture();
    let first = planned(&s, Operation::AddOlder { player: 0 });
    let mut s = first.after().clone();
    set_slot(&mut s.players[0], 1, EMPTY);
    set_slot(&mut s.players[0], 2, 1);
    s.rows[1][17] = 122;
    let p = planned(&s, Operation::AddOlder { player: 0 });
    assert_eq!(slot(&p.after().players[0], 1), EMPTY);
    assert_eq!(slot(&p.after().players[0], 3), 2);
    s.rows[1][17] = 123;
    assert_eq!(
        plan(&s, Operation::AddOlder { player: 0 }).unwrap_err(),
        Refusal::Calendar
    );
}

#[test]
fn remove_oldest_and_native_clear_reserved_bits() {
    let mut s = fixture();
    for (at, v) in [(48, 0xa0000000), (56, 0xf8000000), (60, 0xb0002000)] {
        put_dword(&mut s.rows[0], at, v);
    }
    let p = planned(&s, Operation::RemoveOldest { player: 0 });
    assert_eq!(
        p.phases(),
        [Phase::UnlinkSlot, Phase::ClearRow, Phase::ReleaseHead]
    );
    assert_eq!(slot(&p.after().players[0], 0), EMPTY);
    assert_eq!(word(&p.after().manager, 0), 0);
    assert_eq!(p.after().rows[0], cleared(&s.rows[0], 1));
    assert!(p.after().validate().is_ok());
    let p = planned(&fixture(), Operation::AddOlder { player: 0 });
    let remove = planned(p.after(), Operation::RemoveOldest { player: 0 });
    assert_eq!(remove.after(), &fixture());
    assert_eq!(
        plan(&shared(), Operation::RemoveOldest { player: 0 }).unwrap_err(),
        Refusal::SharedRemoval
    );
}

#[test]
fn narrow_copy_occurrences_and_exact_dependency_order() {
    for same_player in [false, true] {
        let mut s = if same_player { fixture() } else { shared() };
        if same_player {
            set_slot(&mut s.players[0], 32, 0);
            set_refcount(&mut s.rows[0], 2);
        }
        let p = planned(&s, copy_op());
        assert_eq!(
            p.phases(),
            [
                Phase::ReserveHead,
                Phase::InitializeRow,
                Phase::PublishSlot,
                Phase::DecrementSourceCount
            ]
        );
        assert_eq!(slot(&p.after().players[0], 0), 1);
        assert_eq!(refcount(&p.after().rows[0]), 1);
        assert_eq!(refcount(&p.after().rows[1]), 1);
        if same_player {
            assert_eq!(slot(&p.after().players[0], 32), 0);
        } else {
            assert_eq!(p.after().players[1], s.players[1]);
        }
        assert_eq!(
            p.reverse_preview(p.after()).unwrap(),
            [
                Phase::DecrementSourceCount,
                Phase::PublishSlot,
                Phase::InitializeRow,
                Phase::ReserveHead
            ]
        );
        assert!(p.after().validate().is_ok());
    }
}

#[test]
fn narrow_copy_noop_and_edit_refusals() {
    let mut s = shared();
    s.rows.truncate(1);
    put_word(&mut s.manager, 0, EMPTY);
    assert!(matches!(
        plan(
            &s,
            Operation::NarrowSharedCopy {
                player: 0,
                slot: 0,
                changes: json!({"gp":0})
            }
        ),
        Ok(Outcome::Unchanged)
    ));
    assert_eq!(plan(&s, copy_op()).unwrap_err(), Refusal::NoFreeRow);
    assert_eq!(plan(&fixture(), copy_op()).unwrap_err(), Refusal::NotShared);
    for changes in [
        json!(null),
        json!({"ppg":2}),
        json!({"unknown":3}),
        json!({"gp":-1}),
        json!({"gp":0,"gs":1}),
        json!({"award:mvp":2}),
    ] {
        assert_eq!(
            plan(
                &shared(),
                Operation::NarrowSharedCopy {
                    player: 0,
                    slot: 0,
                    changes
                }
            )
            .unwrap_err(),
            Refusal::Changes
        );
    }
    assert_eq!(
        plan(
            &shared(),
            Operation::NarrowSharedCopy {
                player: 0,
                slot: 25,
                changes: json!({"gp":1})
            }
        )
        .unwrap_err(),
        Refusal::Slot
    );
    assert_eq!(
        plan(&shared(), Operation::AddOlder { player: 50 }).unwrap_err(),
        Refusal::Player
    );
}

#[test]
fn narrow_copy_never_changes_unproved_destination_bits() {
    for at in [16, 48, 56, 60] {
        let mut s = shared();
        let mask = if at == 16 { 1 } else { 128 };
        s.rows[0][if at == 16 { at } else { at + 3 }] |= mask;
        assert!(
            matches!(plan(&s, copy_op()), Err(Refusal::UnprovedCopyBit(_))),
            "offset {at}"
        );
        if at != 16 {
            s.rows[1][at + 3] |= mask;
            assert!(
                plan(&s, copy_op()).is_ok(),
                "matching destination bits {at}"
            );
        }
    }
    let mut s = shared();
    s.rows[0][8..16].copy_from_slice(&0x10001u64.to_le_bytes());
    assert_eq!(plan(&s, copy_op()).unwrap_err(), Refusal::TeamPointer);
}

#[test]
fn whole_snapshot_prefix_and_unknown_state_are_not_authority() {
    let cases = [
        planned(&fixture(), Operation::AddOlder { player: 0 }),
        planned(&fixture(), Operation::RemoveOldest { player: 0 }),
        planned(&shared(), copy_op()),
    ];
    for p in &cases {
        assert_eq!(p.inspect(p.before()), ModelState::Before);
        assert_eq!(p.inspect(p.after()), ModelState::After);
        for n in 1..p.steps.len() {
            let prefix = p.expected_prefix(n).unwrap();
            assert_eq!(
                p.inspect(&prefix),
                ModelState::KnownPaused { completed_steps: n }
            );
            assert_eq!(
                p.reverse_preview(&prefix).unwrap(),
                p.steps[..n]
                    .iter()
                    .rev()
                    .map(|s| s.phase)
                    .collect::<Vec<_>>()
            );
            assert!(
                prefix.validate().is_err(),
                "reserved/overcounted prefixes are not full valid snapshots"
            );
            let mut foreign = prefix.clone();
            foreign.manager[55] ^= 1;
            assert_eq!(p.inspect(&foreign), ModelState::Unknown);
            assert_eq!(p.reverse_preview(&foreign), Err(Refusal::UnknownPrefix));
        }
        assert!(p.expected_prefix(p.steps.len() + 1).is_none());
    }
    assert_eq!(
        cases[0].reverse_preview(cases[0].after()).unwrap(),
        [Phase::PublishSlot, Phase::InitializeRow, Phase::ReserveHead]
    );
    assert_eq!(
        cases[1].reverse_preview(cases[1].after()).unwrap(),
        [Phase::ReleaseHead, Phase::ClearRow, Phase::UnlinkSlot]
    );
}

#[test]
fn every_partial_byte_fault_is_only_exact_equivalence_or_unknown() {
    for p in [
        planned(&fixture(), Operation::AddOlder { player: 0 }),
        planned(&fixture(), Operation::RemoveOldest { player: 0 }),
        planned(&shared(), copy_op()),
    ] {
        for (n, step) in p.steps.iter().enumerate() {
            for bytes in 0..=step.after.len() {
                let mut partial = p.expected_prefix(n).unwrap();
                target_mut(&mut partial, step.target)[step.offset..step.offset + bytes]
                    .copy_from_slice(&step.after[..bytes]);
                let exact =
                    (0..=p.steps.len()).any(|k| p.expected_prefix(k).as_ref() == Some(&partial));
                assert_eq!(p.inspect(&partial) != ModelState::Unknown, exact);
                assert_eq!(p.reverse_preview(&partial).is_ok(), exact);
                // Even exact byte equivalence does not establish call completion.
                partial.players[0][0x90] ^= 1;
                assert_eq!(p.inspect(&partial), ModelState::Unknown);
            }
        }
    }
}

#[test]
fn directory_bounds_and_logical_snapshot_bounds() {
    for teams in [
        TeamDirectory { base: 0, count: 1 },
        TeamDirectory {
            base: 0x10001,
            count: 1,
        },
        TeamDirectory {
            base: 0x10000,
            count: 0,
        },
        TeamDirectory {
            base: 0x10000,
            count: 1025,
        },
        TeamDirectory {
            base: u64::MAX - 7,
            count: 1,
        },
    ] {
        let mut s = fixture();
        s.teams = teams;
        assert_eq!(s.validate().unwrap_err(), Refusal::TeamDirectory);
    }
    let mut s = fixture();
    s.players.clear();
    assert_eq!(s.validate().unwrap_err(), Refusal::Shape);
    let mut s = fixture();
    s.rows.clear();
    assert_eq!(s.validate().unwrap_err(), Refusal::Shape);
}

#[test]
fn full_bounded_chain_and_player_capacity() {
    let mut s = fixture();
    s.rows.resize(32767, [0; STAT_BYTES]);
    for r in 1..s.rows.len() {
        s.rows[r] = cleared(&s.rows[r], if r == 32766 { EMPTY } else { r as u16 + 1 });
    }
    assert_eq!(s.validate().unwrap().free_chain().len(), 32766);
    s.rows.push([0; STAT_BYTES]);
    assert_eq!(s.validate().unwrap_err(), Refusal::Shape);
    let mut s = fixture();
    let mut empty = s.players[0];
    set_slot(&mut empty, 0, EMPTY);
    s.players.resize(8192, empty);
    assert!(s.validate().is_ok());
    s.players.push(empty);
    assert_eq!(s.validate().unwrap_err(), Refusal::Shape);
}
