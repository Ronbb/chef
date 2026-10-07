//! Private synthesized media shared by auditions and course tasks. No public asset registration.
use crate::{AppError, voice_references::hex};
use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn store(
    root: &std::path::Path,
    s: crate::qwen::Speech,
    cloned: bool,
) -> Result<Value, AppError> {
    let info = crate::audio::inspect(&s.wav, "audio/wav").map_err(|_| AppError::Unavailable)?;
    if s.provider_wav.len() < 44
        || !s.provider_wav.starts_with(b"RIFF")
        || &s.provider_wav[8..12] != b"WAVE"
        || s.provider_wav.len() > 16 * 1024 * 1024
        || s.wav.len() > 16 * 1024 * 1024
        || info.sample_rate != 24000
        || info.channels != 1
        || info.duration_ms > 180000
        || info != s.info
        || !crate::qwen::valid_id(&s.request_id)
        || s.verification.is_some() != cloned
        || s.verification.as_ref().is_some_and(|v| {
            v.model != crate::qwen::MODEL
                || v.status != "OK"
                || !crate::qwen::valid_id(&v.request_id)
        })
    {
        return Err(AppError::Unavailable);
    }
    if crate::qwen::normalize_wave(&s.provider_wav).map_err(|_| AppError::Unavailable)? != s.wav {
        return Err(AppError::Unavailable);
    }
    let sha = crate::media::digest(&s.wav);
    let provider_sha = crate::media::digest(&s.provider_wav);
    crate::media::store_file(root, &s.provider_wav, &provider_sha, "wav")
        .map_err(|_| AppError::Unavailable)?;
    crate::media::store_file(root, &s.wav, &sha, "wav").map_err(|_| AppError::Unavailable)?;
    let verification = s
        .verification
        .map(|v| json!({"model":v.model,"status":v.status,"requestId":v.request_id}));
    Ok(
        json!({"sha256":sha,"providerSha256":provider_sha,"byteLength":s.wav.len(),"durationMs":info.duration_ms,"requestId":s.request_id,"inputTokens":s.input_tokens,"outputTokens":s.output_tokens,"verification":verification,"postprocessing":"qwen-riff-length-v1-metadata-preserved"}),
    )
}
/// Validate both original and normalized objects before serving or reusing paid output.
pub(crate) fn read(root: &Path, result: &Value) -> Result<(String, Vec<u8>), AppError> {
    let sha = result["sha256"]
        .as_str()
        .filter(|s| hex(s, 64))
        .ok_or(AppError::Unavailable)?;
    let original = result["providerSha256"]
        .as_str()
        .filter(|s| hex(s, 64))
        .ok_or(AppError::Unavailable)?;
    let duration = result["durationMs"].as_u64().ok_or(AppError::Unavailable)?;
    let length = result["byteLength"].as_u64().ok_or(AppError::Unavailable)?;
    let bytes = crate::media::stored_bytes(root, sha, "wav").map_err(|_| AppError::Unavailable)?;
    let raw =
        crate::media::stored_bytes(root, original, "wav").map_err(|_| AppError::Unavailable)?;
    let info = crate::audio::inspect(&bytes, "audio/wav").map_err(|_| AppError::Unavailable)?;
    if crate::media::digest(&bytes) != sha
        || crate::media::digest(&raw) != original
        || bytes.len() as u64 != length
        || info.duration_ms as u64 != duration
        || info.channels != 1
        || info.sample_rate != 24000
        || crate::qwen::normalize_wave(&raw).map_err(|_| AppError::Unavailable)? != bytes
    {
        return Err(AppError::Unavailable);
    }
    Ok((sha.into(), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(std::path::PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "brioche-speech-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn output(stream: bool) -> crate::qwen::Speech {
        let mut raw = Vec::new();
        raw.extend(b"RIFF");
        raw.extend(0u32.to_le_bytes());
        raw.extend(b"WAVEfmt ");
        raw.extend(16u32.to_le_bytes());
        raw.extend([1, 0, 1, 0]);
        raw.extend(24000u32.to_le_bytes());
        raw.extend(48000u32.to_le_bytes());
        raw.extend([2, 0, 16, 0]);
        raw.extend(b"AIGC");
        raw.extend(4u32.to_le_bytes());
        raw.extend(b"tag!");
        raw.extend(b"data");
        raw.extend(4800u32.to_le_bytes());
        raw.extend(vec![42; 4800]);
        let length = (raw.len() - 8) as u32;
        raw[4..8].copy_from_slice(&(if stream { 2_147_483_583u32 } else { length }).to_le_bytes());
        if stream {
            raw[52..56].copy_from_slice(&2_147_483_315u32.to_le_bytes());
        }
        let wav = crate::qwen::normalize_wave(&raw).unwrap();
        let info = crate::audio::inspect(&wav, "audio/wav").unwrap();
        crate::qwen::Speech {
            provider_wav: raw,
            wav,
            info,
            request_id: "test-speech".into(),
            verification: None,
            input_tokens: Some(7),
            output_tokens: Some(9),
        }
    }
    #[test]
    fn preserved_original_and_normalized_output_are_both_required_for_reuse() {
        let dir = Directory::new();
        let speech = output(true);
        let raw = speech.provider_wav.clone();
        let wav = speech.wav.clone();
        let result = store(&dir.0, speech, false).unwrap();
        assert_ne!(result["sha256"], result["providerSha256"]);
        let (sha, bytes) = read(&dir.0, &result).unwrap();
        assert_eq!(bytes, wav);
        assert_eq!(&bytes[44..48], b"tag!");
        let original = result["providerSha256"].as_str().unwrap();
        assert_eq!(
            crate::media::stored_bytes(&dir.0, original, "wav").unwrap(),
            raw
        );
        for field in ["sha256", "providerSha256"] {
            let mut bad = result.clone();
            bad[field] = json!("../not-a-file");
            assert!(read(&dir.0, &bad).is_err());
        }
        for field in ["durationMs", "byteLength"] {
            let mut bad = result.clone();
            bad[field] = json!(1);
            assert!(read(&dir.0, &bad).is_err());
        }
        let mut different = output(false);
        different.provider_wav[60] = 1;
        different.wav = different.provider_wav.clone();
        let other = store(&dir.0, different, false).unwrap();
        let mut mismatched = result.clone();
        mismatched["providerSha256"] = other["providerSha256"].clone();
        assert!(read(&dir.0, &mismatched).is_err());
        let corrupt = vec![0; wav.len()];
        std::fs::write(dir.0.join(format!("{sha}.wav")), corrupt).unwrap();
        assert!(read(&dir.0, &result).is_err());
    }
    #[test]
    fn unverified_clone_and_unrelated_repaired_audio_are_not_stored() {
        let dir = Directory::new();
        assert!(store(&dir.0, output(false), true).is_err());
        let mut speech = output(true);
        speech.wav[60] = 1;
        assert!(store(&dir.0, speech, false).is_err());
        let mut speech = output(false);
        speech.info.duration_ms += 1;
        assert!(store(&dir.0, speech, false).is_err());
        let mut speech = output(false);
        speech.request_id = "invalid/request".into();
        assert!(store(&dir.0, speech, false).is_err());
        let mut speech = output(false);
        speech.verification = Some(crate::qwen::Details {
            model: crate::qwen::MODEL.into(),
            status: "OK".into(),
            request_id: "query-test".into(),
        });
        assert!(store(&dir.0, speech, false).is_err());
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
    }
}
