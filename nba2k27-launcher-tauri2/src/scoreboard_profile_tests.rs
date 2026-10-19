use super::*;
fn scalar(key: u32, value: u32) -> Vec<u8> {
    let mut b = vec![0; 96];
    b[..4].copy_from_slice(&key.to_le_bytes());
    b[4..8].copy_from_slice(&0x800005u32.to_le_bytes());
    b[8..12].copy_from_slice(&value.to_le_bytes());
    for i in 12..96 {
        b[i] = i as u8;
    }
    b
}
pub(super) fn fixture(id: &str, legacy: bool) -> BTreeMap<u64, Vec<u8>> {
    let c = catalog().unwrap().into_iter().find(|c| c.id == id).unwrap();
    let p = if legacy { LEGACY } else { c.profile };
    BTreeMap::from([
        (
            0x10000,
            scalar(
                if c.resource == STOCK { NAME } else { RESOURCE },
                c.resource,
            ),
        ),
        (0x10100, scalar(CLASS, p[0])),
        (0x10200, scalar(p[1], p[2])),
    ])
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
fn all_882_native_profile_transitions_match_unchanged_production_csharp() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/scoreboard-original-regressions.json"
    ))
    .unwrap();
    assert_eq!(oracle["schema"], "nba2k27-original-scoreboard-oracle-v1");
    assert_eq!(oracle["gameAccess"], false);
    let vectors = oracle["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 882);
    for row in vectors {
        let from = row["from"].as_str().unwrap();
        let to = row["to"].as_str().unwrap();
        let fields = fixture(from, row["legacy"].as_bool().unwrap());
        let wanted = catalog()
            .unwrap()
            .into_iter()
            .find(|c| c.id == to)
            .unwrap()
            .resource;
        let p = plan_resource(&fields, wanted).unwrap();
        assert_eq!(p.current as u64, row["current"].as_u64().unwrap());
        let changes:Vec<_>=p.changes.iter().map(|c|serde_json::json!({"address":c.address,"before":hex(&c.before[..12]),"after":hex(&c.after)})).collect();
        assert_eq!(
            serde_json::to_value(changes).unwrap(),
            row["changes"],
            "{from}->{to}"
        );
        let mut after = fields.clone();
        for c in &p.changes {
            assert_eq!(c.after.len(), 12);
            assert_eq!(&c.before[4..8], &c.after[4..8]);
            after.get_mut(&c.address).unwrap()[..12].copy_from_slice(&c.after);
        }
        assert!(plan_resource(&after, wanted).unwrap().changes.is_empty());
    }
}
#[test]
fn catalog_preserves_19_picker_ids_and_never_accepts_the_two_withdrawn_ids() {
    let catalog = catalog().unwrap();
    assert_eq!(catalog.len(), 21);
    assert_eq!(catalog.iter().filter(|c| c.selectable).count(), 19);
    for id in [
        "mycareer-story",
        "flashback-openrun",
        "unknown",
        "../default",
    ] {
        assert!(choice(id).is_err());
        assert!(plan(&fixture("default", false), id).is_err());
    }
    assert_eq!(migrate_saved("mycareer-story").unwrap().id, "broadcast");
    assert_eq!(migrate_saved("flashback-openrun").unwrap().id, "default");
    assert_eq!(choice("era-1990").unwrap().resource, 0xB81032F5);
    assert_eq!(
        choice("broadcast").unwrap().profile,
        [0x3A09A326, MODE, 0xAFA29832]
    );
}
#[test]
fn malformed_unknown_overlapping_or_mixed_controller_fields_are_rejected() {
    for mode in 0..9 {
        let mut fields = fixture("default", false);
        match mode {
            0 => {
                fields.get_mut(&0x10000).unwrap()[8] = 0;
            }
            1 => {
                fields.get_mut(&0x10100).unwrap()[8] = 0;
            }
            2 => {
                fields.get_mut(&0x10200).unwrap()[4] = 11;
            }
            3 => {
                fields.insert(0x10300, scalar(HANDLER, 0x759F1AB6));
            }
            4 => {
                let b = fields.remove(&0x10100).unwrap();
                fields.insert(0x10004, b);
            }
            5 => {
                fields.get_mut(&0x10000).unwrap().pop();
            }
            6 => {
                fields.insert(0x10300, scalar(CLASS, LEGACY[0]));
            }
            7 => {
                let bytes = fields.remove(&0x10200).unwrap();
                fields.insert(0x10201, bytes);
            }
            8 => {
                fields.clear();
            }
            _ => unreachable!(),
        }
        assert!(plan(&fields, "era-1990").is_err(), "mode {mode}");
    }
}
#[test]
fn full_property_progress_guard_preserves_unrelated_scalars_and_every_link_byte() {
    let mut original = fixture("default", false);
    original.insert(0x10300, scalar(0x12345678, 2));
    let p = plan(&original, "era-1990").unwrap();
    let mut current = original.clone();
    for n in 0..=p.changes.len() {
        expected_progress(&original, &p, n, &current).unwrap();
        let mut foreign = current.clone();
        foreign.get_mut(&0x10300).unwrap()[88] ^= 1;
        assert!(expected_progress(&original, &p, n, &foreign).is_err());
        if n < p.changes.len() {
            let c = &p.changes[n];
            current.get_mut(&c.address).unwrap()[..12].copy_from_slice(&c.after);
        }
    }
    assert!(expected_progress(&original, &p, p.changes.len() + 1, &current).is_err());
}
