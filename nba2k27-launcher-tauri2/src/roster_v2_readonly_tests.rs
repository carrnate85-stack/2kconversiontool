use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
const BASE: u64 = 0x18000_0000;
const REGION: owner::OwnedRegion = owner::OwnedRegion {
    base: 0x22000_0000,
    len: 0x800000,
};
const CONTAINER: u64 = 0x24000_0000;
const OWNER: u64 = REGION.base + 0x50;
const PLAYERS: u64 = REGION.base + 0x680;
const TEAMS: u64 = REGION.base + 0x200000;
struct Fake {
    bytes: RefCell<BTreeMap<u64, u8>>,
    ended: Cell<bool>,
    short: Cell<u64>,
    seen: RefCell<BTreeMap<u64, usize>>,
    mutation: RefCell<Option<(u64, usize, u64, Vec<u8>)>>,
}
impl Fake {
    fn put(&self, at: u64, bytes: &[u8]) {
        let mut b = self.bytes.borrow_mut();
        for (i, v) in bytes.iter().enumerate() {
            b.insert(at + i as u64, *v);
        }
    }
    fn q(&self, at: u64, value: u64) {
        self.put(at, &value.to_le_bytes());
    }
    fn text(&self, at: u64, value: &str) {
        let bytes: Vec<u8> = value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .chain([0, 0])
            .collect();
        self.put(at, &bytes);
    }
    fn descriptor(&self, offset: u64, count: u64, table: u64) {
        self.q(OWNER + offset, count);
        self.q(OWNER + offset + 8, table);
    }
    fn fixture() -> Self {
        let m = Self {
            bytes: RefCell::new(BTreeMap::new()),
            ended: Cell::new(false),
            short: Cell::new(0),
            seen: RefCell::new(BTreeMap::new()),
            mutation: RefCell::new(None),
        };
        let mut header = vec![0; 4096];
        header[..2].copy_from_slice(b"MZ");
        header[60..64].copy_from_slice(&128u32.to_le_bytes());
        header[128..132].copy_from_slice(b"PE\0\0");
        header[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        header[136..140].copy_from_slice(&0x6ac5_3765u32.to_le_bytes());
        header[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
        header[208..212].copy_from_slice(&0x35bd_e000u32.to_le_bytes());
        m.put(BASE, &header);
        // This fixture explicitly models unsupported optional Arena code.
        m.put(BASE + 0x2ed9aa0, &vec![0; 275]);
        // Optional arena decoder is intentionally unavailable in this pre-existing fixture.
        m.put(BASE + 0x2ed9aa0, &[0u8; 275]);
        for &(rva, prefix, target) in owner::RIP_ANCHORS {
            let mut b = prefix.to_vec();
            b.extend_from_slice(
                &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
            );
            m.put(BASE + rva, &b);
        }
        for &(rva, b) in owner::FIELD_ANCHORS {
            m.put(BASE + rva, b);
        }
        m.q(BASE + owner::ROOT_CLASS, BASE + 0x024e_65f0);
        m.q(BASE + owner::ROOT_CLASS + 8, BASE + 0x024e_6660);
        m.q(BASE + owner::ROOT, CONTAINER);
        m.q(CONTAINER, BASE + owner::ROOT_CLASS);
        m.q(CONTAINER + 0xa8, OWNER);
        m.descriptor(0x10, 100, PLAYERS);
        m.descriptor(0x60, 1, REGION.base + 0x100000);
        m.descriptor(0x80, 1, TEAMS);
        m.descriptor(0x290, 0, 0);
        m.descriptor(0x240, 0, 0);
        m.descriptor(0x250, 0, 0);
        m.descriptor(0x90, 0, 0);
        m.put(PLAYERS, &vec![0; 100 * owner::PLAYER_STRIDE]);
        for i in 0..100 {
            let at = PLAYERS + i as u64 * owner::PLAYER_STRIDE as u64;
            m.text(at, "Player");
            m.text(at + 40, &format!("Test{i}"));
            m.put(at + 0x128, &(i as u16 + 1000).to_le_bytes());
        }
        m.put(TEAMS, &vec![0; owner::TEAM_STRIDE]);
        m.text(TEAMS + 0x2fa, "Test team");
        m.put(TEAMS + 0xf20, &42u16.to_le_bytes());
        m.put(TEAMS + 0x123c, &(61u32 << 26).to_le_bytes());
        m
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        let count = {
            let mut seen = self.seen.borrow_mut();
            let count = seen.entry(at).or_default();
            *count += 1;
            *count
        };
        let mutation = self.mutation.borrow().clone();
        if let Some((trigger, when, target, bytes)) = mutation {
            if trigger == at && when == count {
                self.put(target, &bytes);
            }
        }
        let b = self.bytes.borrow();
        let mut out: Vec<u8> = (0..len)
            .map(|i| {
                b.get(&(at + i as u64))
                    .copied()
                    .context("Unmapped fake read")
            })
            .collect::<Result<_>>()?;
        if self.short.get() == at {
            out.pop();
        }
        Ok(out)
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(!self.ended.get(), "Session ended");
        Ok(())
    }
}
fn proof() -> Value {
    serde_json::from_str(include_str!("../assets/roster-v2-field-proof.json")).unwrap()
}
fn original() -> Value {
    serde_json::from_str(include_str!("../assets/roster-layout.json")).unwrap()
}
#[test]
fn all_455_fields_have_independent_matching_shapes() {
    let p = proof();
    let f = fields_from(&p, &original()).unwrap();
    assert_eq!(f.packed.len(), 455);
    assert_eq!(f.metadata.len(), 11);
    for (index, field) in f.packed.iter().enumerate() {
        // All-ones and one-hot records exercise every mapped bit, including
        // unaligned byte-packed fields and full 32-bit shoe colorway IDs.
        let row = vec![255; owner::PLAYER_STRIDE];
        assert_eq!(
            uint(&row, field.offset, field.shift, field.bits).unwrap(),
            (1u64 << field.bits) - 1
        );
        for bit in 0..field.bits {
            let mut row = vec![0; owner::PLAYER_STRIDE];
            let global = field.offset * 8 + field.shift as usize + bit as usize;
            row[global / 8] |= 1 << (global % 8);
            assert_eq!(
                uint(&row, field.offset, field.shift, field.bits).unwrap(),
                1u64 << bit,
                "field {index}"
            );
        }
    }
}
#[test]
fn rejects_mutated_duplicate_reordered_or_incomplete_field_proof() {
    let old = original();
    for mutate in 0..9 {
        let mut p = proof();
        match mutate {
            0 => {
                p["rows"][0]["extracted"]["offset"] =
                    json!(p["rows"][0]["extracted"]["offset"].as_u64().unwrap() + 1)
            }
            1 => p["rows"][0]["matches"] = json!(false),
            2 => {
                p["rows"].as_array_mut().unwrap().remove(0);
            }
            3 => p["rows"].as_array_mut().unwrap().swap(0, 1),
            4 => p["writesSupported"] = json!(true),
            5 => p["readOnly"] = json!(false),
            6 => p["metadata"][0]["recordOffset"] = json!(8),
            7 => p["serializerBytes"] = json!(50325),
            _ => p["rows"][0]["nameRva"] = json!(START - 1),
        }
        assert!(
            fields_from(&p, &old).is_err(),
            "mutation {mutate} was accepted"
        );
    }
}
#[test]
fn directory_snapshot_is_bounded_read_only_and_details_are_lazy() {
    let m = Fake::fixture();
    let before = m.bytes.borrow().clone();
    let snap = capture_verified(&m, REGION).unwrap();
    let doc = snap.document();
    assert_eq!(doc["players"].as_array().unwrap().len(), 100);
    assert_eq!(doc["teams"][0]["kind"], 61);
    assert_eq!(doc["teams"][0]["id"], 42);
    assert_eq!(doc["players"][0]["metadata"]["UniqueId"], 1000);
    assert_eq!(doc["players"][0]["name"], "Test0 Player");
    assert!(doc["players"][0].get("ratingsRaw").is_none());
    assert_eq!(doc["readOnly"], true);
    assert_eq!(doc["writeAvailable"], false);
    let cached = snap.cached_players().unwrap();
    assert_eq!(cached.len(), 100);
    assert!(
        cached
            .iter()
            .all(|p| p["ratingsRaw"].as_array().unwrap().len() == 455
                && p["writeAvailable"] == false)
    );
    let details = details_verified(&m, &snap, 0).unwrap();
    assert_eq!(details["ratingsRaw"].as_array().unwrap().len(), 455);
    assert_eq!(details["writeAvailable"], false);
    assert!(!serde_json::to_string(doc).unwrap().contains("0x"));
    assert_eq!(*m.bytes.borrow(), before);
}
#[test]
fn fixture_capture_feeds_browser_editor_contract_without_writing() {
    let m = Fake::fixture();
    let before = m.bytes.borrow().clone();
    let snap = capture_verified(&m, REGION).unwrap();
    let doc = crate::roster_v2_access::browser_document(&snap).unwrap();
    assert_eq!(doc["status"], "ready");
    assert_eq!(doc["readOnly"], false);
    assert_eq!(doc["writes"], 0);
    assert_eq!(doc["writeAvailable"], true);
    assert_eq!(doc["source"], "version-2-owned");
    assert_eq!(doc["access"], "guarded-write");
    assert_eq!(doc["declaredPlayerCount"], 100);
    assert_eq!(doc["decodedPlayerCount"], 100);
    let players = doc["players"].as_array().unwrap();
    assert_eq!(players.len(), 100);
    assert_eq!(players[3]["name"], "Test3 Player");
    assert_eq!(players[3]["uniqueId"], 1003);
    assert_eq!(players[3]["recordIndex"], 3);
    assert!(players.iter().all(|p| p["source"] == "live-roster"
        && p["readOnly"] == false
        && p["writeAvailable"] == true
        && p["potential"].is_u64()
        && p["recordAddress"].is_null()
        && p["teamAddress"].is_null()
        && p["ratingsRaw"].as_array().unwrap().len() == 455));
    let team = &doc["teams"][0];
    assert_eq!(team["source"], "live-team");
    assert_eq!(team["teamId"], 42);
    assert_eq!(team["name"], "Test team");
    assert_eq!(team["rosterTypeId"], 61);
    assert_eq!(team["writeAvailable"], true);
    assert!(team["recordAddress"].is_null());
    assert_eq!(team["cityName"], "");
    assert_eq!(team["uniforms"], json!([]));
    assert_eq!(team["emptyUniformSlots"].as_array().unwrap().len(), 32);
    let text = serde_json::to_string(&doc).unwrap().to_ascii_lowercase();
    for address in [BASE, REGION.base, OWNER, PLAYERS, TEAMS, CONTAINER] {
        assert!(!text.contains(&address.to_string()));
        assert!(!text.contains(&format!("{address:x}")));
    }
    assert_eq!(*m.bytes.borrow(), before);
}
#[test]
fn rejects_changed_rows_or_owner_during_capture() {
    for (trigger, target, bytes) in [
        (PLAYERS, PLAYERS + 40, vec![88]),
        (TEAMS, TEAMS + 0xf20, vec![1]),
        (
            PLAYERS,
            CONTAINER + 0xa8,
            (OWNER + 8).to_le_bytes().to_vec(),
        ),
    ] {
        let m = Fake::fixture();
        *m.mutation.borrow_mut() = Some((trigger, 2, target, bytes));
        assert!(capture_verified(&m, REGION).is_err());
    }
}
#[test]
fn rejects_stale_player_details_and_out_of_bounds_selection() {
    let m = Fake::fixture();
    let snap = capture_verified(&m, REGION).unwrap();
    assert!(details_verified(&m, &snap, 100).is_err());
    m.put(PLAYERS + 0x110, &[7]);
    assert!(details_verified(&m, &snap, 0).is_err());
    let m = Fake::fixture();
    let snap = capture_verified(&m, REGION).unwrap();
    m.q(CONTAINER + 0xa8, OWNER + 8);
    assert!(details_verified(&m, &snap, 0).is_err());
}
#[test]
fn rejects_row_change_between_details_reads() {
    let m = Fake::fixture();
    let snap = capture_verified(&m, REGION).unwrap();
    let seen = *m.seen.borrow().get(&PLAYERS).unwrap();
    *m.mutation.borrow_mut() = Some((PLAYERS, seen + 2, PLAYERS + 0x110, vec![9]));
    assert!(details_verified(&m, &snap, 0).is_err());
}
#[test]
fn rejects_bad_names_partial_reads_and_ended_sessions() {
    let m = Fake::fixture();
    m.put(PLAYERS, &[0, 0xd8, 0, 0]);
    assert!(capture_verified(&m, REGION).is_err());
    let m = Fake::fixture();
    m.text(PLAYERS, "Bad\nName");
    assert!(capture_verified(&m, REGION).is_err());
    let m = Fake::fixture();
    m.short.set(PLAYERS);
    assert!(capture_verified(&m, REGION).is_err());
    let m = Fake::fixture();
    m.ended.set(true);
    assert!(capture_verified(&m, REGION).is_err());
    let m = Fake::fixture();
    m.put(PLAYERS, &vec![0; owner::PLAYER_STRIDE]);
    assert!(capture_verified(&m, REGION).is_err());
}
#[test]
fn no_read_only_capture_can_bypass_the_real_body_fingerprint() {
    let m = Fake::fixture();
    m.put(BASE + START, &vec![0; LENGTH]);
    assert!(capture(&m, REGION).is_err());
}

#[test]
#[ignore]
fn exact_version_two_field_profile_is_verified_from_disk_only() {
    use std::{
        fs::File,
        io::{Read, Seek, SeekFrom},
        path::PathBuf,
    };
    let path = PathBuf::from(
        std::env::var_os("C27_ROSTER_GAME_EXE").expect("Explicit disk image required"),
    );
    let mut file = File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut chunk = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut chunk).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&chunk[..n]);
    }
    assert_eq!(format!("{:X}", hash.finalize()), IMAGE_HASH);
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut header = vec![0; 4096];
    file.read_exact(&mut header).unwrap();
    let pe = u32::from_le_bytes(header[60..64].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(header[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let opt = u16::from_le_bytes(header[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let base = u64::from_le_bytes(header[pe + 48..pe + 56].try_into().unwrap());
    let mut sections = Vec::new();
    for i in 0..count {
        let at = pe + 24 + opt + i * 40;
        sections.push((
            u32::from_le_bytes(header[at + 12..at + 16].try_into().unwrap()) as u64,
            u32::from_le_bytes(header[at + 16..at + 20].try_into().unwrap()) as u64,
            u32::from_le_bytes(header[at + 20..at + 24].try_into().unwrap()) as u64,
        ));
    }
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
            let rva = at.checked_sub(self.base).context("Below image")?;
            if rva == 0 && len == 4096 {
                return Ok(self.header.clone());
            }
            let (start, _, raw) = self
                .sections
                .iter()
                .find(|(s, n, _)| {
                    rva >= *s && rva.checked_add(len as u64).is_some_and(|end| end <= s + n)
                })
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
    verify_profile(&m).unwrap();
    println!(
        "{}",
        json!({"pass":true,"packedPlayerFields":455,"basicTeamIdNameAndKind":true,"uniformHeaderOwnerRelative":true,"fullSerializerBytes":LENGTH,"serializerSha256":BODY_HASH,"writesSupported":false,"gameProcessAccess":false,"gameFilesChanged":false})
    );
}
const UNIFORMS: u64 = REGION.base + 0x300000;
/// Legacy oracle team + uniform rows placed in the owned V2 directories.
fn oracle(case: usize) -> (Fake, Value) {
    let rows = crate::uniform_changes::tests::fixture();
    let c = rows[case].clone();
    let m = Fake::fixture();
    let mut team = crate::uniform_changes::tests::bytes(&c["teamBefore"]);
    let table: u64 = c["uniformTable"].as_str().unwrap().parse().unwrap();
    let count = c["uniformCount"].as_u64().unwrap();
    // Relocate only the oracle's existing slot pointers into the owned table.
    for slot in 0..32 {
        let at = 0xde0 + slot * 8;
        let pointer = u64::from_le_bytes(team[at..at + 8].try_into().unwrap());
        if pointer >= table && pointer < table + count * 0x170 {
            team[at..at + 8].copy_from_slice(&(UNIFORMS + pointer - table).to_le_bytes());
        }
    }
    m.put(TEAMS, &team);
    m.descriptor(0x290, count, UNIFORMS);
    m.put(
        UNIFORMS,
        &crate::uniform_changes::tests::bytes(&c["allUniformsBefore"]),
    );
    (m, c)
}
#[test]
fn team_text_slots_and_uniform_cards_match_the_legacy_oracle_decoder() {
    let (m, c) = oracle(1);
    let before = m.bytes.borrow().clone();
    let snap = capture_verified(&m, REGION).unwrap();
    let doc = crate::roster_v2_access::browser_document(&snap).unwrap();
    let team = &doc["teams"][0];
    let expected = &c["cached"];
    for key in [
        "name",
        "cityName",
        "cityShort",
        "stateShort",
        "mainLogo",
        "teamId",
    ] {
        assert_eq!(team[key], expected[key], "{key}");
    }
    let uniforms = team["uniforms"].as_array().unwrap();
    for key in [
        "ownedUniformSlotCount",
        "invalidUniformSlotCount",
        "emptyUniformSlots",
    ] {
        assert_eq!(team[key], expected[key], "{key}");
    }
    assert_eq!(
        uniforms.len(),
        expected["uniforms"].as_array().unwrap().len()
    );
    let legacy = &expected["uniforms"][0];
    let v2 = uniforms
        .iter()
        .find(|u| u["slot"] == legacy["slot"])
        .unwrap();
    let mut compared = 0;
    for (key, value) in legacy.as_object().unwrap() {
        if matches!(key.as_str(), "recordIndex" | "recordAddress")
            || key.starts_with("customUniform")
            || key.starts_with("baked")
        {
            continue;
        }
        if key == "hasCustomUniformData" {
            assert_eq!(v2[key], false);
            continue;
        }
        assert_eq!(v2[key], *value, "{key}");
        compared += 1;
    }
    assert!(compared >= 80, "{compared}");
    assert!(v2["recordAddress"].is_null());
    assert_eq!(*m.bytes.borrow(), before);
}
#[test]
fn v2_documents_feed_the_uniform_planner_end_to_end_with_oracle_bytes() {
    let mut planned = 0;
    for case in 0..crate::uniform_changes::tests::fixture()
        .as_array()
        .unwrap()
        .len()
    {
        let (m, c) = oracle(case);
        let Ok(snap) = capture_verified(&m, REGION) else {
            continue;
        };
        let doc = crate::roster_v2_access::browser_document(&snap).unwrap();
        let team = &doc["teams"][0];
        let slot = c["input"]["slot"].as_u64();
        let Some(uniform) = team["uniforms"]
            .as_array()
            .and_then(|u| u.iter().find(|u| u["slot"].as_u64() == slot).or(u.first()))
        else {
            continue;
        };
        let index = uniform["recordIndex"].as_u64().unwrap() as usize;
        let mut input = c["input"].clone();
        input["recordIndex"] = json!(0);
        let result = writer::plan_uniform(
            &input,
            team,
            uniform,
            snap.row(Kind::Team, 0).unwrap(),
            snap.row(Kind::Uniform, index).unwrap(),
            UNIFORMS + (index * 0x170) as u64,
            index,
        );
        assert_eq!(result.is_ok(), c["ok"] == true, "case {case}");
        if let Ok((_, plan)) = result {
            assert_eq!(
                plan.after,
                crate::uniform_changes::tests::bytes(&c["after"]),
                "case {case}"
            );
            planned += 1;
        }
    }
    assert!(planned > 50, "{planned}");
}
#[test]
fn verified_read_back_is_adopted_in_place_and_corrupt_caches_are_refused() {
    let (m, _) = oracle(1);
    let mut snap = capture_verified(&m, REGION).unwrap();
    let mut player = snap.row(Kind::Player, 3).unwrap().to_vec();
    player[40..80].fill(0);
    for (i, w) in "Saved".encode_utf16().enumerate() {
        player[40 + i * 2..42 + i * 2].copy_from_slice(&w.to_le_bytes());
    }
    snap.replace(Kind::Player, 3, &player).unwrap();
    assert_eq!(snap.document()["players"][3]["name"], "Saved Player");
    assert_eq!(snap.row(Kind::Player, 3).unwrap(), &player[..]);
    let mut team = snap.row(Kind::Team, 0).unwrap().to_vec();
    team[0x32c..0x32c + 36].fill(0);
    team[0x32c..0x32c + 4].copy_from_slice(&[b'Q', 0, b'A', 0]);
    snap.replace(Kind::Team, 0, &team).unwrap();
    assert_eq!(snap.document()["teams"][0]["cityName"], "QA");
    let mut uniform = snap.row(Kind::Uniform, 0).unwrap().to_vec();
    uniform[0x114..0x118].copy_from_slice(&0xFF11_2233u32.to_le_bytes());
    snap.replace(Kind::Uniform, 0, &uniform).unwrap();
    assert_eq!(
        snap.document()["teams"][0]["uniforms"][0]["primaryColor"],
        0xFF11_2233u32
    );
    assert!(snap.replace(Kind::Uniform, 4, &uniform).is_err());
    assert!(snap.replace(Kind::Team, 0, &team[1..]).is_err());
    // A player renamed to nothing would leave fewer than 100 named players.
    let mut blank = snap.row(Kind::Player, 0).unwrap().to_vec();
    blank[..80].fill(0);
    assert!(snap.replace(Kind::Player, 0, &blank).is_err());
}
#[test]
fn player_team_links_and_unreferenced_uniforms_follow_the_live_proven_layout() {
    let (m, c) = oracle(1);
    m.put(PLAYERS + 0x60, &TEAMS.to_le_bytes());
    m.put(PLAYERS + 0x4F8 + 0x60, &(TEAMS + 0x10).to_le_bytes());
    let snap = capture_verified(&m, REGION).unwrap();
    let doc = crate::roster_v2_access::browser_document(&snap).unwrap();
    assert_eq!(doc["players"][0]["teamIndex"], 0);
    assert_eq!(doc["players"][0]["teamName"], c["cached"]["name"]);
    assert!(
        doc["players"][1]["teamIndex"].is_null(),
        "misaligned pointer"
    );
    assert!(doc["players"][2]["teamIndex"].is_null());
    assert!(doc["players"][0]["teamAddress"].is_null());
    assert_eq!(doc["verifiedTeamLinks"], 1);
    let loose = doc["unreferencedPopulatedUniforms"].as_array().unwrap();
    let owned: Vec<_> = doc["teams"][0]["uniforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["recordIndex"].clone())
        .collect();
    for u in loose {
        assert!(!owned.contains(&u["recordIndex"]));
        assert!(!u["baseFilename"].as_str().unwrap().is_empty());
        assert_eq!(
            u["stockResourceAvailable"], false,
            "no manifest in fixtures"
        );
        assert!(u["recordAddress"].is_null());
    }
    assert_eq!(
        loose.len() + owned.len(),
        snap.unreferenced_uniforms().unwrap().len() + owned.len()
    );
}
#[test]
fn verified_replacement_rebuilds_cached_audio_without_reads() {
    let m = Fake::fixture();
    let mut snap = capture_verified(&m, REGION).unwrap();
    let before = m.seen.borrow().clone();
    let mut row = snap.row(Kind::Player, 0).unwrap().to_vec();
    row[0x2e8..0x2ea].copy_from_slice(&(-1i16).to_le_bytes());
    row[0x50..0x58].copy_from_slice(&1u64.to_le_bytes());
    snap.replace(Kind::Player, 0, &row).unwrap();
    let players = snap.cached_players().unwrap();
    let audio = &players[0]["audio"];
    assert_eq!(audio["fullNameId"], -1);
    assert_eq!(audio["nameOverrideActive"], true);
    assert!(audio["lastName"].is_null());
    assert_eq!(*m.seen.borrow(), before);
}

#[test]
fn native_origins_are_joined_and_rebuilt_after_verified_pointer_change_without_new_reads() {
    let m = Fake::fixture();
    let proof: Value =
        serde_json::from_str(include_str!("../assets/roster-v2-origins-proof.json")).unwrap();
    for method in proof["methods"].as_array().unwrap() {
        let text = method["bytesHex"].as_str().unwrap();
        let bytes: Vec<u8> = (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect();
        m.put(BASE + method["rva"].as_u64().unwrap(), &bytes);
    }
    let table = REGION.base + 0x300000;
    m.descriptor(0x90, 2, table);
    let mut rows = vec![0; 144];
    rows[..4].copy_from_slice(b"One\0");
    rows[72..76].copy_from_slice(b"Two\0");
    m.put(table, &rows);
    let mut snap = capture_verified(&m, REGION).unwrap();
    let oldid = snap.origins().unwrap().id().to_owned();
    assert_eq!(
        snap.cached_players().unwrap()[0]["originState"],
        "unassigned"
    );
    let before = m.seen.borrow().clone();
    let mut player = snap.row(Kind::Player, 0).unwrap().to_vec();
    player[0xb0..0xb8].copy_from_slice(&(table + 72).to_le_bytes());
    snap.replace(Kind::Player, 0, &player).unwrap();
    let value = &snap.cached_players().unwrap()[0];
    assert_eq!(value["nativeOriginIndex"], 1);
    assert_eq!(value["collegeName"], "Two");
    assert_eq!(value["nativeOriginCatalogId"], oldid);
    assert_eq!(*m.seen.borrow(), before);
}
