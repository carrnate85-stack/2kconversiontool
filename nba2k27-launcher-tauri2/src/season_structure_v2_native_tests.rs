//! Pure codecs/address/prefix/journal tests. No game/process/native-memory access.
use super::*;
fn fixture() -> (Captured, Selector, IntentMeta) {
    let mut player = [0u8; graph::PLAYER_BYTES];
    player[0x130..0x130 + 66].fill(255);
    player[0x130..0x132].copy_from_slice(&0u16.to_le_bytes());
    player[0x128..0x12a].copy_from_slice(&7u16.to_le_bytes());
    let team = 0x400000u64;
    let mut rows = vec![[0u8; 64]; 3];
    rows[0][..8].copy_from_slice(&team.to_le_bytes());
    rows[0][17] = 100;
    rows[0][18..20].copy_from_slice(&u16::MAX.to_le_bytes());
    rows[0][60..64].copy_from_slice(&(1u32 << 13).to_le_bytes());
    rows[1][18..20].copy_from_slice(&2u16.to_le_bytes());
    rows[2][18..20].copy_from_slice(&u16::MAX.to_le_bytes());
    let mut manager = [0; 56];
    manager[..2].copy_from_slice(&1u16.to_le_bytes());
    let graph = graph::Snapshot {
        players: vec![player],
        rows,
        manager,
        pool: graph::PoolKind::Player,
        manager_count: 1,
        teams: graph::TeamDirectory {
            base: team,
            count: 1,
        },
    };
    graph.validate().unwrap();
    let layout = Layout {
        image: 0x140000000,
        root: 0x900000,
        owner: 0x100680,
        allocation: 0x100000,
        allocation_bytes: 0x800000,
        players: 0x200000,
        player_count: 1,
        teams: team,
        team_count: 1,
        manager: 0x300000,
        rows: 0x300038,
        row_count: 3,
        headers: vec![0; 32],
    };
    let scope = vec![
        1,
        1,
        0x6509837b,
        0x6509837b,
        0,
        0,
        0x600000,
        0,
        8,
        0x14791e158,
    ];
    let before = Captured {
        layout: layout.clone(),
        graph,
        teams: vec![0; profile::TEAM_STRIDE],
        owner: vec![0; OWNER_BYTES],
        scope: scope.clone(),
    };
    let selector = Selector {
        action: "addOlder".into(),
        player_index: 0,
        expected_player_hash: digest(&before.graph.players[0]),
        expected_season_hash: digest(&before.graph.rows[0]),
        expected_slot: 0,
        expected_row_index: 0,
        slot: None,
        changes: None,
        import: None,
    };
    let meta = IntentMeta {
        schema: "nba2k27-season-structure-v1".into(),
        game: Identity {
            pid: 12,
            started: "13".into(),
            path: "C:\\owned\\NBA2K27.exe".into(),
        },
        parent: Identity {
            pid: 22,
            started: "23".into(),
            path: "C:\\owned\\launcher.exe".into(),
        },
        session: "0".repeat(48),
        transaction: "1".repeat(64),
        selector: selector.clone(),
        layout,
        scope,
    };
    (before, selector, meta)
}
#[test]
fn immutable_intent_codec_roundtrip_exact() {
    let (before, _, meta) = fixture();
    let raw = encode(&meta, &before).unwrap();
    let (m, out) = decode(&raw).unwrap();
    assert!(out.same(&before));
    assert_eq!(m.transaction, meta.transaction);
    assert_eq!(
        serde_json::to_value(m.selector).unwrap(),
        serde_json::to_value(meta.selector).unwrap()
    );
}
#[test]
fn codec_rejects_truncation_trailing_bad_magic_counts_and_noncanonical_model() {
    let (b, _, mut m) = fixture();
    let raw = encode(&m, &b).unwrap();
    for n in [0, 1, 7, 8, 11, 12, raw.len() - 1] {
        assert!(decode(&raw[..n]).is_err(), "truncated{n}");
    }
    let mut extra = raw.clone();
    extra.push(0);
    assert!(decode(&extra).is_err());
    let mut bad = raw.clone();
    bad[0] ^= 1;
    assert!(decode(&bad).is_err());
    m.layout.row_count = 32768;
    assert!(decode(&encode(&m, &b).unwrap()).is_err());
    let (mut bad, _, m) = fixture();
    bad.graph.rows[0][60..64].fill(0);
    assert!(decode(&encode(&m, &bad).unwrap()).is_err());
}
#[test]
fn admission_pins_both_player_and_target_season_and_oldest_slot() {
    let (before, s, _) = fixture();
    assert!(plan(&before, &s).is_ok());
    let mut b = before.clone();
    b.graph.players[0][0x12c] ^= 1;
    assert!(plan(&b, &s).is_err());
    let mut b = before.clone();
    b.graph.rows[0][20] ^= 1;
    assert!(plan(&b, &s).is_err());
    let mut s = s.clone();
    s.expected_slot = 1;
    assert!(plan(&before, &s).is_err());
    s.expected_slot = 0;
    s.expected_row_index = 2;
    assert!(plan(&before, &s).is_err());
}
#[test]
fn native_effect_addresses_are_only_derived_bounded_logical_spans() {
    let (b, s, _) = fixture();
    let p = plan(&b, &s).unwrap();
    let addresses: Vec<_> = p
        .steps()
        .iter()
        .map(|step| step_address(&b.layout, step).unwrap())
        .collect();
    assert_eq!(
        addresses,
        vec![
            b.layout.manager,
            b.layout.rows + 64,
            b.layout.players + 0x132
        ]
    );
    let mut bad = b.layout.clone();
    bad.players = u64::MAX - 1;
    assert!(step_address(&bad, &p.steps()[2]).is_err());
}
#[test]
fn every_owned_prefix_requires_all_external_dependency_bytes() {
    let (b, s, _) = fixture();
    let p = plan(&b, &s).unwrap();
    for n in 0..=p.steps().len() {
        let expected = p.expected_prefix(n).unwrap();
        let mut observed = b.clone();
        observed.graph = expected.clone();
        assert!(ensure_known(&observed, &b, &expected).is_ok());
        for kind in 0..4 {
            let mut changed = observed.clone();
            match kind {
                0 => changed.teams[12] ^= 1,
                1 => changed.owner[16] ^= 1,
                2 => changed.scope[8] ^= 1,
                _ => changed.layout.headers[4] ^= 1,
            };
            assert!(
                ensure_known(&changed, &b, &expected).is_err(),
                "prefix{n}/dependency{kind}"
            );
        }
    }
}
#[test]
fn every_byte_partial_write_is_unknown_or_an_exact_complete_prefix_only() {
    let (b, s, _) = fixture();
    let p = plan(&b, &s).unwrap();
    for (i, step) in p.steps().iter().enumerate() {
        for n in 0..=step.after.len() {
            let mut observed = p.expected_prefix(i).unwrap();
            let dst = match step.target {
                graph::Target::Manager => observed.manager.as_mut_slice(),
                graph::Target::Player(index) => observed.players[index].as_mut_slice(),
                graph::Target::Row(index) => observed.rows[index].as_mut_slice(),
            };
            dst[step.offset..step.offset + n].copy_from_slice(&step.after[..n]);
            let classified = p.inspect(&observed);
            if classified != graph::ModelState::Unknown {
                assert!(
                    (0..=p.steps().len()).any(|k| p.expected_prefix(k).as_ref() == Some(&observed))
                );
            }
        }
    }
}
#[test]
fn disk_verified_file_never_grants_live_pending_permission() {
    let (b, _, m) = fixture();
    let root = std::env::temp_dir().join(format!(
        "C27-season-journal-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir = directory(&root).unwrap();
    let p = dir.join(format!("{}.pending", m.transaction));
    let raw = encode(&m, &b).unwrap();
    let mut f = ordinary_new(&p).unwrap();
    f.write_all(&raw).unwrap();
    f.sync_all().unwrap();
    drop(f);
    fs::write(p.with_extension("verified"),serde_json::to_vec(&json!({"schema":"nba2k27-season-structure-verified-v1","game":m.game,"session":m.session,"intentSha256":digest(&raw),"status":"after","transaction":m.transaction})).unwrap()).unwrap();
    assert!(pending(&root).unwrap());
    let rows = vec![Process {
        name: "NBA2K27.exe".into(),
        pid: 12,
        parent: 22,
        identity: Some(m.game.clone()),
    }];
    assert!(resolve_ended(&root, &rows).is_err());
    assert!(p.exists());
    resolve_ended(&root, &[]).unwrap();
    assert!(!p.exists());
    assert_eq!(fs::read(p.with_extension("ended")).unwrap(), raw);
    assert!(!pending(&root).unwrap());
    let resolved = root.canonicalize().unwrap();
    let temp = std::env::temp_dir().canonicalize().unwrap();
    assert!(
        resolved.starts_with(temp)
            && root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("C27-season-journal-test-")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn structural_directory_aliases_are_denied_before_any_effect() {
    let allocation = 0x100000u64;
    let owner = allocation + 0x700;
    let root = 0x900000;
    let ordinary = [
        (0x200000, 56usize),
        (0x200038, 192),
        (0x300000, 0x1c8),
        (0x400000, 0x68),
    ];
    directory_extents_disjoint(allocation, 0x800000, owner, root, &ordinary).unwrap();
    directory_extents_disjoint(allocation, 0x800000, allocation, root, &ordinary).unwrap();
    for (at, n) in [
        (allocation + 0x678, 16),
        (owner + 0x180, 8),
        (owner + 0x3c8, 8),
        (root + 0xa8, 8),
        (0x300008, 64),
        (0x400008, 64),
    ] {
        let mut spans = ordinary.to_vec();
        spans[0] = (at, n);
        assert!(
            directory_extents_disjoint(allocation, 0x800000, owner, root, &spans).is_err(),
            "alias {at:x}"
        );
    }
    let mut spans = ordinary.to_vec();
    spans[0] = (u64::MAX - 8, 16);
    assert!(directory_extents_disjoint(allocation, 0x800000, owner, root, &spans).is_err());
    assert!(directory_extents_disjoint(u64::MAX - 8, 16, owner, root, &ordinary).is_err());
    assert!(
        directory_extents_disjoint(allocation, 0x800000, owner, u64::MAX - 8, &ordinary).is_err()
    );
    let boundary = [
        (owner + OWNER_BYTES as u64, 8),
        (0x200000, 56),
        (0x200038, 64),
    ];
    directory_extents_disjoint(allocation, 0x800000, owner, root, &boundary).unwrap();
    let mut partial = boundary;
    partial[0].0 -= 1;
    assert!(directory_extents_disjoint(allocation, 0x800000, owner, root, &partial).is_err());
}
#[test]
fn frozen_directory_transfer_brackets_every_read_and_rejects_partial_faults() {
    use std::cell::Cell;
    let checks = Cell::new(0);
    let regions = Cell::new(0);
    let reads = Cell::new(0);
    let out = frozen_transfer(
        8192,
        || {
            checks.set(checks.get() + 1);
            Ok(())
        },
        || {
            regions.set(regions.get() + 1);
            Ok(())
        },
        |out| {
            reads.set(reads.get() + 1);
            out.fill(7);
            Ok(out.len())
        },
    )
    .unwrap();
    assert_eq!((checks.get(), regions.get(), reads.get()), (2, 2, 1));
    assert!(out.iter().all(|&v| v == 7));
    for size in [0, 16 * 1024 * 1024 + 1] {
        assert!(
            frozen_transfer(size, || Ok(()), || Ok(()), |_| panic!("out-of-bound read")).is_err()
        );
    }
    assert!(
        frozen_transfer(
            64,
            || anyhow::bail!("lost frozen holder"),
            || Ok(()),
            |_| panic!("read after failed ownership")
        )
        .is_err()
    );
    assert!(
        frozen_transfer(
            64,
            || Ok(()),
            || anyhow::bail!("foreign allocation"),
            |_| panic!("read after foreign allocation")
        )
        .is_err()
    );
    assert!(frozen_transfer(64, || Ok(()), || Ok(()), |_| Ok(63)).is_err());
    let count = Cell::new(0);
    assert!(
        frozen_transfer(
            64,
            || {
                count.set(count.get() + 1);
                ensure!(count.get() == 1, "holder lost after read");
                Ok(())
            },
            || Ok(()),
            |out| Ok(out.len())
        )
        .is_err()
    );
    let count = Cell::new(0);
    assert!(
        frozen_transfer(
            64,
            || Ok(()),
            || {
                count.set(count.get() + 1);
                ensure!(count.get() == 1, "allocation changed after read");
                Ok(())
            },
            |out| Ok(out.len())
        )
        .is_err()
    );
}
#[test]
fn frozen_full_directory_self_read_matches_chunked_path_and_counts_transfers() {
    // Read-only CURRENTSELF benchmark, never process discovery or game memory.
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    let source: Vec<u8> = (0..8192 * graph::PLAYER_BYTES)
        .map(|i| (i.wrapping_mul(17) & 255) as u8)
        .collect();
    let process = unsafe { GetCurrentProcess() };
    let calls = std::cell::Cell::new(0usize);
    let read = |at: u64, out: &mut [u8]| -> Result<usize> {
        let mut n = 0;
        unsafe {
            ReadProcessMemory(
                process,
                at as _,
                out.as_mut_ptr() as _,
                out.len(),
                Some(&mut n),
            )?;
        }
        calls.set(calls.get() + 1);
        Ok(n)
    };
    let start = Instant::now();
    let mut chunks = Vec::with_capacity(source.len());
    for part in source.chunks(4096) {
        let mut out = vec![0; part.len()];
        assert_eq!(read(part.as_ptr() as u64, &mut out).unwrap(), out.len());
        chunks.extend(out);
    }
    let chunk_ms = start.elapsed().as_secs_f64() * 1000.;
    let chunk_calls = calls.replace(0);
    let start = Instant::now();
    let batch = frozen_transfer(
        source.len(),
        || Ok(()),
        || Ok(()),
        |out| read(source.as_ptr() as u64, out),
    )
    .unwrap();
    let batch_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(chunks, source);
    assert_eq!(batch, source);
    assert_eq!(calls.get(), 1);
    assert_eq!(chunk_calls, source.len().div_ceil(4096));
    println!(
        "owned-self readonly {}B transfer calls {}→1 chunk_ms={chunk_ms:.3} batch_ms={batch_ms:.3}; excludes production proof costs",
        source.len(),
        chunk_calls
    );
}
#[test]
fn bulk_selector_binds_full_destination_and_exact_team_directory() {
    let (before, mut s, _) = fixture();
    s.action = "importSeasons".into();
    s.expected_slot = 0;
    s.expected_row_index = 0;
    s.expected_season_hash = history_digest(&before.graph);
    s.import = Some(BulkSelector {
        expected_teams_hash: digest(&before.teams),
        expected_snapshot_hash: snapshot_digest(&before).unwrap(),
        choices: vec![graph::bulk::Choice {
            year: 1999,
            primary_team_index: 0,
            secondary_team_index: None,
            changes: json!({"gp":1}),
        }],
    });
    let p = plan(&before, &s).unwrap();
    p.after().validate().unwrap();
    for drift in 0..5 {
        let mut altered = before.clone();
        match drift {
            0 => altered.graph.players[0][7] ^= 1,
            1 => altered.graph.rows[2][0] ^= 1,
            2 => altered.teams[3] ^= 1,
            3 => altered.owner[0x3c0] ^= 1,
            _ => altered.scope[0] ^= 1,
        };
        assert!(plan(&altered, &s).is_err(), "drift{drift}");
    }
    let mut forged = s.clone();
    forged.import.as_mut().unwrap().expected_teams_hash = "0".repeat(64);
    assert!(plan(&before, &forged).is_err());
    let mut forged = s.clone();
    forged.slot = Some(0);
    assert!(forged.operation().is_err());
}
#[test]
fn import_source_mapping_omissions_and_retained_review_are_strict() {
    let totals: serde_json::Map<String, Value> = crate::player_dna::schema()["seasonLimits"]
        .as_object()
        .unwrap()
        .keys()
        .map(|k| (k.clone(), json!(0)))
        .collect();
    let mut source = json!({"seasons":[{"year":2000,"team":"A","totals":totals}],"teamMappings":{},"preserveTotals":{"2000":["points"]},"normalTeamsOnly":true});
    let teams = json!([{"index":0,"name":"A","normal":true},{"index":1,"name":"B","normal":false}]);
    let c = import_choices(&source, &teams).unwrap();
    assert_eq!(c.len(), 1);
    assert!(!c[0].changes.as_object().unwrap().contains_key("points"));
    assert!(!c[0].changes.as_object().unwrap().contains_key("award:mvp"));
    source["seasons"][0]["team"] = json!("Absent");
    let e = import_choices(&source, &teams).unwrap_err();
    let m = e.downcast_ref::<RequiredMapping>().unwrap();
    assert_eq!(
        (m.year, m.role, m.name.as_str()),
        (2000, "primary", "Absent")
    );
    source["teamMappings"] = json!({"2000":{"primaryTeamIndex":1}});
    assert!(import_choices(&source, &teams).is_err());
    source["teamMappings"] = json!({"2000":{"primaryTeamIndex":0}});
    assert_eq!(
        import_choices(&source, &teams).unwrap()[0].primary_team_index,
        0
    );
    source["preserveTotals"]["2000"] = json!(["points", "points"]);
    assert!(import_choices(&source, &teams).is_err());
    let (_, selector, _) = fixture();
    let id = json!({"recordIndex":0,"firstName":"A"});
    let input = json!({"reviewId":"current","seasons":[],"teamMappings":{},"preserveTotals":{}});
    let review = Some(Review {
        id: "current".into(),
        selector,
        identity: id.clone(),
        selection: import_selection(&input),
        public: json!({"changed":true}),
    });
    retained_import(&review, &input, &id).unwrap();
    let mut stale = input.clone();
    stale["reviewId"] = json!("old");
    assert!(retained_import(&review, &stale, &id).is_err());
    let mut stale = input.clone();
    stale["teamMappings"] = json!({"2000":{"primaryTeamIndex":1}});
    assert!(retained_import(&review, &stale, &id).is_err());
    assert!(retained_import(&review, &input, &json!({"recordIndex":1})).is_err());
    assert!(retained_import(&None, &input, &id).is_err());
    // Actual old UI apply sends only reviewId + native player identity.
    retained_import(&review, &json!({"reviewId":"current"}), &id).unwrap();
}
#[test]
fn trusted_completion_reports_applied_and_refuses_restored_as_noop_success() {
    let receipt = json!({"transaction":"owned","status":"after","verified":false});
    let out = verified_completion(receipt).unwrap();
    assert_eq!(out["saved"], true);
    assert_eq!(out["verified"], true);
    assert_eq!(out["changed"], true);
    let error = verified_completion(json!({"status":"before"}))
        .unwrap_err()
        .to_string();
    assert!(error.contains("original history was restored"));
    assert!(verified_completion(json!({"saved":true})).is_err());
}
#[test]
fn new_scoped_review_revokes_old_token_even_when_new_pure_mapping_fails() {
    let (_, selector, _) = fixture();
    let identity = json!({"recordIndex":0});
    let original = Some(Review {
        id: "old".into(),
        selector,
        identity: identity.clone(),
        selection: json!({}),
        public: json!({}),
    });
    let mut cache = original.clone();
    assert!(revoke_import_review(&mut cache, Some(&json!("foreign")), &identity).is_err());
    assert!(cache.is_some());
    assert!(
        revoke_import_review(&mut cache, Some(&json!("old")), &json!({"recordIndex":1})).is_err()
    );
    assert!(cache.is_some());
    revoke_import_review(&mut cache, Some(&json!("old")), &identity).unwrap();
    assert!(cache.is_none());
    assert!(retained_import(&cache, &json!({"reviewId":"old"}), &identity).is_err());
    let mut cache = original;
    revoke_import_review(&mut cache, None, &identity).unwrap();
    assert!(cache.is_none());
    // A later pure mapping/planning failure never republishes the revoked token.
    assert!(import_choices(&json!({"seasons":[]}), &json!([])).is_err());
    assert!(cache.is_none());
}

#[test]
fn only_current_private_completion_archives_exact_intent_and_releases_capacity() {
    let (b, _, mut m) = fixture();
    let root = std::env::temp_dir().join(format!(
        "C27-season-archive-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let dir = directory(&root).unwrap();
    let forgotten = std::cell::Cell::new(0);
    for n in 0..140 {
        m.transaction = format!("{n:064x}");
        let path = dir.join(format!("{}.pending", m.transaction));
        let raw = encode(&m, &b).unwrap();
        let mut file = ordinary_new(&path).unwrap();
        file.write_all(&raw).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let receipt = json!({"schema":"nba2k27-season-structure-verified-v1","game":m.game,"session":m.session,"intentSha256":digest(&raw),"status":"after","transaction":m.transaction});
        fs::write(
            path.with_extension("verified"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        archive_current_verified_with(&root, &m.game, &m.session, &|_, _, _| false, &|_, _, _| {
            panic!("disk alone cannot forget")
        })
        .unwrap();
        assert!(path.exists());
        archive_current_verified_with(
            &root,
            &m.game,
            &m.session,
            &|g, s, h| g == &m.game && s == m.session && h == digest(&raw),
            &|_, _, _| {
                forgotten.set(forgotten.get() + 1);
                Ok(())
            },
        )
        .unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(path.with_extension("completed")).unwrap(), raw);
        assert!(!pending(&root).unwrap());
    }
    assert_eq!(forgotten.get(), 140);
    m.transaction = "f".repeat(64);
    let path = dir.join(format!("{}.pending", m.transaction));
    let raw = encode(&m, &b).unwrap();
    let mut f = ordinary_new(&path).unwrap();
    f.write_all(&raw).unwrap();
    f.sync_all().unwrap();
    drop(f);
    fs::write(path.with_extension("verified"), b"{}").unwrap();
    assert!(
        archive_current_verified_with(
            &root,
            &m.game,
            &m.session,
            &|_, _, _| true,
            &|_, _, _| panic!("invalid receipt")
        )
        .is_err()
    );
    assert!(path.exists());
    fs::remove_dir_all(&root).unwrap();
}
