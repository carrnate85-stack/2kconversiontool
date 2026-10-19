//! Player editor sound preview: the Rust port of WPF core/player-audio-preview.js.
//! Stock English name clips are located through the game's CRC64 archive
//! index (`0A`) and newaudio/{players,paplayers}/directory_listing.adb2, then
//! the mono 48 kHz Opus packets are remuxed into Ogg. Read-only; no decoding,
//! no game memory, no game file writes.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    fs::File,
    os::windows::fs::FileExt,
    path::{Path, PathBuf},
};

/// CRC-64/XZ (ECMA polynomial, reflected) of the upper-case ASCII name.
pub(crate) fn archive_key(name: &str) -> u64 {
    let mut crc = u64::MAX;
    for b in name.to_ascii_uppercase().bytes() {
        crc ^= u64::from(b);
        for _ in 0..8 {
            crc = (crc >> 1)
                ^ if crc & 1 == 1 {
                    0xc96c_5795_d787_0f42
                } else {
                    0
                };
        }
    }
    crc ^ u64::MAX
}
fn page(packet: &[u8], sequence: u32, granule: u64, flags: u8) -> Vec<u8> {
    let mut lacing = vec![255u8; packet.len() / 255];
    lacing.push((packet.len() % 255) as u8);
    let mut out = vec![0u8; 27];
    out[..4].copy_from_slice(b"OggS");
    out[5] = flags;
    out[6..14].copy_from_slice(&granule.to_le_bytes());
    out[14..18].copy_from_slice(&1u32.to_le_bytes());
    out[18..22].copy_from_slice(&sequence.to_le_bytes());
    out[26] = lacing.len() as u8;
    out.extend(&lacing);
    out.extend(packet);
    let mut crc = 0u32;
    for b in &out {
        crc ^= u32::from(*b) << 24;
        for _ in 0..8 {
            crc = (crc << 1)
                ^ if crc & 0x8000_0000 != 0 {
                    0x04c1_1db7
                } else {
                    0
                };
        }
    }
    out[22..26].copy_from_slice(&crc.to_le_bytes());
    out
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
/// WPF toOgg(): validated stock clip container -> Ogg Opus.
pub(crate) fn to_ogg(raw: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        raw.len() >= 44
            && u32_at(raw, 0) == 0xbbcf_a17e
            && u32_at(raw, 4) == 36
            && u32_at(raw, 8) == 0x7581_95f9
            && u32_at(raw, 12) == 1
            && u32_at(raw, 16) == 48000
            && u32_at(raw, 28) as usize == raw.len() - 44,
        "This audio format is not supported by preview yet."
    );
    let samples = u64::from(u32_at(raw, 20));
    ensure!(
        samples > 0 && samples <= 48000 * 20,
        "Name clip exceeds the preview limit."
    );
    let mut packets = Vec::new();
    let (mut cursor, mut total) = (44usize, 0u64);
    while cursor < raw.len() {
        ensure!(cursor + 2 <= raw.len(), "Truncated audio packet.");
        let n = u16::from_le_bytes([raw[cursor], raw[cursor + 1]]) as usize;
        cursor += 2;
        ensure!(
            n > 0 && n <= 1275 && cursor + n <= raw.len(),
            "Invalid audio packet."
        );
        let p = &raw[cursor..cursor + n];
        cursor += n;
        // The two mono Opus frame configurations validated against stock clips.
        ensure!(
            p[0] == 0x78 || p[0] == 0x70,
            "This voice variant is not supported by preview yet."
        );
        total += if p[0] == 0x78 { 960 } else { 480 };
        packets.push((p, total));
        ensure!(packets.len() <= 2000, "Too many audio packets.");
    }
    ensure!(
        samples <= total && samples + 960 > total,
        "Audio duration is inconsistent."
    );
    let mut head = vec![0u8; 19];
    head[..8].copy_from_slice(b"OpusHead");
    head[8] = 1;
    head[9] = 1;
    head[12..16].copy_from_slice(&48000u32.to_le_bytes());
    let mut tags = vec![0u8; 16];
    tags[..8].copy_from_slice(b"OpusTags");
    let mut out = page(&head, 0, 0, 2);
    out.extend(page(&tags, 1, 0, 0));
    let last = packets.len().saturating_sub(1);
    for (i, (p, t)) in packets.iter().enumerate() {
        let (granule, flags) = if i == last { (samples, 4) } else { (*t, 0) };
        out.extend(page(p, i as u32 + 2, granule, flags));
    }
    Ok(out)
}
struct Archives {
    index: Vec<u8>,
    ranges: Vec<(PathBuf, u64, u64)>,
}
impl Archives {
    fn open(root: &Path) -> Result<Self> {
        let index_path = root.join("0A");
        let len = std::fs::metadata(&index_path)?.len();
        ensure!(
            (4096..=128 * 1024 * 1024).contains(&len),
            "Unsupported game archive index."
        );
        let index = std::fs::read(&index_path)?;
        ensure!(
            u32_at(&index, 0) == 0xaa00_b3bf && u32_at(&index, 8) == 1,
            "Unsupported game archive index version."
        );
        let mut names: Vec<String> = std::fs::read_dir(root)?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                n.len() == 2
                    && n.as_bytes()[0].is_ascii_digit()
                    && n.as_bytes()[1].is_ascii_uppercase()
            })
            .collect();
        names.sort();
        let mut ranges = Vec::new();
        let mut end = 0u64;
        for n in names {
            let file = root.join(&n);
            let meta = std::fs::metadata(&file)?;
            if !meta.is_file() {
                continue;
            }
            ranges.push((file, end, end + meta.len()));
            end += meta.len();
        }
        Ok(Self { index, ranges })
    }
    fn read(&self, name: &str, max: u32) -> Result<Vec<u8>> {
        let needle = archive_key(name).to_le_bytes();
        let hits: Vec<usize> = self
            .index
            .windows(8)
            .enumerate()
            .filter(|(_, w)| *w == needle)
            .map(|(i, _)| i)
            .take(2)
            .collect();
        ensure!(
            hits.len() == 1 && hits[0] + 24 <= self.index.len(),
            "Audio resource is missing or ambiguous."
        );
        let at = hits[0];
        let size = u32_at(&self.index, at + 8);
        let position = u64::from_le_bytes(self.index[at + 16..at + 24].try_into()?) >> 23;
        let range = self
            .ranges
            .iter()
            .find(|r| position >= r.1 && position < r.2)
            .filter(|r| size > 0 && size <= max && position + u64::from(size) <= r.2)
            .context("Audio archive range is invalid.")?;
        let mut data = vec![0u8; size as usize];
        let read = File::open(&range.0)?.seek_read(&mut data, position - range.1)?;
        ensure!(read == data.len(), "Incomplete audio read.");
        Ok(data)
    }
}
struct NoClip;
fn bank_preview(
    archives: &Archives,
    bank: &str,
    id: u32,
) -> std::result::Result<Value, Result<NoClip>> {
    let inner = || -> Result<Option<Value>> {
        let folder = if bank == "pa" {
            "newaudio/paplayers"
        } else {
            "newaudio/players"
        };
        let directory = archives.read(
            &format!("{folder}/directory_listing.adb2"),
            16 * 1024 * 1024,
        )?;
        ensure!(directory.len() % 28 == 0, "Unsupported audio directory.");
        let mut matches: Vec<([u32; 6], f32)> = directory
            .chunks_exact(28)
            .filter(|r| u32_at(r, 4) == id)
            .filter_map(|r| {
                let keys: [u32; 6] = std::array::from_fn(|i| u32_at(r, i * 4));
                let seconds = f32::from_le_bytes(r[24..28].try_into().unwrap());
                (keys.iter().all(|k| *k <= 999_999)
                    && seconds.is_finite()
                    && seconds > 0.
                    && seconds <= 20.)
                    .then_some((keys, seconds))
            })
            .collect();
        matches.sort_by_key(|m| (m.0[4], m.0[5]));
        if matches.is_empty() {
            return Ok(None);
        }
        let mut last = None;
        for (keys, seconds) in matches.iter().take(8) {
            let name = format!(
                "{folder}/{}.wav",
                keys.iter()
                    .map(|k| format!("{k:06}"))
                    .collect::<Vec<_>>()
                    .join("-")
            );
            let attempt = (|| -> Result<Value> {
                let raw = archives.read(&name, 1024 * 1024)?;
                let ogg = to_ogg(&raw)?;
                ensure!(
                    (f64::from(u32_at(&raw, 20)) / 48000. - f64::from(*seconds)).abs() <= 0.025,
                    "Audio and directory durations differ."
                );
                Ok(
                    json!({"base64":crate::roster_artwork::base64(&ogg),"variants":matches.len(),"context":keys[4],"seconds":seconds}),
                )
            })();
            match attempt {
                Ok(v) => return Ok(Some(v)),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap())
    };
    match inner() {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Err(Ok(NoClip)),
        Err(e) => Err(Err(e)),
    }
}
/// WPF preview(): auto tries commentary then PA.
pub(crate) fn preview(game: &Path, request: &Value) -> Result<Value> {
    let bank = request["bank"]
        .as_str()
        .filter(|b| matches!(*b, "auto" | "commentary" | "pa"));
    let id = request["id"]
        .as_f64()
        .filter(|v| v.fract() == 0. && (0. ..=32767.).contains(v));
    let (Some(bank), Some(id)) = (bank, id) else {
        bail!("Choose an assigned sound ID from 0 to 32767.");
    };
    let archives = Archives::open(game)?;
    let banks: &[&str] = if bank == "auto" {
        &["commentary", "pa"]
    } else {
        &[bank]
    };
    let mut all_missing = true;
    for b in banks {
        match bank_preview(&archives, b, id as u32) {
            Ok(mut v) => {
                v["bank"] = json!(b);
                return Ok(v);
            }
            Err(Ok(NoClip)) => {}
            Err(Err(_)) => all_missing = false,
        }
    }
    if all_missing {
        bail!(
            "No clip available for this sound ID in {}",
            match bank {
                "auto" => "stock English commentary or PA.",
                "pa" => "stock English PA.",
                _ => "stock English commentary.",
            }
        )
    }
    bail!("No supported stock preview could be loaded for this ID.")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
    #[test]
    fn archive_keys_and_ogg_remux_match_the_wpf_javascript() {
        let o: Value =
            serde_json::from_str(include_str!("../assets/audio-preview-regressions.json")).unwrap();
        for k in o["keys"].as_array().unwrap() {
            assert_eq!(
                format!("{:016x}", archive_key(k["name"].as_str().unwrap())),
                k["key"]
            );
        }
        for c in o["clips"].as_array().unwrap() {
            let result = to_ogg(&hex(c["raw"].as_str().unwrap()));
            match c["ogg"].as_str() {
                Some(ogg) => assert_eq!(result.unwrap(), hex(ogg)),
                None => assert_eq!(
                    result.unwrap_err().to_string(),
                    c["error"].as_str().unwrap()
                ),
            }
        }
    }
    #[test]
    fn requests_outside_wpf_limits_are_refused_before_reading() {
        let nowhere = Path::new(r"C:\definitely\missing");
        for bad in [
            json!({"bank":"auto","id":-1}),
            json!({"bank":"x","id":1}),
            json!({"bank":"pa","id":32768}),
            json!({"bank":"pa","id":1.5}),
        ] {
            assert!(
                preview(nowhere, &bad)
                    .unwrap_err()
                    .to_string()
                    .contains("0 to 32767")
            );
        }
    }
    #[test]
    #[ignore]
    fn installed_stock_name_clip_previews() {
        let game = PathBuf::from(std::env::var_os("C27_GAME_ROOT").expect("C27_GAME_ROOT"));
        let mut found = 0;
        for id in [1u32, 10, 100, 500, 1000] {
            if let Ok(v) = preview(&game, &json!({"bank":"auto","id":id})) {
                assert!(v["base64"].as_str().unwrap().starts_with("T2dnUw"));
                found += 1;
            }
        }
        println!("{found} of 5 sample IDs have stock clips");
        assert!(found > 0);
    }
}
