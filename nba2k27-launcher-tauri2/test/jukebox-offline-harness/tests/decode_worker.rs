#[path = "../../../src/jukebox_audio.rs"]
mod jukebox_audio;
#[path = "../../../src/jukebox_catalog.rs"]
mod jukebox_catalog;
#[path = "../../../src/jukebox_decode_worker.rs"]
mod jukebox_decode_worker;
#[path = "../../../src/jukebox_decoder.rs"]
mod jukebox_decoder;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
#[test]
fn actual_owned_decoder_child_returns_pcm_and_exits() {
    let source = jukebox_audio::pcm_wave(&[0; 1920]).unwrap();
    let pcm = jukebox_decode_worker::decode(
        Path::new(env!("CARGO_BIN_EXE_jukebox-decode-test-worker")),
        &source,
        &"a".repeat(48),
        Arc::new(AtomicBool::new(false)),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(pcm, source[44..]);
}
#[test]
fn malformed_input_is_reaped_and_precancel_never_spawns() {
    let exe = Path::new(env!("CARGO_BIN_EXE_jukebox-decode-test-worker"));
    let start = Instant::now();
    assert!(
        jukebox_decode_worker::decode(
            exe,
            b"invalid-media",
            &"b".repeat(48),
            Arc::new(AtomicBool::new(false)),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(10));
    let start = Instant::now();
    assert!(
        jukebox_decode_worker::decode(
            exe,
            b"input",
            &"c".repeat(48),
            Arc::new(AtomicBool::new(true)),
            |_| Ok(())
        )
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(1));
}
#[test]
fn actively_cancelled_native_child_is_killed_and_reaped_promptly() {
    let exe = Path::new(env!("CARGO_BIN_EXE_jukebox-decode-test-worker"));
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        signal.store(true, std::sync::atomic::Ordering::Release);
    });
    let start = Instant::now();
    assert!(
        jukebox_decode_worker::decode(exe, b"input", &"d".repeat(48), cancel, |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    t.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
}
#[test]
fn native_child_timeout_is_bounded_and_reaped() {
    let exe = Path::new(env!("CARGO_BIN_EXE_jukebox-decode-test-worker"));
    let start = Instant::now();
    assert!(
        jukebox_decode_worker::decode_test_deadline(
            exe,
            b"input",
            &"d".repeat(48),
            Duration::from_millis(150)
        )
        .unwrap_err()
        .to_string()
        .contains("timed out")
    );
    assert!(start.elapsed() < Duration::from_secs(3));
}
