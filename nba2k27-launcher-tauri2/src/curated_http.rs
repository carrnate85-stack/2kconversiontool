//! Bounded MediaFire transport. Only the native curated folder service supplies URLs.
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Library download cancelled"
    );
    Ok(())
}
pub(crate) fn safe_url(value: &str) -> Result<tauri::Url> {
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
        "Library host returned an unsupported download address"
    );
    Ok(url)
}
pub(crate) fn catalog_url(value: &str) -> Result<tauri::Url> {
    let u = tauri::Url::parse(value)?;
    ensure!(
        u.scheme() == "https"
            && u.host_str() == Some("raw.githubusercontent.com")
            && u.port_or_known_default() == Some(443)
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.path().split('/').count() >= 5,
        "Use a public raw.githubusercontent.com catalog or artwork URL"
    );
    Ok(u)
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
            "Library download timed out. Try again."
        );
        let n = input
            .read(&mut buffer)
            .context("Library connection interrupted. Try again.")?;
        check(cancel)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(
            bytes <= expected,
            "Library download exceeds its expected size"
        );
        output
            .write_all(&buffer[..n])
            .context("Could not save librarys. Check available disk space.")?;
        hash.update(&buffer[..n]);
        if last.elapsed() >= Duration::from_millis(200) || bytes == expected {
            progress(bytes, expected);
            last = Instant::now();
        }
    }
    check(cancel)?;
    ensure!(
        (digest.is_empty() || bytes == expected),
        "Library download was incomplete. Try again."
    );
    ensure!(
        (digest.is_empty() || format!("{:x}", hash.finalize()) == digest),
        "Library checksum does not match. Existing librarys were kept."
    );
    Ok(())
}

pub(crate) fn fetch(
    url: &str,
    output: &mut impl Write,
    expected: u64,
    digest: &str,
    json: bool,
    cancel: &AtomicBool,
    progress: impl Fn(u64, u64),
) -> Result<()> {
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
            "Could not open library connection: {}",
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
            "Invalid library HTTP header"
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
    let github = json && tauri::Url::parse(url)?.host_str() == Some("raw.githubusercontent.com");
    let validate = |u: &str| if github { catalog_url(u) } else { safe_url(u) };
    let mut url = validate(url)?;
    unsafe {
        let agent = wide("2KLauncher/curated-library");
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
                "Accept: application/json,application/zip,application/octet-stream,video/mp4\r\nAccept-Encoding: identity\r\n"
                    .encode_utf16()
                    .collect::<Vec<_>>();
            WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)?;
            WinHttpReceiveResponse(request.0, ptr::null_mut())?;
            let status: u32 = header(request.0, WINHTTP_QUERY_STATUS_CODE)?.parse()?;
            if [301, 302, 303, 307, 308].contains(&status) {
                ensure!(hop < 5, "Too many library download redirects");
                url = validate(
                    url.join(&header(request.0, WINHTTP_QUERY_LOCATION)?)?
                        .as_str(),
                )?;
                continue;
            }
            ensure!(
                status == 200,
                "Library host returned HTTP {status}. Try again later or import a ZIP."
            );
            let kind = header(request.0, WINHTTP_QUERY_CONTENT_TYPE)?.to_ascii_lowercase();
            let mime = kind.split(';').next().unwrap_or("").trim();
            ensure!(
                if json {
                    matches!(mime, "application/json" | "text/plain" | "text/javascript")
                } else {
                    matches!(
                        mime,
                        "application/zip"
                            | "application/octet-stream"
                            | "application/x-zip-compressed"
                            | "video/mp4"
                    )
                },
                "MediaFire returned a webpage or unsupported response. Try again later."
            );
            if let Ok(length) = header(request.0, WINHTTP_QUERY_CONTENT_LENGTH) {
                let length: u64 = length.parse()?;
                ensure!(
                    if json {
                        length <= expected
                    } else {
                        length == expected
                    },
                    "Download size changed; refresh the library and retry."
                );
            }
            receive(
                &mut Body(request.0),
                output,
                expected,
                digest,
                cancel,
                progress,
            )?;
            return Ok(());
        }
    }
    anyhow::bail!("Library download did not complete")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hosts_are_bounded() {
        for u in [
            "https://evil.test/a",
            "http://www.mediafire.com/a",
            "https://www.mediafire.com:444/a",
            "https://user@www.mediafire.com/a",
            "https://download.mediafire.com/a",
            "https://download1.mediafire.com.evil/a",
        ] {
            assert!(safe_url(u).is_err(), "{u}");
        }
        assert!(safe_url("https://download123.mediafire.com/a").is_ok());
    }
    #[test]
    fn size_hash_and_cancel() {
        let c = AtomicBool::new(false);
        let hash = format!("{:x}", Sha256::digest(b"abc"));
        assert!(receive(&mut &b"abc"[..], &mut vec![], 3, &hash, &c, |_, _| {}).is_ok());
        assert!(receive(&mut &b"ab"[..], &mut vec![], 3, &hash, &c, |_, _| {}).is_err());
        assert!(receive(&mut &b"abcd"[..], &mut vec![], 3, &hash, &c, |_, _| {}).is_err());
        assert!(receive(&mut &b"abd"[..], &mut vec![], 3, &hash, &c, |_, _| {}).is_err());
        c.store(true, Ordering::Release);
        assert!(receive(&mut &b"abc"[..], &mut vec![], 3, &hash, &c, |_, _| {}).is_err());
    }
}
