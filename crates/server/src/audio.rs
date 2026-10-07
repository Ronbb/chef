//! Bounded recording inspection. Call from a blocking worker, never an async executor.
use anyhow::{Context, Result, ensure};
use std::{io::Cursor, path::Path};
use symphonia::core::{
    codecs::audio::{AudioDecoderOptions, well_known::CODEC_ID_MP3},
    common::Limit,
    formats::{TrackType, probe::Hint},
    io::MediaSourceStream,
    meta::MetadataOptions,
};

pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_SECONDS: u64 = 1800;

#[derive(Debug, PartialEq, Eq)]
pub struct RecordingInfo {
    pub duration_ms: u32,
    pub sample_rate: u32,
    pub channels: u32,
}

pub fn extension(mime: &str) -> Result<&'static str> {
    match mime {
        "audio/mpeg" => Ok("mp3"),
        "audio/wav" => Ok("wav"),
        _ => anyhow::bail!("unsupported recording MIME type"),
    }
}

/// Reads at most 32 MiB + one byte, including files whose size changes during the read.
pub fn inspect_file(path: &Path, mime: &str) -> Result<(Vec<u8>, RecordingInfo)> {
    use std::io::Read;
    let file = std::fs::File::open(path).context("recording file unavailable")?;
    ensure!(
        file.metadata()?.is_file(),
        "recording must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    let info = inspect(&bytes, mime)?;
    Ok((bytes, info))
}

pub fn inspect(bytes: &[u8], mime: &str) -> Result<RecordingInfo> {
    ensure!(
        !bytes.is_empty() && bytes.len() <= MAX_BYTES,
        "recording size must be 1..32 MiB"
    );
    let ext = extension(mime)?;
    // Demuxers may treat a truncated final packet as EOF. Check complete framing first.
    match ext {
        "wav" => validate_wave(bytes)?,
        "mp3" => validate_mp3(bytes)?,
        _ => unreachable!(),
    }
    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let metadata = MetadataOptions::default()
        .limit_tag_bytes(Limit::Maximum(64 * 1024))
        .limit_visual_bytes(Limit::Maximum(0));
    let mut format = symphonia::default::get_probe()
        .probe(&hint, stream, Default::default(), metadata)
        .context("recording container cannot be decoded")?;
    ensure!(
        format.tracks().len() == 1,
        "recording requires exactly one track"
    );
    let track = format
        .default_track(TrackType::Audio)
        .context("recording has no audio track")?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .context("recording has no audio parameters")?;
    ensure!(
        ext != "mp3" || params.codec == CODEC_ID_MP3,
        "MIME does not match MP3 codec"
    );
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default().verify(true))?;
    let mut frames = 0u64;
    let mut spec = None;
    let mut packets = 0u32;
    while let Some(packet) = format
        .next_packet()
        .context("recording packet is damaged")?
    {
        packets += 1;
        ensure!(
            packets <= 200_000 && packet.track_id == track_id,
            "recording packet limit or track mismatch"
        );
        let decoded = decoder
            .decode(&packet)
            .context("recording audio is damaged")?;
        let current = (
            decoded.spec().rate(),
            decoded.spec().channels().count() as u32,
        );
        ensure!(
            (8000..=96000).contains(&current.0) && (1..=2).contains(&current.1),
            "recording requires 8–96 kHz mono or stereo"
        );
        ensure!(
            spec.is_none_or(|previous| previous == current),
            "recording format changes mid-stream"
        );
        spec = Some(current);
        frames = frames
            .checked_add(decoded.frames() as u64)
            .context("recording frame overflow")?;
        ensure!(
            frames <= u64::from(current.0) * MAX_SECONDS,
            "recording exceeds 30 minutes"
        );
    }
    ensure!(
        decoder.finalize().verify_ok != Some(false),
        "recording checksum failed"
    );
    let (sample_rate, channels) = spec.context("recording has no decoded samples")?;
    ensure!(frames > 0, "recording has no decoded samples");
    // Round up so the final partial millisecond remains addressable by time cues.
    let duration_ms = (frames * 1000).div_ceil(u64::from(sample_rate)) as u32;
    Ok(RecordingInfo {
        duration_ms,
        sample_rate,
        channels,
    })
}

fn validate_wave(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
        "recording is not RIFF WAVE"
    );
    let read_u32 =
        |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
    ensure!(read_u32(4) + 8 == bytes.len(), "WAVE length mismatch");
    let mut offset = 12;
    let (mut format, mut data) = (None, None);
    while offset < bytes.len() {
        ensure!(bytes.len() - offset >= 8, "truncated WAVE chunk");
        let size = read_u32(offset + 4);
        let start = offset + 8;
        let end = start.checked_add(size).context("WAVE chunk overflow")?;
        ensure!(end <= bytes.len(), "truncated WAVE data");
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                ensure!(format.is_none() && size >= 16, "invalid WAVE format chunk");
                let tag = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
                ensure!(tag == 1 || tag == 3, "WAVE requires PCM or IEEE float");
                let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
                let rate = read_u32(start + 4);
                let bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
                ensure!(
                    (1..=2).contains(&channels) && (8000..=96000).contains(&rate),
                    "WAVE requires 8–96 kHz mono or stereo"
                );
                ensure!(
                    if tag == 1 {
                        matches!(bits, 8 | 16 | 24 | 32)
                    } else {
                        matches!(bits, 32 | 64)
                    },
                    "unsupported WAVE sample format"
                );
                let alignment =
                    u16::from_le_bytes(bytes[start + 12..start + 14].try_into().unwrap()) as usize;
                ensure!(
                    alignment == usize::from(channels) * usize::from(bits) / 8,
                    "invalid WAVE frame alignment"
                );
                ensure!(
                    read_u32(start + 8) == rate * alignment,
                    "invalid WAVE byte rate"
                );
                format = Some(alignment);
            }
            b"data" => {
                ensure!(data.is_none() && size > 0, "invalid WAVE data chunk");
                data = Some(size);
            }
            _ => {}
        }
        offset = end + size % 2;
        ensure!(offset <= bytes.len(), "missing WAVE chunk padding");
    }
    let alignment = format.context("missing WAVE format")?;
    ensure!(
        data.context("missing WAVE data")? % alignment == 0,
        "partial WAVE frame"
    );
    Ok(())
}

fn validate_mp3(bytes: &[u8]) -> Result<()> {
    let mut offset = 0usize;
    if bytes.starts_with(b"ID3") {
        ensure!(
            bytes.len() >= 10 && matches!(bytes[3], 2..=4),
            "invalid MP3 ID3 header"
        );
        ensure!(
            bytes[6..10].iter().all(|b| b & 128 == 0),
            "invalid ID3 size"
        );
        let tag_size = bytes[6..10]
            .iter()
            .fold(0usize, |size, b| (size << 7) | usize::from(*b));
        ensure!(tag_size <= 64 * 1024, "MP3 metadata exceeds 64 KiB");
        let footer = if bytes[3] == 4 && bytes[5] & 0x10 != 0 {
            10
        } else {
            0
        };
        offset = 10 + tag_size + footer;
        ensure!(offset <= bytes.len(), "truncated MP3 metadata");
    }
    let end = if bytes.len() >= 128 && &bytes[bytes.len() - 128..bytes.len() - 125] == b"TAG" {
        bytes.len() - 128
    } else {
        bytes.len()
    };
    let mut count = 0;
    while offset < end {
        ensure!(end - offset >= 4, "truncated MP3 frame header");
        let h = &bytes[offset..offset + 4];
        let version = (h[1] >> 3) & 3;
        ensure!(
            h[0] == 255 && h[1] & 224 == 224 && version != 1 && (h[1] >> 1) & 3 == 1,
            "invalid MPEG Layer III frame"
        );
        let bitrate_index = usize::from(h[2] >> 4);
        let rate_index = usize::from((h[2] >> 2) & 3);
        ensure!(
            (1..15).contains(&bitrate_index) && rate_index < 3,
            "unsupported MP3 bitrate or sample rate"
        );
        let rates = [44100, 48000, 32000];
        let rate = rates[rate_index]
            / match version {
                3 => 1,
                2 => 2,
                _ => 4,
            };
        let bitrates = if version == 3 {
            [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ]
        } else {
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
        };
        let size = (if version == 3 { 144 } else { 72 }) * bitrates[bitrate_index] * 1000 / rate
            + usize::from((h[2] >> 1) & 1);
        ensure!(size >= 4 && size <= end - offset, "truncated MP3 frame");
        offset += size;
        count += 1;
        ensure!(count <= 200_000, "MP3 frame limit exceeded");
    }
    ensure!(count > 0, "MP3 has no audio frames");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wave(rate: u32, channels: u16, frames: u32) -> Vec<u8> {
        let size = frames * u32::from(channels) * 2;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((size + 36).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(channels) * 2).to_le_bytes());
        out.extend((channels * 2).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(size.to_le_bytes());
        out.resize(44 + size as usize, 0);
        out
    }
    #[test]
    fn measures_decoded_frames_and_rejects_bad_wave() {
        assert_eq!(
            inspect(&wave(8000, 1, 8001), "audio/wav").unwrap(),
            RecordingInfo {
                duration_ms: 1001,
                sample_rate: 8000,
                channels: 1
            }
        );
        assert_eq!(
            inspect(&wave(48000, 2, 48000), "audio/wav")
                .unwrap()
                .duration_ms,
            1000
        );
        for bytes in [
            wave(4000, 1, 4000),
            wave(8000, 3, 8000),
            wave(8000, 1, 0),
            wave(8000, 1, 8000)[..100].to_vec(),
        ] {
            assert!(inspect(&bytes, "audio/wav").is_err());
        }
        let bytes = wave(8000, 1, 8000);
        assert!(inspect(&bytes, "audio/mpeg").is_err());
        assert!(inspect(&bytes, "audio/ogg").is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(inspect(&trailing, "audio/wav").is_err());
        assert!(inspect(&vec![0; MAX_BYTES + 1], "audio/wav").is_err());
    }
    #[test]
    fn decodes_real_mp3_and_rejects_truncation_and_wrong_mime() {
        let bytes = include_bytes!("../tests/fixtures/audio/synthetic.mp3");
        let info = inspect(bytes, "audio/mpeg").unwrap();
        assert_eq!(info.sample_rate, 24000);
        assert_eq!(info.channels, 1);
        assert!((990..=1010).contains(&info.duration_ms), "{info:?}");
        assert!(inspect(&bytes[..bytes.len() - 1], "audio/mpeg").is_err());
        assert!(inspect(bytes, "audio/wav").is_err());
        let mut corrupt = bytes.to_vec();
        corrupt[0] = 0;
        assert!(inspect(&corrupt, "audio/mpeg").is_err());
    }

    #[test]
    fn rejects_duration_over_limit_and_forged_wave_headers() {
        assert!(inspect(&wave(8000, 1, 8000 * 1800 + 1), "audio/wav").is_err());
        for (offset, value) in [(22, 0), (28, 0), (32, 0), (34, 0)] {
            let mut bytes = wave(8000, 1, 8000);
            bytes[offset] = value;
            bytes[offset + 1] = 0;
            assert!(inspect(&bytes, "audio/wav").is_err(), "offset {offset}");
        }
        let mut bytes = wave(8000, 1, 8000);
        bytes[40..44].copy_from_slice(&15999u32.to_le_bytes());
        assert!(inspect(&bytes, "audio/wav").is_err());
        assert!(inspect(b"ID3\x04\0\0\x7f\x7f\x7f\x7f", "audio/mpeg").is_err());
        assert!(inspect(b"ID3\x04\0\0\0\0\0\x02", "audio/mpeg").is_err());
    }
}
