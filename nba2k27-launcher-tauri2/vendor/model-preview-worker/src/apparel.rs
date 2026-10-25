//! Preview-only verified shoe palettes and clothing designs. Never used by export.
use crate::{
    catalog::Catalog,
    codec::{self, Archive, Oodle},
    player, textures,
};
use anyhow::{Context, Result, ensure};
use base64::Engine;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};
pub struct Session {
    assignments: Vec<Value>,
    kind: Kind,
}
enum Kind {
    Shoe {
        id: String,
        variants: Vec<Palette>,
        maps: BTreeMap<String, Vec<u8>>,
        baked: Option<Vec<Value>>,
    },
    Clothing {
        base: String,
        variants: Vec<(String, String)>,
    },
}
struct Palette {
    name: String,
    label: String,
    colors: Vec<[u8; 3]>,
    team: bool,
}
struct Style {
    id: String,
    related: String,
    count: usize,
}
fn open(c: &Catalog, o: &Oodle, name: &str, cancel: &AtomicBool) -> Result<Archive> {
    let archive = Archive::open_with_cancel(
        crate::archive_decode::preview(o, &c.root, &codec::range(c, c.resolve(name)?)?, cancel)?,
        cancel,
    )?;
    archive.require_unique_members()?;
    Ok(archive)
}
fn style(b: &[u8], id: &str, slug: &str) -> Result<Style> {
    ensure!(
        b.len() >= 272
            && codec::u32_at(b, 16)? == 0xf86ea2b6
            && codec::u32_at(b, 20)?.to_string() == id,
        "Shoe style identity mismatch"
    );
    let s = &b[216..272];
    let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    ensure!(
        std::str::from_utf8(&s[..end])? == slug,
        "Shoe style does not match geometry"
    );
    Ok(Style {
        id: id.into(),
        related: codec::u32_at(b, 36)?.to_string(),
        count: codec::u32_at(b, 84)? as usize,
    })
}
fn palette(b: &[u8], id: &str, count: usize, name: String) -> Result<Palette> {
    ensure!(
        b.len() == 848
            && codec::u32_at(b, 0)? == 0xfae98e2b
            && codec::u32_at(b, 72)?.to_string() == id,
        "Shoe colorway identity mismatch"
    );
    ensure!((1..=32).contains(&count), "Unsupported shoe region count");
    let mut colors = Vec::new();
    let mut team = false;
    for i in 0..count {
        let p = 80 + i * 24;
        ensure!(
            codec::u16_at(b, p)? as usize == i && b[p + 7] == 255,
            "Unsupported region record"
        );
        colors.push(b[p + 4..p + 7].try_into()?);
        team |= b[p + 8] != 0xc0;
    }
    let label = name
        .strip_prefix(&format!("shoe_{id}_"))
        .unwrap_or(&name)
        .into();
    Ok(Palette {
        name,
        label,
        colors,
        team,
    })
}
fn colorize(b: &[u8], colors: &[[u8; 3]]) -> Result<(usize, usize, Vec<u8>)> {
    ensure!(
        b.len() >= 148
            && b.starts_with(b"DDS ")
            && b.get(84..88) == Some(b"DX10")
            && codec::u32_at(b, 128)? == 28,
        "Unsupported region map format"
    );
    let w = codec::u32_at(b, 16)? as usize;
    let h = codec::u32_at(b, 12)? as usize;
    ensure!(
        (1..=2048).contains(&w)
            && (1..=2048).contains(&h)
            && codec::u32_at(b, 20)? as usize == w * 4
            && 148 + w * h * 4 <= b.len(),
        "Invalid region dimensions"
    );
    let mut pixels = vec![0; w * h * 4];
    for p in (0..pixels.len()).step_by(4) {
        let color = colors
            .get((b[148 + p] >> 3) as usize)
            .context("Region index outside palette")?;
        pixels[p..p + 3].copy_from_slice(color);
        pixels[p + 3] = 255;
    }
    Ok((w, h, pixels))
}
fn package(
    c: &Catalog,
    o: &Oodle,
    name: &str,
    assignments: &[Value],
    cancel: &AtomicBool,
    only_color: bool,
) -> Result<Vec<Value>> {
    let mut a = open(c, o, name, cancel)?;
    let gallery = textures::gallery(&mut a, c, cancel, |n, info, a, item| {
        item["range"] = player::range(&player::preview_metadata(a, n, info));
    })?;
    let names: Vec<_> = gallery["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap_or("").to_lowercase())
        .collect();
    let right_color = names.iter().any(|n| n.contains("shoe_color_r."));
    let color_re = regex::Regex::new(r"_color(?:_r)?\.")?;
    let normal_re = regex::Regex::new(r"_normal(?:_r)?\.")?;
    let mut result = Vec::new();
    for raw in gallery["items"].as_array().unwrap() {
        let n = raw["name"].as_str().unwrap_or("").to_lowercase();
        let ch = if color_re.is_match(&n) {
            "color"
        } else if normal_re.is_match(&n) && !only_color {
            "normal"
        } else {
            continue;
        };
        let side = if n.contains("_r.") { "right" } else { "left" };
        let matched: Vec<_> = assignments
            .iter()
            .filter(|a| {
                if a["side"].is_string() {
                    a["side"] == side
                        || (ch == "color" && side == "left" && !right_color && a["side"] == "right")
                } else {
                    a["channel"] == ch
                }
            })
            .map(|a| json!({"material":a["material"],"channel":ch}))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let mut item = raw.clone();
        item["bytes"] = item.as_object_mut().unwrap().remove("data").unwrap();
        item["assignments"] = json!(matched);
        item["source"] = json!(name);
        item["binding"] = json!({"target":"apparel","channel":ch});
        item["name"] = json!(
            raw["name"]
                .as_str()
                .unwrap()
                .strip_suffix(".tld")
                .map(|s| format!("{s}.dds"))
                .unwrap_or(raw["name"].as_str().unwrap().into())
        );
        result.push(item);
    }
    Ok(result)
}
fn reference(b: &[u8], p: usize) -> Result<usize> {
    let n = codec::u64_at(b, p)?;
    ensure!(
        n > 0 && n <= 9_007_199_254_740_991,
        "Invalid clothing reference"
    );
    let t = p
        .checked_add(n as usize - 1)
        .context("Clothing reference overflow")?;
    ensure!(t > p && t < b.len(), "Clothing reference outside table");
    Ok(t)
}
fn string(b: &[u8], p: usize) -> Result<String> {
    let tail = b.get(p..).context("Clothing string outside table")?;
    let end = tail
        .iter()
        .position(|n| *n == 0)
        .context("Unterminated clothing string")?;
    ensure!(end > 0 && end <= 256, "Invalid clothing string length");
    Ok(std::str::from_utf8(&tail[..end])?.into())
}
fn clothing_variants(
    casual: &[u8],
    items: &[u8],
    geometry: &str,
    c: &Catalog,
    cancel: &AtomicBool,
) -> Result<Vec<(String, String)>> {
    ensure!(
        codec::u32_at(casual, 0)? == 0x3a70f295 && codec::u32_at(items, 0)? == 0x6e2fad8b,
        "Unsupported clothing table schema"
    );
    let count = codec::u32_at(casual, 24)? as usize;
    let start = reference(casual, 32)?;
    ensure!(
        (1..=200_000).contains(&count) && start + count * 24 <= casual.len(),
        "Invalid clothing names array"
    );
    let mut names = BTreeMap::new();
    for i in 0..count {
        codec::cancelled(cancel)?;
        let p = start + i * 24;
        names.insert(
            codec::u32_at(casual, p + 8)?,
            (
                string(casual, reference(casual, p)?)?,
                codec::u32_at(casual, p + 12)?,
                codec::u32_at(casual, p + 16)?,
            ),
        );
    }
    let count = codec::u32_at(items, 28)? as usize;
    let start = reference(items, 32)?;
    ensure!(
        (1..=200_000).contains(&count) && start + count * 40 <= items.len(),
        "Invalid clothing items array"
    );
    let mut result = Vec::new();
    let valid = regex::Regex::new(r"^[a-zA-Z0-9_]{1,200}$")?;
    for i in 0..count {
        codec::cancelled(cancel)?;
        let p = start + i * 40;
        let Some((name, a, b)) = names.get(&codec::u32_at(items, p + 4)?) else {
            continue;
        };
        ensure!(
            codec::u32_at(items, p)? == 0x6d762063
                && codec::u32_at(items, p + 8)? == *a
                && codec::u32_at(items, p + 12)? == *b,
            "Clothing item identity mismatch"
        );
        let count = codec::u32_at(items, p + 28)? as usize;
        let array = reference(items, p + 32)?;
        ensure!(
            (1..=64).contains(&count) && array + count * 8 <= items.len(),
            "Invalid clothing dependency array"
        );
        let mut deps = Vec::new();
        for j in 0..count {
            let t = reference(items, array + j * 8)?;
            deps.push((items[t], string(items, t + 1)?));
        }
        if !deps.iter().any(|(kind, n)| *kind == 2 && n == geometry) {
            continue;
        }
        let colors: Vec<_> = deps
            .iter()
            .filter(|(kind, n)| *kind == 1 && n == &format!("{name}_color_tex"))
            .collect();
        if colors.len() != 1 || !valid.is_match(name) {
            continue;
        }
        let resource = format!("clothing/{}.iff", colors[0].1);
        if c.resolve(&resource).is_ok() {
            result.push((name.clone(), resource));
        }
    }
    result.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    Ok(result)
}
impl Session {
    pub fn create(
        payload: &mut Value,
        c: &Catalog,
        o: &Oodle,
        name: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<Self>> {
        let re = regex::Regex::new(
            r"(?i)^clothing/geo/(?:dynamic|simple|standard)/([a-z0-9_]+)_geo\.iff$",
        )?;
        let Some(found) = re.captures(name) else {
            return Ok(None);
        };
        let model = found[1].to_lowercase();
        let mats: Vec<_> = payload["materials"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|m| m["name"].as_str().map(str::to_string))
            .collect();
        if let Some(slug) = model.strip_prefix("shoes_") {
            let assignments:Vec<_>=mats.iter().map(|m|json!({"material":m,"side":if m.to_uppercase().starts_with("R_"){"right"}else if m.to_uppercase().starts_with("L_"){"left"}else{"unknown"}})).collect();
            let mapping: Value =
                serde_json::from_str(include_str!("../Data/shoe-model-names.json"))?;
            let mut styles = Vec::new();
            for (id, value) in mapping["models"]
                .as_object()
                .context("Shoe mapping missing")?
            {
                if value["slug"] != slug {
                    continue;
                }
                let mut archive = open(c, o, &format!("shoes/shoe_style_{id}.iff"), cancel)?;
                styles.push(style(
                    &archive.named(&format!("{id}_style.NikeIdStyle"))?,
                    id,
                    slug,
                )?);
            }
            ensure!(
                !styles.is_empty(),
                "No verified style mapping for this shoe"
            );
            let roots: Vec<_> = styles
                .iter()
                .filter(|s| s.related == s.id || s.related == "0")
                .collect();
            let style = if styles.len() == 1 {
                &styles[0]
            } else {
                ensure!(
                    roots.len() == 1
                        && styles
                            .iter()
                            .all(|s| s.id == roots[0].id || s.related == roots[0].id),
                    "Ambiguous shoe style relationships"
                );
                roots[0]
            };
            let normal = format!("shoes/shoe_ng_{}.iff", style.id);
            let base = if c.resolve(&normal).is_ok() {
                package(c, o, &normal, &assignments, cancel, false)?
            } else {
                vec![]
            };
            let mut maps = BTreeMap::new();
            let mut variants = Vec::new();
            let baked = if style.count == 0 {
                let colors: Vec<_> = base
                    .iter()
                    .filter(|t| {
                        t["assignments"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|a| a["channel"] == "color")
                    })
                    .cloned()
                    .collect();
                ensure!(
                    !colors.is_empty(),
                    "Style has no supported baked color textures"
                );
                Some(colors)
            } else {
                ensure!(style.count <= 32, "Unsupported shoe region count");
                let mut global = open(c, o, "shoes/shoe_global.iff", cancel)?;
                for m in global.names()? {
                    codec::cancelled(cancel)?;
                    let name = m["name"].as_str().unwrap_or("");
                    if name.starts_with(&format!("shoe_{}_", style.id))
                        && name.ends_with(".CustomShoeData")
                    {
                        if let Ok(p) = palette(
                            &global.read(m["id"].as_u64().unwrap() as usize, 1024 * 1024)?,
                            &style.id,
                            style.count,
                            name.trim_end_matches(".CustomShoeData").into(),
                        ) {
                            variants.push(p);
                        }
                    }
                }
                let mut regions = open(
                    c,
                    o,
                    &format!("shoes/shoe_ed_region_{}.iff", style.id),
                    cancel,
                )?;
                // Region IFFs commonly contain only TXTR descriptors; the DDS/TLD
                // itself is shared. Resolve through the descriptor gallery.
                let gallery = textures::gallery(&mut regions, c, cancel, |_, _, _, _| {})?;
                let names = gallery["items"]
                    .as_array()
                    .context("Missing region texture list")?;
                for (side, stem) in [("left", "shoe_regions."), ("right", "shoe_regions_r.")] {
                    let m = names
                        .iter()
                        .find(|m| {
                            let name = m["name"].as_str().unwrap_or("").to_lowercase();
                            name.rsplit('/').next().unwrap_or(&name).starts_with(stem)
                                && (name.ends_with(".dds") || name.ends_with(".tld"))
                        })
                        .context("Missing shoe region map")?;
                    let dds = base64::engine::general_purpose::STANDARD
                        .decode(m["data"].as_str().context("Missing decoded region DDS")?)?;
                    maps.insert(side.into(), dds);
                }
                None
            };
            let session = Self {
                assignments,
                kind: Kind::Shoe {
                    id: style.id.clone(),
                    variants,
                    maps,
                    baked,
                },
            };
            let render = session.change(c, o, "", cancel)?;
            let mut textures: Vec<_> = base
                .into_iter()
                .filter(|t| {
                    !t["assignments"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|a| a["channel"] == "color")
                })
                .collect();
            textures.extend(render["textures"].as_array().unwrap().iter().cloned());
            payload["textures"] = json!(textures);
            payload["shoe"] = render["shoe"].clone();
            payload["notes"] = json!(
                "Approximate dominant-region shoe previews; layered patterns, finishes, edge blending and runtime team colors are not reproduced. Geometry and exports are unchanged."
            );
            return Ok(Some(session));
        }
        let gender = regex::Regex::new(r"^(?:male|female|child|teen)_")?;
        let suffix = regex::Regex::new(r"_(?:mat|shader)$")?;
        let family = gender.replace(&model, "").into_owned();
        let assignments: Vec<_> = mats
            .iter()
            .filter(|m| gender.replace(&suffix.replace(&m.to_lowercase(), ""), "") == family)
            .map(|m| json!({"material":m,"channel":"color"}))
            .collect();
        if assignments.is_empty() {
            return Ok(None);
        }
        let mut base = String::new();
        let mut textures = Vec::new();
        for channel in ["color", "normal"] {
            let resource = [
                format!("clothing/{model}_{channel}_tex.iff"),
                format!("clothing/{family}_{channel}_tex.iff"),
                format!("clothing/{model}_tex.iff"),
                format!("clothing/{family}_tex.iff"),
            ]
            .into_iter()
            .find(|n| c.resolve(n).is_ok());
            if let Some(name) = resource {
                if channel == "color" {
                    base = name.clone();
                }
                let a: Vec<_> = assignments
                    .iter()
                    .map(|a| json!({"material":a["material"],"channel":channel}))
                    .collect();
                textures.extend(package(c, o, &name, &a, cancel, false)?);
            }
        }
        if base.is_empty() {
            return Ok(None);
        }
        let variants = (|| -> Result<_> {
            let mut table = open(c, o, "clothing/clothing_itemtable.iff", cancel)?;
            clothing_variants(
                &table.named("clothing_item_table.CasualsTable")?,
                &table.named("item_table.ConsolidatedClothingItems")?,
                &format!("{model}_geo"),
                c,
                cancel,
            )
        })();
        let variants = match variants {
            Ok(v) => v,
            Err(e) => {
                codec::cancelled(cancel)?;
                payload["skipped"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name":"Clothing item table","reason":format!("{e:#}")}));
                vec![]
            }
        };
        payload["textures"] = json!(textures);
        payload["apparel"] = json!({"selected":"","variants":variants.iter().map(|(n,_)|json!({"name":n,"label":n})).collect::<Vec<_>>()});
        payload["notes"] = json!(
            "Verified clothing item designs; layered decals and runtime patterns may differ in-game. Changing design retains geometry and normal maps."
        );
        Ok(Some(Self {
            assignments,
            kind: Kind::Clothing { base, variants },
        }))
    }
    pub fn change(
        &self,
        c: &Catalog,
        o: &Oodle,
        choice: &str,
        cancel: &AtomicBool,
    ) -> Result<Value> {
        codec::cancelled(cancel)?;
        let materials: Vec<_> = self
            .assignments
            .iter()
            .map(|a| a["material"].clone())
            .collect();
        match &self.kind {
            Kind::Shoe {
                id,
                variants,
                maps,
                baked,
            } => {
                let selected = if !choice.is_empty() {
                    choice.into()
                } else if baked.is_some() {
                    format!("shoe_{id}_baked")
                } else {
                    variants
                        .iter()
                        .find(|v| v.name == format!("shoe_{id}_default") && !v.team)
                        .map(|v| v.name.clone())
                        .unwrap_or("neutral".into())
                };
                let mut textures = Vec::new();
                let meta = if baked.is_some() {
                    vec![
                        json!({"name":format!("shoe_{id}_baked"),"label":"Original texture","teamColors":false}),
                    ]
                } else {
                    variants
                        .iter()
                        .map(|v| json!({"name":v.name,"label":v.label,"teamColors":v.team}))
                        .collect()
                };
                if selected != "neutral" {
                    if let Some(baked) = baked {
                        ensure!(
                            selected == format!("shoe_{id}_baked"),
                            "Unknown baked shoe choice"
                        );
                        textures = baked.clone();
                    } else {
                        let v = variants
                            .iter()
                            .find(|v| v.name == selected && !v.team)
                            .context("Colorway unavailable or requires runtime team colors")?;
                        for side in ["left", "right"] {
                            let (w, h, pixels) =
                                colorize(maps.get(side).context("Missing region map")?, &v.colors)?;
                            let assignments: Vec<_> = self
                                .assignments
                                .iter()
                                .filter(|a| a["side"] == side)
                                .map(|a| json!({"material":a["material"],"channel":"color"}))
                                .collect();
                            textures.push(json!({"name":format!("{}_{}_preview",v.name,side),"source":"Preview-only region palette (approximate)","format":"RGBA8_SRGB","width":w,"height":h,"previewRgba":true,"bytes":base64::engine::general_purpose::STANDARD.encode(pixels),"assignments":assignments}));
                        }
                    }
                }
                Ok(
                    json!({"textures":textures,"shoe":{"selected":selected,"variants":meta,"approximate":true},"replaceMaterials":materials}),
                )
            }
            Kind::Clothing { base, variants } => {
                let name = if choice.is_empty() {
                    base
                } else {
                    &variants
                        .iter()
                        .find(|(n, _)| n == choice)
                        .context("Design unavailable for this garment")?
                        .1
                };
                let textures = package(c, o, name, &self.assignments, cancel, true)?;
                ensure!(
                    !textures.is_empty(),
                    "No supported color texture; previous design unchanged"
                );
                Ok(
                    json!({"textures":textures,"apparelVariant":choice,"replaceMaterials":materials}),
                )
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn region_decode_uses_red_index_and_preserves_palette() {
        let mut b = vec![0; 156];
        b[..4].copy_from_slice(b"DDS ");
        b[84..88].copy_from_slice(b"DX10");
        b[128..132].copy_from_slice(&28u32.to_le_bytes());
        b[16..20].copy_from_slice(&2u32.to_le_bytes());
        b[12..16].copy_from_slice(&1u32.to_le_bytes());
        b[20..24].copy_from_slice(&8u32.to_le_bytes());
        b[152] = 8;
        let (_, _, p) = colorize(&b, &[[22, 33, 44], [55, 66, 77]]).unwrap();
        assert_eq!(p, [22, 33, 44, 255, 55, 66, 77, 255]);
        assert!(colorize(&b, &[[1, 2, 3]]).is_err());
    }
    #[test]
    fn malformed_palette_and_table_refs_rejected() {
        assert!(palette(&[0; 848], "1", 32, "shoe_1_default".into()).is_err());
        assert!(reference(&[0; 8], 0).is_err());
        assert!(string(&[65; 300], 0).is_err());
    }
}
