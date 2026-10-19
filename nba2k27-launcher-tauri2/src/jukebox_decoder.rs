//! Native Windows 8+ Media Foundation audio decoding/resampling. Run this in a
//! bounded, owned decode-helper process, never the launcher UI thread/process:
//! codec parsing is native code and an in-flight ReadSample cannot be cancelled
//! safely by killing a Rust thread. No output files, URLs or subprocesses here.
//! https://learn.microsoft.com/windows/win32/api/mfreadwrite/nf-mfreadwrite-imfsourcereader-setcurrentmediatype
#![allow(dead_code)]
use crate::jukebox_audio;
use anyhow::{Context, Result, ensure};
use std::time::{Duration, Instant};
use windows::Win32::{
    Media::MediaFoundation::*,
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
    UI::Shell::SHCreateMemStream,
};
struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
struct Foundation;
impl Drop for Foundation {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}
struct Locked(IMFMediaBuffer);
impl Drop for Locked {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Unlock();
        }
    }
}
fn verify(reader: &IMFSourceReader, stream: u32) -> Result<()> {
    let media = unsafe { reader.GetCurrentMediaType(stream)? };
    unsafe {
        ensure!(
            media.GetGUID(&MF_MT_MAJOR_TYPE)? == MFMediaType_Audio
                && media.GetGUID(&MF_MT_SUBTYPE)? == MFAudioFormat_PCM
                && media.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? == 2
                && media.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)? == 48000
                && media.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)? == 16
                && media.GetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT)? == 4
                && media.GetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND)? == 192000,
            "Windows decoder did not negotiate stereo 48kHz PCM16"
        );
    }
    Ok(())
}
/// Exact output is raw stereo48k PCM16, ready for pcm_wave()/wrap(). Windows
/// supplies MP3/AAC/WAV/FLAC decoding; Ogg depends on installed Windows codecs
/// and fails clearly if unavailable. The caller preserves every source file.
pub(crate) fn decode_pcm(
    input: &[u8],
    mut check: impl FnMut() -> Result<()>,
    mut progress: impl FnMut(u32) -> Result<()>,
) -> Result<Vec<u8>> {
    ensure!(
        !input.is_empty() && input.len() <= jukebox_audio::MAX_AUDIO,
        "Compressed song exceeds the bounded decoder input limit"
    );
    check()?;
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .context("Audio decoder requires its dedicated MTA helper thread")?;
    }
    let _com = Com;
    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET)?;
    }
    let _foundation = Foundation;
    // Byte stream only: malformed playlist metadata can never cause a URL or
    // arbitrary network/file path to be opened by Media Foundation.
    let stream = unsafe { SHCreateMemStream(Some(input)) }
        .context("Unable to allocate bounded audio stream")?;
    let stream = unsafe { MFCreateMFByteStreamOnStream(&stream)? };
    let reader = unsafe { MFCreateSourceReaderFromByteStream(&stream, None::<&IMFAttributes>) }
        .context("Windows cannot decode this audio format; source retained")?;
    let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
    unsafe {
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(audio, true)?;
    }
    let output = unsafe { MFCreateMediaType()? };
    unsafe {
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, 48000)?;
        output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        output.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
        output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192000)?;
        reader
            .SetCurrentMediaType(audio, None, &output)
            .context("Windows audio conversion to stereo48k PCM16 is unavailable")?;
    }
    verify(&reader, audio)?;
    let (mut pcm, mut empty) = (Vec::new(), 0usize);
    let start = Instant::now();
    for _ in 0..1_000_000 {
        check()?;
        ensure!(
            start.elapsed() < Duration::from_secs(120),
            "Audio conversion timed out; source retained"
        );
        let (mut flags, mut sample) = (0u32, None);
        unsafe {
            reader.ReadSample(audio, 0, None, Some(&mut flags), None, Some(&mut sample))?;
        }
        ensure!(
            flags & MF_SOURCE_READERF_ERROR.0 as u32 == 0,
            "Windows reported an audio decode error"
        );
        if flags
            & (MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32
                | MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED.0 as u32)
            != 0
        {
            verify(&reader, audio)?;
        }
        if let Some(sample) = sample {
            let buffer = unsafe { sample.ConvertToContiguousBuffer()? };
            let (mut pointer, mut max, mut length) = (std::ptr::null_mut(), 0u32, 0u32);
            unsafe {
                buffer.Lock(&mut pointer, Some(&mut max), Some(&mut length))?;
            }
            let lock = Locked(buffer);
            ensure!(
                length <= max
                    && length <= 16 * 1024 * 1024
                    && (length == 0 || !pointer.is_null())
                    && length % 4 == 0,
                "Windows returned an invalid audio buffer"
            );
            ensure!(
                pcm.len()
                    .checked_add(length as usize)
                    .is_some_and(|n| n <= jukebox_audio::MAX_FRAMES as usize * 4),
                "Song exceeds twenty minutes"
            );
            if length > 0 {
                pcm.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(pointer, length as usize)
                });
                empty = 0;
            } else {
                empty += 1;
            }
            drop(lock);
            progress((pcm.len() / 4) as u32)?;
        } else {
            empty += 1;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            ensure!(!pcm.is_empty(), "Decoded song has no audio");
            check()?;
            return Ok(pcm);
        }
        ensure!(empty <= 1000, "Audio decoder made no progress");
    }
    anyhow::bail!("Audio decode exceeded the sample-operation bound")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_windows_decode_stereo_wav_keeps_source_unchanged() {
        let source = jukebox_audio::pcm_wave(&[0; 1920]).unwrap();
        let before = source.clone();
        let pcm = decode_pcm(&source, || Ok(()), |_| Ok(())).unwrap();
        assert_eq!(pcm, source[44..]);
        assert_eq!(source, before);
    }
    #[test]
    fn actual_windows_resamples_mono_44100_to_stereo_48000() {
        let payload = vec![0u8; 8820];
        let mut wave = vec![0u8; 44];
        wave[..4].copy_from_slice(b"RIFF");
        wave[4..8].copy_from_slice(&(36 + payload.len() as u32).to_le_bytes());
        wave[8..16].copy_from_slice(b"WAVEfmt ");
        wave[16..20].copy_from_slice(&16u32.to_le_bytes());
        wave[20..24].copy_from_slice(&[1, 0, 1, 0]);
        wave[24..28].copy_from_slice(&44100u32.to_le_bytes());
        wave[28..32].copy_from_slice(&88200u32.to_le_bytes());
        wave[32..36].copy_from_slice(&[2, 0, 16, 0]);
        wave[36..40].copy_from_slice(b"data");
        wave[40..44].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        wave.extend(payload);
        let pcm = decode_pcm(&wave, || Ok(()), |_| Ok(())).unwrap();
        assert!((4790..=4810).contains(&(pcm.len() / 4)));
    }
    #[test]
    fn cancellation_occurs_before_native_codec_activation() {
        assert!(
            decode_pcm(b"invalid", || anyhow::bail!("cancelled"), |_| Ok(()))
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }
}
