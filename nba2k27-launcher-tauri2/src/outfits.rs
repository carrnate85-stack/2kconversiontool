//! Portable outfit interchange. Preserves observed item order; never writes to the game.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, io::Read, sync::OnceLock};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub pool: Option<String>,
    pub original: Vec<String>,
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub layers: Vec<String>,
    pub shoes: Vec<String>,
    pub accessories: Vec<String>,
    pub separable: bool,
    pub manual_only: bool,
}
#[derive(Clone, Deserialize)]
struct Shoe {
    asset: String,
    pools: Vec<String>,
}
#[derive(Clone, Deserialize)]
struct Accessory {
    asset: String,
    slot: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub presets: Vec<Preset>,
    additional_shoes: Vec<Shoe>,
    accessories: Vec<Accessory>,
    slots: Vec<String>,
    #[serde(skip)]
    ids: HashMap<String, usize>,
    #[serde(skip)]
    allowed: HashMap<String, Option<String>>,
}
pub fn catalog() -> Result<&'static Catalog> {
    static C: OnceLock<Catalog> = OnceLock::new();
    if let Some(c) = C.get() {
        return Ok(c);
    }
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("../assets/outfits.json.gz")[..])
        .read_to_end(&mut bytes)?;
    let mut c: Catalog = serde_json::from_slice(&bytes)?;
    c.ids = c
        .presets
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), i))
        .collect();
    c.allowed = c
        .accessories
        .iter()
        .map(|a| (a.asset.clone(), a.slot.clone()))
        .collect();
    let _ = C.set(c);
    Ok(C.get().unwrap())
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub street_id: String,
    pub bottom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shoe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sock: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessories: Option<Vec<String>>,
}
impl Catalog {
    pub fn base(&self, id: &str) -> Result<&Preset> {
        self.ids
            .get(id)
            .map(|i| &self.presets[*i])
            .context("Top variant is not available")
    }
    pub fn layers(&self, p: &Preset) -> Vec<String> {
        let mut rows = Vec::new();
        for d in &self.presets {
            if d.pool == p.pool && d.top == p.top && d.layers.len() <= 1 {
                let layer = d.layers.first().cloned().unwrap_or_else(|| "none".into());
                if !rows.contains(&layer) {
                    rows.push(layer)
                }
            }
        }
        rows
    }
    pub fn compose(&self, s: &Selection) -> Result<Vec<String>> {
        let p = self.base(&s.street_id)?;
        let mut items = p.original.clone();
        if let Some(layer) = s.layer.as_deref().filter(|s| !s.is_empty()) {
            ensure!(
                p.layers.len() <= 1 && self.layers(p).iter().any(|a| a == layer),
                "Layer has not been observed with this top"
            );
            let current = p.layers.first().map(String::as_str).unwrap_or("none");
            if layer != current {
                if let Some(old) = p.layers.first() {
                    let i = items
                        .iter()
                        .position(|a| a == old)
                        .context("Original layer missing")?;
                    if layer == "none" {
                        items.remove(i);
                    } else {
                        items[i] = layer.into();
                    }
                } else {
                    let d = self
                        .presets
                        .iter()
                        .find(|d| {
                            d.pool == p.pool
                                && d.top == p.top
                                && d.layers.first().is_some_and(|l| l == layer)
                        })
                        .context("Layer placement missing")?;
                    let i = d
                        .original
                        .iter()
                        .position(|a| a == layer)
                        .context("Layer placement missing")?;
                    let after_top = d
                        .original
                        .iter()
                        .position(|a| Some(a) == d.top.as_ref())
                        .is_some_and(|t| i == t + 1);
                    ensure!(
                        after_top || i == d.original.len() - 1,
                        "Layer placement is unsupported"
                    );
                    let pos = if after_top {
                        items
                            .iter()
                            .position(|a| Some(a) == p.top.as_ref())
                            .context("Original top missing")?
                            + 1
                    } else {
                        items.len()
                    };
                    items.insert(pos, layer.into());
                }
            }
        }
        if let Some(shoe) = s.shoe.as_deref().filter(|a| !a.is_empty()) {
            if shoe == "roster" {
                ensure!(
                    p.shoes.len() <= 1,
                    "Multiple original shoes are unsupported"
                );
                if let Some(old) = p.shoes.first() {
                    let i = items
                        .iter()
                        .position(|a| a == old)
                        .context("Original shoe missing")?;
                    items.remove(i);
                }
            } else {
                ensure!(
                    p.shoes.len() == 1
                        && (self
                            .presets
                            .iter()
                            .any(|d| d.pool == p.pool && d.shoes.iter().any(|a| a == shoe))
                            || self.additional_shoes.iter().any(|d| d.asset == shoe
                                && p.pool.as_ref().is_some_and(|pool| d.pools.contains(pool)))),
                    "Shoe is not compatible with this variant and fit"
                );
                let i = items
                    .iter()
                    .position(|a| a == &p.shoes[0])
                    .context("Original shoe missing")?;
                items[i] = shoe.into();
            }
        }
        if let Some(sock) = s.sock.as_deref().filter(|a| !a.is_empty()) {
            let socks: Vec<_> = p.accessories.iter().filter(|a| is_sock(a)).collect();
            ensure!(
                socks.len() <= 1
                    && is_sock(sock)
                    && self
                        .presets
                        .iter()
                        .any(|d| d.pool == p.pool && d.accessories.iter().any(|a| a == sock)),
                "Sock is not compatible with this fit"
            );
            if let Some(old) = socks.first() {
                let i = items
                    .iter()
                    .position(|a| a == *old)
                    .context("Original sock missing")?;
                items[i] = sock.into();
            } else {
                ensure!(items.len() < 7, "Outfit exceeds seven pieces");
                items.push(sock.into());
            }
        }
        if Some(&s.bottom) != p.bottom.as_ref() {
            ensure!(
                p.separable
                    && self.presets.iter().any(|d| d.separable
                        && d.pool == p.pool
                        && d.bottom.as_ref() == Some(&s.bottom)),
                "Bottom is not separable or does not match this fit"
            );
            let i = items
                .iter()
                .position(|a| Some(a) == p.bottom.as_ref())
                .context("Original bottom missing")?;
            items[i] = s.bottom.clone();
        }
        let allowed = &self.allowed;
        if let Some(values) = &s.accessories {
            ensure!(values.len() <= 7, "Too many accessories");
            let mut chosen = HashMap::new();
            for asset in values {
                let slot = allowed
                    .get(asset.as_str())
                    .and_then(|s| s.as_deref())
                    .context("Accessory is unavailable")?;
                ensure!(
                    fit(asset) == p.pool.as_deref().unwrap_or("")
                        && chosen.insert(slot, asset).is_none(),
                    "Accessory fit mismatch or duplicate slot"
                );
            }
            items.retain(|a| !allowed.contains_key(a.as_str()));
            for slot in &self.slots {
                if let Some(a) = chosen.get(slot.as_str()) {
                    items.push((*a).clone())
                }
            }
        } else if let Some(asset) = &s.accessory {
            ensure!(
                asset.is_empty()
                    || (allowed.contains_key(asset.as_str())
                        && fit(asset) == p.pool.as_deref().unwrap_or("")),
                "Accessory is unavailable or has the wrong fit"
            );
            items.retain(|a| !allowed.contains_key(a.as_str()));
            if !asset.is_empty() {
                items.push(asset.clone())
            }
        }
        ensure!(
            (2..=7).contains(&items.len()),
            "Outfit must contain 2–7 pieces"
        );
        Ok(items)
    }
    pub fn decode(&self, bytes: &[u8]) -> Result<(String, Selection)> {
        ensure!(bytes.len() <= 16384, "Outfit file exceeds 16 KB");
        let v: Value = serde_json::from_slice(bytes).context("Invalid .2koutfit JSON")?;
        ensure!(
            matches!(
                v["schema"].as_str(),
                Some(
                    "nba2k27-shared-outfit-v1"
                        | "nba2k27-shared-outfit-v2"
                        | "nba2k27-shared-outfit-v3"
                )
            ),
            "Unsupported outfit schema"
        );
        let name = name(v["name"].as_str().context("Outfit name required")?)?;
        let top = v["top"]["asset"]
            .as_str()
            .context("Top identity required")?;
        let original: Vec<String> = serde_json::from_value(v["top"]["original"].clone())?;
        ensure!(
            (2..=7).contains(&original.len()) && original.iter().all(|s| s.len() <= 256),
            "Invalid original top identity"
        );
        let p = self
            .presets
            .iter()
            .find(|p| {
                matches!(p.pool.as_deref(), Some("male" | "female"))
                    && p.top.as_deref() == Some(top)
                    && p.original == original
            })
            .context("Exact top variant is not available; nothing was imported")?;
        let pieces = v["pieces"].as_object().context("Outfit pieces required")?;
        let mut value = json!({"streetId":p.id,"bottom":pieces.get("bottom")});
        for k in ["shoe", "sock", "layer", "accessory", "accessories"] {
            if let Some(v) = pieces.get(k) {
                value[k] = v.clone();
            }
        }
        let s: Selection = serde_json::from_value(value)?;
        self.compose(&s)?;
        Ok((name, s))
    }
    pub fn encode(&self, n: &str, s: &Selection) -> Result<Value> {
        let n = name(n)?;
        self.compose(s)?;
        let p = self.base(&s.street_id)?;
        let mut pieces = serde_json::to_value(s)?;
        pieces.as_object_mut().unwrap().remove("streetId");
        Ok(
            json!({"schema":if s.accessories.is_some(){"nba2k27-shared-outfit-v3"}else if s.accessory.as_ref().is_some_and(|a|!a.is_empty()){"nba2k27-shared-outfit-v2"}else{"nba2k27-shared-outfit-v1"},"name":n,"top":{"asset":p.top,"original":p.original},"pieces":pieces}),
        )
    }
    pub fn variants(&self, top: &str, pool: &str) -> Value {
        let top = if top == "Shirtless" { "NO SHIRT" } else { top };
        json!(self.presets.iter().filter(|p|p.top.as_deref()==Some(top) && p.pool.as_deref()==Some(pool) && !p.id.starts_with("accessory-test-") && self.original_outfit(&p.id,"Fit").is_ok_and(|o|self.selection(&o).is_ok())).map(|p|json!({"id":p.id,"original":p.original,"layers":self.layers(p),"manualOnly":p.manual_only})).collect::<Vec<_>>())
    }
    fn base_outfit(&self, id: &str, n: &str) -> Result<crate::backend::Outfit> {
        let p = self.base(id)?;
        let outfit = crate::backend::Outfit {
            name: name(n)?,
            top: if p.top.as_deref() == Some("NO SHIRT") {
                "Shirtless".into()
            } else {
                p.top.clone().context("Top missing")?
            },
            bottom: p.bottom.clone().context("Bottom missing")?,
            shoes: p
                .shoes
                .first()
                .cloned()
                .unwrap_or_else(|| "Currently installed".into()),
            socks: p
                .accessories
                .iter()
                .find(|a| is_sock(a))
                .cloned()
                .unwrap_or_default(),
            pool: p.pool.clone().context("Fit missing")?,
            accessories: p
                .accessories
                .iter()
                .filter(|a| self.allowed.contains_key(a.as_str()))
                .cloned()
                .collect(),
            street_id: p.id.clone(),
            layer: String::new(),
        };
        Ok(outfit)
    }
    pub fn original_outfit(&self, id: &str, n: &str) -> Result<crate::backend::Outfit> {
        let outfit = self.base_outfit(id, n)?;
        crate::backend::validate_outfit(&outfit)?;
        Ok(outfit)
    }
    /// Reconstruct an editable draft only when it reproduces the verified
    /// native item list exactly. Never substitute a default for unknown gear.
    pub fn worn_outfit(&self, items: &[String], pool: &str) -> Option<crate::backend::Outfit> {
        if !(2..=7).contains(&items.len()) || !matches!(pool, "male" | "female") {
            return None;
        }
        let bottom = self
            .presets
            .iter()
            .filter(|p| p.pool.as_deref() == Some(pool))
            .filter_map(|p| p.bottom.as_ref())
            .find(|a| items.contains(a))?;
        let socks = items.iter().find(|a| is_sock(a));
        let shoes = items.iter().find(|a| {
            self.additional_shoes.iter().any(|s| s.asset == **a)
                || self
                    .presets
                    .iter()
                    .any(|p| p.pool.as_deref() == Some(pool) && p.shoes.contains(a))
        });
        let accessories = items
            .iter()
            .filter(|a| self.allowed.contains_key(a.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for p in self.presets.iter().filter(|p| {
            p.pool.as_deref() == Some(pool) && p.top.as_ref().is_some_and(|a| items.contains(a))
        }) {
            let Ok(mut o) = self.base_outfit(&p.id, "Current outfit") else {
                continue;
            };
            o.bottom = bottom.clone();
            o.shoes = shoes
                .cloned()
                .unwrap_or_else(|| "Currently installed".into());
            o.socks = socks.cloned().unwrap_or_default();
            o.accessories = accessories.clone();
            o.layer = self
                .presets
                .iter()
                .filter(|d| d.pool == p.pool && d.top == p.top)
                .flat_map(|d| &d.layers)
                .find(|a| items.contains(a))
                .cloned()
                .unwrap_or_else(|| "none".into());
            if crate::backend::validate_outfit(&o).is_ok()
                && self
                    .selection(&o)
                    .and_then(|s| self.compose(&s))
                    .is_ok_and(|actual| actual == items)
            {
                return Some(o);
            }
        }
        None
    }
    pub fn selection(&self, o: &crate::backend::Outfit) -> Result<Selection> {
        let p = self.base(&o.street_id)?;
        ensure!(
            p.top.as_deref()
                == Some(if o.top == "Shirtless" {
                    "NO SHIRT"
                } else {
                    &o.top
                })
                && p.pool.as_deref() == Some(&o.pool),
            "Selected top no longer matches its variant; choose a fit variant"
        );
        let s = Selection {
            street_id: o.street_id.clone(),
            bottom: o.bottom.clone(),
            shoe: Some(if o.shoes == "Currently installed" {
                "roster".into()
            } else {
                o.shoes.clone()
            }),
            sock: (!o.socks.is_empty()).then(|| o.socks.clone()),
            layer: (!o.layer.is_empty()).then(|| o.layer.clone()),
            accessory: None,
            accessories: Some(o.accessories.clone()),
        };
        self.compose(&s)?;
        Ok(s)
    }
    pub fn bind_draft(&self, o: &mut crate::backend::Outfit) {
        // Never retain the prior top's identity after changing or randomizing it.
        o.street_id.clear();
        o.layer.clear();
        for p in self.presets.iter().filter(|p| {
            p.pool.as_deref() == Some(&o.pool)
                && p.top.as_deref()
                    == Some(if o.top == "Shirtless" {
                        "NO SHIRT"
                    } else {
                        &o.top
                    })
                && !p.id.starts_with("accessory-test-")
        }) {
            o.street_id = p.id.clone();
            if self.selection(o).is_ok() {
                return;
            }
        }
        o.street_id.clear();
    }
    pub fn imported_outfit(&self, bytes: &[u8]) -> Result<crate::backend::Outfit> {
        let (n, s) = self.decode(bytes)?;
        let expected = self.compose(&s)?;
        let mut o = self.base_outfit(&s.street_id, &n)?;
        o.bottom = s.bottom.clone();
        if let Some(shoe) = s.shoe {
            o.shoes = if shoe == "roster" {
                "Currently installed".into()
            } else {
                shoe
            };
        }
        if let Some(sock) = s.sock {
            o.socks = sock;
        }
        if let Some(layer) = s.layer {
            o.layer = layer;
        }
        if let Some(accessories) = s.accessories {
            o.accessories = accessories;
        } else if let Some(accessory) = s.accessory {
            o.accessories = if accessory.is_empty() {
                vec![]
            } else {
                vec![accessory]
            };
        }
        crate::backend::validate_outfit(&o)?;
        // Implicit legacy pieces cannot be silently removed by the modern editor.
        ensure!(
            self.compose(&self.selection(&o)?)? == expected,
            "Legacy outfit contains implicit pieces this editor cannot preserve"
        );
        Ok(o)
    }
}
fn fit(a: &str) -> &str {
    if a.starts_with("female_") {
        "female"
    } else {
        "male"
    }
}
fn is_sock(a: &str) -> bool {
    a.strip_prefix("female_")
        .unwrap_or(a)
        .starts_with("acc_sock_")
}
fn name(n: &str) -> Result<String> {
    let n = n.trim();
    ensure!(
        !n.is_empty()
            && n.encode_utf16().count() <= 64
            && !n.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'),
        "Enter an outfit name (1–64 characters)"
    );
    Ok(n.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baseline_composition_oracle() {
        let cases: Value =
            serde_json::from_str(include_str!("../assets/outfit-regressions.json")).unwrap();
        for v in cases.as_array().unwrap() {
            let s: Selection = serde_json::from_value(v["selection"].clone()).unwrap();
            let actual = catalog().unwrap().compose(&s).ok();
            assert_eq!(json!(actual), v["items"], "{}", v["selection"]);
        }
    }
    #[test]
    fn exact_variant_sharing_round_trip() {
        let c = catalog().unwrap();
        for p in c
            .presets
            .iter()
            .filter(|p| matches!(p.pool.as_deref(), Some("male" | "female")))
            .step_by(31)
        {
            let s = Selection {
                street_id: p.id.clone(),
                bottom: p.bottom.clone().unwrap_or_default(),
                ..Default::default()
            };
            if c.compose(&s).is_ok() {
                let d = c.encode("QA fit", &s).unwrap();
                let (_, r) = c.decode(&serde_json::to_vec(&d).unwrap()).unwrap();
                assert_eq!(c.compose(&s).unwrap(), c.compose(&r).unwrap());
                assert!(d.get("streetId").is_none());
            }
        }
    }
    #[test]
    fn reject_wrong_variant_size_schema_and_duplicate_slot() {
        let c = catalog().unwrap();
        let p = c
            .presets
            .iter()
            .find(|p| p.pool.as_deref() == Some("male") && p.original.len() >= 2)
            .unwrap();
        let s = Selection {
            street_id: p.id.clone(),
            bottom: p.bottom.clone().unwrap(),
            ..Default::default()
        };
        let mut d = c.encode("QA", &s).unwrap();
        d["top"]["original"][0] = json!("not-in-catalog");
        assert!(c.decode(&serde_json::to_vec(&d).unwrap()).is_err());
        assert!(c.decode(&vec![b' '; 16385]).is_err());
        d["schema"] = json!("unknown");
        assert!(c.decode(&serde_json::to_vec(&d).unwrap()).is_err());
        let a = c
            .accessories
            .iter()
            .find(|a| fit(&a.asset) == "male" && a.slot.is_some())
            .unwrap();
        let mut s = s;
        s.accessories = Some(vec![a.asset.clone(), a.asset.clone()]);
        assert!(c.compose(&s).is_err());
    }
    #[test]
    fn imported_draft_reexports_identical_items() {
        let c = catalog().unwrap();
        for p in c
            .presets
            .iter()
            .filter(|p| p.separable && p.pool.as_deref() == Some("male"))
            .step_by(53)
        {
            let Ok(o) = c.original_outfit(&p.id, "QA") else {
                continue;
            };
            if let Ok(s) = c.selection(&o) {
                let d = c.encode(&o.name, &s).unwrap();
                let r = c.imported_outfit(&serde_json::to_vec(&d).unwrap()).unwrap();
                assert_eq!(
                    c.compose(&s).unwrap(),
                    c.compose(&c.selection(&r).unwrap()).unwrap()
                );
            }
        }
    }
    #[test]
    fn visible_variants_are_editable_and_shareable() {
        let c = catalog().unwrap();
        for p in c.presets.iter().filter(|p| {
            p.top.as_deref() == Some("hoodie_pullover_flag_brazil_gold_wht")
                && p.pool.as_deref() == Some("male")
        }) {
            if p.accessories
                .iter()
                .filter(|a| c.allowed.contains_key(a.as_str()))
                .count()
                > 3
            {
                assert!(c.original_outfit(&p.id, "QA").is_err());
            }
        }
        for v in c
            .variants("hoodie_pullover_flag_brazil_gold_wht", "male")
            .as_array()
            .unwrap()
        {
            let o = c.original_outfit(v["id"].as_str().unwrap(), "QA").unwrap();
            crate::backend::validate_outfit(&o).unwrap();
            c.encode(&o.name, &c.selection(&o).unwrap()).unwrap();
        }
    }
    #[test]
    fn worn_player_drafts_round_trip_verified_items_without_defaults() {
        let c = catalog().unwrap();
        let mut checked = 0;
        for p in c
            .presets
            .iter()
            .filter(|p| p.separable && p.layers.len() <= 1)
        {
            let Ok(o) = c.original_outfit(&p.id, "QA current") else {
                continue;
            };
            let Ok(s) = c.selection(&o) else { continue };
            let items = c.compose(&s).unwrap();
            let draft = c
                .worn_outfit(&items, &o.pool)
                .expect("Known editable clothes must hydrate");
            assert_eq!(c.compose(&c.selection(&draft).unwrap()).unwrap(), items);
            checked += 1;
            if checked == 50 {
                break;
            }
        }
        assert_eq!(checked, 50);
        assert!(
            c.worn_outfit(
                &["unrecognized_top".into(), "unrecognized_bottom".into()],
                "male"
            )
            .is_none()
        );
        assert!(c.worn_outfit(&[], "male").is_none());
        assert!(
            c.worn_outfit(&["a".into(), "b".into()], "unknown")
                .is_none()
        );
    }
    #[test]
    fn import_can_replace_over_limit_original_accessories() {
        // Synthetic future/legacy variant, not a claim about the current stock catalog.
        let mut c = catalog().unwrap().clone();
        let mut p = c
            .presets
            .iter()
            .find(|p| p.pool.as_deref() == Some("male") && p.separable)
            .unwrap()
            .clone();
        let mut slots = std::collections::HashSet::new();
        let accessories = c
            .accessories
            .iter()
            .filter(|a| {
                fit(&a.asset) == "male" && a.slot.as_ref().is_some_and(|s| slots.insert(s.clone()))
            })
            .take(4)
            .map(|a| a.asset.clone())
            .collect::<Vec<_>>();
        assert_eq!(accessories.len(), 4);
        p.id = "synthetic-over-limit".into();
        p.layers.clear();
        p.shoes.clear();
        p.accessories = accessories.clone();
        p.original = vec![p.top.clone().unwrap(), p.bottom.clone().unwrap()];
        p.original.extend(accessories);
        c.ids.insert(p.id.clone(), c.presets.len());
        c.presets.push(p.clone());
        assert!(c.original_outfit(&p.id, "QA").is_err());
        let s = Selection {
            street_id: p.id.clone(),
            bottom: p.bottom.clone().unwrap(),
            accessories: Some(vec![]),
            ..Default::default()
        };
        let d = c.encode("QA reduced fit", &s).unwrap();
        let o = c.imported_outfit(&serde_json::to_vec(&d).unwrap()).unwrap();
        assert!(o.accessories.is_empty());
        assert_eq!(
            c.compose(&s).unwrap(),
            c.compose(&c.selection(&o).unwrap()).unwrap()
        );
    }
}
