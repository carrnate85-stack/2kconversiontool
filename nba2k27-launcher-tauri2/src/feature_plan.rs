//! Immutable, bounded next-launch choices. A settings screen is not an
//! authority to write to a game; the provider revalidates this frozen plan.
use crate::{backend::Settings, library::Library};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Camera {
    pub id: u32,
    pub name: String,
    pub height: i32,
    pub zoom: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Plan {
    pub cameras: Vec<Camera>,
    pub lock_camera: bool,
    pub scaler_percent: Option<u32>,
    /// Absent in plans frozen before 0.47: no character groups.
    #[serde(default)]
    pub scaler_groups: crate::scaler_npc_v2::Groups,
    pub wnba_freestyle: bool,
    /// Experimental mixed NBA+WNBA Learn2K roster (wnba_roster_v2.rs). Absent
    /// in older plans: off. Supersedes the withdrawn `wnba_freestyle` loader.
    #[serde(default)]
    pub wnba_mixed: Option<crate::wnba_roster_v2::Variant>,
    /// DEV-only Freestyle toggle. An independent exact-session category token
    /// and the current native priority source proof authorize its four DWORDs.
    /// It never builds/publishes a roster or swaps a stock loader callback.
    #[serde(default)]
    pub wnba_priority_categories: bool,
    /// DEV-only Blacktop toggle; a frozen next-Play choice authorizes the
    /// two-entry source and independent Blacktop category permission.
    #[serde(default)]
    pub wnba_blacktop_categories: bool,
    #[serde(default)]
    pub presentation: crate::presentation_v2_runtime::Options,
    #[serde(default)]
    pub jukebox: crate::jukebox_launch::Plan,
    /// WPF "Use playlist in Freestyle". Absent in older plans: off. Only
    /// applied with a Jukebox playlist on the V2 outfit proxy.
    #[serde(default)]
    pub freestyle_playlist: bool,
    /// WPF applied the saved Blacktop choice without F8. Absent in older
    /// plans: no automatic arena/scoreboard request.
    #[serde(default)]
    pub arena: Option<String>,
    #[serde(default)]
    pub scoreboard: Option<String>,
}
impl Plan {
    pub fn freeze(settings: &Settings, library: &Library) -> Result<Self> {
        let mut plan = Self {
            lock_camera: settings.cameras_enabled && settings.lock_camera,
            // The experimental mixed-roster route replaces the withdrawn
            // stock-loader swap; a stale/unknown variant requests nothing.
            // V2 streams the roster from its archive, bypassing our provider.
            // Preserve preferences, but do not arm a known-ineffective route
            // (or capture a roster key) merely because an old test left it on.
            wnba_freestyle: false,
            wnba_mixed: None,
            wnba_priority_categories: cfg!(debug_assertions) && settings.unlock_wnba_freestyle,
            wnba_blacktop_categories: cfg!(debug_assertions) && settings.unlock_wnba,
            freestyle_playlist: settings.use_playlist_in_freestyle,
            presentation: crate::presentation_v2_runtime::Options {
                remove_drone: settings.remove_drone,
                remove_dj: settings.remove_dj,
                remove_mc: settings.remove_mc,
                remove_crowd: settings.remove_crowd,
            },
            arena: saved_arena(&settings.arena_id, &settings.arena),
            scoreboard: saved_scoreboard(&settings.scoreboard_id, &settings.scoreboard),
            ..Self::default()
        };
        if settings.cameras_enabled {
            for row in library.cameras.iter().filter(|row| row.enabled) {
                let steps = crate::camera_policy::Steps::from_stored(row.height, row.zoom)?;
                plan.cameras.push(Camera {
                    id: row.native_id()?,
                    name: row.name.clone(),
                    height: steps.height,
                    zoom: steps.zoom,
                });
            }
            ensure!(
                !plan.cameras.is_empty(),
                "Enable at least one camera or turn Cameras off"
            );
        }
        // Preserve old drafts on disk, but never silently turn a 120% legacy
        // preference into a supported 100% game operation.
        if settings.scaler != 1.0 {
            let percent = settings.scaler * 100.;
            ensure!(
                percent.is_finite()
                    && (95. ..=100.).contains(&percent)
                    && (percent - percent.round()).abs() < 1e-9,
                "Version 2 scaler supports whole percentages 95–100 only; reset the saved scale first"
            );
            plan.scaler_percent = Some(percent.round() as u32);
            plan.scaler_groups = crate::scaler_npc_v2::Groups {
                coaches: settings.scaler_coaches,
                cheerleaders: settings.scaler_cheerleaders,
                arena_personnel: settings.scaler_arena_personnel,
            };
        }
        plan.validate()?;
        Ok(plan)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !(self.wnba_priority_categories || self.wnba_blacktop_categories)
                || (cfg!(debug_assertions) && !self.wnba_freestyle && self.wnba_mixed.is_none()),
            "Priority WNBA categories require the isolated dev mode; legacy roster routes must stay off"
        );
        ensure!(
            self.cameras.len() <= 7,
            "Camera launch plan exceeds the curated catalog"
        );
        let mut ids = BTreeSet::new();
        let mut names = BTreeMap::new();
        for row in &self.cameras {
            ensure!(
                crate::camera_policy::CURATED
                    .iter()
                    .any(|(id, _)| *id == row.id)
                    && ids.insert(row.id),
                "Unknown or duplicate camera type in launch plan"
            );
            crate::camera_policy::Steps::from_stored(row.height as f64, row.zoom as f64)?;
            names.insert(row.id, row.name.clone());
        }
        crate::camera_names::names(&names)?;
        ensure!(
            !self.lock_camera || !self.cameras.is_empty(),
            "Camera lock needs an active camera plan"
        );
        ensure!(
            self.scaler_percent.is_none_or(|p| (95..=100).contains(&p)),
            "Unsupported scaler percentage"
        );
        self.jukebox.validate()?;
        if let Some(id) = &self.arena {
            ensure!(
                id != "default" && crate::blacktop_arena::choice(id).is_ok(),
                "Unknown saved arena in launch plan"
            );
        }
        if let Some(id) = &self.scoreboard {
            ensure!(
                id != "default" && crate::scoreboard_profile::choice(id).is_ok(),
                "Unknown saved scoreboard in launch plan"
            );
        }
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.camera_scaler_freestyle_enabled()
            || self.presentation.enabled()
            || self.jukebox.enabled()
    }
    pub fn camera_scaler_freestyle_enabled(&self) -> bool {
        !self.cameras.is_empty()
            || self.scaler_percent.is_some()
            || self.wnba_freestyle
            || self.wnba_mixed.is_some()
            || self.wnba_priority_categories
            || self.wnba_blacktop_categories
    }
    pub fn camera_names(&self) -> BTreeMap<u32, String> {
        self.cameras
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect()
    }
    /// The stock-first loader must ask the provider first for the renamed
    /// English catalog. Never accept override paths from the UI or settings.
    pub fn stock_overrides(&self) -> Vec<String> {
        let mut overrides = if self.cameras.is_empty() {
            Vec::new()
        } else {
            vec!["loc/english.iff".into()]
        };
        overrides.extend(self.jukebox.stock_overrides());
        // Fourth forced slot: until the locally built mixed roster is
        // published the provider reports it missing and the proxy reads stock.
        if self.wnba_mixed.is_some() {
            overrides.push(crate::wnba_roster_v2::PATH.into());
        }
        overrides
    }
}

/// Settings keep the visible name; the id disambiguates repeated names. A
/// stale, unknown, disabled or ambiguous choice is not guessed: no request.
fn saved_arena(id: &str, label: &str) -> Option<String> {
    let rows = crate::blacktop_arena::choices();
    let id = if rows.iter().any(|c| c.id == id && c.label == label) {
        id
    } else {
        let mut named = rows.iter().filter(|c| c.label == label);
        let only = named.next()?;
        if named.next().is_some() {
            return None;
        }
        only.id.as_str()
    };
    let choice = crate::blacktop_arena::choice(id).ok()?;
    (choice.id != "default").then(|| choice.id.clone())
}
fn saved_scoreboard(id: &str, label: &str) -> Option<String> {
    let rows = crate::scoreboard_profile::catalog().ok()?;
    let id = if rows.iter().any(|c| c.id == id && c.label == label) {
        id.to_string()
    } else {
        let named: Vec<_> = rows.iter().filter(|c| c.label == label).collect();
        if named.len() != 1 {
            return None;
        }
        named[0].id.clone()
    };
    // Same migration as WPF's saved selection.json.
    let choice = crate::scoreboard_profile::migrate_saved(&id).ok()?;
    (choice.id != "default").then_some(choice.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn priority_category_dev_mode_is_separate_from_all_roster_loading() {
        let mut plan = Plan {
            wnba_priority_categories: cfg!(debug_assertions),
            ..Plan::default()
        };
        plan.validate().unwrap();
        assert!(plan.stock_overrides().is_empty());
        assert!(!plan.wnba_freestyle && plan.wnba_mixed.is_none());
        if cfg!(debug_assertions) {
            plan.wnba_freestyle = true;
            assert!(plan.validate().is_err());
            plan.wnba_freestyle = false;
            plan.wnba_mixed = Some(crate::wnba_roster_v2::Variant::WpfEra);
            assert!(plan.validate().is_err());
        }
        let old: Plan = serde_json::from_value(serde_json::json!({
            "cameras":[],"lockCamera":false,"scalerPercent":null,"wnbaFreestyle":false
        }))
        .unwrap();
        assert!(!old.wnba_priority_categories && !old.enabled());
    }
    #[test]
    fn default_has_no_new_automatic_game_writes() {
        let plan = Plan::freeze(&Settings::default(), &Library::default()).unwrap();
        assert!(!plan.enabled());
        assert!(!plan.wnba_priority_categories);
        assert!(!plan.lock_camera);
        assert!(plan.stock_overrides().is_empty());
        assert!(!plan.jukebox.enabled());
        assert_eq!((plan.arena, plan.scoreboard), (None, None));
        assert_eq!(plan.wnba_mixed, None);
    }
    #[test]
    fn legacy_wnba_delivery_stays_off_when_freestyle_toggle_requests_priority_categories() {
        let mut settings = Settings::default();
        // The canonical toggle requests only the proven priority category path
        // in dev; it never requests a roster build or withdrawn loader swap.
        settings.unlock_wnba_freestyle = true;
        let plan = Plan::freeze(&settings, &Library::default()).unwrap();
        assert_eq!(plan.wnba_mixed, None);
        assert!(plan.stock_overrides().is_empty());
        settings.wnba_freestyle_mixed = true;
        let plan = Plan::freeze(&settings, &Library::default()).unwrap();
        assert_eq!(plan.wnba_mixed, None);
        assert!(!plan.wnba_freestyle, "the withdrawn loader route stays off");
        assert_eq!(plan.enabled(), cfg!(debug_assertions));
        assert_eq!(
            plan.camera_scaler_freestyle_enabled(),
            cfg!(debug_assertions)
        );
        assert!(plan.stock_overrides().is_empty());
        settings.wnba_freestyle_variant = "national".into();
        assert_eq!(
            Plan::freeze(&settings, &Library::default())
                .unwrap()
                .wnba_mixed,
            None
        );
        settings.wnba_freestyle_variant = "unknown".into();
        assert_eq!(
            Plan::freeze(&settings, &Library::default())
                .unwrap()
                .wnba_mixed,
            None
        );
        let old: Plan = serde_json::from_value(serde_json::json!({"cameras":[],"lockCamera":false,"scalerPercent":null,"wnbaFreestyle":false})).unwrap();
        assert_eq!(old.wnba_mixed, None);
        assert!(!old.wnba_priority_categories);
    }
    #[test]
    fn freestyle_toggle_is_frozen_for_next_play_and_off_requests_no_category_work() {
        let mut settings = Settings::default();
        let off = Plan::freeze(&settings, &Library::default()).unwrap();
        assert!(!off.wnba_priority_categories && !off.enabled());
        settings.unlock_wnba_freestyle = true;
        let on = Plan::freeze(&settings, &Library::default()).unwrap();
        settings.unlock_wnba_freestyle = false;
        assert_eq!(on.wnba_priority_categories, cfg!(debug_assertions));
        assert!(!on.wnba_freestyle && on.wnba_mixed.is_none());
        assert!(on.stock_overrides().is_empty());
        let next = Plan::freeze(&settings, &Library::default()).unwrap();
        assert!(!next.wnba_priority_categories && !next.enabled());
    }
    #[test]
    fn blacktop_and_freestyle_toggles_freeze_independently_for_next_play() {
        for (freestyle, blacktop) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut settings = Settings::default();
            settings.unlock_wnba_freestyle = freestyle;
            settings.unlock_wnba = blacktop;
            let frozen = Plan::freeze(&settings, &Library::default()).unwrap();
            settings.unlock_wnba_freestyle = false;
            settings.unlock_wnba = false;
            assert_eq!(
                frozen.wnba_priority_categories,
                cfg!(debug_assertions) && freestyle
            );
            assert_eq!(
                frozen.wnba_blacktop_categories,
                cfg!(debug_assertions) && blacktop
            );
            assert_eq!(
                frozen.enabled(),
                cfg!(debug_assertions) && (freestyle || blacktop)
            );
            assert!(!frozen.wnba_freestyle && frozen.wnba_mixed.is_none());
            assert!(frozen.stock_overrides().is_empty());
            let next = Plan::freeze(&settings, &Library::default()).unwrap();
            assert!(!next.wnba_priority_categories && !next.wnba_blacktop_categories);
        }
        let old:Plan=serde_json::from_value(serde_json::json!({"cameras":[],"lockCamera":false,"scalerPercent":null,"wnbaFreestyle":false})).unwrap();
        assert!(!old.wnba_blacktop_categories);
    }
    #[test]
    fn saved_arena_and_scoreboard_are_frozen_by_exact_catalog_id() {
        let mut settings = Settings::default();
        settings.arena = "Aaugym".into();
        settings.scoreboard = "Broadcast".into();
        let plan = Plan::freeze(&settings, &Library::default()).unwrap();
        settings.arena = "Default · Day / Night".into();
        assert_eq!(plan.arena.as_deref(), Some("arena_aaugym_int"));
        assert_eq!(plan.scoreboard.as_deref(), Some("broadcast"));
        // Automatic Blacktop requests are not memory features and need no
        // Version 2 camera/scaler/presentation gate.
        assert!(!plan.enabled());
        assert!(plan.stock_overrides().is_empty());
        // A repeated name is resolved only by its saved id, never guessed.
        settings.arena = "Historic Warriors / Oracle Arena".into();
        assert_eq!(
            Plan::freeze(&settings, &Library::default()).unwrap().arena,
            None
        );
        settings.arena_id = "arena_637_int".into();
        assert_eq!(
            Plan::freeze(&settings, &Library::default())
                .unwrap()
                .arena
                .as_deref(),
            Some("arena_637_int")
        );
        // A stale id falls back to the visible name.
        settings.arena_id = "arena_637_int".into();
        settings.arena = "Aaugym".into();
        assert_eq!(
            Plan::freeze(&settings, &Library::default())
                .unwrap()
                .arena
                .as_deref(),
            Some("arena_aaugym_int")
        );
        // Defaults, unknown names and blocked scoreboards make no request;
        // WPF's saved MyCareer story migrates to Broadcast.
        for (arena, scoreboard, expected) in [
            ("Default · Day / Night", "Default", None),
            ("Not an arena", "Not a scoreboard", None),
            ("Default · Day / Night", "Flashback NBA open run", None),
            ("Default · Day / Night", "MyCareer story", Some("broadcast")),
        ] {
            settings.arena = arena.into();
            settings.arena_id.clear();
            settings.scoreboard = scoreboard.into();
            let plan = Plan::freeze(&settings, &Library::default()).unwrap();
            assert_eq!(plan.arena, None);
            assert_eq!(plan.scoreboard.as_deref(), expected);
        }
    }
    #[test]
    fn old_plans_make_no_automatic_blacktop_request_and_tampered_ids_are_rejected() {
        let old: Plan = serde_json::from_value(serde_json::json!({"cameras":[],"lockCamera":false,
            "scalerPercent":null,"wnbaFreestyle":false}))
        .unwrap();
        assert_eq!((old.arena, old.scoreboard), (None, None));
        for (key, id) in [
            ("arena", "default"),
            ("arena", "../arena_000_int"),
            ("scoreboard", "default"),
            ("scoreboard", "mycareer-story"),
            ("scoreboard", "unknown-board"),
        ] {
            let mut plan = Plan::default();
            let value = Some(id.to_string());
            if key == "arena" {
                plan.arena = value;
            } else {
                plan.scoreboard = value;
            }
            assert!(plan.validate().is_err(), "{key} {id}");
        }
    }
    #[test]
    fn jukebox_is_separately_sealed_without_enabling_memory_features_or_legacy_freestyle() {
        let mut plan = Plan::default();
        plan.jukebox = serde_json::from_value(
            serde_json::json!({"schema":"nba2k27-rust-jukebox-launch-v1","selection":{
            "revision":"a".repeat(64),"stockSha256":"b".repeat(64),"playlistId":"c".repeat(48),
            "trackKeys":["1:1:1"],"selectedSha256":"d".repeat(64)}}),
        )
        .unwrap();
        plan.validate().unwrap();
        assert!(plan.enabled());
        assert!(!plan.camera_scaler_freestyle_enabled());
        assert!(!plan.wnba_freestyle);
        assert_eq!(plan.stock_overrides(), ["music.iff"]);
        let old: Plan = serde_json::from_value(serde_json::json!({"cameras":[],"lockCamera":false,"scalerPercent":null,"wnbaFreestyle":false})).unwrap();
        assert!(!old.jukebox.enabled());
        assert!(!old.enabled());
        assert!(old.stock_overrides().is_empty());
    }
    #[test]
    fn presentation_choices_are_frozen_and_old_plans_default_to_no_removals() {
        let mut settings = Settings::default();
        settings.remove_drone = true;
        settings.remove_crowd = true;
        let plan = Plan::freeze(&settings, &Library::default()).unwrap();
        settings.remove_drone = false;
        settings.remove_crowd = false;
        assert!(plan.presentation.remove_drone && plan.presentation.remove_crowd);
        assert!(!plan.presentation.remove_dj && !plan.presentation.remove_mc);
        assert!(plan.enabled());
        assert!(!plan.camera_scaler_freestyle_enabled());
        assert!(plan.stock_overrides().is_empty());
        let old: Plan = serde_json::from_value(serde_json::json!({"cameras":[],"lockCamera":false,
            "scalerPercent":null,"wnbaFreestyle":false}))
        .unwrap();
        assert!(!old.presentation.enabled() && !old.enabled());
        let mut tampered = serde_json::to_value(plan).unwrap();
        tampered["presentation"]["address"] = 123.into();
        assert!(serde_json::from_value::<Plan>(tampered).is_err());
    }
    #[test]
    fn camera_type_order_and_values_are_frozen_not_reinferred() {
        let mut settings = Settings::default();
        settings.cameras_enabled = true;
        let mut library = Library::default();
        library.cameras.reverse();
        let plan = Plan::freeze(&settings, &library).unwrap();
        library.cameras.clear();
        settings.cameras_enabled = false;
        assert_eq!(plan.cameras[0].id, 55);
        assert_eq!(plan.cameras.len(), 7);
        assert_eq!(plan.stock_overrides(), ["loc/english.iff"]);
        plan.validate().unwrap();
        let mut duplicate = plan.clone();
        duplicate.cameras[1].id = 55;
        assert!(duplicate.validate().is_err());
    }
    #[test]
    fn legacy_fractional_camera_and_out_of_range_scale_are_not_coerced() {
        let mut settings = Settings::default();
        settings.cameras_enabled = true;
        let mut library = Library::default();
        library.cameras[0].zoom = 1.1;
        assert!(Plan::freeze(&settings, &library).is_err());
        settings.cameras_enabled = false;
        for factor in [0.8, 0.949, 0.955, 1.01, 1.2] {
            settings.scaler = factor;
            assert!(Plan::freeze(&settings, &library).is_err());
        }
        for percent in 95..100 {
            settings.scaler = percent as f64 / 100.;
            assert_eq!(
                Plan::freeze(&settings, &library).unwrap().scaler_percent,
                Some(percent)
            );
        }
    }
    #[test]
    fn worker_handshake_rejects_unbounded_or_unknown_choices() {
        let mut plan = Plan::default();
        plan.lock_camera = true;
        assert!(plan.validate().is_err());
        plan.lock_camera = false;
        plan.scaler_percent = Some(101);
        assert!(plan.validate().is_err());
        assert!(
            serde_json::from_value::<Plan>(serde_json::json!({"cameras":[], "lockCamera":false,
            "scalerPercent":null,"wnbaFreestyle":false,"address":123}))
            .is_err()
        );
    }
}
