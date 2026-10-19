//! Owned loopback audio endpoint: opaque preview tokens and immutable range
//! bytes only. No paths, arbitrary URL fetches, directory listing or mutations.
use crate::jukebox_api::Api;
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) struct Server {
    pub port: u16,
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Server {
    pub fn start(api: Arc<Api>) -> Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let active = Arc::new(AtomicUsize::new(0));
        let thread = thread::Builder::new()
            .name("jukebox-preview-http".into())
            .spawn(move || {
                while !flag.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                            if let Some(permit) = Connection::claim(&active) {
                                let api = api.clone();
                                let flag = flag.clone();
                                let _ = thread::Builder::new()
                                    .name("jukebox-preview-connection".into())
                                    .spawn(move || {
                                        let _permit = permit;
                                        let _ = serve(&mut stream, &api, port, &flag);
                                    });
                            }
                        }
                        Ok(_) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(20))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            port,
            stop,
            thread: Mutex::new(Some(thread)),
        })
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub fn shutdown(&self) {
        self.stop();
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(500) {
            let Ok(mut t) = self.thread.lock() else {
                return;
            };
            if t.as_ref().is_none_or(JoinHandle::is_finished) {
                if let Some(t) = t.take() {
                    let _ = t.join();
                }
                return;
            }
            drop(t);
            thread::sleep(Duration::from_millis(5));
        }
    }
}
struct Connection(Arc<AtomicUsize>);
impl Connection {
    fn claim(active: &Arc<AtomicUsize>) -> Option<Self> {
        active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 8).then_some(n + 1)
            })
            .ok()
            .map(|_| Self(active.clone()))
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}
fn response(stream: &mut TcpStream, code: &str, extra: &str) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code}\r\nConnection: close\r\nContent-Length: 0\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n{extra}\r\n"
    )?;
    Ok(())
}
fn range(value: Option<&str>, size: usize) -> Result<(usize, usize, bool)> {
    ensure!(size > 0, "Empty preview");
    let Some(value) = value else {
        return Ok((0, size - 1, false));
    };
    let range = value
        .strip_prefix("bytes=")
        .context("Unsupported media range")?;
    ensure!(
        !range.contains(','),
        "Multiple media ranges are unsupported"
    );
    let (left, right) = range.split_once('-').context("Invalid media range")?;
    ensure!(
        left.bytes().all(|b| b.is_ascii_digit()) && right.bytes().all(|b| b.is_ascii_digit()),
        "Invalid media range digits"
    );
    if left.is_empty() {
        let suffix: usize = right.parse()?;
        ensure!(suffix > 0, "Empty suffix range");
        return Ok((size.saturating_sub(suffix), size - 1, true));
    }
    let start: usize = left.parse()?;
    let end = if right.is_empty() {
        size - 1
    } else {
        right.parse::<usize>()?.min(size - 1)
    };
    ensure!(
        start < size && end >= start,
        "Media range is outside the preview"
    );
    Ok((start, end, true))
}
fn serve(stream: &mut TcpStream, api: &Api, port: u16, stop: &AtomicBool) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(100)))?;
    stream.set_write_timeout(Some(Duration::from_millis(100)))?;
    let mut data = Vec::new();
    let start = Instant::now();
    loop {
        ensure!(
            !stop.load(Ordering::Acquire) && start.elapsed() < Duration::from_millis(200),
            "Preview connection stopped"
        );
        let mut b = [0; 1024];
        match stream.read(&mut b) {
            Ok(0) => return Ok(()),
            Ok(n) => data.extend_from_slice(&b[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        if data.len() > 8192 {
            return response(stream, "431 Request Header Fields Too Large", "");
        }
        if data.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    if !matches!(request.parse(&data), Ok(httparse::Status::Complete(_))) {
        return response(stream, "400 Bad Request", "");
    }
    if !matches!(request.method, Some("GET" | "HEAD")) {
        return response(stream, "405 Method Not Allowed", "Allow: GET, HEAD\r\n");
    }
    let mut host = None;
    let mut requested_range = None;
    for h in request.headers.iter() {
        let text = std::str::from_utf8(h.value)?;
        if h.name.eq_ignore_ascii_case("host") {
            if host.replace(text).is_some() {
                return response(stream, "400 Bad Request", "");
            }
        }
        if h.name.eq_ignore_ascii_case("range") {
            if requested_range.replace(text).is_some() {
                return response(stream, "400 Bad Request", "");
            }
        }
        if h.name.eq_ignore_ascii_case("origin") && text != "https://tauri.localhost" {
            return response(stream, "403 Forbidden", "");
        }
        if h.name.eq_ignore_ascii_case("transfer-encoding")
            || h.name.eq_ignore_ascii_case("content-length") && text != "0"
        {
            return response(stream, "400 Bad Request", "");
        }
    }
    if host != Some(format!("127.0.0.1:{port}").as_str()) {
        return response(stream, "403 Forbidden", "");
    }
    let Some(token) = request
        .path
        .and_then(|p| p.strip_prefix("/api/jukebox/preview/"))
        .filter(|t| t.len() == 48 && t.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return response(stream, "404 Not Found", "");
    };
    let meta = match api.read_preview(token, 0, 0) {
        Ok(m) => m,
        Err(_) => return response(stream, "404 Not Found", ""),
    };
    let (first, last, partial) = match range(requested_range, meta.total) {
        Ok(r) => r,
        Err(_) => {
            return response(
                stream,
                "416 Range Not Satisfiable",
                &format!("Content-Range: bytes */{}\r\n", meta.total),
            );
        }
    };
    let code = if partial {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    write!(
        stream,
        "HTTP/1.1 {code}\r\nConnection: close\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n",
        meta.mime_type,
        last - first + 1
    )?;
    if partial {
        write!(
            stream,
            "Content-Range: bytes {first}-{last}/{}\r\n",
            meta.total
        )?;
    }
    stream.write_all(b"\r\n")?;
    if request.method == Some("HEAD") {
        return Ok(());
    }
    let mut at = first;
    while at <= last {
        ensure!(!stop.load(Ordering::Acquire), "Preview server stopping");
        let n = (last - at + 1).min(64 * 1024);
        let chunk = api.read_preview(token, at, n)?;
        ensure!(chunk.bytes.len() == n, "Preview data ended early");
        stream.write_all(&chunk.bytes)?;
        at += n;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    include!("jukebox_test_paths.rs");
    use super::*;
    #[test]
    fn connection_budget_is_bounded_and_released_on_drop() {
        let active = Arc::new(AtomicUsize::new(0));
        let mut held = Vec::new();
        for _ in 0..8 {
            held.push(Connection::claim(&active).unwrap());
        }
        assert!(Connection::claim(&active).is_none());
        held.pop();
        assert!(Connection::claim(&active).is_some());
        drop(held);
        assert_eq!(active.load(Ordering::Acquire), 0);
    }
    #[test]
    fn ranges_match_browser_audio_requests_and_reject_ambiguity() {
        assert_eq!(range(None, 100).unwrap(), (0, 99, false));
        assert_eq!(range(Some("bytes=0-"), 100).unwrap(), (0, 99, true));
        assert_eq!(range(Some("bytes=50-500"), 100).unwrap(), (50, 99, true));
        assert_eq!(range(Some("bytes=-10"), 100).unwrap(), (90, 99, true));
        for v in [
            "bytes=100-",
            "bytes=20-10",
            "bytes=0-1,4-5",
            "items=0-1",
            "bytes=-0",
            "bytes=+1-2",
        ] {
            assert!(range(Some(v), 100).is_err());
        }
    }
    #[test]
    fn real_loopback_endpoint_serves_immutable_ranges_and_rejects_foreign_authority() {
        use crate::{
            jukebox_api::Host,
            jukebox_audio,
            jukebox_catalog::{Music, Track},
            jukebox_service::Service,
        };
        use std::{fs, path::PathBuf};
        struct NoPick;
        impl Host for NoPick {
            fn choose_audio(&self, _: &AtomicBool) -> Result<Option<Vec<PathBuf>>> {
                Ok(None)
            }
            fn choose_playlist(&self, _: &AtomicBool) -> Result<Option<PathBuf>> {
                Ok(None)
            }
            fn choose_export(&self, _: &str, _: &AtomicBool) -> Result<Option<PathBuf>> {
                Ok(None)
            }
        }
        let root = fixture_artifacts().join(format!("2k-jukebox-http-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let stock = Music {
            tracks: vec![Track {
                name: "Stock".into(),
                artist: "Artist".into(),
                cover: "001_helicopter".into(),
                collection: 1,
                bank: 1,
                unique: 1,
                extra: serde_json::Map::new(),
            }],
            extra: serde_json::Map::new(),
        };
        let mut service = Service::open(&root, stock.clone(), &"a".repeat(48)).unwrap();
        let id = "b".repeat(48);
        service.create_playlist("Preview", &id).unwrap();
        let key = service
            .import_audio(
                &id,
                "Preview",
                "Artist",
                &jukebox_audio::pcm_wave(&[0; 1920]).unwrap(),
                &"c".repeat(48),
            )
            .unwrap();
        drop(service);
        let api = Arc::new(Api::open(&root, stock, None, Arc::new(NoPick)).unwrap());
        let p = api
            .prepare_preview(&key, &api.view().unwrap().revision)
            .unwrap();
        let server = Server::start(api.clone()).unwrap();
        // An idle browser connection must not monopolize the accept loop.
        let idle = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, server.port)).unwrap();
        let port = server.port;
        let request = |method: &str, path: &str, host: &str, extra: &str| {
            let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: {host}\r\n{extra}\r\n"
            )
            .unwrap();
            let mut out = vec![];
            stream.read_to_end(&mut out).unwrap();
            out
        };
        let path = format!("/api/jukebox/preview/{}", p.token);
        let host = format!("127.0.0.1:{port}");
        let full = request("GET", &path, &host, "");
        assert!(full.starts_with(b"HTTP/1.1 200 OK"));
        assert!(full.windows(4).any(|w| w == b"RIFF"));
        let partial = request("GET", &path, &host, "Range: bytes=8-15\r\n");
        let boundary = partial.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&partial[boundary..], b"WAVEfmt ");
        assert!(
            String::from_utf8_lossy(&partial[..boundary]).contains("Content-Range: bytes 8-15/")
        );
        let head = request("HEAD", &path, &host, "");
        assert!(head.ends_with(b"\r\n\r\n"));
        assert!(!head.windows(4).any(|w| w == b"RIFF"));
        for (method, path, authority, extra, code) in [
            ("GET", path.as_str(), "attacker.invalid", "", 403),
            (
                "GET",
                path.as_str(),
                host.as_str(),
                "Origin: https://attacker.invalid\r\n",
                403,
            ),
            (
                "GET",
                "/api/jukebox/preview/../../secret",
                host.as_str(),
                "",
                404,
            ),
            (
                "GET",
                path.as_str(),
                host.as_str(),
                "Range: bytes=0-1,3-4\r\n",
                416,
            ),
            (
                "GET",
                path.as_str(),
                host.as_str(),
                "Range: bytes=0-1\r\nRange: bytes=4-5\r\n",
                400,
            ),
            ("POST", path.as_str(), host.as_str(), "", 405),
        ] {
            let response = request(method, path, authority, extra);
            assert!(response.starts_with(format!("HTTP/1.1 {code}").as_bytes()));
        }
        api.release_preview(&p.token).unwrap();
        assert!(request("GET", &path, &host, "").starts_with(b"HTTP/1.1 404"));
        drop(idle);
        server.shutdown();
        drop(server);
        api.shutdown(Duration::from_secs(1)).unwrap();
        drop(api);
        fs::remove_dir_all(root).unwrap();
    }
}
