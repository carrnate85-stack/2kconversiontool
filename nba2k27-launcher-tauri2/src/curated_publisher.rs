//! Compiled only into the curator dev build. Uses the local GitHub CLI sign-in;
//! no credential or arbitrary shell command is accepted from the renderer.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const REPO: &str = "repos/carrnate85-stack/2kconversiontool/contents/launcher-downloads";
const GH: &str = "C:\\Program Files\\GitHub CLI\\gh.exe";
struct Job(windows::Win32::Foundation::HANDLE);
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
fn own(child: &std::process::Child) -> Result<Job> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{Foundation::HANDLE, System::JobObjects::*};
    let job = Job(unsafe { CreateJobObjectW(None, None)? });
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            std::mem::size_of_val(&limits) as u32,
        )?;
        AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))?;
    }
    Ok(job)
}
static CLOSING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn shutdown() {
    CLOSING.store(true, std::sync::atomic::Ordering::Release);
}
fn b64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
            | c.get(2).copied().unwrap_or(0) as u32;
        out.push(A[(n >> 18) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if c.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
fn un64(s: &str) -> Result<Vec<u8>> {
    let mut out = vec![];
    let mut n = 0u32;
    let mut bits = 0;
    for b in s.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if b == b'=' {
            break;
        }
        let x = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => anyhow::bail!("Invalid GitHub content encoding"),
        };
        n = (n << 6) | x as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((n >> bits) as u8);
        }
    }
    Ok(out)
}
fn api(path: &str, body: Option<Value>) -> Result<Option<Value>> {
    ensure!(
        !CLOSING.load(std::sync::atomic::Ordering::Acquire),
        "Launcher is closing"
    );
    ensure!(
        Path::new(GH).is_file(),
        "Install GitHub CLI and sign in with gh auth login before publishing"
    );
    let mut cmd = Command::new(GH);
    cmd.args(["api", "--hostname", "github.com", path])
        .env("GH_PROMPT_DISABLED", "1")
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if body.is_some() {
        cmd.args(["--method", "PUT", "--input", "-"]);
    }
    let mut child = cmd.spawn().context("Could not start GitHub CLI")?;
    let _owner = match own(&child) {
        Ok(owner) => owner,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.context("Could not retain the GitHub process; nothing was sent"));
        }
    };
    let mut stdin = child.stdin.take().unwrap();
    let data = body
        .map(|v| serde_json::to_vec(&v))
        .transpose()?
        .unwrap_or_default();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let output = std::thread::spawn(move || {
        let mut b = vec![];
        stdout
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut b)
            .map(|_| b)
    });
    let error = std::thread::spawn(move || {
        let mut b = vec![];
        stderr.take(16385).read_to_end(&mut b).map(|_| b)
    });
    let start = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None)
                if start.elapsed() < Duration::from_secs(30)
                    && !CLOSING.load(std::sync::atomic::Ordering::Acquire) =>
            {
                std::thread::sleep(Duration::from_millis(50))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(anyhow::anyhow!(
                    "GitHub request stopped or timed out. Reload the curator before retrying; publication may have completed."
                ));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.into());
            }
        }
    };
    let _ = writer.join();
    let bytes = output
        .join()
        .map_err(|_| anyhow::anyhow!("GitHub output failed"))??;
    let err = error
        .join()
        .map_err(|_| anyhow::anyhow!("GitHub output failed"))??;
    let status = result?;
    ensure!(bytes.len() <= 8 * 1024 * 1024, "GitHub response too large");
    if !status.success() {
        let text = String::from_utf8_lossy(&err);
        if text.contains("HTTP 404") {
            return Ok(None);
        }
        anyhow::bail!(
            "GitHub request failed. Check GitHub CLI sign-in and repository write access. If someone edited the catalog, reload before publishing."
        );
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}
pub fn check_link(id: &str, url: &str) -> Result<crate::curated_catalog::Download> {
    ensure!(
        id == crate::curated_catalog::PREVIEW_ID
            || id.starts_with("videos-")
            || id.starts_with("playlists-"),
        "Unknown download kind"
    );
    crate::curated_catalog::resolve_download(id, url, &CLOSING)
}
pub fn editor() -> Result<Value> {
    if let Some(v) = api(&format!("{REPO}/catalog.json?ref=main"), None)? {
        let c: crate::curated_metadata::Catalog = serde_json::from_slice(&un64(
            v["content"]
                .as_str()
                .context("GitHub catalog content missing")?,
        )?)?;
        crate::curated_metadata::validate(&c)?;
        Ok(json!({"catalog":c,"sha":v["sha"],"url":crate::curated_metadata::URL}))
    } else {
        Ok(
            json!({"catalog":crate::curated_metadata::bundled()?,"sha":null,"url":crate::curated_metadata::URL}),
        )
    }
}
fn put(path: &str, bytes: &[u8], sha: Option<&str>, message: &str) -> Result<Value> {
    let mut body = json!({"message":message,"content":b64(bytes),"branch":"main"});
    if let Some(sha) = sha {
        ensure!(
            sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid GitHub revision"
        );
        body["sha"] = json!(sha);
    }
    api(&format!("{REPO}/{path}"), Some(body))?.context("GitHub did not confirm publication")
}
pub fn publish(c: &crate::curated_metadata::Catalog, sha: Option<&str>) -> Result<Value> {
    crate::curated_metadata::validate(c)?;
    // Resolve again before any GitHub writes: renderer cannot forge a pin.
    for i in &c.items {
        if let Some(d) = &i.download {
            let current = crate::curated_catalog::resolve_download(&i.id, &d.url, &CLOSING)?;
            ensure!(
                &current == d,
                "Download changed. Check its MediaFire link again before publishing."
            );
        }
    }
    // Publish our included cover images before the catalog that references them.
    for (name, bytes) in ART {
        if c.items.iter().any(|i| i.artwork == *name)
            && api(&format!("{REPO}/{name}?ref=main"), None)?.is_none()
        {
            put(name, bytes, None, "Add launcher library cover artwork")?;
        }
    }
    let result = put(
        "catalog.json",
        &serde_json::to_vec_pretty(c)?,
        sha,
        "Update curated launcher download catalog",
    )?;
    Ok(json!({"sha":result["content"]["sha"],"url":crate::curated_metadata::URL}))
}
pub fn artwork(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ensure!(
        matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp"),
        "Choose PNG, JPG, or WebP cover art"
    );
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= 4 * 1024 * 1024,
        "Cover art must be an ordinary image under 4 MB"
    );
    let mut bytes = vec![];
    f.read_to_end(&mut bytes)?;
    ensure!(
        match ext.as_str() {
            "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            _ => bytes.starts_with(&[255, 216, 255]),
        },
        "File does not contain the selected image format"
    );
    let name = format!("art/{}.{ext}", format!("{:x}", Sha256::digest(&bytes)));
    if api(&format!("{REPO}/{name}?ref=main"), None)?.is_none() {
        put(&name, &bytes, None, "Add curated launcher cover artwork")?;
    }
    Ok(
        crate::curated_http::catalog_url(crate::curated_metadata::URL)?
            .join(&name)?
            .to_string(),
    )
}
include!("curated_art.rs");
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoding_roundtrip() {
        for s in ["", "a", "ab", "abc", "artwork ✓"] {
            assert_eq!(un64(&b64(s.as_bytes())).unwrap(), s.as_bytes());
        }
    }
}
