// Read-only material recipes. No geometry regeneration, export rewrites or disk cache.
use crate::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
    player, textures,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
#[derive(Default)]
pub struct Recipe {
    specs: Vec<(String, Value, Value)>,
    pub materials: BTreeMap<String, Value>,
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
pub fn channel(path: &str) -> &'static str {
    let p = path.to_lowercase();
    let leaf = p.rsplit('/').next().unwrap_or(&p);
    if leaf == "combinedlayermaskmap" {
        return "layer-mask";
    }
    if leaf.starts_with("layer") && leaf.ends_with("_coneanglemap") {
        return "layer-ao";
    }
    if leaf == "meshmapconeanglemap" {
        return "mesh-ao";
    }
    if leaf.starts_with("logo") && leaf.ends_with("texture") {
        return "logo";
    }
    if p.contains("emissive") {
        return "emissive";
    }
    if (leaf.starts_with("addon") || leaf.starts_with("graffiti")) && leaf.contains("albedo") {
        return "overlay";
    }
    if [
        "colortexture",
        "colortextureinternal",
        "basetexture",
        "linetexture",
    ]
    .contains(&leaf)
        || p.contains("albedo")
        || p.contains("diffuse")
        || p.contains("basecolor")
        || p.contains("colormap")
    {
        "color"
    } else if p.contains("normal") {
        "normal"
    } else if p.contains("rough") {
        "roughness"
    } else if p.contains("metal") {
        "metalness"
    } else if leaf == "coneanglemap" {
        "ao"
    } else {
        ""
    }
}
fn shader(v: &Value, depth: usize) -> String {
    if depth > 6 {
        return String::new();
    }
    if let Some(map) = v.as_object() {
        for (key, child) in map {
            if ["shader", "technique", "effect"]
                .iter()
                .any(|s| key.to_lowercase().contains(s))
            {
                if let Some(s) = child.as_str() {
                    return s.into();
                }
            }
        }
        for child in map.values() {
            let found = shader(child, depth + 1);
            if !found.is_empty() {
                return found;
            }
        }
    }
    String::new()
}
fn links(
    v: &Value,
    path: String,
    texture_map: &Value,
    material: &str,
    specs: &mut BTreeMap<String, (Value, Value)>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 128, "Material metadata depth exceeds limit");
    let pixelmap = text(&v["Pixelmap"]);
    let ch = channel(&path);
    if !pixelmap.is_empty() && !ch.is_empty() {
        let info = &texture_map[pixelmap];
        let binary = text(&info["Binary"]);
        let lower = binary.to_lowercase();
        if lower.ends_with(".dds") || lower.ends_with(".tld") {
            let (stored,public)=specs.entry(lower).or_insert_with(||(info.clone(),json!({"name":binary.strip_suffix(".tld").map(|s|format!("{s}.dds")).unwrap_or(binary.into()),"resourceName":binary,"source":pixelmap,"binding":{"target":"arena","channel":ch},"width":info["Width"],"height":info["Height"],"format":info["Format"],"assignments":[],"resourceSlots":[]})));
            if !stored.is_object() {
                *stored = info.clone();
            }
            let assignments = public["assignments"].as_array_mut().unwrap();
            if !assignments
                .iter()
                .any(|a| a["material"] == material && a["channel"] == ch)
            {
                let mut assignment = json!({"material":material,"channel":ch});
                if ch == "logo" {
                    assignment["slot"] = json!(path.rsplit('/').next().unwrap_or(""));
                }
                assignments.push(assignment);
            }
            let slot = path.rsplit('/').next().unwrap_or("");
            let slots = public["resourceSlots"].as_array_mut().unwrap();
            if !slots
                .iter()
                .any(|v| v["material"] == material && v["slot"] == slot)
            {
                slots.push(json!({"material":material,"slot":slot}));
            }
        }
    }
    if let Some(map) = v.as_object() {
        for (key, child) in map {
            links(
                child,
                format!("{path}/{key}"),
                texture_map,
                material,
                specs,
                depth + 1,
            )?;
        }
    }
    if let Some(arr) = v.as_array() {
        for (i, child) in arr.iter().enumerate() {
            links(
                child,
                format!("{path}/{i}"),
                texture_map,
                material,
                specs,
                depth + 1,
            )?;
        }
    }
    Ok(())
}
fn collect(
    v: &Value,
    source: &str,
    specs: &mut BTreeMap<String, (Value, Value)>,
    materials: &mut BTreeMap<String, Value>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 128, "Material scene depth exceeds limit");
    if let Some(map) = v["Material"].as_object() {
        for (name, material) in map {
            materials
                .entry(name.clone())
                .or_insert_with(|| json!({"shader":shader(material,0),"source":source}));
            links(material, String::new(), &v["Texture"], name, specs, 0)?;
        }
    }
    if let Some(map) = v.as_object() {
        for child in map.values() {
            collect(child, source, specs, materials, depth + 1)?;
        }
    }
    if let Some(arr) = v.as_array() {
        for child in arr {
            collect(child, source, specs, materials, depth + 1)?;
        }
    }
    Ok(())
}
impl Recipe {
    pub fn read(archive: &mut Archive, cancel: &AtomicBool) -> Result<Self> {
        let mut recipe = Self::default();
        let mut specs = BTreeMap::new();
        for m in archive.names()? {
            codec::cancelled(cancel)?;
            let name = text(&m["name"]);
            if !name.to_lowercase().ends_with(".scne") {
                continue;
            }
            let scene =
                codec::tree(&archive.read(m["id"].as_u64().unwrap() as usize, 64 * 1024 * 1024)?)?;
            collect(&scene, name, &mut specs, &mut recipe.materials, 0)?;
        }
        recipe.specs = specs
            .into_values()
            .map(|(info, public)| (text(&public["resourceName"]).into(), info, public))
            .collect();
        recipe.specs.sort_by_key(|(name, _, p)| {
            let color = p["assignments"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["channel"] == "color");
            let focus = [
                "court",
                "floor",
                "hardwood",
                "basket",
                "scoreboard",
                "scorecube",
            ]
            .iter()
            .any(|s| name.to_lowercase().contains(s));
            (!focus, !color, name.clone())
        });
        Ok(recipe)
    }
    pub fn decorate(&self, payload: &mut Value) {
        if let Some(materials) = payload["materials"].as_array_mut() {
            for item in materials {
                if let Some(meta) = self.materials.get(text(&item["name"])) {
                    item["shader"] = meta["shader"].clone();
                    item["source"] = meta["source"].clone();
                }
            }
        }
        let mut dependencies: Vec<_> = self
            .specs
            .iter()
            .map(|(_, _, public)| public.clone())
            .collect();
        if let Some(textures) = payload["textures"].as_array() {
            for t in textures {
                if !dependencies.iter().any(|d| d["name"] == t["name"]) {
                    let mut meta = t.clone();
                    if let Some(o) = meta.as_object_mut() {
                        o.remove("bytes");
                        o.remove("data");
                    }
                    dependencies.push(meta);
                }
            }
        }
        payload["textureDependencies"] = json!(dependencies);
        payload["materialTextureTotal"] = json!(self.specs.len());
    }
    pub fn batch(
        &self,
        archive: &mut Archive,
        catalog: &Catalog,
        oodle: &Oodle,
        cursor: usize,
        cancel: &AtomicBool,
    ) -> Result<Value> {
        ensure!(cursor <= self.specs.len(), "Texture cursor is invalid");
        let limit = (cursor + 64).min(self.specs.len());
        let mut end = cursor;
        let mut items = Vec::new();
        let mut skipped = Vec::new();
        let mut bytes = 0;
        for (name, info, public) in &self.specs[cursor..limit] {
            codec::cancelled(cancel)?;
            match textures::reconstruct(
                archive,
                catalog,
                oodle,
                name,
                info,
                cancel,
                16 * 1024 * 1024,
            ) {
                Ok(dds) => {
                    // Leave this item at the next cursor; a full batch isn't a missing texture.
                    if bytes + dds.len() > 64 * 1024 * 1024 {
                        break;
                    }
                    bytes += dds.len();
                    let meta = player::preview_metadata(archive, name, info);
                    let mut item = public.clone();
                    let encoded = crate::backend::texture(name, &dds);
                    item["bytes"] = encoded["data"].clone();
                    item["width"] = encoded["width"].clone();
                    item["height"] = encoded["height"].clone();
                    item["mips"] = encoded["mips"].clone();
                    item["range"] = player::range(&meta);
                    items.push(item);
                }
                Err(e) => skipped.push(json!({"name":name,"reason":format!("{e:#}")})),
            }
            end += 1;
        }
        Ok(
            json!({"textures":items,"skipped":skipped,"end":end,"total":self.specs.len(),"bytes":bytes}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn links_require_material_pixelmaps_not_filename_guessing() {
        let scene = json!({"Texture":{"tex":{"Binary":"shared/floor_color.tld","Width":32}},"Material":{"court":{"Shader":"arena", "ColorTexture":{"Pixelmap":"tex"},"NormalMap":{"Pixelmap":"missing"}}}});
        let mut specs = BTreeMap::new();
        let mut materials = BTreeMap::new();
        collect(&scene, "floor.SCNE", &mut specs, &mut materials, 0).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs["shared/floor_color.tld"].1["assignments"][0]["material"],
            "court"
        );
        assert_eq!(materials["court"]["shader"], "arena");
    }
    #[test]
    fn unsupported_slots_are_not_treated_as_colour() {
        assert_eq!(channel("/Params/ColorTextureInternal"), "color");
        assert_eq!(channel("/NormalMap"), "normal");
        assert_eq!(channel("/Layer0_ConeAngleMap"), "layer-ao");
        assert_eq!(channel("/CombinedLayerMaskMap"), "layer-mask");
        assert_eq!(channel("/AddOn1AlbedoMap"), "overlay");
    }
}
