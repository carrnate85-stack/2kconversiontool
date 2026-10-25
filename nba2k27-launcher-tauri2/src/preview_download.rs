//! One optional, pinned preview pack. Never accepts a URL or digest from the UI.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub const URL: &str = "https://www.mediafire.com/file_premium/3qtncc4boudviij/preview-34e5ab928969668401b8cd186f5614a372cdfbadb898c686bb390677e5742821.zip/file";
pub const BYTES: u64 = 1_039_360_223;
pub const SHA256: &str = "34e5ab928969668401b8cd186f5614a372cdfbadb898c686bb390677e5742821";

pub fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Preview download cancelled"
    );
    Ok(())
}
fn safe_url(value: &str) -> Result<tauri::Url> {
    let url = tauri::Url::parse(value)?;
    let host = url.host_str().unwrap_or("");
    let download_host = host
        .strip_prefix("download")
        .and_then(|h| h.strip_suffix(".mediafire.com"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    ensure!(
        url.scheme() == "https"
            && (host == "www.mediafire.com" || download_host)
            && url.port_or_known_default() == Some(443)
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none(),
        "Preview host returned an unsupported download address"
    );
    Ok(url)
}
fn receive(
    input: &mut impl Read,
    output: &mut impl Write,
    expected: u64,
    digest: &str,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<()> {
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let started = Instant::now();
    let mut last = Instant::now();
    progress(0, expected);
    loop {
        check(cancel)?;
        ensure!(
            started.elapsed() < Duration::from_secs(7200),
            "Preview download timed out. Try again."
        );
        let n = input
            .read(&mut buffer)
            .context("Preview connection interrupted. Try again.")?;
        check(cancel)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(
            bytes <= expected,
            "Preview download exceeds its expected size"
        );
        output
            .write_all(&buffer[..n])
            .context("Could not save previews. Check available disk space.")?;
        hash.update(&buffer[..n]);
        if last.elapsed() >= Duration::from_millis(200) || bytes == expected {
            progress(bytes, expected);
            last = Instant::now();
        }
    }
    check(cancel)?;
    ensure!(
        bytes == expected,
        "Preview download was incomplete. Try again."
    );
    ensure!(
        format!("{:x}", hash.finalize()) == digest,
        "Preview checksum does not match. Existing previews were kept."
    );
    Ok(())
}

pub fn download(path: &Path, cancel: &AtomicBool, progress: impl Fn(u64, u64)) -> Result<()> {
    download_from(path, URL, BYTES, SHA256, cancel, progress)
}
pub fn download_from(
    path: &Path,
    source: &str,
    expected: u64,
    digest: &str,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<()> {
    ensure!(
        (1..=2 * 1024 * 1024 * 1024).contains(&expected)
            && digest.len() == 64
            && digest.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid preview download pin"
    );
    use std::{ffi::c_void, ptr};
    use windows::{Win32::Networking::WinHttp::*, core::PCWSTR};
    struct Handle(*mut c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
    fn opened(p: *mut c_void) -> Result<Handle> {
        ensure!(
            !p.is_null(),
            "Could not open preview connection: {}",
            std::io::Error::last_os_error()
        );
        Ok(Handle(p))
    }
    fn header(h: *mut c_void, key: u32) -> Result<String> {
        let mut buf = [0u16; 4096];
        let mut size = (buf.len() * 2) as u32;
        let mut index = 0;
        unsafe {
            WinHttpQueryHeaders(
                h,
                key,
                PCWSTR::null(),
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                &mut index,
            )?;
        }
        ensure!(
            size as usize <= buf.len() * 2 && size % 2 == 0,
            "Invalid preview HTTP header"
        );
        Ok(String::from_utf16(&buf[..size as usize / 2])?
            .trim_end_matches('\0')
            .into())
    }
    struct Body(*mut c_void);
    impl Read for Body {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let mut count = 0;
            unsafe {
                WinHttpReadData(
                    self.0,
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as u32,
                    &mut count,
                )
            }
            .map_err(std::io::Error::other)?;
            Ok(count as usize)
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    let mut url = safe_url(source)?;
    unsafe {
        let agent = wide("2KLauncher/preview-pack");
        let session = opened(WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 15000, 15000, 15000, 15000)?;
        for hop in 0..=5 {
            check(cancel)?;
            let host = wide(url.host_str().unwrap());
            let object = wide(&format!(
                "{}{}",
                url.path(),
                url.query().map(|q| format!("?{q}")).unwrap_or_default()
            ));
            let connection = opened(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0))?;
            let verb = wide("GET");
            let request = opened(WinHttpOpenRequest(
                connection.0,
                PCWSTR(verb.as_ptr()),
                PCWSTR(object.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                ptr::null(),
                WINHTTP_FLAG_SECURE | WINHTTP_FLAG_REFRESH,
            ))?;
            let disabled = WINHTTP_DISABLE_REDIRECTS
                | WINHTTP_DISABLE_COOKIES
                | WINHTTP_DISABLE_AUTHENTICATION;
            WinHttpSetOption(
                Some(request.0),
                WINHTTP_OPTION_DISABLE_FEATURE,
                Some(&disabled.to_ne_bytes()),
            )?;
            let headers =
                "Accept: application/zip,application/octet-stream\r\nAccept-Encoding: identity\r\n"
                    .encode_utf16()
                    .collect::<Vec<_>>();
            WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)?;
            WinHttpReceiveResponse(request.0, ptr::null_mut())?;
            let status: u32 = header(request.0, WINHTTP_QUERY_STATUS_CODE)?.parse()?;
            if [301, 302, 303, 307, 308].contains(&status) {
                ensure!(hop < 5, "Too many preview download redirects");
                url = safe_url(
                    url.join(&header(request.0, WINHTTP_QUERY_LOCATION)?)?
                        .as_str(),
                )?;
                continue;
            }
            ensure!(
                status == 200,
                "Preview host returned HTTP {status}. Try again later or import a ZIP."
            );
            let kind = header(request.0, WINHTTP_QUERY_CONTENT_TYPE)?.to_ascii_lowercase();
            ensure!(
                matches!(
                    kind.split(';').next().unwrap_or("").trim(),
                    "application/zip" | "application/octet-stream" | "application/x-zip-compressed"
                ),
                "MediaFire returned a webpage instead of the ZIP. Try again later or import a ZIP."
            );
            ensure!(
                header(request.0, WINHTTP_QUERY_CONTENT_LENGTH)?.parse::<u64>()? == expected,
                "Preview download size changed; existing previews were kept."
            );
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            receive(
                &mut Body(request.0),
                &mut output,
                expected,
                digest,
                cancel,
                progress,
            )?;
            output.sync_all()?;
            return Ok(());
        }
    }
    anyhow::bail!("Preview download did not complete")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redirect_hosts_are_https_mediafire_only() {
        assert!(safe_url(URL).is_ok());
        assert!(safe_url("https://download1327.mediafire.com/file.zip").is_ok());
        for u in [
            "http://download1.mediafire.com/a",
            "https://mediafire.com.evil.test/a",
            "https://download1.mediafire.com.evil.test/a",
            "https://www.mediafire.com:444/a",
            "https://user@www.mediafire.com/a",
            "https://127.0.0.1/a",
            "https://download.mediafire.com/a",
        ] {
            assert!(safe_url(u).is_err(), "{u}");
        }
    }
    #[test]
    fn interrupted_changed_and_oversized_payloads_are_rejected() {
        let bytes = b"verified fixture";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let cancel = AtomicBool::new(false);
        let mut saved = Vec::new();
        receive(
            &mut &bytes[..],
            &mut saved,
            bytes.len() as u64,
            &hash,
            &cancel,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(saved, bytes);
        for (body, len, digest) in [
            (&bytes[..3], bytes.len() as u64, hash.as_str()),
            (&bytes[..], 2, hash.as_str()),
            (&bytes[..], bytes.len() as u64, "wrong"),
        ] {
            assert!(
                receive(
                    &mut &body[..],
                    &mut Vec::new(),
                    len,
                    digest,
                    &cancel,
                    |_, _| {}
                )
                .is_err()
            );
        }
        cancel.store(true, Ordering::Release);
        let mut saved = Vec::new();
        assert!(
            receive(
                &mut &bytes[..],
                &mut saved,
                bytes.len() as u64,
                &hash,
                &cancel,
                |_, _| {}
            )
            .is_err()
        );
        assert!(saved.is_empty());
    }
}
