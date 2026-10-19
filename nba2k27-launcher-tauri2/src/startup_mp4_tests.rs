use super::*;
use std::io::Cursor;
fn bx(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut b = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend(kind);
    b.extend(payload);
    b
}
fn join(parts: Vec<Vec<u8>>) -> Vec<u8> {
    parts.into_iter().flatten().collect()
}
fn track(codec: &[u8; 4], audio: bool) -> Vec<u8> {
    let mut hdlr = vec![0; 12];
    hdlr[8..12].copy_from_slice(if audio { b"soun" } else { b"vide" });
    let mut sample = vec![0; if audio { 28 } else { 78 }];
    if !audio {
        sample[24..26].copy_from_slice(&1920u16.to_be_bytes());
        sample[26..28].copy_from_slice(&1080u16.to_be_bytes());
    }
    sample.extend(bx(
        if audio {
            b"esds"
        } else {
            match codec {
                b"av01" => b"av1C",
                b"vp09" => b"vpcC",
                _ => b"avcC",
            }
        },
        if audio { vec![0; 6] } else { vec![1; 8] },
    ));
    let mut stsd = vec![0; 4];
    stsd.extend(1u32.to_be_bytes());
    stsd.extend(bx(codec, sample));
    bx(
        b"trak",
        bx(
            b"mdia",
            join(vec![
                bx(b"hdlr", hdlr),
                bx(b"minf", bx(b"stbl", bx(b"stsd", stsd))),
            ]),
        ),
    )
}
fn moov(codec: &[u8; 4], audio: bool, version: u8, scale: u32, ticks: u64) -> Vec<u8> {
    let mut mvhd = vec![0; if version == 1 { 32 } else { 20 }];
    mvhd[0] = version;
    if version == 1 {
        mvhd[20..24].copy_from_slice(&scale.to_be_bytes());
        mvhd[24..32].copy_from_slice(&ticks.to_be_bytes());
    } else {
        mvhd[12..16].copy_from_slice(&scale.to_be_bytes());
        mvhd[16..20].copy_from_slice(&(ticks as u32).to_be_bytes());
    }
    let mut parts = vec![bx(b"mvhd", mvhd), track(codec, false)];
    if audio {
        parts.push(track(b"mp4a", true));
    }
    bx(b"moov", join(parts))
}
fn ftyp() -> Vec<u8> {
    bx(
        b"ftyp",
        join(vec![b"isom".to_vec(), vec![0; 4], b"mp42".to_vec()]),
    )
}
pub(crate) fn valid(codec: &[u8; 4], audio: bool) -> Vec<u8> {
    join(vec![
        ftyp(),
        moov(codec, audio, 0, 1000, 45000),
        bx(b"mdat", vec![1; 64]),
    ])
}
#[test]
fn h264_av1_vp9_aac_and_silent_metadata_match_production_choices() {
    for (codec, want) in [
        (b"avc1", Codec::H264),
        (b"avc3", Codec::H264),
        (b"av01", Codec::Av1),
        (b"vp09", Codec::Vp9),
    ] {
        for audio in [false, true] {
            let info = inspect(&mut Cursor::new(valid(codec, audio)), "MP4").unwrap();
            assert_eq!(info.duration_ms, 45000);
            assert_eq!(info.video_codecs, vec![want]);
            assert_eq!(info.audio_tracks, usize::from(audio));
            assert_eq!((info.max_width, info.max_height), (1920, 1080));
            assert!(info.playback_probe_required);
        }
    }
}
#[test]
fn duration_versions_limits_zero_and_overflow_are_checked_without_floats() {
    for version in [0, 1] {
        let data = join(vec![
            ftyp(),
            moov(b"avc1", false, version, 1000, 600000),
            bx(b"mdat", vec![1]),
        ]);
        assert_eq!(
            inspect(&mut Cursor::new(data), "mp4").unwrap().duration_ms,
            600000
        );
    }
    for (scale, ticks) in [(0, 100), (1000, 0), (1000, 600001), (1, u64::MAX)] {
        let data = join(vec![
            ftyp(),
            moov(b"avc1", false, 1, scale, ticks),
            bx(b"mdat", vec![1]),
        ]);
        assert!(inspect(&mut Cursor::new(data), "mp4").is_err());
    }
}
#[test]
fn legacy_mov_fake_mp4_missing_media_and_duplicate_metadata_are_refused() {
    assert!(inspect(&mut Cursor::new(valid(b"avc1", false)), "mov").is_err());
    for data in [
        vec![0; 32],
        join(vec![ftyp(), moov(b"avc1", false, 0, 1000, 1000)]),
        join(vec![
            ftyp(),
            ftyp(),
            moov(b"avc1", false, 0, 1000, 1000),
            bx(b"mdat", vec![1]),
        ]),
        join(vec![
            ftyp(),
            moov(b"avc1", false, 0, 1000, 1000),
            moov(b"avc1", false, 0, 1000, 1000),
            bx(b"mdat", vec![1]),
        ]),
    ] {
        assert!(inspect(&mut Cursor::new(data), "mp4").is_err());
    }
    let qt = join(vec![
        bx(b"ftyp", join(vec![b"qt  ".to_vec(), vec![0; 4]])),
        moov(b"avc1", false, 0, 1000, 1000),
        bx(b"mdat", vec![1]),
    ]);
    assert!(inspect(&mut Cursor::new(qt), "mp4").is_err());
}
#[test]
fn truncated_extended_and_escaping_boxes_never_panic_or_allocate_payload() {
    let source = valid(b"avc1", true);
    for len in 0..source.len() {
        let bytes = source[..len].to_vec();
        let _ = inspect(&mut Cursor::new(bytes), "mp4");
    }
    for size in [1u64, 7, 15, u64::MAX] {
        let mut data = 1u32.to_be_bytes().to_vec();
        data.extend(b"moov");
        data.extend(size.to_be_bytes());
        data.extend([0; 32]);
        assert!(inspect(&mut Cursor::new(data), "mp4").is_err());
    }
}
#[test]
fn unsupported_and_encrypted_video_tracks_are_not_marked_ready() {
    for codec in [b"hvc1", b"hev1", b"encv", b"jpeg", b"zzzz"] {
        assert!(inspect(&mut Cursor::new(valid(codec, true)), "mp4").is_err());
    }
}
#[test]
fn changed_codec_count_empty_config_dimensions_and_audio_are_refused() {
    for mutate in 0..5 {
        let mut data = valid(b"avc1", true);
        let at = data.windows(4).position(|b| b == b"stsd").unwrap();
        match mutate {
            0 => data[at + 8..at + 12].copy_from_slice(&2u32.to_be_bytes()),
            1 => {
                let p = data.windows(4).position(|b| b == b"avcC").unwrap();
                data[p - 4..p].copy_from_slice(&8u32.to_be_bytes());
            }
            2 => {
                let p = data.windows(4).position(|b| b == b"avc1").unwrap();
                data[p + 4 + 24..p + 4 + 26].copy_from_slice(&0u16.to_be_bytes());
            }
            3 => {
                let p = data.windows(4).position(|b| b == b"mp4a").unwrap();
                data[p..p + 4].copy_from_slice(b"enca");
            }
            _ => {
                let p = data.windows(4).position(|b| b == b"esds").unwrap();
                data[p..p + 4].copy_from_slice(b"junk");
            }
        }
        assert!(
            inspect(&mut Cursor::new(data), "mp4").is_err(),
            "mutation {mutate}"
        );
    }
}
#[test]
fn extended_and_size_zero_media_boxes_are_supported_without_decoding() {
    for zero in [false, true] {
        let mut media = if zero {
            0u32.to_be_bytes().to_vec()
        } else {
            1u32.to_be_bytes().to_vec()
        };
        media.extend(b"mdat");
        if !zero {
            media.extend(32u64.to_be_bytes());
        }
        media.extend(vec![1; if zero { 24 } else { 16 }]);
        let data = join(vec![ftyp(), moov(b"avc1", false, 0, 1000, 1000), media]);
        inspect(&mut Cursor::new(data), "mp4").unwrap();
    }
}
#[test]
fn metadata_and_top_level_box_budgets_are_bounded() {
    let mut b = bx(b"free", vec![]).repeat(10001);
    b.extend(valid(b"avc1", false));
    assert!(inspect(&mut Cursor::new(b), "mp4").is_err());
    let mut budget = MAX_BOXES;
    assert!(boxes(&bx(b"free", vec![]), 0..8, &mut budget).is_err());
}
#[test]
#[ignore = "explicit local user-provided MP4 folder metadata audit; no playback, writes or game access"]
fn actual_local_mp4_catalog_is_inspected_without_playback() {
    use std::{
        fs::{self, File},
        path::PathBuf,
    };
    let folder = PathBuf::from(
        std::env::var_os("C27_STARTUP_MP4_FOLDER")
            .expect("Explicit local MP4 source folder required"),
    );
    let mut rows = vec![];
    for entry in fs::read_dir(folder).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("mp4"))
        {
            continue;
        }
        let meta = fs::symlink_metadata(&path).unwrap();
        assert!(meta.is_file() && !meta.file_type().is_symlink());
        let mut file = File::open(&path).unwrap();
        let info =
            inspect(&mut file, "mp4").unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
        rows.push(serde_json::json!({"name":entry.file_name().to_string_lossy(),"bytes":info.bytes,"durationMs":info.duration_ms,"codecs":format!("{:?}",info.video_codecs),"videoTracks":info.video_tracks,"audioTracks":info.audio_tracks,"width":info.max_width,"height":info.max_height,"playbackProbeRequired":info.playback_probe_required}));
    }
    assert!(!rows.is_empty());
    println!(
        "{}",
        serde_json::json!({"schema":"nba2k27-startup-mp4-local-metadata-audit-v1","files":rows,"gameAccess":false,"videoPlayback":false,"filesChanged":false})
    );
}
