use super::*;
use serde_json::json;
use std::{cell::RefCell, collections::VecDeque, io::Cursor};
fn png(width: u32, height: u32, color: png::ColorType, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, width, height);
        e.set_color(color);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(bytes).unwrap();
    }
    out
}
fn read_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut r = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut out = vec![0; r.output_buffer_size().unwrap()];
    let f = r.next_frame(&mut out).unwrap();
    out.truncate(f.buffer_size());
    (f.width, f.height, out)
}
fn cover() -> Vec<u8> {
    png(2, 2, png::ColorType::Rgb, &[0, 255, 0].repeat(4))
}
fn response(body: Vec<u8>, content_type: &str) -> Response {
    Response {
        status: 200,
        content_type: content_type.into(),
        body,
        ..Default::default()
    }
}
fn search() -> Vec<u8> {
    serde_json::to_vec(&json!({"recordings":[{"title":"Song","artist-credit":[{"name":"Artist"}],"releases":[{"id":"11111111-1111-1111-1111-111111111111","title":"Album","release-group":{"id":"22222222-2222-2222-2222-222222222222"}}]}]})).unwrap()
}
struct Mock {
    responses: RefCell<VecDeque<Response>>,
    requests: RefCell<Vec<String>>,
    cancel_on_get: bool,
}
impl Mock {
    fn new(responses: Vec<Response>) -> Self {
        Self {
            responses: RefCell::new(responses.into()),
            requests: RefCell::new(Vec::new()),
            cancel_on_get: false,
        }
    }
}
impl Transport for Mock {
    fn get(&self, url: &str, _: usize, _: Instant, cancel: &AtomicBool) -> Result<Response> {
        self.requests.borrow_mut().push(url.into());
        if self.cancel_on_get {
            cancel.store(true, Ordering::Release);
        }
        self.responses
            .borrow_mut()
            .pop_front()
            .context("No mock response")
    }
}
#[test]
fn bc1_header_and_preview_match_the_exact_game_payload() {
    let cancel = AtomicBool::new(false);
    for color in [
        [0, 0, 0],
        [255, 255, 255],
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [200, 40, 90],
    ] {
        let rgba = [color[0], color[1], color[2], 255].repeat(SIDE * SIDE);
        let native = tld(&rgba, &cancel).unwrap();
        validate_native(&native).unwrap();
        assert_eq!(native.len(), 32800);
        for b in native[32..].chunks_exact(8) {
            assert!(
                u16::from_le_bytes(b[..2].try_into().unwrap())
                    > u16::from_le_bytes(b[2..4].try_into().unwrap())
            );
        }
        let (w, h, decoded) = read_png(&preview(&native).unwrap());
        assert_eq!((w, h), (256, 256));
        for p in decoded.chunks_exact(3) {
            for c in 0..3 {
                assert!(
                    (i16::from(p[c]) - i16::from(color[c])).abs() <= 8,
                    "{color:?} -> {p:?}"
                );
            }
        }
    }
}
#[test]
fn actual_wic_png_uses_center_crop_opaque_black_and_prepared_png() {
    let cancel = AtomicBool::new(false);
    let row = [255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255];
    let input = png(4, 2, png::ColorType::Rgb, &row.repeat(2));
    let prepared = prepare(&input, &cancel).unwrap();
    let (w, h, p) = read_png(&prepared.preview);
    assert_eq!((w, h), (256, 256));
    assert!(p.chunks_exact(3).all(|c| c == [0, 255, 0]));
    let transparent = png(1, 1, png::ColorType::Rgba, &[255, 255, 255, 0]);
    let p = read_png(&prepare(&transparent, &cancel).unwrap().preview).2;
    assert!(p.iter().all(|v| *v == 0));
}
#[test]
fn actual_wic_bmp_and_jpeg_are_accepted_without_external_tools() {
    let cancel = AtomicBool::new(false);
    let mut bmp = vec![0u8; 58];
    bmp[..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
    bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
    bmp[34..38].copy_from_slice(&4u32.to_le_bytes());
    bmp[54..57].copy_from_slice(&[90, 40, 200]);
    validate_native(&prepare(&bmp, &cancel).unwrap().native).unwrap();
    let jpeg: &[u8] = &[
        255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67,
        0, 3, 2, 2, 3, 2, 2, 3, 3, 3, 3, 4, 3, 3, 4, 5, 8, 5, 5, 4, 4, 5, 10, 7, 7, 6, 8, 12, 10,
        12, 12, 11, 10, 11, 11, 13, 14, 18, 16, 13, 14, 17, 14, 11, 11, 16, 22, 16, 17, 19, 20, 21,
        21, 21, 12, 15, 23, 24, 22, 20, 24, 18, 20, 21, 20, 255, 219, 0, 67, 1, 3, 4, 4, 5, 4, 5,
        9, 5, 5, 9, 20, 13, 11, 13, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20,
        20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20,
        20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 255, 192, 0, 17, 8, 0, 2, 0, 2, 3, 1, 34, 0, 2,
        17, 1, 3, 17, 1, 255, 196, 0, 21, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7,
        255, 196, 0, 20, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 196, 0, 21, 1,
        1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 8, 255, 196, 0, 20, 17, 1, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 146,
        0, 37, 150, 255, 217,
    ];
    validate_native(&prepare(jpeg, &cancel).unwrap().native).unwrap();
}
#[test]
fn malformed_oversized_cancelled_or_other_format_is_not_decoded() {
    let cancel = AtomicBool::new(false);
    for bytes in [
        &b"not an image"[..],
        &b"GIF89a000000"[..],
        &b"\x89PNG\r\n\x1a\nxxxx"[..],
        &b"RIFFxxxxWEBPxxxx"[..],
    ] {
        assert!(prepare(bytes, &cancel).is_err());
    }
    assert!(image_kind(&vec![0; MAX_INPUT + 1]).is_err());
    let cancel = AtomicBool::new(true);
    assert!(
        prepare(&cover(), &cancel)
            .unwrap_err()
            .to_string()
            .contains("cancel")
    );
    assert!(tld(&vec![0; SIDE * SIDE * 4], &cancel).is_err());
}
#[test]
fn native_header_rejects_changed_format_dimensions_mips_or_trailing_data() {
    let original = tld(
        &[0, 255, 0, 255].repeat(SIDE * SIDE),
        &AtomicBool::new(false),
    )
    .unwrap();
    for offset in [0, 4, 8, 9, 10, 11, 12, 16, 18, 20, 22, 24, 28] {
        let mut b = original.clone();
        b[offset] ^= 1;
        assert!(validate_native(&b).is_err(), "header offset {offset}");
    }
    let mut b = original;
    b.push(0);
    assert!(validate_native(&b).is_err());
}
#[test]
fn query_is_bounded_escaped_and_cannot_be_a_renderer_url() {
    let u = search_url(" A & B ", "Song \"title\"", false).unwrap();
    let u = tauri::Url::parse(&u).unwrap();
    assert_eq!(u.host_str(), Some("musicbrainz.org"));
    let q = u.query_pairs().find(|(k, _)| k == "query").unwrap().1;
    assert_eq!(q, "recording:\"Song \\\"title\\\"\" AND artist:\"A \\& B\"");
    for (artist, title) in [
        ("Custom", "Song"),
        ("", "Song"),
        ("Artist", ""),
        ("\0x", "Song"),
    ] {
        assert!(search_url(artist, title, false).is_err());
    }
    assert!(search_url(&"a".repeat(121), "Song", false).is_err());
}
#[test]
fn allowed_redirects_never_accept_credentials_ports_non_https_or_foreign_hosts() {
    for url in [
        "file:///c:/x",
        "https://evil.invalid/x",
        "https://archive.org.evil.invalid/x",
        "https://u:p@archive.org/x",
        "https://archive.org:444/x",
        "https://archive.org/x#fragment",
        "https://archive.org/x\r\nX:Y",
        "https://archive.org@127.0.0.1/x",
    ] {
        assert!(safe_url(url).is_err(), "{url}");
    }
    assert_eq!(
        safe_url("http://ia801.archive.org/file").unwrap().scheme(),
        "https"
    );
    assert!(safe_url("https://coverartarchive.org/release/x/front-500").is_ok());
}
#[test]
fn bounded_candidate_parser_prefers_group_deduplicates_and_skips_bad_ids() {
    let parsed = matches(&search()).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].kind, "release-group");
    assert_eq!(parsed[0].artist, "Artist");
    assert_eq!(parsed[1].kind, "release");
    let bytes =
        serde_json::to_vec(&json!({"recordings":[{"releases":[{"id":"../../unsafe"}]}]})).unwrap();
    assert!(matches(&bytes).unwrap().is_empty());
    assert!(matches(&vec![b' '; MAX_JSON + 1]).is_err());
    assert!(matches(br#"{"recordings":"bad"}"#).is_err());
}
#[test]
fn lookup_mock_only_downloads_the_first_available_safe_prepared_cover() {
    let mock = Mock::new(vec![
        response(search(), "application/json"),
        Response {
            status: 404,
            ..Default::default()
        },
        Response {
            status: 302,
            location: "https://ia800.archive.org/cover.png".into(),
            ..Default::default()
        },
        response(cover(), "image/png"),
    ]);
    let result = lookup_with(&mock, "Artist", "Song", &AtomicBool::new(false), &|_, _| {
        Ok(())
    })
    .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].matched_artist, "Artist");
    assert_eq!(result[0].source, "MusicBrainz / Cover Art Archive");
    validate_native(&result[0].artwork.native).unwrap();
    assert_eq!(mock.requests.borrow().len(), 4);
    assert!(mock.requests.borrow()[1].contains("release-group/22222222"));
}
#[test]
fn lookup_broadens_empty_search_but_never_accepts_html_or_foreign_redirects() {
    let mock = Mock::new(vec![
        response(br#"{"recordings":[]}"#.to_vec(), "application/json"),
        response(search(), "application/json"),
        response(cover(), "image/png"),
    ]);
    assert!(
        lookup_with(
            &mock,
            "Artist",
            "Song",
            &AtomicBool::new(false),
            &|_, _| Ok(())
        )
        .is_ok()
    );
    assert_eq!(mock.requests.borrow().len(), 3);
    for r in [
        Response {
            status: 302,
            location: "https://127.0.0.1/private".into(),
            ..Default::default()
        },
        response(b"<html>no</html>".to_vec(), "text/html"),
        Response {
            status: 429,
            ..Default::default()
        },
    ] {
        let mock = Mock::new(vec![r]);
        assert!(
            lookup_with(
                &mock,
                "Artist",
                "Song",
                &AtomicBool::new(false),
                &|_, _| Ok(())
            )
            .is_err()
        );
    }
}
#[test]
fn network_cancel_size_compression_and_redirect_limits_are_enforced_with_mocks() {
    let cancel = AtomicBool::new(false);
    let mut mock = Mock::new(vec![response(search(), "application/json")]);
    mock.cancel_on_get = true;
    assert!(lookup_with(&mock, "Artist", "Song", &cancel, &|_, _| Ok(())).is_err());
    let mock = Mock::new(vec![response(vec![0; MAX_JSON + 1], "application/json")]);
    assert!(
        lookup_with(
            &mock,
            "Artist",
            "Song",
            &AtomicBool::new(false),
            &|_, _| Ok(())
        )
        .is_err()
    );
    let mock = Mock::new(vec![Response {
        status: 200,
        content_type: "application/json".into(),
        encoding: "gzip".into(),
        body: search(),
        ..Default::default()
    }]);
    assert!(
        lookup_with(
            &mock,
            "Artist",
            "Song",
            &AtomicBool::new(false),
            &|_, _| Ok(())
        )
        .is_err()
    );
    let mock = Mock::new(
        (0..6)
            .map(|_| Response {
                status: 302,
                location: "/again".into(),
                ..Default::default()
            })
            .collect(),
    );
    assert!(
        fetch(
            &mock,
            "https://archive.org/x",
            MAX_INPUT,
            Instant::now() + Duration::from_secs(1),
            &AtomicBool::new(false),
            &|_, _| Ok(())
        )
        .is_err()
    );
    assert_eq!(mock.requests.borrow().len(), 6);
}
#[test]
fn musicbrainz_slots_are_more_than_one_second_apart_even_when_concurrent() {
    let now = Instant::now();
    let mut next = None;
    let a = reserve(&mut next, now);
    let b = reserve(&mut next, now);
    let c = reserve(&mut next, now + Duration::from_millis(100));
    assert_eq!(a, now);
    assert!(b - a >= Duration::from_millis(1100));
    assert!(c - b >= Duration::from_millis(1100));
    let future = now + Duration::from_secs(30);
    assert_eq!(reserve(&mut next, future), future);
}
