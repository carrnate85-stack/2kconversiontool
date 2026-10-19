use super::*;
use crate::blacktop_outfits::{
    self,
    tests::{BASE, Fixture, game},
};
fn choice() -> Choice {
    let f = Fixture::selection();
    let p = blacktop_outfits::capture(&f, &game(), &[])
        .unwrap()
        .players
        .remove(0);
    Choice::new(
        "1".repeat(48),
        &p,
        vec![
            "top_white".into(),
            "pants_black".into(),
            "acc_sock_white".into(),
        ],
    )
    .unwrap()
}
fn matched() -> (Fixture, Choice) {
    let mut f = Fixture::selection();
    let c = choice();
    f.d(BASE + 0xAC77660 + 64, 0xB4D37B22);
    f.d(BASE + 0xAEB69D0 + 0xEA8, 1);
    f.map(BASE + MATCH_TABLE, 30 * 0x50);
    let record = BASE + MATCH_TABLE;
    let copy = 0x800000000;
    let manager = 0x900000000;
    let player = 0xA00000000;
    let desc = player + 0xF20;
    let array = 0xB00000000;
    f.map(copy, 0x520);
    f.map(manager, 0xC0);
    f.map(player, 0x1400);
    f.map(array, 16);
    f.map(0xC00000000, 1024);
    f.q(record, BASE + 0x75C0D58);
    f.q(record + 0x10, manager);
    f.q(record + 0x30, copy);
    f.q(copy + 0x500, c.roster);
    f.q(copy + 0x518, manager);
    f.q(manager, BASE + 0x6820088);
    f.q(manager + 0x98, player);
    f.q(player, BASE + 0x682C528);
    f.q(desc, BASE + 0x682DB58);
    f.q(desc + 0xB8, desc + 0x378);
    f.d(desc + 0xD0, 2);
    f.q(desc + 0xC8, array);
    f.q(array, 0xC00000000);
    f.q(array + 8, 0xC00000200);
    f.text(0xC00000000, "old_top");
    f.text(0xC00000200, "old_bottom");
    (f, c)
}
#[test]
fn v3_plans_match_original_powershell_bytes_and_rejections() {
    let oracle: serde_json::Value =
        serde_json::from_str(include_str!("../assets/outfit-plan-regressions.json")).unwrap();
    let game: Identity = serde_json::from_value(oracle["game"].clone()).unwrap();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 15);
    for case in oracle["cases"].as_array().unwrap() {
        let rows = case["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let mut v = v.clone();
                v.as_object_mut().unwrap().remove("status");
                serde_json::from_value::<Choice>(v).unwrap()
            })
            .collect::<Vec<_>>();
        let result = encode(&game, &rows);
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if let Ok(actual) = result {
            let hex = case["bytes"].as_str().unwrap();
            let expected = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{}", case["name"]);
        }
    }
    let mut bad = choice();
    bad.items[0] = "NO SHIRT ".into();
    assert!(encode(&game, &[bad]).is_err());
    let mut bad = choice();
    bad.choice_id = "invalid".into();
    assert!(encode(&game, &[bad]).is_err());
}
#[test]
fn choices_replace_displaced_slots_and_durably_consume_before_any_replay() {
    let mut ledger = Ledger::default();
    let first = choice();
    ledger.record(first.clone()).unwrap();
    let mut displaced = first.clone();
    displaced.choice_id = "2".repeat(48);
    displaced.roster += 0x4F8;
    ledger.record(displaced.clone()).unwrap();
    assert_eq!(ledger.choices, [displaced.clone()]);
    assert!(ledger.fresh(&[first.roster]).unwrap().is_empty());
    let frozen = ledger.fresh(&[displaced.roster]).unwrap();
    assert!(
        ledger
            .consume(&frozen, |_| anyhow::bail!("durable store failed"))
            .is_err()
    );
    assert!(ledger.consumed.is_empty());
    ledger
        .consume(&frozen, |next| {
            assert!(next.consumed.contains(&displaced.choice_id));
            Ok(())
        })
        .unwrap();
    assert!(ledger.fresh(&[displaced.roster]).unwrap().is_empty());
    assert!(
        ledger
            .consume(&frozen, |_| panic!("must not persist twice"))
            .is_err()
    );
    let restored: Ledger = serde_json::from_slice(&serde_json::to_vec(&ledger).unwrap()).unwrap();
    assert_eq!(restored, ledger);
    ledger.leave_blacktop();
    assert!(ledger.choices.is_empty());
    assert_eq!(ledger.consumed.len(), 1);
    assert!(ledger.record(displaced.clone()).is_err());
    displaced.choice_id = "3".repeat(48);
    ledger.record(displaced.clone()).unwrap();
    assert_eq!(ledger.fresh(&[displaced.roster]).unwrap().len(), 1);
    ledger.restore(displaced.roster, displaced.slot);
    assert!(ledger.choices.is_empty());
    assert_eq!(ledger.consumed.len(), 1);
}
#[test]
fn frozen_choice_changes_and_ambiguous_or_exhausted_ledgers_are_rejected() {
    let mut ledger = Ledger::default();
    let c = choice();
    ledger.record(c.clone()).unwrap();
    let mut changed = c.clone();
    changed.items[0] = "other".into();
    assert!(
        ledger
            .consume(&[changed], |_| panic!("changed snapshot cannot persist"))
            .is_err()
    );
    assert!(ledger.fresh(&[c.roster, c.roster]).is_err());
    let mut bad = ledger.clone();
    bad.choices.push(c.clone());
    assert!(bad.validate().is_err());
    let mut full = ledger.clone();
    full.consumed = (0..MAX_CONSUMED).map(|i| format!("{i:048x}")).collect();
    let mut fresh = c;
    fresh.choice_id = "f".repeat(48);
    full.choices = vec![fresh.clone()];
    assert!(
        full.consume(&[fresh], |_| panic!("exhausted session cannot persist"))
            .is_err()
    );
}
#[test]
fn readiness_requires_unique_idle_match_managers_and_stable_original_descriptions() {
    let (mut f, c) = matched();
    let rows = capture(&f, &[c.clone()]).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].items, ["old_top", "old_bottom"]);
    f.put(rows[0].manager + 0xB8, &[1]);
    assert!(capture(&f, &[c.clone()]).is_err());
    let (mut f, c) = matched();
    f.d(rows[0].description + 0xD0, 4);
    assert!(capture(&f, &[c]).is_err());
    let (mut f, c) = matched();
    f.q(BASE + MATCH_TABLE + 0x50, BASE + 0x75C0D58);
    f.q(BASE + MATCH_TABLE + 0x50 + 0x30, rows[0].roster_copy);
    assert!(capture(&f, &[c]).is_err());
    let (mut f, c) = matched();
    f.q(BASE + MATCH_TABLE + 0x30, u64::MAX);
    assert!(capture(&f, &[c]).is_err());
    let (mut f, c) = matched();
    f.tear = Some(rows[0].description + 0xC8);
    assert!(capture(&f, &[c]).is_err());
    let (mut f, c) = matched();
    f.d(BASE + 0xAC77660 + 64, 0x079964C0);
    assert!(capture(&f, &[c]).is_err());
}
#[test]
fn stability_requires_three_spaced_identical_snapshots_not_three_fast_polls() {
    let (f, c) = matched();
    let players = capture(&f, &[c.clone()]).unwrap();
    let mut stable = Stable::default();
    for at in [0, 100, 249, 250, 499] {
        assert!(!stable.observe(at, players.clone(), vec![c.clone()]));
    }
    assert!(stable.observe(500, players.clone(), vec![c.clone()]));
    let mut changed = c;
    changed.choice_id = "a".repeat(48);
    assert!(!stable.observe(750, players.clone(), vec![changed.clone()]));
    assert!(!stable.observe(1000, players.clone(), vec![changed.clone()]));
    assert!(stable.observe(1250, players.clone(), vec![changed.clone()]));
    assert!(!stable.observe(100, players.clone(), vec![changed.clone()]));
    stable.reset();
    assert!(!stable.observe(1500, players, vec![changed]));
}
