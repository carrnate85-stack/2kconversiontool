use super::*;
use crate::startup_video_native::Bounds;
#[derive(Default)]
struct Fake {
    log: Vec<String>,
    diagnostics: Vec<String>,
    window: Option<OwnedWindow>,
    audio: VecDeque<Notice>,
    fail_create: bool,
    fail_silence: bool,
    ended: bool,
    foreign: bool,
    fail_destroy: bool,
}
impl Port for Fake {
    fn foreign_foreground(&mut self) -> Result<bool> {
        Ok(self.foreign)
    }
    fn observe(&mut self) -> Result<Observation> {
        Ok(if self.ended {
            Observation::GameEnded
        } else {
            Observation::Window(self.window)
        })
    }
    fn create_hidden(&mut self, _: OwnedWindow, _: Volume) -> Result<()> {
        self.log.push("hidden".into());
        ensure!(!self.fail_create, "fixture create failure");
        Ok(())
    }
    fn create_owner(&mut self, _owner: u64, _volume: Volume) -> Result<()> {
        self.log.push("owner-hidden".into());
        ensure!(!self.fail_create, "fixture create failure");
        Ok(())
    }
    fn bind_game(&mut self, _game: crate::startup_video_native::ExpectedGame) -> Result<()> {
        self.log.push("bind-game".into());
        Ok(())
    }
    fn start_audio(&mut self) -> Result<()> {
        self.log.push("audio-start".into());
        Ok(())
    }
    fn poll_audio(&mut self) -> Result<Option<Notice>> {
        Ok(self.audio.pop_front())
    }
    fn set_volume(&mut self, _: Volume) -> Result<()> {
        self.log.push("volume".into());
        Ok(())
    }
    fn show_fullscreen(&mut self) -> Result<()> {
        self.log.push("fullscreen".into());
        Ok(())
    }
    fn start_controllers(&mut self, _: u64) -> Result<()> {
        self.log.push("pads-start".into());
        Ok(())
    }
    fn controllers(&mut self, _: u64) -> Result<Vec<PadSample>> {
        Ok(vec![])
    }
    fn stop_controllers(&mut self) {
        self.log.push("pads-stop".into());
    }
    fn play(&mut self) -> Result<()> {
        self.log.push("play".into());
        Ok(())
    }
    fn silence(&mut self) -> Result<()> {
        self.log.push("silence".into());
        ensure!(!self.fail_silence, "fixture mute unavailable");
        Ok(())
    }
    fn stop_audio(&mut self) {
        self.log.push("audio-stop".into());
    }
    fn destroy(&mut self) -> Result<()> {
        self.log.push("destroy".into());
        ensure!(!self.fail_destroy, "fixture destroy failure");
        Ok(())
    }
    fn return_focus_if_owned(&mut self, _: GameKey) -> Result<()> {
        self.log.push("conditional-focus".into());
        Ok(())
    }
    fn diagnostic(&mut self, message: &str) {
        self.diagnostics.push(message.into());
    }
}
fn key() -> GameKey {
    GameKey { pid: 10, birth: 20 }
}
fn owned() -> OwnedWindow {
    OwnedWindow {
        window: policy::Window {
            game: key(),
            hwnd: 30,
            visible: true,
            minimized: false,
            top_level: true,
            width: 1920,
            height: 1080,
        },
        monitor: Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        },
    }
}
fn preparing() -> Engine<Fake> {
    let mut e = Engine::new(
        1,
        key(),
        Fake {
            window: Some(owned()),
            ..Default::default()
        },
        0,
    )
    .unwrap();
    e.step(0).unwrap();
    e.step(300).unwrap();
    assert_eq!(e.phase(), Phase::Preparing);
    e
}
fn playing() -> Engine<Fake> {
    let mut e = preparing();
    e.event(1, Event::VideoPrepared, 310).unwrap();
    e.port.audio.push_back(Notice::Watching(Some(Volume {
        level: 0.2,
        muted: false,
    })));
    e.step(320).unwrap();
    assert_eq!(e.phase(), Phase::Playing);
    e
}
#[test]
fn real_window_and_both_audio_and_media_readiness_precede_fullscreen_play() {
    let mut e = Engine::new(1, key(), Fake::default(), 0).unwrap();
    e.step(0).unwrap();
    e.step(900).unwrap();
    assert!(e.port.log.is_empty());
    e.port.window = Some(owned());
    e.step(1000).unwrap();
    e.step(1200).unwrap();
    assert!(e.port.log.is_empty());
    e.step(1300).unwrap();
    assert_eq!(e.port.log, ["hidden"]);
    e.event(1, Event::VideoPrepared, 1310).unwrap();
    assert_eq!(e.port.log, ["hidden", "audio-start"]);
    assert!(!e.port.log.iter().any(|v| v == "play"));
    e.port.audio.push_back(Notice::Watching(None));
    e.step(1320).unwrap();
    assert_eq!(
        &e.port.log[2..],
        ["volume", "fullscreen", "pads-start", "play"]
    );
}
#[test]
fn cancellation_before_window_is_inert_and_late_callbacks_cannot_revive_it() {
    let mut e = Engine::new(1, key(), Fake::default(), 0).unwrap();
    e.event(1, Event::Finish(Reason::ParentClosed), 1).unwrap();
    e.port.window = Some(owned());
    e.step(1000).unwrap();
    e.event(1, Event::VideoPrepared, 1001).unwrap();
    assert!(e.finished() && e.port.log.is_empty());
}
#[test]
fn failed_hidden_window_never_starts_audio_or_playback() {
    let mut e = Engine::new(
        1,
        key(),
        Fake {
            window: Some(owned()),
            fail_create: true,
            ..Default::default()
        },
        0,
    )
    .unwrap();
    e.step(0).unwrap();
    e.step(300).unwrap();
    assert!(e.finished());
    assert!(
        !e.port
            .log
            .iter()
            .any(|v| v == "audio-start" || v == "play" || v == "fullscreen")
    );
}
#[test]
fn silence_ack_precedes_restore_and_restoration_ack_precedes_destroy() {
    let mut e = playing();
    e.port.fail_silence = true;
    e.event(1, Event::Finish(Reason::Skip), 1100).unwrap();
    assert!(
        !e.port
            .log
            .iter()
            .any(|v| v == "audio-stop" || v == "destroy")
    );
    e.step(1200).unwrap();
    assert!(!e.port.log.iter().any(|v| v == "audio-stop"));
    e.port.fail_silence = false;
    e.step(1300).unwrap();
    assert_eq!(e.port.log.last().unwrap(), "audio-stop");
    assert!(!e.finished());
    e.port.audio.push_back(Notice::Restored);
    e.step(1400).unwrap();
    assert!(e.finished());
    assert_eq!(
        &e.port.log[e.port.log.len() - 2..],
        ["destroy", "conditional-focus"]
    );
}
#[test]
fn game_or_launcher_shutdown_never_requests_focus_after_restore() {
    for reason in [
        Reason::ParentClosed,
        Reason::GameEnded,
        Reason::LaunchFailed,
    ] {
        let mut e = playing();
        e.event(1, Event::Finish(reason), 1100).unwrap();
        e.port.audio.push_back(Notice::Restored);
        e.step(1200).unwrap();
        assert!(e.finished());
        assert!(!e.port.log.iter().any(|v| v == "conditional-focus"));
    }
}
#[test]
fn media_prepare_timeout_and_late_generation_are_bounded() {
    let mut e = preparing();
    e.event(99, Event::VideoPrepared, 400).unwrap();
    assert_eq!(e.phase(), Phase::Preparing);
    e.step(15_300).unwrap();
    assert!(e.finished());
    assert!(
        !e.port
            .log
            .iter()
            .any(|v| v == "audio-start" || v == "audio-stop")
    );
}
#[test]
fn controller_a_release_uses_the_same_owned_finish_path() {
    let mut e = playing();
    for (at, buttons) in [(400, 0), (1100, 0x1000), (1200, 0), (1400, 0)] {
        e.event(
            1,
            Event::Controller {
                slot: 0,
                connected: true,
                buttons,
            },
            at,
        )
        .unwrap();
    }
    assert_eq!(e.phase(), Phase::Restoring);
    assert_eq!(e.port.log.last().unwrap(), "audio-stop");
}
#[test]
fn decode_failure_before_canplay_never_mutes_game_audio() {
    let mut e = preparing();
    e.event(1, Event::Finish(Reason::Error), 400).unwrap();
    assert!(e.finished());
    assert!(!e.port.log.iter().any(|v| v == "audio-start" || v == "play"));
}
#[test]
fn an_early_guardian_restore_ack_waits_for_native_video_silence() {
    let mut e = playing();
    e.port.fail_silence = true;
    e.port.audio.push_back(Notice::Restored);
    e.step(1100).unwrap();
    assert!(!e.finished());
    assert!(!e.port.log.iter().any(|v| v == "destroy"));
    e.port.fail_silence = false;
    e.step(1200).unwrap();
    assert!(e.finished());
}
#[test]
fn alt_tab_cancels_instead_of_periodically_refocusing_intro() {
    let mut e = playing();
    e.port.foreign = true;
    e.step(1100).unwrap();
    assert_eq!(e.phase(), Phase::Restoring);
    assert_eq!(
        e.port
            .log
            .iter()
            .filter(|v| v.as_str() == "fullscreen")
            .count(),
        1
    );
}
#[test]
fn failed_window_destruction_holds_replacement_and_retries_without_late_focus() {
    let mut e = playing();
    e.port.fail_destroy = true;
    e.event(1, Event::Finish(Reason::Skip), 1100).unwrap();
    e.port.audio.push_back(Notice::Restored);
    e.step(1200).unwrap();
    assert!(!e.finished());
    assert_eq!(e.phase(), Phase::RestorationPending);
    assert!(!e.port.log.iter().any(|v| v == "conditional-focus"));
    e.step(1300).unwrap();
    assert!(!e.finished());
    e.port.fail_destroy = false;
    e.step(1400).unwrap();
    assert!(e.finished());
    assert!(!e.port.log.iter().any(|v| v == "conditional-focus"));
}
#[test]
fn authenticated_pre_drive_failure_completes_after_silence_without_an_infinite_restore_hold() {
    let mut e = preparing();
    e.event(1, Event::VideoPrepared, 310).unwrap();
    e.port
        .audio
        .push_back(Notice::NotStarted("Fixture pre-drive bind failure".into()));
    e.step(320).unwrap();
    assert!(e.finished());
    assert!(!e.port.log.iter().any(|v| v == "play" || v == "fullscreen"));
    assert!(e.port.log.iter().any(|v| v == "silence"));
}
#[test]
fn zero_owned_pre_drive_result_also_completes_a_concurrent_shutdown_without_focus() {
    let mut e = preparing();
    e.event(1, Event::VideoPrepared, 310).unwrap();
    e.event(1, Event::Finish(Reason::ParentClosed), 315)
        .unwrap();
    e.port
        .audio
        .push_back(Notice::NotStarted("Fixture pre-drive failure".into()));
    e.step(320).unwrap();
    assert!(e.finished());
    assert!(!e.port.log.iter().any(|v| v == "conditional-focus"));
}
#[test]
fn a_post_ready_zero_mutation_claim_cannot_release_audio_ownership() {
    let mut e = playing();
    e.port
        .audio
        .push_back(Notice::NotStarted("invalid late attestation".into()));
    e.step(1100).unwrap();
    assert!(!e.finished());
    assert_eq!(e.phase(), Phase::RestorationPending);
    assert!(e.audio_started);
}
#[test]
fn prepare_timeout_identifies_media_vs_audio_without_playing_or_losing_restore_ownership() {
    let mut media = preparing();
    media.step(15_300).unwrap();
    assert!(media.finished());
    assert!(
        media
            .port
            .diagnostics
            .iter()
            .any(|d| d.contains("decodable media"))
    );
    assert!(
        !media
            .port
            .log
            .iter()
            .any(|d| d == "audio-start" || d == "play")
    );
    let mut audio = preparing();
    audio.event(1, Event::VideoPrepared, 310).unwrap();
    audio.step(15_300).unwrap();
    assert!(!audio.finished());
    assert!(
        audio
            .port
            .diagnostics
            .iter()
            .any(|d| d.contains("audio guardian did not become ready"))
    );
    assert!(!audio.port.log.iter().any(|d| d == "play"));
    audio.port.audio.push_back(Notice::Restored);
    audio.step(15_400).unwrap();
    assert!(audio.finished());
}

fn launched_game() -> crate::startup_video_native::ExpectedGame {
    crate::startup_video_native::ExpectedGame::new(
        key(),
        std::path::Path::new(r"C:\Game\NBA2K27.exe"),
    )
    .unwrap()
}
#[test]
fn play_click_starts_without_game_or_audio_then_binds_without_replaying() {
    let mut e = Engine::launch(1, 50, Fake::default(), 0).unwrap();
    assert_eq!(e.phase(), Phase::Preparing);
    e.event(1, Event::VideoPrepared, 10).unwrap();
    assert_eq!(e.phase(), Phase::Playing);
    assert!(e.port.log.contains(&"play".into()));
    assert!(!e.port.log.contains(&"audio-start".into()));
    e.bind_launched_game(launched_game(), 100).unwrap();
    e.port.audio.push_back(Notice::Watching(Some(Volume {
        level: 0.4,
        muted: false,
    })));
    e.step(116).unwrap();
    assert_eq!(e.phase(), Phase::Playing);
    assert_eq!(e.port.log.iter().filter(|v| *v == "play").count(), 1);
    assert_eq!(e.port.log.iter().filter(|v| *v == "audio-start").count(), 1);
    assert!(e.bind_launched_game(launched_game(), 120).is_err());
}
#[test]
fn early_skip_cannot_start_late_game_audio_or_replay() {
    let mut e = Engine::launch(1, 50, Fake::default(), 0).unwrap();
    e.event(1, Event::VideoPrepared, 10).unwrap();
    e.event(1, Event::PointerReleased, 800).unwrap();
    assert!(e.finished());
    e.bind_launched_game(launched_game(), 900).unwrap();
    assert!(!e.port.log.contains(&"bind-game".into()));
    assert!(!e.port.log.contains(&"audio-start".into()));
}
#[test]
fn failed_launch_closes_pre_game_video_without_audio_mutation() {
    let mut e = Engine::launch(1, 50, Fake::default(), 0).unwrap();
    e.event(1, Event::VideoPrepared, 10).unwrap();
    e.event(1, Event::Finish(Reason::LaunchFailed), 100)
        .unwrap();
    assert!(e.finished());
    assert!(e.port.log.contains(&"silence".into()));
    assert!(e.port.log.contains(&"destroy".into()));
    assert!(!e.port.log.contains(&"audio-start".into()));
}
#[test]
fn binding_before_canplay_still_waits_for_decodable_media_before_ducking() {
    let mut e = Engine::launch(1, 50, Fake::default(), 0).unwrap();
    e.bind_launched_game(launched_game(), 5).unwrap();
    assert!(!e.port.log.contains(&"audio-start".into()));
    e.event(1, Event::VideoPrepared, 10).unwrap();
    assert!(e.port.log.contains(&"audio-start".into()));
    assert_eq!(e.phase(), Phase::Preparing);
    e.port.audio.push_back(Notice::Watching(None));
    e.step(20).unwrap();
    assert_eq!(e.phase(), Phase::Playing);
}
#[test]
fn pre_game_alt_tab_closes_video_without_stealing_focus() {
    let mut e = Engine::launch(1, 50, Fake::default(), 0).unwrap();
    e.event(1, Event::VideoPrepared, 10).unwrap();
    e.port.foreign = true;
    e.step(20).unwrap();
    assert!(e.finished());
    assert!(!e.port.log.contains(&"conditional-focus".into()));
}

#[test]
fn late_audio_still_restores_only_after_native_silence_ack() {
    let mut e = Engine::launch(1, 50, Fake::default(), 0).unwrap();
    e.event(1, Event::VideoPrepared, 10).unwrap();
    e.bind_launched_game(launched_game(), 30).unwrap();
    e.port.audio.push_back(Notice::Watching(None));
    e.step(40).unwrap();
    e.port.fail_silence = true;
    e.event(1, Event::Finish(Reason::LaunchFailed), 100)
        .unwrap();
    assert!(!e.port.log.contains(&"audio-stop".into()));
    assert!(!e.finished());
    e.port.fail_silence = false;
    e.step(200).unwrap();
    assert!(e.port.log.contains(&"audio-stop".into()));
    assert!(!e.finished());
    e.port.audio.push_back(Notice::Restored);
    e.step(220).unwrap();
    assert!(e.finished());
}
