//! Integration through the actual native metadata Service::request dispatcher.
//! Synthetic app roots only; no game, launch, availability, or enable calls.
use super::*;
fn root() -> PathBuf {
    let r = std::env::current_dir()
        .unwrap()
        .join("artifacts")
        .join(format!(
            "qa084-legacy-service-self-{}",
            backend::unique_id()
        ));
    fs::create_dir_all(r.join("Data/player-research")).unwrap();
    r
}
fn row(team: u16, target: u16) -> Value {
    json!({"teamId":team,"expectedTeamName":format!("Team {team}"),"donorArenaId":17,"targetArenaId":target,"arenaName":"Imported Court","arenaFile":"arena_import","floorFile":"floor_import"})
}
fn source(r: &Path, rows: Vec<Value>) -> (PathBuf, Vec<u8>) {
    let path = r.join("Data/player-research/custom-arena-profiles.json");
    let bytes=serde_json::to_vec_pretty(&json!({"schema":"nba2k27-custom-arena-profiles-v1","updatedAt":"2026-10-10T00:00:00Z","profiles":rows})).unwrap();
    fs::write(&path, &bytes).unwrap();
    (path, bytes)
}
fn review(s: &Service, r: &Path) -> Value {
    s.request(r, &json!({"action":"reviewLegacy"}), &|_| {
        panic!("Review cannot request enable consent")
    })
    .unwrap()
}
fn selection(v: &Value, indices: &[usize]) -> Value {
    json!({"action":"importLegacy","reviewToken":v["reviewToken"],"selectedIds":indices.iter().map(|i|v["entries"][*i]["id"].clone()).collect::<Vec<_>>()})
}
fn witness(r: &Path) -> (crate::file_identity::Stable, Vec<u8>) {
    let p = path(r);
    let f = fs::File::open(&p).unwrap();
    (
        crate::file_identity::Stable::read(&f).unwrap(),
        fs::read(p).unwrap(),
    )
}
fn existing() -> Profile {
    Profile {
        id: "f".repeat(48),
        team_id: 8,
        expected_team_name: "Team 8".into(),
        donor_arena_id: 17,
        target_arena_id: 1000,
        arena_name: "Existing".into(),
        arena_file: "arena_existing".into(),
        floor_file: "floor_existing".into(),
        enabled: true,
    }
}
#[test]
fn service_review_is_readonly_and_does_not_create_or_activate_v2_store() {
    let r = root();
    let (p, b) = source(&r, vec![row(7, 999)]);
    let s = Service::default();
    let v = review(&s, &r);
    assert_eq!(v["entries"][0]["valid"], true);
    assert_eq!(v["memoryWrites"], 0);
    assert_eq!(v["activation"], false);
    assert!(!path(&r).exists());
    assert_eq!(fs::read(p).unwrap(), b);
    assert!(s.launch.lock().unwrap().is_none());
}
#[test]
fn service_import_persists_only_selected_disabled_profiles_without_enable_consent() {
    let r = root();
    let (p, b) = source(&r, vec![row(7, 999), row(9, 1001)]);
    let s = Service::default();
    let v = review(&s, &r);
    let result = s
        .request(&r, &selection(&v, &[1]), &|_| {
            panic!("Import cannot enable a profile")
        })
        .unwrap();
    assert_eq!(result["importedCount"], 1);
    assert_eq!(result["activation"], false);
    assert_eq!(result["legacyUnchanged"], true);
    let store = read(&r).unwrap();
    assert_eq!(store.profiles.len(), 1);
    assert_eq!(store.profiles[0].team_id, 9);
    assert!(!store.profiles[0].enabled);
    assert!(s.launch.lock().unwrap().is_none());
    assert_eq!(fs::read(p).unwrap(), b);
}
#[test]
fn service_rejects_stale_source_token_without_replacing_existing_v2_file() {
    let r = root();
    source(&r, vec![row(7, 999)]);
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![existing()],
        },
    )
    .unwrap();
    let before = witness(&r);
    let s = Service::default();
    let v = review(&s, &r);
    let (_, fresh) = source(&r, vec![row(7, 999), row(9, 1001)]);
    assert!(s.request(&r, &selection(&v, &[0]), &|_| panic!()).is_err());
    assert_eq!(witness(&r), before);
    assert_eq!(
        fs::read(r.join("Data/player-research/custom-arena-profiles.json")).unwrap(),
        fresh
    );
}
#[test]
fn service_all_batch_conflict_preserves_unrelated_enabled_profile_and_exact_v2_file() {
    let r = root();
    source(&r, vec![row(7, 999), row(9, 1000)]);
    let p = existing();
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![p.clone()],
        },
    )
    .unwrap();
    let before = witness(&r);
    let s = Service::default();
    let v = review(&s, &r);
    assert!(
        s.request(&r, &selection(&v, &[0, 1]), &|_| panic!())
            .is_err()
    );
    assert_eq!(witness(&r), before);
    assert_eq!(read(&r).unwrap().profiles, vec![p]);
}
#[test]
fn service_ambiguous_or_duplicate_selected_entries_never_create_v2_store() {
    let r = root();
    source(&r, vec![row(7, 999), row(8, 999)]);
    let s = Service::default();
    let v = review(&s, &r);
    assert_eq!(v["entries"][0]["valid"], false);
    assert!(s.request(&r, &selection(&v, &[0]), &|_| panic!()).is_err());
    assert!(!path(&r).exists());
    source(&r, vec![row(7, 999)]);
    let v = review(&s, &r);
    assert!(
        s.request(&r, &selection(&v, &[0, 0]), &|_| panic!())
            .is_err()
    );
    assert!(!path(&r).exists());
}
#[test]
fn service_known_action_schema_refuses_caller_paths_activation_and_foreign_selection() {
    let r = root();
    source(&r, vec![row(7, 999)]);
    let s = Service::default();
    let v = review(&s, &r);
    for input in [
        json!({"action":"reviewLegacy","path":"C:/foreign"}),
        {
            let mut x = selection(&v, &[0]);
            x["enabled"] = json!(true);
            x
        },
        {
            let mut x = selection(&v, &[0]);
            x["selectedIds"] = json!(["foreign"]);
            x
        },
    ] {
        assert!(s.request(&r, &input, &|_| panic!()).is_err());
        assert!(!path(&r).exists());
        assert!(s.launch.lock().unwrap().is_none());
    }
}
