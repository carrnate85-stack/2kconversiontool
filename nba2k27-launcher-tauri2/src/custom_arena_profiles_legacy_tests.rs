use super::*;
fn root() -> PathBuf {
    let r = std::env::current_dir()
        .unwrap()
        .join("artifacts")
        .join(format!(
            "qa084-legacy-profile-self-{}",
            backend::unique_id()
        ));
    fs::create_dir_all(r.join("Data/player-research")).unwrap();
    r
}
fn row(team: u16, target: u16) -> Value {
    json!({"teamId":team,"expectedTeamName":format!("Team {team}"),"targetArenaId":target,"donorArenaId":17,"arenaName":"Legacy Court","arenaFile":"arena_legacy","floorFile":"floor_legacy"})
}
fn source(root: &Path, profiles: Vec<Value>) -> (PathBuf, Vec<u8>) {
    let p = root.join("Data/player-research/custom-arena-profiles.json");
    let bytes = serde_json::to_vec_pretty(
        &json!({"schema":LEGACY_SCHEMA,"updatedAt":"2026-10-10T00:00:00Z","profiles":profiles}),
    )
    .unwrap();
    fs::write(&p, &bytes).unwrap();
    (p, bytes)
}
fn select(review: &Value, indices: &[usize]) -> Value {
    json!({"reviewToken":review["reviewToken"],"selectedIds":indices.iter().map(|i|review["entries"][*i]["id"].clone()).collect::<Vec<_>>()})
}
fn existing(team: u16, target: u16) -> Profile {
    let (p, issues) = decode(&row(team, target));
    assert!(issues.is_empty());
    let mut p = p.unwrap();
    p.id = "f".repeat(48);
    p.enabled = true;
    p
}
#[test]
fn explicit_selected_import_preserves_unrelated_v2_and_legacy_bytes_and_never_enables() {
    let r = root();
    let (path, bytes) = source(&r, vec![row(7, 999), row(9, 1000)]);
    let reviewed = review(&r).unwrap();
    assert_eq!(reviewed["activation"], false);
    assert_eq!(reviewed["entries"][0]["valid"], true);
    let old = existing(8, 1001);
    let mut store = Store {
        schema: SCHEMA.into(),
        profiles: vec![old.clone()],
    };
    let result = import(&r, &select(&reviewed, &[0]), &mut store).unwrap();
    assert_eq!(result["importedCount"], 1);
    assert_eq!(result["memoryWrites"], 0);
    assert_eq!(store.profiles[0], old);
    assert_eq!(store.profiles.len(), 2);
    assert!(!store.profiles[1].enabled);
    assert_ne!(store.profiles[1].id, "0".repeat(48));
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn file_replacement_even_equal_bytes_and_inplace_changes_invalidate_review_token() {
    for replacement in [false, true] {
        let r = root();
        let (path, bytes) = source(&r, vec![row(7, 999)]);
        let reviewed = review(&r).unwrap();
        if replacement {
            fs::rename(&path, r.join("old.json")).unwrap();
            fs::write(&path, &bytes).unwrap();
        } else {
            source(&r, vec![row(7, 999), row(9, 1000)]);
        }
        let mut store = Store::default();
        assert!(import(&r, &select(&reviewed, &[0]), &mut store).is_err());
        assert!(store.profiles.is_empty());
    }
}
#[test]
fn ambiguous_source_targets_and_teams_are_all_invalid_without_first_entry_adoption() {
    for entries in [
        vec![row(7, 999), row(8, 999)],
        vec![row(7, 999), row(7, 1000)],
    ] {
        let r = root();
        source(&r, entries);
        let reviewed = review(&r).unwrap();
        for i in 0..2 {
            assert_eq!(reviewed["entries"][i]["valid"], false);
            assert!(
                reviewed["entries"][i]["issues"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["field"] == "targetArenaId/teamId")
            );
        }
        let mut store = Store::default();
        assert!(import(&r, &select(&reviewed, &[0]), &mut store).is_err());
        assert!(store.profiles.is_empty());
    }
}
#[test]
fn field_path_activation_address_and_bound_errors_are_reviewed_not_silently_imported() {
    let mut entries = Vec::new();
    for (key, value) in [
        ("arenaFile", json!("../arena")),
        ("floorFile", json!("C:\\foreign")),
        ("arenaName", json!("x".repeat(35))),
        ("targetArenaId", json!(995)),
        ("teamId", json!(-1)),
        ("enabled", json!(true)),
        ("address", json!(0x20000000u64)),
    ] {
        let mut v = row(7, 999);
        v[key] = value;
        entries.push(v);
    }
    let r = root();
    source(&r, entries);
    let reviewed = review(&r).unwrap();
    for entry in reviewed["entries"].as_array().unwrap() {
        assert_eq!(entry["valid"], false);
        assert!(entry["profile"].is_null());
        assert!(!entry["issues"].as_array().unwrap().is_empty());
    }
    let mut store = Store::default();
    assert!(import(&r, &select(&reviewed, &[0]), &mut store).is_err());
}
#[test]
fn whole_selected_batch_refuses_conflict_invalid_ids_duplicate_selection_and_capacity() {
    let r = root();
    source(&r, vec![row(7, 999), row(9, 1000)]);
    let reviewed = review(&r).unwrap();
    let old = existing(10, 1000);
    let mut store = Store {
        schema: SCHEMA.into(),
        profiles: vec![old],
    };
    let before = serde_json::to_value(&store).unwrap();
    assert!(import(&r, &select(&reviewed, &[0, 1]), &mut store).is_err());
    assert_eq!(serde_json::to_value(&store).unwrap(), before);
    for input in [
        select(&reviewed, &[0, 0]),
        json!({"reviewToken":reviewed["reviewToken"],"selectedIds":["foreign"]}),
        json!({"reviewToken":reviewed["reviewToken"],"selectedIds":[]}),
        json!({"reviewToken":reviewed["reviewToken"],"selectedIds":[reviewed["entries"][0]["id"]],"address":7}),
    ] {
        let mut store = Store::default();
        assert!(import(&r, &input, &mut store).is_err());
        assert!(store.profiles.is_empty());
    }
}
#[test]
fn readonly_retained_source_blocks_writes_and_replacement_missing_and_bad_schema_fail_closed() {
    let r = root();
    assert_eq!(review(&r).unwrap()["legacyPresent"], false);
    let (path, _) = source(&r, vec![row(7, 999)]);
    let pin = read_source(&r).unwrap().unwrap();
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::rename(&path, r.join("moved.json")).is_err());
    drop(pin);
    fs::write(
        &path,
        serde_json::to_vec(&json!({"schema":"future-unsupported","profiles":[]})).unwrap(),
    )
    .unwrap();
    assert!(review(&r).is_err());
    fs::write(&path, vec![b' '; LIMIT + 1]).unwrap();
    assert!(review(&r).is_err());
}
