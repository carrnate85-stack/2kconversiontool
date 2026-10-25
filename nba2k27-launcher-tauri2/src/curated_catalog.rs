//! Public curator folders are the source of truth. Renderer supplies IDs, never URLs.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const FOLDERS: [(&str, &str); 2] =
    [("videos", "42pf1wsmrcs2t"), ("playlists", "3d9j5r16tk85y")];
pub const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
pub const PREVIEW_ID: &str = "previews-blacktop";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Download {
    pub url: String,
    pub filename: String,
    pub bytes: u64,
    pub sha256: String,
}
pub fn link_key(value: &str) -> Result<String> {
    ensure!(value.len() <= 2048, "MediaFire link is too long");
    let u = crate::curated_http::safe_url(value)?;
    let p: Vec<_> = u.path().split('/').collect();
    ensure!(
        u.host_str() == Some("www.mediafire.com")
            && u.query().is_none()
            && p.len() >= 4
            && matches!(p[1], "file" | "file_premium")
            && (10..=32).contains(&p[2].len())
            && p[2].bytes().all(|b| b.is_ascii_alphanumeric()),
        "Paste a public MediaFire file link"
    );
    Ok(p[2].into())
}
pub fn validate_download(id: &str, d: &Download) -> Result<()> {
    link_key(&d.url)?;
    let ext = if id == PREVIEW_ID {
        ".zip"
    } else if id.starts_with("videos-") {
        ".mp4"
    } else {
        ".2kplaylist"
    };
    ensure!(
        !d.filename.is_empty()
            && d.filename.len() <= 180
            && !d.filename.chars().any(char::is_control)
            && !d
                .filename
                .contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
            && d.filename.to_ascii_lowercase().ends_with(ext),
        "Choose a {ext} file for this download"
    );
    ensure!(
        (1..=MAX_FILE).contains(&d.bytes)
            && d.sha256.len() == 64
            && d.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid download size or checksum"
    );
    Ok(())
}
pub fn resolve_download(
    id: &str,
    link: &str,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Download> {
    let key = link_key(link)?;
    let url = format!(
        "https://www.mediafire.com/api/1.5/file/get_info.php?quick_key={key}&response_format=json"
    );
    let mut bytes = vec![];
    crate::curated_http::fetch(
        &url,
        &mut bytes,
        2 * 1024 * 1024,
        "",
        true,
        cancel,
        |_, _| {},
    )?;
    let value: Value = serde_json::from_slice(&bytes)?;
    let f = &value["response"]["file_info"];
    ensure!(
        value["response"]["result"] == "Success"
            && f["quickkey"] == key
            && f["privacy"] == "public"
            && f["password_protected"] == "no"
            && f["ready"] == "yes",
        "MediaFire file must be public, ready, and not password protected"
    );
    let d = Download {
        url: text(&f["links"], "normal_download", 2048)?.into(),
        filename: text(f, "filename", 180)?.into(),
        bytes: text(f, "size", 20)?.parse()?,
        sha256: text(f, "hash", 64)?.to_ascii_lowercase(),
    };
    ensure!(
        link_key(&d.url)? == key,
        "MediaFire returned a different file"
    );
    validate_download(id, &d)?;
    Ok(d)
}
pub fn preview_pack() -> Entry {
    Entry {
        id: PREVIEW_ID.into(),
        kind: "previews".into(),
        name: "Blacktop Preview Pack".into(),
        filename: "Blacktop previews.zip".into(),
        description: "Rendered clothing and accessory thumbnails for the Blacktop catalog.".into(),
        bytes: crate::preview_download::BYTES,
        sha256: crate::preview_download::SHA256.into(),
        url: crate::preview_download::URL.into(),
        revision: "1".into(),
        supported: true,
        reason: String::new(),
        artwork: "art/blacktop-previews.svg".into(),
        order: 0,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub filename: String,
    pub description: String,
    pub bytes: u64,
    pub sha256: String,
    pub url: String,
    pub revision: String,
    pub supported: bool,
    pub reason: String,
    #[serde(default)]
    pub artwork: String,
    #[serde(default)]
    pub order: i32,
}
fn text<'a>(v: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    let s = v[key]
        .as_str()
        .with_context(|| format!("MediaFire listing is missing {key}"))?;
    ensure!(
        s.len() <= max && !s.chars().any(char::is_control),
        "Invalid listing field {key}"
    );
    Ok(s)
}
pub fn page(value: &Value, kind: &str, folder: &str, chunk: usize) -> Result<(Vec<Entry>, bool)> {
    ensure!(FOLDERS.contains(&(kind, folder)), "Unknown curated folder");
    let response = &value["response"];
    ensure!(
        response["result"] == "Success",
        "MediaFire could not list this folder. Try Refresh later."
    );
    let content = &response["folder_content"];
    ensure!(
        content["folderkey"] == folder
            && content["content_type"] == "files"
            && content["chunk_number"].as_str() == Some(&chunk.to_string()),
        "MediaFire returned a different folder or page"
    );
    let files = content["files"]
        .as_array()
        .context("MediaFire file listing is missing")?;
    ensure!(files.len() <= 100, "MediaFire listing exceeds page limit");
    let mut entries = vec![];
    for f in files {
        if f["privacy"] != "public" || f["password_protected"] != "no" {
            continue;
        }
        let filename = text(f, "filename", 180)?;
        ensure!(
            !filename.is_empty()
                && !filename.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
                && !filename.ends_with([' ', '.']),
            "Unsupported download filename"
        );
        let lower = filename.to_ascii_lowercase();
        if !(if kind == "videos" {
            lower.ends_with(".mp4") || lower.ends_with(".mov")
        } else {
            lower.ends_with(".2kplaylist")
        }) {
            continue;
        }
        let key = text(f, "quickkey", 32)?;
        ensure!(
            (10..=32).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric()),
            "Invalid download identity"
        );
        let hash = text(f, "hash", 64)?;
        ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "Download checksum unavailable"
        );
        let bytes = text(f, "size", 20)?.parse::<u64>()?;
        ensure!(
            (1..=MAX_FILE).contains(&bytes),
            "Download exceeds the 2 GB library limit"
        );
        let url = text(&f["links"], "normal_download", 2048)?;
        let parsed = crate::curated_http::safe_url(url)?;
        let parts: Vec<_> = parsed.path().split('/').collect();
        ensure!(
            parsed.host_str() == Some("www.mediafire.com")
                && parts.len() >= 4
                && matches!(parts[1], "file" | "file_premium")
                && parts[2] == key
                && parsed.query().is_none(),
            "Download link does not match the listed file"
        );
        let supported = kind == "playlists" || lower.ends_with(".mp4");
        let mut name = filename.rsplit_once('.').unwrap().0.to_string();
        if kind == "playlists" {
            if let Some((prefix, suffix)) = name.rsplit_once('-') {
                if suffix.len() == 8 && suffix.bytes().all(|b| b.is_ascii_hexdigit()) {
                    name = prefix.to_string();
                }
            }
        }
        entries.push(Entry {id:format!("{kind}-{key}"),kind:kind.into(),name,filename:filename.into(),description:text(f,"description",4000)?.into(),bytes,sha256:hash.to_ascii_lowercase(),url:url.into(),revision:text(f,"revision",64)?.into(),supported,reason:if supported {""} else {"Needs an MP4 version. This game-format MOV cannot play in the startup video window."}.into(),artwork:String::new(),order:10000});
    }
    let more = text(content, "more_chunks", 3)?;
    ensure!(matches!(more, "yes" | "no"), "Invalid MediaFire pagination");
    Ok((entries, more == "yes"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Value {
        json!({"response":{"result":"Success","folder_content":{"folderkey":FOLDERS[0].1,"content_type":"files","chunk_number":"1","more_chunks":"no","files":[{"privacy":"public","password_protected":"no","filename":"NBA 2K14.mov","quickkey":"abcdefghijklmno","hash":"a".repeat(64),"size":"500","description":"Intro","revision":"4","links":{"normal_download":"https://www.mediafire.com/file_premium/abcdefghijklmno/intro.mov/file"}}]}}})
    }
    #[test]
    fn mov_is_visible_but_never_importable() {
        let (e, more) = page(&fixture(), FOLDERS[0].0, FOLDERS[0].1, 1).unwrap();
        assert!(!more);
        assert!(!e[0].supported);
        assert_eq!(e[0].name, "NBA 2K14");
    }
    #[test]
    fn rejects_wrong_folder_page_hash_size_link_and_path() {
        for (key, val) in [
            ("filename", "../intro.mp4"),
            ("hash", "abc"),
            ("size", "2147483649"),
        ] {
            let mut v = fixture();
            v["response"]["folder_content"]["files"][0][key] = json!(val);
            assert!(page(&v, FOLDERS[0].0, FOLDERS[0].1, 1).is_err());
        }
        assert!(page(&fixture(), FOLDERS[0].0, FOLDERS[0].1, 2).is_err());
        let mut v = fixture();
        v["response"]["folder_content"]["files"][0]["links"]["normal_download"] =
            json!("https://www.mediafire.com/file/foreignidentity/file");
        assert!(page(&v, FOLDERS[0].0, FOLDERS[0].1, 1).is_err());
    }
    #[test]
    fn mp4_and_private_filter() {
        let mut v = fixture();
        v["response"]["folder_content"]["files"][0]["filename"] = json!("Intro.mp4");
        assert!(page(&v, FOLDERS[0].0, FOLDERS[0].1, 1).unwrap().0[0].supported);
        v["response"]["folder_content"]["files"][0]["privacy"] = json!("private");
        assert!(
            page(&v, FOLDERS[0].0, FOLDERS[0].1, 1)
                .unwrap()
                .0
                .is_empty()
        );
    }
}
