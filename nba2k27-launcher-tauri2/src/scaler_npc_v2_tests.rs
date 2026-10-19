use super::*;
use std::cell::RefCell;

const BASE: u64 = 0x1_8000_0000;
const HEAP: u64 = 0x2_2000_0000;
fn input97() -> u32 {
    scaler_policy::target_bits(97).unwrap()
}

struct Fake {
    bytes: RefCell<BTreeMap<u64, u8>>,
}
impl Fake {
    fn new() -> Self {
        Self {
            bytes: RefCell::new(BTreeMap::new()),
        }
    }
    fn put(&self, at: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.borrow_mut().insert(at + i as u64, *v);
        }
    }
    fn q(&self, at: u64, v: u64) {
        self.put(at, &v.to_le_bytes());
    }
    fn pair(&self, actor: u64, scale: f32) {
        self.put(
            actor + scaler_v2::SCALE_PAIR,
            &scale.to_bits().to_le_bytes(),
        );
        self.put(
            actor + scaler_v2::SCALE_PAIR + 4,
            &(1.0f32 / scale).to_bits().to_le_bytes(),
        );
    }
    /// One list node with an actor of `class` (None = an unrelated class, e.g. a prop).
    fn character(&self, node: u64, class: Option<&Class>, flag: u8, scale: f32) -> u64 {
        let actor = node + 0x10_0000;
        self.q(node, BASE + scaler_v2::MANAGER);
        self.q(node + NODE_ACTOR, actor);
        self.put(node + NODE_STATE, &[0]);
        match class {
            Some(c) => {
                self.q(actor, BASE + c.actor);
                self.q(actor + 0x78, BASE + c.interface);
                self.q(actor + c.description_offset, BASE + c.description);
                self.put(actor + c.description_offset + OPT_IN, &[flag]);
            }
            None => self.q(actor, BASE + 0x1234),
        }
        self.pair(actor, scale);
        actor
    }
    /// The game's rostered-player wrapper table (30 rows of 0x50).
    fn roster(&self, managers: &[u64]) {
        self.put(BASE + scaler_v2::TABLE, &vec![0; scaler_v2::TABLE_BYTES]);
        for (row, &manager) in managers.iter().enumerate() {
            self.q(BASE + scaler_v2::TABLE + row as u64 * 0x50 + 0x10, manager);
        }
    }
    fn link(&self, nodes: &[u64]) {
        let sentinel = BASE + LIST_SENTINEL;
        let mut previous = sentinel;
        for &n in nodes {
            self.q(n + 8, previous);
            self.q(previous + 0x10, n);
            previous = n;
        }
        self.q(previous + 0x10, sentinel);
        self.q(sentinel + 8, previous);
    }
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        BASE
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        (0..len as u64)
            .map(|i| {
                self.bytes
                    .borrow()
                    .get(&(at + i))
                    .copied()
                    .context("unmapped fixture read")
            })
            .collect()
    }
}
fn arena() -> Fake {
    let m = Fake::new();
    let nodes = [
        HEAP,
        HEAP + 0x1000,
        HEAP + 0x2000,
        HEAP + 0x3000,
        HEAP + 0x4000,
        HEAP + 0x5000,
    ];
    m.character(nodes[0], Some(&CLASSES[0]), 0x01, 0.982); // coach
    m.character(nodes[1], Some(&CLASSES[1]), 0x01, 1.0); // cheerleader
    m.character(nodes[2], Some(&CLASSES[2]), 0x01, 0.98819); // arena personnel
    m.character(nodes[3], Some(&CLASSES[2]), 0x03, 0.93937); // referee: native bit
    // Rostered player that has not set its native bit yet: never adopted.
    m.character(nodes[4], Some(&CLASSES[3]), 0x00, 0.96);
    m.character(nodes[5], None, 0x01, 1.0); // prop: unrelated class
    m.roster(&[nodes[4]]);
    m.link(&nodes);
    m
}
const ALL: Groups = Groups {
    coaches: true,
    cheerleaders: true,
    arena_personnel: true,
};

#[test]
fn scan_finds_only_the_three_groups_with_exact_object_chains() {
    let m = arena();
    let found = scan(&m).unwrap();
    let groups: Vec<_> = found.iter().map(|c| (c.group, c.flag)).collect();
    assert_eq!(
        groups,
        [
            (Group::Coach, 1),
            (Group::Cheerleader, 1),
            (Group::Arena, 1),
            (Group::Arena, 3)
        ]
    );
    assert_eq!(found[1].description, found[1].actor + 0xED0);
    assert_eq!(found[0].description, found[0].actor + 0xF30);
}

#[test]
fn scan_rejects_torn_lists_foreign_nodes_and_unstable_sizes_but_skips_busy_managers() {
    let m = arena();
    m.q(HEAP + 0x2000 + 8, HEAP); // broken back-link
    assert!(scan(&m).is_err());
    let m = arena();
    m.q(HEAP + 0x1000, BASE + 0x1234); // not a manager
    assert!(scan(&m).is_err());
    let m = arena();
    m.put(HEAP + 0x10_0000 + scaler_v2::SCALE_PAIR, &[0; 8]); // coach size loading
    assert!(scan(&m).is_err());
    let m = arena();
    m.q(HEAP + 0x2000 + 0x10_0000 + 0xF30, BASE + 0x9999); // wrong description class
    assert!(scan(&m).is_err());
    let m = arena();
    m.put(HEAP + NODE_STATE, &[1]); // busy coach manager is skipped, not adopted
    assert_eq!(scan(&m).unwrap().len(), 3);
}

#[test]
fn plan_opts_in_selected_groups_seeds_arena_once_and_never_touches_native_or_unselected() {
    let m = arena();
    let found = scan(&m).unwrap();
    let mut owned = BTreeMap::new();
    let input = input97();
    let p = plan(&found, ALL, input, &mut owned);
    assert_eq!(p.conflicts, 0);
    // coach: flag only; cheerleader + arena: flag + seed; referee untouched.
    assert_eq!(p.writes.len(), 5);
    assert!(p.writes.iter().all(|w| w.actor != found[3].actor));
    let coach: Vec<_> = p
        .writes
        .iter()
        .filter(|w| w.actor == found[0].actor)
        .collect();
    assert_eq!(coach.len(), 1);
    assert_eq!(
        (coach[0].before.clone(), coach[0].after.clone()),
        (vec![1], vec![3])
    );
    let seed = p
        .writes
        .iter()
        .find(|w| w.actor == found[2].actor && w.after.len() == 8)
        .unwrap();
    let scale = f32::from_bits(u32::from_le_bytes(seed.after[..4].try_into().unwrap()));
    let inverse = f32::from_bits(u32::from_le_bytes(seed.after[4..].try_into().unwrap()));
    assert!((scale - 0.98819 * f32::from_bits(input)).abs() < 1e-6);
    assert!((scale * inverse - 1.0).abs() < 1e-6);
    assert_eq!(summary(&owned).values().sum::<usize>(), 3);

    // Apply the plan; the next tick has nothing to do.
    for w in &p.writes {
        m.put(w.at, &w.after);
    }
    let again = plan(&scan(&m).unwrap(), ALL, input, &mut owned);
    assert!(again.writes.is_empty() && again.conflicts == 0);

    // Unselected groups are never adopted.
    let mut fresh = BTreeMap::new();
    let only_coaches = Groups {
        coaches: true,
        ..Groups::default()
    };
    let p = plan(&scan(&arena()).unwrap(), only_coaches, input, &mut fresh);
    assert_eq!(p.writes.len(), 1);
    assert!(
        plan(&found, Groups::default(), input, &mut BTreeMap::new())
            .writes
            .is_empty()
    );
}

#[test]
fn plan_skips_seed_after_native_recompute_and_counts_foreign_flag_changes() {
    let m = arena();
    let input = input97();
    let mut owned = BTreeMap::new();
    let p = plan(&scan(&m).unwrap(), ALL, input, &mut owned);
    // Only the flag writes land; the game recomputes the arena cache itself.
    for w in p.writes.iter().filter(|w| w.after.len() == 1) {
        m.put(w.at, &w.after);
    }
    m.pair(HEAP + 0x2000 + 0x10_0000, 0.98819 * f32::from_bits(input));
    m.pair(HEAP + 0x1000 + 0x10_0000, f32::from_bits(input));
    let next = plan(&scan(&m).unwrap(), ALL, input, &mut owned);
    assert!(next.writes.is_empty(), "{:?}", next.writes);
    // Someone else changes an owned flag: counted and skipped, never rewritten.
    m.put(HEAP + 0x10_0000 + 0xF30 + OPT_IN, &[0x41]);
    let foreign = plan(&scan(&m).unwrap(), ALL, input, &mut owned);
    assert_eq!(foreign.conflicts, 1);
    assert!(foreign.writes.is_empty());
}

#[test]
fn plan_forgets_unloaded_characters_and_never_writes_to_them() {
    let m = arena();
    let input = input97();
    let mut owned = BTreeMap::new();
    plan(&scan(&m).unwrap(), ALL, input, &mut owned);
    assert_eq!(owned.len(), 3);
    // Coach unloads; a new object reuses the node with another actor.
    m.character(HEAP, Some(&CLASSES[0]), 0x01, 0.95);
    m.q(HEAP + NODE_ACTOR, HEAP + 0x50_0000);
    m.q(HEAP + 0x50_0000, BASE + CLASSES[0].actor);
    m.q(HEAP + 0x50_0000 + 0x78, BASE + CLASSES[0].interface);
    m.q(HEAP + 0x50_0000 + 0xF30, BASE + CLASSES[0].description);
    m.put(HEAP + 0x50_0000 + 0xF30 + OPT_IN, &[0x01]);
    m.pair(HEAP + 0x50_0000, 0.95);
    let p = plan(&scan(&m).unwrap(), ALL, input, &mut owned);
    // The old receipt is dropped; the new coach is adopted fresh.
    assert_eq!(owned.len(), 3);
    assert_eq!(owned[&HEAP].actor, HEAP + 0x50_0000);
    assert!(p.writes.iter().all(|w| w.actor != HEAP + 0x10_0000));
}

#[test]
fn identity_reverification_rejects_a_swapped_object_before_any_write() {
    let m = arena();
    assert!(identity(&m, HEAP + 0x2000).unwrap().is_some());
    m.q(HEAP + 0x2000 + 0x10_0000, BASE + CLASSES[0].actor); // class swapped
    assert!(identity(&m, HEAP + 0x2000).is_err());
    m.q(HEAP + 0x2000 + 0x10_0000, BASE + CLASSES[2].actor);
    m.q(HEAP + 0x2000 + NODE_ACTOR, 0); // actor released
    assert!(identity(&m, HEAP + 0x2000).unwrap().is_none());
}

#[test]
fn group_selection_round_trips_and_defaults_off_for_old_plans() {
    let g: Groups =
        serde_json::from_str(r#"{"coaches":true,"cheerleaders":false,"arenaPersonnel":true}"#)
            .unwrap();
    assert!(g.any() && g.includes(Group::Coach) && !g.includes(Group::Cheerleader));
    assert!(!Groups::default().any());
    assert!(serde_json::from_str::<Groups>(r#"{"coaches":true,"x":1}"#).is_err());
}

#[test]
fn player_class_is_withheld_before_and_after_wrapper_binding_without_celebrity_proof() {
    for flag in [0x00, 0x41, 0x43] {
        for description in [scaler_v2::DESCRIPTION, scaler_v2::DESCRIPTION_PRIVATE] {
            let m = arena();
            let node = HEAP + 0x6000;
            let actor = m.character(node, Some(&CLASSES[3]), flag, 1.03674);
            m.q(actor + 0xF30, BASE + description);
            m.put(actor + 0xF30 + OPT_IN, &[flag]);
            m.link(&[
                HEAP,
                HEAP + 0x1000,
                HEAP + 0x2000,
                HEAP + 0x3000,
                HEAP + 0x4000,
                HEAP + 0x5000,
                node,
            ]);
            // Listed first, not yet bound to any wrapper: no celebrity inference.
            assert!(identity(&m, node).unwrap().is_none());
            let found = scan(&m).unwrap();
            assert!(found.iter().all(|c| c.node != node));
            let mut owned = BTreeMap::new();
            let before = plan(&found, ALL, input97(), &mut owned);
            assert_eq!(before.writes.len(), 5, "three supported groups unchanged");
            assert!(before.writes.iter().all(|w| w.actor != actor));
            assert!(!owned.contains_key(&node));
            // Later wrapper binding cannot make an old NPC plan/identity own it.
            m.roster(&[HEAP + 0x4000, node]);
            assert!(identity(&m, node).unwrap().is_none());
            let found = scan(&m).unwrap();
            assert!(found.iter().all(|c| c.node != node));
            let after = plan(&found, ALL, input97(), &mut BTreeMap::new());
            assert!(after.writes.iter().all(|w| w.actor != actor));
            assert_eq!(m.read(actor + 0xF30 + OPT_IN, 1).unwrap(), [flag]);
            assert!(!ALL.includes(Group::Celebrity));
        }
    }
}

#[test]
fn pure_planner_never_adopts_or_replays_forged_or_retained_player_receipts() {
    for flag in [0x00, 0x41, 0x43] {
        let c = Character {
            group: Group::Celebrity,
            node: HEAP + 0x6000,
            actor: HEAP + 0x106000,
            description: HEAP + 0x106000 + 0xF30,
            flag,
            scale: 1.03674f32.to_bits(),
            inverse: (1.0f32 / 1.03674).to_bits(),
        };
        let mut owned = BTreeMap::new();
        assert!(
            plan(&[c.clone()], ALL, input97(), &mut owned)
                .writes
                .is_empty()
        );
        assert!(owned.is_empty());
        for seeded in [false, true] {
            owned.insert(
                c.node,
                Owned {
                    group: c.group,
                    node: c.node,
                    actor: c.actor,
                    description: c.description,
                    original_flag: flag & !OPT_IN_BIT,
                    original: (c.scale, c.inverse),
                    seed: Some(seed_pair(c.scale, input97())),
                    seeded,
                },
            );
            let p = plan(&[c.clone()], ALL, input97(), &mut owned);
            assert!(p.writes.is_empty() && p.conflicts == 0);
            assert!(
                !owned.contains_key(&c.node),
                "unsupported receipt must be dropped without writes"
            );
        }
    }
}

#[test]
fn native_opted_in_generic_referee_stays_untouched_with_withheld_player_nearby() {
    let m = arena();
    let referee = HEAP + 0x3000;
    let c = identity(&m, referee).unwrap().unwrap();
    assert_eq!(c.group, Group::Arena);
    assert_ne!(c.flag & OPT_IN_BIT, 0);
    let mut owned = BTreeMap::new();
    let p = plan(&scan(&m).unwrap(), ALL, input97(), &mut owned);
    assert_eq!(p.writes.len(), 5);
    assert!(p.writes.iter().all(|w| w.actor != c.actor));
    assert!(!owned.contains_key(&referee));
}

#[test]
fn a_roster_change_during_the_walk_is_not_ready() {
    let m = arena();
    assert!(scan(&m).is_ok());
    // A row appears mid-load whose manager is not readable yet.
    m.q(BASE + scaler_v2::TABLE + 0x50 + 0x10, 0xDEAD_0000);
    assert!(scan(&m).is_err());
}
