//! User-initiated, bounded native cover preparation and MusicBrainz/CAA lookup.
//! Only artist/title enter lookup; remote paths never leave this module. WIC
//! decodes memory, pure Rust encodes the game's exact 256x256 BC1 TLD format.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
const SIDE: usize = 256;
pub(crate) const MAX_INPUT: usize = 4 * 1024 * 1024;
const MAX_JSON: usize = 2 * 1024 * 1024;
const MAX_CANDIDATES: usize = 8;
const USER_AGENT: &str = "2k-Launcher-Rust/0.37 (desktop; user-initiated artwork lookup)";
#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    pub native: Vec<u8>,
    pub preview: Vec<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub label: String,
    pub artwork: Prepared,
    pub matched_title: String,
    pub matched_artist: String,
    pub source: String,
}
fn check(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Acquire),
        "Artwork operation cancelled"
    );
    Ok(())
}
fn image_kind(input: &[u8]) -> Result<&'static str> {
    ensure!(
        (12..=MAX_INPUT).contains(&input.len()),
        "Artwork must be a PNG, JPEG, BMP or WebP image of at most 4 MiB"
    );
    if input.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok("PNG")
    } else if input.starts_with(b"\xff\xd8\xff") {
        Ok("JPEG")
    } else if input.starts_with(b"BM") {
        Ok("BMP")
    } else if &input[..4] == b"RIFF" && &input[8..12] == b"WEBP" {
        Ok("WebP")
    } else {
        anyhow::bail!("Artwork must be PNG, JPEG, BMP or WebP; other formats are not decoded")
    }
}
fn pixels(input: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    use windows::{
        Win32::{Graphics::Imaging::*, System::Com::*},
        core::HRESULT,
    };
    check(cancel)?;
    let kind = image_kind(input)?;
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    ensure!(
        status.is_ok() || status == HRESULT(0x80010106u32 as i32),
        "Artwork COM apartment initialization failed: {status:?}"
    );
    let _apartment = Apartment(status.is_ok());
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(input)?;
        let decoder=factory.CreateDecoderFromStream(&stream,std::ptr::null(),WICDecodeMetadataCacheOnDemand)
            .with_context(|| if kind=="WebP" { "WebP decoder unavailable: this Windows installation may need its WebP Image Extension; choose PNG or JPEG" } else { "The artwork image is invalid or its Windows decoder rejected it" })?;
        ensure!(
            decoder.GetFrameCount()? == 1,
            "Choose a single-frame artwork image"
        );
        let frame = decoder.GetFrame(0)?;
        let (mut width, mut height) = (0, 0);
        frame.GetSize(&mut width, &mut height)?;
        ensure!(
            (1..=8192).contains(&width)
                && (1..=8192).contains(&height)
                && u64::from(width) * u64::from(height) <= 16_777_216,
            "Artwork dimensions exceed the safe 16-megapixel limit"
        );
        check(cancel)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.,
            WICBitmapPaletteTypeCustom,
        )?;
        // WPF's cover scaling is exactly a centered square crop then resize.
        let side = width.min(height);
        let rect = WICRect {
            X: ((width - side) / 2) as i32,
            Y: ((height - side) / 2) as i32,
            Width: side as i32,
            Height: side as i32,
        };
        let clip = factory.CreateBitmapClipper()?;
        clip.Initialize(&converter, &rect)?;
        let scaler = factory.CreateBitmapScaler()?;
        scaler.Initialize(
            &clip,
            SIDE as u32,
            SIDE as u32,
            WICBitmapInterpolationModeFant,
        )?;
        let mut rgba = vec![0; SIDE * SIDE * 4];
        scaler.CopyPixels(std::ptr::null(), (SIDE * 4) as u32, &mut rgba)?;
        check(cancel)?;
        // Canvas {alpha:false} composites transparency onto black, not white.
        for p in rgba.chunks_exact_mut(4) {
            for c in 0..3 {
                p[c] = ((u32::from(p[c]) * u32::from(p[3]) + 127) / 255) as u8;
            }
            p[3] = 255;
        }
        Ok(rgba)
    }
}
fn rgb565(p: [u8; 3]) -> u16 {
    ((u16::from(p[0]) * 31 + 127) / 255) << 11
        | ((u16::from(p[1]) * 63 + 127) / 255) << 5
        | ((u16::from(p[2]) * 31 + 127) / 255)
}
fn rgb888(p: u16) -> [u8; 3] {
    let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    ]
}
fn palette(a: u16, b: u16) -> [[u8; 3]; 4] {
    let (a, b) = (rgb888(a), rgb888(b));
    let mut p = [a, b, [0; 3], [0; 3]];
    for c in 0..3 {
        p[2][c] = ((2 * u16::from(a[c]) + u16::from(b[c])) / 3) as u8;
        p[3][c] = ((u16::from(a[c]) + 2 * u16::from(b[c])) / 3) as u8;
    }
    p
}
fn block(points: &[[u8; 3]; 16]) -> [u8; 8] {
    // Fit endpoints along a color principal axis rather than arbitrary channel
    // extrema, then refine the two endpoints against their chosen BC1 indices.
    let mut mean = [0.; 3];
    for p in points {
        for c in 0..3 {
            mean[c] += f64::from(p[c]) / 16.;
        }
    }
    let mut covariance = [[0.; 3]; 3];
    for p in points {
        for x in 0..3 {
            for y in 0..3 {
                covariance[x][y] += (f64::from(p[x]) - mean[x]) * (f64::from(p[y]) - mean[y]);
            }
        }
    }
    let mut axis = [1., 1., 1.];
    for _ in 0..8 {
        let v = std::array::from_fn::<_, 3, _>(|i| {
            (0..3).map(|j| covariance[i][j] * axis[j]).sum::<f64>()
        });
        let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm < 1e-9 {
            break;
        }
        for c in 0..3 {
            axis[c] = v[c] / norm;
        }
    }
    let projection = |p: &[u8; 3]| (0..3).map(|c| f64::from(p[c]) * axis[c]).sum::<f64>();
    let low = *points
        .iter()
        .min_by(|a, b| projection(a).total_cmp(&projection(b)))
        .unwrap();
    let high = *points
        .iter()
        .max_by(|a, b| projection(a).total_cmp(&projection(b)))
        .unwrap();
    let (mut a, mut b) = (rgb565(high), rgb565(low));
    if a < b {
        std::mem::swap(&mut a, &mut b);
    }
    if a == b {
        if a < u16::MAX {
            a += 1;
        } else {
            b -= 1;
        }
    }
    let mut indices = 0u32;
    for pass in 0..3 {
        let colors = palette(a, b);
        indices = 0;
        let (mut aa, mut ab, mut bb) = (0., 0., 0.);
        let (mut ax, mut bx) = ([0.; 3], [0.; 3]);
        for (i, p) in points.iter().enumerate() {
            let index = (0..4)
                .min_by_key(|&n| {
                    (0..3)
                        .map(|c| {
                            let d = i32::from(p[c]) - i32::from(colors[n][c]);
                            [3, 6, 1][c] * d * d
                        })
                        .sum::<i32>()
                })
                .unwrap();
            indices |= (index as u32) << (2 * i);
            let (x, y) = match index {
                0 => (1., 0.),
                1 => (0., 1.),
                2 => (2. / 3., 1. / 3.),
                _ => (1. / 3., 2. / 3.),
            };
            aa += x * x;
            ab += x * y;
            bb += y * y;
            for c in 0..3 {
                ax[c] += x * f64::from(p[c]);
                bx[c] += y * f64::from(p[c]);
            }
        }
        if pass == 2 {
            break;
        }
        let determinant = aa * bb - ab * ab;
        if determinant < 1e-9 {
            break;
        }
        let x = std::array::from_fn(|c| {
            ((ax[c] * bb - bx[c] * ab) / determinant)
                .round()
                .clamp(0., 255.) as u8
        });
        let y = std::array::from_fn(|c| {
            ((bx[c] * aa - ax[c] * ab) / determinant)
                .round()
                .clamp(0., 255.) as u8
        });
        let (mut next_a, mut next_b) = (rgb565(x), rgb565(y));
        if next_a < next_b {
            std::mem::swap(&mut next_a, &mut next_b);
        }
        if next_a == next_b {
            break;
        }
        a = next_a;
        b = next_b;
    }
    let mut out = [0; 8];
    out[..2].copy_from_slice(&a.to_le_bytes());
    out[2..4].copy_from_slice(&b.to_le_bytes());
    out[4..].copy_from_slice(&indices.to_le_bytes());
    out
}
fn tld(rgba: &[u8], cancel: &AtomicBool) -> Result<Vec<u8>> {
    ensure!(
        rgba.len() == SIDE * SIDE * 4,
        "Artwork needs exact 256x256 RGBA pixels"
    );
    let mut out = vec![0; 32800];
    out[..4].copy_from_slice(b"TLD ");
    out[8..12].copy_from_slice(&[5, 33, 71, 1]);
    out[12..16].copy_from_slice(&31u32.to_le_bytes());
    out[16..20].copy_from_slice(&[0, 1, 0, 1]);
    out[20..24].copy_from_slice(&[1, 0, 1, 0]);
    out[24..28].copy_from_slice(&32768u32.to_le_bytes());
    out[28..32].copy_from_slice(&32768u32.to_le_bytes());
    for by in 0..64 {
        check(cancel)?;
        for bx in 0..64 {
            let mut p = [[0; 3]; 16];
            for y in 0..4 {
                for x in 0..4 {
                    let at = ((by * 4 + y) * SIDE + bx * 4 + x) * 4;
                    p[y * 4 + x].copy_from_slice(&rgba[at..at + 3]);
                }
            }
            let at = 32 + (by * 64 + bx) * 8;
            out[at..at + 8].copy_from_slice(&block(&p));
        }
    }
    Ok(out)
}
pub(crate) fn validate_native(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() == 32800
            && &bytes[..4] == b"TLD "
            && bytes[4..8] == [0; 4]
            && bytes[8..12] == [5, 33, 71, 1]
            && bytes[12..16] == 31u32.to_le_bytes()
            && bytes[16..24] == [0, 1, 0, 1, 1, 0, 1, 0]
            && bytes[24..28] == 32768u32.to_le_bytes()
            && bytes[28..32] == 32768u32.to_le_bytes(),
        "Invalid 256x256 BC1 cover TLD"
    );
    Ok(())
}
fn preview(native: &[u8]) -> Result<Vec<u8>> {
    validate_native(native)?;
    let mut rgb = vec![0; SIDE * SIDE * 3];
    for by in 0..64 {
        for bx in 0..64 {
            let at = 32 + (by * 64 + bx) * 8;
            let a = u16::from_le_bytes(native[at..at + 2].try_into()?);
            let b = u16::from_le_bytes(native[at + 2..at + 4].try_into()?);
            ensure!(
                a > b,
                "Transparent BC1 mode is unsupported for opaque covers"
            );
            let p = palette(a, b);
            let bits = u32::from_le_bytes(native[at + 4..at + 8].try_into()?);
            for y in 0..4 {
                for x in 0..4 {
                    let dst = ((by * 4 + y) * SIDE + bx * 4 + x) * 3;
                    rgb[dst..dst + 3]
                        .copy_from_slice(&p[((bits >> (2 * (y * 4 + x))) & 3) as usize]);
                }
            }
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, SIDE as u32, SIDE as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgb)?;
    }
    Ok(png)
}
pub(crate) fn prepare(input: &[u8], cancel: &AtomicBool) -> Result<Prepared> {
    let normalized = pixels(input, cancel)?;
    let native = tld(&normalized, cancel)?;
    check(cancel)?;
    let preview = preview(&native)?;
    check(cancel)?;
    Ok(Prepared { native, preview })
}

#[derive(Clone, Debug)]
struct Match {
    kind: &'static str,
    id: String,
    title: String,
    artist: String,
    album: String,
}
fn clean(value: &str, label: &str) -> Result<String> {
    ensure!(
        value.len() <= 1000 && !value.chars().any(|c| c.is_control() && !c.is_whitespace()),
        "Invalid {label} for artwork search"
    );
    let s = value.split_whitespace().collect::<Vec<_>>().join(" ");
    ensure!(
        !s.is_empty() && s.encode_utf16().count() <= 120,
        "{label} is required and must be at most 120 characters"
    );
    Ok(s)
}
fn lucene(value: &str) -> String {
    let mut s = String::new();
    for c in value.chars() {
        if "+-&|!(){}[]^\"~*?:\\/".contains(c) {
            s.push('\\');
        }
        s.push(c);
    }
    s
}
fn search_url(artist: &str, title: &str, broad: bool) -> Result<String> {
    let artist = clean(artist, "Artist")?;
    ensure!(
        !artist.eq_ignore_ascii_case("custom"),
        "Name the artist before searching for artwork"
    );
    let title = clean(title, "Song title")?;
    let query = if broad {
        format!(
            "recording:{} AND artist:{}",
            lucene(&title),
            lucene(&artist)
        )
    } else {
        format!(
            "recording:\"{}\" AND artist:\"{}\"",
            lucene(&title),
            lucene(&artist)
        )
    };
    let mut url = tauri::Url::parse("https://musicbrainz.org/ws/2/recording")?;
    url.query_pairs_mut()
        .append_pair("query", &query)
        .append_pair("fmt", "json")
        .append_pair("limit", "5");
    Ok(url.to_string())
}
fn safe_url(value: &str) -> Result<tauri::Url> {
    ensure!(
        value.len() <= 4096 && !value.chars().any(char::is_control),
        "Unsafe artwork URL"
    );
    let mut u = tauri::Url::parse(value)?;
    let host = u.host_str().context("Artwork URL host missing")?;
    ensure!(
        (host == "musicbrainz.org"
            || host == "coverartarchive.org"
            || host == "archive.org"
            || host.ends_with(".archive.org"))
            && matches!(u.scheme(), "http" | "https")
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
            && u.fragment().is_none(),
        "Artwork service returned an untrusted URL"
    );
    u.set_scheme("https")
        .map_err(|_| anyhow::anyhow!("Artwork URL scheme rejected"))?;
    Ok(u)
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
fn label(value: Option<&str>) -> String {
    value
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect()
}
fn matches(bytes: &[u8]) -> Result<Vec<Match>> {
    ensure!(bytes.len() <= MAX_JSON, "Artwork search response too large");
    let v: Value = serde_json::from_slice(bytes).context("Artwork search returned invalid JSON")?;
    let rows = v["recordings"]
        .as_array()
        .context("Artwork search has no recording list")?;
    ensure!(
        rows.len() <= 10,
        "Artwork search returned too many recordings"
    );
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
    for recording in rows {
        let Some(releases) = recording["releases"].as_array() else {
            continue;
        };
        ensure!(
            releases.len() <= 128,
            "Artwork search returned too many releases"
        );
        let artist = recording["artist-credit"]
            .as_array()
            .map(|a| {
                a.iter()
                    .take(16)
                    .map(|c| label(c["name"].as_str()))
                    .collect::<String>()
            })
            .unwrap_or_default();
        for release in releases {
            for (kind, id) in [
                ("release-group", release["release-group"]["id"].as_str()),
                ("release", release["id"].as_str()),
            ] {
                if let Some(id) = id.filter(|id| uuid(id)) {
                    if seen.insert(format!("{kind}:{id}")) {
                        out.push(Match {
                            kind,
                            id: id.to_ascii_lowercase(),
                            title: label(recording["title"].as_str()),
                            artist: artist.chars().take(200).collect(),
                            album: label(
                                release["title"]
                                    .as_str()
                                    .or(release["release-group"]["title"].as_str()),
                            ),
                        });
                        if out.len() == MAX_CANDIDATES {
                            return Ok(out);
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

#[derive(Default)]
struct Response {
    status: u16,
    content_type: String,
    encoding: String,
    location: String,
    body: Vec<u8>,
}
trait Transport {
    fn get(
        &self,
        url: &str,
        max: usize,
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> Result<Response>;
}
struct WinHttp;
impl Transport for WinHttp {
    fn get(
        &self,
        url: &str,
        max: usize,
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> Result<Response> {
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
                "Artwork network handle failed: {}",
                std::io::Error::last_os_error()
            );
            Ok(Handle(p))
        }
        fn header(h: *mut c_void, key: u32, optional: bool) -> Result<String> {
            let mut buf = [0u16; 4096];
            let mut size = (buf.len() * 2) as u32;
            let mut index = 0;
            let result = unsafe {
                WinHttpQueryHeaders(
                    h,
                    key,
                    PCWSTR::null(),
                    Some(buf.as_mut_ptr().cast()),
                    &mut size,
                    &mut index,
                )
            };
            if let Err(e) = result {
                if optional && e.code().0 as u32 == 0x80072F76 {
                    return Ok(String::new());
                }
                return Err(e.into());
            }
            ensure!(
                size as usize <= buf.len() * 2 && size % 2 == 0,
                "Invalid artwork HTTP header"
            );
            Ok(String::from_utf16(&buf[..size as usize / 2])?
                .trim_end_matches('\0')
                .into())
        }
        fn budget(h: *mut c_void, deadline: Instant, cancel: &AtomicBool) -> Result<()> {
            check(cancel)?;
            let ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(2000) as i32;
            ensure!(ms > 0, "Artwork lookup timed out");
            unsafe {
                WinHttpSetTimeouts(h, ms, ms, ms, ms)?;
            }
            Ok(())
        }
        let url = safe_url(url)?;
        let host = url
            .host_str()
            .unwrap()
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let object = format!(
            "{}{}",
            url.path(),
            url.query().map(|q| format!("?{q}")).unwrap_or_default()
        )
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>();
        let ua = USER_AGENT.encode_utf16().chain([0]).collect::<Vec<_>>();
        unsafe {
            check(cancel)?;
            let session = opened(WinHttpOpen(
                PCWSTR(ua.as_ptr()),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))?;
            budget(session.0, deadline, cancel)?;
            let connection = opened(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0))?;
            let verb = "GET\0".encode_utf16().collect::<Vec<_>>();
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
            budget(request.0, deadline, cancel)?;
            let headers="Accept: application/json,image/png,image/jpeg,image/webp,image/bmp\r\nAccept-Encoding: identity\r\n".encode_utf16().collect::<Vec<_>>();
            WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)?;
            budget(request.0, deadline, cancel)?;
            WinHttpReceiveResponse(request.0, ptr::null_mut())?;
            let status = header(request.0, WINHTTP_QUERY_STATUS_CODE, false)?.parse()?;
            let mut r = Response {
                status,
                ..Default::default()
            };
            if [301, 302, 303, 307, 308].contains(&status) {
                r.location = header(request.0, WINHTTP_QUERY_LOCATION, false)?;
                return Ok(r);
            }
            if status != 200 {
                return Ok(r);
            }
            r.content_type = header(request.0, WINHTTP_QUERY_CONTENT_TYPE, false)?;
            r.encoding = header(request.0, WINHTTP_QUERY_CONTENT_ENCODING, true)?;
            let length = header(request.0, WINHTTP_QUERY_CONTENT_LENGTH, true)?;
            if !length.is_empty() {
                ensure!(
                    length.parse::<u64>()? <= max as u64,
                    "Artwork HTTP response exceeds safe size"
                );
            }
            let mut chunk = [0; 16384];
            loop {
                budget(request.0, deadline, cancel)?;
                let mut read = 0;
                WinHttpReadData(
                    request.0,
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut read,
                )?;
                ensure!(
                    read as usize <= chunk.len() && r.body.len() + read as usize <= max,
                    "Artwork response exceeds safe size"
                );
                if read == 0 {
                    break;
                }
                r.body.extend_from_slice(&chunk[..read as usize]);
            }
            check(cancel)?;
            Ok(r)
        }
    }
}
fn reserve(next: &mut Option<Instant>, now: Instant) -> Instant {
    let slot = next.filter(|v| *v > now).unwrap_or(now);
    *next = Some(slot + Duration::from_millis(1100));
    slot
}
fn wait(until: Instant, deadline: Instant, cancel: &AtomicBool) -> Result<()> {
    loop {
        check(cancel)?;
        let now = Instant::now();
        ensure!(now < deadline, "Artwork lookup timed out");
        if now >= until {
            return Ok(());
        }
        std::thread::sleep((until - now).min(Duration::from_millis(50)));
    }
}
fn musicbrainz_rate(deadline: Instant, cancel: &AtomicBool) -> Result<()> {
    static NEXT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    let slot = reserve(
        &mut *NEXT
            .get_or_init(|| Mutex::new(None))
            .lock()
            .map_err(|_| anyhow::anyhow!("Artwork rate limiter unavailable"))?,
        Instant::now(),
    );
    wait(slot, deadline, cancel)
}
fn fetch(
    t: &impl Transport,
    initial: &str,
    max: usize,
    deadline: Instant,
    cancel: &AtomicBool,
    rate: &impl Fn(Instant, &AtomicBool) -> Result<()>,
) -> Result<Response> {
    let mut url = safe_url(initial)?;
    for redirect in 0..=5 {
        check(cancel)?;
        ensure!(Instant::now() < deadline, "Artwork lookup timed out");
        if url.host_str() == Some("musicbrainz.org") {
            rate(deadline, cancel)?;
        }
        let r = t.get(url.as_str(), max, deadline, cancel)?;
        check(cancel)?;
        ensure!(r.body.len() <= max, "Artwork response exceeds safe size");
        if [301, 302, 303, 307, 308].contains(&r.status) {
            ensure!(redirect < 5, "Too many artwork redirects");
            url = safe_url(url.join(&r.location)?.as_str())?;
            continue;
        }
        ensure!(
            r.encoding.is_empty() || r.encoding.eq_ignore_ascii_case("identity"),
            "Unsupported compressed artwork response"
        );
        return Ok(r);
    }
    unreachable!()
}
fn mime(value: &str) -> &str {
    value.split(';').next().unwrap_or("").trim()
}
fn lookup_with(
    t: &impl Transport,
    artist: &str,
    title: &str,
    cancel: &AtomicBool,
    rate: &impl Fn(Instant, &AtomicBool) -> Result<()>,
) -> Result<Vec<Candidate>> {
    let deadline = Instant::now() + Duration::from_secs(60);
    check(cancel)?;
    let mut candidates = Vec::new();
    for broad in [false, true] {
        let r = fetch(
            t,
            &search_url(artist, title, broad)?,
            MAX_JSON,
            deadline,
            cancel,
            rate,
        )?;
        ensure!(
            r.status == 200,
            "Artwork search service returned HTTP {}; try again later",
            r.status
        );
        ensure!(
            mime(&r.content_type).eq_ignore_ascii_case("application/json"),
            "Artwork search did not return JSON"
        );
        candidates = matches(&r.body)?;
        if !candidates.is_empty() {
            break;
        }
    }
    ensure!(
        !candidates.is_empty(),
        "No online music match was found; refine artist and song title"
    );
    for candidate in candidates {
        let url = format!(
            "https://coverartarchive.org/{}/{}/front-500",
            candidate.kind, candidate.id
        );
        let r = fetch(t, &url, MAX_INPUT, deadline, cancel, rate)?;
        if r.status == 404 {
            continue;
        }
        ensure!(
            r.status == 200,
            "Cover artwork service returned HTTP {}; try again later",
            r.status
        );
        ensure!(
            [
                "image/png",
                "image/jpeg",
                "image/webp",
                "image/bmp",
                "image/x-ms-bmp"
            ]
            .contains(&mime(&r.content_type).to_ascii_lowercase().as_str()),
            "Cover service returned an unsupported image"
        );
        let artwork = prepare(&r.body, cancel)?;
        return Ok(vec![Candidate {
            label: format!(
                "{} · {}",
                candidate.artist,
                if candidate.album.is_empty() {
                    &candidate.title
                } else {
                    &candidate.album
                }
            ),
            artwork,
            matched_title: candidate.title,
            matched_artist: candidate.artist,
            source: "MusicBrainz / Cover Art Archive".into(),
        }]);
    }
    anyhow::bail!("Music matched, but no cover artwork is available for this song")
}
pub(crate) fn lookup(artist: &str, title: &str, cancel: &AtomicBool) -> Result<Vec<Candidate>> {
    lookup_with(&WinHttp, artist, title, cancel, &musicbrainz_rate)
}
#[cfg(test)]
#[path = "jukebox_artwork_tests.rs"]
mod tests;
