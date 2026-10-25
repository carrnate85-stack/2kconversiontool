//! Preview-only configuration selection/attachment assembly. Never used by exports.
use crate::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path, sync::atomic::AtomicBool};

pub fn default_name(appearance: &Value) -> &str {
    let items = &appearance["accessory_items"];
    items["default_config"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| items["default_out_of_uniform_config"].as_str())
        .unwrap_or("")
        .trim()
}
pub fn choices(appearance: &Value) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in appearance["accessory_items"]["configurations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["name"].as_str())
        .chain(std::iter::once(default_name(appearance)))
    {
        let name = name.trim();
        if !name.is_empty()
            && name.len() <= 128
            && out.len() < 128
            && seen.insert(name.to_ascii_lowercase())
        {
            out.push(name.into());
        }
    }
    out
}
pub fn selected_appearance(appearance: &Value, requested: Option<&str>) -> Result<(Value, String)> {
    // No explicit launcher choice means use the authored default. An explicit
    // empty value is kept as the opt-out used by the Configs checkbox.
    let requested = match requested {
        None => default_name(appearance),
        Some(requested) if requested.is_empty() => {
            return Ok((appearance.clone(), String::new()));
        }
        Some(requested) => requested,
    };
    if requested.is_empty() {
        return Ok((appearance.clone(), String::new()));
    };
    let name = choices(appearance)
        .into_iter()
        .find(|n| n.eq_ignore_ascii_case(requested))
        .context("Player configuration is not declared by this appearance")?;
    let mut out = appearance.clone();
    out["accessory_items"]["default_config"] = json!(name);
    Ok((out, name))
}
pub fn items<'a>(appearance: &'a Value, name: &str) -> Vec<&'a Value> {
    let Some(config) = appearance["accessory_items"]["configurations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| {
            c["name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
    else {
        return vec![];
    };
    config["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let name = n.as_str()?;
            appearance["accessory_items"]["items"]
                .as_array()?
                .iter()
                .find(|item| {
                    item["name"]
                        .as_str()
                        .is_some_and(|s| s.eq_ignore_ascii_case(name))
                })
        })
        .collect()
}
pub(crate) fn token(s: &str) -> String {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}
pub fn geometry_candidates(file: &str, item: &Value) -> Vec<String> {
    let base = file.trim_end_matches(".iff");
    let mut names = vec![token(item["name"].as_str().unwrap_or(""))];
    if let Some(mesh) = item["mesh"].as_str() {
        names.push(token(mesh.strip_suffix("Shape").unwrap_or(mesh)));
    }
    let mut seen = HashSet::new();
    let mut out = vec![];
    for name in names {
        if name.is_empty() {
            continue;
        }
        for suffix in ["", "_tangentspace"] {
            let source = format!("{base}_geo_{name}{suffix}.iff");
            if seen.insert(source.clone()) {
                out.push(source)
            }
        }
    }
    out
}
pub fn attach(
    payload: &mut Value,
    appearance: &Value,
    file: &str,
    name: &str,
    catalog: &Catalog,
    oodle: &Oodle,
    loose_directory: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<()> {
    if name.is_empty() {
        return Ok(());
    }
    let mut added = 0usize;
    let mut sources = HashSet::new();
    if let Some(config) = appearance["accessory_items"]["configurations"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| {
            c["name"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(name))
        })
    {
        for requested in config["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !items(appearance, name).iter().any(|item| {
                item["name"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(requested))
            }) {
                payload["skipped"].as_array_mut().unwrap().push(json!({"name":requested,"reason":"Configuration item has no appearance definition"}));
            }
        }
    }
    for item in items(appearance, name) {
        codec::cancelled(cancel)?;
        let label = item["name"].as_str().unwrap_or("accessory");
        let candidates = geometry_candidates(file, item);
        let mut selected = None;
        for source in &candidates {
            if let Some(archive) =
                crate::player::local_companion(loose_directory, source, catalog, oodle, cancel)?
            {
                selected = Some((source.clone(), archive));
                break;
            }
            if let Ok(resource) = catalog.resolve(source) {
                let bytes = crate::archive_decode::preview(
                    oodle,
                    &catalog.root,
                    &codec::range(catalog, resource)?,
                    cancel,
                )?;
                selected = Some((
                    resource.name.clone(),
                    Archive::open_with_cancel(bytes, cancel)?,
                ));
                break;
            }
        }
        let Some((source_name, mut archive)) = selected else {
            payload["skipped"].as_array_mut().unwrap().push(json!({"name":label,"reason":"Configuration geometry is not available in the installed manifest"}));
            continue;
        };
        if !sources.insert(source_name.clone()) {
            continue;
        }
        let result = (|| -> Result<usize> {
            let mut extra =
                crate::preview::build(&mut archive, catalog, oodle, &source_name, cancel)?;
            crate::tangent::project_player_attachment(&mut extra, payload, cancel)?;
            let triangles = extra["stats"]["triangles"].as_u64().unwrap_or(0);
            let meshes = extra["meshes"]
                .as_array_mut()
                .context("Configuration meshes missing")?;
            ensure!(
                !meshes.is_empty(),
                "No supported complete geometry in configuration item"
            );
            ensure!(
                payload["meshes"].as_array().unwrap().len() + meshes.len() <= 1000
                    && payload["stats"]["triangles"].as_u64().unwrap_or(0) + triangles <= 2_000_000,
                "Combined player/config preview budget exceeded"
            );
            for mesh in meshes.iter_mut() {
                mesh["configurationItem"] = json!(label);
                mesh["configurationSource"] = json!(source_name);
            }
            let count = meshes.len();
            payload["meshes"]
                .as_array_mut()
                .unwrap()
                .extend(std::mem::take(meshes));
            payload["materials"]
                .as_array_mut()
                .unwrap()
                .extend(std::mem::take(extra["materials"].as_array_mut().unwrap()));
            payload["skipped"]
                .as_array_mut()
                .unwrap()
                .extend(std::mem::take(extra["skipped"].as_array_mut().unwrap()));
            payload["stats"]["triangles"] =
                json!(payload["stats"]["triangles"].as_u64().unwrap_or(0) + triangles);
            Ok(count)
        })();
        match result {
            Ok(count) => added += count,
            Err(error) => {
                codec::cancelled(cancel)?;
                payload["skipped"].as_array_mut().unwrap().push(
                    json!({"name":source_name,"reason":format!("Configuration item: {error:#}")}),
                );
            }
        }
    }
    payload["player"]["configurationMeshes"] = json!(added);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_defaults_on_supports_opt_out_and_preserves_authored_metadata() {
        let source = json!({"accessory_items":{"default_config":"AFRO","configurations":[{"name":"AFRO"},{"name":"GOATEE"}]}});
        let (base, name) = selected_appearance(&source, None).unwrap();
        assert_eq!(name, "AFRO");
        assert_eq!(base, source);
        let (base, name) = selected_appearance(&source, Some("")).unwrap();
        assert!(name.is_empty());
        assert_eq!(base, source);
        let (chosen, name) = selected_appearance(&source, Some("goatee")).unwrap();
        assert_eq!(name, "GOATEE");
        assert_eq!(chosen["accessory_items"]["default_config"], "GOATEE");
        assert_eq!(source["accessory_items"]["default_config"], "AFRO");
        assert!(selected_appearance(&source, Some("../../other")).is_err());
    }
    #[test]
    fn geometry_candidates_support_config_aliases_without_path_traversal() {
        let paths = geometry_candidates(
            "char/sig/png04090.iff",
            &json!({"name":"AFRO_MEDIUM_facialHair_01","mesh":"facialHair_01Shape"}),
        );
        assert!(paths.contains(&"char/sig/png04090_geo_facialhair_01.iff".into()));
        assert!(paths.contains(&"char/sig/png04090_geo_facialhair_01_tangentspace.iff".into()));
        assert!(
            !geometry_candidates("char/sig/png00001.iff", &json!({"name":"../../outside"}))
                .iter()
                .any(|s| s.contains(".."))
        );
    }
}
