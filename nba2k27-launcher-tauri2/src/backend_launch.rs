//! Settings admission and external-media freeze are part of the same launch
//! snapshot. The UI cannot race a settings save into a half-old launch plan.
use super::*;
impl Backend {
    pub(crate) fn launch_request(
        &self,
        channel: &str,
        args: Value,
    ) -> Result<(Value, Option<crate::startup_video_media::VideoLease>)> {
        self.launch_request_inner(channel, args, None)
    }
    pub(crate) fn launch_request_with_intro(
        &self,
        channel: &str,
        args: Value,
        callback: &mut dyn FnMut(crate::startup_video_media::VideoLease) -> Result<()>,
    ) -> Result<(Value, Option<crate::startup_video_media::VideoLease>)> {
        self.launch_request_inner(channel, args, Some(callback))
    }
    fn launch_request_inner(
        &self,
        channel: &str,
        args: Value,
        callback: Option<&mut dyn FnMut(crate::startup_video_media::VideoLease) -> Result<()>>,
    ) -> Result<(Value, Option<crate::startup_video_media::VideoLease>)> {
        let launch_clock = std::time::Instant::now();
        self.ensure_open_for_launch()?;
        ensure!(
            matches!(channel, "native-launch-modded" | "native-launch-steam-only"),
            "Not a launch request"
        );
        // No decoder child can appear between idle validation and Steam
        // dispatch. This narrow gate does not exempt any helper from ownership.
        let _media_launch = self.jukebox.hold_launch_admission()?;
        let settings = self
            .settings
            .lock()
            .map_err(|_| anyhow::anyhow!("Settings unavailable"))?;
        self.settings_store
            .lock()
            .map_err(|_| anyhow::anyhow!("Settings storage unavailable"))?
            .verify_current()?;
        let modded = channel == "native-launch-modded";
        let debug_menus = modded
            && args
                .get("debugMenus")
                .and_then(Value::as_bool)
                .context("Session-only debug-menu choice is missing")?;
        ensure!(
            !debug_menus || crate::feature_plan::dev_debug_menus(),
            "Debug menus are available only in the isolated dev build"
        );
        let expected = if modded {
            json!({"acknowledgement":"native-modded-qa-explicit-blacktop","gameRoot":settings.game_root,"modsRoot":settings.mods_root,"debugMenus":debug_menus})
        } else {
            json!({"acknowledgement":"steam-only-no-features","gameRoot":settings.game_root})
        };
        ensure!(
            args == expected,
            "Confirm launch scope for the current saved folders before launching"
        );
        // Hash/metadata validation and both retained media pins precede Steam,
        // so an invalid selection can never start one file and play another.
        // WPF parity: optional media never blocks Play. A missing/invalid MP4
        // only skips the intro for this launch and says why.
        let (mut intro, mut intro_skipped) = if settings.startup_video {
            match self.selected_intro(&settings.video) {
                Ok(lease) => (Some(lease), None),
                Err(error) => {
                    self.ensure_open_for_launch()?;
                    (None, Some(format!("{error:#}")))
                }
            }
        } else {
            (None, None)
        };
        let media_validation_ms = launch_clock.elapsed().as_millis();
        // Start the retained selection before native preflight/Steam dispatch.
        // Video errors remain optional; launch failures are cancelled by caller.
        if let Some(callback) = callback {
            if let Some(media) = intro.take() {
                if let Err(error) = callback(media) {
                    intro_skipped = Some(format!("{error:#}"));
                }
            }
        }
        let intro_ready_ms = launch_clock.elapsed().as_millis();
        let arena_profiles = if modded {
            Some(self.arena_profiles.freeze_launch(
                &self.root,
                Path::new(&settings.game_root),
                Path::new(&settings.mods_root),
            )?)
        } else {
            None
        };
        let result = if modded {
            let mut features = {
                let library = self
                    .library
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Library unavailable"))?;
                crate::feature_plan::Plan::freeze(&settings, &library)?
            };
            features.debug_menus = debug_menus;
            features.jukebox = self.jukebox.freeze_launch_plan()?;
            features.validate()?;
            self.ensure_open_for_launch()?;
            self.session.launch_modded(
                Path::new(&settings.game_root),
                Path::new(&settings.mods_root),
                features,
            )?
        } else {
            self.ensure_open_for_launch()?;
            self.session.launch(Path::new(&settings.game_root))?
        };
        let accepted = matches!(
            result["phase"].as_str(),
            Some("waiting-for-game" | "game-running")
        );
        if accepted {
            if let Some(profiles) = arena_profiles {
                self.arena_profiles.arm(profiles)?;
            } else {
                self.arena_profiles.disarm()?;
            }
        }
        let mut result = result;
        if let (Some(reason), Some(object)) = (intro_skipped, result.as_object_mut()) {
            object.insert(
                "startupVideoSkipped".into(),
                json!(format!("Startup video skipped: {reason}")),
            );
        }
        let _ = crate::backend::atomic_json(
            &self.root.join("Logs/launch-timing-last.json"),
            &json!({"version":VERSION,"mediaValidationMs":media_validation_ms,"introReadyMs":intro_ready_ms,"dispatchReturnedMs":launch_clock.elapsed().as_millis(),"accepted":accepted}),
        );
        Ok((result, if accepted { intro } else { None }))
    }
    pub(crate) fn selected_intro(
        &self,
        id: &str,
    ) -> Result<crate::startup_video_media::VideoLease> {
        let selected = self.with_videos(|store| store.resolve(id, &self.closing))?;
        let item = &selected.item;
        crate::startup_video_media::VideoLease::from_trusted_catalog(
            &self.root,
            crate::startup_video_media::CatalogEntry {
                id: item.id.clone(),
                name: item.name.clone(),
                relative_path: PathBuf::from(&item.relative_path),
                sha256: item.sha256.clone(),
                bytes: item.bytes,
            },
            || self.closing.load(std::sync::atomic::Ordering::Acquire),
        )
    }
}
