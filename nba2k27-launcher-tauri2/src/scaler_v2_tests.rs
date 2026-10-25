use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
};

const BASE: u64 = 0x18000_0000;
const OBJECT: u64 = 0x22000_0000;

struct Fake {
    bytes: RefCell<BTreeMap<u64, u8>>,
    counts: RefCell<HashMap<u64, usize>>,
    mutation: RefCell<Option<(u64, usize, u64, Vec<u8>)>>,
    checks: Cell<usize>,
    stop_at: Cell<usize>,
    short_at: Cell<u64>,
}
impl Fake {
    fn new() -> Self {
        Self {
            bytes: RefCell::new(BTreeMap::new()),
            counts: RefCell::new(HashMap::new()),
            mutation: RefCell::new(None),
            checks: Cell::new(0),
            stop_at: Cell::new(usize::MAX),
            short_at: Cell::new(0),
        }
    }
    fn put(&self, at: u64, bytes: &[u8]) {
        for (index, byte) in bytes.iter().enumerate() {
            self.bytes.borrow_mut().insert(at + index as u64, *byte);
        }
    }
    fn q(&self, at: u64, value: u64) {
        self.put(at, &value.to_le_bytes());
    }
    fn d(&self, at: u64, value: u32) {
        self.put(at, &value.to_le_bytes());
    }
    fn fixture() -> Self {
        let m = Self::new();
        let mut header = vec![0; 4096];
        header[..2].copy_from_slice(b"MZ");
        header[60..64].copy_from_slice(&128u32.to_le_bytes());
        header[128..132].copy_from_slice(b"PE\0\0");
        header[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        header[136..140].copy_from_slice(&0x6ad2_e48cu32.to_le_bytes());
        header[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
        header[208..212].copy_from_slice(&0x3653_2000u32.to_le_bytes());
        m.put(BASE, &header);
        for &(rva, prefix, target) in RIP_ANCHORS {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(
                &((target as i64 - rva as i64 - prefix.len() as i64 - 4) as i32).to_le_bytes(),
            );
            m.put(BASE + rva, &bytes);
        }
        for &(rva, bytes) in FIELD_ANCHORS {
            m.put(BASE + rva, bytes);
        }
        for (rva, methods) in [
            (WRAPPER, [0x0292_3230, 0x0292_3290]),
            (MANAGER, [0x0081_da20, 0x0081_db40]),
            (MANAGER_INTERFACE, [0x0081_e1e0, 0x0081_db40]),
            (PLAYER, [0x0082_60b0, 0x0082_6160]),
            (PLAYER_INTERFACE, [0x0082_6cb0, 0x0082_6160]),
            (DESCRIPTION, [0x14f0, 0x0082_7040]),
            (DESCRIPTION_PRIVATE, [0x0082_7130, 0x0082_7190]),
        ] {
            for (index, method) in methods.into_iter().enumerate() {
                m.q(BASE + rva + index as u64 * 8, BASE + method);
            }
        }
        for (rva, method) in [
            (MANAGER + 0xc8, 0x0080_f260),
            (PLAYER_INTERFACE + 0x158, 0x0081_1790),
            (PLAYER_INTERFACE + 0x190, 0x006e_4630),
        ] {
            m.q(BASE + rva, BASE + method);
        }
        m.q(BASE + OWNER, OBJECT);
        m.d(OBJECT + 0xa04, 0);
        m.put(BASE + TABLE, &vec![0; TABLE_BYTES]);
        for index in 0..30 {
            m.q(BASE + TABLE + index * 0x50, BASE + WRAPPER);
        }
        m.d(BASE + INPUT, scaler_policy::ORIGINAL);
        m
    }
    fn player(&self, index: usize) -> (u64, u64, u64) {
        let row = BASE + TABLE + index as u64 * 0x50;
        let manager = OBJECT + 0x10000 + index as u64 * 0x10000;
        let roster = manager + 0x1000;
        let actor = manager + 0x2000;
        self.q(row + 0x10, manager);
        self.q(row + ROSTER_OFFSET as u64, roster);
        self.q(manager, BASE + MANAGER);
        self.q(manager + MANAGER_ACTOR, actor);
        self.q(roster + 0x518, manager);
        self.q(actor, BASE + PLAYER);
        self.q(actor + 0x78, BASE + PLAYER_INTERFACE);
        self.q(actor + 0xf30, BASE + DESCRIPTION);
        self.put(actor + 0xf30 + 0x7d, &[2]);
        self.d(actor + SCALE_PAIR, 1.0f32.to_bits());
        self.d(actor + SCALE_PAIR + 4, 1.0f32.to_bits());
        (manager, roster, actor)
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        let count = {
            let mut counts = self.counts.borrow_mut();
            let count = counts.entry(at).or_default();
            *count += 1;
            *count
        };
        let change = self.mutation.borrow().clone();
        if let Some((trigger, nth, write_at, bytes)) = change {
            if trigger == at && count == nth {
                self.put(write_at, &bytes);
            }
        }
        let bytes = self.bytes.borrow();
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let byte = bytes
                .get(&(at + i as u64))
                .context("Unmapped fake scaler read")?;
            out.push(*byte);
        }
        if self.short_at.get() == at {
            out.pop();
        }
        Ok(out)
    }
    fn checkpoint(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        ensure!(
            self.checks.get() < self.stop_at.get(),
            "Retained process ended"
        );
        Ok(())
    }
}

#[test]
fn fixed_four_byte_plan_matches_wpf_and_preserves_memory() {
    let m = Fake::fixture();
    let before = m.bytes.borrow().clone();
    let plan = plan_shared_input(&m, 95, None).unwrap().unwrap();
    assert_eq!(plan.address, BASE + INPUT);
    assert_eq!(plan.before, scaler_policy::ORIGINAL.to_le_bytes());
    assert_eq!(
        plan.after,
        scaler_policy::target_bits(95).unwrap().to_le_bytes()
    );
    assert!(!plan.reload_required);
    assert_eq!(plan.readiness.loaded_players(), 0);
    assert_eq!(*m.bytes.borrow(), before);
    revalidate(&m, &plan.readiness).unwrap();
}

#[test]
fn supported_modes_and_loaded_player_identity_are_recaptured() {
    for mode in [0, 4, 6] {
        let m = Fake::fixture();
        m.d(OBJECT + 0xa04, mode);
        m.player(0);
        m.player(29);
        let snapshot = snapshot_readiness(&m).unwrap();
        assert_eq!(snapshot.loaded_players(), 2);
        assert!(snapshot.loaded());
        revalidate(&m, &snapshot).unwrap();
        let (_, _, actor) = m.player(0);
        m.d(actor + SCALE_PAIR, 0.98f32.to_bits());
        m.d(actor + SCALE_PAIR + 4, (1.0f32 / 0.98).to_bits());
        // Normal native cache refresh does not change context identity.
        revalidate(&m, &snapshot).unwrap();
    }
    for mode in [1, 2, 3, 5, 7, u32::MAX] {
        let m = Fake::fixture();
        m.d(OBJECT + 0xa04, mode);
        assert!(snapshot_readiness(&m).is_err());
    }
}

#[test]
fn live_manager_interface_and_private_description_are_normalized_not_guessed() {
    for backreference_interface in [false, true] {
        let m = Fake::fixture();
        let (manager, roster, actor) = m.player(0);
        m.q(manager + 0x78, BASE + MANAGER_INTERFACE);
        m.q(BASE + TABLE + 0x10, manager + 0x78);
        m.q(
            roster + 0x518,
            manager + if backreference_interface { 0x78 } else { 0 },
        );
        m.q(actor + 0xf30, BASE + DESCRIPTION_PRIVATE);
        let before = m.bytes.borrow().clone();
        let state = snapshot_readiness(&m).unwrap();
        assert_eq!(state.players[0].manager, manager);
        assert_eq!(
            state.players[0].description_class,
            BASE + DESCRIPTION_PRIVATE
        );
        assert_eq!(*m.bytes.borrow(), before);
        revalidate(&m, &state).unwrap();
        m.q(manager, BASE + MANAGER + 8);
        assert!(
            snapshot_readiness(&m).is_err(),
            "Interface cannot bypass primary identity"
        );
    }
}

#[test]
fn private_descriptor_change_and_unknown_interface_refuse_plan() {
    let m = Fake::fixture();
    let (manager, _, actor) = m.player(0);
    let state = snapshot_readiness(&m).unwrap();
    m.q(actor + 0xf30, BASE + DESCRIPTION_PRIVATE);
    assert!(revalidate(&m, &state).is_err());
    m.q(actor + 0xf30, BASE + DESCRIPTION_PRIVATE + 8);
    assert!(snapshot_readiness(&m).is_err());
    m.q(actor + 0xf30, BASE + DESCRIPTION_PRIVATE);
    m.q(manager + 0x78, BASE + MANAGER_INTERFACE + 8);
    m.q(BASE + TABLE + 0x10, manager + 0x78);
    assert!(snapshot_readiness(&m).is_err());
}

#[test]
fn loaded_size_observation_is_read_only_and_not_a_false_stock_percentage() {
    let m = Fake::fixture();
    m.d(BASE + INPUT, scaler_policy::target_bits(97).unwrap());
    assert_eq!(observe(&m, 97).unwrap().players, 0);
    let (_, _, actor) = m.player(0);
    let raw = 0.93365544f32;
    m.d(actor + SCALE_PAIR, raw.to_bits());
    m.d(actor + SCALE_PAIR + 4, (1.0 / raw).to_bits());
    let before = m.bytes.borrow().clone();
    let result = observe(&m, 97).unwrap();
    assert_eq!(result.players, 1);
    assert_eq!(result.minimum_cache, Some(raw));
    assert_eq!(*m.bytes.borrow(), before);
    assert!(observe(&m, 96).is_err());
    m.d(actor + SCALE_PAIR + 4, 0f32.to_bits());
    assert!(observe(&m, 97).is_err());
}

#[test]
fn full_game_modes_are_observable_but_never_plan_writes() {
    let m = Fake::fixture();
    m.d(BASE + INPUT, scaler_policy::target_bits(97).unwrap());
    let (_, _, actor) = m.player(0);
    let raw = 0.93365544f32;
    m.d(actor + SCALE_PAIR, raw.to_bits());
    m.d(actor + SCALE_PAIR + 4, (1.0 / raw).to_bits());
    // A Quick Play NBA game reports a mode outside Main/Freestyle/Blacktop.
    m.d(OBJECT + 0xa04, 1);
    assert_eq!(observe(&m, 97).unwrap().players, 1);
    assert!(snapshot_readiness(&m).is_err());
    assert!(plan_shared_input(&m, 96, None).is_err());
}

#[test]
fn old_manager_and_roster_layouts_do_not_pass_as_version_two() {
    let m = Fake::fixture();
    let (manager, roster, actor) = m.player(0);
    m.q(manager + MANAGER_ACTOR, 0);
    m.q(manager + 0x98, actor);
    assert!(snapshot_readiness(&m).is_err());
    m.q(manager + MANAGER_ACTOR, actor);
    m.q(BASE + TABLE + ROSTER_OFFSET as u64, 0);
    m.q(BASE + TABLE + 0x30, roster);
    assert!(snapshot_readiness(&m).is_err());
    m.q(BASE + TABLE + ROSTER_OFFSET as u64, roster);
    m.q(actor + 0xf30, 0);
    m.q(actor + 0xf20, BASE + DESCRIPTION);
    assert!(snapshot_readiness(&m).is_err());
}

#[test]
fn malformed_players_duplicates_and_cache_values_fail_closed() {
    for fault in 0..9 {
        let m = Fake::fixture();
        let (manager, roster, actor) = m.player(0);
        match fault {
            0 => m.q(BASE + TABLE, BASE + WRAPPER + 8),
            1 => m.q(manager, BASE + MANAGER + 8),
            2 => m.q(roster + 0x518, manager + 8),
            3 => m.q(actor, BASE + PLAYER + 8),
            4 => m.q(actor + 0x78, BASE + PLAYER_INTERFACE + 8),
            5 => m.put(actor + 0xf30 + 0x7d, &[0]),
            6 => m.d(actor + SCALE_PAIR, f32::NAN.to_bits()),
            7 => m.d(actor + SCALE_PAIR + 4, 2.0f32.to_bits()),
            8 => {
                m.q(BASE + TABLE + 0x50 + 0x10, manager);
                m.q(BASE + TABLE + 0x50 + ROSTER_OFFSET as u64, roster);
            }
            _ => unreachable!(),
        }
        assert!(snapshot_readiness(&m).is_err(), "Fault {fault} accepted");
    }
}

#[test]
fn every_header_code_class_and_target_guard_is_required() {
    let m = Fake::fixture();
    verify_profile(&m).unwrap();
    for &(rva, prefix, _) in RIP_ANCHORS {
        for offset in [0, prefix.len()] {
            let m = Fake::fixture();
            let original = m.read(BASE + rva + offset as u64, 1).unwrap()[0];
            m.put(BASE + rva + offset as u64, &[original ^ 1]);
            assert!(verify_profile(&m).is_err());
        }
    }
    for &(rva, _) in FIELD_ANCHORS {
        let m = Fake::fixture();
        let original = m.read(BASE + rva, 1).unwrap()[0];
        m.put(BASE + rva, &[original ^ 1]);
        assert!(verify_profile(&m).is_err());
    }
    for rva in [
        WRAPPER,
        MANAGER,
        MANAGER_INTERFACE,
        PLAYER,
        PLAYER_INTERFACE,
        DESCRIPTION,
        DESCRIPTION_PRIVATE,
        MANAGER + 0xc8,
        PLAYER_INTERFACE + 0x158,
        PLAYER_INTERFACE + 0x190,
    ] {
        let m = Fake::fixture();
        m.q(BASE + rva, BASE + 0x1234);
        assert!(verify_profile(&m).is_err());
    }
    for offset in [0, 60, 132, 136, 152, 209] {
        let m = Fake::fixture();
        m.put(BASE + offset, &[0]);
        assert!(verify_profile(&m).is_err());
    }
}

#[test]
fn transition_short_read_and_process_exit_never_create_a_plan() {
    let m = Fake::fixture();
    *m.mutation.borrow_mut() = Some((
        BASE + OWNER,
        2,
        BASE + OWNER,
        (OBJECT + 0x10000).to_le_bytes().to_vec(),
    ));
    assert!(plan_shared_input(&m, 95, None).is_err());
    for address in [BASE, BASE + TABLE, BASE + INPUT, BASE + OWNER] {
        let m = Fake::fixture();
        m.short_at.set(address);
        assert!(plan_shared_input(&m, 95, None).is_err());
    }
    for checkpoint in 1..=4 {
        let m = Fake::fixture();
        m.stop_at.set(checkpoint);
        assert!(plan_shared_input(&m, 95, None).is_err());
    }
}

#[test]
fn prepared_context_is_not_reused_across_mode_or_player_change() {
    for mutate in 0..3 {
        let m = Fake::fixture();
        let (manager, _, _) = m.player(0);
        let plan = plan_shared_input(&m, 96, None).unwrap().unwrap();
        match mutate {
            0 => m.d(OBJECT + 0xa04, 4),
            1 => m.q(BASE + TABLE + 0x10, 0),
            2 => m.q(manager + MANAGER_ACTOR, OBJECT + 0x40000),
            _ => unreachable!(),
        }
        assert!(revalidate(&m, &plan.readiness).is_err());
    }
}

#[test]
fn restore_requires_this_sessions_exact_ownership() {
    let m = Fake::fixture();
    let low = scaler_policy::target_bits(95).unwrap();
    m.d(BASE + INPUT, low);
    assert!(plan_shared_input(&m, 95, None).is_err());
    assert!(plan_restore(&m, None).unwrap().is_none());
    let owned = Some(Ownership {
        applied: low,
        requested: low,
    });
    let plan = plan_restore(&m, owned).unwrap().unwrap();
    assert_eq!(plan.before, low.to_le_bytes());
    assert_eq!(plan.after, scaler_policy::ORIGINAL.to_le_bytes());
    m.d(BASE + INPUT, scaler_policy::target_bits(96).unwrap());
    assert!(plan_restore(&m, owned).is_err());
}

/// Explicit opt-in disk audit. It does not enumerate/open a game process and
/// only reads the selected exact SHA-pinned executable and PE sections.
#[test]
#[ignore]
fn actual_installed_version_two_scaler_profile_is_read_only() {
    use sha2::{Digest, Sha256};
    use std::{
        fs::File,
        io::{Read, Seek, SeekFrom},
        path::PathBuf,
    };
    let path = PathBuf::from(
        std::env::var_os("C27_SCALER_GAME_EXE").expect("Explicit disk image required"),
    );
    let mut stream = File::open(path).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let n = stream.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    assert_eq!(
        format!("{:X}", digest.finalize()),
        "B673EB04593D6D5A4DFB9E270178E04C017BD9F277514D256EA98FFB792ED27B"
    );
    stream.seek(SeekFrom::Start(0)).unwrap();
    let mut h = vec![0; 4096];
    stream.read_exact(&mut h).unwrap();
    let pe = u32::from_le_bytes(h[60..64].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(h[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(h[pe + 20..pe + 22].try_into().unwrap()) as usize;
    assert!((1..=32).contains(&count));
    let preferred = u64::from_le_bytes(h[pe + 48..pe + 56].try_into().unwrap());
    struct Disk {
        file: RefCell<File>,
        header: Vec<u8>,
        sections: Vec<(u64, u64, u64)>,
        base: u64,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            let rva = at
                .checked_sub(self.base)
                .context("Disk address below image")?;
            if rva == 0 && len == self.header.len() {
                return Ok(self.header.clone());
            }
            let (start, _, offset) = self
                .sections
                .iter()
                .find(|(start, raw, _)| {
                    rva >= *start
                        && rva
                            .checked_add(len as u64)
                            .is_some_and(|end| end <= start + raw)
                })
                .context("Disk span not raw-backed")?;
            let mut out = vec![0; len];
            let mut file = self.file.borrow_mut();
            file.seek(SeekFrom::Start(offset + rva - start))?;
            file.read_exact(&mut out)?;
            Ok(out)
        }
    }
    let mut sections = Vec::new();
    for i in 0..count {
        let at = pe + 24 + optional + i * 40;
        sections.push((
            u32::from_le_bytes(h[at + 12..at + 16].try_into().unwrap()) as u64,
            u32::from_le_bytes(h[at + 16..at + 20].try_into().unwrap()) as u64,
            u32::from_le_bytes(h[at + 20..at + 24].try_into().unwrap()) as u64,
        ));
    }
    let disk = Disk {
        file: RefCell::new(stream),
        header: h,
        sections,
        base: preferred,
    };
    verify_profile(&disk).unwrap();
    println!(
        "{}",
        serde_json::json!({"pass":true,"profile":"version-two-scaler-shared-input","codeAnchors":RIP_ANCHORS.len()+FIELD_ANCHORS.len(),"input":"0A41D07C","managerActor":"110","wrapperRoster":"28","description":"F30","scalePair":"BCC/BD0","npcWritesEnabled":false,"gameProcessAccess":false,"gameFilesChanged":false})
    );
}
