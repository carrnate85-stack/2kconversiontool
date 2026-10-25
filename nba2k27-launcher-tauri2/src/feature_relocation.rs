//! Per-feature game-update resilience: relocates each Version 2 feature's
//! pinned code anchors in a new NBA2K27.exe and re-derives the game globals
//! they reference, following WPF's provider-resolver idea
//! (nba2k27-version-proxy\src\provider_resolver.cpp: compiled-in signatures,
//! masked bytes, unique matches, structural cross-checks, fail closed).
//!
//! The signature catalog (`assets/v2-relocation-catalog.json`) is generated
//! once from the verified game build by the ignored test
//! `generate_relocation_catalog`. It is made from each feature module's own
//! anchors, widened until unique, with RIP-relative and rel32 operands
//! masked. At runtime the exe FILE is mapped read-only; nothing attaches to
//! the game or writes anything except a small cache report.
//!
//! A feature counts as re-derived only when every anchor matches exactly once
//! and every pinned global is re-derived from a relocated reference that lands
//! in a section with the original permissions. Proofs that cannot relocate
//! (code hashes, full-file SHA-256, vtable contents, native compiled-in
//! profiles) are listed as blockers, and keep the feature at "Needs mapping".
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};

use crate::build_resolver::Image;

pub(crate) const SCHEMA: &str = "nba2k27-v2-relocation-catalog-v1";
pub(crate) const REPORT_SCHEMA: &str = "nba2k27-v2-feature-compatibility-v1";
pub(crate) const PINNED: (u32, u32) = (0x6AD2_E48C, 0x3653_2000);
pub(crate) const RESOLVED_LABEL: &str = "Resolved game update";
const MAX_WINDOW: usize = 128;

// ---------- Catalog ----------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Field {
    /// Byte offset of the 32-bit operand inside the window.
    pub off: usize,
    /// Instruction end relative to the window start (target = rva + end + disp).
    pub end: usize,
    /// "rip" (data/code reference) or "rel" (call/jump; masked, not derived).
    pub kind: String,
    /// Original absolute RVA target.
    pub target: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Anchor {
    pub label: String,
    pub rva: u32,
    pub section: String,
    pub hex: String,
    /// One character per byte: 'x' exact, '?' masked.
    pub mask: String,
    pub fields: Vec<Field>,
    /// Matches of the masked window in the reference build (must be 1).
    pub reference_matches: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Constant {
    pub rva: u32,
    /// Section permissions in the reference build (IMAGE_SCN_* high bits).
    pub flags: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeatureCatalog {
    pub key: String,
    pub label: String,
    pub depends: Vec<String>,
    pub blockers: Vec<String>,
    pub anchors: Vec<Anchor>,
    pub constants: Vec<Constant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Catalog {
    pub schema: String,
    pub timestamp: String,
    pub image_size: String,
    pub features: Vec<FeatureCatalog>,
}
pub(crate) fn catalog() -> Result<Catalog> {
    let c: Catalog = serde_json::from_str(include_str!("../assets/v2-relocation-catalog.json"))
        .context("Relocation catalog is invalid")?;
    ensure!(c.schema == SCHEMA, "Unsupported relocation catalog");
    Ok(c)
}

// ---------- Feature sources (each module's own anchors) ----------

pub(crate) struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    pub depends: &'static [&'static str],
    pub blockers: &'static [&'static str],
    /// (label, rva, the module's own proof bytes at that rva)
    pub anchors: Vec<(String, u32, Vec<u8>)>,
    pub constants: Vec<u32>,
}
fn json_anchors(text: &str) -> Vec<(String, u32, Vec<u8>)> {
    let v: Value = serde_json::from_str(text).unwrap_or_default();
    let rows = v["anchors"]
        .as_array()
        .or_else(|| v["fields"].as_array())
        .cloned()
        .unwrap_or_default();
    rows.iter()
        .filter_map(|a| {
            let rva = a["rva"].as_u64()? as u32;
            let label = a["label"]
                .as_str()
                .or_else(|| a["id"].as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("0x{rva:X}"));
            Some((label, rva, unhex(a["hex"].as_str()?)?))
        })
        .collect()
}
fn json_rvas(text: &str, keys: &[&str]) -> Vec<u32> {
    let v: Value = serde_json::from_str(text).unwrap_or_default();
    keys.iter()
        .filter_map(|k| v[*k].as_u64().map(|r| r as u32))
        .collect()
}
fn unhex(s: &str) -> Option<Vec<u8>> {
    (s.len() % 2 == 0)
        .then(|| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
                .collect()
        })
        .flatten()
}
fn rip(list: &[(u64, &[u8], u64)], prefix: &str) -> (Vec<(String, u32, Vec<u8>)>, Vec<u32>) {
    let anchors = list
        .iter()
        .map(|(rva, bytes, _)| {
            (
                format!("{prefix}-rip-0x{rva:X}"),
                *rva as u32,
                bytes.to_vec(),
            )
        })
        .collect();
    (anchors, list.iter().map(|(_, _, t)| *t as u32).collect())
}
fn fields(list: &[(u64, &[u8])], prefix: &str) -> Vec<(String, u32, Vec<u8>)> {
    list.iter()
        .map(|(rva, bytes)| {
            (
                format!("{prefix}-field-0x{rva:X}"),
                *rva as u32,
                bytes.to_vec(),
            )
        })
        .collect()
}
pub(crate) fn specs() -> Vec<Spec> {
    let mut out = vec![];
    out.push(Spec {
        key: "mods",
        label: "Mod loading (file provider)",
        depends: &[],
        blockers: &[],
        anchors: crate::provider_build_profile::current_anchors()
            .into_iter()
            .map(|(rva, b)| (format!("provider-abi-0x{rva:X}"), rva, b.to_vec()))
            .collect(),
        constants: vec![],
    });
    {
        let (mut a, c) = rip(
            crate::blacktop_native::observer_profile::RIP_ANCHORS,
            "observer",
        );
        a.extend(fields(
            crate::blacktop_native::observer_profile::FIELD_ANCHORS,
            "observer",
        ));
        out.push(Spec {
            key: "observer",
            label: "Blacktop session reader (shared)",
            depends: &[],
            blockers: &[],
            anchors: a,
            constants: c,
        });
    }
    {
        let (mut a, mut c) = rip(crate::rules_v2::RIP_ANCHORS, "rules");
        a.extend(fields(crate::rules_v2::FIELD_ANCHORS, "rules"));
        let getters = include_str!("../assets/rules-v2-getters.json");
        a.extend(
            json_anchors(getters)
                .into_iter()
                .map(|(label, old, mut bytes)| {
                    let current = crate::rules_v2::updated_getter(&label, u64::from(old)) as u32;
                    if bytes.len() >= 7 && bytes.starts_with(&[0x48, 0x8B, 0x05]) {
                        let displacement =
                            (crate::rules_v2::OWNER as i64 - i64::from(current) - 7) as i32;
                        bytes[3..7].copy_from_slice(&displacement.to_le_bytes());
                    }
                    (label, current, bytes)
                }),
        );
        c.push(crate::rules_v2::OWNER as u32);
        out.push(Spec {
            key: "rules",
            label: "Game rules",
            depends: &["observer"],
            blockers: &[],
            anchors: a,
            constants: c,
        });
    }
    {
        let (mut a, c) = rip(crate::roster_v2_profile::RIP_ANCHORS, "roster-id");
        a.extend(fields(crate::roster_v2_profile::FIELD_ANCHORS, "roster-id"));
        out.push(Spec {
            key: "roster-identity",
            label: "Roster identity (shared)",
            depends: &["observer"],
            blockers: &[],
            anchors: a,
            constants: c,
        });
    }
    out.push(Spec {
        key: "cameras",
        label: "Cameras",
        depends: &["observer"],
        blockers: &[],
        anchors: crate::camera_v2::ANCHORS
            .iter()
            .map(|(rva, b)| (format!("camera-0x{rva:X}"), *rva as u32, b.to_vec()))
            .collect(),
        constants: [
            crate::camera_v2::REGISTRY,
            crate::camera_v2::MODES,
            crate::camera_v2::LABELS,
            crate::camera_v2::STATIC_AVAILABILITY,
            crate::camera_v2::RUNTIME_AVAILABILITY,
        ]
        .iter()
        .map(|v| *v as u32)
        .collect(),
    });
    {
        let (mut a, c) = rip(crate::scaler_v2::RIP_ANCHORS, "scaler");
        a.extend(fields(crate::scaler_v2::FIELD_ANCHORS, "scaler"));
        out.push(Spec {
            key: "scaler",
            label: "Scaler",
            depends: &["observer"],
            blockers: &[],
            anchors: a,
            constants: c,
        });
    }
    {
        let (mut a, c) = rip(crate::freestyle_v2::RIP_ANCHORS, "freestyle");
        a.extend(fields(crate::freestyle_v2::FIELD_ANCHORS, "freestyle"));
        out.push(Spec {
            key: "freestyle",
            label: "Freestyle gym and WNBA teams",
            depends: &["rules"],
            blockers: &[],
            anchors: a,
            constants: c,
        });
    }
    let arena = include_str!("../assets/arena-v2-anchors.json");
    out.push(Spec {
        key: "arena",
        label: "Arena",
        depends: &["rules", "roster-identity"],
        blockers: &[],
        anchors: json_anchors(arena),
        constants: json_rvas(arena, &["mappingRva"]),
    });
    let ball = include_str!("../assets/ball-v2-anchors.json");
    out.push(Spec {
        key: "ball",
        label: "Gameplay ball",
        depends: &["rules"],
        blockers: &[],
        anchors: json_anchors(ball),
        constants: json_rvas(
            ball,
            &["registryRva", "holderRva", "rendererRva", "referencesRva"],
        ),
    });
    let scoreboard = include_str!("../assets/scoreboard-v2-anchors.json");
    out.push(Spec {
        key: "scoreboard",
        label: "Scoreboard",
        depends: &["rules"],
        blockers: &[],
        anchors: json_anchors(scoreboard),
        constants: json_rvas(
            scoreboard,
            &[
                "packageTableRva",
                "activeRva",
                "secondaryActiveRva",
                "resourcesRva",
            ],
        ),
    });
    let presentation = include_str!("../assets/presentation-v2-anchors.json");
    out.push(Spec {
        key: "presentation",
        label: "Drone / DJ / MC / crowd",
        depends: &["rules"],
        blockers: &[],
        anchors: json_anchors(presentation),
        constants: json_rvas(presentation, &["settingsOwnerRva", "preloadTableRva"]),
    });
    for (key, label, blockers) in [
        (
            "roster",
            "Roster / team editor",
            &["The roster serializer is proven by a 50 KB code hash."][..],
        ),
        ("outfits", "Outfit editing and carry-over", &[][..]),
        ("jukebox", "Jukebox and Freestyle playlist", &[][..]),
    ] {
        out.push(Spec {
            key,
            label,
            depends: &[],
            blockers,
            anchors: vec![],
            constants: vec![],
        });
    }
    out
}

// ---------- Masking ----------

/// x86-64 operand fields that change when code or data moves: RIP-relative
/// ModRM displacements and rel32 call/jump targets. Conservative: a false
/// positive only masks more bytes; the generator proves each masked window is
/// still unique in the reference build.
pub(crate) fn operand_fields(bytes: &[u8], rva: u32) -> Vec<Field> {
    let mut out: Vec<Field> = vec![];
    let mut i = 1;
    while i + 4 <= bytes.len() {
        let prev = bytes[i - 1];
        let rel = prev == 0xE8
            || prev == 0xE9
            || (i >= 2 && bytes[i - 2] == 0x0F && (0x80..=0x8F).contains(&prev));
        let rip = i >= 2 && prev & 0xC7 == 0x05 && modrm_opcode(&bytes[..i - 1]);
        if rel || rip {
            let imm = if rip {
                immediate(&bytes[..i - 1], prev)
            } else {
                0
            };
            let end = i + 4 + imm;
            let disp = i32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as i64;
            let target = (rva as i64 + end as i64 + disp) as u32;
            out.push(Field {
                off: i,
                end,
                kind: if rip { "rip" } else { "rel" }.into(),
                target,
            });
            i += 4;
            continue;
        }
        i += 1;
    }
    out
}
/// The byte before ModRM is an opcode that takes a ModRM operand.
fn modrm_opcode(before: &[u8]) -> bool {
    let n = before.len();
    let op = before[n - 1];
    let two = n >= 2 && before[n - 2] == 0x0F;
    let vex = (n >= 3 && before[n - 3] == 0xC5) || (n >= 4 && before[n - 4] == 0xC4);
    if two || vex {
        return true;
    }
    matches!(
        op,
        0x01 | 0x03
            | 0x09
            | 0x0B
            | 0x21
            | 0x23
            | 0x29
            | 0x2B
            | 0x31
            | 0x33
            | 0x38
            | 0x39
            | 0x3A
            | 0x3B
            | 0x63
            | 0x69
            | 0x6B
            | 0x80
            | 0x81
            | 0x83
            | 0x84
            | 0x85
            | 0x86
            | 0x87
            | 0x88
            | 0x89
            | 0x8A
            | 0x8B
            | 0x8D
            | 0xC6
            | 0xC7
            | 0xF6
            | 0xF7
            | 0xFE
            | 0xFF
    )
}
/// Immediate bytes that follow a RIP-relative displacement.
fn immediate(before: &[u8], modrm: u8) -> usize {
    let n = before.len();
    if n >= 2 && before[n - 2] == 0x0F {
        return if matches!(before[n - 1], 0xBA | 0x70 | 0xC2 | 0xC6) {
            1
        } else {
            0
        };
    }
    match before[n - 1] {
        0x80 | 0x83 | 0xC6 | 0x6B => 1,
        0x81 | 0xC7 | 0x69 => 4,
        0xF6 if (modrm >> 3) & 7 == 0 => 1,
        0xF7 if (modrm >> 3) & 7 == 0 => 4,
        _ => 0,
    }
}
fn mask_of(len: usize, fields: &[Field]) -> Vec<bool> {
    let mut m = vec![false; len];
    for f in fields {
        for b in &mut m[f.off..f.off + 4] {
            *b = true;
        }
    }
    m
}
fn pattern(bytes: &[u8], mask: &[bool]) -> Result<regex::bytes::Regex> {
    let mut p = String::from("(?s-u)");
    for (b, masked) in bytes.iter().zip(mask) {
        if *masked {
            p.push('.');
        } else {
            p.push_str(&format!("\\x{b:02X}"));
        }
    }
    Ok(regex::bytes::RegexBuilder::new(&p)
        .unicode(false)
        .dot_matches_new_line(true)
        .build()?)
}
fn matches_in(image: &Image, section: &str, re: &regex::bytes::Regex, limit: usize) -> Vec<u32> {
    let mut hits = vec![];
    for (name, va, raw) in image.code_sections() {
        if name != section {
            continue;
        }
        for m in re.find_iter(raw) {
            hits.push(va + m.start() as u32);
            if hits.len() >= limit {
                return hits;
            }
        }
    }
    hits
}

// ---------- Generation (reference build only) ----------

/// Builds the catalog from the verified reference executable. Every module
/// anchor must be present verbatim in the reference image.
pub(crate) fn generate(exe: &Path) -> Result<Catalog> {
    let image = Image::open(exe)?;
    ensure!(
        (image.timestamp, image.image_size) == PINNED,
        "Generate the catalog only from the verified Version 2 build"
    );
    let mut features = vec![];
    for spec in specs() {
        let mut anchors = vec![];
        for (label, rva, proof) in &spec.anchors {
            let section = image
                .section_name(*rva)
                .context("Anchor outside the image")?;
            let mut len = proof.len().max(24).min(MAX_WINDOW);
            let (bytes, flds, count) = loop {
                let bytes = image
                    .read_rva(*rva, len)
                    .context("Anchor outside the image")?
                    .to_vec();
                ensure!(
                    bytes.starts_with(proof),
                    "{label}: module proof differs from the reference build"
                );
                let flds = operand_fields(&bytes, *rva);
                let count =
                    matches_in(&image, &section, &pattern(&bytes, &mask_of(len, &flds))?, 3).len();
                if count <= 1 || len >= MAX_WINDOW {
                    break (bytes, flds, count);
                }
                len = (len + 8).min(MAX_WINDOW);
            };
            anchors.push(Anchor {
                label: label.clone(),
                rva: *rva,
                section,
                hex: bytes.iter().map(|b| format!("{b:02X}")).collect(),
                mask: mask_of(bytes.len(), &flds)
                    .iter()
                    .map(|m| if *m { '?' } else { 'x' })
                    .collect(),
                fields: flds,
                reference_matches: count,
            });
        }
        let mut constants: Vec<Constant> = spec
            .constants
            .iter()
            .map(|rva| Constant {
                rva: *rva,
                flags: image.section_flags(*rva).unwrap_or(0),
            })
            .collect();
        constants.sort_by_key(|c| c.rva);
        constants.dedup_by_key(|c| c.rva);
        features.push(FeatureCatalog {
            key: spec.key.into(),
            label: spec.label.into(),
            depends: spec.depends.iter().map(|s| s.to_string()).collect(),
            blockers: spec.blockers.iter().map(|s| s.to_string()).collect(),
            anchors,
            constants,
        });
    }
    Ok(Catalog {
        schema: SCHEMA.into(),
        timestamp: format!("{:08X}", PINNED.0),
        image_size: format!("{:08X}", PINNED.1),
        features,
    })
}

// ---------- Resolution ----------

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeatureResult {
    pub key: String,
    pub label: String,
    /// Mapped (verified build) | Re-derived (game update, all proofs relocated) | Needs mapping
    pub status: &'static str,
    pub message: String,
    pub anchors: usize,
    pub found: usize,
    pub missing: Vec<String>,
    pub ambiguous: Vec<String>,
    pub constants: usize,
    pub derived: usize,
    pub blockers: Vec<String>,
    /// Original RVA -> re-derived RVA for every global and anchor.
    pub relocations: BTreeMap<String, String>,
}
fn hex32(v: u32) -> String {
    format!("0x{v:08X}")
}
/// Relocate one feature's anchors in `image`. `pinned` uses the verified
/// build's fast path: every masked window must match at its original RVA.
fn resolve_feature(image: &Image, f: &FeatureCatalog, fast: bool, pinned: bool) -> FeatureResult {
    let mut found = 0;
    let (mut missing, mut ambiguous) = (vec![], vec![]);
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    let mut conflict = false;
    for a in &f.anchors {
        let Some(bytes) = unhex(&a.hex) else {
            missing.push(a.label.clone());
            continue;
        };
        let mask: Vec<bool> = a.mask.chars().map(|c| c == '?').collect();
        let hit: Option<u32> = if fast {
            image
                .read_rva(a.rva, bytes.len())
                .filter(|have| {
                    have.iter()
                        .zip(&bytes)
                        .zip(&mask)
                        .all(|((h, w), m)| *m || h == w)
                })
                .map(|_| a.rva)
        } else {
            match pattern(&bytes, &mask) {
                Ok(re) => {
                    let hits = matches_in(image, &a.section, &re, 2);
                    if hits.len() > 1 {
                        ambiguous.push(a.label.clone());
                        None
                    } else {
                        hits.first().copied()
                    }
                }
                Err(_) => None,
            }
        };
        let Some(new_rva) = hit else {
            if !ambiguous.contains(&a.label) {
                missing.push(a.label.clone());
            }
            continue;
        };
        found += 1;
        map.insert(a.rva, new_rva);
        let Some(have) = image.read_rva(new_rva, bytes.len()) else {
            continue;
        };
        for field in a.fields.iter().filter(|x| x.kind == "rip") {
            let disp =
                i32::from_le_bytes(have[field.off..field.off + 4].try_into().unwrap()) as i64;
            let target = (new_rva as i64 + field.end as i64 + disp) as u32;
            if map.get(&field.target).is_some_and(|t| *t != target) {
                conflict = true;
            }
            map.insert(field.target, target);
        }
    }
    let derived = f
        .constants
        .iter()
        .filter(|c| {
            map.get(&c.rva).is_some_and(|new| {
                // A re-derived global must land in a section with the original
                // read/write/execute permissions.
                image
                    .section_flags(*new)
                    .is_some_and(|fl| fl & 0xE000_0000 == c.flags & 0xE000_0000)
            })
        })
        .count();
    let anchors_ok =
        found == f.anchors.len() && missing.is_empty() && ambiguous.is_empty() && !conflict;
    let proofs_ok = anchors_ok && derived == f.constants.len();
    // A feature with no relocatable proof may be admitted only by the exact
    // pinned-build identity.  Treating an empty proof set as successfully
    // re-derived would silently enable hash/native-profile features on the
    // next unknown game update.
    let has_relocatable_proof = !f.anchors.is_empty() || !f.constants.is_empty();
    let status = match (
        proofs_ok,
        pinned,
        f.blockers.is_empty(),
        has_relocatable_proof,
    ) {
        (true, true, true, _) => "Mapped",
        (true, false, true, true) => "Re-derived",
        _ => "Needs mapping",
    };
    let message = match status {
        "Mapped" => "Verified for this game build.".to_string(),
        "Re-derived" => format!("Game updated: all {} proofs re-derived.", f.anchors.len()),
        _ if f.anchors.is_empty() => format!(
            "Game updated: {} needs a new mapping. {}",
            f.label,
            f.blockers.join(" ")
        ),
        _ if !anchors_ok => format!(
            "Game updated: {} needs a new mapping ({} of {} code anchors found{}{}).",
            f.label,
            found,
            f.anchors.len(),
            if ambiguous.is_empty() {
                String::new()
            } else {
                format!(", {} ambiguous", ambiguous.len())
            },
            if conflict {
                ", conflicting references"
            } else {
                ""
            }
        ),
        _ if derived < f.constants.len() => format!(
            "Game updated: {} needs a new mapping ({} of {} game globals re-derived).",
            f.label,
            derived,
            f.constants.len()
        ),
        _ => format!(
            "Game updated: {} needs a new mapping. Code anchors re-derived; {}",
            f.label,
            f.blockers.join(" ")
        ),
    };
    FeatureResult {
        key: f.key.clone(),
        label: f.label.clone(),
        status,
        message,
        anchors: f.anchors.len(),
        found,
        missing,
        ambiguous,
        constants: f.constants.len(),
        derived,
        blockers: f.blockers.clone(),
        relocations: map.iter().map(|(k, v)| (hex32(*k), hex32(*v))).collect(),
    }
}
/// Resolve features (all when `only` is None) and apply dependencies.
pub(crate) fn resolve_image(
    image: &Image,
    catalog: &Catalog,
    only: Option<&[&str]>,
) -> Vec<FeatureResult> {
    let pinned = (image.timestamp, image.image_size) == PINNED;
    resolve_image_with(image, catalog, only, pinned, pinned)
}
/// `fast` checks anchors only at their original RVAs (verified build);
/// otherwise every anchor is searched for a unique match.
pub(crate) fn resolve_image_with(
    image: &Image,
    catalog: &Catalog,
    only: Option<&[&str]>,
    fast: bool,
    pinned: bool,
) -> Vec<FeatureResult> {
    let mut results: Vec<FeatureResult> = catalog
        .features
        .iter()
        .filter(|f| only.is_none_or(|o| o.contains(&f.key.as_str())))
        .map(|f| resolve_feature(image, f, fast, pinned))
        .collect();
    let ok: BTreeMap<String, bool> = results
        .iter()
        .map(|r| (r.key.clone(), r.status != "Needs mapping"))
        .collect();
    for r in &mut results {
        let deps = &catalog
            .features
            .iter()
            .find(|f| f.key == r.key)
            .unwrap()
            .depends;
        let failed: Vec<&String> = deps.iter().filter(|d| ok.get(*d) == Some(&false)).collect();
        if r.status != "Needs mapping" && !failed.is_empty() {
            r.status = "Needs mapping";
            r.message = format!(
                "Game updated: {} needs a new mapping (depends on {}).",
                r.label,
                failed
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    results
}

// ---------- Mod-loading admission ----------

/// Admits only mod loading for an unrecognized game update. This needs two
/// proofs: (1) WPF's structural provider resolver, the same check the native
/// proxy repeats before it registers anything, and (2) every Rust provider ABI
/// anchor re-derived exactly once. Everything else stays off.
pub(crate) fn admit_mod_loading(exe: &Path) -> Result<Value> {
    let key = identity(exe)?;
    if let Some(v) = ADMISSION.lock().ok().and_then(|m| m.get(&key).cloned()) {
        return v.map_err(|e| anyhow::anyhow!(e));
    }
    let result = (|| -> Result<Value> {
        let report = crate::build_resolver::resolve(exe).to_json();
        ensure!(
            crate::build_resolver::proven(&report)?,
            "Game updated and the file provider could not be proven: {}",
            report["failure"].as_str().unwrap_or("unknown")
        );
        let image = Image::open(exe)?;
        let pinned = (image.timestamp, image.image_size) == PINNED;
        let mods = resolve_image(&image, &catalog()?, Some(&["mods"]))
            .pop()
            .context("Mod-loading catalog missing")?;
        ensure!(
            mod_loading_proof_admitted(&mods.status, pinned),
            "{}",
            mods.message
        );
        Ok(json!({
            "label": RESOLVED_LABEL,
            "modProviderMapped": true,
            "nativeWritesMapped": false,
            "version2ArenaBallMapped": false,
            "version2GameplayBallMapped": false,
            "version2ScoreboardMapped": false,
            "observerMapped": false,
            "version2OutfitCarryMapped": false,
            "version2OutfitCarryError": "Game updated: outfit editing needs a new mapping.",
            "resolvedUpdate": {"timestamp": report["timestamp"], "imageSize": report["imageSize"]},
        }))
    })();
    if let Ok(mut m) = ADMISSION.lock() {
        m.insert(
            key,
            result
                .as_ref()
                .map(|v| v.clone())
                .map_err(|e| format!("{e:#}")),
        );
    }
    result
}
/// The pinned build resolves at its catalog RVAs and reports `Mapped`. A later
/// executable is admitted only when every provider proof uniquely relocates
/// and reports `Re-derived`. Keeping these cases explicit prevents a newly
/// pinned build from being rejected with "Verified for this game build."
pub(crate) fn mod_loading_proof_admitted(status: &str, pinned: bool) -> bool {
    status == if pinned { "Mapped" } else { "Re-derived" }
}
type Admission = BTreeMap<(PathBuf, u64, u128), std::result::Result<Value, String>>;
static ADMISSION: Mutex<Admission> = Mutex::new(BTreeMap::new());
fn identity(exe: &Path) -> Result<(PathBuf, u64, u128)> {
    let meta = fs::metadata(exe)?;
    Ok((
        exe.into(),
        meta.len(),
        meta.modified()?.duration_since(UNIX_EPOCH)?.as_millis(),
    ))
}

/// Features a plan may not use on a resolved update (all native features).
pub(crate) fn resolved_update_message() -> &'static str {
    "Game updated: mods load through the re-derived file provider. Cameras, scaler, outfits, arena, ball, scoreboard, presentation, Jukebox and Freestyle need a new mapping and stay off for this launch."
}

// ---------- Settings report ----------

static REPORT: Mutex<Option<((PathBuf, u64, u128), Value)>> = Mutex::new(None);

/// Settings > Advanced "Game build compatibility" (read-only).
pub(crate) fn report(root: &Path, game_root: &Path, force: bool) -> Result<Value> {
    let exe = game_root.join("NBA2K27.exe");
    if !exe.is_file() {
        return Ok(
            json!({"schema":REPORT_SCHEMA,"build":null,"features":[],"message":"Choose the installed NBA 2K27 folder."}),
        );
    }
    let key = identity(&exe)?;
    if !force {
        if let Some((k, v)) = REPORT.lock().ok().and_then(|r| r.clone()) {
            if k == key {
                return Ok(v);
            }
        }
        if let Some(v) = read_cache(root, &key) {
            remember(&key, &v);
            return Ok(v);
        }
    }
    let image = Image::open(&exe)?;
    let pinned = (image.timestamp, image.image_size) == PINNED;
    let build = format!("{:08X}:{:08X}", image.timestamp, image.image_size);
    let results = resolve_image(&image, &catalog()?, None);
    let admitted = if pinned {
        true
    } else {
        admit_mod_loading(&exe).is_ok()
    };
    let v = json!({
        "schema": REPORT_SCHEMA,
        "build": build,
        "buildLabel": if pinned { "Version 2" } else { "Unrecognized game update" },
        "pinned": pinned,
        "modLoadingAdmitted": admitted,
        "checkedAt": crate::launcher_health::now_iso(),
        "features": results,
        "message": if pinned {
            "This is the verified game build; every feature uses its pinned mapping."
        } else if admitted {
            "Game updated. Mods load through the re-derived file provider; every other feature waits for a verified mapping. Nothing is written on a guess."
        } else {
            "Game updated. No feature could be proven for this build; native features stay off."
        },
        "fileSize": key.1,
    });
    let _ = write_cache(root, &key, &build, &v);
    remember(&key, &v);
    Ok(v)
}
fn remember(key: &(PathBuf, u64, u128), v: &Value) {
    if let Ok(mut r) = REPORT.lock() {
        *r = Some((key.clone(), v.clone()));
    }
}
fn cache_path(root: &Path, build: &str) -> PathBuf {
    root.join("Cache/feature-relocation")
        .join(format!("{}.json", build.replace(':', "-")))
}
fn read_cache(root: &Path, key: &(PathBuf, u64, u128)) -> Option<Value> {
    let dir = root.join("Cache/feature-relocation");
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let meta = entry.metadata().ok()?;
        if !meta.is_file() || meta.len() > 1024 * 1024 {
            continue;
        }
        let v: Value = serde_json::from_slice(&fs::read(entry.path()).ok()?).ok()?;
        if v["schema"] == REPORT_SCHEMA
            && v["exe"].as_str().is_some_and(|p| Path::new(p) == key.0)
            && v["fileSize"] == key.1
            && v["modifiedMs"].as_str() == Some(&key.2.to_string())
        {
            return Some(v);
        }
    }
    None
}
fn write_cache(root: &Path, key: &(PathBuf, u64, u128), build: &str, v: &Value) -> Result<()> {
    let mut v = v.clone();
    v["exe"] = json!(key.0);
    v["modifiedMs"] = json!(key.2.to_string());
    let path = cache_path(root, build);
    fs::create_dir_all(path.parent().unwrap())?;
    crate::backend::atomic_json(&path, &v)
}

#[cfg(test)]
#[path = "feature_relocation_tests.rs"]
mod tests;
