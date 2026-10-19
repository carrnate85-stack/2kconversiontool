use super::*;
fn utf(row: &mut [u8], at: usize, value: &str) {
    for (i, c) in value.encode_utf16().enumerate() {
        row[at + i * 2..at + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
}
fn fixture() -> (Immutable, clone::Plan, Vec<u8>, Vec<u64>) {
    let base = 0x20000000u64;
    let table = base + 0x10000;
    let arena_table = base + 0x40000;
    let mut team = vec![0; profile::TEAM_STRIDE];
    team[writer::TEAM_ID..writer::TEAM_ID + 2].copy_from_slice(&7u16.to_le_bytes());
    utf(&mut team, 0x2fa, "Fixture");
    team[clone::LINK..clone::LINK + 8].copy_from_slice(&arena_table.to_le_bytes());
    let mut rows = vec![0; clone::STRIDE * 3];
    rows[clone::ID..clone::ID + 2].copy_from_slice(&17u16.to_le_bytes());
    utf(&mut rows, 0, "Donor");
    rows[0x7e..0x7e + 9].copy_from_slice(b"arena_017");
    rows[0xa4..0xa4 + 9].copy_from_slice(b"floor_017");
    rows[0x300] = 37;
    rows[clone::STRIDE + clone::ID..clone::STRIDE + clone::ID + 2]
        .copy_from_slice(&999u16.to_le_bytes());
    rows[2 * clone::STRIDE + clone::ID..2 * clone::STRIDE + clone::ID + 2]
        .copy_from_slice(&1000u16.to_le_bytes());
    let links = vec![arena_table; 30];
    let catalog = catalog_hash_parts(
        [base + 0x50, base + 0x100, base, 0x800000, arena_table, 3],
        &rows,
    );
    let o = clone::Owner {
        team: writer::Owner {
            game: Identity {
                pid: 7,
                started: "42".into(),
                path: r"C:\QA\NBA2K27.exe".into(),
            },
            session: "a".repeat(48),
            image: 0x140000000,
            root: base + 0x50,
            owner: base + 0x100,
            region_base: base,
            region_len: 0x800000,
            kind: Kind::Team,
            table,
            count: 30,
            index: 0,
            identity: writer::identity(Kind::Team, &team).unwrap(),
            proof: writer::proof_tag().into(),
        },
        arena_table,
        arena_count: 3,
        donor: 0,
        target: 1,
        target_id: 999,
        donor_hash: clone::hash(&rows[..clone::STRIDE]),
        ids_hash: clone::ids_hash(&rows).unwrap(),
        other_links_hash: clone::links_hash(&links, 0),
        catalog_id: catalog.clone(),
        protected_header_hash: "e".repeat(64),
    };
    let edit = clone::Edit {
        record_index: 0,
        team_id: 7,
        expected_team_name: "Fixture".into(),
        donor_arena_id: 17,
        target_arena_id: 999,
        arena_name: "New Court".into(),
        arena_file: "arena_custom".into(),
        floor_file: "floor_custom".into(),
        catalog_id: catalog,
    };
    let p = clone::plan(o, &edit, &team, &rows, &links).unwrap();
    let i = Immutable {
        operation_hash: "a".repeat(64),
        mode: clone::Mode::CloneAndLink,
        prepared_intent: clone::prepare_intent(&p).unwrap(),
        schema: "nba2k27-v2-arena-clone-immutable-v1".into(),
        session: p.owner.team.session.clone(),
        game: p.owner.team.game.clone(),
        edit,
        expected_team_hash: clone::hash(&team),
        mods_root: PathBuf::from(r"C:\QA\Mods"),
        mods: Vec::new(),
        owner: p.owner.clone(),
        before: p.before.clone(),
        after: p.after.clone(),
    };
    (i, p, rows, links)
}
fn root() -> PathBuf {
    let p = std::env::current_dir()
        .unwrap()
        .join("artifacts")
        .join(format!(
            "qa084-arena-access-self-{}",
            crate::backend::unique_id()
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn native_effect_admission_is_exact_closed_plan_id_excluding_and_phase_bound() {
    let (_, p, _, _) = fixture();
    for (index, patch) in p.patches.iter().enumerate() {
        let (at, phase) = effect(&p, patch.area, patch.offset, &patch.after, false).unwrap();
        assert_eq!(phase, index);
        assert_eq!(
            effect(&p, patch.area, patch.offset, &patch.before, true).unwrap(),
            (at, index + 1)
        );
        assert!(
            at >= p.owner.team.region_base
                && at + patch.after.len() as u64
                    <= p.owner.team.region_base + p.owner.team.region_len
        );
        let mut forged = patch.after.clone();
        forged[0] ^= 1;
        assert!(effect(&p, patch.area, patch.offset, &forged, false).is_err());
        assert!(effect(&p, patch.area, patch.offset + 1, &patch.after, false).is_err());
    }
    for (at, len) in [
        (clone::ID, 2),
        (clone::ID - 1, 2),
        (clone::STRIDE, 1),
        (usize::MAX, 2),
    ] {
        assert!(effect(&p, clone::Area::Arena, at, &vec![1; len], false).is_err());
    }
    assert!(effect(&p, clone::Area::Team, clone::LINK - 8, &[0; 8], false).is_err());
    assert!(effect(&p, clone::Area::Arena, 0, &[0; 129], false).is_err());
}
#[test]
fn independent_reconstruction_accepts_whole_before_after_but_rejects_foreign_catalog_link_and_forged_images()
 {
    let (i, p, rows, links) = fixture();
    assert_eq!(
        reconstruct(&i, rows.clone(), links.clone()).unwrap().after,
        p.after
    );
    let mut after = rows.clone();
    after[clone::STRIDE..clone::STRIDE * 2].copy_from_slice(&p.after.arena);
    let mut afterlinks = links.clone();
    afterlinks[0] = p.owner.target_address();
    assert_eq!(
        reconstruct(&i, after.clone(), afterlinks.clone())
            .unwrap()
            .before,
        p.before
    );
    for at in [0, clone::STRIDE + 3, 2 * clone::STRIDE + 0x301] {
        let mut bad = after.clone();
        bad[at] ^= 1;
        assert!(reconstruct(&i, bad, afterlinks.clone()).is_err(), "{at:x}");
    }
    let mut badlinks = afterlinks.clone();
    badlinks[1] = p.owner.target_address();
    assert!(reconstruct(&i, after.clone(), badlinks).is_err());
    let mut forged: Immutable = serde_json::from_value(serde_json::to_value(&i).unwrap()).unwrap();
    forged.after.arena[0x300] ^= 1;
    assert!(reconstruct(&forged, rows.clone(), links.clone()).is_err());
    let mut forged: Immutable = serde_json::from_value(serde_json::to_value(&i).unwrap()).unwrap();
    forged.expected_team_hash = "a".repeat(64);
    assert!(reconstruct(&forged, rows, links).is_err());
}
#[test]
fn selector_cannot_gain_addresses_or_unknown_fields_and_requires_native_absolute_mods_root() {
    let (i, _, _, _) = fixture();
    let base =
        json!({"edit":i.edit,"expectedTeamHash":i.expected_team_hash,"modsRoot":"C:\\QA\\Mods"});
    assert!(Selector::parse(&Operation::ArenaClone(base.clone())).is_ok());
    for (key, value) in [
        ("address", json!(0x20000000u64)),
        ("mode", json!("populateOnly")),
    ] {
        let mut v = base.clone();
        v[key] = value;
        assert!(Selector::parse(&Operation::ArenaClone(v)).is_err());
    }
    let mut v = base.clone();
    v["modsRoot"] = json!("relative");
    assert!(Selector::parse(&Operation::ArenaClone(v)).is_err());
    let mut v = base;
    v["edit"]["arenaFile"] = json!("../foreign");
    assert!(Selector::parse(&Operation::ArenaClone(v)).is_err());
}
#[test]
fn pinned_mod_and_immutable_intent_are_retained_ordinary_readonly_exact_files() {
    let r = root();
    fs::create_dir(r.join("levels")).unwrap();
    fs::write(r.join("levels/arena_custom.iff"), b"synthetic IFF fixture").unwrap();
    fs::write(r.join("root_only.iff"), b"wrong location").unwrap();
    assert!(PinnedMod::open(&r, "root_only").is_err());
    let m = PinnedMod::open(&r, "arena_custom").unwrap();
    m.check().unwrap();
    assert_eq!(m.proof.len, 21);
    assert_eq!(m.proof.sha256, clone::hash(b"synthetic IFF fixture"));
    assert!(
        OpenOptions::new()
            .write(true)
            .open(r.join("levels/arena_custom.iff"))
            .is_err()
    );
    assert!(fs::rename(r.join("levels/arena_custom.iff"), r.join("other.iff")).is_err());
    assert!(PinnedMod::open(&r, "../foreign").is_err());
    fs::write(r.join("levels/empty.iff"), b"").unwrap();
    assert!(PinnedMod::open(&r, "empty").is_err());
    let (i, _, _, _) = fixture();
    let (mut f, _pins) = immutable(&r, &i).unwrap();
    let mut b = Vec::new();
    f.read_to_end(&mut b).unwrap();
    assert_eq!(b, serde_json::to_vec(&i).unwrap());
    let dir = r.join("Data/Owned Arena Transactions");
    let path = fs::read_dir(dir).unwrap().next().unwrap().unwrap().path();
    assert!(OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::remove_file(&path).is_err());
}

#[test]
fn saved_profile_reconstruction_uses_populate_only_and_never_admits_a_team_effect() {
    let (mut i, p, rows, links) = fixture();
    let profile =
        clone::plan_profile(p.owner.clone(), &i.edit, &p.before.team, &rows, &links).unwrap();
    i.mode = clone::Mode::PopulateOnly;
    i.prepared_intent = clone::prepare_intent(&profile).unwrap();
    i.before = profile.before.clone();
    i.after = profile.after.clone();
    assert_eq!(
        reconstruct(&i, rows.clone(), links.clone()).unwrap().mode,
        clone::Mode::PopulateOnly
    );
    let mut after = rows.clone();
    after[clone::STRIDE..2 * clone::STRIDE].copy_from_slice(&profile.after.arena);
    assert!(reconstruct(&i, after.clone(), links.clone()).is_ok());
    let team_patch = p.patches.last().unwrap().clone();
    let mut forged = profile.clone();
    forged.patches.push(team_patch.clone());
    assert!(
        effect(
            &forged,
            clone::Area::Team,
            team_patch.offset,
            &team_patch.after,
            false
        )
        .is_err()
    );
    i.mode = clone::Mode::CloneAndLink;
    assert!(reconstruct(&i, after, links).is_err());
}
#[test]
fn automatic_profile_retry_key_is_stable_entire_launch_and_excludes_own_row_bytes() {
    let (i, _, _, _) = fixture();
    let game = i.game.clone();
    let parent = Identity {
        pid: 8,
        started: "43".into(),
        path: r"C:\QA\Launcher.exe".into(),
    };
    let binding = Binding::from_launched(
        game.clone(),
        parent.clone(),
        PathBuf::from(r"C:\QA"),
        "a".repeat(48),
    )
    .unwrap();
    let mut profile = crate::custom_arena_profiles::Profile::from_verified(&i.edit);
    profile.enabled = true;
    let key = profile_key(&binding, &profile).unwrap();
    profile.arena_name = "Changed local desired value".into();
    profile.target_arena_id = 1000;
    assert_eq!(profile_key(&binding, &profile).unwrap(), key);
    let mut changed = binding.clone();
    changed.session = "b".repeat(48);
    assert_ne!(profile_key(&changed, &profile).unwrap(), key);
    let mut changed = binding.clone();
    changed.game.started = "44".into();
    assert_ne!(profile_key(&changed, &profile).unwrap(), key);
    profile.id = "f".repeat(48);
    assert_ne!(profile_key(&binding, &profile).unwrap(), key);
}

fn protected_fixture() -> (u64, u64, u64, Vec<u8>) {
    let base = 0x3000_0000u64;
    let owner = base + 0x800;
    let root = base + 0x100;
    let mut h = vec![0; OWNER_HEADER];
    // Space every pool independently, and exercise opaque upper DWORDs.
    for (i, (off, count)) in [
        (0x10, 100u64),
        (0x60, 3),
        (0x80, 30),
        (0x290, 2),
        (0x180, 2),
        (0x3c0, 2),
        (0x1c0, 1),
        (0x1d0, 2),
        (0x90, 2),
        (0x240, 2),
        (0x250, 2),
    ]
    .into_iter()
    .enumerate()
    {
        h[off..off + 8].copy_from_slice(&count.to_le_bytes());
        if ![0x10, 0x60, 0x80, 0x290].contains(&off) {
            h[off + 4..off + 8].copy_from_slice(&0xfedc_ba98u32.to_le_bytes());
        }
        h[off + 8..off + 16].copy_from_slice(&(base + 0x10000 + i as u64 * 0x100000).to_le_bytes());
    }
    (base, owner, root, h)
}
fn protected_geometry(base: u64, owner: u64, root: u64, h: &[u8]) -> Result<()> {
    super::super::structural::directory_extents_disjoint(
        base,
        0x2000000,
        owner,
        root,
        &protected_extents(h)?,
    )
}
#[test]
fn protected_pool_extents_preserve_dword_opacity_and_optional_absence() {
    let (b, o, r, h) = protected_fixture();
    let ext = protected_extents(&h).unwrap();
    assert_eq!(ext.len(), 11);
    assert_eq!(ext[0].1, 100 * profile::PLAYER_STRIDE);
    assert_eq!(ext[4].1, 2 * 0x1c8);
    assert_eq!(ext[5].1, 2 * 0x68);
    assert_eq!(ext[6].1, 56);
    protected_geometry(b, o, r, &h).unwrap();
    let mut absent = h.clone();
    for at in [0x290, 0x90, 0x240, 0x250] {
        absent[at..at + 16].fill(0);
    }
    assert_eq!(protected_extents(&absent).unwrap().len(), 7);
    protected_geometry(b, o, r, &absent).unwrap();
    for at in [
        0x10, 0x60, 0x80, 0x290, 0x180, 0x3c0, 0x1c0, 0x1d0, 0x90, 0x240, 0x250,
    ] {
        let mut bad = h.clone();
        bad[at + 8..at + 16].copy_from_slice(&1u64.to_le_bytes());
        assert!(protected_extents(&bad).is_err(), "alignment {at:x}");
        let mut bad = h.clone();
        bad[at..at + 4].fill(0);
        assert!(protected_extents(&bad).is_err(), "half empty {at:x}");
    }
    for (at, n) in [(0x180, 8193u32), (0x3c0, 65537), (0x1c0, 2), (0x1d0, 32768)] {
        let mut bad = h.clone();
        bad[at..at + 4].copy_from_slice(&n.to_le_bytes());
        assert!(protected_extents(&bad).is_err());
    }
}
#[test]
fn protected_pool_geometry_rejects_partial_aliases_and_all_metadata() {
    let (b, o, r, h) = protected_fixture();
    let ext = protected_extents(&h).unwrap();
    for selected in [0x60, 0x80] {
        // Arena/Team rows cannot overlap any other known owned pool, even by 8 bytes.
        for (i, &(at, n)) in ext.iter().enumerate() {
            if i == if selected == 0x60 { 1 } else { 2 } {
                continue;
            }
            let mut bad = h.clone();
            bad[selected + 8..selected + 16]
                .copy_from_slice(&(at + ((n - 1) / 8 * 8) as u64).to_le_bytes());
            assert!(
                protected_geometry(b, o, r, &bad).is_err(),
                "pool {selected:x}/{i}"
            );
        }
        for at in [b + 0x678, o + 0x2a0, o + 0x3c8, r + 0xa8] {
            let mut bad = h.clone();
            bad[selected + 8..selected + 16].copy_from_slice(&at.to_le_bytes());
            assert!(
                protected_geometry(b, o, r, &bad).is_err(),
                "metadata {selected:x}/{at:x}"
            );
        }
        let mut bad = h.clone();
        bad[selected + 8..selected + 16].copy_from_slice(&(u64::MAX - 7).to_le_bytes());
        assert!(protected_geometry(b, o, r, &bad).is_err());
    }
    // Exact adjacency between the manager and stat row zero is valid.
    let mut adjacent = h.clone();
    let manager = ext[6].0;
    adjacent[0x1d8..0x1e0].copy_from_slice(&(manager + 56).to_le_bytes());
    protected_geometry(b, o, r, &adjacent).unwrap();
}
#[test]
fn complete_protected_header_witness_rejects_late_auxiliary_and_opaque_changes() {
    let (_, _, _, h) = protected_fixture();
    let hash = clone::hash(&h);
    verify_protected_header(&h, &hash).unwrap();
    for at in [
        0x184, 0x188, 0x1c0, 0x1c8, 0x1d8, 0x248, 0x298, 0x3c0, 0x3c8, 0x3cf,
    ] {
        let mut late = h.clone();
        late[at] ^= 1;
        assert!(
            verify_protected_header(&late, &hash).is_err(),
            "late {at:x}"
        );
    }
    assert!(verify_protected_header(&h[..OWNER_HEADER - 1], &hash).is_err());
}
