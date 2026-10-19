//! On-demand WPF ball thumbnail fallback. Reads one small authored store
//! thumbnail through the existing bounded archive decoder; writes only256px PNG cache.
use crate::fit_assets::FitAssets;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read, Write},
    path::Path,
    sync::{Condvar, Mutex, OnceLock},
};

const PACKED_LIMIT: u64 = 4 * 1024 * 1024;
const DDS_LIMIT: u64 = 2 * 1024 * 1024;
struct Service {
    assets: FitAssets,
    active: Mutex<HashSet<String>>,
    conversions: Mutex<usize>,
    ready: Condvar,
}
struct Work<'a> {
    service: &'a Service,
    key: String,
    slot: bool,
}
impl Drop for Work<'_> {
    fn drop(&mut self) {
        if self.slot {
            let mut conversions = self
                .service
                .conversions
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *conversions = conversions.saturating_sub(1);
            self.service.ready.notify_all();
        }
        self.service
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}
impl Service {
    fn claim(&self, key: String) -> Result<Option<Work<'_>>> {
        {
            let mut active = self
                .active
                .lock()
                .map_err(|_| anyhow::anyhow!("Ball preview cache unavailable"))?;
            if active.len() >= 64 || !active.insert(key.clone()) {
                return Ok(None);
            }
        }
        // Guard the accepted key before any fallible semaphore operation.
        let mut work = Work {
            service: self,
            key,
            slot: false,
        };
        let mut conversions = self
            .conversions
            .lock()
            .map_err(|_| anyhow::anyhow!("Ball preview queue unavailable"))?;
        while *conversions >= 4 {
            conversions = self
                .ready
                .wait(conversions)
                .map_err(|_| anyhow::anyhow!("Ball preview queue unavailable"))?;
        }
        *conversions += 1;
        work.slot = true;
        Ok(Some(work))
    }
}
fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(|| Service {
        assets: FitAssets::default(),
        active: Mutex::new(HashSet::new()),
        conversions: Mutex::new(0),
        ready: Condvar::new(),
    })
}
fn ids() -> Result<&'static HashSet<String>> {
    static IDS: OnceLock<HashSet<String>> = OnceLock::new();
    if let Some(ids) = IDS.get() {
        return Ok(ids);
    }
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("../assets/choices.json.gz")[..])
        .read_to_end(&mut bytes)?;
    let choices: Value = serde_json::from_slice(&bytes)?;
    let values = choices["ball"]
        .as_array()
        .context("Ball catalog missing")?
        .iter()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect();
    let _ = IDS.set(values);
    Ok(IDS.get().unwrap())
}
/// WPF ball-selection-manager.js previewCandidates, preserving priority order.
fn candidates(id: &str) -> Vec<String> {
    let stem = id
        .strip_prefix("balls/")
        .unwrap_or(id)
        .strip_suffix(".iff")
        .unwrap_or(id);
    let stem = stem.strip_prefix("ball_").unwrap_or(stem);
    let stem = stem.strip_prefix("gameplay_").unwrap_or(stem);
    let stem = stem.strip_prefix("official_").unwrap_or(stem);
    let parts: Vec<_> = stem.split('_').filter(|s| !s.is_empty()).collect();
    let category = parts.first().copied().unwrap_or("");
    let rest = parts.iter().skip(1).copied().collect::<Vec<_>>().join("_");
    let last = parts.last().copied().unwrap_or("");
    let mut names = Vec::new();
    if category == "food" {
        names.push(format!("{last}_fruit_ball"));
    }
    if category == "cloth" {
        names.push(format!("ai_ball_texture_{rest}"));
    }
    names.extend([
        stem.to_owned(),
        format!("ball_{stem}"),
        format!("{stem}_ball"),
    ]);
    if !rest.is_empty() {
        names.extend([
            rest.clone(),
            format!("ball_{category}_{rest}"),
            format!("{category}_{rest}_ball"),
            format!("{rest}_ball"),
            format!("ball_{rest}"),
            format!("ai_ball_texture_{rest}"),
        ]);
    }
    names.push(format!("ai_ball_texture_{stem}"));
    if parts.len() == 2 {
        names.push(format!("{}_{}_ball", parts[1], parts[0]));
    }
    if category == "country" {
        let country = parts
            .iter()
            .skip(if parts.get(1) == Some(&"flag") { 2 } else { 1 })
            .copied()
            .collect::<Vec<_>>()
            .join("_");
        names.extend([format!("country_{country}_ball"), format!("{country}_ball")]);
    }
    if category == "animal" {
        names.extend([
            format!("{last}_ball"),
            format!("{last}_skin_ball"),
            format!("{last}_scale_ball"),
        ]);
    }
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|s| !s.is_empty() && seen.insert(s.clone()))
        .map(|s| format!("store/thumbnails/{s}.iff"))
        .collect()
}
fn decoded_png(container: Vec<u8>) -> Result<Option<Vec<u8>>> {
    ensure!(
        container.len() as u64 <= PACKED_LIMIT,
        "Ball thumbnail container exceeds4MB"
    );
    let mut zip = zip::ZipArchive::new(Cursor::new(container))?;
    ensure!(
        zip.len() <= 128,
        "Ball thumbnail member count exceeds limit"
    );
    let textures: Vec<_> = (0..zip.len())
        .filter(|i| {
            zip.name_for_index(*i)
                .is_some_and(|n| n.to_ascii_lowercase().ends_with(".dds"))
        })
        .collect();
    if textures.len() != 1 {
        return Ok(None);
    }
    let mut member = zip.by_index(textures[0])?;
    ensure!(
        member.size() <= DDS_LIMIT,
        "Ball thumbnail texture exceeds2MB"
    );
    let mut dds = Vec::new();
    member.by_ref().take(DDS_LIMIT + 1).read_to_end(&mut dds)?;
    ensure!(
        dds.len() as u64 == member.size(),
        "Ball thumbnail texture size changed"
    );
    let (width, height, rgba) = crate::preview_service::decode_dds(&dds)?;
    let rgba = crate::roster_artwork::resize(width as usize, height as usize, &rgba, 256, 256);
    Ok(Some(crate::preview_service::encode_png(256, 256, &rgba)?))
}
fn valid_png(bytes: &[u8]) -> bool {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: 1024 * 1024 });
    let Ok(mut reader) = decoder.read_info() else {
        return false;
    };
    if reader.info().width != 256 || reader.info().height != 256 {
        return false;
    }
    let Some(size) = reader.output_buffer_size().filter(|n| *n <= 1024 * 1024) else {
        return false;
    };
    reader.next_frame(&mut vec![0; size]).is_ok()
}
fn cached_png(path: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes).is_ok()
        && bytes.len() <= 1024 * 1024
        && valid_png(&bytes)
}
/// Returns missing/busy on unavailable optional art. The UI keeps its ball placeholder.
pub(crate) fn thumbnail(root: &Path, game_root: &Path, id: &str) -> Result<Value> {
    ensure!(ids()?.contains(id), "Unknown ball preview");
    if id == "current" {
        return Ok(json!({"state":"missing"}));
    }
    let service = service();
    let game = service.assets.game(game_root)?;
    let Some(name) = candidates(id).into_iter().find(|name| {
        game.size(name)
            .is_some_and(|size| size > 0 && size <= PACKED_LIMIT)
    }) else {
        return Ok(json!({"state":"missing"}));
    };
    let source = game.key(&name)?;
    let key = format!(
        "{:x}",
        Sha256::digest(format!("ball-thumb256-v1:{source}:{name}"))
    );
    let dir = root.join("Cache/PreviewImages/ball-thumbnails");
    let path = dir.join(format!("{key}.png"));
    if cached_png(&path) {
        return Ok(json!({"state":"ready","imagePath":path}));
    }
    let Some(_work) = service.claim(key.clone())? else {
        return Ok(json!({"state":"busy"}));
    };
    let result = (|| -> Result<Value> {
        let container = game.export(&name, &|| Ok(()))?;
        let Some(bytes) = decoded_png(container)? else {
            return Ok(json!({"state":"missing"}));
        };
        ensure!(
            game.key(&name)? == source,
            "Ball preview source changed during decoding"
        );
        fs::create_dir_all(&dir)?;
        let temp = dir.join(format!(".{key}.{}.tmp", crate::backend::unique_id()));
        let published = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, &path)?;
            Ok(())
        })();
        if published.is_err() {
            let _ = fs::remove_file(&temp);
        }
        published?;
        Ok(json!({"state":"ready","imagePath":path}))
    })();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidates_match_wpf_special_cases_and_catalog_is_strict() {
        assert!(
            ids()
                .unwrap()
                .contains("balls/ball_gameplay_official_allstar_allstar.iff")
        );
        assert!(!ids().unwrap().contains("../store/thumbnails/evil.iff"));
        assert_eq!(
            candidates("balls/ball_gameplay_official_food_apple.iff")[0],
            "store/thumbnails/apple_fruit_ball.iff"
        );
        assert_eq!(
            candidates("balls/ball_gameplay_official_cloth_denim.iff")[0],
            "store/thumbnails/ai_ball_texture_denim.iff"
        );
        assert!(
            candidates("balls/ball_gameplay_official_country_flag_usa.iff")
                .contains(&"store/thumbnails/country_usa_ball.iff".into())
        );
        assert!(
            candidates("balls/ball_gameplay_official_animal_tiger.iff")
                .contains(&"store/thumbnails/tiger_skin_ball.iff".into())
        );
    }
    #[test]
    fn pending_key_is_released_on_semaphore_error_and_conversion_panic() {
        fn pool() -> Service {
            Service {
                assets: FitAssets::default(),
                active: Mutex::new(HashSet::new()),
                conversions: Mutex::new(0),
                ready: Condvar::new(),
            }
        }
        let poisoned = pool();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _lock = poisoned.conversions.lock().unwrap();
            panic!("poison fixture");
        }));
        assert!(poisoned.claim("failed".into()).is_err());
        assert!(poisoned.active.lock().unwrap().is_empty());
        let pool = pool();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _work = pool.claim("panic".into()).unwrap().unwrap();
            assert_eq!(*pool.conversions.lock().unwrap(), 1);
            panic!("conversion fixture");
        }));
        assert!(pool.active.lock().unwrap().is_empty());
        assert_eq!(*pool.conversions.lock().unwrap(), 0);
        assert!(pool.claim("panic".into()).unwrap().is_some());
    }
    fn zip_dds(dds: &[u8], count: usize) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for n in 0..count {
            zip.start_file(
                format!("ball{n}.dds"),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(dds).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn valid_bc1_dds_blocks_decode_to_256px_png_and_ambiguous_zip_is_missing() {
        // Authored valid BC1 block: red RGB565 endpoint, all16texels index0.
        let mut dds = vec![0; 136];
        dds[..4].copy_from_slice(b"DDS ");
        dds[12..16].copy_from_slice(&4u32.to_le_bytes());
        dds[16..20].copy_from_slice(&4u32.to_le_bytes());
        dds[84..88].copy_from_slice(b"DXT1");
        dds[128..130].copy_from_slice(&0xF800u16.to_le_bytes());
        let png = decoded_png(zip_dds(&dds, 1)).unwrap().unwrap();
        assert!(valid_png(&png));
        let mut reader = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert!(pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
        assert!(decoded_png(zip_dds(&dds, 2)).unwrap().is_none());
        dds.truncate(128);
        assert!(decoded_png(zip_dds(&dds, 1)).is_err());
    }
    #[test]
    #[ignore = "explicit read-only game archive preview audit; PNG cache only, no process access"]
    fn installed_ball_thumbnail_cold_and_warm_cache() -> Result<()> {
        let game_root = std::path::PathBuf::from(std::env::var("NBA2K27_BALL_FIXTURE_GAME")?);
        let root = std::path::PathBuf::from(std::env::var("NBA2K27_BALL_FIXTURE_ROOT")?);
        ensure!(!root.exists(), "Use a fresh fixture folder");
        let assets = FitAssets::default();
        let game = assets.game(&game_root)?;
        let mut selected: Vec<_> = ids()?
            .iter()
            .filter(|id| {
                id.as_str() != "current"
                    && candidates(id).iter().any(|name| {
                        game.size(name)
                            .is_some_and(|size| size > 0 && size <= PACKED_LIMIT)
                    })
            })
            .cloned()
            .collect();
        selected.sort();
        selected.truncate(3);
        ensure!(
            selected.len() == 3,
            "Need three real allowlisted thumbnail samples"
        );
        for id in selected {
            let started = std::time::Instant::now();
            let cold = thumbnail(&root, &game_root, &id)?;
            ensure!(
                cold["state"] == "ready",
                "Real thumbnail was not decoded: {id}"
            );
            let cold_ms = started.elapsed().as_millis();
            let path =
                std::path::PathBuf::from(cold["imagePath"].as_str().context("Missing image path")?);
            ensure!(
                path.starts_with(&root) && cached_png(&path),
                "Invalid real preview cache"
            );
            let before = fs::metadata(&path)?.modified()?;
            let started = std::time::Instant::now();
            let warm = thumbnail(&root, &game_root, &id)?;
            ensure!(
                warm == cold && fs::metadata(&path)?.modified()? == before,
                "Warm cache was rewritten"
            );
            println!(
                "REAL_BALL={id} COLD_MS={cold_ms} WARM_MS={} PNG={}",
                started.elapsed().as_millis(),
                path.display()
            );
        }
        Ok(())
    }
    #[test]
    #[ignore = "explicit full read-only game archive preview audit; writes only isolated PNG cache and report"]
    fn installed_ball_thumbnail_catalog_audit() -> Result<()> {
        let game_root = std::path::PathBuf::from(std::env::var("NBA2K27_BALL_FIXTURE_GAME")?);
        let root = std::path::PathBuf::from(std::env::var("NBA2K27_BALL_FIXTURE_ROOT")?);
        ensure!(!root.exists(), "Use a fresh fixture folder");
        let game = service().assets.game(&game_root)?;
        let mut selected: Vec<_> = ids()?
            .iter()
            .filter(|id| id.as_str() != "current")
            .cloned()
            .collect();
        selected.sort();
        let started = std::time::Instant::now();
        let mut rows = Vec::new();
        let (mut mapped, mut ready, mut failed) = (0, 0, 0);
        for id in selected {
            let source = candidates(&id).into_iter().find(|name| {
                game.size(name)
                    .is_some_and(|size| size > 0 && size <= PACKED_LIMIT)
            });
            if source.is_some() {
                mapped += 1;
            }
            let began = std::time::Instant::now();
            let result = thumbnail(&root, &game_root, &id);
            let row = match result {
                Ok(value) => {
                    if value["state"] == "ready" {
                        ready += 1;
                    } else if source.is_some() {
                        failed += 1;
                    }
                    json!({"id":id,"source":source,"result":value,"milliseconds":began.elapsed().as_millis()})
                }
                Err(error) => {
                    failed += 1;
                    json!({"id":id,"source":source,"error":error.to_string(),"milliseconds":began.elapsed().as_millis()})
                }
            };
            rows.push(row);
        }
        fs::create_dir_all(&root)?;
        let report = json!({"schema":"qa066-real-ball-thumbnail-catalog-v1","rows":rows,
            "mapped":mapped,"ready":ready,"failed":failed,"elapsedMilliseconds":started.elapsed().as_millis()});
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join("report.json"))?;
        output.write_all(&serde_json::to_vec_pretty(&report)?)?;
        output.sync_all()?;
        println!(
            "REAL_BALL_CATALOG MAPPED={mapped} READY={ready} FAILED={failed} REPORT={}",
            root.join("report.json").display()
        );
        ensure!(
            mapped > 0 && ready == mapped && failed == 0,
            "Some mapped real thumbnails could not be decoded"
        );
        Ok(())
    }
}
