//! Prepare exact-owned labels before Steam can load localization. The same
//! generation is frozen for the provider after its normal activation gate.
use anyhow::{Result, ensure};
use std::path::Path;
pub(crate) fn prepare(
    assets: &crate::provider_assets::Service,
    portable: &Path,
    game: &Path,
    mods: &Path,
    session: &str,
    plan: &crate::feature_plan::Plan,
    lease: &mut Option<crate::camera_name_files::Store>,
    check: impl Fn() -> Result<()>,
) -> Result<()> {
    prepare_with(assets, portable, mods, session, plan, lease, &check, || {
        stock_source(game, &check)
    })
}
/// Read-only stock English decode. It opens no receipt and changes nothing, so
/// the launch may run it on a parallel lane before the owned steps below.
pub(crate) fn stock_source(game: &Path, check: &dyn Fn() -> Result<()>) -> Result<Vec<u8>> {
    let resource = crate::stock_selectors::Resource::camera_localization(game, check)?;
    let packed = resource.packed(game, check)?;
    crate::stock_selectors::Decoder::load(game)?.decode(&packed, check)
}
/// The owned half of `prepare` using a source decoded earlier by
/// `stock_source`. Receipt, activation and publication keep their order and
/// their fresh checks; a failed decode is reported here, before any lease.
pub(crate) fn prepare_from_source(
    assets: &crate::provider_assets::Service,
    portable: &Path,
    mods: &Path,
    session: &str,
    plan: &crate::feature_plan::Plan,
    lease: &mut Option<crate::camera_name_files::Store>,
    check: impl Fn() -> Result<()>,
    source: Result<Vec<u8>>,
) -> Result<()> {
    prepare_with(assets, portable, mods, session, plan, lease, check, || {
        source
    })
}
fn prepare_with(
    assets: &crate::provider_assets::Service,
    portable: &Path,
    mods: &Path,
    session: &str,
    plan: &crate::feature_plan::Plan,
    lease: &mut Option<crate::camera_name_files::Store>,
    check: impl Fn() -> Result<()>,
    source: impl FnOnce() -> Result<Vec<u8>>,
) -> Result<()> {
    ensure!(lease.is_none(), "Camera labels are already owned");
    if plan.cameras.is_empty() {
        return Ok(());
    }
    plan.validate()?;
    check()?;
    let source = source()?;
    let names = plan.camera_names();
    let prepared = crate::camera_names::build(&source, &names)?;
    check()?;
    // Retain the receipt owner before activation so all later errors use
    // exact-owned cleanup. Never replace/adopt an existing English mod.
    *lease = Some(crate::camera_name_files::Store::create(
        portable, mods, session, &prepared, &check,
    )?);
    lease.as_mut().unwrap().activate(&prepared, &check)?;
    assets.publish_owned_camera_names(prepared, &names, lease.as_ref().unwrap())?;
    check()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        fs,
        sync::{Arc, atomic::AtomicBool},
    };
    fn fixture() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rust-camera-localization-{}",
            crate::backend::unique_id()
        ));
        for name in ["Data", "mods"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        root
    }
    fn service(root: &Path) -> crate::provider_assets::Service {
        crate::provider_assets::Service::open(
            &root.join("mods"),
            &[],
            &[],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    fn plan() -> crate::feature_plan::Plan {
        let mut settings = crate::backend::Settings::default();
        settings.cameras_enabled = true;
        crate::feature_plan::Plan::freeze(&settings, &crate::library::Library::default()).unwrap()
    }
    fn source() -> Vec<u8> {
        crate::selector_tables::pack(&BTreeMap::from([
            (
                "TEXT.VCLOCALIZEDATA".into(),
                crate::camera_names::tests::stock(),
            ),
            ("OTHER".into(), b"retain".to_vec()),
        ]))
        .unwrap()
    }
    #[test]
    fn disabled_route_never_decodes_or_creates_a_lease() {
        let root = fixture();
        let assets = service(&root);
        let mut lease = None;
        prepare_with(
            &assets,
            &root,
            &root.join("mods"),
            "unused",
            &Default::default(),
            &mut lease,
            || panic!("disabled route checked game"),
            || panic!("disabled route decoded stock"),
        )
        .unwrap();
        assert!(lease.is_none());
        assert!(!root.join("mods/loc").exists());
        drop(assets);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn early_owned_labels_and_later_provider_reads_are_the_same_then_restore() {
        let root = fixture();
        let assets = service(&root);
        let plan = plan();
        let mut lease = None;
        prepare_with(
            &assets,
            &root,
            &root.join("mods"),
            &crate::backend::unique_id(),
            &plan,
            &mut lease,
            || Ok(()),
            || Ok(source()),
        )
        .unwrap();
        let early = fs::read(root.join("mods/loc/english.iff")).unwrap();
        lease.as_ref().unwrap().verify_current().unwrap();
        crate::camera_names::verify(&early, &plan.camera_names()).unwrap();
        assert_eq!(
            assets
                .read("loc/english.iff", 0, crate::provider_protocol::MAX_READ)
                .unwrap()
                .1,
            early
        );
        assert_eq!(plan.stock_overrides(), ["loc/english.iff"]);
        assets.cancel();
        assets.release_closed_handles().unwrap();
        lease.as_mut().unwrap().restore(|| Ok(())).unwrap();
        assert!(!root.join("mods/loc/english.iff").exists());
        assert!(
            !root
                .join("Data")
                .join(crate::camera_name_files::MARKER)
                .exists()
        );
        drop(lease);
        drop(assets);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_source_existing_mod_and_cancelled_preparation_never_replace_a_file() {
        for mode in 0..3 {
            let root = fixture();
            if mode == 1 {
                fs::create_dir(root.join("mods/loc")).unwrap();
                fs::write(root.join("mods/loc/english.iff"), b"existing user mod").unwrap();
            }
            let assets = service(&root);
            let mut lease = None;
            let cancelled = std::cell::Cell::new(false);
            assert!(
                prepare_with(
                    &assets,
                    &root,
                    &root.join("mods"),
                    &crate::backend::unique_id(),
                    &plan(),
                    &mut lease,
                    || {
                        ensure!(!cancelled.get(), "cancelled");
                        Ok(())
                    },
                    || {
                        cancelled.set(mode == 2);
                        Ok(if mode == 0 {
                            b"invalid IFF".to_vec()
                        } else {
                            source()
                        })
                    }
                )
                .is_err()
            );
            assert!(lease.is_none());
            if mode == 1 {
                assert_eq!(
                    fs::read(root.join("mods/loc/english.iff")).unwrap(),
                    b"existing user mod"
                );
            } else {
                assert!(!root.join("mods/loc/english.iff").exists());
            }
            drop(assets);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn foreign_profile_and_alias_collision_leave_a_recoverable_owned_lease() {
        for alias in [false, true] {
            let root = fixture();
            fs::create_dir(root.join("other-mods")).unwrap();
            fs::write(root.join("mods/another.iff"), b"existing alias source").unwrap();
            let assets = crate::provider_assets::Service::open(
                &root.join(if alias { "mods" } else { "other-mods" }),
                &if alias {
                    vec![("loc/english.iff".into(), "another.iff".into())]
                } else {
                    Vec::new()
                },
                &[],
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            let mut lease = None;
            assert!(
                prepare_with(
                    &assets,
                    &root,
                    &root.join("mods"),
                    &crate::backend::unique_id(),
                    &plan(),
                    &mut lease,
                    || Ok(()),
                    || Ok(source())
                )
                .is_err()
            );
            assert!(lease.is_some());
            assets.cancel();
            assets.release_closed_handles().unwrap();
            lease.as_mut().unwrap().restore(|| Ok(())).unwrap();
            assert!(!root.join("mods/loc/english.iff").exists());
            assert_eq!(
                fs::read(root.join("mods/another.iff")).unwrap(),
                b"existing alias source"
            );
            drop(lease);
            drop(assets);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn interrupted_english_recovers_before_the_selected_profile_is_released() {
        let root = fixture();
        let game = root.join("Game");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/original.iff"), b"original installed mod").unwrap();
        let session = crate::backend::unique_id();
        let assets = service(&root);
        let mut profile = crate::mod_profile::Store::create(
            &root,
            &game,
            &root.join("mods"),
            &session,
            || Ok(()),
        )
        .unwrap()
        .unwrap();
        let mut lease = None;
        prepare_with(
            &assets,
            &root,
            &root.join("mods"),
            &session,
            &plan(),
            &mut lease,
            || Ok(()),
            || Ok(source()),
        )
        .unwrap();
        profile.activate(|| Ok(())).unwrap();
        assert!(game.join("mods/loc/english.iff").is_file());
        assets.cancel();
        assets.release_closed_handles().unwrap();
        drop(assets);
        drop(lease);
        drop(profile);
        let mut profile = crate::mod_profile::Store::recover(&root, &game)
            .unwrap()
            .unwrap();
        let mut recovered =
            crate::camera_name_files::Store::recover(&root, profile.selected_root(), &session)
                .unwrap()
                .unwrap();
        recovered.restore(|| Ok(())).unwrap();
        profile.restore(|| Ok(())).unwrap();
        assert_eq!(
            fs::read(game.join("mods/original.iff")).unwrap(),
            b"original installed mod"
        );
        assert!(!root.join("mods/loc/english.iff").exists());
        drop(recovered);
        drop(profile);
        fs::remove_dir_all(root).unwrap();
    }
}
