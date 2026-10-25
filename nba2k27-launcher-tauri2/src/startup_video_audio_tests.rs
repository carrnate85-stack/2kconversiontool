use super::*;
use std::fs;
use std::io::Cursor;
#[test]
fn extracted_guardian_matches_actual_process_path_before_any_audio_bootstrap() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "guardian-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir_all(&root).unwrap();
    let binary = crate::startup_video_helper::prepare(
        &root,
        include_bytes!("../assets/startup-audio-guardian.exe"),
        include_str!("../assets/startup-audio-guardian.sha256").trim(),
    )
    .unwrap();
    // The helper blocks waiting for its private bootstrap. No bootstrap is sent,
    // so this cannot bind a game, enumerate mixers, or change audio.
    let mut child = Command::new(binary.path())
        .args([FLAG, &"a".repeat(48)])
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let actual = identity(HANDLE(child.as_raw_handle()));
    child.stdin.take();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
        thread::sleep(std::time::Duration::from_millis(10));
    }
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("Owned no-bootstrap fixture did not exit");
    }
    assert_eq!(normalize(&actual.unwrap().exe), normalize(binary.path()));
}
#[test]
fn windows_path_spellings_match_but_other_helpers_and_roots_do_not() {
    let expected = Path::new(r"C:\Portable")
        .join("Cache/Runtime")
        .join("startup-audio-a.exe");
    assert_eq!(
        normalize(&expected),
        normalize(Path::new(
            r"\\?\C:\Portable\Cache\Runtime\startup-audio-a.exe"
        ))
    );
    assert_eq!(
        normalize(Path::new(r"\\?\UNC\server\share\helper.exe")),
        normalize(Path::new(r"\\server\share\helper.exe"))
    );
    for different in [
        r"C:\Other\Cache\Runtime\startup-audio-a.exe",
        r"C:\Portable\Cache\Runtime\startup-audio-b.exe",
    ] {
        assert_ne!(normalize(&expected), normalize(Path::new(different)));
    }
}
fn parent() -> Identity {
    Identity {
        pid: 10,
        birth: 20,
        exe: r"C:\Portable\App\2k Launcher.exe".into(),
    }
}
fn helper() -> PathBuf {
    format!(
        r"C:\Portable\Cache\Runtime\startup-audio-{}.exe",
        "a".repeat(64)
    )
    .into()
}
fn request() -> Bootstrap {
    Bootstrap {
        schema: SCHEMA.into(),
        nonce: "a".repeat(48),
        session: "b".repeat(48),
        helper_sha256: "a".repeat(64),
        parent: parent(),
        game: Identity {
            pid: 30,
            birth: 40,
            exe: r"C:\NBA 2K27\NBA2K27.exe".into(),
        },
    }
}
#[test]
fn bootstrap_is_same_exact_creator_same_exe_and_private_nonce_only() {
    let mut b = request();
    validate_bootstrap(&b, &b.nonce, &parent(), &helper(), 10).unwrap();
    assert!(validate_bootstrap(&b, &"b".repeat(48), &parent(), &helper(), 10).is_err());
    assert!(validate_bootstrap(&b, &b.nonce, &parent(), &helper(), 99).is_err());
    let mut replaced = parent();
    replaced.birth += 1;
    assert!(validate_bootstrap(&b, &b.nonce, &replaced, &helper(), 10).is_err());
    assert!(
        validate_bootstrap(
            &b,
            &b.nonce,
            &parent(),
            Path::new(r"C:\Other\2k Launcher.exe"),
            10
        )
        .is_err()
    );
    b.game.pid = 10;
    assert!(validate_bootstrap(&b, &b.nonce, &parent(), &helper(), 10).is_err());
}
#[test]
fn shipped_main_dev_and_versioned_share_names_bind_without_relaxing_identity_or_root() {
    for name in [
        "2k Launcher Rust.exe",
        "2k Launcher Rust Prototype.exe",
        "nba2k27-launcher-tauri2.exe",
        "2k Launcher.exe",
        "NBA 2K27 Launcher 0.53.29.exe",
    ] {
        let mut b = request();
        b.parent.exe = PathBuf::from(r"C:\Portable\App").join(name);
        validate_bootstrap(&b, &b.nonce, &b.parent, &helper(), 10).unwrap();
        let mut stale = b.parent.clone();
        stale.birth += 1;
        assert!(validate_bootstrap(&b, &b.nonce, &stale, &helper(), 10).is_err());
        b.parent.exe = PathBuf::from(r"C:\Other\App").join(name);
        assert!(validate_bootstrap(&b, &b.nonce, &b.parent, &helper(), 10).is_err());
    }
    for name in [
        "notepad.exe",
        "NBA 2K27 Launcher fake.exe",
        "NBA 2K27 Launcher 0.53.29.exe.exe",
        "NBA 2K27 Launcher 0.53.exe",
    ] {
        let mut b = request();
        b.parent.exe = PathBuf::from(r"C:\Portable\App").join(name);
        assert!(validate_bootstrap(&b, &b.nonce, &b.parent, &helper(), 10).is_err());
    }
}
#[test]
fn bounded_private_packet_does_not_consume_the_following_stop_stream() {
    let mut bytes = 3u32.to_le_bytes().to_vec();
    bytes.extend(b"abcstop");
    let mut input = Cursor::new(bytes);
    assert_eq!(read_packet(&mut input).unwrap(), b"abc");
    let mut tail = String::new();
    input.read_to_string(&mut tail).unwrap();
    assert_eq!(tail, "stop");
    for input in [
        Vec::new(),
        0u32.to_le_bytes().to_vec(),
        ((MAX_PACKET + 1) as u32).to_le_bytes().to_vec(),
        vec![4, 0, 0, 0, 1],
    ] {
        assert!(read_packet(&mut Cursor::new(input)).is_err());
    }
}
#[test]
fn mixer_notices_never_claim_unknown_volume_or_incomplete_restoration() {
    assert_eq!(
        parse(br#"{"event":"volume","found":false,"pending":0}"#).unwrap(),
        Notice::Watching(None)
    );
    assert_eq!(
        parse(br#"{"event":"volume","found":true,"volume":0.3,"muted":false,"pending":1}"#)
            .unwrap(),
        Notice::Watching(Some(Volume {
            level: 0.3,
            muted: false
        }))
    );
    for bad in [
        br#"{"event":"volume","found":true,"volume":2,"muted":false,"pending":0}"#.as_slice(),
        br#"{"event":"audio-restored","pending":1}"#,
        br#"{"event":"whatever","pending":0}"#,
        br#"{"event":"volume","found":true,"volume":0.3,"pending":0}"#,
    ] {
        assert!(parse(bad).is_err());
    }
}
#[test]
fn bounded_latest_mailbox_never_loses_terminal_restore_ack() {
    let mut m = Mailbox::default();
    for _ in 0..100_000 {
        m.put(Notice::Watching(None));
    }
    m.put(Notice::Restored);
    m.put(Notice::Watching(None));
    assert_eq!(m.terminal, Some(Notice::Restored));
}
#[test]
fn pre_drive_failure_requires_exact_nonce_session_and_zero_owned_mutations() {
    let nonce = "a".repeat(48);
    let session = "b".repeat(48);
    let packet = not_started(&nonce, &session, "No audio service").unwrap();
    assert_eq!(
        parse_owned(&packet, &nonce, &session).unwrap(),
        Notice::NotStarted("No audio service".into())
    );
    assert!(parse_owned(&packet, &"c".repeat(48), &session).is_err());
    assert!(parse_owned(&packet, &nonce, &"c".repeat(48)).is_err());
    let mut forged: Value = serde_json::from_slice(&packet).unwrap();
    forged["ownsMutation"] = true.into();
    assert!(parse_owned(&serde_json::to_vec(&forged).unwrap(), &nonce, &session).is_err());
    forged["ownsMutation"] = false.into();
    forged["pending"] = 1.into();
    assert!(parse_owned(&serde_json::to_vec(&forged).unwrap(), &nonce, &session).is_err());
}
#[test]
fn no_mutation_attestation_is_never_accepted_after_any_drive_packet() {
    for first in [Notice::Watching(None), Notice::Pending, Notice::Restored] {
        let mut seen = false;
        accept_drive_scope(first, &mut seen).unwrap();
        assert!(
            accept_drive_scope(Notice::NotStarted("invalid late claim".into()), &mut seen).is_err()
        );
    }
    let mut seen = false;
    accept_drive_scope(Notice::NotStarted("pre-drive error".into()), &mut seen).unwrap();
    assert!(!seen);
}
