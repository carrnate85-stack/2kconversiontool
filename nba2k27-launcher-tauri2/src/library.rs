use crate::backend::{Outfit, Settings, validate_outfit};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub folder: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Camera {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    pub name: String,
    pub enabled: bool,
    pub height: f64,
    pub zoom: f64,
}
impl Camera {
    pub(crate) fn native_id(&self) -> Result<u32> {
        let id = self.id.or_else(|| crate::camera_policy::CURATED.iter()
            .find(|(_,name)| *name == self.name).map(|(id,_)| *id))
            .ok_or_else(|| anyhow::anyhow!("Camera '{}' has no verified original type; choose its camera type before applying", self.name))?;
        ensure!(
            crate::camera_policy::CURATED
                .iter()
                .any(|(known, _)| *known == id),
            "Unknown camera type"
        );
        Ok(id)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Library {
    pub profiles: Vec<Profile>,
    pub cameras: Vec<Camera>,
    pub playlists: Vec<Playlist>,
    pub outfits: Vec<Outfit>,
    pub tags: BTreeMap<String, String>,
}
impl Default for Library {
    fn default() -> Self {
        let s = Settings::default();
        Self {
            profiles: vec![Profile {
                name: s.profile,
                folder: s.mods_root,
            }],
            cameras: crate::camera_policy::CURATED
                .iter()
                .map(|(id, n)| Camera {
                    id: Some(*id),
                    name: n.to_string(),
                    enabled: true,
                    height: 1.,
                    zoom: 1.,
                })
                .collect(),
            playlists: vec![Playlist {
                name: "My playlist".into(),
                tracks: vec![],
            }],
            outfits: vec![],
            tags: BTreeMap::new(),
        }
    }
}
fn names<'a>(values: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for name in values {
        ensure!(
            !name.trim().is_empty() && name.len() <= 80 && !name.chars().any(char::is_control),
            "Name must contain 1–80 printable characters"
        );
        ensure!(
            seen.insert(name.trim().to_lowercase()),
            "Names must be unique"
        );
    }
    Ok(())
}
impl Library {
    pub fn validate(&self, _root: &Path) -> Result<()> {
        ensure!(
            self.profiles.len() <= 100
                && self.cameras.len() <= 100
                && self.playlists.len() <= 100
                && self.outfits.len() <= 1000
                && self.tags.len() <= 10000,
            "Library exceeds limits"
        );
        names(self.profiles.iter().map(|p| p.name.as_str()))?;
        names(self.cameras.iter().map(|p| p.name.as_str()))?;
        names(self.playlists.iter().map(|p| p.name.as_str()))?;
        names(self.outfits.iter().map(|p| p.name.as_str()))?;
        for p in &self.profiles {
            ensure!(
                p.folder.len() <= 1000 && !p.folder.contains('\0'),
                "Invalid mod folder"
            );
        }
        for c in &self.cameras {
            if c.id.is_some() {
                c.native_id()?;
            }
            ensure!(
                c.height.is_finite()
                    && c.zoom.is_finite()
                    && (-100. ..=100.).contains(&c.height)
                    && (-100. ..=100.).contains(&c.zoom),
                "Camera steps must be between -100 and 100"
            );
        }
        for p in &self.playlists {
            ensure!(p.tracks.len() <= 1000, "Playlist exceeds 1000 tracks");
            let mut seen = std::collections::HashSet::new();
            for track in &p.tracks {
                ensure!(
                    Path::new(track).file_name().and_then(|n| n.to_str()) == Some(track)
                        && !track.contains(['/', '\\', ':']),
                    "Playlist tracks must be imported filenames"
                );
                ensure!(
                    track.len() <= 255
                        && Path::new(track)
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| matches!(
                                e.to_lowercase().as_str(),
                                "mp3" | "wav" | "m4a" | "ogg"
                            )),
                    "Unsupported playlist track"
                );
                ensure!(
                    seen.insert(track.to_lowercase()),
                    "Duplicate playlist track"
                );
            }
        }
        for o in &self.outfits {
            validate_outfit(o)?;
        }
        for (key, value) in &self.tags {
            ensure!(
                key.len() <= 1500
                    // WPF: 12 tags × 40 characters, comma separated.
                    && value.len() <= 600
                    && !key.contains('\0')
                    && !value.chars().any(char::is_control),
                "Invalid mod tags"
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_identity_survives_reordering_and_renaming() {
        let mut library = Library::default();
        assert_eq!(library.cameras[0].native_id().unwrap(), 51);
        library.cameras.reverse();
        library.cameras[0].name = "My nosebleeds".into();
        assert_eq!(library.cameras[0].native_id().unwrap(), 55);
        let copy: Library =
            serde_json::from_value(serde_json::to_value(&library).unwrap()).unwrap();
        assert_eq!(copy.cameras[0].native_id().unwrap(), 55);
        library.cameras[0].id = Some(999);
        assert!(library.validate(Path::new(".")).is_err());
    }
    #[test]
    fn legacy_camera_names_infer_only_exact_known_types_not_list_position() {
        let mut library = Library::default();
        library.cameras.reverse();
        for row in &mut library.cameras {
            row.id = None;
        }
        let copy: Library =
            serde_json::from_value(serde_json::to_value(&library).unwrap()).unwrap();
        assert_eq!(copy.cameras[0].native_id().unwrap(), 55);
        assert_eq!(copy.cameras[6].native_id().unwrap(), 51);
        library.cameras[0].name = "Renamed unknown camera".into();
        // Preserve a legacy draft without guessing its origin or rewriting it.
        library.validate(Path::new(".")).unwrap();
        assert!(library.cameras[0].native_id().is_err());
        assert!(library.cameras[0].id.is_none());
    }
    #[test]
    fn names_and_camera_ranges() {
        let mut l = Library::default();
        l.validate(Path::new(".")).unwrap();
        l.profiles.push(l.profiles[0].clone());
        assert!(l.validate(Path::new(".")).is_err());
        l.profiles.pop();
        l.cameras[0].zoom = 101.;
        assert!(l.validate(Path::new(".")).is_err());
    }
    #[test]
    fn playlist_cannot_reference_external_files() {
        let mut l = Library::default();
        l.playlists[0].tracks = vec!["../settings.json".into()];
        assert!(l.validate(Path::new(".")).is_err());
    }
}
