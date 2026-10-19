//! Player editor artwork (portraits, team logos, badge icons): the Rust port
//! of WPF core/roster-artwork.js and core/badge-icons.js. Stock resources are
//! read through fit_assets (the Rust Nba2kFastFitDecoder port: VCZ/Oodle,
//! external TLD textures -> DDS); the BC7 DDS is decoded by crate::bc7 and
//! resized like WPF's texconv call (-w 512 -h 512, badges -w 96 -h 96).
//! Read-only toward the game; PNGs live only in the launcher cache, manual
//! overrides only in Data/player-editor-artwork.json (WPF library format).
use anyhow::{Result, ensure};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

/// WPF candidates(): portrait chr_g{id} then chr_r{id}_a1; logo by name;
/// background = logo, with logo000's 1978 variant first.
pub(crate) fn candidates(kind: &str, id: &Value) -> Vec<String> {
    match kind {
        "background" => {
            let base = candidates("logo", id);
            if base.first().map(String::as_str) == Some("logos/logo000.iff") {
                std::iter::once("logos/logo000phi_1978.iff".to_owned())
                    .chain(base)
                    .collect()
            } else {
                base
            }
        }
        "portrait" => match id.as_u64().filter(|v| *v <= 65535) {
            Some(n) => vec![
                format!("player_images/chr_g{n:04}.iff"),
                format!("player_images/chr_r{n:04}_a1.iff"),
            ],
            None => vec![],
        },
        "logo" => {
            let Some(name) = id.as_str() else {
                return vec![];
            };
            let name = name
                .strip_suffix(".iff")
                .unwrap_or(name)
                .to_ascii_lowercase();
            let ok = name.len() > 4
                && name.len() <= 94
                && name.starts_with("logo")
                && name[4..]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_');
            if ok {
                vec![format!("logos/{name}.iff")]
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}
/// WPF badge-icons.js slug matching.
fn normalized(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}
pub(crate) fn wanted_slug(label: &str) -> String {
    let key = normalized(label);
    match key.as_str() {
        "deadeye" => "deadeye_scoring".into(),
        "lightninglaunch" => "lightning_laun".into(),
        "quicktrigger" => "quickdraw".into(),
        _ => key,
    }
}
pub(crate) fn member_slug(name: &str) -> Option<String> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let rest = base.strip_prefix("icon_mycareer_badge_")?;
    let rest = rest.strip_suffix(".dds")?;
    let (slug, hash) = rest.rsplit_once('.')?;
    (!slug.is_empty() && !hash.is_empty() && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| normalized(slug))
}
/// DDS (BC7 DX10 98/99, DXT1/DXT5) top mip -> RGBA8.
pub(crate) fn decode_dds(dds: &[u8]) -> Result<(usize, usize, Vec<u8>)> {
    ensure!(
        dds.len() >= 128 && dds.len() <= 24 * 1024 * 1024 && &dds[..4] == b"DDS ",
        "Unsupported artwork texture."
    );
    let u = |p: usize| u32::from_le_bytes(dds[p..p + 4].try_into().unwrap());
    let (h, w) = (u(12) as usize, u(16) as usize);
    ensure!(
        (1..=4096).contains(&w) && (1..=4096).contains(&h),
        "Unsupported artwork texture."
    );
    let pixels = match &dds[84..88] {
        b"DX10" => {
            ensure!(dds.len() >= 148, "Unsupported artwork texture.");
            match u(128) {
                97..=99 => crate::bc7::image(&dds[148..], w, h)?,
                71 | 72 => crate::bc7::bc1_bc3(&dds[148..], w, h, false)?,
                77 | 78 => crate::bc7::bc1_bc3(&dds[148..], w, h, true)?,
                f => anyhow::bail!("Unsupported artwork texture format {f}."),
            }
        }
        b"DXT1" => crate::bc7::bc1_bc3(&dds[128..], w, h, false)?,
        b"DXT5" => crate::bc7::bc1_bc3(&dds[128..], w, h, true)?,
        _ => anyhow::bail!("Unsupported artwork texture."),
    };
    Ok((w, h, pixels))
}
/// Area-average resample to exactly `tw` x `th` (texconv -w/-h stretches).
pub(crate) fn resize(w: usize, h: usize, rgba: &[u8], tw: usize, th: usize) -> Vec<u8> {
    let mut out = vec![0u8; tw * th * 4];
    for ty in 0..th {
        let (y0, y1) = (
            ty * h / th,
            ((ty + 1) * h).div_ceil(th).max(ty * h / th + 1),
        );
        for tx in 0..tw {
            let (x0, x1) = (
                tx * w / tw,
                ((tx + 1) * w).div_ceil(tw).max(tx * w / tw + 1),
            );
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    let p = (y * w + x) * 4;
                    // Premultiply so transparent texels do not bleed color.
                    let a = u64::from(rgba[p + 3]);
                    for c in 0..3 {
                        sum[c] += u64::from(rgba[p + c]) * a;
                    }
                    sum[3] += a;
                    n += 1;
                }
            }
            let o = (ty * tw + tx) * 4;
            if sum[3] > 0 {
                for c in 0..3 {
                    out[o + c] = ((sum[c] + sum[3] / 2) / sum[3]) as u8;
                }
            }
            out[o + 3] = ((sum[3] + n / 2) / n.max(1)) as u8;
        }
    }
    out
}
pub(crate) fn png(w: usize, h: usize, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w as u32, h as u32);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()?.write_image_data(rgba)?;
    }
    Ok(out)
}
pub(crate) fn base64(bytes: &[u8]) -> String {
    base64_engine::encode(bytes)
}
fn data_url(png: &[u8]) -> String {
    use base64_engine::encode;
    format!("data:image/png;base64,{}", encode(png))
}
mod base64_engine {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
        for c in b.chunks(3) {
            let n = (u32::from(c[0]) << 16)
                | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                | u32::from(*c.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= c.len() {
                    s.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }
}
fn valid_png(b: &[u8], limit: usize) -> bool {
    b.len() >= 24 && b.len() <= limit && b[..8] == [137, 80, 78, 71, 13, 10, 26, 10]
}
pub(crate) struct Service {
    assets: crate::fit_assets::FitAssets,
    cache: PathBuf,
    library: PathBuf,
    memory: Mutex<Vec<(String, String)>>,
}
impl Service {
    pub fn new(root: &Path) -> Self {
        Self {
            assets: crate::fit_assets::FitAssets::default(),
            cache: root.join("Cache/Player Editor"),
            library: root.join("Data/player-editor-artwork.json"),
            memory: Mutex::new(Vec::new()),
        }
    }
    fn remember(&self, key: &str) -> Option<String> {
        self.memory
            .lock()
            .ok()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
    fn store(&self, key: String, value: String, limit: usize) {
        if let Ok(mut m) = self.memory.lock() {
            m.retain(|(k, _)| *k != key);
            m.push((key, value));
            while m.len() > limit {
                m.remove(0);
            }
        }
    }
    /// WPF manual override (choose:true) library lookup: key sha256(kind:key).
    pub fn manual(&self, kind: &str, key: &str) -> Result<Option<String>> {
        let Ok(meta) = std::fs::metadata(&self.library) else {
            return Ok(None);
        };
        ensure!(
            meta.len() <= 32 * 1024 * 1024,
            "Artwork library exceeds its size limit."
        );
        let library: Value = serde_json::from_slice(&std::fs::read(&self.library)?)?;
        let id = format!("{:x}", Sha256::digest(format!("{kind}:{key}")));
        Ok(library[&id]
            .as_str()
            .filter(|v| v.starts_with("data:image/"))
            .map(str::to_owned))
    }
    fn texture(&self, game: &Path, name: &str) -> Result<Option<Vec<u8>>> {
        let g = self.assets.game(game)?;
        if g.artwork_size(name).is_none() {
            return Ok(None);
        }
        let zip = g.export_artwork(name, &|| Ok(()))?;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip))?;
        let mut dds = Vec::new();
        for i in 0..z.len() {
            use std::io::Read;
            let mut f = z.by_index(i)?;
            if f.name().to_ascii_lowercase().ends_with(".dds") {
                let mut b = Vec::new();
                f.read_to_end(&mut b)?;
                dds.push(b);
            }
        }
        // WPF: exactly one texture, otherwise no artwork.
        Ok((dds.len() == 1).then(|| dds.pop().unwrap()))
    }
    /// WPF RosterArtwork.get(root, kind, asset) -> PNG data URL or null.
    pub fn artwork(&self, game: &Path, kind: &str, asset: &Value) -> Result<Option<String>> {
        let names = candidates(kind, asset);
        let g = self.assets.game(game)?;
        let Some(name) = names.iter().find(|n| g.artwork_size(n).is_some()) else {
            return Ok(None);
        };
        let key = format!(
            "{:x}",
            Sha256::digest(format!("{}:{name}", g.artwork_key(name)?))
        );
        if let Some(v) = self.remember(&key) {
            return Ok(Some(v));
        }
        let dir = self.cache.join("art-cache");
        let file = dir.join(format!("{key}.png"));
        let data = match std::fs::read(&file) {
            Ok(b) if valid_png(&b, 2 * 1024 * 1024) => b,
            _ => {
                let Some(dds) = self.texture(game, name)? else {
                    return Ok(None);
                };
                let (w, h, rgba) = decode_dds(&dds)?;
                let png = png(512, 512, &resize(w, h, &rgba, 512, 512))?;
                ensure!(png.len() <= 2 * 1024 * 1024, "Artwork is too large.");
                std::fs::create_dir_all(&dir)?;
                let temp = dir.join(format!("{key}.{}.tmp", crate::backend::unique_id()));
                std::fs::write(&temp, &png)?;
                let _ = std::fs::rename(&temp, &file);
                let _ = std::fs::remove_file(&temp);
                prune(&dir, 128 * 1024 * 1024);
                png
            }
        };
        let value = data_url(&data);
        self.store(key, value.clone(), 32);
        Ok(Some(value))
    }
    /// WPF BadgeIcons.get(root, labels) -> {label: data URL}.
    pub fn badges(&self, game: &Path, labels: &[String]) -> Result<Map<String, Value>> {
        let mut out = Map::new();
        let g = self.assets.game(game)?;
        if g.artwork_size("badgeicons.iff").is_none() {
            return Ok(out);
        }
        let stamp = format!("{:x}", Sha256::digest(g.artwork_key("badgeicons.iff")?));
        let mut missing = Vec::new();
        for label in labels {
            let key = format!("{}-{}", &stamp[..20], normalized(label));
            if let Some(v) = self.remember(&key) {
                out.insert(label.clone(), json!(v));
            } else {
                missing.push((label.clone(), key));
            }
        }
        if missing.is_empty() {
            return Ok(out);
        }
        let zip = g.export_artwork("badgeicons.iff", &|| Ok(()))?;
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip))?;
        ensure!(z.len() <= 1000, "Badge icon archive has too many members.");
        let mut members = Vec::new();
        for i in 0..z.len() {
            let f = z.by_index(i)?;
            if let Some(slug) = member_slug(f.name()) {
                members.push((slug, i));
            }
        }
        for (label, key) in missing {
            let wanted = normalized(&wanted_slug(&label));
            let Some(&(_, i)) = members.iter().find(|(s, _)| *s == wanted) else {
                continue;
            };
            let mut b = Vec::new();
            {
                use std::io::Read;
                z.by_index(i)?.read_to_end(&mut b)?;
            }
            let Ok((w, h, rgba)) = decode_dds(&b) else {
                continue;
            };
            let png = png(96, 96, &resize(w, h, &rgba, 96, 96))?;
            if !valid_png(&png, 512 * 1024) {
                continue;
            }
            let value = data_url(&png);
            self.store(key, value.clone(), 96);
            out.insert(label, json!(value));
        }
        Ok(out)
    }
}
fn prune(dir: &Path, limit: u64) {
    let Ok(rows) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = rows
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".png"))
        .filter_map(|e| {
            Some((
                e.metadata().ok()?.modified().ok()?,
                e.metadata().ok()?.len(),
                e.path(),
            ))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= limit {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= len;
        }
    }
}
/// The player's PhotoId (proven V2 field, inside ratingsRaw).
pub(crate) fn photo_id(player: &Value) -> Option<u64> {
    let index = crate::roster_snapshot::rating_fields()
        .iter()
        .position(|f| f.key == "PhotoId")?;
    let raw = player["ratingsRaw"].as_array()?;
    (raw.len() == crate::roster_snapshot::rating_fields().len())
        .then(|| raw[index].as_u64())
        .flatten()
}
/// WPF logo asset: team mainLogo if `logo<digit>...`, else logo{teamId:000}.
pub(crate) fn logo_asset(team: &Value) -> Option<String> {
    let main = team["mainLogo"].as_str().unwrap_or("");
    if main.len() > 4
        && main[..4].eq_ignore_ascii_case("logo")
        && main.as_bytes()[4].is_ascii_digit()
    {
        return Some(main.to_owned());
    }
    team["teamId"]
        .as_u64()
        .filter(|v| *v <= 65535)
        .map(|id| format!("logo{id:03}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wpf_candidate_and_badge_slug_rules() {
        assert_eq!(
            candidates("portrait", &json!(4090)),
            [
                "player_images/chr_g4090.iff",
                "player_images/chr_r4090_a1.iff"
            ]
        );
        assert_eq!(
            candidates("portrait", &json!(7)),
            [
                "player_images/chr_g0007.iff",
                "player_images/chr_r0007_a1.iff"
            ]
        );
        assert!(candidates("portrait", &json!(70000)).is_empty());
        assert_eq!(
            candidates("background", &json!("logo000")),
            ["logos/logo000phi_1978.iff", "logos/logo000.iff"]
        );
        assert_eq!(
            candidates("logo", &json!("LOGO012.iff")),
            ["logos/logo012.iff"]
        );
        assert!(candidates("logo", &json!("../x")).is_empty());
        assert!(candidates("logo", &json!("logo")).is_empty());
        assert_eq!(
            logo_asset(&json!({"mainLogo":"logo123_alt","teamId":5})).as_deref(),
            Some("logo123_alt")
        );
        assert_eq!(
            logo_asset(&json!({"mainLogo":"lakers","teamId":5})).as_deref(),
            Some("logo005")
        );
        assert_eq!(wanted_slug("Deadeye"), "deadeye_scoring");
        assert_eq!(wanted_slug("Quick Trigger"), "quickdraw");
        assert_eq!(
            member_slug("icon_mycareer_badge_post_spin_tech.1a2b.dds").as_deref(),
            Some("postspintech")
        );
        assert_eq!(member_slug("icon_badge_gold.2f5b.dds"), None);
        assert_eq!(
            normalized(&wanted_slug("Deadeye")),
            member_slug("icon_mycareer_badge_deadeye_scoring.ff.dds").unwrap()
        );
    }
    #[test]
    fn resize_png_and_data_url_are_well_formed() {
        let mut rgba = vec![0u8; 8 * 8 * 4];
        for p in rgba.chunks_mut(4) {
            p.copy_from_slice(&[200, 100, 50, 255]);
        }
        let small = resize(8, 8, &rgba, 4, 4);
        assert!(small.chunks(4).all(|p| p == [200, 100, 50, 255]));
        let big = resize(8, 8, &rgba, 12, 6);
        assert_eq!(big.len(), 12 * 6 * 4);
        let mut half = rgba.clone();
        for p in half.chunks_mut(8) {
            p[4..8].copy_from_slice(&[0, 0, 0, 0]); // transparent neighbor
        }
        assert_eq!(
            &resize(8, 8, &half, 4, 4)[..3],
            &[200, 100, 50],
            "no dark fringe"
        );
        let png = png(4, 4, &small).unwrap();
        assert!(valid_png(&png, 4096));
        assert!(data_url(&png).starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert_eq!(base64_engine::encode(b"Ma"), "TWE=");
        assert_eq!(base64_engine::encode(b"Man"), "TWFu");
    }
    #[test]
    fn shared_bc7_decoder_matches_texconv_on_every_mode_and_partition() {
        let oracle: Value =
            serde_json::from_str(include_str!("../assets/bc7-oracle.json")).unwrap();
        let hex = |s: &str| -> Vec<u8> {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect()
        };
        for (n, row) in oracle["blocks"].as_array().unwrap().iter().enumerate() {
            let input: [u8; 16] = hex(row[0].as_str().unwrap()).try_into().unwrap();
            let got: Vec<u8> = crate::bc7::block(&input)
                .iter()
                .flatten()
                .copied()
                .collect();
            assert_eq!(got, hex(row[1].as_str().unwrap()), "block {n}");
        }
    }
    #[test]
    #[ignore]
    fn installed_portrait_logo_and_badges_decode_from_stock_files() {
        let game = PathBuf::from(std::env::var_os("C27_GAME_ROOT").expect("C27_GAME_ROOT"));
        let dir = std::env::temp_dir().join(format!("art-{}", crate::backend::unique_id()));
        let s = Service::new(&dir);
        let portrait = s.artwork(&game, "portrait", &json!(4090)).unwrap().unwrap();
        let logo = s
            .artwork(&game, "background", &json!("logo000"))
            .unwrap()
            .unwrap();
        let badges = s
            .badges(&game, &["Deadeye".into(), "Bogus Badge".into()])
            .unwrap();
        println!(
            "{}",
            json!({"portraitBytes":portrait.len(),"logoBytes":logo.len(),"badges":badges.keys().collect::<Vec<_>>()})
        );
        assert!(portrait.starts_with("data:image/png;base64,"));
        if let Some(out) = std::env::var_os("C27_ART_DUMP") {
            for (name, url) in [("portrait", &portrait), ("logo", &logo)] {
                std::fs::write(PathBuf::from(&out).join(format!("{name}.txt")), url).unwrap();
            }
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
