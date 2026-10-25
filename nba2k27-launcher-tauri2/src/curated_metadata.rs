//! Curated presentation and validated, checksum-pinned MediaFire downloads.
use crate::curated_catalog::Entry;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
pub const URL: &str = "https://raw.githubusercontent.com/carrnate85-stack/2kconversiontool/main/launcher-downloads/catalog.json";
pub fn bundled() -> Result<Catalog> {
    Ok(serde_json::from_str(include_str!(
        "../download-catalog/catalog.json"
    ))?)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema: u32,
    pub items: Vec<Item>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download: Option<crate::curated_catalog::Download>,
}
pub fn validate(c: &Catalog) -> Result<()> {
    ensure!(
        c.schema == 1 && c.items.len() <= 2000,
        "Unsupported or oversized curator catalog"
    );
    let mut ids = std::collections::BTreeSet::new();
    for i in &c.items {
        ensure!(
            i.id.len() <= 50
                && (i.id.starts_with("videos-")
                    || i.id.starts_with("playlists-")
                    || i.id == crate::curated_catalog::PREVIEW_ID)
                && i.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && ids.insert(&i.id),
            "Invalid or duplicate catalog identity"
        );
        ensure!(
            !i.title.trim().is_empty()
                && i.title.chars().count() <= 120
                && !i.title.chars().any(char::is_control)
                && i.description.len() <= 4000
                && !i.description.chars().any(|c| c.is_control() && c != '\n'),
            "Catalog title or description exceeds limits"
        );
        if let Some(d) = &i.download {
            crate::curated_catalog::validate_download(&i.id, d)?;
        }
        if !i.artwork.is_empty() {
            if i.artwork.starts_with("https://") {
                crate::curated_http::catalog_url(&i.artwork)?;
            } else {
                ensure!(
                    i.artwork.starts_with("art/")
                        && i.artwork.len() <= 160
                        && i.artwork
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
                        && !i.artwork.contains("..")
                        && i.artwork.matches('/').count() == 1,
                    "Artwork must be an art/ filename or raw GitHub URL"
                );
            }
            let a = i.artwork.to_ascii_lowercase();
            ensure!(
                [".png", ".jpg", ".jpeg", ".webp", ".svg"]
                    .iter()
                    .any(|s| a.ends_with(s)),
                "Use PNG, JPG, WebP, or SVG artwork"
            );
        }
    }
    Ok(())
}
pub fn apply(entries: &mut Vec<Entry>, catalog: &Catalog, source: Option<&str>) -> Result<()> {
    validate(catalog)?;
    // Replacing an item's file keeps its editorial identity, without also listing
    // the same newly uploaded file under its automatically discovered identity.
    entries.retain(|e| {
        !catalog.items.iter().any(|i| {
            i.id != e.id
                && i.download.as_ref().is_some_and(|d| {
                    crate::curated_catalog::link_key(&d.url).ok()
                        == crate::curated_catalog::link_key(&e.url).ok()
                        && (i.id.starts_with("videos-") && e.kind == "videos"
                            || i.id.starts_with("playlists-") && e.kind == "playlists")
                })
        })
    });
    for i in &catalog.items {
        if !i.hidden && !entries.iter().any(|e| e.id == i.id) && i.download.is_some() {
            let mut e = crate::curated_catalog::preview_pack();
            e.id = i.id.clone();
            e.kind = if i.id == crate::curated_catalog::PREVIEW_ID {
                "previews"
            } else if i.id.starts_with("videos-") {
                "videos"
            } else {
                "playlists"
            }
            .into();
            entries.push(e);
        }
    }
    entries.retain(|e| !catalog.items.iter().any(|i| i.id == e.id && i.hidden));
    for e in entries.iter_mut() {
        if let Some(i) = catalog.items.iter().find(|i| i.id == e.id) {
            if let Some(d) = &i.download {
                e.url = d.url.clone();
                e.filename = d.filename.clone();
                e.bytes = d.bytes;
                e.sha256 = d.sha256.to_ascii_lowercase();
                e.supported = true;
                e.reason.clear();
            }
            e.name = i.title.clone();
            e.description = i.description.clone();
            e.order = i.order;
            e.artwork = if !i.artwork.is_empty() && !i.artwork.starts_with("https://") {
                if let Some(base) = source {
                    crate::curated_http::catalog_url(base)?
                        .join(&i.artwork)?
                        .to_string()
                } else {
                    i.artwork.clone()
                }
            } else {
                i.artwork.clone()
            };
        }
    }
    entries.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_editor_changes_presentation_only() {
        let mut entries = vec![crate::curated_catalog::preview_pack()];
        let original = entries[0].clone();
        let mut c = bundled().unwrap();
        let item = c
            .items
            .iter_mut()
            .find(|i| i.id == crate::curated_catalog::PREVIEW_ID)
            .unwrap();
        item.title = "My previews".into();
        item.description = "Custom description".into();
        item.order = -9;
        apply(&mut entries, &c, None).unwrap();
        assert_eq!(entries[0].name, "My previews");
        assert_eq!(entries[0].url, original.url);
        assert_eq!(entries[0].sha256, original.sha256);
        assert_eq!(entries[0].bytes, original.bytes);
        c.items
            .iter_mut()
            .find(|i| i.id == crate::curated_catalog::PREVIEW_ID)
            .unwrap()
            .hidden = true;
        apply(&mut entries, &c, None).unwrap();
        assert!(entries.is_empty());
    }
    #[test]
    fn rejects_unsafe_art_and_download_overrides() {
        let mut c: Catalog = serde_json::from_str(
            r#"{"schema":1,"items":[{"id":"videos-abc123456789","title":"Intro"}]}"#,
        )
        .unwrap();
        assert!(validate(&c).is_ok());
        for art in [
            "javascript:alert(1)",
            "art/../../secret.png",
            "https://evil.test/a.png",
        ] {
            c.items[0].artwork = art.into();
            assert!(validate(&c).is_err());
        }
        assert!(
            serde_json::from_str::<Catalog>(
                r#"{"schema":1,"items":[],"downloadUrl":"https://evil.test"}"#
            )
            .is_err()
        );
    }
}
