//! Native Jukebox metadata/order/slot planning, independent of build pointers.
//! Retains unrecognized music.json fields. No file or game mutation.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const MAX_TRACKS: usize = 4096;
pub(crate) const MAIN_SLOTS: usize = 71;
pub(crate) const PARK_SLOTS: usize = 54;
pub(crate) const MAX_IFF: usize = 16 * 1024 * 1024;
pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn hex(text: &str, n: usize) -> bool {
    text.len() == n && text.bytes().all(|b| b.is_ascii_hexdigit())
}
fn text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= max
        && !value.chars().any(char::is_control)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Track {
    #[serde(rename = "Artist", default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    #[serde(rename = "Collection")]
    pub collection: u16,
    #[serde(
        rename = "Cover Name",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub cover: String,
    #[serde(rename = "Name", default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(rename = "Bank Index")]
    pub bank: u16,
    #[serde(rename = "Unique Id")]
    pub unique: u16,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Track {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.collection, self.bank, self.unique)
    }
    pub fn custom(&self) -> bool {
        self.collection == 1 && self.bank > 71
    }
    pub fn validate(&self) -> Result<()> {
        // Other stock collections legitimately omit Name/Cover and use banks
        // above 999 (Version 2 collection5 banks2000+). Preserve those records;
        // only the session/custom-import gates restrict main soundtrack banks.
        ensure!(
            [(&self.name, 120), (&self.artist, 120), (&self.cover, 160)]
                .into_iter()
                .all(|(s, n)| s.chars().count() <= n && !s.chars().any(char::is_control)),
            "Invalid Jukebox song metadata"
        );
        ensure!(
            !self.extra.keys().any(|k| [
                "Artist",
                "Collection",
                "Cover Name",
                "Name",
                "Bank Index",
                "Unique Id"
            ]
            .contains(&k.as_str())),
            "Reserved song metadata field"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Music {
    #[serde(rename = "Tracks")]
    pub tracks: Vec<Track>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Music {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.tracks.len() <= MAX_TRACKS && !self.extra.contains_key("Tracks"),
            "Invalid Jukebox song count"
        );
        let mut keys = BTreeSet::new();
        for track in &self.tracks {
            track.validate()?;
            ensure!(keys.insert(track.key()), "Duplicate Jukebox song identity");
        }
        Ok(())
    }
    /// Selection follows the saved playlist, not catalog order. No silent loss.
    pub fn selected(&self, keys: &[String]) -> Result<Self> {
        self.validate()?;
        ensure!(
            !keys.is_empty() && keys.len() <= MAX_TRACKS,
            "Choose a nonempty playlist"
        );
        let by_key: BTreeMap<_, _> = self.tracks.iter().map(|t| (t.key(), t)).collect();
        let mut seen = BTreeSet::new();
        let tracks = keys
            .iter()
            .map(|key| {
                ensure!(seen.insert(key), "Duplicate playlist song");
                by_key
                    .get(key)
                    .map(|t| (*t).clone())
                    .with_context(|| format!("Playlist song is unavailable: {key}"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            tracks,
            extra: self.extra.clone(),
        })
    }
    pub fn add_custom(&mut self, name: &str, artist: &str, cover: &str) -> Result<Track> {
        self.validate()?;
        ensure!(self.tracks.len() < MAX_TRACKS, "Jukebox library is full");
        let bank = (72..999)
            .find(|i| {
                !self
                    .tracks
                    .iter()
                    .any(|t| t.collection == 1 && t.bank == *i)
            })
            .context("No free custom song bank")?;
        let unique = (130..65535)
            .find(|i| !self.tracks.iter().any(|t| t.unique == *i))
            .context("No free custom song ID")?;
        let track = Track {
            name: name.trim().into(),
            artist: artist.trim().into(),
            cover: cover.trim().into(),
            collection: 1,
            bank,
            unique,
            extra: Map::new(),
        };
        track.validate()?;
        ensure!(
            text(&track.name, 120) && text(&track.artist, 120) && text(&track.cover, 160),
            "Custom song title, artist and cover are required"
        );
        self.tracks.push(track.clone());
        Ok(track)
    }
}
pub(crate) fn audio_path(bank: u16) -> String {
    format!("newaudio/jukeboxmusic/{bank:06}-000000-000000-000000-000000-000000.wav")
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Mapping {
    pub slot: u16,
    pub source: String,
    pub target: String,
    pub title: String,
    #[serde(default)]
    pub source_is_stock: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioPolicy {
    pub main: String,
    pub park: String,
    pub custom: String,
}
impl AudioPolicy {
    pub fn validate(&self) -> Result<()> {
        let mask = |s: &str, n| s.len() == n && s.bytes().all(|b| b == b'0' || b == b'1');
        ensure!(
            mask(&self.main, MAIN_SLOTS)
                && mask(&self.park, PARK_SLOTS)
                && mask(&self.custom, MAIN_SLOTS)
                && self
                    .main
                    .bytes()
                    .zip(self.custom.bytes())
                    .all(|(m, c)| c == b'0' || m == b'1'),
            "Invalid native Jukebox masks"
        );
        Ok(())
    }
    /// Fixed loader-owned fields only. No renderer-controlled configuration.
    pub fn loader_config(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "AUDIO_SLOT_COUNT=71\r\nAUDIO_ALLOWED_MAIN={}\r\nAUDIO_ALLOWED_PARK={}\r\nAUDIO_CUSTOM_MAIN={}\r\n",
            self.main, self.park, self.custom
        ))
    }
    pub fn blocked(&self, path: &str) -> Result<bool> {
        self.validate()?;
        if let Some(slot) = path_slot(path, "newaudio/jukeboxmusic/") {
            return Ok(slot > 71 || self.main.as_bytes()[slot - 1] != b'1');
        }
        if let Some(slot) = path_slot(path, "newaudio/jukeboxmusic_park/") {
            return Ok(slot > 54 || self.park.as_bytes()[slot - 1] != b'1');
        }
        Ok(false)
    }
}
fn path_slot(path: &str, prefix: &str) -> Option<usize> {
    let rest = path.strip_prefix(prefix)?;
    let suffix = "-000000-000000-000000-000000-000000.wav";
    if rest.len() != 6 + suffix.len()
        || !rest.is_ascii()
        || &rest[6..] != suffix
        || !rest.as_bytes()[..6].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let n = rest[..6].parse::<usize>().ok()?;
    if n == 0 { None } else { Some(n) }
}
pub(crate) struct Plan {
    pub music: Music,
    pub mappings: Vec<Mapping>,
    pub policy: AudioPolicy,
}
/// Exact WPF stock-first reservation / first-free custom remapping. Library
/// banks remain 72..998; only this session's output uses 1..71.
pub(crate) fn plan(music: &Music) -> Result<Plan> {
    let stock = Music {
        tracks: music
            .tracks
            .iter()
            .filter(|t| t.bank <= 71)
            .cloned()
            .collect(),
        extra: Map::new(),
    };
    plan_with_stock(music, &stock)
}
/// Version 2 MUST use the actual stock catalog, never the legacy >71 heuristic:
/// its soundtrack expanded to93 main songs. The proven provider has71 active
/// positions, so selected stock72..93 are safely copied/remapped like custom
/// payloads, but sourced from pinned game archives rather than a library guess.
pub(crate) fn plan_with_stock(music: &Music, stock: &Music) -> Result<Plan> {
    music.validate()?;
    ensure!(
        !music.tracks.is_empty(),
        "Choose a nonempty playlist before Play"
    );
    stock.validate()?;
    let stock_keys: BTreeSet<_> = stock.tracks.iter().map(Track::key).collect();
    let (mut main, park, mut custom) = ([b'0'; 71], [b'0'; 54], [b'0'; 71]);
    let mut used = BTreeSet::new();
    for t in &music.tracks {
        ensure!(
            t.collection == 1,
            "Custom sessions support main soundtrack songs only; use Default Playlist unchanged for other collections"
        );
        ensure!(
            (1..999).contains(&t.bank),
            "Unsupported main soundtrack bank"
        );
        if t.bank <= 71 {
            ensure!(
                stock_keys.contains(&t.key()),
                "Unknown low soundtrack bank cannot replace a stock identity"
            );
            used.insert(t.bank);
            main[t.bank as usize - 1] = b'1';
        }
    }
    let mut unique: BTreeSet<_> = music
        .tracks
        .iter()
        .filter(|t| t.bank <= 71)
        .map(|t| t.unique)
        .collect();
    let (mut assigned, mut mappings, mut tracks) = (BTreeMap::new(), Vec::new(), Vec::new());
    for t in &music.tracks {
        let mut t = t.clone();
        if t.bank > 71 {
            let source = audio_path(t.bank);
            let slot = if let Some(slot) = assigned.get(&source) {
                *slot
            } else {
                let slot=(1..=71).find(|slot|!used.contains(slot)).context("A playlist supports at most 71 main soundtrack positions; split it without deleting library songs")?;
                used.insert(slot);
                assigned.insert(source.clone(), slot);
                main[slot as usize - 1] = b'1';
                custom[slot as usize - 1] = b'1';
                mappings.push(Mapping {
                    slot,
                    source,
                    target: audio_path(slot),
                    title: t.name.clone(),
                    source_is_stock: stock_keys.contains(&t.key()),
                });
                slot
            };
            let id = if !unique.contains(&(slot - 1)) {
                slot - 1
            } else {
                (0..125)
                    .find(|id| !unique.contains(id))
                    .context("No verified unique soundtrack ID is available")?
            };
            unique.insert(id);
            t.bank = slot;
            t.unique = id;
        }
        tracks.push(t);
    }
    let policy = AudioPolicy {
        main: String::from_utf8(main.to_vec())?,
        park: String::from_utf8(park.to_vec())?,
        custom: String::from_utf8(custom.to_vec())?,
    };
    policy.validate()?;
    Ok(Plan {
        music: Music {
            tracks,
            extra: music.extra.clone(),
        },
        mappings,
        policy,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    pub fn track(bank: u16, id: u16) -> Track {
        Track {
            artist: "Artist".into(),
            collection: 1,
            cover: "001_helicopter".into(),
            name: format!("Song {bank}"),
            bank,
            unique: id,
            extra: Map::new(),
        }
    }
    #[test]
    fn reserve_stock_before_custom_and_keep_order() {
        let music = Music {
            tracks: vec![track(72, 130), track(1, 0), track(73, 131)],
            extra: Map::new(),
        };
        let original = music.clone();
        let p = plan(&music).unwrap();
        assert_eq!(
            p.music.tracks.iter().map(|t| t.bank).collect::<Vec<_>>(),
            vec![2, 1, 3]
        );
        assert_eq!(&p.policy.main[..4], "1110");
        assert_eq!(&p.policy.custom[..4], "0110");
        assert_eq!(music, original);
    }
    #[test]
    fn selection_preserves_user_order_and_missing_never_silent() {
        let music = Music {
            tracks: vec![track(1, 0), track(2, 1)],
            extra: Map::new(),
        };
        assert_eq!(
            music
                .selected(&["1:2:1".into(), "1:1:0".into()])
                .unwrap()
                .tracks[0]
                .bank,
            2
        );
        assert!(music.selected(&["1:99:9".into()]).is_err());
        assert!(music.selected(&["1:1:0".into(), "1:1:0".into()]).is_err());
    }
    #[test]
    fn full_slots_do_not_truncate() {
        let mut music = Music {
            tracks: (1..=71).map(|i| track(i, i - 1)).collect(),
            extra: Map::new(),
        };
        assert!(plan(&music).is_ok());
        music.tracks.push(track(72, 130));
        assert!(plan(&music).is_err());
    }
    #[test]
    fn ids_and_banks_are_not_reused() {
        let mut music = Music {
            tracks: vec![track(72, 130)],
            extra: Map::new(),
        };
        let t = music.add_custom("New", "Artist", "001_helicopter").unwrap();
        assert_eq!((t.bank, t.unique), (73, 131));
    }
    #[test]
    fn masks_only_affect_full_exact_audio_paths() {
        let p = plan(&Music {
            tracks: vec![track(1, 0)],
            extra: Map::new(),
        })
        .unwrap()
        .policy;
        assert!(!p.blocked(&audio_path(1)).unwrap());
        assert!(p.blocked(&audio_path(2)).unwrap());
        for path in [
            "000002-000000-000000-000000-000000-000000.wav",
            "newaudio/commentary/000002-000000-000000-000000-000000-000000.wav",
            "newaudio/jukeboxmusic/directory_listing.adb2",
            "newaudio/jukeboxmusic/000002-000000-000000-000000-000000-000000.wav.bak",
        ] {
            assert!(!p.blocked(path).unwrap());
        }
        let mut bad = p.clone();
        bad.custom = "1".repeat(71);
        assert!(bad.loader_config().is_err());
    }
    #[test]
    fn version2_stock_above71_is_not_misidentified_as_custom() {
        let stock = Music {
            tracks: (1..=93).map(|i| track(i, i - 1)).collect(),
            extra: Map::new(),
        };
        let selected = stock.selected(&["1:93:92".into(), "1:1:0".into()]).unwrap();
        let p = plan_with_stock(&selected, &stock).unwrap();
        assert_eq!(p.mappings.len(), 1);
        assert_eq!(p.mappings[0].slot, 2);
        assert!(p.mappings[0].source_is_stock);
        let mut library = stock.clone();
        assert_eq!(
            library
                .add_custom("New", "Artist", "001_helicopter")
                .unwrap()
                .bank,
            94
        );
    }
}
