use super::*;
fn fixture(shared: bool) -> Snapshot {
    let mut p = [0u8; PLAYER_BYTES];
    p[0x130..0x130 + ALL_SLOTS * 2].fill(255);
    set_slot(&mut p, 0, 0);
    if shared {
        set_slot(&mut p, 25, 0);
    }
    let mut rows = vec![[0; STAT_BYTES]; 8];
    rows[0][..8].copy_from_slice(&0x400000u64.to_le_bytes());
    rows[0][17] = 120;
    put_word(&mut rows[0], 18, EMPTY);
    set_refcount(&mut rows[0], if shared { 2 } else { 1 });
    for i in 1..rows.len() {
        let next = if i + 1 < rows.len() {
            (i + 1) as u16
        } else {
            EMPTY
        };
        put_word(&mut rows[i], 18, next);
    }
    let mut manager = [0; MANAGER_BYTES];
    put_word(&mut manager, 0, 1);
    let s = Snapshot {
        players: vec![p],
        rows,
        manager,
        pool: PoolKind::Player,
        manager_count: 1,
        teams: TeamDirectory {
            base: 0x400000,
            count: 2,
        },
    };
    s.validate().unwrap();
    s
}
fn choice(year: u16, changes: Value) -> Choice {
    Choice {
        year,
        primary_team_index: 0,
        secondary_team_index: None,
        changes,
    }
}
fn planned(s: &Snapshot, c: &[Choice]) -> Plan {
    match plan(s, 0, c).unwrap() {
        Outcome::Planned(p) => p,
        _ => panic!("expected change"),
    }
}
#[test]
fn mixed_import_initializes_before_directory_and_decrements_only_after() {
    let s = fixture(true);
    let p = planned(
        &s,
        &[
            choice(2019, serde_json::json!({"gp":2})),
            choice(2020, serde_json::json!({"gp":4})),
            choice(2021, serde_json::json!({})),
        ],
    );
    assert_eq!(
        p.phases(),
        vec![
            Phase::ReserveHead,
            Phase::InitializeRow,
            Phase::InitializeRow,
            Phase::InitializeRow,
            Phase::PublishDirectory,
            Phase::DecrementSourceCount
        ]
    );
    assert_eq!(slot(&p.after.players[0], 25), 0);
    assert_eq!(refcount(&p.after.rows[0]), 1);
    assert_eq!(word(&p.after.manager, 0), 4);
    assert_eq!(p.after.rows[slot(&p.after.players[0], 0) as usize][17], 121);
    p.after.validate().unwrap();
    assert_eq!(
        p.reverse_preview(p.after()).unwrap().last(),
        Some(&Phase::ReserveHead)
    );
}
#[test]
fn in_place_updates_and_omissions_preserve_identity_and_unknown_bytes() {
    let mut s = fixture(false);
    s.rows[0][51] = 0xa0;
    let p = planned(&s, &[choice(2020, serde_json::json!({"gp":2,"gs":1}))]);
    assert_eq!(p.phases(), vec![Phase::UpdateRow]);
    assert_eq!(p.after.rows[0][51] & 0xf0, 0xa0);
    assert_eq!(p.after.manager, s.manager);
    assert_eq!(p.after.players, s.players);
    assert!(matches!(
        plan(
            &p.after,
            0,
            &[choice(2020, serde_json::json!({"gp":2,"gs":1}))]
        )
        .unwrap(),
        Outcome::Unchanged
    ));
}
#[test]
fn empty_history_and_full_twentyfive_year_history_are_supported() {
    let mut s = fixture(false);
    set_slot(&mut s.players[0], 0, EMPTY);
    s.rows[0] = cleared(&s.rows[0], 1);
    put_word(&mut s.manager, 0, 0);
    s.validate().unwrap();
    let p = planned(&s, &[choice(2020, serde_json::json!({}))]);
    assert_eq!(slot(&p.after.players[0], 0), 0);
    p.after.validate().unwrap();
    let mut s = fixture(false);
    s.rows = vec![[0; STAT_BYTES]; 32];
    for i in 0..32 {
        put_word(
            &mut s.rows[i],
            18,
            if i + 1 < 32 { (i + 1) as u16 } else { EMPTY },
        );
    }
    s.players[0][0x130..0x130 + ALL_SLOTS * 2].fill(255);
    put_word(&mut s.manager, 0, 0);
    s.validate().unwrap();
    let c: Vec<_> = (0..25)
        .map(|i| choice(2024 - i, serde_json::json!({})))
        .collect();
    let p = planned(&s, &c);
    assert_eq!(word(&p.after.manager, 0), 25);
    for i in 0..25 {
        assert_eq!(
            p.after.rows[slot(&p.after.players[0], i) as usize][17],
            124 - i as u8
        );
    }
    p.after.validate().unwrap();
}
#[test]
fn bounds_duplicates_no_eviction_teams_fields_and_exhaustion_refuse() {
    let s = fixture(false);
    for c in [
        vec![],
        vec![choice(1899, serde_json::json!({}))],
        vec![choice(2156, serde_json::json!({}))],
        vec![
            choice(2020, serde_json::json!({})),
            choice(2020, serde_json::json!({})),
        ],
        vec![choice(2045, serde_json::json!({}))],
        vec![choice(2020, serde_json::json!({"bogus":1}))],
        vec![choice(2020, serde_json::json!({"gp":1,"gs":2}))],
    ] {
        assert!(plan(&s, 0, &c).is_err());
    }
    let mut c = choice(2020, serde_json::json!({}));
    c.primary_team_index = 2;
    assert!(plan(&s, 0, &[c]).is_err());
    let mut s = s;
    s.rows.truncate(1);
    put_word(&mut s.manager, 0, EMPTY);
    s.validate().unwrap();
    assert_eq!(
        plan(&s, 0, &[choice(2019, serde_json::json!({}))]).unwrap_err(),
        Refusal::NoFreeRow
    );
}
#[test]
fn shared_copy_preserves_destination_unproved_bits_or_refuses() {
    let mut s = fixture(true);
    s.rows[0][51] = 0x10;
    assert_eq!(
        plan(&s, 0, &[choice(2020, serde_json::json!({"gp":2}))]).unwrap_err(),
        Refusal::UnprovedCopyBit(51)
    );
    s.rows[1][51] = 0x10;
    s.validate().unwrap();
    let p = planned(&s, &[choice(2020, serde_json::json!({"gp":2}))]);
    assert_eq!(p.after.rows[1][51] & 0xf0, 0x10);
    assert_eq!(refcount(&p.after.rows[0]), 1);
    assert_eq!(refcount(&p.after.rows[1]), 1);
}
#[test]
fn every_prefix_exact_reverse_and_every_partial_write_are_classified() {
    let s = fixture(true);
    let p = planned(
        &s,
        &[
            choice(2019, serde_json::json!({"gp":2})),
            choice(2020, serde_json::json!({"gp":4})),
        ],
    );
    for done in 0..=p.steps.len() {
        let mut observed = p.expected_prefix(done).unwrap();
        assert_ne!(p.inspect(&observed), ModelState::Unknown);
        for step in p.steps[..done].iter().rev() {
            let bytes = &mut target_mut(&mut observed, step.target)
                [step.offset..step.offset + step.before.len()];
            assert_eq!(bytes, step.after);
            bytes.copy_from_slice(&step.before);
        }
        assert_eq!(observed, s);
    }
    for (i, step) in p.steps.iter().enumerate() {
        for cut in 0..=step.after.len() {
            let mut x = p.expected_prefix(i).unwrap();
            target_mut(&mut x, step.target)[step.offset..step.offset + cut]
                .copy_from_slice(&step.after[..cut]);
            let state = p.inspect(&x);
            if state != ModelState::Unknown {
                assert!((0..=p.steps.len()).any(|n| p.expected_prefix(n).as_ref() == Some(&x)));
            }
        }
    }
    let mut foreign = p.after.clone();
    foreign.players[0][7] ^= 1;
    assert_eq!(p.inspect(&foreign), ModelState::Unknown);
}
#[test]
fn choices_do_not_alias_caller_and_all_thirtythree_references_are_counted() {
    let s = fixture(true);
    let mut c = vec![choice(2020, serde_json::json!({"gp":2}))];
    let p = planned(&s, &c);
    let expected = p.after.clone();
    c[0].changes["gp"] = serde_json::json!(99);
    assert_eq!(p.after, expected);
    let mut bad = s.clone();
    set_refcount(&mut bad.rows[0], 1);
    assert_eq!(plan(&bad, 0, &c).unwrap_err(), Refusal::CountMismatch(0));
    let mut bad = s;
    bad.players[0][0xe0] = 1;
    assert_eq!(plan(&bad, 0, &c).unwrap_err(), Refusal::PrivateHistory(0));
}
