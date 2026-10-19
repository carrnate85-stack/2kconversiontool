//! Player editor header archetype labels: the Rust port of WPF
//! core/player-header-labels.js (+ camera-localization.js resolve). The key is
//! the player's cached archetype description (player+0xE8, valid only when
//! +0x418 bit 7 is set, V2 PLAYERDATA::GetCachedDescription 0x2E20580). The
//! stock loc/english.iff member is read through the same fixed, read-only
//! route as camera names; nothing is written and no game memory is touched.
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

type Stamp = (PathBuf, u64, SystemTime);
static CACHE: Mutex<Option<(Stamp, Vec<u8>)>> = Mutex::new(None);

fn stamp(game: &Path) -> Result<Stamp> {
    let meta = std::fs::metadata(game.join("manifest"))?;
    Ok((game.to_path_buf(), meta.len(), meta.modified()?))
}
fn text(game: &Path) -> Result<Vec<u8>> {
    let before = stamp(game)?;
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("Header label cache unavailable"))?;
    if let Some((s, text)) = cache.as_ref() {
        if *s == before {
            return Ok(text.clone());
        }
    }
    let check = || Ok(());
    let resource = crate::stock_selectors::Resource::camera_localization(game, check)?;
    let packed = resource.packed(game, check)?;
    let archive = crate::stock_selectors::Decoder::load(game)?.decode(&packed, check)?;
    let text = crate::selector_tables::archive(&archive)?
        .get("TEXT.VCLOCALIZEDATA")
        .context("English player labels are unavailable.")?
        .clone();
    ensure!(
        text.len() <= 32 * 1024 * 1024,
        "English archive exceeds its size limit."
    );
    ensure!(
        stamp(game)? == before,
        "Game data changed during label loading."
    );
    *cache = Some((before, text.clone()));
    Ok(text)
}
/// WPF camera-localization resolve(): 64-byte key rows (keys in the first 40
/// bytes, u32 UTF-16 offsets at +40), string pool rounded to 64 bytes.
pub(crate) fn resolve(text: &[u8], key: u64) -> Result<String> {
    ensure!(text.len() >= 64, "Localization size is invalid");
    let declared = u64::from_le_bytes(text[24..32].try_into()?);
    let pool = usize::try_from(declared.checked_add(63).context("Pool overflow")? & !63)?;
    ensure!(pool >= 64 && pool < text.len(), "Unsupported string pool");
    let needle = key.to_le_bytes();
    let mut found = None;
    for at in (64..pool).step_by(8).filter(|at| at % 64 < 40) {
        if text.get(at..at + 8) == Some(needle.as_slice()) {
            ensure!(
                found.replace(at).is_none(),
                "Localization key is not unique"
            );
        }
    }
    let at = found.context("Localization key is missing")?;
    let field = (at / 64) * 64 + 40 + (at % 64 / 8) * 4;
    let offset = u32::from_le_bytes(text[field..field + 4].try_into()?) as usize;
    let mut cursor = pool
        .checked_add(offset.checked_mul(2).context("Offset overflow")?)
        .context("Offset overflow")?;
    let mut words = Vec::new();
    loop {
        let w = text
            .get(cursor..cursor + 2)
            .context("Label is outside its pool")?;
        let w = u16::from_le_bytes([w[0], w[1]]);
        if w == 0 {
            break;
        }
        words.push(w);
        cursor += 2;
        ensure!(words.len() <= 4096, "Label exceeds bounds");
    }
    Ok(String::from_utf16(&words)?)
}
/// WPF archetypeLabel(): only a real 16-hex key, a short printable label.
pub(crate) fn archetype_label(text: &[u8], key: &str) -> Option<String> {
    if key.len() != 16 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u64::from_str_radix(key, 16).ok()?;
    if value == 0 || value == u64::MAX {
        return None;
    }
    let label = resolve(text, value).ok()?;
    (!label.is_empty()
        && label.chars().count() <= 120
        && !label.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}'))
    .then_some(label)
}
pub(crate) fn label(game: &Path, key: &str) -> Result<Option<String>> {
    let valid =
        key.len() == 16 && u64::from_str_radix(key, 16).is_ok_and(|v| v != 0 && v != u64::MAX);
    if !valid {
        return Ok(None);
    }
    Ok(archetype_label(&text(game)?, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn table(rows: &[(u64, &str)]) -> Vec<u8> {
        let mut pool_words: Vec<u16> = Vec::new();
        let mut offsets = Vec::new();
        for (_, label) in rows {
            offsets.push(pool_words.len() as u32);
            pool_words.extend(label.encode_utf16());
            pool_words.push(0);
        }
        let key_bytes = 64 + rows.len().div_ceil(5) * 64;
        let mut text = vec![0u8; key_bytes];
        text[24..32].copy_from_slice(&(key_bytes as u64).to_le_bytes());
        for (i, (key, _)) in rows.iter().enumerate() {
            let row = 64 + (i / 5) * 64;
            text[row + (i % 5) * 8..row + (i % 5) * 8 + 8].copy_from_slice(&key.to_le_bytes());
            text[row + 40 + (i % 5) * 4..row + 44 + (i % 5) * 4]
                .copy_from_slice(&offsets[i].to_le_bytes());
        }
        for w in pool_words {
            text.extend(w.to_le_bytes());
        }
        text
    }
    #[test]
    fn archetype_labels_follow_the_wpf_resolver_and_filters() {
        let text = table(&[
            (0x1234, "Slashing Playmaker"),
            (0xABCDEF0011223344, "Post Scorer"),
            (7, "Bad\u{7}"),
        ]);
        assert_eq!(
            archetype_label(&text, "0000000000001234").as_deref(),
            Some("Slashing Playmaker")
        );
        assert_eq!(
            archetype_label(&text, "abcdef0011223344").as_deref(),
            Some("Post Scorer")
        );
        assert_eq!(
            archetype_label(&text, "0000000000000007"),
            None,
            "control characters"
        );
        assert_eq!(archetype_label(&text, "0000000000000000"), None);
        assert_eq!(archetype_label(&text, "ffffffffffffffff"), None);
        assert_eq!(archetype_label(&text, "1234"), None);
        assert_eq!(
            archetype_label(&text, "000000000000beef"),
            None,
            "missing key"
        );
        let dup = table(&[(5, "A"), (5, "B")]);
        assert_eq!(
            archetype_label(&dup, "0000000000000005"),
            None,
            "duplicate key"
        );
    }
}
