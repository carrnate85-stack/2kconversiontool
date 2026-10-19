//! Explicit owned-session deep readiness probe; no writer or native calls.
use std::{fs, path::PathBuf};
#[test]
#[ignore = "Explicit owned dev-session read-only deep Freestyle readiness probe"]
fn owned_picker_passes_existing_category_capture_without_writes() {
    let root = PathBuf::from(
        std::env::var("C27_FREESTYLE_PROBE_ROOT").expect("Explicit portable root required"),
    );
    let leases: Vec<_> = fs::read_dir(root.join("Data"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("blacktop-outfit-v2-")
                && p.extension().is_some_and(|e| e == "lease")
        })
        .collect();
    assert_eq!(
        leases.len(),
        1,
        "Exactly one current owned dev lease required"
    );
    let bytes = fs::read(&leases[0]).unwrap();
    assert_eq!(bytes.len(), 1632);
    assert_eq!(&bytes[..8], b"2KOV2L01");
    let nonce = std::str::from_utf8(&bytes[48..96]).unwrap();
    assert!(nonce.len() == 48 && nonce.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        leases[0].file_name().unwrap().to_string_lossy(),
        format!("blacktop-outfit-v2-{nonce}.lease")
    );
    let identity = |pid_at: usize, birth_at: usize, path_at: usize| {
        let wide: Vec<_> = bytes[path_at..path_at + 512]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
            .take_while(|v| *v != 0)
            .collect();
        crate::session_safety::Identity {
            pid: u32::from_le_bytes(bytes[pid_at..pid_at + 4].try_into().unwrap()),
            started: u64::from_le_bytes(bytes[birth_at..birth_at + 8].try_into().unwrap())
                .to_string(),
            path: String::from_utf16(&wide).unwrap(),
        }
    };
    let game = identity(8, 24, 96);
    let parent = identity(16, 40, 1120);
    let path = PathBuf::from(&game.path);
    let access = crate::blacktop_native::feature_writer::Access::open(
        &game,
        path.parent().unwrap(),
        &parent,
    )
    .unwrap();
    let first = crate::freestyle_v2::capture(&access, &access).unwrap();
    let second = crate::freestyle_v2::capture(&access, &access).unwrap();
    let tables = crate::freestyle_v2::Tables::read(&access).unwrap();
    let plan = crate::freestyle_v2::plan(tables, &first, &second).unwrap();
    crate::wnba_priority_categories::verify_source_readonly(&root, nonce, &game, &access).unwrap();
    assert_eq!(plan.changes.len(), 4);
    assert_eq!(fs::read(&leases[0]).unwrap(), bytes);
    println!(
        "{}",
        serde_json::json!({"schema":"wnba-freestyle-deep-readonly-v1","gamePid":game.pid,"teams":first.team_count,"eligible":first.eligible,"profile":first.profile,"categoryDwords":4,"gameWrites":0,"keyReads":0,"gameFunctionCalls":0})
    );
}
