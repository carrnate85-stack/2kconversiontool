use super::*;
fn put(b: &mut [u8], at: usize, v: u64, n: usize) {
    b[at..at + n].copy_from_slice(&v.to_le_bytes()[..n]);
}
fn fixture(base: usize, rows: &[(u32, u32, &str)]) -> Vec<u8> {
    let mut b = Vec::new();
    for (i, (pid, parent, name)) in rows.iter().enumerate() {
        let name = name.encode_utf16().collect::<Vec<_>>();
        let maximum = if name.is_empty() {
            0
        } else {
            name.len() * 2 + 2
        };
        let len = (48 + maximum).next_multiple_of(8);
        let at = b.len();
        b.resize(at + len, 0);
        if i + 1 < rows.len() {
            put(&mut b, at, len as u64, 4);
        }
        put(&mut b, at + 8, *pid as u64, 8);
        put(&mut b, at + 16, *parent as u64, 8);
        put(&mut b, at + 24, if *pid == 0 { 0 } else { 17 }, 8);
        put(&mut b, at + 32, (name.len() * 2) as u64, 2);
        put(&mut b, at + 34, maximum as u64, 2);
        if !name.is_empty() {
            put(&mut b, at + 40, (base + at + 48) as u64, 8);
        }
        for (n, c) in name.into_iter().enumerate() {
            put(&mut b, at + 48 + n * 2, c as u64, 2);
        }
    }
    b
}
fn parsed(b: &[u8]) -> Result<Vec<SnapshotRow>> {
    decode_basic_snapshot(b, 0x10000, 42)
}
#[test]
fn qa073_basic_complete_record_parent_unicode() {
    let b = fixture(0x10000, &[(0, 0, ""), (42, 8, "launcher \u{1f3c0}.exe")]);
    let r = parsed(&b).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!((r[1].pid, r[1].parent), (42, 8));
    assert_eq!(r[1].name, "launcher \u{1f3c0}.exe");
    assert!(decode_basic_snapshot(&b, 0x10000, 99).is_err());
    assert!(decode_basic_snapshot(&b, 0x10000, 0).is_err());
}
#[test]
fn qa073_basic_corrupt_fields_and_chain() {
    let good = fixture(0x10000, &[(0, 0, ""), (42, 8, "launcher.exe")]);
    let at = 48;
    for (offset, v, n) in [
        (0, 47, 4),
        (0, 49, 4),
        (0, u32::MAX as u64, 4),
        (at + 8, 1u64 << 32, 8),
        (at + 16, 1u64 << 32, 8),
        (at + 24, 0, 8),
        (at + 32, 3, 2),
        (at + 32, 520, 2),
        (at + 34, 1, 2),
        (at + 34, 522, 2),
        (at + 40, 0, 8),
        (at + 40, 0x10000 + at as u64 + 50, 8),
        (at + 40, u64::MAX, 8),
        (at + 48, 0, 2),
        (at + 48, 31, 2),
        (at + 48, 47, 2),
        (at + 48, 92, 2),
        (at + 48, 58, 2),
        (at + 48, 0xD800, 2),
        (at + 48, 0xDC00, 2),
        (8, 42, 8),
        (40, 0x10000 + 48, 8),
    ] {
        let mut b = good.clone();
        put(&mut b, offset, v, n);
        assert!(parsed(&b).is_err(), "offset={offset} value={v}");
    }
    for n in [0, 47, 95, good.len() - 8] {
        assert!(parsed(&good[..n]).is_err());
    }
    let mut b = good.clone();
    b.extend([0; 8]);
    assert!(parsed(&b).is_err());
    let mut b = good;
    put(&mut b, 80, 0, 2);
    assert!(parsed(&b).is_err());
}
#[test]
fn qa073_basic_count_size_bounds() {
    let mut rows = (1..=65536).map(|n| (n, 0, "x")).collect::<Vec<_>>();
    assert_eq!(parsed(&fixture(0x10000, &rows)).unwrap().len(), 65536);
    rows.push((65537, 0, "x"));
    assert!(parsed(&fixture(0x10000, &rows)).is_err());
    assert!(parsed(&vec![0; 16 * 1024 * 1024 + 1]).is_err());
}
fn success(s: &mut [u64], r: &mut u32, parent: u32) -> i32 {
    let b = fixture(s.as_ptr() as usize, &[(42, parent, "launcher.exe")]);
    let out = unsafe { std::slice::from_raw_parts_mut(s.as_mut_ptr().cast::<u8>(), s.len() * 8) };
    out[..b.len()].copy_from_slice(&b);
    *r = b.len() as u32;
    0
}
#[test]
fn qa073_basic_bounded_fresh_query() {
    let mut calls = 0;
    let rows = basic_snapshot_with(42, |s, r| {
        calls += 1;
        if calls == 1 {
            *r = 70000;
            0xC0000004u32 as i32
        } else {
            assert_eq!(s.len() * 8, 135536);
            success(s, r, 9)
        }
    })
    .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(rows[0].parent, 9);
    for status in [1, 0xC0000003u32 as i32, 0xC0000023u32 as i32] {
        let mut n = 0;
        assert!(
            basic_snapshot_with(42, |_, r| {
                n += 1;
                *r = 1;
                status
            })
            .is_err()
        );
        assert_eq!(n, 1);
    }
    let mut n = 0;
    assert!(
        basic_snapshot_with(42, |s, r| {
            n += 1;
            *r = (s.len() * 8 + 8) as u32;
            0xC0000023u32 as i32
        })
        .is_err()
    );
    assert_eq!(n, 4);
    assert!(
        basic_snapshot_with(42, |_, r| {
            *r = 16 * 1024 * 1024;
            0xC0000004u32 as i32
        })
        .is_err()
    );
    for len in [0, 65537] {
        assert!(
            basic_snapshot_with(42, |_, r| {
                *r = len;
                0
            })
            .is_err()
        );
    }
    for parent in [8, 9] {
        assert_eq!(
            basic_snapshot_with(42, |s, r| success(s, r, parent)).unwrap()[0].parent,
            parent
        );
    }
}
#[test]
fn qa073_basic_fallback_order_fail_closed() {
    let calls = std::cell::RefCell::new(Vec::new());
    let rows = snapshot_transport(
        || {
            calls.borrow_mut().push(252);
            anyhow::bail!("unsupported")
        },
        || {
            calls.borrow_mut().push(5);
            Ok(vec![SnapshotRow {
                name: "fresh.exe".into(),
                pid: 42,
                parent: 3,
            }])
        },
        || panic!("unexpected Toolhelp"),
    )
    .unwrap();
    assert_eq!(*calls.borrow(), vec![252, 5]);
    assert_eq!(rows[0].parent, 3);
    calls.borrow_mut().clear();
    assert!(
        snapshot_transport(
            || {
                calls.borrow_mut().push(252);
                anyhow::bail!("corrupt")
            },
            || {
                calls.borrow_mut().push(5);
                anyhow::bail!("failed")
            },
            || {
                calls.borrow_mut().push(0);
                anyhow::bail!("failed closed")
            }
        )
        .is_err()
    );
    assert_eq!(*calls.borrow(), vec![252, 5, 0]);
    // A successful native status with corrupt bytes must invoke a fresh
    // compatibility snapshot, not accept a partial Basic list.
    let repaired = snapshot_transport(
        || {
            basic_snapshot_with(42, |storage, returned| {
                let status = success(storage, returned, 11);
                storage[3] = 0; // Missing nonzero process sequence.
                status
            })
        },
        || {
            Ok(vec![SnapshotRow {
                name: "fresh.exe".into(),
                pid: 42,
                parent: 12,
            }])
        },
        || panic!("unexpected Toolhelp"),
    )
    .unwrap();
    assert_eq!(repaired[0].parent, 12);

    assert_eq!(
        snapshot_transport(
            || basic_snapshot_with(42, |s, r| success(s, r, 11)),
            || panic!("unexpected classic"),
            || panic!("unexpected Toolhelp")
        )
        .unwrap()[0]
            .parent,
        11
    );
}
#[test]
fn qa073_basic_live_bracket_pid_name_parent() {
    // Metadata only, compatibility fallback on unsupported Windows.
    let Ok(before) = basic_snapshot_rows() else {
        return;
    };
    let classic = system_snapshot_rows().unwrap();
    let tool = toolhelp_snapshot_rows().unwrap();
    let after = basic_snapshot_rows().unwrap();
    let mut compared = 0;
    for a in &before {
        if a.pid == 0 {
            continue;
        }
        let Some(b) = after
            .iter()
            .find(|b| b.pid == a.pid && b.name == a.name && b.parent == a.parent)
        else {
            continue;
        };
        for rows in [&classic, &tool] {
            let c = rows
                .iter()
                .find(|c| c.pid == a.pid)
                .expect("stable bracket process missing");
            assert_eq!((&c.name, c.parent), (&b.name, b.parent));
        }
        compared += 1;
    }
    assert!(compared > 0);
}
#[test]
#[ignore = "explicit metadata-only enumeration benchmark; no game access"]
fn qa073_basic_process_snapshot_benchmark() {
    let sample = |f: fn() -> Result<Vec<SnapshotRow>>| {
        let mut us = Vec::new();
        let mut count = 0;
        for _ in 0..100 {
            let start = std::time::Instant::now();
            let rows = f().unwrap();
            us.push(start.elapsed().as_micros());
            count += rows.len();
        }
        us.sort();
        serde_json::json!({"samples":100,"p50Microseconds":us[50],"p95Microseconds":us[95],"totalMicroseconds":us.iter().sum::<u128>(),"rows":count})
    };
    println!(
        "QA073_RUST_SNAPSHOT_BENCH {}",
        serde_json::json!({"basic":sample(basic_snapshot_rows),"classic":sample(system_snapshot_rows),"toolhelp":sample(toolhelp_snapshot_rows),"freshIdentityQueriesUnchanged":true})
    );
}
