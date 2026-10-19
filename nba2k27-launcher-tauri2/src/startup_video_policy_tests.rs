use super::*;
const TOKEN: u64 = 10;
fn game() -> GameKey {
    GameKey {
        pid: 123,
        birth: 456,
    }
}
fn window() -> Window {
    Window {
        game: game(),
        hwnd: 100,
        visible: true,
        minimized: false,
        top_level: true,
        width: 1920,
        height: 1080,
    }
}
fn preparing() -> Session {
    let (mut session, _) = Session::game(TOKEN, game(), 0).unwrap();
    assert!(
        session
            .handle(TOKEN, Event::WindowObserved(Some(window())), 100)
            .unwrap()
            .is_empty()
    );
    let effects = session
        .handle(TOKEN, Event::WindowObserved(Some(window())), 350)
        .unwrap();
    assert_eq!(
        effects[0],
        Effect::CreateHiddenFullscreen {
            monitor: MonitorTarget::GameWindow(100),
            initial_volume: Volume::fallback()
        }
    );
    assert_eq!(
        effects[1],
        Effect::StartAudioGuardian {
            game: Some(game()),
            duck_game: true
        }
    );
    session
}
fn playing() -> Session {
    let mut session = preparing();
    session.handle(TOKEN, Event::VideoPrepared, 400).unwrap();
    let effects = session
        .handle(
            TOKEN,
            Event::AudioReady(Volume {
                level: 0.3,
                muted: false,
            }),
            500,
        )
        .unwrap();
    assert_eq!(
        effects,
        vec![
            Effect::SetVideoVolume(Volume {
                level: 0.3,
                muted: false
            }),
            Effect::ShowTrueFullscreen,
            Effect::StartControllerObserver,
            Effect::PlayVideo
        ]
    );
    session
}
#[test]
fn actual_owned_window_must_be_large_visible_top_level_and_stable() {
    for fault in 0..6 {
        let (mut s, _) = Session::game(TOKEN, game(), 0).unwrap();
        let mut w = window();
        match fault {
            0 => w.game.birth += 1,
            1 => w.visible = false,
            2 => w.minimized = true,
            3 => w.top_level = false,
            4 => w.width = 639,
            _ => w.hwnd = 0,
        };
        s.handle(TOKEN, Event::WindowObserved(Some(w)), 100)
            .unwrap();
        s.handle(TOKEN, Event::WindowObserved(Some(w)), 1000)
            .unwrap();
        assert_eq!(s.phase(), Phase::WaitingWindow);
    }
    let (mut s, _) = Session::game(TOKEN, game(), 0).unwrap();
    s.handle(TOKEN, Event::WindowObserved(Some(window())), 100)
        .unwrap();
    let mut w = window();
    w.hwnd += 1;
    s.handle(TOKEN, Event::WindowObserved(Some(w)), 300)
        .unwrap();
    assert_eq!(s.phase(), Phase::WaitingWindow);
    s.handle(TOKEN, Event::WindowObserved(Some(w)), 550)
        .unwrap();
    assert_eq!(s.phase(), Phase::Preparing);
}
#[test]
fn source_cannot_play_until_both_media_and_audio_guardian_are_ready() {
    let mut s = preparing();
    s.handle(TOKEN, Event::VideoPrepared, 400).unwrap();
    assert_eq!(s.phase(), Phase::Preparing);
    s.handle(TOKEN, Event::AudioReady(Volume::fallback()), 500)
        .unwrap();
    assert_eq!(s.phase(), Phase::Playing);
    let mut s = preparing();
    s.handle(TOKEN, Event::AudioReady(Volume::fallback()), 400)
        .unwrap();
    assert_eq!(s.phase(), Phase::Preparing);
    s.handle(TOKEN, Event::VideoPrepared, 500).unwrap();
    assert_eq!(s.phase(), Phase::Playing);
}
#[test]
fn latest_live_mixer_update_is_not_overridden_by_stale_initial_cache() {
    let mut s = preparing();
    let live = Volume {
        level: 0.12,
        muted: true,
    };
    s.handle(TOKEN, Event::MixerChanged(live), 400).unwrap();
    s.handle(TOKEN, Event::AudioReady(Volume::fallback()), 450)
        .unwrap();
    let fx = s.handle(TOKEN, Event::VideoPrepared, 500).unwrap();
    assert_eq!(fx[0], Effect::SetVideoVolume(live));
    for level in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(
            Volume {
                level,
                muted: false
            }
            .validate()
            .is_err()
        );
    }
}
#[test]
fn opening_release_is_ignored_then_pointer_or_supported_keyboard_release_skips() {
    for event in [
        Event::PointerReleased,
        Event::KeyReleased {
            supported_key: true,
        },
    ] {
        let mut s = playing();
        assert!(s.handle(TOKEN, event.clone(), 1000).unwrap().is_empty());
        let fx = s.handle(TOKEN, event, 1200).unwrap();
        assert_eq!(fx[0], Effect::SilenceVideo);
        assert_eq!(s.phase(), Phase::Restoring);
    }
    let mut s = playing();
    assert!(
        s.handle(
            TOKEN,
            Event::KeyReleased {
                supported_key: false
            },
            1500
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn xbox_held_at_open_needs_neutral_fresh_press_and_200ms_release() {
    let mut s = playing();
    let pad = |down| Event::Controller {
        slot: 0,
        connected: true,
        buttons: if down { 0x1000 } else { 0 },
    };
    for (now, down) in [
        (1200, true),
        (1300, false),
        (1700, false),
        (1800, true),
        (1900, false),
        (2099, false),
    ] {
        assert!(s.handle(TOKEN, pad(down), now).unwrap().is_empty());
    }
    assert_eq!(
        s.handle(TOKEN, pad(false), 2100).unwrap()[0],
        Effect::SilenceVideo
    );
}
#[test]
fn all_four_xbox_slots_accept_only_a_b_start_and_disconnect_resets() {
    for slot in 0..4 {
        for buttons in [0x1000, 0x2000, 0x10] {
            let mut s = playing();
            s.handle(
                TOKEN,
                Event::Controller {
                    slot,
                    connected: true,
                    buttons: 0,
                },
                1200,
            )
            .unwrap();
            s.handle(
                TOKEN,
                Event::Controller {
                    slot,
                    connected: true,
                    buttons,
                },
                1300,
            )
            .unwrap();
            s.handle(
                TOKEN,
                Event::Controller {
                    slot,
                    connected: false,
                    buttons: 0,
                },
                1400,
            )
            .unwrap();
            assert!(
                s.handle(
                    TOKEN,
                    Event::Controller {
                        slot,
                        connected: true,
                        buttons: 0
                    },
                    1800
                )
                .unwrap()
                .is_empty()
            );
        }
    }
    let mut s = playing();
    s.handle(
        TOKEN,
        Event::Controller {
            slot: 0,
            connected: true,
            buttons: 0,
        },
        1200,
    )
    .unwrap();
    s.handle(
        TOKEN,
        Event::Controller {
            slot: 0,
            connected: true,
            buttons: 0x8000,
        },
        1300,
    )
    .unwrap();
    assert!(
        s.handle(
            TOKEN,
            Event::Controller {
                slot: 0,
                connected: true,
                buttons: 0
            },
            1600
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        s.handle(
            TOKEN,
            Event::Controller {
                slot: 4,
                connected: true,
                buttons: 0
            },
            1700
        )
        .is_err()
    );
}
#[test]
fn shutdown_silences_before_restoring_and_never_hard_kills_restoration() {
    let mut s = playing();
    let fx = s.handle(TOKEN, Event::Finish(Reason::Skip), 1500).unwrap();
    assert_eq!(
        fx,
        vec![
            Effect::SilenceVideo,
            Effect::StopControllerObserver,
            Effect::StopAudioGuardianAndRestore
        ]
    );
    assert!(!s.may_replace());
    s.handle(TOKEN, Event::AudioRestorationPending, 1600)
        .unwrap();
    assert_eq!(s.phase(), Phase::RestorationPending);
    assert!(!s.may_replace());
    // WPF parity: the intro waits up to 2 s for the restore ACK, then
    // closes and returns focus while the guardian keeps restoring.
    assert!(s.handle(TOKEN, Event::Tick, 3499).unwrap().is_empty());
    assert_eq!(
        s.handle(TOKEN, Event::Tick, 3500).unwrap(),
        vec![Effect::DestroyVideoWindow, Effect::FocusOwnedGame(game())]
    );
    assert!(s.handle(TOKEN, Event::Tick, 900000).unwrap().is_empty());
    assert!(!s.may_replace());
    let fx = s.handle(TOKEN, Event::AudioRestored, 900001).unwrap();
    assert!(fx.is_empty());
    assert!(s.may_replace());
    assert!(
        s.handle(TOKEN, Event::AudioRestored, 900002)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn late_callbacks_and_parent_game_exit_cannot_reopen_or_refocus_foreign_game() {
    let mut s = playing();
    assert!(
        s.handle(TOKEN - 1, Event::Finish(Reason::Skip), 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(s.phase(), Phase::Playing);
    s.handle(TOKEN, Event::Finish(Reason::Skip), 1500).unwrap();
    s.handle(TOKEN, Event::Finish(Reason::GameEnded), 1600)
        .unwrap();
    assert_eq!(
        s.handle(TOKEN, Event::AudioRestored, 1700).unwrap(),
        vec![Effect::DestroyVideoWindow]
    );
}
#[test]
fn timeouts_are_enforced_even_if_ready_callback_arrives_before_timer() {
    let (mut s, _) = Session::game(TOKEN, game(), 0).unwrap();
    assert_eq!(
        s.handle(
            TOKEN,
            Event::WindowObserved(Some(window())),
            WINDOW_TIMEOUT_MS
        )
        .unwrap(),
        vec![Effect::CancelWindowWatch]
    );
    let mut s = preparing();
    assert_eq!(
        s.handle(TOKEN, Event::VideoPrepared, 350 + PLAYBACK_TIMEOUT_MS)
            .unwrap()[0],
        Effect::SilenceVideo
    );
    let mut s = playing();
    assert!(s.handle(TOKEN, Event::Tick, 499).is_err());
}
#[test]
fn preview_is_fullscreen_but_never_ducks_game_audio() {
    let (mut s, fx) = Session::preview(TOKEN, 99, 0).unwrap();
    assert_eq!(
        fx[1],
        Effect::StartAudioGuardian {
            game: None,
            duck_game: false
        }
    );
    s.handle(TOKEN, Event::Finish(Reason::WindowClosed), 100)
        .unwrap();
    assert_eq!(
        s.handle(TOKEN, Event::AudioRestored, 200).unwrap(),
        vec![Effect::DestroyVideoWindow, Effect::FocusOwnerWindow(99)]
    );
}
#[test]
fn mute_lease_mirrors_original_state_and_conditional_restore_is_idempotent() {
    let mut lease = MuteLease::new(game(), "instance".into(), false).unwrap();
    assert_eq!(lease.duck(game(), "instance").unwrap(), Some(true));
    assert!(!lease.observe(true));
    assert_eq!(
        lease.restore(game(), "instance", true).unwrap(),
        Some(false)
    );
    assert!(lease.confirm_restored(true).is_err());
    lease.confirm_restored(false).unwrap();
    assert_eq!(lease.restore(game(), "instance", false).unwrap(), None);
    let mut original_muted = MuteLease::new(game(), "instance".into(), true).unwrap();
    assert_eq!(original_muted.duck(game(), "instance").unwrap(), None);
    assert!(original_muted.observe(true));
    assert_eq!(
        original_muted.restore(game(), "instance", true).unwrap(),
        None
    );
}
#[test]
fn user_unmute_loses_ownership_without_remuting_and_reused_pid_is_refused() {
    let mut lease = MuteLease::new(game(), "instance".into(), false).unwrap();
    lease.duck(game(), "instance").unwrap();
    assert!(!lease.observe(false));
    assert_eq!(lease.duck(game(), "instance").unwrap(), None);
    assert_eq!(lease.restore(game(), "instance", false).unwrap(), None);
    assert!(
        lease
            .restore(
                GameKey {
                    birth: 457,
                    ..game()
                },
                "instance",
                true
            )
            .is_err()
    );
    assert!(lease.restore(game(), "another-instance", true).is_err());
}

#[test]
fn controller_edges_match_original_csharp_oracle_without_xinput_calls() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/startup-video-policy-regressions.json"
    ))
    .unwrap();
    assert_eq!(v["gameAccess"], false);
    assert_eq!(v["audioSettingsChanged"], false);
    for sequence in v["pads"].as_array().unwrap() {
        let mut edge = PadEdge::default();
        for row in sequence.as_array().unwrap() {
            let result = edge.step(
                row["connected"].as_bool().unwrap(),
                row["down"].as_bool().unwrap(),
                row["ms"].as_u64().unwrap(),
            );
            assert_eq!(result, row["skip"].as_bool().unwrap(), "{row}");
        }
    }
}
#[test]
fn mute_ownership_matches_original_csharp_pure_lease_without_audio_calls() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../assets/startup-video-policy-regressions.json"
    ))
    .unwrap();
    for row in v["mute"].as_array().unwrap() {
        let original = row["original"].as_bool().unwrap();
        let mut current = original;
        let mut changes = 0;
        let mut lease = MuteLease::new(game(), "test-instance".into(), original).unwrap();
        if let Some(value) = lease.duck(game(), "test-instance").unwrap() {
            current = value;
            changes += 1;
        }
        assert_eq!(
            lease.observe(current),
            row["observedBefore"].as_bool().unwrap()
        );
        if row["externalUnmute"].as_bool().unwrap() {
            current = false;
        }
        assert_eq!(
            lease.observe(current),
            row["observedAfter"].as_bool().unwrap()
        );
        if let Some(value) = lease.restore(game(), "test-instance", current).unwrap() {
            current = value;
            changes += 1;
            lease.confirm_restored(current).unwrap();
        }
        assert_eq!(
            lease.restore(game(), "test-instance", current).unwrap(),
            None
        );
        assert_eq!(current, row["finalMuted"].as_bool().unwrap());
        assert_eq!(changes, row["changes"].as_u64().unwrap());
    }
}
#[test]
fn coordinator_blocks_replacement_until_audio_ack_and_never_reuses_generation() {
    let mut owner = Coordinator::default();
    let (first, _) = owner.start_preview(99, 0).unwrap();
    assert_eq!(owner.phase(), Some(Phase::Preparing));
    assert!(owner.start_game(game(), 100).is_err());
    owner
        .handle(first, Event::Finish(Reason::Skip), 100)
        .unwrap();
    owner
        .handle(first, Event::AudioRestorationPending, 200)
        .unwrap();
    assert!(owner.start_preview(99, 300).is_err());
    owner.handle(first, Event::AudioRestored, 400).unwrap();
    let (second, _) = owner.start_game(game(), 500).unwrap();
    assert!(second > first);
    assert!(
        owner
            .handle(first, Event::Finish(Reason::Error), 0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(owner.phase(), Some(Phase::WaitingWindow));
}
#[test]
fn a_prompt_restore_ack_releases_the_window_once_without_waiting() {
    let mut s = playing();
    s.handle(TOKEN, Event::Finish(Reason::Skip), 1500).unwrap();
    assert_eq!(
        s.handle(TOKEN, Event::AudioRestored, 1700).unwrap(),
        vec![Effect::DestroyVideoWindow, Effect::FocusOwnedGame(game())]
    );
    assert!(s.handle(TOKEN, Event::Tick, 9000).unwrap().is_empty());
}
