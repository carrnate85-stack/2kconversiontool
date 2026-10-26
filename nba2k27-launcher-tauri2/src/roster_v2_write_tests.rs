use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

const REGION: u64 = 0x2_2000_0000;

fn owner_for(kind: Kind, index: usize, raw: &[u8]) -> Owner {
    Owner {
        game: Identity {
            pid: 7,
            started: "42".into(),
            path: r"C:\QA\NBA2K27.exe".into(),
        },
        session: "a".repeat(48),
        image: 0x1_4000_0000,
        root: 0x2_4000_0000,
        owner: REGION + 0x50,
        region_base: REGION,
        region_len: 0x80_0000,
        kind,
        table: REGION + 0x680,
        count: if kind == Kind::Team { 30 } else { 100 },
        index,
        identity: identity(kind, raw).unwrap(),
        proof: proof_tag().into(),
    }
}

/// Fixture record memory behind the writer's Access contract. Identity is
/// recomputed from live bytes, like the native lease.
struct Fake {
    owner: RefCell<Owner>,
    raw: RefCell<Vec<u8>>,
    writes: Cell<usize>,
    reads: Cell<usize>,
    fail_read_at: Option<usize>,
    fail_at: Option<usize>,
    fail_after_write: bool,
    partial: bool,
    foreign: bool,
    change_owner: bool,
    cancel_at: Option<usize>,
    expired_at: Option<usize>,
    recovering: Cell<bool>,
    recovery_checks: Cell<usize>,
    recovery_limit: Option<usize>,
}
impl Fake {
    fn new(owner: &Owner, raw: &[u8]) -> Self {
        Self {
            owner: RefCell::new(owner.clone()),
            raw: RefCell::new(raw.to_vec()),
            writes: Cell::new(0),
            reads: Cell::new(0),
            fail_read_at: None,
            fail_at: None,
            fail_after_write: false,
            partial: false,
            foreign: false,
            change_owner: false,
            cancel_at: None,
            expired_at: None,
            recovering: Cell::new(false),
            recovery_checks: Cell::new(0),
            recovery_limit: None,
        }
    }
}
impl Fake {
    fn retained_check(&self) -> Result<()> {
        if self.recovering.get() {
            let n = self.recovery_checks.get() + 1;
            self.recovery_checks.set(n);
            ensure!(
                self.recovery_limit.is_none_or(|limit| n <= limit),
                "Synthetic recovery deadline expired"
            );
        } else {
            ensure!(
                self.cancel_at.is_none_or(|n| self.writes.get() < n),
                "Synthetic close cancellation"
            );
            ensure!(
                self.expired_at.is_none_or(|n| self.writes.get() < n),
                "Synthetic forward budget expired"
            );
        }
        Ok(())
    }
}
impl Access for Fake {
    fn begin_recovery(&self) -> Result<()> {
        ensure!(!self.recovering.replace(true), "Recovery cannotrenew");
        Ok(())
    }

    fn owner(&self) -> Result<Owner> {
        self.retained_check()?;
        let mut o = self.owner.borrow().clone();
        o.identity = identity(o.kind, &self.raw.borrow())?;
        Ok(o)
    }
    fn capture(&self) -> Result<Vec<u8>> {
        self.retained_check()?;
        let n = self.reads.get() + 1;
        self.reads.set(n);
        ensure!(self.fail_read_at != Some(n), "Synthetic read-back failure");
        Ok(self.raw.borrow().clone())
    }
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        self.retained_check()?;
        let n = self.writes.get() + 1;
        self.writes.set(n);
        let fail = self.fail_at == Some(n);
        if fail && !self.fail_after_write {
            bail!("Synthetic write failure");
        }
        let kind = self.owner.borrow().kind;
        let mut raw = self.raw.borrow_mut();
        raw[offset..offset + bytes.len()].copy_from_slice(bytes);
        if fail && self.partial {
            raw[offset] ^= 0x80;
        }
        if fail && self.foreign {
            match kind {
                Kind::Player => raw[0x128] ^= 1,
                Kind::Team => raw[TEAM_ID] ^= 1,
                Kind::Uniform => raw[0x50] ^= 1,
                Kind::Staff => raw[0x120] ^= 1,
                Kind::Season => raw[0x11] ^= 1,
            }
        }
        if fail && self.change_owner {
            self.owner.borrow_mut().owner += 8;
        }
        ensure!(!fail, "Synthetic reported failure after write");
        Ok(())
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            !self.recovering.get(),
            "Recovery neverallowsnewforwardwrites"
        );
        self.retained_check()?;
        ensure!(
            self.cancel_at.is_none_or(|n| self.writes.get() < n),
            "Synthetic close cancellation"
        );
        Ok(())
    }
}
fn text(row: &mut [u8], at: usize, value: &str) {
    for (i, w) in value.encode_utf16().enumerate() {
        row[at + i * 2..at + i * 2 + 2].copy_from_slice(&w.to_le_bytes());
    }
}
fn set(row: &mut [u8], bits: Bits, value: u64) {
    let start = bits.0 * 8 + bits.1 as usize;
    for i in 0..bits.2 as usize {
        let (byte, bit) = ((start + i) / 8, 1u8 << ((start + i) % 8));
        if (value >> i) & 1 == 1 {
            row[byte] |= bit;
        } else {
            row[byte] &= !bit;
        }
    }
}
/// Deterministic noisy record: unrelated bytes must survive every write.
fn noisy(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}
fn player_row() -> Vec<u8> {
    let mut row = noisy(profile::PLAYER_STRIDE, 0x9e37_79b9_7f4a_7c15);
    row[..80].fill(0);
    text(&mut row, 0, "Player");
    text(&mut row, 40, "Test");
    set(&mut row, bio("BirthYear").unwrap(), 1990);
    set(&mut row, bio("BirthMonth").unwrap(), 6);
    set(&mut row, bio("BirthDay").unwrap(), 15);
    set(&mut row, bio("Position").unwrap(), 2);
    set(&mut row, bio("SecondaryPosition").unwrap(), 3);
    set(&mut row, bio("JerseyNumber").unwrap(), 23);
    row
}
fn player_cached(row: &[u8]) -> Value {
    let ratings: Vec<u64> = roster_snapshot::rating_fields()
        .iter()
        .map(|f| get(row, (f.offset, f.shift, f.bits)).unwrap())
        .collect();
    let p = rating("PotentialRaw").unwrap();
    let potential = get(row, (p.offset, p.shift, p.bits)).unwrap();
    let b = |k| get(row, bio(k).unwrap()).unwrap();
    json!({"source":"live-roster","recordIndex":3,"firstName":utf16(row,40,40).unwrap(),"lastName":utf16(row,0,40).unwrap(),
        "cyberfaceId":b("GraphicId"),"positionId":b("Position"),"secondaryPositionId":b("SecondaryPosition"),"jerseyNumberRaw":b("JerseyNumber"),
        "potentialRaw":potential,"potential":110u64.min(25+(potential+1)/3),"ratingsRaw":ratings,"bioLayoutVerified":true,
        "birthYear":b("BirthYear"),"birthMonth":b("BirthMonth"),"birthDay":b("BirthDay"),"yearsPro":b("YearsPro"),
        "weightLbs":null,"heightCm":null,"wingspanCm":null})
}
/// Exactly what the unchanged WPF form sends for a Version 2 player whose
/// measurements are unmapped (blank weight → 0, shim height/wingspan → null).
fn player_input(cached: &Value) -> Value {
    json!({"recordIndex":3,"expectedFirstName":cached["firstName"],"expectedLastName":cached["lastName"],"expectedCyberfaceId":cached["cyberfaceId"],
        "ratingEdits":[],"firstName":cached["firstName"],"lastName":cached["lastName"],"cyberfaceId":cached["cyberfaceId"],
        "positionId":cached["positionId"],"secondaryPositionId":cached["secondaryPositionId"],"jerseyNumberRaw":cached["jerseyNumberRaw"],
        "weightLbs":0,"heightCm":null,"wingspanCm":null,"potential":cached["potential"]})
}
fn commit(kind: Kind, index: usize, plan: &Plan) -> Vec<u8> {
    let o = owner_for(kind, index, &plan.before);
    let m = Fake::new(&o, &plan.before);
    let records = RefCell::new(Vec::<Intent>::new());
    let c = apply(&m, &o, plan, |i| {
        if records.borrow().is_empty() {
            assert_eq!(m.writes.get(), 0, "intent must precede the first write");
        }
        records.borrow_mut().push(i.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(c.report["readBackVerified"], true);
    assert_eq!(c.read_back, *m.raw.borrow());
    if !plan.patches.is_empty() {
        assert!(records.borrow()[0].pending && !records.borrow().last().unwrap().pending);
    }
    c.read_back
}
/// Only `bits` may differ between the records.
fn only(before: &[u8], after: &[u8], fields: &[Bits]) {
    let mut allowed = vec![0u8; before.len()];
    for &(o, s, b) in fields {
        for i in 0..b as usize {
            let g = o * 8 + s as usize + i;
            allowed[g / 8] |= 1 << (g % 8);
        }
    }
    for i in 0..before.len() {
        assert_eq!(
            (before[i] ^ after[i]) & !allowed[i],
            0,
            "byte {i:#x} changed"
        );
    }
}

#[test]
fn write_proof_is_complete_and_matches_the_legacy_writer_layouts() {
    let p = proof().unwrap();
    assert_eq!(p.uniform.controls.len(), 52);
    assert_eq!(p.uniform.display_fields.len(), 81);
    assert_eq!(p.team_text.len(), 5);
    assert_eq!(proof_tag().len(), 64);
    let mut asset: Value =
        serde_json::from_str(include_str!("../assets/roster-v2-write-proof.json")).unwrap();
    for mutate in 0..7 {
        let mut bad = asset.clone();
        match mutate {
            0 => bad["uniform"]["controls"][0]["offset"] = json!(0x110),
            1 => bad["uniform"]["controls"][3]["matches"] = json!(false),
            2 => bad["teamText"][0]["capacityUnits"] = json!(0x18),
            3 => bad["uniformSlots"]["offset"] = json!(0xDE8),
            4 => bad["imageSha256"] = json!("0".repeat(64)),
            5 => {
                bad["uniform"]["controls"].as_array_mut().unwrap().pop();
            }
            _ => bad["uniform"]["mismatches"] = json!([{"request": "typeId"}]),
        }
        let parsed: Proof = serde_json::from_value(bad).unwrap();
        assert!(check_proof(&parsed).is_err(), "mutation {mutate}");
    }
    asset["anchors"][0]["hex"] = json!("zz");
    assert!(check_proof(&serde_json::from_value(asset).unwrap()).is_err());
    // Team text: the WPF buffer always fits the proven V2 capacity.
    for (_, offset, bytes, capacity) in TEAM_TEXT {
        assert!(bytes <= capacity * 2 && offset + capacity * 2 <= profile::TEAM_STRIDE);
    }
}
#[test]
fn player_layout_is_the_proven_version_two_serializer_layout() {
    // roster_v2_readonly proves all 455 bit positions against the V2 body;
    // the writer uses the same table, and the bio bits it writes.
    let proof: Value =
        serde_json::from_str(include_str!("../assets/roster-v2-field-proof.json")).unwrap();
    let rows = proof["rows"].as_array().unwrap();
    for (row, f) in rows.iter().zip(roster_snapshot::rating_fields()) {
        let e = &row["extracted"];
        assert_eq!(row["key"], f.key.as_str());
        assert_eq!(
            e["offset"].as_u64().unwrap() as usize * 8 + e["shift"].as_u64().unwrap() as usize,
            f.offset * 8 + f.shift as usize
        );
        assert_eq!(e["bits"].as_u64().unwrap() as u32, f.bits);
    }
    for (key, offset, shift, bits) in model::BIO {
        let m = proof["metadata"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["key"] == *key)
            .unwrap();
        assert_eq!(
            m["shape"],
            json!({"offset":offset,"shift":shift,"bits":bits})
        );
    }
}
#[test]
fn every_writable_player_rating_field_writes_only_its_own_bits() {
    let row = player_row();
    let cached = player_cached(&row);
    let mut tested = 0;
    for (index, f) in roster_snapshot::rating_fields().iter().enumerate() {
        let bits = (f.offset, f.shift, f.bits);
        let before = get(&row, bits).unwrap();
        let mut input = player_input(&cached);
        let value = if f.maximum <= 0 {
            0
        } else {
            (before + 1) % (f.maximum as u64 + 1)
        };
        input["ratingEdits"] = json!([{"index":index,"expected":before,"value":value}]);
        let result = plan_player(&input, &cached, &row);
        if f.maximum < 0 {
            assert!(result.is_err(), "read-only field {} was planned", f.key);
            continue;
        }
        if before > i32::MAX as u64 {
            assert!(result.is_err());
            continue;
        }
        let plan = result.unwrap_or_else(|e| panic!("{}: {e:#}", f.key));
        let after = commit(Kind::Player, 3, &plan);
        assert_eq!(get(&after, bits).unwrap(), value, "{}", f.key);
        only(&row, &after, &[bits]);
        tested += 1;
    }
    assert!(tested > 400, "only {tested} rating writers exercised");
}
#[test]
fn player_identity_bio_and_potential_writers_match_their_proven_bits() {
    let row = player_row();
    let cached = player_cached(&row);
    let cases: Vec<(Value, Vec<Bits>, Box<dyn Fn(&[u8])>)> = vec![
        (
            json!({"firstName":"🏀 Ünïcode"}),
            vec![
                (40, 0, 32),
                (44, 0, 32),
                (48, 0, 32),
                (52, 0, 32),
                (56, 0, 32),
                (60, 0, 32),
                (64, 0, 32),
                (68, 0, 32),
                (72, 0, 32),
                (76, 0, 32),
            ],
            Box::new(|r: &[u8]| assert_eq!(utf16(r, 40, 40).unwrap(), "🏀 Ünïcode")),
        ),
        (
            json!({"lastName":"Q"}),
            (0..10).map(|i| (i * 4, 0, 32)).collect(),
            Box::new(|r: &[u8]| assert_eq!(utf16(r, 0, 40).unwrap(), "Q")),
        ),
        (
            json!({"cyberfaceId":65535}),
            vec![bio("GraphicId").unwrap()],
            Box::new(|r: &[u8]| assert_eq!(get(r, bio("GraphicId").unwrap()).unwrap(), 65535)),
        ),
        (
            json!({"positionId":4,"secondaryPositionId":5,"jerseyNumberRaw":100}),
            vec![
                bio("Position").unwrap(),
                bio("SecondaryPosition").unwrap(),
                bio("JerseyNumber").unwrap(),
            ],
            Box::new(|r: &[u8]| {
                assert_eq!(get(r, bio("Position").unwrap()).unwrap(), 4);
                assert_eq!(get(r, bio("SecondaryPosition").unwrap()).unwrap(), 5);
                assert_eq!(get(r, bio("JerseyNumber").unwrap()).unwrap(), 100);
            }),
        ),
        (
            json!({"potential":110}),
            vec![(0x45C, 8, 8)],
            Box::new(|r: &[u8]| assert_eq!(get(r, (0x45C, 8, 8)).unwrap(), 254)),
        ),
        (
            json!({"birthDate":"2004-02-29","expectedBirthYear":1990,"expectedBirthMonth":6,"expectedBirthDay":15}),
            vec![
                bio("BirthYear").unwrap(),
                bio("BirthMonth").unwrap(),
                bio("BirthDay").unwrap(),
            ],
            Box::new(|r: &[u8]| {
                assert_eq!(get(r, bio("BirthYear").unwrap()).unwrap(), 2004);
                assert_eq!(get(r, bio("BirthMonth").unwrap()).unwrap(), 2);
                assert_eq!(get(r, bio("BirthDay").unwrap()).unwrap(), 29);
            }),
        ),
    ];
    for (changes, bits, verify) in cases {
        let mut input = player_input(&cached);
        for (k, v) in changes.as_object().unwrap() {
            input[k] = v.clone();
        }
        let plan = plan_player(&input, &cached, &row).unwrap();
        assert!(!plan.patches.is_empty(), "{changes}");
        assert!(plan.patches.iter().all(|p| p.after.len() <= 40));
        let after = commit(Kind::Player, 3, &plan);
        verify(&after);
        only(&row, &after, &bits);
    }
    let years = cached["yearsPro"].as_u64().unwrap();
    let mut input = player_input(&cached);
    input["yearsPro"] = json!((years + 1) % 32);
    input["expectedYearsPro"] = json!(years);
    let after = commit(
        Kind::Player,
        3,
        &plan_player(&input, &cached, &row).unwrap(),
    );
    assert_eq!(
        get(&after, bio("YearsPro").unwrap()).unwrap(),
        (years + 1) % 32
    );
    only(&row, &after, &[bio("YearsPro").unwrap()]);
    // Unchanged form: no intent, no write.
    let plan = plan_player(&player_input(&cached), &cached, &row).unwrap();
    assert!(plan.patches.is_empty());
    let o = owner_for(Kind::Player, 3, &row);
    let m = Fake::new(&o, &row);
    let r = apply(&m, &o, &plan, |_| bail!("No-op must not create an intent")).unwrap();
    assert_eq!(r.report["noChanges"], true);
    assert_eq!(m.writes.get(), 0);
}
#[test]
fn unmapped_player_fields_and_stale_baselines_are_refused_before_any_plan() {
    let row = player_row();
    let cached = player_cached(&row);
    for (key, value) in [
        ("heightCm", json!(203.2)),
        ("wingspanCm", json!(210)),
        ("fromIndex", json!(4)),
    ] {
        let mut input = player_input(&cached);
        input[key] = value;
        let e = plan_player(&input, &cached, &row).unwrap_err().to_string();
        assert!(e.contains("not mapped for Version 2"), "{key}: {e}");
    }
    let mut stale = row.clone();
    set(&mut stale, bio("Position").unwrap(), 0);
    assert!(plan_player(&player_input(&cached), &cached, &stale).is_err());
    let mut stale = row.clone();
    stale[40] ^= 1;
    assert!(plan_player(&player_input(&cached), &cached, &stale).is_err());
    let mut wrong = cached.clone();
    wrong["recordIndex"] = json!(4);
    assert!(plan_player(&player_input(&cached), &wrong, &row).is_err());
    let mut input = player_input(&cached);
    input["ratingEdits"] =
        json!([{"index":0,"expected":(cached["ratingsRaw"][0].as_u64().unwrap()+1)%64,"value":1}]);
    assert!(plan_player(&input, &cached, &row).is_err());
}
#[test]
fn legacy_player_oracle_bytes_are_reproduced_for_every_proven_field() {
    let f = crate::player_changes::tests::fixture();
    // Weight and the appearance pointer are unmapped on Version 2; the
    // oracle record otherwise uses the identical proven bit layout.
    let v2 = |input: &Value| {
        let mut v = input.clone();
        v["heightCm"] = Value::Null;
        v["wingspanCm"] = Value::Null;
        v
    };
    let original = crate::player_changes::tests::bytes(&f["before"]);
    let mut matched = 0;
    for case in f["fields"].as_array().unwrap() {
        let c: Vec<_> = case["changes"]
            .as_str()
            .unwrap()
            .split(':')
            .map(|s| s.parse::<u64>().unwrap())
            .collect();
        let mut input = v2(&f["input"]);
        input["ratingEdits"] = json!([{"index":c[0],"expected":c[1],"value":c[2]}]);
        let result = plan_player(&input, &f["cached"], &original);
        assert_eq!(result.is_ok(), case["ok"] == true, "{}", case["changes"]);
        if let Ok(p) = result {
            let mut expected = crate::player_changes::tests::bytes(&case["after"]);
            assert_eq!(p.after, expected, "{}", case["changes"]);
            matched += 1;
        }
    }
    assert!(matched > 400);
    for case in f["bio"].as_array().unwrap() {
        let mut input = v2(&f["input"]);
        input["birthDate"] = case["birthday"].clone();
        input["yearsPro"] = case["years"].clone();
        input["expectedBirthYear"] = json!(1988);
        input["expectedBirthMonth"] = json!(3);
        input["expectedBirthDay"] = json!(14);
        input["expectedYearsPro"] = json!(17);
        let result = plan_player(&input, &f["cached"], &original);
        assert_eq!(result.is_ok(), case["ok"] == true);
        if let Ok(p) = result {
            assert_eq!(p.after, crate::player_changes::tests::bytes(&case["after"]));
        }
    }
    for case in f["basic"].as_array().unwrap() {
        let raw = crate::player_changes::tests::bytes(&case["before"]);
        let legacy = crate::player_changes::plan(
            &crate::player_changes::validate(&case["input"]).unwrap(),
            &case["cached"],
            &crate::player_changes::tests::raw(case),
            crate::player_changes::Extras::default(),
        )
        .unwrap();
        let p = plan_player(&v2(&case["input"]), &case["cached"], &raw).unwrap();
        assert_eq!(
            p.after, legacy.after.record,
            "weight included, appearance unmapped"
        );
    }
}
fn team_row() -> Vec<u8> {
    let mut row = noisy(profile::TEAM_STRIDE, 0x51);
    for (_, offset, _, capacity) in TEAM_TEXT {
        row[offset..offset + capacity * 2].fill(0);
    }
    text(&mut row, 0x2FA, "Test team");
    text(&mut row, 0x32C, "Testville");
    text(&mut row, 0x350, "TST");
    text(&mut row, 0x35E, "TS");
    text(&mut row, 0x1AE, "logo_tst");
    row[TEAM_ID..TEAM_ID + 2].copy_from_slice(&42u16.to_le_bytes());
    set(&mut row, (0x123C, 26, 6), 3);
    row
}
fn team_cached(row: &[u8], kind: u64) -> Value {
    json!({"source":"live-team","recordIndex":2,"teamId":42,"name":utf16(row,0x2FA,48).unwrap(),"cityName":utf16(row,0x32C,36).unwrap(),
        "cityShort":utf16(row,0x350,14).unwrap(),"stateShort":utf16(row,0x35E,14).unwrap(),"mainLogo":utf16(row,0x1AE,72).unwrap(),"rosterTypeId":kind})
}
fn team_input(c: &Value) -> Value {
    // Unchanged WPF form: arena '' → 0, null historic year → 0, no floor.
    json!({"recordIndex":2,"teamId":42,"expectedName":c["name"],"name":c["name"],"cityName":c["cityName"],"cityShort":c["cityShort"],
        "stateShort":c["stateShort"],"mainLogo":c["mainLogo"],"rosterTypeId":c["rosterTypeId"],"historicYearId":0,"arenaId":0})
}
#[test]
fn team_text_writers_touch_only_their_proven_buffers() {
    let row = team_row();
    let cached = team_cached(&row, 3);
    for (key, offset, len, _) in TEAM_TEXT {
        let mut input = team_input(&cached);
        let value = match key {
            "cityShort" | "stateShort" => "QAZ",
            _ => "Rust QA Ü",
        };
        input[key] = json!(value);
        let plan = plan_team(&input, &cached, &row).unwrap();
        let after = commit(Kind::Team, 2, &plan);
        assert_eq!(utf16(&after, offset, len).unwrap(), value, "{key}");
        assert_eq!(after[..offset], row[..offset], "{key}");
        assert_eq!(after[offset + len..], row[offset + len..], "{key}");
        assert_eq!(
            *after[..offset + len].last().unwrap(),
            0,
            "{key} stays terminated"
        );
    }
    // All five at once, longest values the old editor allows.
    let mut input = team_input(&cached);
    input["name"] = json!("A".repeat(23));
    input["cityName"] = json!("B".repeat(17));
    input["cityShort"] = json!("C".repeat(6));
    input["stateShort"] = json!("D".repeat(6));
    input["mainLogo"] = json!("e".repeat(35));
    let after = commit(Kind::Team, 2, &plan_team(&input, &cached, &row).unwrap());
    assert_eq!(utf16(&after, 0x2FA, 48).unwrap(), "A".repeat(23));
    assert_eq!(utf16(&after, 0x1AE, 72).unwrap(), "e".repeat(35));
    assert_eq!(after[TEAM_ID..TEAM_ID + 2], row[TEAM_ID..TEAM_ID + 2]);
    let mut outside = after.clone();
    let mut original = row.clone();
    for (_, offset, len, _) in TEAM_TEXT {
        outside[offset..offset + len].fill(0);
        original[offset..offset + len].fill(0);
    }
    assert_eq!(outside, original);
}
#[test]
fn unmapped_team_fields_and_stale_teams_are_refused() {
    let mut row = team_row();
    set(&mut row, (0x123C, 26, 6), 61);
    let cached = team_cached(&row, 61);
    assert_eq!(team_input(&cached)["rosterTypeId"], 61);
    assert!(plan_team(&team_input(&cached), &cached, &row).is_ok());
    for (key, value) in [
        ("rosterTypeId", json!(1)),
        ("historicYearId", json!(5)),
        ("arenaId", json!(1002)),
        ("floorFile", json!("court_qa")),
    ] {
        let mut input = team_input(&cached);
        input[key] = value;
        let e = plan_team(&input, &cached, &row).unwrap_err().to_string();
        assert!(
            e.contains("not mapped for Version 2")
                || (key == "rosterTypeId" && e.contains("read-only")),
            "{key}: {e}"
        );
    }
    let mut stale = row.clone();
    text(&mut stale, 0x32C, "Elsewhere");
    assert!(plan_team(&team_input(&cached), &cached, &stale).is_err());
    let mut stale = row.clone();
    stale[TEAM_ID] ^= 1;
    assert!(plan_team(&team_input(&cached), &cached, &stale).is_err());
    let mut input = team_input(&cached);
    input["name"] = json!("Bad\u{7}Name");
    assert!(plan_team(&input, &cached, &row).is_err());
}
#[test]
fn legacy_team_oracle_text_bytes_are_reproduced() {
    let cases = crate::team_changes::tests::fixture();
    let mut matched = 0;
    for c in cases.as_array().unwrap() {
        let (d, r, cached) = crate::team_changes::tests::data(c);
        let Ok(legacy) = crate::team_changes::plan(
            &crate::team_changes::validate(&c["input"]).unwrap(),
            &cached,
            &d,
            &r,
            r.references[0],
        ) else {
            continue;
        };
        let mut v2_cached = cached.clone();
        v2_cached["source"] = json!("live-team");
        v2_cached["recordIndex"] = c["input"]["recordIndex"].clone();
        let mut input = c["input"].clone();
        v2_cached["rosterTypeId"] = json!(get(&r.team, (0x123C, 26, 6)).unwrap());
        input["rosterTypeId"] = v2_cached["rosterTypeId"].clone();
        input["historicYearId"] = json!(0);
        input["arenaId"] = json!(0);
        input.as_object_mut().unwrap().remove("floorFile");
        v2_cached["historicYearId"] = Value::Null;
        let p = plan_team(&input, &v2_cached, &r.team).unwrap();
        for (_, offset, len, _) in TEAM_TEXT {
            assert_eq!(
                p.after[offset..offset + len],
                legacy.after.team[offset..offset + len]
            );
        }
        matched += 1;
    }
    assert!(matched > 0);
}
#[test]
fn all_52_uniform_controls_match_the_legacy_oracle_through_the_v2_transaction() {
    let rows = crate::uniform_changes::tests::fixture();
    let mut matched = 0;
    for c in rows.as_array().unwrap() {
        let (_, team, uniform, address) = crate::uniform_changes::tests::case_data(c);
        let mut cached_team = c["cached"].clone();
        cached_team["source"] = json!("live-team");
        let result = plan_uniform(
            &c["input"],
            &cached_team,
            &c["cached"]["uniforms"][0],
            &team,
            &uniform,
            address,
            0,
        );
        assert_eq!(result.is_ok(), c["ok"] == true);
        if let Ok((_, plan)) = result {
            assert_eq!(
                plan.after,
                crate::uniform_changes::tests::bytes(&c["after"])
            );
            let after = commit(Kind::Uniform, 0, &plan);
            assert_eq!(after, crate::uniform_changes::tests::bytes(&c["after"]));
            assert_eq!(after[..0x10C], uniform[..0x10C]);
            matched += 1;
        }
    }
    assert!(matched > 50);
    // Each control individually writes only its own proven bits.
    let (_, base, cached) = crate::uniform_changes::tests::largest_plan();
    let (_, team, uniform, address) = crate::uniform_changes::tests::case_data(&cached);
    let mut cached_team = cached["cached"].clone();
    cached_team["source"] = json!("live-team");
    let mut base_input = cached["input"].clone();
    for f in crate::uniform_changes::fields() {
        base_input[&f.request] = json!(crate::uniform_changes::value(&uniform, f).unwrap());
    }
    assert!(base.patches.len() > 1);
    for f in crate::uniform_changes::fields() {
        let mut input = base_input.clone();
        let old = crate::uniform_changes::value(&uniform, f).unwrap();
        let new = if old == 0 { f.maximum.min(1) } else { 0 };
        input[&f.request] = json!(new);
        let (_, plan) = plan_uniform(
            &input,
            &cached_team,
            &cached["cached"]["uniforms"][0],
            &team,
            &uniform,
            address,
            0,
        )
        .unwrap_or_else(|e| panic!("{}: {e:#}", f.request));
        let after = commit(Kind::Uniform, 0, &plan);
        assert_eq!(
            crate::uniform_changes::value(&after, f).unwrap(),
            new,
            "{}",
            f.request
        );
        only(
            &uniform,
            &after,
            &[(f.offset, f.shift, f.mask.count_ones())],
        );
    }
}
fn player_plan() -> (Plan, Owner) {
    let row = player_row();
    let cached = player_cached(&row);
    let mut input = player_input(&cached);
    input["firstName"] = json!("Changed");
    input["positionId"] = json!(0);
    input["jerseyNumberRaw"] = json!(9);
    input["potential"] = json!(90);
    let plan = plan_player(&input, &cached, &row).unwrap();
    assert!(plan.patches.len() >= 3);
    let o = owner_for(Kind::Player, 3, &row);
    (plan, o)
}
#[test]
fn each_read_back_failure_or_reported_write_failure_rolls_back_the_owned_prefix() {
    let (p, o) = player_plan();
    let good = Fake::new(&o, &p.before);
    apply(&good, &o, &p, |_| Ok(())).unwrap();
    for at in 1..=good.reads.get() {
        let mut m = Fake::new(&o, &p.before);
        m.fail_read_at = Some(at);
        let last = RefCell::new(None);
        assert!(
            apply(&m, &o, &p, |i| {
                *last.borrow_mut() = Some(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert_eq!(*m.raw.borrow(), p.before, "read {at}");
        assert!(last.borrow().as_ref().is_none_or(|i| !i.pending));
    }
    for at in 1..=p.patches.len() {
        for after in [false, true] {
            let mut m = Fake::new(&o, &p.before);
            m.fail_at = Some(at);
            m.fail_after_write = after;
            let last = RefCell::new(None);
            assert!(
                apply(&m, &o, &p, |i| {
                    *last.borrow_mut() = Some(i.clone());
                    Ok(())
                })
                .is_err()
            );
            assert_eq!(*m.raw.borrow(), p.before);
            assert!(!last.borrow().as_ref().unwrap().pending);
        }
    }
    for at in 1..p.patches.len() {
        let mut m = Fake::new(&o, &p.before);
        m.cancel_at = Some(at);
        assert!(apply(&m, &o, &p, |_| Ok(())).is_err());
        assert_eq!(*m.raw.borrow(), p.before);
    }
}
#[test]
fn partial_foreign_or_changed_owner_is_held_and_never_blindly_restored() {
    let (p, o) = player_plan();
    for kind in 0..3 {
        let mut m = Fake::new(&o, &p.before);
        m.fail_at = Some(2);
        m.fail_after_write = true;
        m.partial = kind == 0;
        m.foreign = kind == 1;
        m.change_owner = kind == 2;
        let last = RefCell::new(None);
        assert!(
            apply(&m, &o, &p, |i| {
                *last.borrow_mut() = Some(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert!(last.borrow().as_ref().unwrap().pending, "case {kind}");
        assert_eq!(m.writes.get(), 2);
    }
    let m = Fake::new(&o, &p.before);
    assert!(apply(&m, &o, &p, |_| bail!("Disk full before intent")).is_err());
    assert_eq!(m.writes.get(), 0);
    let m = Fake::new(&o, &p.before);
    let calls = Cell::new(0);
    assert!(
        apply(&m, &o, &p, |_| {
            calls.set(calls.get() + 1);
            ensure!(calls.get() != 2, "Commit record failed");
            Ok(())
        })
        .is_err()
    );
    assert_eq!(*m.raw.borrow(), p.before);
}
#[test]
fn foreign_records_unowned_masks_and_other_layout_evidence_never_write() {
    let (mut p, o) = player_plan();
    let m = Fake::new(&o, &p.before);
    let mut wrong = o.clone();
    wrong.index = 4;
    assert!(apply(&m, &wrong, &p, |_| Ok(())).is_err());
    let mut wrong = o.clone();
    wrong.kind = Kind::Team;
    assert!(apply(&m, &wrong, &p, |_| Ok(())).is_err());
    let mut wrong = o.clone();
    wrong.proof = "0".repeat(64);
    assert!(apply(&m, &wrong, &p, |_| Ok(())).is_err());
    let mut wrong = o.clone();
    wrong.table = REGION + 0x7F_F000;
    assert!(apply(&m, &wrong, &p, |_| Ok(())).is_err());
    assert_eq!(m.writes.get(), 0);
    // Appearance pointer and bytes after From are not owned.
    for offset in [0x78, 0xB8] {
        let mut bad = p.clone();
        bad.patches[0] = Patch {
            offset,
            before: p.before[offset..offset + 1].to_vec(),
            after: vec![p.before[offset] ^ 1],
            mask: vec![1],
        };
        assert!(apply(&m, &o, &bad, |_| Ok(())).is_err());
    }
    let mut changed = Fake::new(&o, &p.before);
    changed.raw.get_mut()[0x128] ^= 1;
    assert!(apply(&changed, &o, &p, |_| Ok(())).is_err());
    assert_eq!(changed.writes.get(), 0);
    p.patches.clear();
    assert!(apply(&m, &o, &p, |_| Ok(())).is_err(), "incomplete plan");
    assert_eq!(m.writes.get(), 0);
    let team = team_row();
    let team_masks = allowed(Kind::Team).unwrap();
    assert_eq!(
        team_masks.iter().filter(|b| **b == 255).count(),
        48 + 36 + 14 + 14 + 72 + 256 + 480 + 8
    );
    assert_eq!(team_masks[TEAM_ID], 0);
    assert!(team_masks[UNIFORM_SLOTS..0xEE0].iter().all(|b| *b == 255));
    assert_eq!(
        team_masks[0xEE0..0xEE8],
        [255; 8],
        "existing arena pointer is the sole owned arena-related span"
    );
    assert!(identity(Kind::Team, &team).unwrap().starts_with("team:42"));
    let uniform = allowed(Kind::Uniform).unwrap();
    assert!(uniform[..0x10C].iter().all(|b| *b == 0));
}
#[test]
fn journal_pending_requires_known_game_exit_and_malformed_records_stay_intact() {
    static N: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "roster-v2-intent-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("Data")).unwrap();
    let (p, o) = player_plan();
    assert!(!any_pending(&root).unwrap());
    let i = Intent {
        schema: INTENT_SCHEMA.into(),
        id: backend::unique_id(),
        owner: o.clone(),
        patches: p.patches.clone(),
        pending: true,
        result: "fixture".into(),
    };
    save(&root, &i).unwrap();
    assert!(pending(&root).unwrap() && any_pending(&root).unwrap());
    let rows = [session_safety::Process {
        name: "NBA2K27.exe".into(),
        pid: o.game.pid,
        parent: 0,
        identity: Some(o.game.clone()),
    }];
    assert!(resolve_ended(&root, &rows).is_err());
    resolve_ended(&root, &[]).unwrap();
    assert!(!pending(&root).unwrap());
    let mut foreign = i.clone();
    foreign.patches[0].offset = 0x78;
    assert!(save(&root, &foreign).is_err());
    fs::write(path(&root), b"{broken").unwrap();
    assert!(pending(&root).is_err() && any_pending(&root).is_err());
    assert_eq!(fs::read(path(&root)).unwrap(), b"{broken");
    fs::remove_dir_all(root).unwrap();
}
struct Image(BTreeMap<u64, u8>);
impl Memory for Image {
    fn base(&self) -> u64 {
        0x1_8000_0000
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        (0..len)
            .map(|i| {
                self.0
                    .get(&(at + i as u64))
                    .copied()
                    .context("Unmapped fake read")
            })
            .collect()
    }
}
#[test]
fn write_profile_rejects_other_builds_and_changed_anchors() {
    // Positive coverage is the ignored disk test below (exact image bytes);
    // fixtures can only prove that wrong bytes are rejected.
    let mut bytes = BTreeMap::new();
    for (i, b) in b"MZ".iter().enumerate() {
        bytes.insert(0x1_8000_0000 + i as u64, *b);
    }
    assert!(verify_write_profile(&Image(bytes)).is_err());
}
#[test]
#[ignore]
fn exact_version_two_write_profile_is_verified_from_disk_only() {
    use std::{
        fs::File,
        io::{Seek, SeekFrom},
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
    let sections: Vec<_> = (0..count)
        .map(|i| {
            let at = pe + 24 + opt + i * 40;
            (
                u32::from_le_bytes(header[at + 12..at + 16].try_into().unwrap()) as u64,
                u32::from_le_bytes(header[at + 16..at + 20].try_into().unwrap()) as u64,
                u32::from_le_bytes(header[at + 20..at + 24].try_into().unwrap()) as u64,
            )
        })
        .collect();
    struct Disk {
        file: RefCell<File>,
        header: Vec<u8>,
        base: u64,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let rva = at - self.base;
            if rva == 0 && len == 4096 {
                return Ok(self.header.clone());
            }
            let (start, _, raw) = self
                .sections
                .iter()
                .find(|(s, n, _)| rva >= *s && rva + len as u64 <= s + n)
                .context("Not disk-backed")?;
            let mut b = vec![0; len];
            let mut f = self.file.borrow_mut();
            f.seek(SeekFrom::Start(raw + rva - start))?;
            f.read_exact(&mut b)?;
            Ok(b)
        }
    }
    let m = Disk {
        file: RefCell::new(file),
        header,
        base,
        sections,
    };
    verify_write_profile(&m).unwrap();
    crate::roster_v2_readonly::verify_profile(&m).unwrap();
    println!(
        "{}",
        json!({"pass":true,"uniformControls":52,"teamTextBuffers":5,"uniformSlots":32,"gameProcessAccess":false,"gameFilesChanged":false})
    );
}
fn link_input() -> Value {
    json!({"recordIndex":2,"teamId":42,"expectedTeamName":"Test team","slot":5,"uniformRecordIndex":7,"uniformId":77,"expectedBaseFilename":"uniform_qa"})
}
fn link_fixture() -> (Vec<u8>, Vec<u8>, Value, Value) {
    let mut team = team_row();
    team[UNIFORM_SLOTS..0xEE0].fill(0);
    let mut uniform = noisy(profile::UNIFORM_STRIDE, 0x77);
    uniform[0x50..0x52].copy_from_slice(&77u16.to_le_bytes());
    uniform[0x52..0x54].copy_from_slice(&42u16.to_le_bytes());
    uniform[8..72].fill(0);
    uniform[8..18].copy_from_slice(b"uniform_qa");
    let mut cached = team_cached(&team, 0);
    cached["emptyUniformSlots"] = json!((0..32).collect::<Vec<_>>());
    let candidate = json!({"recordIndex":7,"uniformId":77,"teamId":42,"baseFilename":"uniform_qa","stockResourceAvailable":true});
    (team, uniform, cached, candidate)
}
#[test]
fn uniform_link_writes_one_pointer_into_an_empty_owned_slot_like_wpf() {
    let (team, uniform, cached, candidate) = link_fixture();
    let address = REGION + 0x30_0000 + 7 * 0x170;
    let edit = validate_uniform_link(&link_input()).unwrap();
    let plan = plan_uniform_link(
        &edit, &cached, &candidate, &team, &uniform, address, false, true,
    )
    .unwrap();
    assert_eq!(plan.patches.len(), 1);
    assert_eq!(plan.patches[0].offset, UNIFORM_SLOTS + 5 * 8);
    assert_eq!(plan.patches[0].after, address.to_le_bytes());
    let after = commit(Kind::Team, 2, &plan);
    assert_eq!(after[..UNIFORM_SLOTS + 40], team[..UNIFORM_SLOTS + 40]);
    assert_eq!(after[UNIFORM_SLOTS + 48..], team[UNIFORM_SLOTS + 48..]);
    // Every WPF refusal, before any intent.
    let refuse = |team: &[u8],
                  uniform: &[u8],
                  cached: &Value,
                  candidate: &Value,
                  referenced: bool,
                  stock: bool| {
        plan_uniform_link(
            &edit, cached, candidate, team, uniform, address, referenced, stock,
        )
        .is_err()
    };
    assert!(
        refuse(&team, &uniform, &cached, &candidate, true, true),
        "already referenced"
    );
    assert!(
        refuse(&team, &uniform, &cached, &candidate, false, false),
        "no stock art"
    );
    let mut occupied = team.clone();
    occupied[UNIFORM_SLOTS + 40] = 1;
    assert!(refuse(
        &occupied, &uniform, &cached, &candidate, false, true
    ));
    let mut other = uniform.clone();
    other[0x52] ^= 1;
    assert!(
        refuse(&team, &other, &cached, &candidate, false, true),
        "other team's uniform"
    );
    let mut stale = cached.clone();
    stale["emptyUniformSlots"] = json!([0, 1]);
    assert!(refuse(&team, &uniform, &stale, &candidate, false, true));
    let mut wrong = candidate.clone();
    wrong["baseFilename"] = json!("uniform_other");
    assert!(refuse(&team, &uniform, &cached, &wrong, false, true));
    for (key, value) in [
        ("slot", json!(32)),
        ("expectedBaseFilename", json!("")),
        ("expectedTeamName", json!("a\u{1}")),
    ] {
        let mut bad = link_input();
        bad[key] = value;
        assert!(validate_uniform_link(&bad).is_err(), "{key}");
    }
    // The adjacent city pointer is unowned, even with native arena selection enabled.
    let o = owner_for(Kind::Team, 2, &team);
    let m = Fake::new(&o, &team);
    let mut bad = plan.clone();
    bad.patches[0].offset = 0xEE8;
    bad.patches[0].before = team[0xEE8..0xEF0].to_vec();
    assert!(apply(&m, &o, &bad, |_| Ok(())).is_err());
    assert_eq!(m.writes.get(), 0);
}

#[test]
fn proven_weight_float_saves_like_wpf_and_keeps_unchanged_floats() {
    let mut row = player_row();
    row[WEIGHT..WEIGHT + 4].copy_from_slice(&220.004f32.to_le_bytes());
    let mut cached = player_cached(&row);
    cached["weightLbs"] = json!(weight_display(&row).unwrap());
    assert_eq!(cached["weightLbs"], 220.0);
    let mut input = player_input(&cached);
    input["weightLbs"] = json!(220.0);
    let same = plan_player(&input, &cached, &row).unwrap();
    assert!(
        same.patches.is_empty(),
        "display-equal weight keeps the exact float"
    );
    input["weightLbs"] = json!(231.5);
    let after = commit(
        Kind::Player,
        3,
        &plan_player(&input, &cached, &row).unwrap(),
    );
    assert_eq!(
        f32::from_le_bytes(after[WEIGHT..WEIGHT + 4].try_into().unwrap()),
        231.5
    );
    assert_eq!(after[..WEIGHT], row[..WEIGHT]);
    assert_eq!(after[WEIGHT + 4..], row[WEIGHT + 4..]);
    let mut stale = cached.clone();
    stale["weightLbs"] = json!(219.0);
    assert!(plan_player(&input, &stale, &row).is_err());
    input["weightLbs"] = json!(600);
    assert!(
        plan_player(&input, &cached, &row).is_err(),
        "WPF range 70..500"
    );
    let mut bad = row.clone();
    bad[WEIGHT..WEIGHT + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    input["weightLbs"] = json!(231.5);
    assert!(plan_player(&input, &cached, &bad).is_err());
}
fn audio_fixture() -> (Vec<u8>, Value, Value) {
    let mut row = player_row();
    row[FULL_NAME_SID..FULL_NAME_SID + 2].copy_from_slice(&1234i16.to_le_bytes());
    let mut cached = player_cached(&row);
    cached["audio"] = json!({"fullNameId":1234});
    let input = player_input(&cached);
    (row, cached, input)
}
#[test]
fn full_name_audio_echo_is_noop_and_changed_signed_scalar_is_exactly_two_bytes() {
    let (row, cached, mut input) = audio_fixture();
    input["fullNameId"] = json!("1234");
    assert!(
        plan_player(&input, &cached, &row)
            .unwrap()
            .patches
            .is_empty()
    );
    for value in [-1i16, 0, 99, 32767] {
        input["fullNameId"] = json!(value);
        let plan = plan_player(&input, &cached, &row).unwrap();
        assert_eq!(plan.patches.len(), 1);
        assert_eq!(plan.patches[0].offset, FULL_NAME_SID);
        assert_eq!(
            plan.patches[0].before,
            row[FULL_NAME_SID..FULL_NAME_SID + 2]
        );
        assert_eq!(plan.patches[0].after, value.to_le_bytes());
        assert_eq!(plan.patches[0].mask, [255, 255]);
        let after = commit(Kind::Player, 3, &plan);
        only(&row, &after, &[(FULL_NAME_SID, 0, 16)]);
        assert_eq!(
            after[FULL_NAME_SID + 2..FULL_NAME_SID + 4],
            row[FULL_NAME_SID + 2..FULL_NAME_SID + 4]
        );
    }
}
#[test]
fn full_name_audio_refuses_invalid_ranges_missing_or_stale_native_baseline() {
    let (row, cached, mut input) = audio_fixture();
    for value in [
        json!(-2),
        json!(32768),
        json!(0.5),
        json!(true),
        json!("NaN"),
        json!([]),
    ] {
        input["fullNameId"] = value;
        assert!(plan_player(&input, &cached, &row).is_err());
    }
    input["fullNameId"] = json!(99);
    let mut stale = row.clone();
    stale[FULL_NAME_SID] ^= 1;
    assert!(plan_player(&input, &cached, &stale).is_err());
    let mut unavailable = cached.clone();
    unavailable["audio"] = Value::Null;
    assert!(plan_player(&input, &unavailable, &row).is_err());
    assert!(plan_player(&input, &cached, &row[..FULL_NAME_SID]).is_err());
    input["fullNameId"] = Value::Null;
    assert!(plan_player(&input, &unavailable, &row).is_err());
    input.as_object_mut().unwrap().remove("fullNameId");
    assert!(
        plan_player(&input, &unavailable, &row)
            .unwrap()
            .patches
            .is_empty()
    );
}
#[test]
fn audio_scalar_proof_rejects_offset_signedness_adjacent_and_method_mutations() {
    let proof: Value =
        serde_json::from_str(include_str!("../assets/roster-v2-audio-proof.json")).unwrap();
    check_audio_proof(&proof).unwrap();
    for n in 0..8 {
        let mut p = proof.clone();
        match n {
            0 => p["offset"] = json!(FULL_NAME_SID + 2),
            1 => p["bytes"] = json!(4),
            2 => p["signed"] = json!(false),
            3 => p["requestMinimum"] = json!(-32768),
            4 => p["requestMaximum"] = json!(65535),
            5 => p["anchors"][0]["hex"] = json!("00"),
            6 => p["methods"][0]["sha256"] = json!("0".repeat(64)),
            _ => p["propertyNameRva"] = json!(0x006B2EF0),
        }
        assert!(check_audio_proof(&p).is_err(), "mutation {n}");
    }
    let masks = allowed(Kind::Player).unwrap();
    assert_eq!(&masks[FULL_NAME_SID..FULL_NAME_SID + 2], &[255, 255]);
    // The neighboring WORD was already writable as a proven independent
    // legacy rating. It must not be attributed to this new audio scalar.
    let trade = rating("NoTradeDaysLeft").unwrap();
    assert_eq!(
        (trade.offset, trade.shift, trade.bits),
        (FULL_NAME_SID + 2, 0, 16)
    );
    assert_eq!(&masks[FULL_NAME_SID + 2..FULL_NAME_SID + 4], &[255, 255]);
    let (row, cached, mut input) = audio_fixture();
    input["fullNameId"] = json!(99);
    let plan = plan_player(&input, &cached, &row).unwrap();
    assert_eq!(
        &plan.after[FULL_NAME_SID + 2..FULL_NAME_SID + 4],
        &row[FULL_NAME_SID + 2..FULL_NAME_SID + 4]
    );
    assert!(plan.patches.iter().all(|p| p.offset + p.after.len() <= FULL_NAME_SID + 2 || p.offset >= FULL_NAME_SID + 4));
    let mut old = Sha256::new();
    old.update(include_str!("../assets/roster-v2-field-proof.json"));
    old.update(include_str!("../assets/roster-v2-write-proof.json"));
    assert_ne!(proof_tag(), format!("{:x}", old.finalize()));
}
#[test]
fn audio_scalar_preserves_intent_before_effect_and_rolls_back_only_exact_owned_values() {
    let (row, cached, mut input) = audio_fixture();
    input["fullNameId"] = json!(32767);
    let plan = plan_player(&input, &cached, &row).unwrap();
    let owner = owner_for(Kind::Player, 3, &row);
    let m = Fake::new(&owner, &row);
    assert!(apply(&m, &owner, &plan, |_| bail!("intent durability failed")).is_err());
    assert_eq!(m.writes.get(), 0);
    for partial in [false, true] {
        let mut m = Fake::new(&owner, &row);
        m.fail_at = Some(1);
        m.fail_after_write = true;
        m.partial = partial;
        let receipts = RefCell::new(Vec::new());
        assert!(
            apply(&m, &owner, &plan, |i| {
                receipts.borrow_mut().push(i.clone());
                Ok(())
            })
            .is_err()
        );
        if partial {
            assert_eq!(m.writes.get(), 1);
            assert!(receipts.borrow().last().unwrap().pending);
        } else {
            assert_eq!(*m.raw.borrow(), row);
            assert_eq!(m.writes.get(), 2);
            assert!(!receipts.borrow().last().unwrap().pending);
        }
    }
}

#[test]
fn unknown_negative_audio_echo_can_be_preserved_but_never_newly_written() {
    let (mut row, mut cached, mut input) = audio_fixture();
    row[FULL_NAME_SID..FULL_NAME_SID + 2].copy_from_slice(&(-9i16).to_le_bytes());
    cached["audio"]["fullNameId"] = json!(-9);
    input["fullNameId"] = json!(-9);
    input["firstName"] = json!("Changed");
    let plan = plan_player(&input, &cached, &row).unwrap();
    assert_eq!(
        plan.after[FULL_NAME_SID..FULL_NAME_SID + 2],
        (-9i16).to_le_bytes()
    );
    assert!(!plan.changed.iter().any(|s| s == "fullNameId"));
    for id in [-8, -1, 0, 99] {
        input["fullNameId"] = json!(id);
        assert!(plan_player(&input, &cached, &row).is_err());
    }
}

#[test]
fn native_team_metadata_echo_preserves_all_six_type_and_eight_year_bits() {
    for kind in [0, 3, 29, 32, 33, 34, 35, 61, 63] {
        for year in [0, 1, 60, 101, 127, 128, 255] {
            let mut row = team_row();
            set(&mut row, (0x123C, 26, 6), kind);
            set(&mut row, (0x1548, 19, 8), year);
            let mut cached = team_cached(&row, kind);
            cached["classicTeamEndYearRaw"] = json!(year);
            let mut input = team_input(&cached);
            input["classicTeamEndYearRaw"] = json!(year);
            input["name"] = json!("New text");
            let plan = plan_team(&input, &cached, &row).unwrap();
            assert_eq!(plan.after[0x123C..0x1240], row[0x123C..0x1240]);
            assert_eq!(plan.after[0x1548..0x154C], row[0x1548..0x154C]);
            assert!(
                !plan
                    .patches
                    .iter()
                    .any(|p| (p.offset..p.offset + p.after.len()).contains(&0x123F))
            );
            let mut changed = input.clone();
            changed["rosterTypeId"] = json!((kind + 1) % 64);
            assert_eq!(
                plan_team(&changed, &cached, &row).is_ok(),
                kind <= 35 && (kind + 1) % 64 <= 35
            );
            let mut changed = input.clone();
            changed["classicTeamEndYearRaw"] = json!((year + 1) % 256);
            assert_eq!(
                plan_team(&changed, &cached, &row).is_ok(),
                year <= 127 && (year + 1) % 256 <= 100
            );
            let mut live = row.clone();
            set(&mut live, (0x1548, 19, 8), (year + 1) % 256);
            assert!(plan_team(&input, &cached, &live).is_err());
        }
    }
    let masks = allowed(Kind::Team).unwrap();
    assert_eq!(&masks[0x123C..0x1240], &[0, 0, 0, 0xFC]);
    assert_eq!(&masks[0x1548..0x154C], &[0, 0, 0xF8, 0x07]);
}
#[test]
fn qa082_stored_journal_compatibility_never_grants_new_live_audio_authority() {
    let (plan, mut owner) = player_plan();
    owner.proof = QA082_JOURNAL_PROOF.into();
    let root = std::env::temp_dir().join(format!("c27-old-audio-proof-{}", backend::unique_id()));
    fs::create_dir_all(root.join("Data")).unwrap();
    let mut intent = Intent {
        schema: INTENT_SCHEMA.into(),
        id: "a".repeat(48),
        owner: owner.clone(),
        patches: plan.patches.clone(),
        pending: false,
        result: "applied and read back verified".into(),
    };
    assert!(owner.validate().is_err());
    assert!(save(&root, &intent).is_err());
    backend::atomic_json(&path(&root), &intent).unwrap();
    assert!(!pending(&root).unwrap());
    intent.pending = true;
    backend::atomic_json(&path(&root), &intent).unwrap();
    assert!(pending(&root).unwrap());
    let memory = Fake::new(&owner, &plan.before);
    assert!(apply(&memory, &owner, &plan, |_| Ok(())).is_err());
    assert_eq!(memory.writes.get(), 0);
    let running = crate::session_safety::Process {
        name: "NBA2K27.exe".into(),
        pid: 7,
        parent: 0,
        identity: Some(owner.game.clone()),
    };
    assert!(resolve_ended(&root, &[running]).is_err());
    assert!(pending(&root).unwrap());
    resolve_ended(&root, &[]).unwrap();
    let migrated = read_intent(&path(&root)).unwrap();
    assert!(!migrated.pending);
    assert_eq!(migrated.owner.proof, proof_tag());
    assert!(migrated.result.contains("cannot be replayed"));
    assert_eq!(memory.writes.get(), 0);
    intent.owner.proof = "0".repeat(64);
    backend::atomic_json(&path(&root), &intent).unwrap();
    assert!(pending(&root).is_err());
    let (row, cached, mut input) = audio_fixture();
    input["fullNameId"] = json!(99);
    let audio = plan_player(&input, &cached, &row).unwrap();
    intent.owner = owner_for(Kind::Player, 3, &row);
    intent.owner.proof = QA082_JOURNAL_PROOF.into();
    intent.patches = audio.patches;
    backend::atomic_json(&path(&root), &intent).unwrap();
    assert!(pending(&root).is_err());
    assert!(resolve_ended(&root, &[]).is_err());
    fs::remove_file(path(&root)).unwrap();
    fs::remove_dir(root.join("Data")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn native_team_type_and_classic_year_changes_touch_only_proven_bits() {
    for kind in 0..=35 {
        for year in [0, 1, 59, 60, 99, 100] {
            let mut row = team_row();
            set(&mut row, TEAM_TYPE_BITS, 3);
            set(&mut row, TEAM_CLASSIC_YEAR_BITS, 17);
            let mut cached = team_cached(&row, 3);
            cached["classicTeamEndYearRaw"] = json!(17);
            let mut input = team_input(&cached);
            input["rosterTypeId"] = json!(kind);
            input["classicTeamEndYearRaw"] = json!(year);
            let plan = plan_team(&input, &cached, &row).unwrap();
            let after = commit(Kind::Team, 2, &plan);
            assert_eq!(get(&after, TEAM_TYPE_BITS).unwrap(), kind);
            assert_eq!(get(&after, TEAM_CLASSIC_YEAR_BITS).unwrap(), year);
            only(&row, &after, &[TEAM_TYPE_BITS, TEAM_CLASSIC_YEAR_BITS]);
            assert_eq!(
                get(&after, (0x1338, 15, 6)).unwrap(),
                get(&row, (0x1338, 15, 6)).unwrap(),
                "OriginalType untouched"
            );
        }
    }
}
#[test]
fn duplicate_native_year_encoding_is_not_normalized_by_a_same_calendar_request() {
    for (canonical, duplicate) in [(1, 101), (17, 117), (27, 127)] {
        let mut row = team_row();
        set(&mut row, TEAM_CLASSIC_YEAR_BITS, duplicate);
        let mut cached = team_cached(&row, 3);
        cached["classicTeamEndYearRaw"] = json!(duplicate);
        let mut input = team_input(&cached);
        input["classicTeamEndYearRaw"] = json!(canonical);
        let plan = plan_team(&input, &cached, &row).unwrap();
        assert!(plan.patches.is_empty());
        assert_eq!(plan.after, row);
    }
    for (before, desired) in [(0, 100), (100, 0)] {
        let mut row = team_row();
        set(&mut row, TEAM_CLASSIC_YEAR_BITS, before);
        let mut cached = team_cached(&row, 3);
        cached["classicTeamEndYearRaw"] = json!(before);
        let mut input = team_input(&cached);
        input["classicTeamEndYearRaw"] = json!(desired);
        let plan = plan_team(&input, &cached, &row).unwrap();
        assert!(!plan.patches.is_empty());
        assert_eq!(get(&plan.after, TEAM_CLASSIC_YEAR_BITS).unwrap(), desired);
    }
}
#[test]
fn native_team_metadata_proof_mutations_and_legacy_claims_are_refused() {
    let proof: Value =
        serde_json::from_str(include_str!("../assets/roster-v2-team-metadata-proof.json")).unwrap();
    check_team_metadata_proof(&proof).unwrap();
    for n in 0..5 {
        let mut p = proof.clone();
        match n {
            0 => p["type"]["bits"] = json!(5),
            1 => p["classicYear"]["bits"] = json!(7),
            2 => p["classicYear"]["canonicalTargetMaximum"] = json!(255),
            3 => p["spans"][0]["sha256"] = json!("0".repeat(64)),
            _ => p["enumTargetRva"] = json!(0),
        };
        assert!(check_team_metadata_proof(&p).is_err());
    }
    let mut row = team_row();
    set(&mut row, TEAM_CLASSIC_YEAR_BITS, 17);
    let mut cached = team_cached(&row, 3);
    cached["classicTeamEndYearRaw"] = json!(17);
    for key in ["rosterTypeId", "classicTeamEndYearRaw"] {
        let mut input = team_input(&cached);
        input[key] = json!(1);
        let plan = plan_team(&input, &cached, &row).unwrap();
        assert!(!plan.patches.is_empty());
        let mut owner = owner_for(Kind::Team, 2, &row);
        owner.proof = QA082_JOURNAL_PROOF.into();
        let intent = Intent {
            schema: INTENT_SCHEMA.into(),
            id: "b".repeat(48),
            owner,
            patches: plan.patches,
            pending: true,
            result: "old".into(),
        };
        assert!(intent.validate_stored().is_err(), "{key}");
    }
}

#[test]
fn native_type_descriptor_pointer_binding_is_relocated_separately_from_hash_spans() {
    for base in [0x140000000u64, 0x180000000, 0x700000000] {
        assert!(team_enum_binding(base, base + 0x0A1E7CD0).is_ok());
        assert!(team_enum_binding(base, base + 0x0A1E7CD8).is_err());
        if base != 0x140000000 {
            assert!(team_enum_binding(base, 0x14A1E7CD0).is_err());
        }
    }
    assert!(team_enum_binding(u64::MAX, 0).is_err());
    for &(rva, len, _) in TEAM_METADATA_SPANS {
        assert!(
            rva + len as u64 <= 0x089BE960 || rva >= 0x089BE968,
            "Known relocatedenum pointer mustnever enter rawhash span"
        );
    }
}
#[test]
fn native_type_and_year_partial_effects_preserve_pending_recovery_without_foreign_undo() {
    let mut row = team_row();
    set(&mut row, TEAM_TYPE_BITS, 3);
    set(&mut row, TEAM_CLASSIC_YEAR_BITS, 17);
    let mut cached = team_cached(&row, 3);
    cached["classicTeamEndYearRaw"] = json!(17);
    for key in ["rosterTypeId", "classicTeamEndYearRaw"] {
        let mut input = team_input(&cached);
        // Flipped bit must yield neither before nor after, rather than recreating year17.
        input[key] = json!(if key == "classicTeamEndYearRaw" { 2 } else { 1 });
        let plan = plan_team(&input, &cached, &row).unwrap();
        let owner = owner_for(Kind::Team, 2, &row);
        let m = Fake::new(&owner, &row);
        assert!(apply(&m, &owner, &plan, |_| bail!("intent failure")).is_err());
        assert_eq!(m.writes.get(), 0);
        let mut m = Fake::new(&owner, &row);
        m.fail_at = Some(1);
        m.fail_after_write = true;
        m.partial = true;
        let last = RefCell::new(None::<Intent>);
        assert!(
            apply(&m, &owner, &plan, |i| {
                *last.borrow_mut() = Some(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert!(last.borrow().as_ref().unwrap().pending);
        assert_eq!(m.writes.get(), 1);
    }
}

#[test]
fn native_arena_requires_prepared_catalog_selection_and_changes_only_team_pointer() {
    let old = 0x230000000u64;
    let target = old + 4728;
    let mut row = team_row();
    row[0xEE0..0xEE8].copy_from_slice(&old.to_le_bytes());
    let cached = team_cached(&row, 3);
    let mut input = team_input(&cached);
    let selection = crate::roster_v2_arena::Selection::for_test(old, target, 77);
    input["nativeArenaId"] = json!(77);
    input["nativeArenaCatalogId"] = json!("pure-planner-fixture");
    assert!(plan_team(&input, &cached, &row).is_err());
    assert!(plan_team_with_arena(&input, &cached, &row, None).is_err());
    let plan = plan_team_with_arena(&input, &cached, &row, Some(&selection)).unwrap();
    assert_eq!(plan.changed, vec!["nativeArenaId"]);
    assert_eq!(plan.patches.len(), 1);
    assert_eq!(plan.patches[0].offset, 0xEE0);
    assert_eq!(plan.patches[0].mask, vec![255; 8]);
    let after = commit(Kind::Team, 2, &plan);
    assert_eq!(&after[0xEE0..0xEE8], &target.to_le_bytes());
    assert_eq!(&after[..0xEE0], &row[..0xEE0]);
    assert_eq!(&after[0xEE8..], &row[0xEE8..]);
    let masks = allowed(Kind::Team).unwrap();
    assert_eq!(&masks[0xEE0..0xEE8], &[255; 8]);
    assert_eq!(
        masks[0xEDF], 255,
        "Existing uniform slot masks are unchanged"
    );
    assert_eq!(masks[0xEE8], 0);
    for invalid in [
        Value::Null,
        json!("77"),
        json!(78),
        json!(-1),
        json!(65536),
        json!(1.5),
        json!(true),
    ] {
        input["nativeArenaId"] = invalid;
        assert!(plan_team_with_arena(&input, &cached, &row, Some(&selection)).is_err());
    }
    input["nativeArenaId"] = json!(77);
    input["nativeArenaCatalogId"] = json!("pure-planner-fixture");
    let mut changed = row.clone();
    changed[0xEE0] ^= 1;
    assert!(plan_team_with_arena(&input, &cached, &changed, Some(&selection)).is_err());
    for bad_catalog in [Value::Null, json!("different-generation"), json!(77)] {
        input["nativeArenaCatalogId"] = bad_catalog;
        assert!(plan_team_with_arena(&input, &cached, &row, Some(&selection)).is_err());
    }
    input["nativeArenaCatalogId"] = json!("pure-planner-fixture");
    input.as_object_mut().unwrap().remove("nativeArenaId");
    assert!(plan_team_with_arena(&input, &cached, &row, Some(&selection)).is_err());
    assert!(plan_team_with_arena(&input, &cached, &row, None).is_err());
    input
        .as_object_mut()
        .unwrap()
        .remove("nativeArenaCatalogId");
    assert!(plan_team_with_arena(&input, &cached, &row, None).is_ok());
}

#[test]
fn native_arena_pointer_keeps_durable_intent_exact_rollback_and_guarded_noop() {
    let old = 0x230000000u64;
    let mut row = team_row();
    row[0xEE0..0xEE8].copy_from_slice(&old.to_le_bytes());
    let cached = team_cached(&row, 3);
    let mut input = team_input(&cached);
    input["nativeArenaId"] = json!(19);
    input["nativeArenaCatalogId"] = json!("pure-planner-fixture");
    let same = crate::roster_v2_arena::Selection::for_test(old, old, 19);
    assert!(
        plan_team_with_arena(&input, &cached, &row, Some(&same))
            .unwrap()
            .patches
            .is_empty()
    );
    let changed = crate::roster_v2_arena::Selection::for_test(old, old + 4728, 19);
    let plan = plan_team_with_arena(&input, &cached, &row, Some(&changed)).unwrap();
    let owner = owner_for(Kind::Team, 2, &row);
    let m = Fake::new(&owner, &row);
    assert!(apply(&m, &owner, &plan, |_| bail!("no durable intent")).is_err());
    assert_eq!(m.writes.get(), 0);
    for partial in [false, true] {
        let mut m = Fake::new(&owner, &row);
        m.fail_at = Some(1);
        m.fail_after_write = true;
        m.partial = partial;
        let receipt = RefCell::new(None::<Intent>);
        assert!(
            apply(&m, &owner, &plan, |i| {
                *receipt.borrow_mut() = Some(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert_eq!(receipt.borrow().as_ref().unwrap().pending, partial);
        if partial {
            assert_eq!(m.writes.get(), 1);
        } else {
            assert_eq!(*m.raw.borrow(), row);
            assert_eq!(m.writes.get(), 2);
        }
    }
    let mut text_input = team_input(&cached);
    text_input["name"] = json!("Changed");
    let text_plan = plan_team(&text_input, &cached, &row).unwrap();
    let m = Fake::new(&owner, &row);
    m.raw.borrow_mut()[0xEE0] ^= 1;
    assert!(apply(&m, &owner, &text_plan, |_| Ok(())).is_err());
    assert_eq!(m.writes.get(), 0);
}

#[test]
fn qa082_legacy_journal_never_acquires_arena_pointer_masks() {
    let mut row = team_row();
    let old = 0x230000000u64;
    row[0xEE0..0xEE8].copy_from_slice(&old.to_le_bytes());
    let cached = team_cached(&row, 3);
    let mut input = team_input(&cached);
    input["nativeArenaId"] = json!(3);
    input["nativeArenaCatalogId"] = json!("pure-planner-fixture");
    let selection = crate::roster_v2_arena::Selection::for_test(old, old + 4728, 3);
    let plan = plan_team_with_arena(&input, &cached, &row, Some(&selection)).unwrap();
    let mut owner = owner_for(Kind::Team, 2, &row);
    owner.proof = QA082_JOURNAL_PROOF.into();
    let intent = Intent {
        schema: INTENT_SCHEMA.into(),
        id: "c".repeat(48),
        owner,
        patches: plan.patches,
        pending: false,
        result: "forged terminal".into(),
    };
    assert!(intent.validate_stored().is_err());
}

#[test]
fn native_origin_uses_typed_catalog_selection_for_eight_player_bytes_only() {
    for original in [0, 0x230000000u64] {
        let target = 0x230000048u64;
        let mut row = player_row();
        row[0xB0..0xB8].copy_from_slice(&original.to_le_bytes());
        let cached = player_cached(&row);
        let mut input = player_input(&cached);
        input["nativeOriginIndex"] = json!(1);
        input["nativeOriginCatalogId"] = json!("pure-planner-fixture");
        let selected = crate::roster_v2_origins::Selection::for_test(original, target, 1);
        assert!(plan_player(&input, &cached, &row).is_err());
        assert!(plan_player_with_origin(&input, &cached, &row, None).is_err());
        let plan = plan_player_with_origin(&input, &cached, &row, Some(&selected)).unwrap();
        assert_eq!(plan.changed, vec!["nativeOriginIndex"]);
        assert_eq!(plan.patches.len(), 1);
        assert_eq!(plan.patches[0].offset, 0xB0);
        assert_eq!(plan.patches[0].mask, vec![255; 8]);
        let after = commit(Kind::Player, 3, &plan);
        assert_eq!(&after[0xB0..0xB8], &target.to_le_bytes());
        assert_eq!(&after[..0xB0], &row[..0xB0]);
        assert_eq!(&after[0xB8..], &row[0xB8..]);
        for invalid in [
            Value::Null,
            json!("1"),
            json!(-1),
            json!(2),
            json!(true),
            json!(0.5),
        ] {
            input["nativeOriginIndex"] = invalid;
            assert!(plan_player_with_origin(&input, &cached, &row, Some(&selected)).is_err());
        }
        input["nativeOriginIndex"] = json!(1);
        for token in [Value::Null, json!("different-generation"), json!(1)] {
            input["nativeOriginCatalogId"] = token;
            assert!(plan_player_with_origin(&input, &cached, &row, Some(&selected)).is_err());
        }
        input["nativeOriginCatalogId"] = json!("pure-planner-fixture");
        let mut changed = row.clone();
        changed[0xB0] ^= 1;
        assert!(plan_player_with_origin(&input, &cached, &changed, Some(&selected)).is_err());
        input.as_object_mut().unwrap().remove("nativeOriginIndex");
        assert!(plan_player_with_origin(&input, &cached, &row, Some(&selected)).is_err());
        assert!(plan_player_with_origin(&input, &cached, &row, None).is_err());
        input
            .as_object_mut()
            .unwrap()
            .remove("nativeOriginCatalogId");
        assert!(plan_player_with_origin(&input, &cached, &row, None).is_ok());
    }
    let masks = allowed(Kind::Player).unwrap();
    assert_eq!(&masks[0xB0..0xB8], &[255; 8]);
    assert_eq!(masks[0xAF], 0);
    assert_eq!(masks[0xB8], 0);
}

#[test]
fn native_origin_preserves_intent_before_effect_exact_rollback_and_legacy_exclusion() {
    let mut row = player_row();
    let original = 0x230000000u64;
    row[0xB0..0xB8].copy_from_slice(&original.to_le_bytes());
    let cached = player_cached(&row);
    let mut input = player_input(&cached);
    input["nativeOriginIndex"] = json!(1);
    input["nativeOriginCatalogId"] = json!("pure-planner-fixture");
    let selected = crate::roster_v2_origins::Selection::for_test(original, original + 72, 1);
    let plan = plan_player_with_origin(&input, &cached, &row, Some(&selected)).unwrap();
    let owner = owner_for(Kind::Player, 3, &row);
    let m = Fake::new(&owner, &row);
    assert!(apply(&m, &owner, &plan, |_| bail!("no durable intent")).is_err());
    assert_eq!(m.writes.get(), 0);
    for partial in [false, true] {
        let mut m = Fake::new(&owner, &row);
        m.fail_at = Some(1);
        m.fail_after_write = true;
        m.partial = partial;
        let receipt = RefCell::new(None::<Intent>);
        assert!(
            apply(&m, &owner, &plan, |i| {
                *receipt.borrow_mut() = Some(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert_eq!(receipt.borrow().as_ref().unwrap().pending, partial);
        if partial {
            assert_eq!(m.writes.get(), 1);
        } else {
            assert_eq!(*m.raw.borrow(), row);
            assert_eq!(m.writes.get(), 2);
        }
    }
    let mut legacy = Intent {
        schema: INTENT_SCHEMA.into(),
        id: "d".repeat(48),
        owner: owner.clone(),
        patches: plan.patches.clone(),
        pending: false,
        result: "forged terminal".into(),
    };
    legacy.owner.proof = QA082_JOURNAL_PROOF.into();
    assert!(legacy.validate_stored().is_err());
    let mut unchanged = player_input(&cached);
    unchanged["firstName"] = json!("Changed");
    let ordinary = plan_player(&unchanged, &cached, &row).unwrap();
    let m = Fake::new(&owner, &row);
    m.raw.borrow_mut()[0xB0] ^= 1;
    assert!(apply(&m, &owner, &ordinary, |_| Ok(())).is_err());
    assert_eq!(m.writes.get(), 0);
}

#[test]
fn cancellation_and_budget_expiry_enter_separate_bounded_reverse_only_mode() {
    let (p, o) = player_plan();
    for expiry in [false, true] {
        let mut m = Fake::new(&o, &p.before);
        if expiry {
            m.expired_at = Some(1)
        } else {
            m.cancel_at = Some(1)
        };
        let last = RefCell::new(None);
        assert!(
            apply(&m, &o, &p, |i| {
                *last.borrow_mut() = Some(i.clone());
                Ok(())
            })
            .is_err()
        );
        assert!(m.recovering.get());
        assert!(m.recovery_checks.get() > 0);
        assert_eq!(*m.raw.borrow(), p.before);
        assert!(!last.borrow().as_ref().unwrap().pending);
        assert!(m.checkpoint().is_err());
    }
    let mut m = Fake::new(&o, &p.before);
    m.expired_at = Some(1);
    m.recovery_limit = Some(0);
    let last = RefCell::new(None);
    assert!(
        apply(&m, &o, &p, |i| {
            *last.borrow_mut() = Some(i.clone());
            Ok(())
        })
        .is_err()
    );
    assert_eq!(m.writes.get(), 1);
    assert!(last.borrow().as_ref().unwrap().pending);
    assert!(m.checkpoint().is_err());
}
