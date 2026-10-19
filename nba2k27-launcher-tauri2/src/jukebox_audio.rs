//! Bounded Rust implementation of the WPF Jukebox's proven audio containers.
//! No codec subprocess, game process, registry, volume or filesystem access.
#![allow(dead_code)]
use anyhow::{Result, ensure};

pub(crate) const MAX_AUDIO: usize = 256 * 1024 * 1024;
pub(crate) const MAX_FRAMES: u32 = 57_600_000; // twenty minutes, stereo 48 kHz
pub(crate) const MAGIC: u32 = 0xbbcfa17e;
pub(crate) const PCM: u32 = 0x063543bd;
pub(crate) const OPUS: u32 = 0x758195f9;
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn put(b: &mut [u8], i: usize, v: u32) {
    b[i..i + 4].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn wave_pcm(wave: &[u8]) -> Result<&[u8]> {
    ensure!(
        (44..=MAX_AUDIO).contains(&wave.len())
            && &wave[..4] == b"RIFF"
            && &wave[8..12] == b"WAVE"
            && u32_at(wave, 4) as usize == wave.len() - 8,
        "Use a complete stereo 48 kHz PCM16 WAV (at most twenty minutes)"
    );
    let (mut fmt, mut pcm, mut cursor) = (None, None, 12usize);
    while cursor < wave.len() {
        ensure!(cursor + 8 <= wave.len(), "Truncated WAV chunk");
        let size = u32_at(wave, cursor + 4) as usize;
        let end = cursor
            .checked_add(8)
            .and_then(|v| v.checked_add(size))
            .ok_or_else(|| anyhow::anyhow!("WAV size overflow"))?;
        ensure!(
            end.checked_add(size & 1).is_some_and(|v| v <= wave.len()),
            "Truncated WAV payload"
        );
        let part = &wave[cursor + 8..end];
        if &wave[cursor..cursor + 4] == b"fmt " {
            ensure!(fmt.replace(part).is_none(), "Duplicate WAV format");
        }
        if &wave[cursor..cursor + 4] == b"data" {
            ensure!(pcm.replace(part).is_none(), "Duplicate WAV audio");
        }
        cursor = end + (size & 1);
    }
    let fmt = fmt.ok_or_else(|| anyhow::anyhow!("Missing WAV format"))?;
    let pcm = pcm.ok_or_else(|| anyhow::anyhow!("Missing WAV audio"))?;
    ensure!(
        fmt.len() >= 16
            && u16_at(fmt, 0) == 1
            && u16_at(fmt, 2) == 2
            && u32_at(fmt, 4) == 48000
            && u32_at(fmt, 8) == 192000
            && u16_at(fmt, 12) == 4
            && u16_at(fmt, 14) == 16
            && !pcm.is_empty()
            && pcm.len() % 4 == 0
            && pcm.len() / 4 <= MAX_FRAMES as usize,
        "Use stereo 48 kHz PCM16 audio under twenty minutes"
    );
    Ok(pcm)
}
pub(crate) fn pcm_wave(pcm: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !pcm.is_empty() && pcm.len() % 4 == 0 && pcm.len() / 4 <= MAX_FRAMES as usize,
        "Invalid stereo PCM16 length"
    );
    let mut out = vec![0; 44];
    out[..4].copy_from_slice(b"RIFF");
    put(&mut out, 4, (36 + pcm.len()) as u32);
    out[8..16].copy_from_slice(b"WAVEfmt ");
    put(&mut out, 16, 16);
    out[20..24].copy_from_slice(&[1, 0, 2, 0]);
    put(&mut out, 24, 48000);
    put(&mut out, 28, 192000);
    out[32..36].copy_from_slice(&[4, 0, 16, 0]);
    out[36..40].copy_from_slice(b"data");
    put(&mut out, 40, pcm.len() as u32);
    out.extend_from_slice(pcm);
    Ok(out)
}
/// The library WAV is unchanged; this creates a distinct game-facing container.
pub(crate) fn wrap(wave: &[u8]) -> Result<Vec<u8>> {
    if wave.len() >= 12 && u32_at(wave, 0) == MAGIC {
        inspect(wave)?;
        return Ok(wave.to_vec());
    }
    let pcm = wave_pcm(wave)?;
    native_pcm(pcm)
}
/// Avoids an extra full-size intermediate WAV when the native decoder already
/// produced exact raw PCM. Input is immutable and never takes library ownership.
pub(crate) fn native_pcm(pcm: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        !pcm.is_empty() && pcm.len() % 4 == 0 && pcm.len() / 4 <= MAX_FRAMES as usize,
        "Invalid native PCM source length"
    );
    let mut out = vec![0; 54];
    for (i, v) in [
        MAGIC,
        46,
        PCM,
        2,
        48000,
        (pcm.len() / 2) as u32,
        18,
        pcm.len() as u32,
        4,
    ]
    .into_iter()
    .enumerate()
    {
        put(&mut out, i * 4, v);
    }
    out[36..52].copy_from_slice(&[1, 0, 2, 0, 0x80, 0xbb, 0, 0, 0, 0xee, 2, 0, 4, 0, 16, 0]);
    out.extend_from_slice(pcm);
    inspect(&out)?;
    Ok(out)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Codec {
    Pcm,
    Opus,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AudioInfo {
    pub codec: Codec,
    pub frames: u32,
    pub bytes: usize,
}
pub(crate) fn inspect(bytes: &[u8]) -> Result<AudioInfo> {
    ensure!(
        (54..=MAX_AUDIO).contains(&bytes.len()) && u32_at(bytes, 0) == MAGIC,
        "Invalid native audio container"
    );
    match u32_at(bytes, 8) {
        PCM => {
            ensure!(
                u32_at(bytes, 4) == 46
                    && u32_at(bytes, 12) == 2
                    && u32_at(bytes, 16) == 48000
                    && u32_at(bytes, 24) == 18
                    && u32_at(bytes, 28) as usize == bytes.len() - 54
                    && u32_at(bytes, 32) == 4
                    && u16_at(bytes, 36) == 1
                    && u16_at(bytes, 38) == 2
                    && u32_at(bytes, 40) == 48000
                    && u32_at(bytes, 44) == 192000
                    && u16_at(bytes, 48) == 4
                    && u16_at(bytes, 50) == 16
                    && u16_at(bytes, 52) == 0,
                "Invalid native PCM descriptor"
            );
            let payload = bytes.len() - 54;
            ensure!(
                payload > 0
                    && payload % 4 == 0
                    && payload / 4 <= MAX_FRAMES as usize
                    && u32_at(bytes, 20) as usize == payload / 2,
                "Invalid native PCM duration"
            );
            Ok(AudioInfo {
                codec: Codec::Pcm,
                frames: (payload / 4) as u32,
                bytes: bytes.len(),
            })
        }
        OPUS => {
            let opus = parse_opus(bytes)?;
            Ok(AudioInfo {
                codec: Codec::Opus,
                frames: opus.samples,
                bytes: bytes.len(),
            })
        }
        _ => anyhow::bail!("Unsupported native audio codec"),
    }
}
fn frames(packet: &[u8]) -> Result<u32> {
    ensure!(
        !packet.is_empty()
            && packet.len() <= 1275
            && packet[0] & 4 != 0
            && packet[0] & 3 == 0
            && packet[0] >> 3 >= 16,
        "Unsupported stereo CELT packet"
    );
    Ok([120, 240, 480, 960][((packet[0] >> 3) & 3) as usize])
}
pub(crate) struct Opus<'a> {
    pub packets: Vec<&'a [u8]>,
    pub samples: u32,
    pub total: u32,
}
pub(crate) fn parse_opus(bytes: &[u8]) -> Result<Opus<'_>> {
    ensure!(
        (52..=80 * 1024 * 1024).contains(&bytes.len())
            && u32_at(bytes, 0) == MAGIC
            && u32_at(bytes, 8) == OPUS
            && u32_at(bytes, 12) == 2
            && u32_at(bytes, 16) == 48000,
        "Invalid native stereo Opus container"
    );
    let count = u32_at(bytes, 36)
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Seek table overflow"))? as usize;
    ensure!(
        (1..=60000).contains(&count)
            && u32_at(bytes, 4) as usize == 28 + count * 8
            && u32_at(bytes, 24) as usize == count * 8
            && u32_at(bytes, 32) == 2048,
        "Invalid Opus seek descriptor"
    );
    let start = 36 + count * 8;
    ensure!(
        start <= bytes.len() && u32_at(bytes, 28) as usize == bytes.len() - start,
        "Invalid Opus payload bounds"
    );
    let (mut cursor, mut total, mut packets) = (start, 0u32, Vec::new());
    while cursor < bytes.len() {
        let i = packets.len();
        ensure!(
            i < count && cursor + 2 <= bytes.len(),
            "Truncated Opus packets"
        );
        let time = f32::from_le_bytes(bytes[40 + i * 8..44 + i * 8].try_into()?);
        ensure!(
            (i == 0 || u32_at(bytes, 36 + i * 8) as usize == cursor - start)
                && time.is_finite()
                && (f64::from(time) - f64::from(total) / 48000.0).abs() <= 0.0001,
            "Opus seek entry differs from payload"
        );
        let n = u16_at(bytes, cursor) as usize;
        cursor += 2;
        ensure!(
            (1..=1275).contains(&n) && cursor + n <= bytes.len(),
            "Invalid Opus packet length"
        );
        let packet = &bytes[cursor..cursor + n];
        total = total
            .checked_add(frames(packet)?)
            .ok_or_else(|| anyhow::anyhow!("Opus duration overflow"))?;
        ensure!(
            total <= MAX_FRAMES + 960,
            "Opus audio exceeds twenty minutes"
        );
        packets.push(packet);
        cursor += n;
    }
    if count == packets.len() + 1 {
        let i = packets.len();
        let time = f32::from_le_bytes(bytes[40 + i * 8..44 + i * 8].try_into()?);
        ensure!(
            total % 960 == 0
                && u32_at(bytes, 36 + i * 8) as usize == bytes.len() - start
                && time.is_finite()
                && (f64::from(time) - f64::from(total) / 48000.0).abs() <= 0.0001,
            "Invalid terminal Opus seek entry"
        );
    } else {
        ensure!(count == packets.len(), "Truncated Opus seek table");
    }
    let scalar = u32_at(bytes, 20);
    let samples = scalar / 2;
    ensure!(
        scalar % 2 == 0
            && samples > 0
            && samples <= MAX_FRAMES
            && samples <= total
            && total - samples <= 960,
        "Invalid Opus sample count/padding"
    );
    Ok(Opus {
        packets,
        samples,
        total,
    })
}
pub(crate) fn build_opus(packets: &[Vec<u8>], samples: u32) -> Result<Vec<u8>> {
    ensure!(
        !packets.is_empty() && packets.len() < 60000 && samples > 0 && samples <= MAX_FRAMES,
        "Invalid Opus source count/duration"
    );
    let (mut entries, mut payload, mut total) = (Vec::new(), Vec::new(), 0u32);
    for packet in packets {
        entries.push((payload.len() as u32, total));
        total += frames(packet)?;
        payload.extend_from_slice(&(packet.len() as u16).to_le_bytes());
        payload.extend_from_slice(packet);
    }
    if total % 960 == 0 {
        entries.push((payload.len() as u32, total));
    }
    ensure!(
        entries.len() <= 60000 && total >= samples && total - samples <= 960,
        "Unexpected Opus padding"
    );
    let mut out = vec![0; 36 + entries.len() * 8];
    for (i, v) in [
        MAGIC,
        (28 + entries.len() * 8) as u32,
        OPUS,
        2,
        48000,
        samples * 2,
        (entries.len() * 8) as u32,
        payload.len() as u32,
        2048,
    ]
    .into_iter()
    .enumerate()
    {
        put(&mut out, i * 4, v);
    }
    for (i, (offset, frame)) in entries.iter().enumerate() {
        put(
            &mut out,
            36 + i * 8,
            if i == 0 {
                (entries.len() - 1) as u32
            } else {
                *offset
            },
        );
        out[40 + i * 8..44 + i * 8].copy_from_slice(&(*frame as f32 / 48000.0).to_le_bytes());
    }
    out.extend_from_slice(&payload);
    parse_opus(&out)?;
    Ok(out)
}
fn page(packet: &[u8], seq: u32, granule: u64, flags: u8) -> Vec<u8> {
    let count = packet.len() / 255 + 1;
    let mut out = vec![0; 27 + count];
    out[..4].copy_from_slice(b"OggS");
    out[5] = flags;
    out[6..14].copy_from_slice(&granule.to_le_bytes());
    put(&mut out, 14, 1);
    put(&mut out, 18, seq);
    out[26] = count as u8;
    for i in 0..count {
        out[27 + i] = if i + 1 < count {
            255
        } else {
            (packet.len() % 255) as u8
        };
    }
    out.extend_from_slice(packet);
    let mut crc = 0u32;
    for b in &out {
        crc ^= u32::from(*b) << 24;
        for _ in 0..8 {
            crc = (crc << 1) ^ if crc & 0x80000000 != 0 { 0x04c11db7 } else { 0 };
        }
    }
    put(&mut out, 22, crc);
    out
}
/// Remuxes proven native Opus to browser-playable Ogg without re-encoding.
pub(crate) fn preview_ogg(native: &[u8], pre_skip: u16) -> Result<Vec<u8>> {
    let opus = parse_opus(native)?;
    ensure!(
        opus.samples + u32::from(pre_skip) <= opus.total,
        "Insufficient Opus decoder padding"
    );
    let mut head = vec![0; 19];
    head[..8].copy_from_slice(b"OpusHead");
    head[8] = 1;
    head[9] = 2;
    head[10..12].copy_from_slice(&pre_skip.to_le_bytes());
    put(&mut head, 12, 48000);
    let mut tags = vec![0; 16];
    tags[..8].copy_from_slice(b"OpusTags");
    let mut out = page(&head, 0, 0, 2);
    out.extend(page(&tags, 1, 0, 0));
    let mut decoded = 0u32;
    for (i, p) in opus.packets.iter().enumerate() {
        decoded += frames(p)?;
        let last = i + 1 == opus.packets.len();
        out.extend(page(
            p,
            (i + 2) as u32,
            u64::from(if last {
                opus.samples + u32::from(pre_skip)
            } else {
                decoded
            }),
            if last { 4 } else { 0 },
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_round_trip_keeps_library_audio() {
        let source = pcm_wave(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let before = source.clone();
        let native = wrap(&source).unwrap();
        assert_eq!(source, before);
        assert_eq!(inspect(&native).unwrap().frames, 2);
        assert_eq!(&native[54..], &source[44..]);
        assert_eq!(wrap(&native).unwrap(), native);
    }
    #[test]
    fn wav_rejects_rate_stereo_alignment_duplicates_and_truncation() {
        let source = pcm_wave(&[0; 8]).unwrap();
        for (at, value) in [(22, 1u8), (24, 0), (32, 2), (34, 24)] {
            let mut b = source.clone();
            b[at] = value;
            assert!(wrap(&b).is_err());
        }
        let mut b = source.clone();
        b.pop();
        assert!(wrap(&b).is_err());
        let mut b = source.clone();
        b.extend_from_slice(&source[12..36]);
        let len = b.len();
        put(&mut b, 4, (len - 8) as u32);
        assert!(wrap(&b).is_err());
    }
    #[test]
    fn opus_build_parse_and_preview() {
        let packets = vec![vec![0x9c, 1, 2], vec![0x9c, 3, 4]];
        let native = build_opus(&packets, 1910).unwrap();
        let parsed = parse_opus(&native).unwrap();
        assert_eq!(parsed.samples, 1910);
        assert_eq!(parsed.total, 1920);
        let ogg = preview_ogg(&native, 0).unwrap();
        assert_eq!(&ogg[..4], b"OggS");
        assert!(ogg.windows(8).any(|w| w == b"OpusHead"));
        assert!(preview_ogg(&native, 11).is_err());
    }
    #[test]
    fn opus_rejects_nan_seek_and_invalid_packets() {
        let native = build_opus(&[vec![0x9c, 1, 2]], 960).unwrap();
        for at in [0, 4, 12, 24, 28, 32, 36] {
            let mut b = native.clone();
            b[at] ^= 0xff;
            assert!(parse_opus(&b).is_err());
        }
        let mut b = native.clone();
        b[40..44].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse_opus(&b).is_err());
        assert!(build_opus(&[vec![0x98]], 960).is_err());
    }
}
