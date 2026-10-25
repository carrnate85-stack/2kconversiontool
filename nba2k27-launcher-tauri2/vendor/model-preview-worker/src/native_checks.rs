//! Developer-only CLI acceptance checks; never called from desktop commands.
use crate::{
    catalog::Catalog,
    codec::{self, Oodle},
    worker::Session,
};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Instant,
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Diagnostic source hashing must not impose the old buffered export cap or
/// allocate an entire source range before/after measuring the actual exporter.
fn hash_range(catalog: &Catalog, resource: &crate::catalog::Resource) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(catalog.path(resource)?)?;
    let len = file.metadata()?.len();
    ensure!(
        resource.offset <= len && resource.size <= len - resource.offset,
        "Resource exceeds archive bounds"
    );
    file.seek(SeekFrom::Start(resource.offset))?;
    let mut hasher = Sha256::new();
    let mut remaining = resource.size;
    let mut chunk = vec![0; 65536];
    while remaining > 0 {
        let n = remaining.min(chunk.len() as u64) as usize;
        file.read_exact(&mut chunk[..n])?;
        hasher.update(&chunk[..n]);
        remaining -= n as u64;
    }
    Ok(format!("{:X}", hasher.finalize()))
}

/// Differential catalog evidence. The WPF oracle owns only a generated fixture;
/// neither this CLI nor the ordinary browser writes game files or asset caches.
pub fn catalog(root: &str, cases_path: &str, report: &str, legacy: bool) -> Result<()> {
    use serde_json::Value;
    let report_path = Path::new(report);
    let directory = report_path.parent().context("Report parent missing")?;
    let document: Value = serde_json::from_slice(&std::fs::read(cases_path)?)?;
    let mut sources = Vec::new();
    let mut results = Vec::new();
    for (source, root) in [
        ("installed", Path::new(root).to_path_buf()),
        ("fixture", directory.join("catalog-fixture")),
    ] {
        let catalog = Catalog::load(&root)?;
        let old = if legacy {
            Some(nba2k_assets::catalog::Catalog::load(&root)?)
        } else {
            None
        };
        let entries = old.as_ref().map_or(&catalog.entries, |old| &old.entries);
        let mut rows_hash = Sha256::new();
        for entry in entries {
            rows_hash.update((entry.id as u64).to_le_bytes());
            for text in [&entry.name, &entry.archive] {
                rows_hash.update((text.len() as u64).to_le_bytes());
                rows_hash.update(text.as_bytes());
            }
            rows_hash.update(entry.offset.to_le_bytes());
            rows_hash.update(entry.size.to_le_bytes());
        }
        let invalid = if legacy {
            std::fs::read_to_string(root.join("manifest"))?
                .lines()
                .count()
                - entries.len()
        } else {
            catalog.invalid_rows
        };
        sources.push(json!({"name":source,"entries":entries.len(),"invalidRows":invalid,"rowsSha256":format!("{:X}",rows_hash.finalize())}));
        for case in document["cases"]
            .as_array()
            .context("Catalog cases missing")?
            .iter()
            .filter(|case| case["source"] == source)
        {
            let name = &case["name"];
            let result = (|| -> Result<Value> {
                if let Some(folder) = case["folder"].as_str() {
                    return Ok(
                        json!({"name":name,"value":if let Some(old)=&old{old.folder_list(folder)}else{catalog.folder_list(folder)?}}),
                    );
                }
                let options = &case["options"];
                let page = if let Some(old) = &old {
                    old.search(options)?
                } else {
                    catalog.search(options)?
                };
                let ids: Vec<_> = page["entries"]
                    .as_array()
                    .context("Search entries missing")?
                    .iter()
                    .map(|entry| entry["id"].clone())
                    .collect();
                let full_hash = if legacy {
                    Value::Null
                } else {
                    let mut hash = Sha256::new();
                    for id in catalog.match_ids(options, &AtomicBool::new(false))?.iter() {
                        hash.update((*id as u64).to_le_bytes());
                    }
                    json!(format!("{:X}", hash.finalize()))
                };
                Ok(
                    json!({"name":name,"total":page["total"],"offset":page["offset"],"limit":page["limit"],"ids":ids,"allIdsSha256":full_hash}),
                )
            })();
            results.push(match result {
                Ok(result) => result,
                Err(error) => json!({"name":name,"error":format!("{error:#}")}),
            });
        }
    }
    std::fs::write(
        report_path,
        serde_json::to_vec_pretty(
            &json!({"legacyPageProbe":legacy,"sources":sources,"cases":results}),
        )?,
    )?;
    Ok(())
}

pub fn inspection(root: &str, report: &str) -> Result<()> {
    let path = Path::new(report);
    let output = path.parent().context("Report parent missing")?;
    std::fs::create_dir_all(output)?;
    let catalog = Catalog::load(Path::new(root))?;
    let oodle = Oodle::load(&catalog.root)?;
    let cancel = AtomicBool::new(false);
    let mut session = Session::new(
        Arc::new(|_| {}),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    session.dispatch("game:load", &[json!(root)])?;
    let mut checks = Vec::new();
    for name in [
        "levels/arena_005_int.iff",
        "char/sig/png04090.iff",
        "char/sig/png00392.iff",
    ] {
        let resource = catalog.resolve(name)?;
        let packed = codec::range(&catalog, resource)?;
        let before = hash(&packed);
        let source = crate::archive_decode::strict(&oodle, &catalog.root, &packed, &cancel)?;
        let mut archive = codec::Archive::open(source)?;
        let listing = session.dispatch("file:inspect", &[json!(resource.id)])?;
        for extension in ["scne", "txtr", "bin"] {
            let member = listing["members"]
                .as_array()
                .context("Members missing")?
                .iter()
                .find(|m| {
                    m["name"]
                        .as_str()
                        .is_some_and(|n| n.to_ascii_lowercase().ends_with(&format!(".{extension}")))
                });
            let Some(member) = member else {
                continue;
            };
            let id = member["id"].as_u64().context("Member id missing")?;
            let member_name = member["name"].as_str().context("Member name missing")?;
            let original = archive.read(id as usize, codec::MAX)?;
            let view = session.dispatch("file:preview-member", &[json!(id)])?;
            let max = if extension == "bin" {
                crate::inspection::HEX_MAX
            } else {
                crate::inspection::TEXT_MAX
            };
            ensure!(
                view["shownBytes"].as_u64().is_some_and(|n| n <= max as u64),
                "Unbounded content sample: {member_name}"
            );
            ensure!(
                view["shownLines"].as_u64().is_some_and(|n| n <= 20_000),
                "Unbounded lines"
            );
            if extension == "bin" {
                ensure!(view["mode"] == "hex", "Binary preview is not hex");
            } else {
                ensure!(
                    view["decoded"] == true,
                    "Document preview did not decode: {member_name}"
                );
            }
            if name.contains("arena_005") && extension == "scne" {
                ensure!(
                    original.len() > 32 * 1024 * 1024 && view["truncated"] == true,
                    "Large scene fixture or sample changed"
                );
            }
            let mut saves = Vec::new();
            for decoded in [false, true] {
                if decoded && extension == "bin" {
                    continue;
                }
                let mut request =
                    session.dispatch("file:save-member", &[json!(id), json!(decoded)])?;
                let target = output.join(format!(
                    "{}-{}.{}",
                    resource.id,
                    id,
                    if decoded { "json" } else { extension }
                ));
                request["target"] = json!(target);
                session.dispatch("file:publish-member", &[request])?;
                let expected = if decoded {
                    let bytes = if codec::vcz(&original) {
                        crate::buffer_decode::decode(&original, None, &oodle, &cancel)?
                    } else {
                        original.clone()
                    };
                    crate::inspection::decoded_json(&bytes)?
                } else {
                    original.clone()
                };
                let saved = std::fs::read(&target)?;
                ensure!(
                    saved == expected,
                    "Full Save changed bytes or used the sample: {member_name}"
                );
                saves.push(json!({"decoded":decoded,"bytes":saved.len(),"sha256":hash(&saved),"completeNotSample":true}));
            }
            checks.push(json!({"resource":name,"member":member_name,"sourceBytes":original.len(),"mode":view["mode"],"shownBytes":view["shownBytes"],"truncated":view["truncated"],"saves":saves}));
        }
        ensure!(
            before == hash(&codec::range(&catalog, resource)?),
            "Source range changed"
        );
    }
    ensure!(
        checks.len() >= 7
            && checks.iter().any(|v| v["member"]
                .as_str()
                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".txtr"))),
        "Inspection fixture coverage incomplete"
    );
    std::fs::write(
        path,
        serde_json::to_vec_pretty(
            &json!({"pass":true,"sourceRangesUnchanged":true,"checks":checks}),
        )?,
    )?;
    Ok(())
}

/// Opt-in developer benchmark of the ordinary isolated-worker export command.
/// Source/output hashing and catalog loading are outside the timed operation.
/// Progress stays in memory, avoiding synchronous diagnostic trace-file writes.
pub fn benchmark(root: &str, cases_path: &str, report: &str) -> Result<()> {
    use std::io::Write;
    let path = Path::new(report);
    ensure!(!path.exists(), "Benchmark report already exists");
    let parent = path.parent().context("Report parent missing")?;
    std::fs::create_dir_all(parent)?;
    let names: Vec<String> = serde_json::from_slice(&std::fs::read(cases_path)?)?;
    ensure!(
        !names.is_empty() && names.len() <= 32,
        "Use 1-32 benchmark cases"
    );
    let mut unique = std::collections::HashSet::new();
    ensure!(
        names.iter().all(|n| unique.insert(n.to_ascii_lowercase())),
        "Duplicate benchmark case"
    );
    let catalog = Catalog::load(Path::new(root))?;
    let progress = Arc::new(Mutex::new(Vec::new()));
    let events = progress.clone();
    let mut session = Session::new(
        Arc::new(move |event| {
            if event["kind"] == "event" && event["channel"] == "progress" {
                if let Some(percent) = event["payload"]["percent"].as_u64() {
                    events.lock().unwrap().push(percent);
                }
            }
        }),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let loading = Instant::now();
    session.dispatch("game:load", &[json!(root)])?;
    let catalog_ms = loading.elapsed().as_secs_f64() * 1000.;
    let mut results = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let entry = catalog
            .entries
            .iter()
            .find(|e| e.name == *name)
            .context("Exact benchmark resource missing")?;
        let before = hash_range(&catalog, entry)?;
        let destination = parent.join(format!("{index:03}"));
        std::fs::create_dir(&destination)?;
        session.dispatch("export:set-root", &[json!(destination.canonicalize()?)])?;
        progress.lock().unwrap().clear();
        let started = Instant::now();
        let result = session.dispatch(
            "export:start",
            &[json!({"ids":[entry.id],"mode":"mod-ready"})],
        );
        let ms = started.elapsed().as_secs_f64() * 1000.;
        let after = hash_range(&catalog, entry)?;
        let result = result?;
        let outputs = result["results"]
            .as_array()
            .context("Missing benchmark outputs")?;
        ensure!(
            outputs.len() == 1,
            "Benchmark must produce exactly one selected output"
        );
        let output = outputs[0]["target"]
            .as_str()
            .context("Missing benchmark target")?;
        let output_hash = {
            use std::io::Read;
            let mut input = std::fs::File::open(output)?;
            let mut hasher = Sha256::new();
            let mut chunk = [0; 65536];
            loop {
                let count = input.read(&mut chunk)?;
                if count == 0 {
                    break;
                }
                hasher.update(&chunk[..count]);
            }
            format!("{:X}", hasher.finalize())
        };
        let progress = progress.lock().unwrap();
        let monotonic = progress.windows(2).all(|p| p[0] <= p[1]) && progress.last() == Some(&100);
        let pass = before == after
            && monotonic
            && result["failures"].as_array().is_some_and(Vec::is_empty)
            && outputs[0]["validation"]["status"] != "failed";
        println!("{name}: {ms:.2}ms; {}", if pass { "OK" } else { "FAILED" });
        results.push(json!({"name":name,"pass":pass,"ms":ms,"sourceUnchanged":before==after,"progressMonotonic":monotonic,"progressEvents":progress.len(),"result":result,"outputHash":output_hash}));
    }
    let pass = results.iter().all(|r| r["pass"] == true);
    let value = json!({"pass":pass,"catalogMs":catalog_ms,"timing":"Loaded-catalog ordinary export:start including isolated worker, destination/decode/validate/publication; source/output hashing excluded; no desktop UI or trace-file progress writes","results":results});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(&value)?)?;
    ensure!(pass, "Benchmark export checks failed; report retained");
    Ok(())
}

/// Opt-in live trace. Ordinary exports never write profiling sidecars.
pub fn profile(root: &str, name: &str, report: &str, mode: &str) -> Result<()> {
    ensure!(
        matches!(mode, "original" | "mod-ready"),
        "Unknown profile export mode"
    );
    use std::io::Write;
    let path = Path::new(report);
    let parent = path.parent().context("Report parent missing")?;
    std::fs::create_dir_all(parent)?;
    let catalog = Catalog::load(Path::new(root))?;
    let entry = catalog.resolve(name)?;
    let id = entry.id;
    let before = hash_range(&catalog, entry)?;
    let trace_path = path.with_extension("jsonl");
    let trace = Arc::new(Mutex::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&trace_path)?,
    ));
    let started = Instant::now();
    let recorded = trace.clone();
    let mut session = Session::new(
        Arc::new(move |event| {
            let mut file = recorded.lock().unwrap();
            let _ = serde_json::to_writer(
                &mut *file,
                &json!({"ms":started.elapsed().as_secs_f64()*1000.,"event":event}),
            );
            let _ = file.write_all(b"\n");
            let _ = file.flush();
        }),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    session.dispatch("game:load", &[json!(root)])?;
    session.dispatch("export:set-root", &[json!(parent.canonicalize()?)])?;
    let result = session.dispatch("export:start", &[json!({"ids":[id],"mode":mode})]);
    let after = hash_range(&catalog, entry)?;
    let report = match result {
        Ok(value) => {
            json!({"name":name,"ms":started.elapsed().as_secs_f64()*1000.,"sourceUnchanged":before==after,"result":value})
        }
        Err(e) => {
            json!({"name":name,"ms":started.elapsed().as_secs_f64()*1000.,"sourceUnchanged":before==after,"error":format!("{e:#}")})
        }
    };
    std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    ensure!(before == after, "Source range changed");
    ensure!(
        report["result"]["results"]
            .as_array()
            .is_some_and(|a| a.len() == 1)
            && report["result"]["failures"]
                .as_array()
                .is_some_and(Vec::is_empty),
        "Profiled export failed; diagnostic report preserved"
    );
    Ok(())
}

pub fn verify(root: &str, report: &str) -> Result<()> {
    let report_path = Path::new(report);
    let output = report_path.parent().context("Report parent missing")?;
    std::fs::create_dir_all(output)?;
    let catalog = Catalog::load(Path::new(root))?;
    let names = [
        "ball_gameplay_official_country_flag_china.iff",
        "ball_gameplay_official_country_flag_uae.iff",
        "ball_gameplay_official_default.iff",
        "png04090.iff",
        "png09403.iff",
        "png00392.iff",
        "shoes_adidas_dame8_geo.iff",
        "shoes_jordan_tatum3_geo.iff",
        "shoes_newbalance_twowxy5_geo.iff",
        "male_tshirt_ls_geo.iff",
        "female_jersey_basketball_nike_v_geo.iff",
        "png06572_geo_hair_curly_afro_tangentspace.iff",
        "png04174_geo_facialhair_01_tangentspace.iff",
        "png04174_geo_hair_short.iff",
        "arena_700_int.iff",
        "arena_700_int_floor.iff",
        "arena_005_int.iff",
        "character_builder_int.iff",
        "chr_g1156.iff",
        "nba_animations.iff",
    ];
    let mut ids = Vec::new();
    for name in names {
        let hits: Vec<_> = catalog
            .entries
            .iter()
            .filter(|e| e.name.rsplit('/').next() == Some(name))
            .collect();
        if name.ends_with("_geo.iff") {
            ensure!(
                !hits.is_empty() && hits.iter().all(|e| e.name.starts_with("clothing/geo/")),
                "Missing/ambiguous shoe fixture: {name}"
            );
            ids.extend(hits.iter().map(|e| e.id));
        } else {
            ensure!(hits.len() == 1, "Missing/ambiguous fixture: {name}");
            ids.push(hits[0].id);
        }
    }
    ids.extend(
        catalog
            .entries
            .iter()
            .filter(|e| e.name.ends_with(".gz"))
            .take(4)
            .map(|e| e.id),
    );
    let before: Vec<_> = ids
        .iter()
        .map(|id| Ok(hash(&codec::range(&catalog, catalog.entry(*id)?)?)))
        .collect::<Result<_>>()?;
    let events = Arc::new(Mutex::new(Vec::<u64>::new()));
    let recorded = events.clone();
    let mut session = Session::new(
        Arc::new(move |v| {
            if let Some(p) = v["payload"]["percent"].as_u64() {
                recorded.lock().unwrap().push(p);
            }
        }),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    session.dispatch("game:load", &[json!(root)])?;
    session.dispatch("export:set-root", &[json!(output.canonicalize()?)])?;
    ensure!(
        session
            .dispatch(
                "export:start",
                &[json!({"ids":vec![ids[0];5001],"mode":"original"})]
            )
            .is_err(),
        "Oversize batch accepted"
    );
    let started = Instant::now();
    let original = session.dispatch("export:start", &[json!({"ids":ids,"mode":"original"})])?;
    ensure!(
        original["failures"].as_array().is_some_and(Vec::is_empty),
        "Original family export failed: {}",
        original["failures"]
    );
    let oodle = Oodle::load(&catalog.root)?;
    let mut exact = Vec::new();
    for item in original["results"]
        .as_array()
        .context("Original results missing")?
    {
        let entry = catalog.entry(item["id"].as_u64().context("Result ID missing")? as usize)?;
        let packed = codec::range(&catalog, entry)?;
        let expected = if entry.name.ends_with(".iff") && codec::vcz(&packed) {
            crate::archive_decode::strict(&oodle, &catalog.root, &packed, &AtomicBool::new(false))?
        } else {
            packed
        };
        let actual = std::fs::read(item["target"].as_str().context("Target missing")?)?;
        ensure!(
            actual == expected,
            "Original output changed: {}",
            entry.name
        );
        exact.push(
            json!({"name":entry.name,"bytes":actual.len(),"sha256":hash(&actual),"byteExact":true}),
        );
    }
    let original_ms = started.elapsed().as_secs_f64() * 1000.;
    events.lock().unwrap().clear();
    let started = Instant::now();
    let converted = session.dispatch("export:start", &[json!({"ids":ids,"mode":"mod-ready"})])?;
    let progress = events.lock().unwrap().clone();
    let monotonic = progress.windows(2).all(|p| p[0] <= p[1])
        && progress.first() == Some(&0)
        && progress.last() == Some(&100);
    let converted_ms = started.elapsed().as_secs_f64() * 1000.;
    let mut buffers = Vec::new();
    for item in converted["results"]
        .as_array()
        .context("Mod-ready results missing")?
    {
        let name = item["name"].as_str().context("Name missing")?;
        if !name.ends_with(".gz") {
            continue;
        }
        let entry = catalog.entry(item["id"].as_u64().context("ID missing")? as usize)?;
        let packed = codec::range(&catalog, entry)?;
        let expected_size = codec::u32_at(&packed, packed.len() - 4)? as usize;
        let expected = oodle.buffer(&packed, expected_size)?;
        let actual = std::fs::read(item["target"].as_str().context("Buffer target missing")?)?;
        ensure!(
            item["target"].as_str().unwrap().ends_with(".bin") && actual == expected,
            "Standalone decode differs from exact-size Oodle path: {name}"
        );
        buffers.push(json!({"name":name,"bytes":actual.len(),"sha256":hash(&actual),"exactSizeDecodeMatches":true,"trailerCrcVerified":true}));
    }
    let dedup = session.dispatch(
        "export:start",
        &[json!({"ids":vec![ids[0];5000],"mode":"original"})],
    )?;
    ensure!(
        dedup["results"].as_array().is_some_and(|r| r.len() == 1),
        "5000 duplicate IDs were not deduplicated"
    );
    let after: Vec<_> = ids
        .iter()
        .map(|id| Ok(hash(&codec::range(&catalog, catalog.entry(*id)?)?)))
        .collect::<Result<_>>()?;
    ensure!(before == after, "Source ranges changed");
    let pass = converted["failures"].as_array().is_some_and(Vec::is_empty)
        && monotonic
        && buffers.len() == 4;
    let report = json!({"pass":pass,"sourceRangesUnchanged":true,"originalByteExact":exact,"standaloneBuffers":buffers,"progressMonotonic":monotonic,"progress":progress,"batch5000Deduplicated":true,"batch5001Rejected":true,"originalMs":original_ms,"modReadyMs":converted_ms,"results":converted["results"],"failures":converted["failures"],"manualAcceptance":"Native dialogs/clipboard, another Windows machine and in-game loading still require user testing."});
    std::fs::write(report_path, serde_json::to_vec_pretty(&report)?)?;
    ensure!(
        pass,
        "Family exports did not all pass; inspect the saved report"
    );
    Ok(())
}
