use super::*;
use catalog::Track;
use serde_json::Map;
use std::io::Cursor;
fn music(n: u16) -> Music {
    Music {
        tracks: (1..=n)
            .map(|i| Track {
                artist: format!("Artist{i}"),
                collection: 1,
                cover: format!("cover_{i:03}"),
                name: format!("Song{i}"),
                bank: i,
                unique: i - 1,
                extra: Map::new(),
            })
            .collect(),
        extra: Map::new(),
    }
}
fn manifest(n: u16) -> String {
    (1..=n)
        .map(|i| {
            format!(
                "shared/{:02x}/cover_{i:03}.{i:016x}.tld,0M,{},32800\n",
                i % 256,
                u64::from(i) * 32800
            )
        })
        .collect()
}
fn slots(stock: &Music) -> Slots {
    Slots::parse(
        Cursor::new(manifest(stock.tracks.len() as u16)),
        stock,
        || Ok(()),
    )
    .unwrap()
}
fn tld(value: u8) -> Vec<u8> {
    let mut b = vec![value; NATIVE_BYTES];
    b[..32].fill(0);
    b[..4].copy_from_slice(b"TLD ");
    b[10] = 71;
    b[11] = 1;
    b[16..18].copy_from_slice(&256u16.to_le_bytes());
    b[18..20].copy_from_slice(&256u16.to_le_bytes());
    b[24..28].copy_from_slice(&32768u32.to_le_bytes());
    b[28..32].copy_from_slice(&32768u32.to_le_bytes());
    b
}
#[test]
fn actual_stock_93_identity_and_normal_audio_remap_are_preserved_with_real_artwork() {
    let stock = music(93);
    let selected = stock
        .selected(&[stock.tracks[92].key(), stock.tracks[0].key()])
        .unwrap();
    let wanted = BTreeMap::from([(selected.tracks[0].key(), tld(7))]);
    let p = prepare(
        &selected,
        &stock,
        &slots(&stock),
        &wanted,
        &BTreeSet::new(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(p.plan().mappings.len(), 1);
    assert!(p.plan().mappings[0].source_is_stock);
    assert_eq!(p.plan().music.tracks[0].bank, 2);
    assert_eq!(p.plan().music.tracks[0].name, "Song93");
    assert_eq!(p.plan().music.tracks[1].cover, "cover_001");
    assert_ne!(p.plan().music.tracks[0].cover, "cover_001");
    assert_eq!(p.bindings().len(), 1);
    assert_eq!(p.routes().len(), 2);
    assert_eq!(p.routes()[&p.bindings()[0].logical].as_slice(), tld(7));
}
#[test]
fn non_overridden_song_covers_and_case_insensitive_full_and_leaf_mods_are_reserved() {
    let stock = music(6);
    let selected = stock
        .selected(&[stock.tracks[0].key(), stock.tracks[5].key()])
        .unwrap();
    let art = BTreeMap::from([(stock.tracks[5].key(), tld(4))]);
    let occupied = BTreeSet::from([
        "SHARED\\02\\COVER_002.0000000000000002.TLD".into(),
        "COVER_003.0000000000000003.TLD".into(),
    ]);
    let p = prepare(
        &selected,
        &stock,
        &slots(&stock),
        &art,
        &occupied,
        || Ok(()),
    )
    .unwrap();
    assert_eq!(p.bindings()[0].cover, "cover_004");
    assert_eq!(p.plan().music.tracks[0].cover, "cover_001");
}
#[test]
fn identical_saved_artwork_uses_one_slot_and_two_immutable_routes() {
    let stock = music(3);
    let art = stock.tracks.iter().map(|t| (t.key(), tld(8))).collect();
    let p = prepare(
        &stock,
        &stock,
        &slots(&stock),
        &art,
        &BTreeSet::new(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(p.bindings().len(), 1);
    assert_eq!(p.bindings()[0].track_keys.len(), 3);
    assert_eq!(p.routes().len(), 2);
    assert!(
        p.plan()
            .music
            .tracks
            .iter()
            .all(|t| t.cover == p.bindings()[0].cover)
    );
    assert!(Arc::ptr_eq(
        &p.routes()[&p.bindings()[0].logical],
        &p.routes()[&p.bindings()[0].leaf]
    ));
}
#[test]
fn all_71_distinct_covers_have_exactly_142_typed_routes_and_bounded_native_permit() {
    let stock = music(71);
    let art = stock
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.key(), tld(i as u8)))
        .collect();
    let p = prepare(
        &stock,
        &stock,
        &slots(&stock),
        &art,
        &BTreeSet::new(),
        || Ok(()),
    )
    .unwrap();
    assert_eq!(p.bindings().len(), MAX_ARTWORK);
    assert_eq!(p.routes().len(), MAX_ROUTES);
    let (bytes, sha) = p.native_permit();
    assert_eq!(catalog::digest(bytes), sha);
    assert!(bytes.len() <= MAX_PERMIT);
    let rows = std::str::from_utf8(bytes)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(rows[0], PERMIT_SCHEMA);
    assert_eq!(rows[1], "71");
    assert_eq!(rows.len(), 73);
    assert!(
        p.routes()
            .keys()
            .all(|r| logical_cover(r).is_some() || !r.contains('/'))
    );
}
#[test]
fn insufficient_slots_and_cancellation_never_return_a_partial_publication() {
    let stock = music(2);
    let art = BTreeMap::from([
        (stock.tracks[0].key(), tld(4)),
        (stock.tracks[1].key(), tld(5)),
    ]);
    let occupied = BTreeSet::from([slots(&stock).entries[0].logical.clone()]);
    assert!(prepare(&stock, &stock, &slots(&stock), &art, &occupied, || Ok(())).is_err());
    assert!(
        prepare(
            &stock,
            &stock,
            &slots(&stock),
            &art,
            &BTreeSet::new(),
            || anyhow::bail!("cancelled")
        )
        .is_err()
    );
    assert!(
        Slots::parse(Cursor::new(manifest(2)), &stock, || anyhow::bail!(
            "cancelled"
        ))
        .is_err()
    );
}
#[test]
fn duplicate_or_ambiguous_manifest_cover_rows_fail_instead_of_borrowing_first() {
    let stock = music(2);
    let good = manifest(2);
    for bad in [
        format!("{good}{good}"),
        format!("{good}shared/ff/cover_001.ffffffffffffffff.tld,0A,0,32800\n"),
        good.replace(",0M,", ",../0M,"),
        good.replace(",32800\n", ",0\n"),
        good.replace(",32800\n", ",20971521\n"),
        "x".repeat(8193),
    ] {
        assert!(Slots::parse(Cursor::new(bad), &stock, || Ok(())).is_err());
    }
}
#[test]
fn only_exact_safe_full_stock_cover_paths_are_candidates() {
    for bad in [
        "shared/00/../x.tld",
        "shared/gg/cover_001.0000000000000001.tld",
        "shared/00/cover_001.x.tld",
        "shared/00/cover_001.0000000000000001.tld/extra",
        "shared/00/COVER_001.0000000000000001.tld",
        "C:/shared/00/cover_001.0000000000000001.tld",
        "shared/00/cover-001.0000000000000001.tld",
    ] {
        assert!(logical_cover(bad).is_none());
    }
    let stock = music(1);
    let unknown = "shared/00/not_stock.0000000000000001.tld,0A,0,32800\n";
    assert_eq!(
        Slots::parse(Cursor::new(unknown), &stock, || Ok(()))
            .unwrap()
            .available(),
        0
    );
}
#[test]
fn malformed_native_art_unselected_keys_wrong_stock_and_72_tracks_are_rejected() {
    let stock = music(3);
    let s = slots(&stock);
    let selected = stock.selected(&[stock.tracks[0].key()]).unwrap();
    let mut invalid = tld(3);
    invalid[10] = 72;
    for art in [
        BTreeMap::from([(selected.tracks[0].key(), invalid)]),
        BTreeMap::from([(stock.tracks[1].key(), tld(3))]),
    ] {
        assert!(prepare(&selected, &stock, &s, &art, &BTreeSet::new(), || Ok(())).is_err());
    }
    let mut changed = stock.clone();
    changed.tracks[0].cover = "another".into();
    assert!(
        prepare(
            &selected,
            &changed,
            &s,
            &BTreeMap::new(),
            &BTreeSet::new(),
            || Ok(())
        )
        .is_err()
    );
    let large = music(72);
    assert!(
        prepare(
            &large,
            &large,
            &slots(&large),
            &BTreeMap::new(),
            &BTreeSet::new(),
            || Ok(())
        )
        .is_err()
    );
}
#[test]
fn no_artwork_leaves_metadata_and_native_masks_exactly_unchanged() {
    let stock = music(3);
    let before = serde_json::to_vec(&stock).unwrap();
    let p = prepare(
        &stock,
        &stock,
        &slots(&stock),
        &BTreeMap::new(),
        &BTreeSet::new(),
        || Ok(()),
    )
    .unwrap();
    assert!(p.bindings().is_empty());
    assert!(p.routes().is_empty());
    assert_eq!(serde_json::to_vec(&p.plan().music).unwrap(), before);
    assert_eq!(
        p.plan().policy,
        catalog::plan_with_stock(&stock, &stock).unwrap().policy
    );
}

#[test]
fn native_permit_rejects_wrong_hash_noncanonical_oversized_and_ambiguous_routes() {
    let row = "shared/00/cover_001.0000000000000001.tld|".to_string() + &"a".repeat(64) + "\n";
    let good = format!("{PERMIT_SCHEMA}\n1\n{row}");
    assert_eq!(
        verify_native_permit(good.as_bytes(), &catalog::digest(good.as_bytes()))
            .unwrap()
            .len(),
        2
    );
    assert!(verify_native_permit(good.as_bytes(), &"0".repeat(64)).is_err());
    for bad in [
        format!("{PERMIT_SCHEMA}\n2\n{row}{row}"),
        good.replace("\n1\n", "\n01\n"),
        good.replace("shared/00/", "../shared/00/"),
        good.replace(".tld|", ".dll|"),
        good.replace('\n', "\r\n"),
        format!("{PERMIT_SCHEMA}\n72\n"),
        good.trim_end().to_string(),
        good.clone() + "extra\n",
        "x".repeat(MAX_PERMIT + 1),
    ] {
        assert!(verify_native_permit(bad.as_bytes(), &catalog::digest(bad.as_bytes())).is_err());
    }
}

#[test]
#[ignore = "Explicit read-only frozen saved-media and installed-manifest audit; no Main writes or game APIs"]
fn actual_saved_six_playlists_prepare_all_saved_native_artwork_read_only() {
    let stage = std::env::var_os("C27_JUKEBOX_ART_STAGE").expect("Explicit frozen stage required");
    let game = std::env::var_os("C27_JUKEBOX_ART_GAME")
        .expect("Explicit installed manifest root required");
    let stage = Path::new(&stage);
    let state: serde_json::Value =
        serde_json::from_slice(&fs::read(stage.join("Data/Jukebox/library.json")).unwrap())
            .unwrap();
    let music: Music = serde_json::from_value(state["music"].clone()).unwrap();
    let audio = state["audio"].as_object().unwrap();
    let stock = Music {
        tracks: music
            .tracks
            .iter()
            .filter(|t| !audio.contains_key(&t.key()))
            .cloned()
            .collect(),
        extra: music.extra.clone(),
    };
    assert_eq!(
        catalog::digest(&serde_json::to_vec(&stock).unwrap()),
        state["stockSha256"].as_str().unwrap()
    );
    assert_eq!(stock.tracks.len(), 164);
    // The isolated harness deliberately stubs all directory-capability opens.
    // Audit the same bounded parser against retained readonly manifest bytes;
    // production Slots::read adds its real ordinary parent/file identity pins.
    let manifest = fs::read(Path::new(&game).join("manifest")).unwrap();
    let slots = Slots::parse(Cursor::new(manifest), &stock, || Ok(())).unwrap();
    let mut playlists = 0usize;
    let mut total_bindings = 0usize;
    for playlist in state["playlists"].as_array().unwrap() {
        let keys = serde_json::from_value::<Vec<String>>(playlist["trackKeys"].clone()).unwrap();
        let selected = music.selected(&keys).unwrap();
        let mut artwork = BTreeMap::new();
        for key in &keys {
            if let Some(blob) = audio
                .get(key)
                .and_then(|a| a.get("artwork"))
                .filter(|b| !b.is_null())
            {
                let sha = blob["sha256"].as_str().unwrap();
                let size = blob["size"].as_u64().unwrap();
                let bytes = fs::read(
                    stage
                        .join("Library/Jukebox/artwork")
                        .join(format!("{sha}.tld")),
                )
                .unwrap();
                assert_eq!(bytes.len() as u64, size);
                assert_eq!(catalog::digest(&bytes), sha);
                artwork.insert(key.clone(), bytes);
            }
        }
        let p = prepare(
            &selected,
            &stock,
            &slots,
            &artwork,
            &BTreeSet::new(),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            p.bindings()
                .iter()
                .map(|b| b.track_keys.len())
                .sum::<usize>(),
            artwork.len()
        );
        total_bindings += p.bindings().len();
        playlists += 1;
    }
    assert_eq!(playlists, 6);
    println!(
        "PASS {playlists} saved playlists; {total_bindings} immutable artwork bindings; {} stock cover candidates; no publication or mutation",
        slots.available()
    );
}
