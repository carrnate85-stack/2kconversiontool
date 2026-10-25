use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const LIMIT: u64 = 512 * 1024 * 1024;
const MARKER: &str = ".cmt-return.json";

fn no_links(root: &Path, path: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    ensure!(
        path.canonicalize()?.starts_with(&root),
        "Recovery path escaped its owner"
    );
    let mut current = path;
    loop {
        ensure!(
            fs::symlink_metadata(current)?.file_attributes() & 0x400 == 0,
            "Recovery links are not supported"
        );
        if current.canonicalize()? == root {
            return Ok(());
        }
        current = current.parent().context("Recovery path has no owner")?;
    }
}

fn digest(file: &mut File, limit: u64) -> Result<(String, u64)> {
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= limit,
        "Invalid or oversized Blender recovery file"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count += read as u64;
        ensure!(count <= limit, "Blender recovery file exceeds budget");
        hash.update(&buffer[..read]);
    }
    ensure!(
        count == metadata.len(),
        "Blender recovery file changed during read"
    );
    Ok((format!("{:x}", hash.finalize()), count))
}

fn read_digest(path: &Path, limit: u64) -> Result<(String, u64)> {
    digest(
        &mut OpenOptions::new().read(true).share_mode(1).open(path)?,
        limit,
    )
}

fn read_json(path: &Path) -> Option<Value> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 || metadata.len() > 32768 {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_json(path: &Path, value: &Value, replace: bool) -> Result<()> {
    let temporary = path.with_file_name(format!(".recovery-{}.tmp", crate::runtime::stamp()));
    let bytes = serde_json::to_vec_pretty(value)?;
    let result: Result<()> = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        if replace {
            if path.exists() {
                ensure!(
                    fs::symlink_metadata(path)?.file_attributes() & 0x400 == 0,
                    "Recovery marker is a link"
                );
            }
            fs::rename(&temporary, path)?;
        } else {
            fs::hard_link(&temporary, path)?;
        }
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

pub struct Recovery {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
    pub warning: Option<String>,
}

fn preserve_with_limit(
    data: &Path,
    session: &Path,
    row: &Value,
    reason: &str,
    limit: u64,
) -> Result<Recovery> {
    let source = PathBuf::from(
        row["returned"]
            .as_str()
            .context("Missing Blender export path")?,
    );
    no_links(session, &source)?;
    ensure!(
        source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("iff")),
        "Only Blender IFF returns can be recovered"
    );
    let mut input = OpenOptions::new().read(true).share_mode(1).open(&source)?;
    let (sha256, bytes) = digest(&mut input, limit)?;
    let target = row["requestedTarget"]
        .as_str()
        .or_else(|| row["target"].as_str())
        .context("Missing Blender target")?;
    let identity = format!(
        "{:x}",
        Sha256::digest(
            format!("{}\0{sha256}", target.replace('\\', "/").to_lowercase()).as_bytes()
        )
    );
    let stem: String = Path::new(target)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("player")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .take(40)
        .collect();
    let stem = if stem.is_empty() { "player" } else { &stem };
    let folder = data.join("Blender Recovery");
    if !folder.exists() {
        fs::create_dir(&folder)?;
    }
    no_links(data, &folder)?;
    for index in 0..32 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index}")
        };
        let path = folder.join(format!("{stem}-blender-{}{suffix}.iff", &identity[..16]));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(1)
            .open(&path)
        {
            Ok(mut output) => {
                let copied = (|| -> Result<()> {
                    input.seek(SeekFrom::Start(0))?;
                    ensure!(
                        std::io::copy(&mut input, &mut output)? == bytes,
                        "Blender recovery copy was incomplete"
                    );
                    output.sync_all()?;
                    Ok(())
                })();
                drop(output);
                if let Err(error) = copied {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                if no_links(data, &path).is_err()
                    || read_digest(&path, limit).ok() != Some((sha256.clone(), bytes))
                {
                    continue;
                }
            }
            Err(error) => return Err(error.into()),
        }
        let metadata = json!({"version":1,"target":target,"expectedDigest":row["expectedDigest"],
            "sha256":sha256,"bytes":bytes,"reason":reason.chars().take(8000).collect::<String>(),
            "createdUnixSeconds":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()});
        let sidecar = path.with_extension("json");
        let mut warning = None;
        if !sidecar.exists() {
            if let Err(error) = write_json(&sidecar, &metadata, false) {
                warning = Some(format!("Recovery metadata was not saved: {error:#}"));
            }
        }
        let marker = source
            .parent()
            .context("Missing Blender export folder")?
            .join(MARKER);
        if let Err(error) = write_json(
            &marker,
            &json!({"recoveryDigest":sha256,"recoveryPath":path}),
            true,
        ) {
            warning = Some(format!(
                "Recovery copy is safe, but session cleanup tracking failed: {error:#}"
            ));
        }
        return Ok(Recovery {
            path,
            sha256,
            bytes,
            warning,
        });
    }
    anyhow::bail!("Recovery filenames are occupied; no existing work was overwritten")
}

pub fn preserve_failure(data: &Path, session: &Path, row: &mut Value) {
    let reason = row["message"]
        .as_str()
        .unwrap_or("Blender return was not applied")
        .to_owned();
    match preserve_with_limit(data, session, row, &reason, LIMIT) {
        Ok(recovery) => {
            row["message"] = json!(format!(
                "{reason}\nUnapplied Blender export preserved at {}",
                recovery.path.display()
            ));
            row["recoveryPath"] = json!(recovery.path);
            row["recoverySha256"] = json!(recovery.sha256);
            row["recoveryBytes"] = json!(recovery.bytes);
            if let Some(warning) = recovery.warning {
                row["recoveryWarning"] = json!(warning);
            }
        }
        Err(error) => {
            row["recoveryError"] = json!(format!("{error:#}"));
            row["message"] = json!(format!(
                "{reason}\nNo recovery copy was confirmed: {error:#}. Any remaining export will be kept in its Blender session."
            ));
        }
    }
}

pub fn finish_return(data: &Path, session: &Path, row: &mut Value, result: Result<Value>) {
    match result {
        Ok(result) => {
            row["state"] = json!("complete");
            row["output"] = result["output"].clone();
            row["message"] = json!("Staged player updated from Blender.");
        }
        Err(error) => {
            row["state"] = json!("error");
            row["message"] = json!(format!("{error:#}"));
            preserve_failure(data, session, row);
        }
    }
}

pub fn recover_session(data: &Path, session: &Path) -> Result<()> {
    if !session.exists() {
        return Ok(());
    }
    no_links(session, session)?;
    for entry in fs::read_dir(session)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("blender-") {
            continue;
        }
        let folder = entry.path();
        no_links(session, &folder)?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let info = read_json(&folder.join("handoff.json")).unwrap_or(Value::Null);
        let marker = read_json(&folder.join(MARKER)).unwrap_or(Value::Null);
        for file in fs::read_dir(&folder)? {
            let file = file?;
            let path = file.path();
            let name = file.file_name().to_string_lossy().to_lowercase();
            if !name.ends_with(".iff") || name == "backup.iff" || name == "static-jersey.iff" {
                continue;
            }
            no_links(session, &path)?;
            let (sha256, _) = read_digest(&path, LIMIT)?;
            if marker["appliedDigest"].as_str() == Some(sha256.as_str()) {
                continue;
            }
            let target = info["target"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| name.clone());
            preserve_with_limit(
                data,
                session,
                &json!({"returned":path,"target":target,"expectedDigest":info["expectedDigest"]}),
                "Unapplied export recovered while retiring a Blender session",
                LIMIT,
            )?;
        }
    }
    Ok(())
}

fn inspect_recovery(folder: &Path, limit: usize) -> Result<(usize, u64, bool)> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    for (index, entry) in fs::read_dir(folder)?.enumerate() {
        let entry = entry?;
        if index == limit {
            return Ok((files, bytes, true));
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_file()
            && metadata.file_attributes() & 0x400 == 0
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("iff"))
        {
            files += 1;
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    Ok((files, bytes, false))
}

pub fn health(data: &Path) -> Option<Value> {
    let folder = data.join("Blender Recovery");
    if !folder.exists() {
        return None;
    }
    let inspected = (|| {
        no_links(data, &folder)?;
        inspect_recovery(&folder, 10000)
    })();
    let check = match inspected {
        Ok((0, _, false)) => return None,
        Ok((files, bytes, truncated)) => {
            json!({"id":"blender-recovery","label":"Unapplied Blender exports","status":"info",
            "message":format!("{}{files} recovery IFF(s), {bytes} bytes{}. These are user work, not disposable cache.",
                if truncated { "At least " } else { "" }, if truncated { " in the inspected entries" } else { "" }),
            "path":folder,"repair":{"action":"open-path","label":"Open Folder","path":folder}})
        }
        Err(error) => {
            json!({"id":"blender-recovery","label":"Blender recovery storage","status":"warning","message":format!("Recovery storage could not be inspected: {error:#}"),"path":folder})
        }
    };
    Some(json!({"id":"blender-recovery","label":"Blender Recovery","checks":[check]}))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        session: PathBuf,
        folder: PathBuf,
        source: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(format!("cmt-blender-recovery-{}", crate::runtime::stamp()));
            let session = root.join("session");
            let folder = session.join("blender-test");
            fs::create_dir_all(&folder).unwrap();
            let source = folder.join("png1234.iff");
            fs::write(&source, b"returned player bytes").unwrap();
            Self {
                root,
                session,
                folder,
                source,
            }
        }
        fn row(&self) -> Value {
            json!({"returned":self.source,"target":"C:/players/png1234.iff","expectedDigest":"before"})
        }
        fn preserve(&self) -> Recovery {
            preserve_with_limit(&self.root, &self.session, &self.row(), "conflict", LIMIT).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn preserves_exact_bytes_and_deduplicates_without_modifying_source() {
        let f = Fixture::new();
        let original = fs::read(&f.source).unwrap();
        let first = f.preserve();
        assert_eq!(fs::read(&first.path).unwrap(), original);
        assert_eq!(fs::read(&f.source).unwrap(), original);
        assert_eq!(f.preserve().path, first.path);
        let marker = read_json(&f.folder.join(MARKER)).unwrap();
        assert_eq!(marker["recoveryDigest"], first.sha256);
        assert_eq!(marker["recoveryPath"], json!(first.path));
        let sidecar = read_json(&first.path.with_extension("json")).unwrap();
        assert_eq!(sidecar["expectedDigest"], "before");
        assert_eq!(sidecar["bytes"], original.len());
    }

    #[test]
    fn changed_recovery_is_not_overwritten() {
        let f = Fixture::new();
        let first = f.preserve();
        fs::write(&first.path, b"user changes to recovered work").unwrap();
        let second = f.preserve();
        assert_ne!(first.path, second.path);
        assert_eq!(
            fs::read(&first.path).unwrap(),
            b"user changes to recovered work"
        );
        assert_eq!(fs::read(second.path).unwrap(), fs::read(&f.source).unwrap());
    }

    #[test]
    fn rejects_foreign_and_oversized_sources_without_creating_recovery() {
        let f = Fixture::new();
        let outside = f.root.join("outside.iff");
        fs::write(&outside, b"outside").unwrap();
        let mut row = f.row();
        row["returned"] = json!(outside);
        assert!(preserve_with_limit(&f.root, &f.session, &row, "test", LIMIT).is_err());
        assert!(preserve_with_limit(&f.root, &f.session, &f.row(), "test", 2).is_err());
        assert!(!f.root.join("Blender Recovery").exists());
    }

    #[test]
    fn failed_return_survives_original_session_removal_and_health_finds_it() {
        let f = Fixture::new();
        let mut row = f.row();
        finish_return(
            &f.root,
            &f.session,
            &mut row,
            Err(anyhow::anyhow!("staged player changed")),
        );
        assert_eq!(row["state"], "error");
        let recovered = PathBuf::from(row["recoveryPath"].as_str().unwrap());
        fs::remove_dir_all(&f.session).unwrap();
        assert_eq!(fs::read(recovered).unwrap(), b"returned player bytes");
        let report = health(&f.root).unwrap();
        assert_eq!(report["checks"][0]["repair"]["action"], "open-path");
        assert!(
            report["checks"][0]["message"]
                .as_str()
                .unwrap()
                .contains("not disposable cache")
        );
    }

    #[test]
    fn successful_return_does_not_create_recovery() {
        let f = Fixture::new();
        let mut row = f.row();
        finish_return(
            &f.root,
            &f.session,
            &mut row,
            Ok(json!({"output":"player.iff"})),
        );
        assert_eq!(row["state"], "complete");
        assert_eq!(row["output"], "player.iff");
        assert!(!f.root.join("Blender Recovery").exists());
    }

    #[test]
    fn session_retirement_recovers_orphans_and_skips_applied_and_references() {
        let f = Fixture::new();
        fs::write(f.folder.join("backup.iff"), b"backup").unwrap();
        fs::write(f.folder.join("static-jersey.iff"), b"jersey").unwrap();
        write_json(
            &f.folder.join("handoff.json"),
            &json!({"target":"C:/players/png1234.iff"}),
            false,
        )
        .unwrap();
        recover_session(&f.root, &f.session).unwrap();
        assert_eq!(
            inspect_recovery(&f.root.join("Blender Recovery"), 100)
                .unwrap()
                .0,
            1
        );
        recover_session(&f.root, &f.session).unwrap();
        assert_eq!(
            inspect_recovery(&f.root.join("Blender Recovery"), 100)
                .unwrap()
                .0,
            1
        );
        fs::write(&f.source, b"applied updated player").unwrap();
        let (sha, _) = read_digest(&f.source, LIMIT).unwrap();
        write_json(&f.folder.join(MARKER), &json!({"appliedDigest":sha}), true).unwrap();
        recover_session(&f.root, &f.session).unwrap();
        assert_eq!(
            inspect_recovery(&f.root.join("Blender Recovery"), 100)
                .unwrap()
                .0,
            1
        );
    }

    #[test]
    fn unavailable_recovery_storage_prevents_successful_retirement() {
        let f = Fixture::new();
        fs::write(f.root.join("Blender Recovery"), b"occupied").unwrap();
        assert!(recover_session(&f.root, &f.session).is_err());
        assert!(f.source.is_file());
        let mut row = f.row();
        preserve_failure(&f.root, &f.session, &mut row);
        assert!(row["recoveryPath"].is_null());
        assert!(
            row["message"]
                .as_str()
                .unwrap()
                .contains("No recovery copy was confirmed")
        );
    }

    #[test]
    fn health_scan_reports_truncation_instead_of_exact_totals() {
        let f = Fixture::new();
        let folder = f.root.join("Blender Recovery");
        fs::create_dir(&folder).unwrap();
        for index in 0..3 {
            fs::write(folder.join(format!("{index}.iff")), b"abc").unwrap();
        }
        assert_eq!(inspect_recovery(&folder, 2).unwrap(), (2, 6, true));
        assert_eq!(inspect_recovery(&folder, 3).unwrap(), (3, 9, false));
    }

    #[test]
    fn junctions_cannot_redirect_recovery_inputs_or_storage() {
        let f = Fixture::new();
        let outside = f.root.join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("player.iff"), b"outside player").unwrap();
        let link = f.folder.join("redirect");
        let junction = |path: &Path| {
            let output = std::process::Command::new("cmd.exe")
                .args(["/C", "mklink", "/J"])
                .arg(path)
                .arg(&outside)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        junction(&link);
        let mut row = f.row();
        row["returned"] = json!(link.join("player.iff"));
        assert!(preserve_with_limit(&f.root, &f.session, &row, "test", LIMIT).is_err());
        fs::remove_dir(&link).unwrap();
        let storage = f.root.join("Blender Recovery");
        junction(&storage);
        assert!(preserve_with_limit(&f.root, &f.session, &f.row(), "test", LIMIT).is_err());
        fs::remove_dir(storage).unwrap();
        assert_eq!(
            fs::read(outside.join("player.iff")).unwrap(),
            b"outside player"
        );
    }
}
