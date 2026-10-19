use super::*;
fn fixture() -> (Value, Value, Value) {
    let mut totals = Map::new();
    for (key, _) in crate::player_dna::schema()["seasonLimits"]
        .as_object()
        .unwrap()
    {
        totals.insert(key.clone(), json!(0));
    }
    totals.insert("gp".into(), json!(50));
    totals.insert("doubleDoubles".into(), json!(10));
    let awards: Map<String, Value> = crate::player_dna::schema()["awards"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["key"].as_str().unwrap().into(), json!(0)))
        .collect();
    let old = json!([{ "slot":0,"year":2025,"team":"Team","secondaryTeam":null,"primaryTeamIndex":3,"secondaryTeamIndex":null,"linkedCount":1,"sharedWarning":null,"token":"native-token","totals":totals,"awards":awards }]);
    (
        json!([{ "year":2025,"team":"Source franchise","totals":{"points":100,"doubleDoubles":0} }]),
        old,
        json!([{ "index":3,"name":"Team","displayName":"Team · Normal","normal":true }]),
    )
}
fn options() -> Value {
    json!({"teamMappings":{"2025":{"primaryTeamIndex":3}},"normalTeamsOnly":true,"preserveTotals":{"2025":["doubleDoubles"]}})
}
#[test]
fn existing_review_preserves_missing_totals_awards_and_only_returns_native_slot_token_delta() {
    let (source, dest, teams) = fixture();
    let result = review(&source, &dest, &teams, &options()).unwrap();
    assert_eq!(
        result["plans"],
        json!([{ "slot":0,"token":"native-token","changes":{"points":100} }])
    );
    assert_eq!(result["capabilities"]["bulkApply"], false);
    assert_eq!(result["capabilities"]["maxStagedRows"], 1);
    assert!(result["rows"][0].get("primaryTeamIndex").is_none());
    assert_eq!(result["rows"][0]["totals"]["doubleDoubles"], 10);
}
#[test]
fn new_ambiguous_shared_and_wrong_native_team_fail_the_entire_review() {
    let (source, dest, teams) = fixture();
    let mut cases = Vec::new();
    let mut x = dest.clone();
    x[0]["year"] = json!(2024);
    cases.push(x);
    let mut x = dest.clone();
    let duplicate = x[0].clone();
    x.as_array_mut().unwrap().push(duplicate);
    cases.push(x);
    let mut x = dest.clone();
    x[0]["primaryTeamIndex"] = json!(2);
    cases.push(x);
    let mut x = dest.clone();
    x[0]["linkedCount"] = json!(2);
    cases.push(x);
    let mut x = dest.clone();
    x[0]["sharedWarning"] = json!("shared");
    cases.push(x);
    let mut x = dest.clone();
    x[0]["linkedCount"] = Value::Null;
    cases.push(x);
    for x in cases {
        assert!(review(&source, &x, &teams, &options()).is_err());
    }
    let mut both = source.clone();
    both.as_array_mut()
        .unwrap()
        .push(json!({"year":2024,"team":"Team","totals":{}}));
    assert!(review(&both, &dest, &teams, &options()).is_err());
}
#[test]
fn preserves_unspecified_fields_and_rejects_malformed_or_ambiguous_source() {
    let (mut source, dest, teams) = fixture();
    source[0]["totals"] = json!({"points":12});
    assert_eq!(
        review(&source, &dest, &teams, &options()).unwrap()["plans"][0]["changes"],
        json!({"points":12})
    );
    source[0]["totals"] = json!({"points":99999});
    assert!(review(&source, &dest, &teams, &options()).is_err());
    source[0]["totals"] = json!({"unknown":1});
    assert!(review(&source, &dest, &teams, &options()).is_err());
    let (source, _, _) = fixture();
    let mut o = options();
    o["preserveTotals"]["2025"] = json!(["unknown"]);
    assert!(review(&source, &dest, &teams, &o).is_err());
    let mut o = options();
    o["teamMappings"] = json!({});
    assert!(review(&source, &dest, &teams, &o).is_err());
    let mut source = source.clone();
    let duplicate = source[0].clone();
    source.as_array_mut().unwrap().push(duplicate);
    assert!(review(&source, &dest, &teams, &options()).is_err());
}
