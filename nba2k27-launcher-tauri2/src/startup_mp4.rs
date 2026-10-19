//! Bounded ISO-BMFF import inspection. No decoder, transcode, MOV replacement,
//! game access or file publication. A native playback probe is still required:
//! valid container/codec metadata does not prove that every frame will decode.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    io::{Read, Seek, SeekFrom},
    ops::Range,
};
pub(crate) const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_METADATA: u64 = 16 * 1024 * 1024;
const MAX_BOXES: usize = 10_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Codec {
    H264,
    Av1,
    Vp9,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub bytes: u64,
    pub duration_ms: u64,
    pub video_codecs: Vec<Codec>,
    pub video_tracks: usize,
    pub audio_tracks: usize,
    pub max_width: u16,
    pub max_height: u16,
    pub playback_probe_required: bool,
}
#[derive(Clone, Debug)]
struct BoxRecord {
    kind: [u8; 4],
    payload: Range<usize>,
}
fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}
fn be64(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().unwrap())
}
fn boxes(b: &[u8], range: Range<usize>, budget: &mut usize) -> Result<Vec<BoxRecord>> {
    ensure!(
        range.start <= range.end && range.end <= b.len(),
        "MP4 metadata range rejected"
    );
    let mut at = range.start;
    let mut result = Vec::new();
    while at < range.end {
        ensure!(
            *budget < MAX_BOXES && range.end - at >= 8,
            "Invalid or excessive MP4 metadata boxes"
        );
        *budget += 1;
        let small = be32(b, at);
        let (size, header) = match small {
            0 => ((range.end - at) as u64, 8usize),
            1 => {
                ensure!(range.end - at >= 16, "Truncated extended MP4 box");
                (be64(b, at + 8), 16)
            }
            n => (u64::from(n), 8),
        };
        ensure!(
            size >= header as u64 && size <= (range.end - at) as u64,
            "Invalid MP4 metadata box size"
        );
        let end = at + usize::try_from(size)?;
        result.push(BoxRecord {
            kind: b[at + 4..at + 8].try_into()?,
            payload: at + header..end,
        });
        at = end;
    }
    Ok(result)
}
fn only<'a>(rows: &'a [BoxRecord], kind: &[u8; 4]) -> Result<&'a BoxRecord> {
    let matching = rows.iter().filter(|r| &r.kind == kind).collect::<Vec<_>>();
    ensure!(
        matching.len() == 1,
        "Missing or duplicate MP4 {} box",
        String::from_utf8_lossy(kind)
    );
    Ok(matching[0])
}
fn movie(b: &[u8], bytes: u64) -> Result<Metadata> {
    let mut budget = 0;
    let root = boxes(b, 0..b.len(), &mut budget)?;
    let mvhd = only(&root, b"mvhd")?;
    let header = &b[mvhd.payload.clone()];
    ensure!(!header.is_empty(), "MP4 duration is missing");
    let (scale, ticks) = match header[0] {
        0 => {
            ensure!(header.len() >= 20, "Truncated MP4 duration");
            (be32(header, 12), u64::from(be32(header, 16)))
        }
        1 => {
            ensure!(header.len() >= 32, "Truncated extended MP4 duration");
            (be32(header, 20), be64(header, 24))
        }
        _ => anyhow::bail!("Unsupported MP4 duration version"),
    };
    ensure!(
        scale > 0 && ticks > 0 && ticks <= u64::from(scale) * 600,
        "Choose a complete MP4 up to 10 minutes long"
    );
    let mut info = Metadata {
        bytes,
        duration_ms: (ticks * 1000).div_ceil(u64::from(scale)),
        video_codecs: vec![],
        video_tracks: 0,
        audio_tracks: 0,
        max_width: 0,
        max_height: 0,
        playback_probe_required: true,
    };
    let tracks = root
        .iter()
        .filter(|r| r.kind == *b"trak")
        .collect::<Vec<_>>();
    ensure!(
        !tracks.is_empty() && tracks.len() <= 64,
        "Invalid MP4 track count"
    );
    let mut codecs = BTreeSet::new();
    for track in tracks {
        let tr = boxes(b, track.payload.clone(), &mut budget)?;
        let mdia = only(&tr, b"mdia")?;
        let md = boxes(b, mdia.payload.clone(), &mut budget)?;
        let hdlr = only(&md, b"hdlr")?;
        ensure!(hdlr.payload.len() >= 12, "Truncated MP4 track handler");
        let kind = &b[hdlr.payload.start + 8..hdlr.payload.start + 12];
        if kind != b"vide" && kind != b"soun" {
            continue;
        }
        let minf = only(&md, b"minf")?;
        let min = boxes(b, minf.payload.clone(), &mut budget)?;
        let stbl = only(&min, b"stbl")?;
        let st = boxes(b, stbl.payload.clone(), &mut budget)?;
        let stsd = only(&st, b"stsd")?;
        ensure!(
            stsd.payload.len() >= 8 && b[stsd.payload.start] == 0,
            "Invalid MP4 sample-description header"
        );
        let entries = boxes(b, stsd.payload.start + 8..stsd.payload.end, &mut budget)?;
        ensure!(
            !entries.is_empty()
                && entries.len() <= 64
                && entries.len() == be32(b, stsd.payload.start + 4) as usize,
            "Invalid MP4 codec table"
        );
        if kind == b"vide" {
            info.video_tracks += 1;
        } else {
            info.audio_tracks += 1;
        }
        for entry in entries {
            if kind == b"vide" {
                let (codec, config) = match &entry.kind {
                    b"avc1" | b"avc3" => (Codec::H264, b"avcC"),
                    b"av01" => (Codec::Av1, b"av1C"),
                    b"vp09" => (Codec::Vp9, b"vpcC"),
                    _ => anyhow::bail!(
                        "Unsupported/encrypted MP4 video codec; use H.264, AV1 or VP9"
                    ),
                };
                ensure!(
                    entry.payload.len() >= 78,
                    "Truncated MP4 video sample entry"
                );
                let width = u16::from_be_bytes(
                    b[entry.payload.start + 24..entry.payload.start + 26].try_into()?,
                );
                let height = u16::from_be_bytes(
                    b[entry.payload.start + 26..entry.payload.start + 28].try_into()?,
                );
                ensure!(
                    width > 0
                        && height > 0
                        && width <= 16384
                        && height <= 16384
                        && u64::from(width) * u64::from(height) <= 67_108_864,
                    "Unsupported or invalid MP4 dimensions"
                );
                let children = boxes(b, entry.payload.start + 78..entry.payload.end, &mut budget)?;
                ensure!(
                    !only(&children, config)?.payload.is_empty(),
                    "MP4 video decoder configuration is missing"
                );
                info.max_width = info.max_width.max(width);
                info.max_height = info.max_height.max(height);
                codecs.insert(codec);
            } else {
                ensure!(
                    entry.kind == *b"mp4a" && entry.payload.len() >= 28,
                    "Unsupported/encrypted MP4 audio codec; use AAC or no audio"
                );
                let version = u16::from_be_bytes(
                    b[entry.payload.start + 8..entry.payload.start + 10].try_into()?,
                );
                ensure!(version == 0, "Unsupported MP4 audio sample-entry version");
                let children = boxes(b, entry.payload.start + 28..entry.payload.end, &mut budget)?;
                let esds = only(&children, b"esds")?;
                ensure!(
                    esds.payload.len() >= 6 && b[esds.payload.start] == 0,
                    "MP4 AAC decoder configuration is missing"
                );
                // mp4a is an MPEG-4 audio sample entry, not proof that a decoder
                // accepts its object/profile. Adapters MUST probe playback.
            }
        }
    }
    ensure!(
        info.video_tracks > 0 && !codecs.is_empty(),
        "This MP4 has no supported video track"
    );
    info.video_codecs = codecs.into_iter().collect();
    Ok(info)
}
fn brand(b: &[u8]) -> Result<()> {
    ensure!(
        b.len() >= 8 && (b.len() - 8) % 4 == 0,
        "Invalid MP4 file-type brands"
    );
    let known = |v: &[u8]| {
        matches!(
            v,
            b"isom"
                | b"iso2"
                | b"iso3"
                | b"iso4"
                | b"iso5"
                | b"iso6"
                | b"iso7"
                | b"iso8"
                | b"iso9"
                | b"mp41"
                | b"mp42"
                | b"avc1"
                | b"av01"
                | b"M4V "
                | b"dash"
                | b"cmfc"
                | b"cmfv"
        )
    };
    ensure!(
        known(&b[..4]) || b[8..].chunks_exact(4).any(known),
        "Container is not a supported MP4 brand; native game MOV is not accepted"
    );
    Ok(())
}
/// Pass a retained, ordinary-file handle opened without sharing writes/deletes.
/// Import adapters must reject source/ancestor reparse points, inspect before
/// copy AND inspect the completed copy before no-clobber library publication.
pub(crate) fn inspect(input: &mut (impl Read + Seek), extension: &str) -> Result<Metadata> {
    ensure!(
        extension.eq_ignore_ascii_case("mp4"),
        "Choose .mp4; native game .mov files are not supported"
    );
    let length = input.seek(SeekFrom::End(0))?;
    ensure!(
        (24..=MAX_BYTES).contains(&length),
        "Choose a regular MP4 no larger than 2 GiB"
    );
    let (mut at, mut count, mut branded, mut parsed, mut media) =
        (0u64, 0usize, false, None, false);
    while at < length {
        ensure!(
            count < MAX_BOXES && length - at >= 8,
            "Invalid or excessive MP4 container boxes"
        );
        count += 1;
        input.seek(SeekFrom::Start(at))?;
        let mut header = [0; 16];
        input.read_exact(&mut header[..8])?;
        let small = be32(&header, 0);
        let (size, head) = match small {
            0 => (length - at, 8u64),
            1 => {
                ensure!(length - at >= 16, "Truncated extended MP4 container");
                input.read_exact(&mut header[8..16])?;
                (be64(&header, 8), 16)
            }
            n => (u64::from(n), 8),
        };
        ensure!(
            size >= head && size <= length - at,
            "Invalid MP4 container size"
        );
        match &header[4..8] {
            b"ftyp" => {
                ensure!(
                    !branded && size - head <= 65536,
                    "Duplicate/oversized MP4 file-type metadata"
                );
                let mut payload = vec![0; usize::try_from(size - head)?];
                input.read_exact(&mut payload)?;
                brand(&payload)?;
                branded = true;
            }
            b"moov" => {
                ensure!(
                    parsed.is_none() && size <= MAX_METADATA,
                    "Duplicate/oversized MP4 movie metadata"
                );
                let mut payload = vec![0; usize::try_from(size - head)?];
                input
                    .read_exact(&mut payload)
                    .context("Truncated MP4 movie metadata")?;
                parsed = Some(movie(&payload, length)?);
            }
            b"mdat" => {
                ensure!(size > head, "MP4 media payload is empty");
                media = true;
            }
            _ => {}
        }
        at += size;
    }
    ensure!(
        branded && media && input.seek(SeekFrom::End(0))? == length,
        "MP4 is incomplete or changed while being inspected"
    );
    parsed.context("MP4 movie metadata is missing")
}

#[cfg(test)]
#[path = "startup_mp4_tests.rs"]
pub(crate) mod tests;
