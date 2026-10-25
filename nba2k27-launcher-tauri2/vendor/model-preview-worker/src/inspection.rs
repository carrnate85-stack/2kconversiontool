//! Read-only, bounded WPF-style content views; Save never uses these samples.
use crate::codec;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Write};
pub const MEMBER_MAX: usize = 64 * 1024 * 1024;
pub const TEXT_MAX: usize = 512 * 1024;
pub const HEX_MAX: usize = 64 * 1024;
const LINE_MAX: usize = 20_000;
/// Member-list labels describe the source; they do not decode or mutate it.
pub fn member_summary(name: &str, compression: &str, encrypted: bool) -> Value {
    let extension = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let kind: String = match extension.as_str() {
        "txt" | "ini" | "cfg" | "log" | "lua" | "shader" => "Text".into(),
        "" => "Binary".into(),
        _ => extension.to_ascii_uppercase(),
    };
    let status = if encrypted {
        "Encrypted member; unsupported"
    } else if !matches!(compression, "Stored" | "Deflate") {
        "Unsupported ZIP compression"
    } else {
        match kind.as_str() {
            "SCNE" => "Decoded SCNE text",
            "DDS" => "Ready DDS texture",
            "BIN" => "Expanded geometry buffer",
            "TLD" => "Preserved native TLD",
            "TXTR" => "Decodable TXTR metadata",
            "GZ" => "Preserved compressed buffer",
            _ if compression == "Deflate" => "ZIP deflate",
            _ => "Stored member",
        }
    };
    json!({"kind":kind,"compression":compression,"status":status})
}

/// Game SCNE documents use a named root fragment. Standalone JSON inspection
/// and Save decoded need its outer object, without parsing/reserializing values.
pub fn decoded_json(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let document = codec::document(bytes, &BTreeMap::new())?;
    if bytes.starts_with(&0xe01ff891u32.to_le_bytes()) {
        let mut json = Vec::with_capacity(document.len() + 2);
        json.push(b'{');
        json.extend_from_slice(&document);
        json.push(b'}');
        Ok(json)
    } else {
        Ok(document)
    }
}

pub fn notice(name: &str, bytes: u64) -> Value {
    json!({"name":name,"text":"Preview disabled for members larger than 64 MiB. Use Save original to save the complete unchanged member.","mode":"notice","decoded":false,"bytes":bytes,"shownBytes":0,"shownLines":1,"truncated":true})
}
pub fn preview(name: &str, bytes: &[u8], source_bytes: u64) -> Value {
    let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let document = matches!(extension.as_str(), "scne" | "txtr" | "json");
    // Direct record emission preserves binary-tree object order, duplicate keys
    // and full numeric precision. No JavaScript-safe string substitutions.
    let decoded = document.then(|| decoded_json(bytes)).and_then(Result::ok);
    let source = decoded.as_deref().unwrap_or(bytes);
    let sample = &source[..source.len().min(8192)];
    let known_text = matches!(
        extension.as_str(),
        "xml" | "txt" | "csv" | "ini" | "cfg" | "log" | "lua" | "shader"
    );
    let text = decoded.is_some()
        || (!sample.contains(&0)
            && (known_text
                || sample.is_empty()
                || sample
                    .iter()
                    .filter(|b| matches!(**b, 9 | 10 | 13 | 32..=255))
                    .count() as f64
                    >= sample.len() as f64 * 0.88));
    let shown = &source[..source.len().min(if text { TEXT_MAX } else { HEX_MAX })];
    let mut content = if text {
        String::from_utf8_lossy(shown)
            .trim_start_matches('\u{feff}')
            .replace('\0', "\u{fffd}")
    } else {
        let mut content = String::new();
        for (row, chunk) in shown.chunks(16).enumerate() {
            if row > 0 {
                content.push('\n');
            }
            write!(&mut content, "{:08X}  ", row * 16).unwrap();
            for i in 0..16 {
                if let Some(byte) = chunk.get(i) {
                    write!(&mut content, "{byte:02X} ").unwrap();
                } else {
                    content.push_str("   ");
                }
            }
            content.push(' ');
            content.extend(chunk.iter().map(|b| {
                if (32..=126).contains(b) {
                    *b as char
                } else {
                    '.'
                }
            }));
        }
        content
    };
    let mut lines = content.split('\n');
    let limit = lines.by_ref().take(LINE_MAX).collect::<Vec<_>>();
    let line_truncated = lines.next().is_some();
    let shown_lines = limit.len();
    if line_truncated {
        content = limit.join("\n");
    }
    json!({"name":name,"text":content,"decoded":decoded.is_some(),"mode":if !text{"hex"}else if decoded.is_some(){"decoded"}else{"text"},"kind":extension.to_ascii_uppercase(),"bytes":source_bytes,"decodedBytes":decoded.as_ref().map(Vec::len),"shownBytes":shown.len(),"shownLines":shown_lines,"truncated":shown.len()<source.len() || line_truncated})
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn member_labels_describe_sources_without_decoding_them() {
        assert_eq!(
            member_summary("Head.TXTR", "Stored", false),
            json!({"kind":"TXTR","compression":"Stored","status":"Decodable TXTR metadata"})
        );
        assert_eq!(
            member_summary("readme.txt", "Deflate", false)["status"],
            "ZIP deflate"
        );
        assert_eq!(member_summary("buffer", "Stored", false)["kind"], "Binary");
        assert_eq!(
            member_summary("bad.dds", "Unsupported(12)", false)["status"],
            "Unsupported ZIP compression"
        );
        assert_eq!(
            member_summary("bad.dds", "Stored", true)["status"],
            "Encrypted member; unsupported"
        );
    }
    pub fn wide_integer_tree() -> Vec<u8> {
        let mut b = vec![0; 149];
        b[..4].copy_from_slice(&0xe01ff891u32.to_le_bytes());
        b[4..8].copy_from_slice(&1u32.to_le_bytes());
        b[8..12].copy_from_slice(&5u32.to_le_bytes());
        b[12..16].copy_from_slice(&3u32.to_le_bytes());
        b[16..24].copy_from_slice(&129i64.to_le_bytes());
        b[144..].copy_from_slice(b"root\0");
        b[28..32].copy_from_slice(&0x40000002u32.to_le_bytes());
        b[36..40].copy_from_slice(&2u32.to_le_bytes());
        for (p, v) in [(64, u64::MAX), (104, 9_007_199_254_740_993u64)] {
            b[p + 4..p + 8].copy_from_slice(&0x40000006u32.to_le_bytes());
            b[p + 16..p + 24].copy_from_slice(&v.to_le_bytes());
        }
        b
    }
    #[test]
    fn decoded_view_preserves_wide_numbers_not_preview_strings() {
        let b = wide_integer_tree();
        let p = preview("level.SCNE", &b, b.len() as u64);
        let parsed: Value = serde_json::from_str(p["text"].as_str().unwrap()).unwrap();
        assert_eq!(parsed["root"][0].as_u64(), Some(u64::MAX));
        assert_eq!(parsed["root"][1].as_u64(), Some(9_007_199_254_740_993));
        assert_eq!(p["decoded"], true);
    }
    #[test]
    fn text_and_hex_samples_and_lines_are_bounded() {
        let p = preview(
            "readme.txt",
            &vec![b'x'; TEXT_MAX + 20],
            (TEXT_MAX + 20) as u64,
        );
        assert_eq!(p["text"].as_str().unwrap().len(), TEXT_MAX);
        assert_eq!(p["truncated"], true);
        let p = preview("buffer.bin", &vec![0; HEX_MAX + 20], (HEX_MAX + 20) as u64);
        assert_eq!(p["mode"], "hex");
        assert_eq!(p["shownBytes"], HEX_MAX);
        assert!(p["text"].as_str().unwrap().starts_with("00000000  00 00"));
        let p = preview(
            "lines.txt",
            &vec![b'\n'; LINE_MAX + 1],
            (LINE_MAX + 1) as u64,
        );
        assert_eq!(p["shownLines"], LINE_MAX);
        assert_eq!(p["truncated"], true);
    }
    #[test]
    fn malformed_documents_stay_inspectable_and_original_bytes_untouched() {
        let b = b"{broken json";
        let p = preview("bad.json", b, b.len() as u64);
        assert_eq!(p["text"], "{broken json");
        assert_eq!(p["decoded"], false);
        let p = preview("bad.TXTR", &[0, 1, 2, 3], 4);
        assert_eq!(p["mode"], "hex");
        assert_eq!(notice("big.bin", (MEMBER_MAX + 1) as u64)["mode"], "notice");
    }
}
