//! Actual Service/FrozenLaunch methods with self-owned metadata only. Availability
//! is injected by the same production freeze function; no game/freeze APIs run.
use super::*;
use std::cell::Cell;
fn root() -> PathBuf {
    let r = std::env::current_dir()
        .unwrap()
        .join("artifacts")
        .join(format!(
            "qa084-profile-launch-self-{}",
            backend::unique_id()
        ));
    fs::create_dir_all(r.join("Data")).unwrap();
    r
}
fn profile() -> Profile {
    Profile {
        id: "a".repeat(48),
        team_id: 7,
        expected_team_name: "Fixture".into(),
        donor_arena_id: 17,
        target_arena_id: 999,
        arena_name: "Court".into(),
        arena_file: "arena_custom".into(),
        floor_file: "floor_custom".into(),
        enabled: true,
    }
}
fn freeze(s: &Service, r: &Path) -> FrozenLaunch {
    s.freeze_launch_with_available(
        r,
        Path::new(r"C:\QA\Game"),
        Path::new(r"C:\QA\Mods"),
        &|| Ok(()),
    )
    .unwrap()
}
#[test]
fn default_and_disabled_profiles_never_require_native_availability_or_arm_on_read() {
    let r = root();
    let s = Service::default();
    let calls = Cell::new(0);
    let available = || {
        calls.set(calls.get() + 1);
        anyhow::bail!("should not be called")
    };
    let empty = s
        .freeze_launch_with_available(
            &r,
            Path::new(r"C:\QA\Game"),
            Path::new(r"C:\QA\Mods"),
            &available,
        )
        .unwrap();
    assert!(empty.profiles.is_empty());
    assert_eq!(calls.get(), 0);
    assert!(s.launch.lock().unwrap().is_none());
    let mut p = profile();
    p.enabled = false;
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![p],
        },
    )
    .unwrap();
    let plan = s
        .freeze_launch_with_available(
            &r,
            Path::new(r"C:\QA\Game"),
            Path::new(r"C:\QA\Mods"),
            &available,
        )
        .unwrap();
    assert!(plan.profiles.is_empty());
    assert_eq!(calls.get(), 0);
    assert!(s.launch.lock().unwrap().is_none());
}
#[test]
fn frozen_enabled_preferences_and_native_paths_survive_later_metadata_edits() {
    let r = root();
    let s = Service::default();
    let p = profile();
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![p.clone()],
        },
    )
    .unwrap();
    let calls = Cell::new(0);
    let plan = s
        .freeze_launch_with_available(
            &r,
            Path::new(r"C:\QA\Game"),
            Path::new(r"C:\QA\Mods"),
            &|| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(plan.profiles, vec![p.clone()]);
    assert!(plan.requested_ms > 0);
    s.arm(plan.clone()).unwrap();
    s.request(&r, &json!({"action":"disable","id":p.id}), &|_| {
        panic!("disabling must not prompt")
    })
    .unwrap();
    let current = s.launch.lock().unwrap().as_ref().unwrap().clone();
    assert_eq!(current.profiles, plan.profiles);
    assert_eq!(current.game_root, PathBuf::from(r"C:\QA\Game"));
    assert_eq!(current.mods_root, PathBuf::from(r"C:\QA\Mods"));
    assert!(s.enabled(&r).unwrap().is_empty());
    s.request(&r, &json!({"action":"remove","id":p.id}), &|_| panic!())
        .unwrap();
    assert!(read(&r).unwrap().profiles.is_empty());
    assert_eq!(s.launch.lock().unwrap().as_ref().unwrap().profiles, vec![p]);
}
#[test]
fn no_replay_claim_stays_spent_and_stale_disarmed_or_replaced_plan_cannot_claim() {
    let r = root();
    let s = Service::default();
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![profile()],
        },
    )
    .unwrap();
    let plan = freeze(&s, &r);
    s.arm(plan.clone()).unwrap();
    assert!(
        !s.claim("stale-launch", "owned-session-profile".into())
            .unwrap()
    );
    assert!(s.claim(&plan.id, "owned-session-profile".into()).unwrap());
    // An error/lost response has no release API; repeated polls remain refused.
    assert!(!s.claim(&plan.id, "owned-session-profile".into()).unwrap());
    assert!(!s.claim(&plan.id, "owned-session-profile".into()).unwrap());
    s.disarm().unwrap();
    assert!(!s.is_armed(&plan.id).unwrap());
    assert!(!s.claim(&plan.id, "different".into()).unwrap());
    let next = freeze(&s, &r);
    assert_ne!(next.id, plan.id);
    s.arm(next.clone()).unwrap();
    assert!(!s.claim(&plan.id, "fresh".into()).unwrap());
    assert!(
        s.claim(&next.id, "new-owned-session-profile".into())
            .unwrap()
    );
}
#[test]
fn failed_availability_or_invalid_folders_never_replace_armed_frozen_launch() {
    let r = root();
    let s = Service::default();
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![profile()],
        },
    )
    .unwrap();
    let plan = freeze(&s, &r);
    s.arm(plan.clone()).unwrap();
    assert!(
        s.freeze_launch_with_available(
            &r,
            Path::new(r"C:\QA\Game"),
            Path::new(r"C:\QA\Mods"),
            &|| anyhow::bail!("unsupported state object API")
        )
        .is_err()
    );
    assert!(
        s.freeze_launch_with_available(
            &r,
            Path::new("relative"),
            Path::new(r"C:\QA\Mods"),
            &|| Ok(())
        )
        .is_err()
    );
    assert!(s.is_armed(&plan.id).unwrap());
}
#[test]
fn enabling_requires_explicit_native_consent_and_remains_future_launch_only() {
    let r = root();
    let s = Service::default();
    let mut p = profile();
    p.enabled = false;
    save(
        &r,
        &Store {
            schema: SCHEMA.into(),
            profiles: vec![p.clone()],
        },
    )
    .unwrap();
    let prompts = Cell::new(0);
    assert!(
        s.request(&r, &json!({"action":"enable","id":p.id}), &|_| {
            prompts.set(prompts.get() + 1);
            false
        })
        .is_err()
    );
    assert!(!read(&r).unwrap().profiles[0].enabled);
    assert!(s.launch.lock().unwrap().is_none());
    s.request(&r, &json!({"action":"enable","id":p.id}), &|text| {
        prompts.set(prompts.get() + 1);
        assert!(text.contains("future Play"));
        assert!(text.contains("close"));
        true
    })
    .unwrap();
    assert_eq!(prompts.get(), 2);
    assert!(read(&r).unwrap().profiles[0].enabled);
    assert!(s.launch.lock().unwrap().is_none());
}
