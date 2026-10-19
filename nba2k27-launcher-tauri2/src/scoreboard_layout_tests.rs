use super::*;
fn english(c: &Catalog) -> Vec<u8> {
    let pool = 192;
    let mut text = vec![0; pool];
    text[24..32].copy_from_slice(&(pool as u64).to_le_bytes());
    for (i, t) in c.targets.iter().enumerate() {
        let slot = 64 + (i / 5) * 64 + (i % 5) * 8;
        let off = (text.len() - pool) / 2;
        text[slot..slot + 8]
            .copy_from_slice(&u64::from_str_radix(&t.key, 16).unwrap().to_le_bytes());
        let f = slot / 64 * 64 + 40 + (slot % 64 / 8) * 4;
        text[f..f + 4].copy_from_slice(&(off as u32).to_le_bytes());
        text.extend(t.text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
    }
    selector_tables::pack(&BTreeMap::from([("TEXT.VCLOCALIZEDATA".into(), text)])).unwrap()
}
#[test]
fn adapter_catalog_contains_only_five_required_layouts_and_existing_english_targets() {
    let c = catalog().unwrap();
    assert_eq!(
        c.layouts
            .iter()
            .map(|l| l.plans[0].edits.len())
            .sum::<usize>(),
        23
    );
    let zip = selector_tables::archive(&english(&c)).unwrap();
    let text = &zip["TEXT.VCLOCALIZEDATA"];
    for t in c.targets {
        assert_eq!(
            resolve(text, u64::from_str_radix(&t.key, 16).unwrap()).unwrap(),
            t.text
        );
    }
    assert!(
        PATHS
            .iter()
            .all(|p| !p.contains("openrun") && !p.contains("english"))
    );
}
#[test]
fn exact_fixed_width_patch_preserves_every_neighbor_and_untouched_member() {
    let original: Vec<u8> = (0..200).map(|i| i as u8).collect();
    let plan = Member {
        member: "Scoreboard.VCUIELEMENT".into(),
        source_sha256: hash(&original),
        edits: vec![Edit {
            offset: 20,
            before: original[20..28]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            after: "0102030405060708".into(),
        }],
    };
    let patched = patch_member(&original, &plan).unwrap();
    assert_eq!(&patched[..20], &original[..20]);
    assert_eq!(&patched[28..], &original[28..]);
    let source = selector_tables::pack(&BTreeMap::from([
        (plan.member.clone(), original.clone()),
        ("Other.DDS".into(), vec![42; 99]),
    ]))
    .unwrap();
    let l = Layout {
        id: "fixture-only".into(),
        asset: "not-publishable".into(),
        plans: vec![plan],
    };
    let zip = selector_tables::archive(&patch(&source, &l).unwrap()).unwrap();
    assert_eq!(zip["Other.DDS"], vec![42; 99]);
    assert_eq!(zip["Scoreboard.VCUIELEMENT"], patched);
}
#[test]
fn patch_rejects_changed_source_before_bytes_overlap_and_truncation() {
    let raw = vec![0; 32];
    let p = Member {
        member: "Scoreboard.VCUIELEMENT".into(),
        source_sha256: hash(&raw),
        edits: vec![Edit {
            offset: 16,
            before: "0000000000000000".into(),
            after: "0102030405060708".into(),
        }],
    };
    let mut changed = raw.clone();
    changed[0] = 1;
    assert!(patch_member(&changed, &p).is_err());
    let mut wrong = p.clone();
    wrong.edits[0].before = "0100000000000000".into();
    assert!(patch_member(&raw, &wrong).is_err());
    let mut duplicate = p.clone();
    duplicate.edits.push(duplicate.edits[0].clone());
    assert!(patch_member(&raw, &duplicate).is_err());
    let mut trunc = p;
    trunc.edits[0].offset = 30;
    assert!(patch_member(&raw, &trunc).is_err());
}
#[test]
fn preparation_never_accepts_unknown_stock_and_honors_cancel_before_source() {
    let c = catalog().unwrap();
    let e = english(&c);
    let mut calls = Vec::new();
    assert!(
        prepare(
            |p| {
                calls.push(p.to_owned());
                Ok(e.clone())
            },
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(calls, ["loc/english.iff", PATHS[0]]);
    let mut n = 0;
    assert!(
        prepare(
            |_| {
                n += 1;
                Ok(e.clone())
            },
            || anyhow::bail!("cancelled")
        )
        .is_err()
    );
    assert_eq!(n, 0);
}
