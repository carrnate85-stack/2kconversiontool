use super::*;
use std::collections::BTreeMap;

fn oracle() -> Value {
    serde_json::from_str(include_str!("../assets/editor-services-regressions.json")).unwrap()
}
fn hex(v: &Value) -> Vec<u8> {
    unhex(v.as_str().unwrap())
}
fn staff_row(slot: usize) -> Vec<u8> {
    let mut b: Vec<u8> = (0..STAFF_STRIDE).map(|i| (i * 37 % 251) as u8).collect();
    b[0x50..0xA0].fill(0);
    for (i, w) in "Becky".encode_utf16().enumerate() {
        b[0x50 + i * 2..0x52 + i * 2].copy_from_slice(&w.to_le_bytes());
    }
    for (i, w) in "Hammon".encode_utf16().enumerate() {
        b[0x78 + i * 2..0x7A + i * 2].copy_from_slice(&w.to_le_bytes());
    }
    b[0x182] = (b[0x182] & 0xE0) | slot as u8;
    // Keep systems inside WPF's valid set so range checks are meaningful.
    let systems = (u32le(&b, 0x130) & !(0xFF << 20)) | (2 << 20) | (15 << 24);
    b[0x130..0x134].copy_from_slice(&systems.to_le_bytes());
    b
}

#[test]
fn staff_decode_and_patch_match_the_wpf_javascript_oracle() {
    let o = oracle();
    let (mut decoded, mut patched, mut refused) = (0, 0, 0);
    for c in o["staff"].as_array().unwrap() {
        let raw = hex(&c["raw"]);
        let (slot, index) = (
            c["slot"].as_u64().unwrap() as usize,
            c["index"].as_u64().unwrap() as usize,
        );
        let result = decode_staff(&raw, slot, index);
        assert_eq!(result.is_ok(), c["decodedOk"] == true, "decode {c:?}");
        if let (Ok(mut v), false) = (result, c["decoded"].is_null()) {
            v.as_object_mut().unwrap().remove("controls");
            let mut expected = c["decoded"].clone();
            expected.as_object_mut().unwrap().remove("controls");
            assert_eq!(v, expected);
            decoded += 1;
        }
        let after = patch_staff(&raw, &c["values"]);
        assert_eq!(
            after.is_ok(),
            !c["after"].is_null(),
            "patch {} -> {:?}",
            c["values"],
            after.as_ref().err()
        );
        match after {
            Ok(a) => {
                assert_eq!(a, hex(&c["after"]));
                patched += 1;
            }
            Err(_) => refused += 1,
        }
    }
    assert!(
        decoded >= 20 && patched >= 50 && refused >= 100,
        "{decoded} {patched} {refused}"
    );
}
#[test]
fn every_staff_control_round_trips_inside_wpf_masks_and_ranges() {
    let base = staff_row(4);
    let masks = staff_masks();
    for f in controls() {
        let key = f["key"].as_str().unwrap();
        let value = if f["text"] == true {
            json!("Rust QA")
        } else {
            let bits = f["bits"].as_u64().unwrap();
            let max = f["max"].as_i64().unwrap_or((1 << bits) - 1);
            let max = f["choices"]
                .as_array()
                .map_or(max, |c| max.min(c.len() as i64 - 1));
            let current = decode_staff(&base, 4, 9).unwrap()["values"][key]
                .as_i64()
                .unwrap();
            json!(if current == max {
                f["min"].as_i64().unwrap_or(0)
            } else {
                max
            })
        };
        let after = patch_staff(&base, &json!({ key: value })).unwrap();
        validate_staff_patch(&base, &after).unwrap_or_else(|e| panic!("{key}: {e:#}"));
        assert_eq!(
            decode_staff(&after, 4, 9).unwrap()["values"][key],
            value,
            "{key}"
        );
        for i in 0..STAFF_STRIDE {
            assert_eq!(
                (base[i] ^ after[i]) & !masks[i],
                0,
                "{key} escaped its mask at {i:#x}"
            );
        }
    }
}
#[test]
fn staff_identity_links_names_and_ranges_follow_wpf_team_staff_patch() {
    let base = staff_row(0);
    for at in [0x18, 0x120, 0x121, 0x182, 0xA0] {
        let mut after = base.clone();
        after[at] ^= 1;
        assert!(validate_staff_patch(&base, &after).is_err(), "{at:#x}");
    }
    let mut blank = base.clone();
    blank[0x50..0xA0].fill(0);
    assert!(validate_staff_patch(&base, &blank).is_err());
    // Systems accept 0..7 and 15 (unset) only.
    for (value, ok) in [(7u32, true), (15, true), (9, false)] {
        let mut after = base.clone();
        let w = (u32le(&after, 0x130) & !(15 << 20)) | (value << 20);
        after[0x130..0x134].copy_from_slice(&w.to_le_bytes());
        assert_eq!(
            validate_staff_patch(&base, &after).is_ok(),
            ok,
            "system {value}"
        );
    }
    assert!(validate_staff_values(&json!({"uniqueId": 1})).is_err());
    assert!(validate_staff_values(&json!({"firstName": "a\u{1}"})).is_err());
    assert!(validate_staff_values(&json!({"firstName": "x".repeat(20)})).is_err());
    assert!(validate_staff_values(&json!({"preferredSystemId": 15})).is_ok());
    assert!(validate_staff_values(&json!({"playbookId": 369})).is_err());
}
#[test]
fn staff_plans_need_an_unchanged_owned_record_and_matching_expectations() {
    let base = staff_row(3);
    let current = decode_staff(&base, 3, 12).unwrap();
    let input = |values: Value| {
        let expected: Map<String, Value> = values
            .as_object()
            .unwrap()
            .keys()
            .map(|k| (k.clone(), current["values"][k].clone()))
            .collect();
        json!({"values": values, "expected": expected})
    };
    let plan = plan_staff(
        &input(json!({"lastName":"Kerr","BenchDepth":99})),
        &base,
        &base,
        12,
        3,
        true,
    )
    .unwrap();
    assert_eq!(plan.kind, Kind::Staff);
    assert!(plan.patches.iter().all(|p| p.after.len() <= 40));
    assert!(plan.changed.contains(&"lastName".to_string()));
    assert_eq!(plan.expected(plan.patches.len()).unwrap(), plan.after);
    assert_eq!(
        decode_staff(&plan.after, 3, 12).unwrap()["lastName"],
        "Kerr"
    );
    let mut live = base.clone();
    live[0x158] ^= 1;
    assert!(
        plan_staff(
            &input(json!({"lastName":"Kerr"})),
            &base,
            &live,
            12,
            3,
            true
        )
        .is_err()
    );
    let shared = plan_staff(
        &input(json!({"lastName":"Kerr"})),
        &base,
        &base,
        12,
        3,
        false,
    )
    .unwrap_err();
    assert!(shared.to_string().contains("shared"));
    let mut stale = input(json!({"lastName":"Kerr"}));
    stale["expected"]["lastName"] = json!("Other");
    assert!(plan_staff(&stale, &base, &base, 12, 3, true).is_err());
    let noop = plan_staff(
        &input(json!({"lastName":"Hammon"})),
        &base,
        &base,
        12,
        3,
        true,
    )
    .unwrap();
    assert!(noop.patches.is_empty());
}
#[test]
fn season_plans_and_awards_match_the_wpf_javascript_oracle() {
    let o = oracle();
    let (mut planned, mut refused) = (0, 0);
    for c in o["seasons"].as_array().unwrap() {
        let record = hex(&c["record"]);
        assert_eq!(decode_totals(&record).unwrap(), c["totals"]);
        assert_eq!(decode_awards(&record).unwrap(), c["awards"]);
        let result = plan_season_record(&record, &c["changes"]);
        assert_eq!(
            result.is_ok(),
            !c["after"].is_null(),
            "{} -> {:?} / {}",
            c["changes"],
            result.as_ref().err(),
            c["error"]
        );
        if let Ok((after, warnings)) = result {
            assert_eq!(after, hex(&c["after"]), "{}", c["changes"]);
            assert_eq!(json!(warnings), c["warnings"]);
            planned += 1;
        } else {
            refused += 1;
        }
    }
    assert!(planned >= 80 && refused >= 100, "{planned} {refused}");
}
#[test]
fn season_patches_touch_only_wpf_season_fields_and_keep_record_identity() {
    let o = oracle();
    let mut checked = 0;
    for c in o["seasons"].as_array().unwrap() {
        let record = hex(&c["record"]);
        let Ok((after, _)) = plan_season_record(&record, &c["changes"]) else {
            continue;
        };
        // WPF's C# writer re-validates; a JS plan it would refuse stays refused.
        match plan_season(5, &record, after.clone()) {
            Ok(plan) => {
                assert_eq!(plan.after, after);
                assert_eq!(
                    after[..0x14],
                    record[..0x14],
                    "team pointers/year untouched"
                );
                checked += 1;
            }
            Err(e) => assert!(validate_season_patch(&record, &after).is_err(), "{e:#}"),
        }
    }
    assert!(checked > 50);
    let base = hex(&o["seasons"][0]["record"]);
    let mut foreign = base.clone();
    foreign[0x11] ^= 1;
    assert!(validate_season_patch(&base, &foreign).is_err());
    let mut both = base.clone();
    both[39] |= 128;
    both[27] |= 16;
    let mut none = base.clone();
    none[39] &= 127;
    none[27] &= !16;
    assert!(validate_season_patch(&none, &both).is_err());
    assert_eq!(season_masks()[0x11], 0);
    assert!(shared_warning(&[1]).unwrap().is_none());
    assert!(
        shared_warning(&[1, 2])
            .unwrap()
            .unwrap()
            .contains("2 player entries")
    );
    assert!(shared_warning(&[]).is_err());
}
struct Image(BTreeMap<u64, u8>);
impl Memory for Image {
    fn base(&self) -> u64 {
        0x1_4000_0000
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        (0..len)
            .map(|i| self.0.get(&(at + i as u64)).copied().context("Unmapped"))
            .collect()
    }
}
#[test]
fn services_profile_refuses_other_images() {
    assert!(verify_profile(&Image(BTreeMap::new())).is_err());
    assert_eq!(ANCHORS.len(), 13);
    assert_eq!(
        policy().masks.iter().map(|b| b.count_ones()).sum::<u32>(),
        1258
    );
}
#[test]
#[ignore]
fn exact_version_two_services_profile_is_verified_from_disk_only() {
    use std::{
        cell::RefCell,
        fs::File,
        io::{Read, Seek, SeekFrom},
    };
    let path = std::path::PathBuf::from(
        std::env::var_os("C27_ROSTER_GAME_EXE").expect("Explicit disk image required"),
    );
    let mut file = File::open(path).unwrap();
    let mut header = vec![0; 4096];
    file.read_exact(&mut header).unwrap();
    let pe = u32::from_le_bytes(header[60..64].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(header[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let opt = u16::from_le_bytes(header[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let base = u64::from_le_bytes(header[pe + 48..pe + 56].try_into().unwrap());
    let sections: Vec<(u64, u64, u64)> = (0..count)
        .map(|i| {
            let at = pe + 24 + opt + i * 40;
            let r = |o: usize| {
                u32::from_le_bytes(header[at + o..at + o + 4].try_into().unwrap()) as u64
            };
            (r(12), r(16), r(20))
        })
        .collect();
    struct Disk(RefCell<File>, Vec<u8>, u64, Vec<(u64, u64, u64)>);
    impl Memory for Disk {
        fn base(&self) -> u64 {
            self.2
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let rva = at - self.2;
            if rva == 0 && len == 4096 {
                return Ok(self.1.clone());
            }
            let (start, _, raw) = self
                .3
                .iter()
                .find(|(s, n, _)| rva >= *s && rva + len as u64 <= s + n)
                .context("Not disk-backed")?;
            let mut b = vec![0; len];
            let mut f = self.0.borrow_mut();
            f.seek(SeekFrom::Start(raw + rva - start))?;
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    verify_profile(&Disk(RefCell::new(file), header, base, sections)).unwrap();
    println!(
        "{}",
        json!({"pass":true,"anchors":ANCHORS.len(),"hashedSpans":SPANS.len(),"gameProcessAccess":false})
    );
}

fn rotation_players(team: u64) -> Vec<u8> {
    let mut rows = Vec::new();
    for i in 0..8u16 {
        let mut r = vec![0u8; profile::PLAYER_STRIDE];
        for (k, w) in format!("P{i}").encode_utf16().enumerate() {
            r[k * 2..k * 2 + 2].copy_from_slice(&w.to_le_bytes());
        }
        r[0x128..0x12A].copy_from_slice(&(1000 + i).to_le_bytes());
        let owner = if i < 6 { team } else { team + 0x1550 };
        r[0x60..0x68].copy_from_slice(&owner.to_le_bytes());
        rows.extend(r);
    }
    rows
}
#[test]
fn rotations_follow_wpf_membership_and_five_distinct_member_rule() {
    let team = 0x2_2000_0000u64;
    let members = rotation_members(team, &rotation_players(team)).unwrap();
    assert_eq!(members.len(), 6);
    assert_eq!(members[0], (1000, 0, "P0".into()));
    assert!(
        rotation_members(team + 8, &rotation_players(team)).is_err(),
        "fewer than five members"
    );
    let before = vec![0u8; ROTATION_BYTES];
    let mut rows: Vec<Vec<u64>> = vec![vec![0; 5]; 48];
    rows[3] = vec![1000, 1001, 1002, 1003, 1004];
    let after = pack_rotations(&before, &json!(rows), &members).unwrap();
    assert_eq!(
        rotation_rows(&after).unwrap()[3],
        [1000, 1001, 1002, 1003, 1004]
    );
    rows[4] = vec![1000, 1000, 1002, 1003, 1004];
    assert!(
        pack_rotations(&before, &json!(rows), &members).is_err(),
        "duplicate player"
    );
    rows[4] = vec![1000, 1001, 1002, 1003, 1006];
    assert!(
        pack_rotations(&before, &json!(rows), &members).is_err(),
        "other team's player"
    );
    rows[4] = vec![0; 5];
    assert!(pack_rotations(&before, &json!(rows[..47]), &members).is_err());
    let mut team_row = vec![7u8; profile::TEAM_STRIDE];
    team_row[ROTATIONS..ROTATIONS + ROTATION_BYTES].fill(0);
    let plan = plan_rotations(2, &team_row, &after).unwrap();
    assert_eq!(plan.kind, Kind::Team);
    assert!(
        plan.patches.iter().all(
            |p| p.offset >= ROTATIONS && p.offset + p.after.len() <= ROTATIONS + ROTATION_BYTES
        )
    );
    assert_eq!(plan.after[ROTATIONS + 30..ROTATIONS + 40], after[30..40]);
    assert_eq!(plan.after[..ROTATIONS], team_row[..ROTATIONS]);
}

#[test]
fn limited_season_import_counts_all_slots_not_unique_players() {
    let stride = crate::roster_v2_write::Kind::Player.stride();
    let mut players = vec![0; stride * 2];
    for row in players.chunks_exact_mut(stride) {
        row[PLAYER_SEASONS..PLAYER_SEASONS + 66].fill(255);
    }
    players[PLAYER_SEASONS..PLAYER_SEASONS + 2].copy_from_slice(&7i16.to_le_bytes());
    assert_eq!(season_reference_occurrences(&players, 7).unwrap(), 1);
    let extra = PLAYER_SEASONS + 25 * 2;
    players[extra..extra + 2].copy_from_slice(&7i16.to_le_bytes());
    assert_eq!(season_reference_occurrences(&players, 7).unwrap(), 2);
    players[stride + PLAYER_PRIVATE_SEASONS] = 1;
    let other = stride + PLAYER_SEASONS;
    players[other..other + 2].copy_from_slice(&7i16.to_le_bytes());
    assert_eq!(season_reference_occurrences(&players, 7).unwrap(), 2);
    players[stride + PLAYER_PRIVATE_SEASONS] = 0;
    assert_eq!(season_reference_occurrences(&players, 7).unwrap(), 3);
    assert!(season_reference_occurrences(&players[..stride - 1], 7).is_err());
    let mut raw = vec![0; 64];
    raw[60..64].copy_from_slice(&(1u32 << 13 | 0xF0000000).to_le_bytes());
    assert_eq!(season_reference_count(&raw).unwrap(), 1);
    raw[60..64].copy_from_slice(&(2u32 << 13 | 0xF0000000).to_le_bytes());
    assert_eq!(season_reference_count(&raw).unwrap(), 2);
}

#[test]
fn final_rotation_membership_refusal_rolls_back_before_verified_completion() {
    use crate::roster_v2_write as w;
    use std::cell::{Cell, RefCell};
    let mut raw = vec![0; Kind::Team.stride()];
    raw[ROTATION_FLAG] = 1;
    let team = 0x220000680u64;
    let players = rotation_players(team);
    let members = rotation_members(team, &players).unwrap();
    let expected = member_digest(&members);
    let mut rows = vec![vec![0; 5]; 48];
    rows[0] = vec![1000, 1001, 1002, 1003, 1004];
    let block = pack_rotations(
        &raw[ROTATIONS..ROTATIONS + ROTATION_BYTES],
        &json!(rows),
        &members,
    )
    .unwrap();
    let plan = plan_rotations(0, &raw, &block).unwrap();
    let owner = w::Owner {
        game: crate::session_safety::Identity {
            pid: 7,
            started: "42".into(),
            path: r"C:\QA\NBA2K27.exe".into(),
        },
        session: "a".repeat(48),
        image: 0x140000000,
        root: 0x240000000,
        owner: 0x220000050,
        region_base: 0x220000000,
        region_len: 0x800000,
        kind: Kind::Team,
        table: team,
        count: 30,
        index: 0,
        identity: w::identity(Kind::Team, &raw).unwrap(),
        proof: w::proof_tag().into(),
    };
    struct F {
        owner: w::Owner,
        raw: RefCell<Vec<u8>>,
        writes: Cell<usize>,
    }
    impl w::Access for F {
        fn owner(&self) -> Result<w::Owner> {
            Ok(self.owner.clone())
        }
        fn capture(&self) -> Result<Vec<u8>> {
            Ok(self.raw.borrow().clone())
        }
        fn write(&self, at: usize, b: &[u8]) -> Result<()> {
            self.raw.borrow_mut()[at..at + b.len()].copy_from_slice(b);
            self.writes.set(self.writes.get() + 1);
            Ok(())
        }
        fn checkpoint(&self) -> Result<()> {
            Ok(())
        }
    }
    let fake = F {
        owner: owner.clone(),
        raw: RefCell::new(raw.clone()),
        writes: Cell::new(0),
    };
    let mut moved = players.clone();
    moved[0x60..0x68].copy_from_slice(&(team + 0x1550).to_le_bytes());
    let final_checks = Cell::new(0);
    let completed = RefCell::new(Vec::new());
    let result = w::apply(&fake, &owner, &plan, |intent| {
        if !intent.pending && intent.result == "applied and read back verified" {
            final_checks.set(final_checks.get() + 1);
            anyhow::ensure!(
                member_digest(&rotation_members(team, &moved)?) == expected,
                "member moved"
            );
        }
        completed
            .borrow_mut()
            .push((intent.pending, intent.result.clone()));
        Ok(())
    });
    assert!(result.is_err());
    assert_eq!(final_checks.get(), 1);
    assert_eq!(*fake.raw.borrow(), raw);
    assert!(fake.writes.get() >= 2);
    assert!(
        !completed
            .borrow()
            .iter()
            .any(|(_, s)| s == "applied and read back verified")
    );
    assert!(
        completed
            .borrow()
            .iter()
            .any(|(p, s)| !*p && s == "failed; original owned values verified")
    );
}
