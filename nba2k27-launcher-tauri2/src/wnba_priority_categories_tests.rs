use super::*;
use std::{cell::Cell, collections::BTreeMap};
const SESSION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
fn identity() -> Identity {
    Identity {
        pid: 123,
        started: "456".into(),
        path: "C:\\game\\NBA2K27.exe".into(),
    }
}
fn approval() -> Vec<u8> {
    let mut b = b"C27WNPC1".to_vec();
    b.extend(123u32.to_le_bytes());
    b.extend([0; 4]);
    b.extend(456u64.to_le_bytes());
    b.extend(SESSION.bytes());
    b
}
#[test]
fn category_token_is_distinct_and_exact_in_every_byte() {
    let good = approval();
    assert!(token(&good, SESSION, &identity()).is_ok());
    for i in 0..72 {
        let mut b = good.clone();
        b[i] ^= 1;
        assert!(token(&b, SESSION, &identity()).is_err(), "mutation {i}");
    }
    assert!(token(&good[..71], SESSION, &identity()).is_err());
    let mut source = good.clone();
    source[..8].copy_from_slice(b"C27WNPA1");
    assert!(token(&source, SESSION, &identity()).is_err());
    let mut other = identity();
    other.pid += 1;
    assert!(token(&good, SESSION, &other).is_err());
    other = identity();
    other.started = "457".into();
    assert!(token(&good, SESSION, &other).is_err());
    assert!(token(&good, &"a".repeat(48), &identity()).is_err());
    assert!(token(&good, "../x", &identity()).is_err());
}
#[test]
fn receipt_requires_exact_terminal_registration_for_the_same_session() {
    let good = format!(
        "SCHEMA=wnba-native-source-admission-test-v1\nSTATE=REGISTERED\nPID=123\nSESSION={SESSION}\nREASON=Test\nCATEGORY_WRITES=0\nGAME_CODE_HOOKS=0\nWNBA_GAMEPLAY_VERIFIED=0\n"
    );
    assert!(receipt(good.as_bytes(), SESSION, &identity()).is_ok());
    for (a, b) in [
        ("STATE=REGISTERED", "STATE=REFUSED"),
        ("PID=123", "PID=124"),
        ("CATEGORY_WRITES=0", "CATEGORY_WRITES=1"),
        (SESSION, "ffffffffffffffffffffffffffffffffffffffffffffffff"),
    ] {
        assert!(receipt(good.replace(a, b).as_bytes(), SESSION, &identity()).is_err());
    }
    assert!(
        receipt(
            format!("{good}STATE=REGISTERED\n").as_bytes(),
            SESSION,
            &identity()
        )
        .is_err()
    );
}
struct Fake {
    bytes: BTreeMap<u64, u8>,
    checks: Cell<usize>,
}
impl Memory for Fake {
    fn base(&self) -> u64 {
        0x140000000
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        (0..len)
            .map(|i| {
                self.bytes
                    .get(&(at + i as u64))
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("unmapped"))
            })
            .collect()
    }
    fn checkpoint(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        Ok(())
    }
}
impl Fake {
    fn put(&mut self, at: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.insert(at + i as u64, *v);
        }
    }
    fn q(&mut self, at: u64, v: u64) {
        self.put(at, &v.to_le_bytes());
    }
    fn d(&mut self, at: u64, v: u32) {
        self.put(at, &v.to_le_bytes());
    }
}
fn fixture() -> (Fake, Vec<u8>, Vec<u8>) {
    let mut f = Fake {
        bytes: BTreeMap::new(),
        checks: Cell::new(0),
    };
    let mut index = vec![0u8; 120];
    for (i, b) in index.iter_mut().enumerate() {
        *b = i as u8;
    }
    index[8] = 1;
    let base = f.base();
    let context = 0x20000;
    let manager = 0x30000;
    let priority = 0x40000;
    f.q(base + 0xBB9A8E8 + 0x38, context);
    f.q(context + 0x100, manager);
    f.q(manager, base + 0x812C8C8);
    f.q(manager + 0x48, priority);
    f.q(priority, base + 0x812D968);
    f.q(priority + 8, manager);
    f.q(priority + 0x10, 0x50000);
    f.q(priority + 0x18, 0x60000);
    f.q(priority + 0x20, 0x70000);
    f.q(priority + 0x28, 0x80000);
    f.d(priority + 0x30, 1);
    f.d(priority + 0x34, 1);
    f.put(0x50000, &index[96..]);
    f.put(0x60000, &index[..48]);
    f.put(0x70000, &index[48..96]);
    let prefix = b"C:\\portable\\Data\\wnba-priority-v2\\qa058-wpf-era/".to_vec();
    let mut path = prefix.clone();
    path.push(0);
    f.put(0x80000, &path);
    (f, index, prefix)
}
#[test]
fn native_source_requires_owned_manager_prefix_raw_entry_header_and_descriptor() {
    let (f, index, prefix) = fixture();
    assert!(source_snapshot(&f, &prefix, &index, PackProfile::Freestyle).is_ok());
    for at in [
        0x30000, 0x30048, 0x40000, 0x40008, 0x40030, 0x40034, 0x50000, 0x60000, 0x70000, 0x70010,
        0x80000,
    ] {
        let (mut f, index, prefix) = fixture();
        let old = f.bytes[&at];
        f.put(at, &[old ^ 1]);
        assert!(
            source_snapshot(&f, &prefix, &index, PackProfile::Freestyle).is_err(),
            "foreign source field {at:x}"
        );
    }
    let (mut f, index, prefix) = fixture();
    f.put(0x70008, &[99; 8]);
    f.put(0x7002c, &[99; 4]);
    assert!(
        source_snapshot(&f, &prefix, &index, PackProfile::Freestyle).is_ok(),
        "mutable handle/tag are not ownership fields"
    );
}
#[test]
fn two_entry_source_is_exact_and_cannot_be_adopted_as_one_entry() {
    let (mut f, mut index, _) = fixture();
    index.extend(120u8..144);
    f.d(0x40030, 2);
    f.put(0x50000, &index[96..]);
    let prefix = b"C:\\portable\\Data\\wnba-priority-v2\\qa064-blacktop-source/";
    let mut path = prefix.to_vec();
    path.push(0);
    f.put(0x80000, &path);
    assert!(source_snapshot(&f, prefix, &index, PackProfile::Blacktop).is_ok());
    assert!(source_snapshot(&f, prefix, &index, PackProfile::Freestyle).is_err());
    f.put(0x50000 + 24, &[99]);
    assert!(
        source_snapshot(&f, prefix, &index, PackProfile::Blacktop).is_err(),
        "second source entry must be exact"
    );
    let (f, index, prefix) = fixture();
    assert!(source_snapshot(&f, &prefix, &index, PackProfile::Blacktop).is_err());
}
#[test]
fn source_profile_is_chosen_only_by_exact_owned_permission() {
    let mut bytes = approval();
    for (magic, profile) in [
        (b"C27WNPA1", PackProfile::Freestyle),
        (b"C27WNBT1", PackProfile::Blacktop),
    ] {
        bytes[..8].copy_from_slice(magic);
        assert_eq!(
            source_profile(&bytes, SESSION, &identity()).unwrap(),
            profile
        );
        for n in 0..72 {
            let mut changed = bytes.clone();
            changed[n] ^= 1;
            assert!(
                source_profile(&changed, SESSION, &identity()).is_err(),
                "mutation {n}"
            );
        }
    }
    for magic in [b"C27WNPC1", b"C27WNBC1", b"C27WNBC2"] {
        bytes[..8].copy_from_slice(magic);
        assert!(source_profile(&bytes, SESSION, &identity()).is_err());
    }
}
