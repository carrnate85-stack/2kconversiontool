#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod apparel;
mod archive_decode;
mod asset_catalog;
mod backend;
mod buffer_decode;
mod catalog_casing;
#[cfg(test)]
mod comparison;
mod diagnostics;
mod exporter;
#[cfg(test)]
mod mod_runner;
mod scene;
mod tangent;
mod validation;
mod worker;
mod wpf_regex;
use asset_catalog as catalog;
mod asset_codec;
mod container;
use asset_codec as codec;
mod texture_decode;
use texture_decode as textures;
mod build_channel;
mod host;
mod iff_directory;
mod inspection;
mod materials;
mod native_checks;
mod player;
mod player_config;
mod preview;
mod raw_export;
use anyhow::{Context, Result};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Instant,
};

fn probe(root: &str, path: &str) -> Result<()> {
    let emit: backend::Emit = Arc::new(|_| {});
    let mut b = backend::Backend::new(emit, Arc::new(AtomicBool::new(false)));
    let loaded = b.dispatch("game:load", &[json!(root)])?;
    let mut report = json!({"index":loaded,"resources":[],"version":backend::VERSION});
    let samples = [
        "balls/ball_gameplay_official_country_flag_china.iff",
        "char/sig/png04090.iff",
        "levels/arena_700_int.iff",
        "levels/arena_005_int.iff",
        "clothing/geo/dynamic/shoes_adidas_dame8_geo.iff",
        "clothing/geo/dynamic/shoes_jordan_tatum3_geo.iff",
        "clothing/geo/dynamic/shoes_newbalance_twowxy5_geo.iff",
        "clothing/geo/dynamic/male_tshirt_ls_geo.iff",
        "char/sig/png06572_geo_hair_curly_afro_tangentspace.iff",
    ];
    for name in samples {
        let hit = b
            .catalog()?
            .entries
            .iter()
            .find(|e| e.name == name)
            .cloned();
        let mut result = json!({"name":name});
        if let Some(e) = hit {
            let started = Instant::now();
            match b.dispatch("file:inspect", &[json!(e.id)]) {
                Ok(iff) => {
                    result["inspectMs"] = json!(started.elapsed().as_secs_f64() * 1000.);
                    result["members"] = json!(iff["members"].as_array().map(Vec::len));
                    result["memberList"] = iff["members"].clone();
                    let scne = iff["members"].as_array().unwrap().iter().find(|m| {
                        m["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .ends_with(".scne")
                    });
                    if let Some(scne) = scne {
                        if name == "levels/arena_005_int.iff" {
                            let member_id =
                                scne["id"].as_u64().context("Large scene member missing")?;
                            let (_, expected) = b.member_bytes(member_id as usize, false)?;
                            anyhow::ensure!(
                                expected.len() > 32 * 1024 * 1024,
                                "Large member fixture no longer exceeds the previous 32 MiB inspection cap"
                            );
                            let target = PathBuf::from(path)
                                .parent()
                                .context("Report parent missing")?
                                .join("arena005-original-level.SCNE");
                            let mut request =
                                b.dispatch("file:save-member", &[json!(member_id), json!(false)])?;
                            request["target"] = json!(target);
                            let saved = b.dispatch("file:publish-member", &[request])?;
                            anyhow::ensure!(
                                std::fs::read(&target)? == expected,
                                "Large original member Save changed source bytes"
                            );
                            result["largeMemberSave"] =
                                json!({"byteExact":true,"bytes":saved["bytes"],"streamed":true});
                        }
                        result["scne"] = match b
                            .dispatch("file:preview-member", &[scne["id"].clone()])
                        {
                            Ok(value) => {
                                let out = PathBuf::from(path)
                                    .parent()
                                    .context("Report parent required")?
                                    .join(format!(
                                        "{}-scene.json",
                                        name.rsplit('/').next().unwrap()
                                    ));
                                std::fs::create_dir_all(out.parent().unwrap())?;
                                std::fs::write(out, value["text"].as_str().unwrap_or(""))?;
                                json!({"readable":true,"characters":value["text"].as_str().unwrap_or("").len()})
                            }
                            Err(e) => json!({"error":e.to_string()}),
                        };
                    }
                    let started = Instant::now();
                    result["model"] = match b.dispatch("preview:load", &[json!(e.id)]) {
                        Ok(p) => {
                            json!({"ms":started.elapsed().as_secs_f64()*1000.,"meshes":p["meshes"].as_array().map(Vec::len),"stats":p["stats"],"skipped":p["skipped"],"materials":p["materials"],"player":p["player"],"shoe":p["shoe"],"apparel":p["apparel"],"notes":p["notes"],"textures":p["textures"].as_array().map(|a|a.iter().map(|v|json!({"name":v["name"],"format":v["format"],"range":v["range"],"binding":v["binding"],"assignments":v["assignments"]})).collect::<Vec<_>>())})
                        }
                        Err(e) => json!({"error":e.to_string()}),
                    };
                }
                Err(error) => result["error"] = json!(format!("{error:#}")),
            }
        } else {
            result["error"] = json!("Manifest entry missing");
        }
        report["resources"].as_array_mut().unwrap().push(result);
    }
    let report_path = PathBuf::from(path);
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
fn verify_exports(root: &str, path: &str) -> Result<()> {
    let report_path = PathBuf::from(path);
    let parent = report_path.parent().context("Report parent required")?;
    std::fs::create_dir_all(parent)?;
    let events = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
    let recorded = events.clone();
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_in_decode = Arc::new(AtomicBool::new(false));
    let should_cancel = cancel_in_decode.clone();
    let cancel_on_event = cancel_flag.clone();
    let emit: backend::Emit = Arc::new(move |event| {
        if should_cancel.load(std::sync::atomic::Ordering::Relaxed)
            && event["payload"]["phase"] == "Native container decode"
        {
            cancel_on_event.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        recorded.lock().unwrap().push(event);
    });
    let mut b = backend::Backend::new(emit, cancel_flag);
    b.dispatch("game:load", &[json!(root)])?;
    b.dispatch("export:set-root", &[json!(parent.canonicalize()?)])?;
    let names = [
        "balls/ball_gameplay_official_country_flag_china.iff",
        "char/sig/png04090.iff",
        "levels/arena_700_int.iff",
    ];
    let ids: Vec<_> = names
        .iter()
        .map(|name| b.catalog()?.resolve(name).map(|e| e.id))
        .collect::<Result<_>>()?;
    events.lock().unwrap().clear();
    let started = Instant::now();
    let exported = b.dispatch("export:start", &[json!(ids)])?;
    let export_ms = started.elapsed().as_secs_f64() * 1000.;
    anyhow::ensure!(
        exported["failures"].as_array().unwrap().is_empty(),
        "Representative export failed"
    );
    let oodle = codec::Oodle::load(&b.catalog()?.root)?;
    let mut checks = Vec::new();
    for result in exported["results"].as_array().unwrap() {
        let e = b
            .catalog()?
            .entry(result["id"].as_u64().unwrap() as usize)?;
        let source = oodle.archive(&codec::range(b.catalog()?, e)?, &b.cancel)?;
        let saved = std::fs::read(result["target"].as_str().unwrap())?;
        anyhow::ensure!(source == saved, "Export bytes changed");
        checks.push(json!({"name":e.name,"bytes":saved.len(),"members":result["members"],"entireDecodedContainerByteExact":true}));
    }
    let progress: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| e["payload"]["percent"].as_u64())
        .collect();
    anyhow::ensure!(
        progress.windows(2).all(|w| w[0] <= w[1])
            && progress.first() == Some(&0)
            && progress.last() == Some(&100)
            && progress.iter().any(|p| *p > 0 && *p < 100),
        "Export progress isn't monotonic"
    );
    b.dispatch("file:inspect", &[json!(ids[0])])?;
    let gallery = b.dispatch("preview:textures", &[json!(ids[0])])?;
    use base64::Engine;
    let mut dds_checks = Vec::new();
    for item in gallery["items"].as_array().unwrap() {
        let name = item["name"].as_str().unwrap();
        let bytes =
            base64::engine::general_purpose::STANDARD.decode(item["data"].as_str().unwrap())?;
        let filename = if name.ends_with(".tld") {
            format!("{}.dds", name.trim_end_matches(".tld"))
        } else {
            name.into()
        };
        let out = parent.join(filename);
        std::fs::write(&out, &bytes)?;
        dds_checks.push(json!({"name":name,"file":out,"bytes":bytes.len()}));
    }
    let textures_export = b.dispatch("export:textures", &[json!(ids[0])])?;
    // Save commands prepare metadata only; publication stays in the worker and
    // preserves source bytes, existing files, and the active model session.
    let members = b.dispatch("file:inspect", &[json!(ids[0])])?;
    let index = members["members"][0]["id"]
        .as_u64()
        .context("Fixture member missing")?;
    let (_, expected_member) = b.member_bytes(index as usize, false)?;
    let mut save = b.dispatch("file:save-member", &[json!(index), json!(false)])?;
    let member_target = parent.join("saved-original-member.bin");
    save["target"] = json!(member_target);
    b.dispatch("file:publish-member", &[save.clone()])?;
    anyhow::ensure!(
        std::fs::read(&member_target)? == expected_member,
        "Member Save altered bytes"
    );
    anyhow::ensure!(
        b.dispatch("file:publish-member", &[save.clone()]).is_err(),
        "Member Save overwrote an existing file"
    );
    save["target"] = json!(b.catalog()?.root.join("must-not-save.bin"));
    anyhow::ensure!(
        b.dispatch("file:publish-member", &[save.clone()]).is_err(),
        "Member Save allowed game-directory writes"
    );
    save["resourceId"] = json!(ids[1]);
    save["target"] = json!(parent.join("stale-member.bin"));
    anyhow::ensure!(
        b.dispatch("file:publish-member", &[save]).is_err(),
        "Stale inspection Save was accepted"
    );
    let item = &gallery["items"][0];
    let mut save = b.dispatch(
        "preview:save-texture",
        &[json!(ids[0]), item["name"].clone()],
    )?;
    let texture_target = parent.join("saved-source-texture.dds");
    save["target"] = json!(texture_target);
    b.dispatch("preview:publish-texture", &[save.clone()])?;
    let expected = base64::engine::general_purpose::STANDARD
        .decode(item["data"].as_str().context("Texture bytes missing")?)?;
    anyhow::ensure!(
        std::fs::read(&texture_target)? == expected,
        "Selected DDS Save changed source bytes"
    );
    anyhow::ensure!(
        b.dispatch("preview:publish-texture", &[save.clone()])
            .is_err(),
        "Selected DDS Save overwrote an existing file"
    );
    save["target"] = json!(b.catalog()?.root.join("must-not-save.dds"));
    anyhow::ensure!(
        b.dispatch("preview:publish-texture", &[save]).is_err(),
        "Texture Save allowed game-directory writes"
    );
    anyhow::ensure!(
        textures_export["failures"].as_array().unwrap().is_empty()
            && textures_export["results"].as_array().unwrap().len() == 3,
        "Source DDS export-all failed"
    );
    for saved in textures_export["results"].as_array().unwrap() {
        let source_name = saved["name"].as_str().unwrap();
        let item = gallery["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["resourceName"] == source_name)
            .context("DDS export missing from gallery reference")?;
        let expected =
            base64::engine::general_purpose::STANDARD.decode(item["data"].as_str().unwrap())?;
        anyhow::ensure!(
            std::fs::read(saved["target"].as_str().unwrap())? == expected,
            "Export-all DDS bytes changed"
        );
    }
    anyhow::ensure!(
        b.last_output.as_ref().map(|p| p.to_string_lossy())
            == textures_export["folder"]
                .as_str()
                .map(std::borrow::Cow::Borrowed),
        "Last output is not the actual texture job folder"
    );
    let last_success = b.last_output.clone();
    let original_size = b.catalog.as_ref().unwrap().entries[ids[0]].size;
    b.catalog.as_mut().unwrap().diagnostic_size(ids[0], 1)?;
    let failure = b.dispatch(
        "export:start",
        &[json!({"ids":[ids[0]],"mode":"mod-ready"})],
    )?;
    b.catalog
        .as_mut()
        .unwrap()
        .diagnostic_size(ids[0], original_size)?;
    anyhow::ensure!(
        failure["results"].as_array().unwrap().is_empty()
            && failure["failures"][0]["id"] == ids[0]
            && b.last_output == last_success,
        "Failed export did not preserve retry ID/last folder"
    );
    let retried = b.dispatch(
        "export:start",
        &[json!({"ids":[ids[0]],"mode":"mod-ready"})],
    )?;
    anyhow::ensure!(
        retried["failures"].as_array().unwrap().is_empty()
            && retried["results"][0]["validation"]["status"] == "ready",
        "Source restored retry or in-memory validation failed"
    );
    anyhow::ensure!(
        b.dispatch(
            "export:start",
            &[json!({"ids":vec![ids[0];5001],"mode":"original"})]
        )
        .is_err(),
        "Oversized batch was not rejected"
    );
    cancel_in_decode.store(true, std::sync::atomic::Ordering::Relaxed);
    let interrupted = b.dispatch("export:start", &[json!(ids)])?;
    cancel_in_decode.store(false, std::sync::atomic::Ordering::Relaxed);
    anyhow::ensure!(
        interrupted["cancelled"] == true && interrupted["pending"].as_array().unwrap().len() == 2,
        "Cancelled export lost unfinished resources"
    );
    b.dispatch("cache:clear", &[])?;
    let cache = b.dispatch("cache:status", &[])?;
    anyhow::ensure!(
        cache["bytes"] == 0 && cache["files"] == 0 && b.catalog()?.entries.len() > 0,
        "Cache clearing affected catalog or failed to release container"
    );
    b.dispatch("export:set-root", &[json!(b.catalog()?.root)])?;
    let reject = b.dispatch("export:start", &[json!([ids[0]])]);
    anyhow::ensure!(reject.is_err(), "Game-folder export was not blocked");
    anyhow::ensure!(
        b.dispatch("export:textures", &[json!(ids[0])]).is_err(),
        "DDS game-folder export was not blocked"
    );
    let report = json!({"pass":true,"exportMs":export_ms,"byteExact":checks,"progress":progress,"textures":dds_checks,"exportAllDdsByteExact":true,"actualTextureJobFolder":textures_export["folder"],"gameFolderProtected":true,"failureRetryValidation":true,"cancelledResourcesRetained":true,"cacheClearNoGameOrExportMutation":true,"folder":exported["folder"]});
    std::fs::write(report_path, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
fn entry() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--verify-catalog") {
        return native_checks::catalog(
            args.get(2).context("Game root required")?,
            args.get(3).context("Cases path required")?,
            args.get(4).context("Report path required")?,
            args.get(5).map(String::as_str) == Some("legacy"),
        );
    }
    if args.get(1).map(String::as_str) == Some("--benchmark-exports") {
        return native_checks::benchmark(
            args.get(2).context("Game root required")?,
            args.get(3).context("Benchmark cases required")?,
            args.get(4).context("Fresh report path required")?,
        );
    }
    if args.get(1).map(String::as_str) == Some("--profile-export") {
        return native_checks::profile(
            args.get(2).context("Game root required")?,
            args.get(3).context("Virtual resource name required")?,
            args.get(4).context("Report path required")?,
            args.get(5).map(String::as_str).unwrap_or("mod-ready"),
        );
    }
    if args.get(1).map(String::as_str) == Some("--verify-families") {
        return native_checks::verify(
            args.get(2).context("Game root required")?,
            args.get(3).context("Report path required")?,
        );
    }
    if args.get(1).map(String::as_str) == Some("--verify-inspection") {
        return native_checks::inspection(
            args.get(2).context("Game root required")?,
            args.get(3).context("Report path required")?,
        );
    }
    if args.get(1).map(String::as_str) == Some("--verify-worker") {
        return worker::verify(
            args.get(2).context("Game root required")?,
            args.get(3).context("Report required")?,
        );
    }
    if args.get(1).map(String::as_str) == Some("--asset-worker") {
        return worker::serve();
    }
    if args.get(1).map(String::as_str) == Some("--verify-mod") {
        let path = PathBuf::from(args.get(3).context("Report path required")?);
        let parent = path.parent().context("Report parent required")?;
        std::fs::create_dir_all(parent)?;
        let events = Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
        let recorded = events.clone();
        let mut b = backend::Backend::new(
            Arc::new(move |event| {
                if let Some(p) = event["payload"]["percent"].as_u64() {
                    recorded.lock().unwrap().push(p);
                }
            }),
            Arc::new(AtomicBool::new(false)),
        );
        b.dispatch(
            "game:load",
            &[json!(args.get(2).context("Game root required")?)],
        )?;
        b.dispatch("export:set-root", &[json!(parent.canonicalize()?)])?;
        let names = [
            "balls/ball_gameplay_official_country_flag_china.iff",
            "char/sig/png04090.iff",
            "levels/arena_700_int.iff",
            "levels/arena_700_int_floor.iff",
            "levels/character_builder_int.iff",
        ];
        let ids: Vec<_> = names
            .iter()
            .filter_map(|n| b.catalog().ok()?.resolve(n).ok().map(|e| e.id))
            .collect();
        events.lock().unwrap().clear();
        let started = Instant::now();
        let mut result = b.dispatch("export:start", &[json!({"ids":ids,"mode":"mod-ready"})])?;
        result["exportMs"] = json!(started.elapsed().as_secs_f64() * 1000.);
        let progress = events.lock().unwrap().clone();
        result["progressMonotonic"] = json!(
            progress.windows(2).all(|p| p[0] <= p[1])
                && progress.first() == Some(&0)
                && progress.last() == Some(&100)
        );
        result["progress"] = json!(progress);
        std::fs::write(path, serde_json::to_vec_pretty(&result)?)?;
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--probe") {
        return probe(
            args.get(2).context("Game root required")?,
            args.get(3).context("Report path required")?,
        );
    }
    if args.get(1).map(String::as_str) == Some("--verify-exports") {
        return verify_exports(
            args.get(2).context("Game root required")?,
            args.get(3).context("Report path required")?,
        );
    }
    let smoke = if args.get(1).map(String::as_str) == Some("--smoke") {
        Some(PathBuf::from(args.get(2).context("Report path required")?))
    } else {
        None
    };
    host::run(smoke)
}
fn main() {
    if let Err(error) = entry() {
        eprintln!("{error:#}");
        if !std::env::args().any(|a| {
            a == "--probe"
                || a == "--smoke"
                || a == "--verify-exports"
                || a == "--verify-mod"
                || a == "--asset-worker"
                || a == "--verify-worker"
                || a == "--verify-families"
                || a == "--verify-inspection"
                || a == "--profile-export"
                || a == "--benchmark-exports"
                || a == "--verify-catalog"
        }) {
            let text:Vec<u16>=format!("NBA 2K27 Exporter could not start:\n\n{error:#}\n\nThis app requires Microsoft's shared Evergreen WebView2 Runtime.").encode_utf16().chain(Some(0)).collect();
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    None,
                    windows::core::PCWSTR(text.as_ptr()),
                    windows::core::w!("NBA 2K27 Exporter"),
                    windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}
