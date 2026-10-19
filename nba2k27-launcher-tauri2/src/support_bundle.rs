//! Bounded, redacted support bundles for user-directed diagnostics.
//!
//! The bundle never includes user media, drafts, cache contents, dumps or
//! binaries. It copies only small text/JSON diagnostics from the portable Logs
//! tree and rewrites paths and credential-like fields before publication.

use anyhow::{Result, ensure};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const MAX_FILES: usize = 128;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 12;

fn user_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)([A-Z]:[\\/]+Users[\\/]+)[^\\/\r\n"']+"#).unwrap())
}

fn text_secret() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?im)(\b[\w-]*(?:token|nonce|secret|password|authorization|acknowledgement)[\w-]*["']?\s*[:=]\s*)[^\r\n]+"#).unwrap()
    })
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "nonce",
        "secret",
        "password",
        "authorization",
        "acknowledgement",
    ]
    .iter()
    .any(|part| key.contains(part))
}

fn redact_text(text: &str, roots: &[(&str, &str)]) -> String {
    let mut text = text.to_owned();
    for (source, replacement) in roots {
        if !source.is_empty() {
            for variant in [
                source.to_string(),
                source.replace('\\', "/"),
                source.replace('\\', "\\\\"),
            ] {
                let pattern = Regex::new(&format!("(?i){}", regex::escape(&variant))).unwrap();
                text = pattern
                    .replace_all(&text, regex::NoExpand(replacement))
                    .into_owned();
            }
        }
    }
    let text = user_path().replace_all(&text, "$1<redacted>");
    text_secret()
        .replace_all(&text, "$1<redacted>")
        .into_owned()
}

pub(crate) fn redact_value(value: &mut Value, roots: &[(&str, &str)]) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if sensitive_key(key) {
                    *value = Value::String("<redacted>".into());
                } else {
                    redact_value(value, roots);
                }
            }
        }
        Value::Array(rows) => rows.iter_mut().for_each(|value| redact_value(value, roots)),
        Value::String(text) => *text = redact_text(text, roots),
        _ => {}
    }
}

fn collect_logs(directory: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    if depth > MAX_DEPTH || out.len() >= MAX_FILES || !directory.try_exists()? {
        return Ok(());
    }
    let mut rows = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    rows.sort_by_key(|row| row.file_name());
    for row in rows {
        if out.len() >= MAX_FILES {
            break;
        }
        let path = row.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0 {
            continue;
        }
        if meta.is_dir() {
            if !row
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Support")
            {
                collect_logs(&path, out, depth + 1)?;
            }
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if meta.is_file()
            && meta.len() <= MAX_FILE_BYTES
            && matches!(extension.as_str(), "json" | "log" | "txt")
        {
            out.push(path);
        }
    }
    Ok(())
}

/// WPF audit D6: on Version 2 the legacy WPF proxy's September-only outfit
/// thread always logs this guard failure. It is not a V2 carry result.
pub(crate) fn legacy_outfit_guard_note(text: &str) -> Option<&'static str> {
    (text.contains("SCHEMA=blacktop-saved-outfits-v1")
        && text.contains("STATE=FAILED")
        && text.contains("REASON=Build or anti-cheat guard"))
    .then_some(
        "NOTE=Legacy WPF September-build outfit worker refused this game build (expected on Version 2). Not a Version 2 outfit carry result.
",
    )
}

fn read_bounded(path: &Path, remaining: u64) -> Result<Vec<u8>> {
    use std::os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
        },
    };
    let limit = MAX_FILE_BYTES.min(remaining);
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    let meta = file.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        meta.is_file()
            && meta.file_attributes() & 0x400 == 0
            && info.nNumberOfLinks == 1
            && meta.len() <= limit,
        "Diagnostic changed or exceeds limits"
    );
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Diagnostic changed or exceeds limits"
    );
    Ok(bytes)
}

pub(crate) fn create(root: &Path, settings: Value, version: &str) -> Result<Value> {
    let support = root.join("Logs/Support");
    fs::create_dir_all(&support)?;
    let id = format!(
        "{}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    );
    let destination = support.join(format!("2k-launcher-support-{id}.zip"));
    let mut settings = settings;
    let game = settings["gameRoot"].as_str().unwrap_or("").to_owned();
    let mods = settings["modsRoot"].as_str().unwrap_or("").to_owned();
    let portable = root.to_string_lossy().into_owned();
    let roots = [
        (portable.as_str(), "<portable-root>"),
        (game.as_str(), "<game-root>"),
        (mods.as_str(), "<mods-root>"),
    ];
    redact_value(&mut settings, &roots);

    let mut candidates = Vec::new();
    collect_logs(&root.join("Logs"), &mut candidates, 0)?;
    let mut total = 0u64;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    let result = (|| -> Result<(usize, u64)> {
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let summary = serde_json::to_vec_pretty(&json!({
            "schema":"nba2k27-launcher-support-v1",
            "version":version,
            "createdUtcSeconds":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            "settings":settings,
            "limits":{"files":MAX_FILES,"fileBytes":MAX_FILE_BYTES,"totalBytes":MAX_TOTAL_BYTES},
            "excluded":["Data","Library","Cache","media","dumps","binaries"]
        }))?;
        zip.start_file("summary.json", options)?;
        zip.write_all(&summary)?;
        let mut included = 0usize;
        for path in candidates {
            if total >= MAX_TOTAL_BYTES {
                break;
            }
            let bytes = match read_bounded(&path, MAX_TOTAL_BYTES - total) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                Err(_) => continue,
            };
            let text = match legacy_outfit_guard_note(&text) {
                Some(note) => note.to_string(),
                None => text,
            };
            let redacted = if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                match serde_json::from_str::<Value>(&text) {
                    Ok(mut value) => {
                        redact_value(&mut value, &roots);
                        serde_json::to_string_pretty(&value)?
                    }
                    Err(_) => redact_text(&text, &roots),
                }
            } else {
                redact_text(&text, &roots)
            };
            let next_total = total.saturating_add(redacted.len() as u64);
            if next_total > MAX_TOTAL_BYTES {
                break;
            }
            total = next_total;
            included += 1;
            let leaf = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("diagnostic.txt")
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .take(80)
                .collect::<String>();
            zip.start_file(format!("logs/{included:04}-{leaf}"), options)?;
            zip.write_all(redacted.as_bytes())?;
        }
        zip.finish()?.sync_all()?;
        Ok((included, total))
    })();
    match result {
        Ok((files, source_bytes)) => Ok(json!({
            "path":destination,
            "files":files,
            "sourceBytes":source_bytes,
            "redacted":true
        })),
        Err(error) => {
            let _ = fs::remove_file(&destination);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn fixture() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("launcher-support-{}", crate::backend::unique_id()));
        fs::create_dir_all(path.join("Logs/native-sessions/session-secret")).unwrap();
        path
    }

    #[test]
    fn bundle_redacts_paths_and_secrets_and_excludes_binary_or_user_data() {
        let root = fixture();
        fs::create_dir_all(root.join("Data")).unwrap();
        fs::write(
            root.join("Data/private.json"),
            br#"{"private":"do not include"}"#,
        )
        .unwrap();
        fs::write(root.join("Logs/game.dmp"), b"dump").unwrap();
        fs::write(
            root.join("Logs/native-sessions/session-secret/provider-state.json"),
            br#"{"token":"abc","path":"C:\\Users\\Alice\\Game","message":"C:\\Users\\Alice\\Game\\NBA2K27.exe"}"#,
        )
        .unwrap();
        let result = create(
            &root,
            json!({"gameRoot":"C:\\Users\\Alice\\Game","modsRoot":"C:\\Users\\Alice\\Mods","profile":"Default"}),
            "test",
        )
        .unwrap();
        let file = File::open(result["path"].as_str().unwrap()).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        assert_eq!(zip.len(), 2);
        let mut all = String::new();
        for index in 0..zip.len() {
            zip.by_index(index)
                .unwrap()
                .read_to_string(&mut all)
                .unwrap();
        }
        assert!(!all.contains("Alice"));
        assert!(!all.contains("abc"));
        assert!(!all.contains("do not include"));
        assert!(!all.contains("game.dmp"));
        assert!(all.contains("<redacted>"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn text_logs_redact_case_slashes_usernames_and_credential_lines() {
        let source = concat!(
            "path=d:/games/nba/mods/example.iff\n",
            "path=D:\\GAMES\\NBA\\Mods\\example.iff\n",
            "path=C:/Users/Alice Smith/Logs/file.log\n",
            "path=C:\\\\Users\\\\Alice Smith\\\\Logs\\\\file.log\n",
            "sessionToken=private-token\n",
            "Authorization: Bearer private-bearer\n",
            "{\"nonce\":\"private-nonce\",\"status\":\"pending\"}\n",
            "status=ready\n"
        );
        let redacted = redact_text(source, &[(r"D:\Games\NBA", "<game-root>")]);
        assert!(!redacted.contains("Alice"));
        assert!(!redacted.contains("private-"));
        assert!(!redacted.to_ascii_lowercase().contains("d:/games"));
        assert!(!redacted.to_ascii_lowercase().contains(r"d:\games"));
        assert!(redacted.contains("status=ready"));
        assert!(redacted.contains("<game-root>"));
    }

    #[test]
    fn legacy_wpf_outfit_guard_log_is_labelled_not_reported_as_v2_failure() {
        let legacy = "SCHEMA=blacktop-saved-outfits-v1
PID=1
STATE=FAILED
REASON=Build or anti-cheat guard
";
        assert!(legacy_outfit_guard_note(legacy).unwrap().contains("expected on Version 2"));
        assert!(legacy_outfit_guard_note(&legacy.replace("FAILED", "ARMED")).is_none());
        assert!(legacy_outfit_guard_note("STATE=FAILED
REASON=V2 worker").is_none());
    }

    #[test]
    fn bundle_skips_linked_and_oversize_files_and_prior_exports() {
        let root = fixture();
        fs::create_dir_all(root.join("Logs/Support")).unwrap();
        fs::write(root.join("private.txt"), b"private outside logs").unwrap();
        fs::hard_link(root.join("private.txt"), root.join("Logs/linked.txt")).unwrap();
        fs::write(root.join("Logs/Support/old.txt"), b"old export").unwrap();
        File::create(root.join("Logs/large.log"))
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        fs::write(root.join("Logs/ok.log"), b"status=ready").unwrap();
        let result = create(&root, json!({}), "test").unwrap();
        assert_eq!(result["files"], 1);
        let mut zip =
            zip::ZipArchive::new(File::open(result["path"].as_str().unwrap()).unwrap()).unwrap();
        assert_eq!(zip.len(), 2);
        let mut contents = String::new();
        zip.by_index(1)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "status=ready");
        drop(zip);
        fs::remove_dir_all(root).unwrap();
    }
}
