// Preview-only port of the stable PlayerCompanions / TXTR metadata path.
// No disk cache, export mutation, inferred skin tint, or bundled game data.
use crate::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
    textures,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
    sync::atomic::AtomicBool,
};

pub(crate) fn local_companion(
    directory: Option<&Path>,
    name: &str,
    catalog: &Catalog,
    oodle: &Oodle,
    cancel: &AtomicBool,
) -> Result<Option<Archive>> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    let directory = directory.canonicalize()?;
    let leaf = name.rsplit('/').next().unwrap_or(name);
    let lower = leaf.to_ascii_lowercase();
    if leaf.is_empty()
        || leaf.contains(['\\', '/'])
        || !(lower.ends_with(".iff") || lower.ends_with(".iff.bin"))
    {
        return Ok(None);
    }
    let candidate = directory.join(leaf);
    if !candidate.is_file() {
        return Ok(None);
    }
    let candidate = candidate.canonicalize()?;
    ensure!(
        candidate.starts_with(&directory),
        "Loose companion escaped the selected mod folder"
    );
    let metadata = candidate.metadata()?;
    ensure!(
        metadata.len() <= crate::container::MAX as u64,
        "Loose companion exceeds the 1 GiB preview limit"
    );
    codec::cancelled(cancel)?;
    let packed = fs::read(&candidate)?;
    ensure!(
        packed.len() as u64 == metadata.len(),
        "Loose companion changed while it was being read"
    );
    let bytes = if packed.starts_with(b"PK\x03\x04") {
        packed
    } else {
        crate::archive_decode::preview(oodle, &catalog.root, &packed, cancel)?
    };
    let archive = Archive::open_with_cancel(bytes, cancel)?;
    archive.require_unique_members()?;
    Ok(Some(archive))
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn token(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}
fn scalar(v: &Value) -> String {
    v.as_str()
        .map(str::to_string)
        .unwrap_or_else(|| v.to_string())
}

pub(crate) fn range(metadata: &Value) -> Value {
    let (Some(min), Some(max)) = (metadata["Min"].as_array(), metadata["Max"].as_array()) else {
        return Value::Null;
    };
    if min.len() < 4 || max.len() < 4 {
        return Value::Null;
    }
    let min: Option<Vec<f64>> = min.iter().take(4).map(Value::as_f64).collect();
    let max: Option<Vec<f64>> = max.iter().take(4).map(Value::as_f64).collect();
    let (Some(min), Some(max)) = (min, max) else {
        return Value::Null;
    };
    if !min.iter().chain(&max).all(|n| n.is_finite()) {
        return Value::Null;
    }
    let compress = metadata["RangeCompress"].as_u64().unwrap_or(0);
    let mask: Vec<u8> = (0..4)
        .map(|i| {
            u8::from(if compress != 0 {
                compress & (1 << i) != 0
            } else {
                min[i].abs() > 1e-7 || (max[i] - 1.).abs() > 1e-7
            })
        })
        .collect();
    if mask.iter().all(|v| *v == 0) {
        Value::Null
    } else {
        json!({"min":min,"max":max,"mask":mask})
    }
}
fn collect(v: &Value, out: &mut BTreeMap<String, Value>, depth: usize) -> Result<()> {
    ensure!(
        depth <= 128,
        "Player texture metadata nesting exceeds limit"
    );
    if let Some(map) = v.as_object() {
        let name = text(&v["Binary"]);
        if name.to_lowercase().ends_with(".tld") || name.to_lowercase().ends_with(".dds") {
            out.insert(name.into(), v.clone());
        }
        for child in map.values() {
            collect(child, out, depth + 1)?;
        }
    }
    if let Some(arr) = v.as_array() {
        for child in arr {
            collect(child, out, depth + 1)?;
        }
    }
    Ok(())
}
fn binding(name: &str) -> (&'static str, &'static str) {
    let target = if name.contains("eyewhite") || name.contains("iris_") {
        "eye"
    } else if name.contains("lash_") {
        "eyelash"
    } else if name.contains("armpithair") {
        "armpit"
    } else if name.contains("face_") {
        "face"
    } else if name.contains("chest_") {
        "chest"
    } else if name.contains("torso_") {
        "torso"
    } else if name.contains("legs_") {
        "legs"
    } else if name.contains("facialhair") || name.contains("beard") {
        "facialhair"
    } else if name.contains("hair") {
        "hair"
    } else {
        ""
    };
    let channel = if name.contains("wrinkle")
        || name.contains("bentnormal")
        || name.contains("coneangle")
        || name.contains("hairmask")
        || name.contains("tangent")
        || name.contains("lightmap")
        || name.contains("occlusion")
    {
        "auxiliary"
    } else if name.contains("normal") || name.contains("_bump") {
        "normal"
    } else if name.contains("color") || name.contains("albedo") || name.contains("diffuse") {
        "color"
    } else {
        "auxiliary"
    };
    (target, channel)
}
fn source_binding(name: &str, source: &str) -> (&'static str, &'static str) {
    let (mut target, mut channel) = binding(name);
    let path = source.to_lowercase();
    if path.contains("hair_underpaint_") {
        target = if path.contains("_fins_") {
            "hair"
        } else {
            "hair-underpaint"
        };
    } else if path.contains("facialhair_") && path.contains("_underpaint_") {
        target = "facialhair-underpaint";
    } else if path.rsplit('/').next().is_some_and(|n| {
        n.starts_with("fh_")
            || n.starts_with("beard_")
            || n.starts_with("fullbeard_")
            || n.starts_with("fullgoatee_")
    }) {
        target = "facialhair";
    }
    if path.ends_with("_fins_color.iff") {
        channel = "color";
    }
    if path.ends_with("_bump.iff") || path.ends_with("_underpaint_normal.iff") {
        channel = "normal";
    }
    if [
        "_fins_tangent.iff",
        "_fins_lightmap.iff",
        "_fins_occlusion.iff",
        "_bentnormal.iff",
        "_coneangle.iff",
        "_hairmask.iff",
    ]
    .iter()
    .any(|n| path.ends_with(n))
    {
        channel = "auxiliary";
    }
    (target, channel)
}
pub fn hair_sources(file: &str) -> Vec<String> {
    let file = file.replace('\\', "/").to_lowercase();
    let stem = file
        .rsplit('/')
        .next()
        .unwrap_or(&file)
        .trim_end_matches(".iff");
    let folder = file.rsplit_once('/').map(|p| p.0).unwrap_or("");
    let mut out = Vec::new();
    if let Ok(re) = regex::Regex::new(r"^(png\d+)_geo_(.+)$") {
        if let Some(c) = re.captures(stem) {
            let token = c[2].trim_end_matches("_tangentspace");
            out.push(format!("{folder}/{}_item_{token}.iff", &c[1]));
        }
    }
    if folder == "char/pcreate/full" {
        if let Ok(re) = regex::Regex::new(
            r"^hair_(.+?)_(?:geo|hat|headband)(?:_tangentspace(?:_(?:child|female|teen))?)?$",
        ) {
            if let Some(c) = re.captures(stem) {
                let style = &c[1];
                let prefix = format!("char/pcreate/hair_underpaint_{style}_colorhair_{style}");
                for suffix in [
                    "fins_color",
                    "fins_tangent",
                    "fins_lightmap",
                    "fins_occlusion",
                    "underpaint_normal",
                    "underpaint_bentnormal",
                    "underpaint_coneangle",
                    "underpaint_hairmask",
                ] {
                    out.push(format!("{prefix}_{suffix}.iff"));
                }
                out.push(format!("char/pcreate/hair_underpaint_{style}_color.iff"));
            }
        }
        if let Some(style) = stem.strip_prefix("fh_") {
            let style = style.trim_end_matches("_tangentspace");
            for suffix in ["color", "bump", "bentnormal", "hairmask"] {
                out.push(format!("char/pcreate/{style}_{suffix}.iff"));
            }
            for suffix in ["normal", "bentnormal", "coneangle", "hairmask"] {
                out.push(format!(
                    "char/pcreate/{style}_colorfacialhair_{style}_underpaint_{suffix}.iff"
                ));
            }
        }
    }
    out
}
pub(crate) fn metadata_for(archive: &mut Archive, name: &str) -> Value {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    let stem = leaf.rsplit_once('.').map(|p| p.0).unwrap_or(leaf);
    let logical = stem
        .rsplit_once('.')
        .filter(|(_, hash)| hash.len() == 16 && hash.bytes().all(|c| c.is_ascii_hexdigit()))
        .map(|p| p.0)
        .unwrap_or(stem);
    archive
        .named(&format!("{logical}.TXTR"))
        .ok()
        .and_then(|b| codec::tree(&b).ok())
        .and_then(|v| v.as_object().and_then(|m| m.values().next()).cloned())
        .unwrap_or(Value::Null)
}
/// SCNE texture descriptors can carry the authored range directly, with no
/// companion TXTR member. Keep explicit TXTR precedence, including identity
/// ranges; this affects display metadata only, never reconstructed DDS bytes.
pub(crate) fn preview_metadata<'a>(
    archive: &mut Archive,
    name: &str,
    descriptor: &'a Value,
) -> Cow<'a, Value> {
    let metadata = metadata_for(archive, name);
    if metadata.is_null() {
        Cow::Borrowed(descriptor)
    } else {
        Cow::Owned(metadata)
    }
}
struct Loaded {
    items: Vec<Value>,
    warnings: Vec<Value>,
    bytes: usize,
    seen: HashSet<String>,
}
impl Loaded {
    fn add(
        &mut self,
        archive: &mut Archive,
        catalog: &Catalog,
        oodle: &Oodle,
        name: &str,
        info: &Value,
        source: &str,
        cancel: &AtomicBool,
        override_binding: Option<(&str, &str)>,
    ) -> Result<()> {
        codec::cancelled(cancel)?;
        let lower = name.to_lowercase();
        let (target, channel) = override_binding.unwrap_or_else(|| source_binding(&lower, source));
        let tangent = (target == "hair" || target == "facialhair")
            && (lower.contains("tangent") || source.contains("_fins_tangent"));
        if target.is_empty() || (channel == "auxiliary" && !tangent && target != "armpit") {
            return Ok(());
        }
        if self.seen.contains(&lower) {
            if let Some(item) = self
                .items
                .iter_mut()
                .find(|v| text(&v["resourceName"]).eq_ignore_ascii_case(name))
            {
                let sources = item["sources"].as_array_mut().unwrap();
                if !sources.iter().any(|v| v.as_str() == Some(source)) {
                    sources.push(json!(source));
                }
            }
            return Ok(());
        }
        if self.items.len() >= 32 {
            self.warnings
                .push(json!({"name":name,"reason":"Player preview texture-count budget"}));
            return Ok(());
        }
        match textures::reconstruct(
            archive,
            catalog,
            oodle,
            name,
            info,
            cancel,
            16 * 1024 * 1024,
        ) {
            Ok(mut dds) => {
                if target == "armpit" {
                    let tld = archive
                        .named(name)
                        .or_else(|_| codec::range(catalog, catalog.resolve(name)?))?;
                    restore_armpit_raw_segments(&tld, &mut dds)?;
                }
                if self.bytes + dds.len() > 64 * 1024 * 1024 {
                    self.warnings
                        .push(json!({"name":name,"reason":"Player preview 64 MiB texture budget"}));
                    return Ok(());
                }
                let meta = preview_metadata(archive, name, info);
                let mut item = crate::backend::texture(name, &dds);
                let data = item.as_object_mut().unwrap().remove("data").unwrap();
                item["bytes"] = data;
                item["range"] = range(&meta);
                item["format"] = json!(
                    meta["Format"]
                        .as_str()
                        .or_else(|| info["Format"].as_str())
                        .unwrap_or("DDS")
                );
                item["binding"] = json!({"target":target,"channel":channel});
                item["source"] = json!(source);
                item["resourceName"] = json!(name);
                item["sources"] = json!([source]);
                item["name"] = json!(
                    name.strip_suffix(".tld")
                        .map(|s| format!("{s}.dds"))
                        .unwrap_or(name.into())
                );
                self.bytes += dds.len();
                self.seen.insert(lower);
                self.items.push(item);
            }
            Err(e) => self
                .warnings
                .push(json!({"name":name,"reason":format!("Player texture: {e:#}")})),
        }
        Ok(())
    }
    fn source(
        &mut self,
        archive: &mut Archive,
        catalog: &Catalog,
        oodle: &Oodle,
        source: &str,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let members = archive.names()?;
        let mut descriptors = BTreeMap::new();
        for member in &members {
            codec::cancelled(cancel)?;
            let name = text(&member["name"]).to_lowercase();
            if name.ends_with(".scne") || name.ends_with(".txtr") {
                if let Ok(tree) = archive
                    .read(member["id"].as_u64().unwrap() as usize, 32 * 1024 * 1024)
                    .and_then(|b| codec::tree(&b))
                {
                    collect(&tree, &mut descriptors, 0)?;
                }
            }
            if name.ends_with(".dds") || name.ends_with(".tld") {
                descriptors
                    .entry(text(&member["name"]).into())
                    .or_insert(Value::Null);
            }
        }
        // Colours before normals, as in the stable loader; do not let auxiliaries crowd out skin.
        let mut ordered: Vec<_> = descriptors.into_iter().collect();
        ordered.sort_by_key(|(name, _)| binding(&name.to_lowercase()).1 != "color");
        for (name, info) in ordered {
            self.add(archive, catalog, oodle, &name, &info, source, cancel, None)?;
        }
        Ok(())
    }
}
// These four authored armpit atlases mix BC7Prep large-mip segments with an
// already-interleaved raw small-mip tail (InflatedSize=0). The shared standalone
// decoder applies generic plane interleaving to that tail. Undo only that step
// for this preview binding; neither the decoder nor any exported DDS is changed.
fn restore_armpit_raw_segments(tld: &[u8], dds: &mut [u8]) -> Result<usize> {
    ensure!(
        tld.len() >= 32 && tld.starts_with(b"TLD") && tld[10] == 98,
        "Unexpected armpit TLD"
    );
    ensure!(
        dds.len() >= 148
            && dds.starts_with(b"DDS ")
            && &dds[84..88] == b"DX10"
            && codec::u32_at(dds, 128)? == 98,
        "Unexpected armpit DDS"
    );
    let end = 32usize
        .checked_add(codec::u32_at(tld, 28)? as usize)
        .ok_or_else(|| anyhow::anyhow!("Armpit tail overflow"))?;
    ensure!(
        end < tld.len() && (tld.len() - end) % 24 == 0,
        "Missing armpit segment table"
    );
    let mut restored = 0;
    for p in (end..tld.len()).step_by(24) {
        if codec::u32_at(tld, p + 16)? != 0 {
            continue;
        }
        let offset = codec::u32_at(tld, p + 8)? as usize;
        let length = codec::u32_at(tld, p + 12)? as usize;
        ensure!(
            length > 0
                && length % 16 == 0
                && offset <= dds.len() - 148
                && length <= dds.len() - 148 - offset,
            "Invalid raw armpit segment"
        );
        let data = &mut dds[148 + offset..148 + offset + length];
        let interleaved = data.to_vec();
        let blocks = length / 16;
        for b in 0..blocks {
            for plane in 0..4 {
                data[plane * blocks * 4 + b * 4..plane * blocks * 4 + b * 4 + 4]
                    .copy_from_slice(&interleaved[b * 16 + plane * 4..b * 16 + plane * 4 + 4]);
            }
        }
        restored += 1;
    }
    Ok(restored)
}
fn recipe(file: &str, appearance: &Value) -> (Value, Vec<String>) {
    let folder = file.rsplit_once('/').map(|p| p.0).unwrap_or("");
    let stem = file
        .rsplit('/')
        .next()
        .unwrap_or(file)
        .trim_end_matches(".iff");
    let accessory = &appearance["accessory_items"];
    let config = crate::player_config::default_name(appearance);
    let body = appearance.get("appearance").unwrap_or(appearance);
    let gender = token(text(&body["gender"]));
    let gender = if gender.starts_with("female") {
        "female"
    } else if gender.starts_with("male") {
        "male"
    } else if gender.starts_with("child") {
        "child"
    } else {
        ""
    };
    let skin = scalar(
        body.get("skin color")
            .or_else(|| body.get("skin_color"))
            .unwrap_or(&Value::Null),
    )
    .trim()
    .parse::<u32>()
    .ok()
    .filter(|n| *n <= 99);
    let mut sources = Vec::new();
    if !config.is_empty() {
        sources.push(format!("{folder}/{stem}_config_{}.iff", token(config)));
        if let Some(chosen) = accessory["configurations"].as_array().and_then(|a| {
            a.iter()
                .find(|v| text(&v["name"]).eq_ignore_ascii_case(config))
        }) {
            if let Some(items) = chosen["items"].as_array() {
                for item in items {
                    let geometry_only = accessory["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|record| text(&record["name"]).eq_ignore_ascii_case(text(item)))
                        .is_some_and(|record| record["texture"].is_null());
                    if geometry_only {
                        continue;
                    }
                    let item = token(text(item));
                    if !item.is_empty() {
                        sources.push(format!("{folder}/{stem}_item_{item}.iff"));
                    }
                }
            }
        }
    }
    if !gender.is_empty() {
        sources.push(format!("char/{gender}_skin.iff"));
    }
    if let Some(skin) = skin {
        for part in ["chest", "torso", "legs"] {
            sources.push(format!(
                "char/{}{part}_color_{skin:02}.iff",
                if gender == "female" { "female_" } else { "" }
            ));
        }
    }
    (
        json!({"mode":"player","defaultConfig":config,"gender":gender,"skinColor":skin,"irisColor":body["iris color"],"irisSize":body["iris size"]}),
        sources,
    )
}

pub fn apply(
    payload: &mut Value,
    archive: &mut Archive,
    catalog: &Catalog,
    oodle: &Oodle,
    file: &str,
    selected_config: Option<&str>,
    loose_directory: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<()> {
    let leaf = file.rsplit('/').next().unwrap_or(file).to_lowercase();
    let stem = leaf.trim_end_matches(".iff");
    if !leaf.ends_with(".iff")
        || !stem
            .strip_prefix("png")
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
    {
        let sources = hair_sources(file);
        if !sources.is_empty() {
            let mut loaded = Loaded {
                items: vec![],
                warnings: vec![],
                bytes: 0,
                seen: HashSet::new(),
            };
            loaded.source(archive, catalog, oodle, file, cancel)?;
            for source in sources {
                codec::cancelled(cancel)?;
                let Ok(resource) = catalog.resolve(&source) else {
                    continue;
                };
                let result = (|| -> Result<()> {
                    let bytes = crate::archive_decode::preview(
                        oodle,
                        &catalog.root,
                        &codec::range(catalog, resource)?,
                        cancel,
                    )?;
                    let mut companion = Archive::open_with_cancel(bytes, cancel)?;
                    companion.require_unique_members()?;
                    loaded.source(&mut companion, catalog, oodle, &source, cancel)
                })();
                if let Err(e) = result {
                    codec::cancelled(cancel)?;
                    loaded
                        .warnings
                        .push(json!({"name":source,"reason":format!("Hair companion: {e:#}")}));
                }
            }
            payload["textures"] = json!(loaded.items);
            payload["skipped"]
                .as_array_mut()
                .unwrap()
                .extend(loaded.warnings);
        }
        return Ok(());
    }
    let appearance = archive
        .named("appearance_info.json")
        .and_then(|b| codec::tree(&b))
        .unwrap_or(Value::Null);
    let (effective, selected) =
        crate::player_config::selected_appearance(&appearance, selected_config)?;
    let (mut player, sources) = recipe(file, &effective);
    player["defaultConfig"] = json!(crate::player_config::default_name(&appearance));
    player["selectedConfig"] = json!(selected);
    player["configurations"] = json!(crate::player_config::choices(&appearance));
    let mut loaded = Loaded {
        items: Vec::new(),
        warnings: Vec::new(),
        bytes: 0,
        seen: HashSet::new(),
    };
    loaded.source(archive, catalog, oodle, file, cancel)?;
    let mut companion_count = 0;
    let mut seen = HashSet::new();
    for name in &sources {
        codec::cancelled(cancel)?;
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        let result = (|| -> Result<()> {
            let mut companion = if let Some(archive) =
                local_companion(loose_directory, name, catalog, oodle, cancel)?
            {
                archive
            } else {
                let resource = catalog.resolve(name)?;
                let packed = codec::range(catalog, resource)?;
                let decoded = if codec::vcz(&packed) {
                    crate::archive_decode::preview(oodle, &catalog.root, &packed, cancel)?
                } else {
                    packed
                };
                Archive::open_with_cancel(decoded, cancel)?
            };
            companion.require_unique_members()?;
            loaded.source(&mut companion, catalog, oodle, name, cancel)?;
            // Config IFFs hold the head/eye geometry, not merely texture declarations.
            if name.contains("_config_") {
                if let Ok(mut extra) =
                    crate::preview::build(&mut companion, catalog, oodle, name, cancel)
                {
                    append_config_geometry(payload, &mut extra)?;
                }
            }
            companion_count += 1;
            Ok(())
        })();
        if let Err(e) = result {
            codec::cancelled(cancel)?;
            loaded
                .warnings
                .push(json!({"name":name,"reason":format!("Player companion: {e:#}")}));
        }
    }
    let body = appearance.get("appearance").unwrap_or(&appearance);
    let pit = armpit_texture(text(&body["pitHair"]));
    player["armpitHair"] = json!(pit.is_some());
    player["armpitHairColor"] = body["pitHair"].clone();
    if let Some(name) = pit {
        loaded.add(
            archive,
            catalog,
            oodle,
            &format!("{name}.tld"),
            &json!({"Format":"BC7_UNORM"}),
            "shared armpit texture",
            cancel,
            Some(("armpit", "color")),
        )?;
    }
    let white = scalar(
        body.get("eye white")
            .or_else(|| body.get("eye_white"))
            .unwrap_or(&json!(0)),
    );
    let white = match white.trim().to_lowercase().as_str() {
        "1" => "shared/8b/eyewhite_color_01.8bcd8c19e6fb8f09",
        "2" => "shared/1a/eyewhite_color_02.1aca11c38da970ca",
        "3" => "shared/a2/eyewhite_color_03.a2f8647ad2256dee",
        "glow" => "shared/16/eyewhite_color_glow.16423ad4da3a86fe",
        _ => "shared/e6/eyewhite_color_00.e69c01fde495d855",
    };
    let lash = if text(
        body.get("lash color")
            .or_else(|| body.get("lash_color"))
            .unwrap_or(&Value::Null),
    )
    .eq_ignore_ascii_case("light")
    {
        "shared/05/lash_light_color.05d7fb9877d4c4b7"
    } else {
        "shared/17/lash_dark_color.1758d32561b833f3"
    };
    for (name, target, channel, format) in [
        (white, "eye", "color", "BC1_UNORM"),
        (
            "shared/ad/iris_normalheight1.ad144d47f7f5520f",
            "eye",
            "normal",
            "BC7_UNORM",
        ),
        (lash, "eyelash", "color", "BC7_UNORM"),
    ] {
        loaded.add(
            archive,
            catalog,
            oodle,
            &format!("{name}.tld"),
            &json!({"Format":format}),
            "shared player texture",
            cancel,
            Some((target, channel)),
        )?;
    }
    player["companionsLoaded"] = json!(companion_count);
    player["companionsRequested"] = json!(sources.len());
    payload["player"] = player;
    crate::player_config::attach(
        payload,
        &appearance,
        file,
        &selected,
        catalog,
        oodle,
        loose_directory,
        cancel,
    )?;
    let geometry_sources: HashSet<String> = payload["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["configurationSource"].as_str().map(str::to_string))
        .collect();
    for source in geometry_sources {
        codec::cancelled(cancel)?;
        let result = (|| -> Result<()> {
            let mut extra = if let Some(archive) =
                local_companion(loose_directory, &source, catalog, oodle, cancel)?
            {
                archive
            } else {
                let resource = catalog.resolve(&source)?;
                let bytes = crate::archive_decode::preview(
                    oodle,
                    &catalog.root,
                    &codec::range(catalog, resource)?,
                    cancel,
                )?;
                Archive::open_with_cancel(bytes, cancel)?
            };
            loaded.source(&mut extra, catalog, oodle, &source, cancel)
        })();
        if let Err(error) = result {
            codec::cancelled(cancel)?;
            loaded
                .warnings
                .push(json!({"name":source,"reason":format!("Configuration texture: {error:#}")}));
        }
    }
    // Exact per-item shader bindings keep beard/hair textures independent, even
    // when their generic logical names match or their binary payload is shared.
    for item in crate::player_config::items(&appearance, &selected) {
        let shader = text(&item["shader"]);
        if shader.is_empty() {
            continue;
        }
        let mut owners = vec![format!(
            "{}_item_{}.iff",
            file.trim_end_matches(".iff"),
            token(text(&item["name"]))
        )];
        owners.extend(crate::player_config::geometry_candidates(file, item));
        for texture in &mut loaded.items {
            if !texture["sources"].as_array().unwrap().iter().any(|s| {
                owners
                    .iter()
                    .any(|o| s.as_str().is_some_and(|s| s.eq_ignore_ascii_case(o)))
            }) {
                continue;
            }
            let channel = if text(&texture["name"]).contains("tangent") {
                "tangent"
            } else {
                text(&texture["binding"]["channel"])
            };
            let assignment = json!({"material":shader,"channel":channel});
            if !texture["assignments"].is_array() {
                texture["assignments"] = json!([]);
            }
            let assignments = texture["assignments"].as_array_mut().unwrap();
            if !assignments.contains(&assignment) {
                assignments.push(assignment);
            }
        }
    }
    payload["textures"] = json!(loaded.items);
    payload["stats"]["textures"] = json!(loaded.items.len());
    payload["stats"]["modelsDecoded"] = json!(payload["meshes"].as_array().unwrap().len());
    payload["skipped"]
        .as_array_mut()
        .unwrap()
        .extend(loaded.warnings);
    payload["notes"] = json!(format!(
        "Player appearance: {companion_count}/{} companions loaded · {} textures. TXTR min/max applied in the renderer. Neutral geometry; facial morph evaluation is not reproduced.",
        sources.len(),
        loaded.items.len()
    ));
    Ok(())
}
fn armpit_texture(value: &str) -> Option<&'static str> {
    match token(value).replace('_', "").as_str() {
        "black" => Some("shared/b9/armpithair_color_black.b9ce331b9c8f3da1"),
        "brown" => Some("shared/42/armpithair_color_brown.42cf06d4438460c6"),
        "darkbrown" => Some("shared/26/armpithair_color_darkbrown.26438cb37d4db30b"),
        "blonde" | "blond" => Some("shared/e2/armpithair_color_blonde.e2ee791b60bc73a5"),
        _ => None,
    }
}
fn append_config_geometry(payload: &mut Value, extra: &mut Value) -> Result<()> {
    let meshes = payload["meshes"].as_array().map_or(0, Vec::len);
    let added = extra["meshes"].as_array().map_or(0, Vec::len);
    let triangles = payload["stats"]["triangles"]
        .as_u64()
        .unwrap_or(0)
        .checked_add(extra["stats"]["triangles"].as_u64().unwrap_or(0));
    ensure!(
        meshes.checked_add(added).is_some_and(|n| n <= 1000)
            && triangles.is_some_and(|n| n <= 2_000_000),
        "Combined player/config preview budget exceeded"
    );
    // Validate before moving anything: a rejected companion leaves the base intact.
    for key in ["meshes", "materials", "skipped"] {
        ensure!(
            payload[key].is_array() && extra[key].is_array(),
            "Invalid configuration {key}"
        );
    }
    for key in ["meshes", "materials", "skipped"] {
        payload[key]
            .as_array_mut()
            .unwrap()
            .extend(std::mem::take(extra[key].as_array_mut().unwrap()));
    }
    payload["stats"]["triangles"] = json!(triangles.unwrap());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn companion_budget_is_checked_before_any_geometry_is_moved() {
        let base =
            json!({"meshes":[{}],"materials":[],"skipped":[],"stats":{"triangles":1_999_999}});
        for (count, triangles) in [(1000, 1), (1, 2), (1, u64::MAX)] {
            let mut payload = base.clone();
            let mut extra = json!({"meshes":vec![json!({});count],"materials":[{}],"skipped":[],"stats":{"triangles":triangles}});
            let original = extra.clone();
            assert!(append_config_geometry(&mut payload, &mut extra).is_err());
            assert_eq!(payload, base);
            assert_eq!(extra, original);
        }
    }
    #[test]
    fn companion_at_budget_boundary_moves_complete_arrays() {
        let mut payload =
            json!({"meshes":[{}],"materials":[],"skipped":[],"stats":{"triangles":1_999_999}});
        let mut extra = json!({"meshes":vec![json!({"complete":true});999],"materials":[{"name":"head"}],"skipped":[{"name":"warning"}],"stats":{"triangles":1}});
        append_config_geometry(&mut payload, &mut extra).unwrap();
        assert_eq!(payload["meshes"].as_array().unwrap().len(), 1000);
        assert_eq!(payload["stats"]["triangles"], 2_000_000);
        assert_eq!(payload["materials"][0]["name"], "head");
        assert!(extra["meshes"].as_array().unwrap().is_empty());
    }
    #[test]
    fn authored_armpit_color_and_no_hair_are_respected() {
        assert!(armpit_texture("BROWN").unwrap().contains("color_brown"));
        assert!(armpit_texture("DARK_BROWN").unwrap().contains("darkbrown"));
        assert!(armpit_texture("").is_none());
        assert!(armpit_texture("NONE").is_none());
        assert_eq!(
            binding("armpithair_color_brown.hash.tld"),
            ("armpit", "color")
        );
    }
    #[test]
    fn raw_armpit_tail_is_restored_without_changing_prepared_mips_or_source() {
        let mut tld = vec![0; 80];
        tld[..3].copy_from_slice(b"TLD");
        tld[10] = 98;
        tld[28..32].copy_from_slice(&0u32.to_le_bytes());
        for (p, offset, inflated) in [(32, 0u32, 0u32), (56, 32, 48)] {
            tld[p + 8..p + 12].copy_from_slice(&offset.to_le_bytes());
            tld[p + 12..p + 16].copy_from_slice(&32u32.to_le_bytes());
            tld[p + 16..p + 20].copy_from_slice(&inflated.to_le_bytes());
        }
        let source = tld.clone();
        let mut dds = vec![99; 212];
        dds[..4].copy_from_slice(b"DDS ");
        dds[84..88].copy_from_slice(b"DX10");
        dds[128..132].copy_from_slice(&98u32.to_le_bytes());
        for b in 0..2 {
            for plane in 0..4 {
                for c in 0..4 {
                    dds[148 + b * 16 + plane * 4 + c] = (plane * 8 + b * 4 + c) as u8;
                }
            }
        }
        let prepared = dds[180..].to_vec();
        assert_eq!(restore_armpit_raw_segments(&tld, &mut dds).unwrap(), 1);
        assert_eq!(&dds[148..180], &(0u8..32).collect::<Vec<_>>());
        assert_eq!(&dds[180..], prepared);
        assert_eq!(tld, source);
        tld[44..48].copy_from_slice(&33u32.to_le_bytes());
        assert!(restore_armpit_raw_segments(&tld, &mut dds).is_err());
    }
    #[test]
    fn inline_scene_range_is_used_without_a_txtr_and_preserves_source() {
        let source = crate::iff_directory::tests::fixture(&[("payload.bin", b"authored")]);
        let mut archive = Archive::open(source.clone()).unwrap();
        let info = json!({"Binary":"flag_china_color.983a9ba3b404b04b.tld","Min":[0.0176419597,0.00802319404,0,1],"Max":[1,1,0.0561285019,1],"RangeCompress":15,"Format":"BC1_UNORM"});
        let metadata =
            preview_metadata(&mut archive, "flag_china_color.983a9ba3b404b04b.tld", &info);
        assert!(matches!(metadata, Cow::Borrowed(_)));
        assert_eq!(range(&metadata)["max"][2], json!(0.0561285019));
        assert_eq!(range(&metadata)["mask"], json!([1, 1, 1, 1]));
        assert_eq!(archive.into_bytes(), source);
    }
    #[test]
    fn explicit_txtr_identity_keeps_precedence_over_inline_range() {
        let identity =
            json!({"color":{"Min":[0,0,0,0],"Max":[1,1,1,1],"Format":"BC1_UNORM"}}).to_string();
        let source = crate::iff_directory::tests::fixture(&[("color.TXTR", identity.as_bytes())]);
        let mut archive = Archive::open(source.clone()).unwrap();
        let info = json!({"Min":[0.1,0.2,0.3,0],"Max":[0.5,0.6,0.7,1],"RangeCompress":15});
        let metadata = preview_metadata(&mut archive, "color.0123456789abcdef.tld", &info);
        assert!(matches!(metadata, Cow::Owned(_)));
        assert!(range(&metadata).is_null());
        assert_eq!(archive.into_bytes(), source);
    }
    #[test]
    fn txtr_channel_mask_matches_stable() {
        assert_eq!(
            range(&json!({"Min":[0.1,0.2,0.3,0],"Max":[0.4,0.5,0.6,1],"RangeCompress":5}))["mask"],
            json!([1, 0, 1, 0])
        );
    }
    #[test]
    fn identity_range_is_not_compressed() {
        assert!(range(&json!({"Min":[0,0,0,0],"Max":[1,1,1,1]})).is_null());
    }
    #[test]
    fn female_recipe_uses_female_body_colors() {
        let (_, sources) = recipe(
            "char/sig/png12345.iff",
            &json!({"appearance":{"gender":"FEMALE","skin color":7},"accessory_items":{"default_config":"NORMAL","configurations":[{"name":"normal","items":["FACE COLOR"]}]}}),
        );
        assert!(sources.contains(&"char/female_chest_color_07.iff".into()));
        assert!(sources.contains(&"char/sig/png12345_item_face_color.iff".into()));
    }
    #[test]
    fn normal_not_bound_as_color() {
        assert_eq!(binding("face_normal.x.tld"), ("face", "normal"));
        assert_eq!(binding("chest_color.x.tld"), ("chest", "color"));
        assert_eq!(
            source_binding(
                "hair_color.x.tld",
                "char/pcreate/hair_underpaint_afro_colorhair_afro_fins_tangent.iff"
            ),
            ("hair", "auxiliary")
        );
        assert_eq!(
            source_binding(
                "hair_color.x.tld",
                "char/pcreate/hair_underpaint_afro_color.iff"
            ),
            ("hair-underpaint", "color")
        );
    }
}
