//! Launch-only orchestration, following WPF multi-config-manager.js:
//! - nothing to generate: a complete user bundle is served as-is, a partial
//!   one is left alone (WPF launched without the selector override);
//! - complete foreign bundle: validate and freeze it before looking at player
//!   inputs; installed coordinated user selectors win without wasted generation;
//! - something to generate: every existing selector file must be WPF-owned or
//!   archive-equivalent to the new output, otherwise the launch stops with
//!   WPF's "A different … mod is installed; it was left untouched.";
//! - with no existing files the generated bundle is published for the
//!   session (owned, restored afterwards); over existing files it is served
//!   by the provider only, so user files are never rewritten.
use crate::{
    backend, loader_files,
    provider_assets::Service,
    selector_files::Store,
    selector_generation::{Inputs, Output},
    selector_tables::{PATHS, Summary, archives_equivalent},
};
use anyhow::{Result, ensure};
use std::{fs, path::Path};

/// WPF `previouslyOwned`: the file is the one WPF's multi-config state says it
/// installed (same path and SHA-256), or archive-equivalent to WPF's cached
/// generation (`matchesPreviousArchive`). WPF kept that state in the portable
/// Data folder this launcher shares.
fn wpf_owned(portable: &Path, mods: &Path, index: usize, bytes: &[u8]) -> bool {
    const KEYS: [&str; 3] = ["all", "edit", "registry"];
    let root = portable.join("Data").join("multi-config");
    let read = |path: &Path, limit: u64| -> Option<Vec<u8>> {
        let meta = fs::symlink_metadata(path).ok()?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
            return None;
        }
        fs::read(path).ok()
    };
    let same = |a: &Path, b: &Path| match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    let Some(text) = read(&root.join("state.json"), 1 << 20) else {
        return false;
    };
    let Ok(state) = serde_json::from_slice::<serde_json::Value>(&text) else {
        return false;
    };
    if state["schema"] != "nba2k27-multi-config-v1" {
        return false;
    }
    let owned = &state["installed"][KEYS[index]];
    let (Some(path), Some(sha)) = (owned["path"].as_str(), owned["sha256"].as_str()) else {
        return false;
    };
    let target = mods.join(PATHS[index].replace('/', "\\"));
    if sha.len() != 64
        || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        || !same(Path::new(path), &target)
    {
        return false;
    }
    if loader_files::digest(bytes).eq_ignore_ascii_case(sha) {
        return true;
    }
    let key = state["generationKey"].as_str().unwrap_or("");
    if state["cacheVersion"] != 5
        || key.len() != 24
        || !key.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        || !state["modsRoot"]
            .as_str()
            .is_some_and(|r| same(Path::new(r), mods))
    {
        return false;
    }
    let name = PATHS[index].rsplit('/').next().unwrap_or("");
    let Some(source) = read(
        &root.join("generated").join(key).join(name),
        crate::sigconfig::MAX_BYTES as u64,
    ) else {
        return false;
    };
    loader_files::digest(&source).eq_ignore_ascii_case(sha) && archives_equivalent(bytes, &source)
}

pub(crate) fn prepare(
    assets: &Service,
    portable: &Path,
    game: &Path,
    mods: &Path,
    session: &str,
    lease: &mut Option<Store>,
    check: impl Fn() -> Result<()>,
) -> Result<Summary> {
    prepare_with(assets, portable, mods, session, lease, &check, |inputs| {
        let manifest = crate::stock_selectors::Manifest::read(game, &check)?;
        let (sources, _) = crate::selector_cache::load(portable, game, &manifest, &check)?;
        check()?;
        crate::selector_generation::build(&manifest, &sources, inputs)
    })
}
fn prepare_with(
    assets: &Service,
    portable: &Path,
    mods: &Path,
    session: &str,
    lease: &mut Option<Store>,
    check: impl Fn() -> Result<()>,
    generate: impl FnOnce(&Inputs) -> Result<Output>,
) -> Result<Summary> {
    ensure!(lease.is_none(), "Generated selector lease already exists");
    ensure!(
        session.len() == 48 && session.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid selector session"
    );
    check()?;
    let present = assets.indexed_selectors()?;
    let complete = present.iter().all(Option::is_some);
    let partial = !complete && present.iter().any(Option::is_some);
    if complete
        && present
            .iter()
            .enumerate()
            .any(|(index, current)| !wpf_owned(portable, mods, index, &current.as_ref().unwrap().1))
    {
        // 0.53.1 already let a coordinated user bundle win, but discovered
        // that only AFTER reading every player base and generating/decompressing
        // replacement tables. Freeze/validate all three archives with the same
        // provider checks first. A malformed bundle still fails; no disk writes
        // occur. Fully owned stale WPF bundles keep the regeneration path below.
        check()?;
        let mut summary = assets.prepare_selectors()?;
        check()?;
        summary.source = format!(
            "{}; installed selector mod kept; automatic player-selector generation skipped",
            summary.source
        );
        return Ok(summary);
    }
    let inputs = assets.selector_inputs()?;
    if inputs.is_empty() {
        // Nothing to generate: serve a complete user bundle as before; WPF
        // ignored a partial one (no override) instead of failing Play.
        return if complete {
            assets.prepare_selectors()
        } else {
            Ok(Summary::absent())
        };
    }
    check()?;
    let output = generate(&inputs)?;
    check()?;
    backend::atomic_json(
        &portable
            .join("Logs")
            .join(format!("native-selectors-{session}.json")),
        &output.report,
    )?;
    let Some(prepared) = output.prepared else {
        if complete {
            return assets.prepare_selectors();
        }
        return Ok(Summary {
            skipped_players: output.report.skipped.len(),
            warnings: output.report.skipped.into_iter().take(12).collect(),
            ..Summary::absent()
        });
    };
    if complete || partial {
        // WPF multi-config-manager.js: each existing file must be its own
        // previous output or equivalent to the new one, else Play stops.
        let foreign = present.iter().enumerate().find_map(|(index, current)| {
            let (_, bytes) = current.as_ref()?;
            (!(archives_equivalent(bytes, &prepared.bytes[index])
                || wpf_owned(portable, mods, index, bytes)))
            .then_some(index)
        });
        if let Some(index) = foreign {
            // A complete foreign bundle is a coordinated user selector mod:
            // serve it unchanged (0.52 behaviour) instead of stopping Play.
            // WPF's ownership state lives in its own portable folder, so its
            // stop fired here for bundles WPF never had a chance to record.
            ensure!(
                complete,
                "A different {} mod is installed; it was left untouched.",
                PATHS[index].rsplit('/').next().unwrap_or(PATHS[index])
            );
            let mut summary = assets.prepare_selectors()?;
            summary.source = format!(
                "{}; installed selector mod kept, {} generated player selector(s) not applied",
                summary.source, prepared.summary.generated_players
            );
            return Ok(summary);
        }
        check()?;
        assets.override_selectors(&prepared, &present)?;
        check()?;
        return Ok(prepared.summary);
    }
    // Retain the durable lease before publication: any later failure goes
    // through exact-owned restoration, never a best-effort filename delete.
    *lease = Some(Store::create(portable, mods, session, &prepared, &check)?);
    lease.as_mut().unwrap().activate(&prepared, &check)?;
    assets.publish_generated_selectors(&prepared, lease.as_ref().unwrap())?;
    check()?;
    Ok(prepared.summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        selector_generation::{Report, Warning},
        selector_tables::{PATHS, Prepared},
    };
    use std::{
        fs,
        sync::{Arc, atomic::AtomicBool},
    };
    fn fixture() -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("rust-selector-runtime-{}", backend::unique_id()));
        for name in ["Data", "Logs", "mods/char"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        root
    }
    fn service(root: &Path) -> Service {
        Service::open(
            &root.join("mods"),
            &[],
            &[],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    #[test]
    fn existing_or_empty_bundle_never_runs_generation_and_partial_bundle_is_left_alone() {
        let root = fixture();
        let s = service(&root);
        assert!(
            !prepare_with(
                &s,
                &root,
                &root.join("mods"),
                &backend::unique_id(),
                &mut None,
                || Ok(()),
                |_| panic!("empty folder cannot generate")
            )
            .unwrap()
            .enabled
        );
        drop(s);
        let bytes = crate::selector_tables::tests::bundle();
        for (name, data) in PATHS.iter().zip(bytes) {
            fs::write(root.join("mods").join(name), data).unwrap();
        }
        let s = service(&root);
        assert!(
            prepare_with(
                &s,
                &root,
                &root.join("mods"),
                &backend::unique_id(),
                &mut None,
                || Ok(()),
                |_| panic!("existing user selectors must win")
            )
            .unwrap()
            .enabled
        );
        drop(s);
        fs::remove_file(root.join("mods").join(PATHS[1])).unwrap();
        let s = service(&root);
        assert!(
            !prepare_with(
                &s,
                &root,
                &root.join("mods"),
                &backend::unique_id(),
                &mut None,
                || Ok(()),
                |_| panic!("nothing to generate for a partial bundle")
            )
            .unwrap()
            .enabled,
            "WPF ignored a partial bundle when nothing needed generating"
        );
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    /// A profile with players to generate plus the given selector files.
    fn generation_fixture(files: &[(usize, Vec<u8>)]) -> std::path::PathBuf {
        let root = fixture();
        fs::write(root.join("mods/png9999.iff"), b"base").unwrap();
        fs::write(root.join("mods/png9999_config_test.iff"), b"config").unwrap();
        for (index, bytes) in files {
            fs::write(root.join("mods").join(PATHS[*index]), bytes).unwrap();
        }
        root
    }
    fn run_generation(root: &Path, s: &Service, generated: [Vec<u8>; 3]) -> Result<Summary> {
        let mut lease = None;
        let result = prepare_with(
            s,
            root,
            &root.join("mods"),
            &backend::unique_id(),
            &mut lease,
            || Ok(()),
            |_| {
                Ok(Output {
                    prepared: Some(Prepared::existing(generated)?),
                    report: Report::default(),
                })
            },
        );
        assert!(lease.is_none(), "existing files never take a disk lease");
        result
    }
    fn served(s: &Service, index: usize) -> Vec<u8> {
        s.read(PATHS[index], 0, crate::provider_protocol::MAX_READ)
            .unwrap()
            .1
    }
    #[test]
    fn equivalent_or_partial_existing_files_get_the_generated_bundle_without_rewrites() {
        let bytes = crate::selector_tables::tests::bundle();
        for files in [
            (0..3).map(|i| (i, bytes[i].clone())).collect::<Vec<_>>(),
            vec![(1, bytes[1].clone())],
        ] {
            let root = generation_fixture(&files);
            let s = service(&root);
            let summary = run_generation(&root, &s, bytes.clone()).unwrap();
            assert!(summary.enabled);
            for index in 0..3 {
                assert_eq!(served(&s, index), bytes[index]);
                let on_disk = root.join("mods").join(PATHS[index]);
                match files.iter().find(|(i, _)| *i == index) {
                    Some((_, original)) => assert_eq!(&fs::read(&on_disk).unwrap(), original),
                    None => assert!(!on_disk.exists(), "missing files are served, not written"),
                }
            }
            drop(s);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn a_foreign_selector_file_stops_play_with_the_wpf_message_and_stays_untouched() {
        let bytes = crate::selector_tables::tests::bundle();
        // A different archive at the all-table path.
        let root = generation_fixture(&[(0, bytes[2].clone())]);
        let s = service(&root);
        let error = run_generation(&root, &s, bytes.clone()).unwrap_err();
        assert_eq!(
            format!("{error:#}"),
            "A different signatureconfigs_all.iff mod is installed; it was left untouched."
        );
        assert_eq!(
            fs::read(root.join("mods").join(PATHS[0])).unwrap(),
            bytes[2]
        );
        assert_eq!(served(&s, 0), bytes[2], "nothing was published");
        assert!(s.read(PATHS[1], 0, 16).is_err());
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_complete_foreign_selector_bundle_is_served_unchanged_and_play_continues() {
        let bytes = crate::selector_tables::tests::bundle();
        // A complete user bundle whose all-table differs from the generated one.
        let mut all = crate::selector_tables::archive(&bytes[0]).unwrap();
        all.insert("user_extra.json".into(), b"{}".to_vec());
        let all = crate::selector_tables::pack(&all).unwrap();
        assert!(!archives_equivalent(&all, &bytes[0]));
        let user = [all, bytes[1].clone(), bytes[2].clone()];
        let root = generation_fixture(&(0..3).map(|i| (i, user[i].clone())).collect::<Vec<_>>());
        let s = service(&root);
        let summary = run_generation(&root, &s, bytes.clone()).unwrap();
        assert!(summary.source.contains("installed selector mod kept"));
        for index in 0..3 {
            assert_eq!(
                fs::read(root.join("mods").join(PATHS[index])).unwrap(),
                user[index]
            );
        }
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn complete_user_bundle_skips_all_player_reads_and_generation() {
        let bytes = crate::selector_tables::tests::bundle();
        let root = generation_fixture(&(0..3).map(|i| (i, bytes[i].clone())).collect::<Vec<_>>());
        let s = service(&root);
        // Input parsing would reject this symlink/replacement or invalid base;
        // it is irrelevant when the user has supplied all coordinated tables.
        let summary = prepare_with(
            &s,
            &root,
            &root.join("mods"),
            &backend::unique_id(),
            &mut None,
            || Ok(()),
            |_| panic!("complete user selectors must not regenerate"),
        )
        .unwrap();
        assert!(summary.enabled);
        assert!(summary.source.contains("generation skipped"));
        for i in 0..3 {
            assert_eq!(served(&s, i), bytes[i]);
        }
        assert!(
            !root
                .join("Data")
                .join(crate::selector_files::MARKER)
                .exists()
        );
        assert_eq!(fs::read_dir(root.join("Logs")).unwrap().count(), 0);
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn invalid_complete_user_bundle_still_fails_validation_before_generation() {
        let bytes = crate::selector_tables::tests::bundle();
        let root = generation_fixture(&[
            (0, b"not an archive".to_vec()),
            (1, bytes[1].clone()),
            (2, bytes[2].clone()),
        ]);
        let s = service(&root);
        assert!(
            prepare_with(
                &s,
                &root,
                &root.join("mods"),
                &backend::unique_id(),
                &mut None,
                || Ok(()),
                |_| panic!("invalid user bundle cannot regenerate")
            )
            .is_err()
        );
        assert_eq!(
            fs::read(root.join("mods").join(PATHS[0])).unwrap(),
            b"not an archive"
        );
        assert!(
            !root
                .join("Data")
                .join(crate::selector_files::MARKER)
                .exists()
        );
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn complete_bundle_with_exact_wpf_ownership_still_regenerates() {
        let bytes = crate::selector_tables::tests::bundle();
        let root = generation_fixture(&(0..3).map(|i| (i, bytes[i].clone())).collect::<Vec<_>>());
        let state_dir = root.join("Data/multi-config");
        fs::create_dir_all(&state_dir).unwrap();
        let installed: serde_json::Map<String, serde_json::Value> = ["all", "edit", "registry"]
            .into_iter()
            .enumerate()
            .map(|(i, key)| {
                (
                    key.into(),
                    serde_json::json!({
                        "path": root.join("mods").join(PATHS[i]),
                        "sha256": loader_files::digest(&bytes[i])
                    }),
                )
            })
            .collect();
        fs::write(
            state_dir.join("state.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema":"nba2k27-multi-config-v1", "installed":installed
            }))
            .unwrap(),
        )
        .unwrap();
        let s = service(&root);
        let generated = std::cell::Cell::new(false);
        let summary = prepare_with(
            &s,
            &root,
            &root.join("mods"),
            &backend::unique_id(),
            &mut None,
            || Ok(()),
            |_| {
                generated.set(true);
                Ok(Output {
                    prepared: Some(Prepared::existing(bytes.clone())?),
                    report: Report::default(),
                })
            },
        )
        .unwrap();
        assert!(
            generated.get(),
            "owned bundles must not take user-bundle fast path"
        );
        assert!(summary.enabled);
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_files_wpf_recorded_as_its_own_are_regenerated() {
        let bytes = crate::selector_tables::tests::bundle();
        let stale = bytes[2].clone();
        let root = generation_fixture(&[(0, stale.clone())]);
        let state_dir = root.join("Data/multi-config");
        fs::create_dir_all(&state_dir).unwrap();
        let state = serde_json::json!({
            "schema": "nba2k27-multi-config-v1",
            "installed": {"all": {
                "path": root.join("mods").join(PATHS[0].replace('/', "\\")),
                "sha256": loader_files::digest(&stale).to_lowercase(),
            }},
        });
        fs::write(
            state_dir.join("state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let s = service(&root);
        assert!(run_generation(&root, &s, bytes.clone()).unwrap().enabled);
        assert_eq!(served(&s, 0), bytes[0]);
        assert_eq!(fs::read(root.join("mods").join(PATHS[0])).unwrap(), stale);
        drop(s);
        // A state entry for a different path does not make the file WPF's.
        let other = serde_json::json!({
            "schema": "nba2k27-multi-config-v1",
            "installed": {"all": {"path": root.join("elsewhere.iff"), "sha256": loader_files::digest(&stale)}},
        });
        fs::write(
            state_dir.join("state.json"),
            serde_json::to_vec(&other).unwrap(),
        )
        .unwrap();
        let s = service(&root);
        assert!(run_generation(&root, &s, bytes).is_err());
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn generated_bundle_publishes_early_and_provider_bytes_are_frozen_then_owned_files_restore() {
        let root = fixture();
        fs::write(root.join("mods/png9999.iff"), b"invalid base preserved").unwrap();
        fs::write(
            root.join("mods/png9999_config_test.iff"),
            b"configuration preserved",
        )
        .unwrap();
        let s = service(&root);
        let inputs = s.selector_inputs().unwrap();
        assert_eq!(inputs.len(), 1);
        assert!(inputs[&9999].bases[0].error.is_some());
        let bytes = crate::selector_tables::tests::bundle();
        let mut lease = None;
        let session = backend::unique_id();
        let summary = prepare_with(
            &s,
            &root,
            &root.join("mods"),
            &session,
            &mut lease,
            || Ok(()),
            |_| {
                Ok(Output {
                    prepared: Some(Prepared::existing(bytes.clone())?),
                    report: Report::default(),
                })
            },
        )
        .unwrap();
        assert!(summary.enabled);
        assert!(
            root.join("Logs")
                .join(format!("native-selectors-{session}.json"))
                .exists()
        );
        for (name, expected) in PATHS.iter().zip(&bytes) {
            assert_eq!(fs::read(root.join("mods").join(name)).unwrap(), *expected);
            assert_eq!(
                s.read(name, 0, crate::provider_protocol::MAX_READ)
                    .unwrap()
                    .1,
                *expected
            );
        }
        s.cancel();
        s.release_closed_handles().unwrap();
        lease.as_mut().unwrap().restore(|| Ok(())).unwrap();
        for name in PATHS {
            assert!(!root.join("mods").join(name).exists());
        }
        assert_eq!(
            fs::read(root.join("mods/png9999.iff")).unwrap(),
            b"invalid base preserved"
        );
        assert!(
            !root
                .join("Data")
                .join(crate::selector_files::MARKER)
                .exists()
        );
        drop(lease);
        drop(s);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn all_skipped_and_cancelled_generation_do_not_publish_files() {
        for cancel in [false, true] {
            let root = fixture();
            fs::write(root.join("mods/png9999_config_test.iff"), b"config").unwrap();
            let s = service(&root);
            let cancelled = std::cell::Cell::new(false);
            let mut lease = None;
            let result = prepare_with(
                &s,
                &root,
                &root.join("mods"),
                &backend::unique_id(),
                &mut lease,
                || {
                    ensure!(!cancelled.get(), "cancelled");
                    Ok(())
                },
                |_| {
                    cancelled.set(cancel);
                    Ok(Output {
                        prepared: None,
                        report: Report {
                            skipped: vec![Warning {
                                id: 9999,
                                reason: "base-png-missing".into(),
                            }],
                            ..Default::default()
                        },
                    })
                },
            );
            if cancel {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap().skipped_players, 1);
            }
            assert!(lease.is_none());
            for name in PATHS {
                assert!(!root.join("mods").join(name).exists());
            }
            drop(s);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
