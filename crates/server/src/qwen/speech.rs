//! Bounded synthesis for trusted, fixed character profiles. No public endpoint or publication side effects.
use super::{Api, Details, MODEL, ProviderError, Transport, valid_id};
use brioche_course_contract::CharacterVoiceProfile;
use serde_json::{Value, json};
use std::time::Duration;

pub struct SpeechRequest {
    pub profile: CharacterVoiceProfile,
    pub text: String,
    pub emotion: String,
}
/// Private receipt: retains original bytes and metadata; never retains the signed download URL.
pub struct Speech {
    pub provider_wav: Vec<u8>,
    pub wav: Vec<u8>,
    pub info: crate::audio::RecordingInfo,
    pub request_id: String,
    pub verification: Option<Details>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
impl SpeechRequest {
    pub fn parameters(&self) -> Result<Value, ProviderError> {
        crate::character_voices::validate(&self.profile).map_err(|_| ProviderError::Rejected)?;
        let p = &self.profile;
        if p.provider != "qwen"
            || p.model != MODEL
            || !bounded_text(&self.text, 2400)
            || self.text.chars().count() > 600
            || !bounded_text(&self.emotion, 1000)
            || (p.voice_kind == "cloned" && !p.voice_id.starts_with(&format!("{MODEL}-")))
            || (p.voice_kind == "system"
                && !brioche_course_contract::QWEN_MULTILINGUAL_SYSTEM_VOICES
                    .iter()
                    .any(|(id, _)| *id == p.voice_id))
        {
            return Err(ProviderError::Rejected);
        }
        let direction = match p.locale.as_str() {
            "fr-FR" => {
                "Speak only the supplied French text, with clear natural French pronunciation for an A1 learner. Do not add words or read these instructions."
            }
            "yue-Hant-HK" => {
                "请用自然的香港粤语朗读提供的原文，保持粤语声调和口语节奏，适合粤语初学者。不要用普通话，不要翻译，不要添加内容或朗读这些指令。"
            }
            _ => return Err(ProviderError::Rejected),
        };
        let mut parameters = json!({"model":MODEL,"input":{
            "text":self.text,"voice":p.voice_id,"format":"wav","sample_rate":24000,
            "language_hints":["fr"],"rate":p.rate,"seed":0,"enable_aigc_tag":true,
            "instruction":format!("{direction} Character: {} Speaking style: {} Default emotion: {} Scene emotion: {}",p.personality,p.speaking_style,p.default_emotion,self.emotion)
        }});
        // The provider documents dialect selection through instruction. Do not invent
        // a language_hints enum or send French hints for Cantonese. Keep French wire
        // bytes unchanged so existing immutable generation/cache keys remain valid.
        if p.locale == "yue-Hant-HK" {
            parameters["input"]
                .as_object_mut()
                .unwrap()
                .remove("language_hints");
        }
        Ok(parameters)
    }
}
fn bounded_text(text: &str, bytes: usize) -> bool {
    !text.trim().is_empty() && text.len() <= bytes && !text.chars().any(char::is_control)
}
struct ResultReceipt {
    url: url::Url,
    request_id: String,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}
fn parse_result(value: &Value) -> Result<ResultReceipt, ProviderError> {
    if value.get("code").is_some() || value["output"]["finish_reason"] != "stop" {
        return Err(ProviderError::Unknown);
    }
    let request_id = value["request_id"]
        .as_str()
        .filter(|v| valid_id(v))
        .ok_or(ProviderError::Unknown)?;
    let source = value["output"]["audio"]["url"]
        .as_str()
        .filter(|s| s.len() <= 8192)
        .ok_or(ProviderError::Unknown)?;
    let mut url = url::Url::parse(source).map_err(|_| ProviderError::Unknown)?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str() != Some("dashscope-result-bj.oss-cn-beijing.aliyuncs.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || url.path() == "/"
    {
        return Err(ProviderError::Unknown);
    }
    // The provider sometimes returns HTTP. The same OSS object/signature is fetched through TLS only.
    url.set_scheme("https")
        .map_err(|_| ProviderError::Unknown)?;
    Ok(ResultReceipt {
        url,
        request_id: request_id.into(),
        input_tokens: value["usage"]["input_tokens"].as_u64(),
        output_tokens: value["usage"]["output_tokens"].as_u64(),
    })
}
const MAX_BYTES: usize = 16 * 1024 * 1024;
async fn fetch_audio(api: &Api, url: url::Url) -> Result<Vec<u8>, ProviderError> {
    // No API key or Cookie is forwarded to OSS. The client's redirect/retry/proxy policy is unchanged.
    let mut response = api
        .client
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|_| ProviderError::Unknown)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|n| n > MAX_BYTES as u64)
    {
        return Err(ProviderError::Unknown);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ProviderError::Unknown)? {
        if bytes.len() + chunk.len() > MAX_BYTES {
            return Err(ProviderError::Unknown);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub(super) async fn synthesize(
    api: &Api,
    request: &SpeechRequest,
) -> Result<Speech, ProviderError> {
    synthesize_with(api, request, |url| fetch_audio(api, url)).await
}
async fn synthesize_with<F, Fut>(
    api: &Api,
    request: &SpeechRequest,
    download: F,
) -> Result<Speech, ProviderError>
where
    F: FnOnce(url::Url) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, ProviderError>>,
{
    let parameters = request.parameters()?;
    let verification = if request.profile.voice_kind == "cloned" {
        let details = api.query(&request.profile.voice_id).await?;
        if details.model != MODEL || details.status != "OK" {
            return Err(ProviderError::Rejected);
        }
        Some(details)
    } else {
        None
    };
    let base = api
        .endpoint
        .strip_suffix("customization")
        .ok_or(ProviderError::Rejected)?;
    let result = api
        .call_at(
            &format!("{base}SpeechSynthesizer"),
            parameters,
            Duration::from_secs(120),
        )
        .await?;
    let receipt = parse_result(&result)?;
    let provider_wav = download(receipt.url).await?;
    let (provider_wav, wav, info) = tokio::task::spawn_blocking(move || {
        let wav = normalize_wave(&provider_wav)?;
        let info = crate::audio::inspect(&wav, "audio/wav").map_err(|_| ProviderError::Unknown)?;
        if info.sample_rate != 24000 || info.channels != 1 || info.duration_ms > 180_000 {
            return Err(ProviderError::Unknown);
        }
        Ok((provider_wav, wav, info))
    })
    .await
    .map_err(|_| ProviderError::Unknown)??;
    Ok(Speech {
        provider_wav,
        wav,
        info,
        request_id: receipt.request_id,
        verification,
        input_tokens: receipt.input_tokens,
        output_tokens: receipt.output_tokens,
    })
}
fn number(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
/// Repair only the documented-in-project Qwen stream lengths, preserving AIGC and PCM.
pub(crate) fn normalize_wave(source: &[u8]) -> Result<Vec<u8>, ProviderError> {
    if source.len() < 44
        || source.len() > MAX_BYTES
        || &source[..4] != b"RIFF"
        || &source[8..12] != b"WAVE"
    {
        return Err(ProviderError::Unknown);
    }
    let streamed = number(source, 4) == 2_147_483_583;
    if !streamed && number(source, 4) as usize != source.len() - 8 {
        return Err(ProviderError::Unknown);
    }
    let mut output = source.to_vec();
    let (mut offset, mut format, mut data) = (12usize, false, false);
    while offset < source.len() {
        if source.len() - offset < 8 {
            return Err(ProviderError::Unknown);
        }
        let length = number(source, offset + 4) as usize;
        let start = offset + 8;
        let remaining = source.len() - start;
        match &source[offset..offset + 4] {
            b"fmt " => {
                if format
                    || length != 16
                    || remaining < 16
                    || source[start..start + 4] != [1, 0, 1, 0]
                    || number(source, start + 4) != 24000
                    || number(source, start + 8) != 48000
                    || source[start + 12..start + 16] != [2, 0, 16, 0]
                {
                    return Err(ProviderError::Unknown);
                }
                format = true;
            }
            b"data" => {
                if data || !format || length == 0 {
                    return Err(ProviderError::Unknown);
                }
                data = true;
                if streamed && length > remaining && remaining > 0 && remaining.is_multiple_of(2) {
                    output[4..8].copy_from_slice(&((source.len() - 8) as u32).to_le_bytes());
                    output[offset + 4..offset + 8]
                        .copy_from_slice(&(remaining as u32).to_le_bytes());
                    return Ok(output);
                }
            }
            _ => {}
        }
        if length > remaining {
            return Err(ProviderError::Unknown);
        }
        offset = start + length + length % 2;
    }
    if streamed || !format || !data || offset != source.len() {
        return Err(ProviderError::Unknown);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    fn request() -> SpeechRequest {
        SpeechRequest {
            profile: CharacterVoiceProfile {
                personality: "Quiet and curious.".into(),
                speaking_style: "Warm, clear French.".into(),
                default_emotion: "Relaxed.".into(),
                provider: "qwen".into(),
                model: MODEL.into(),
                voice_id: "longanhuan_v3.1".into(),
                voice_kind: "system".into(),
                locale: "fr-FR".into(),
                rate: 0.85,
                reference_audio: None,
            },
            text: "Bonjour ! Je voudrais une baguette.".into(),
            emotion: "A gently expectant request.".into(),
        }
    }
    fn wave(stream: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend([1, 0, 1, 0]);
        bytes.extend(24000u32.to_le_bytes());
        bytes.extend(48000u32.to_le_bytes());
        bytes.extend([2, 0, 16, 0]);
        bytes.extend(b"AIGC");
        bytes.extend(4u32.to_le_bytes());
        bytes.extend(b"tag!");
        bytes.extend(b"data");
        bytes.extend(4800u32.to_le_bytes());
        bytes.extend(vec![42; 4800]);
        let length = (bytes.len() - 8) as u32;
        bytes[4..8]
            .copy_from_slice(&(if stream { 2_147_483_583u32 } else { length }).to_le_bytes());
        if stream {
            bytes[52..56].copy_from_slice(&2_147_483_315u32.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn parameters_use_role_directions_and_refuse_invalid_input_before_network() {
        let mut r = request();
        let body = r.parameters().unwrap();
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["input"]["rate"], 0.85);
        assert_eq!(body["input"]["language_hints"], json!(["fr"]));
        assert_eq!(body["input"]["enable_aigc_tag"], true);
        assert_eq!(body["input"]["format"], "wav");
        assert_eq!(body["input"]["sample_rate"], 24000);
        let instruction = body["input"]["instruction"].as_str().unwrap();
        for expected in [
            &r.profile.personality,
            &r.profile.speaking_style,
            &r.profile.default_emotion,
            &r.emotion,
        ] {
            assert!(instruction.contains(expected));
        }
        r.text = "é".repeat(601);
        assert!(r.parameters().is_err());
        r.text = "é".repeat(600);
        assert!(r.parameters().is_ok());
        r.text = "Bonjour\n".into();
        assert!(r.parameters().is_err());
        r.text = "Bonjour".into();
        r.profile.rate = f64::NAN;
        assert!(r.parameters().is_err());
        r.profile.rate = 1.;
        r.profile.model = "qwen-audio-3.1-tts-next".into();
        assert!(r.parameters().is_err());
        r.profile.model = MODEL.into();
        for (voice, _) in brioche_course_contract::QWEN_FRENCH_SYSTEM_VOICES {
            r.profile.voice_id = (*voice).into();
            assert!(r.parameters().is_ok());
        }
        r.profile.voice_id = "longanhuan_v3.6".into();
        assert!(r.parameters().is_err());
        r.profile.voice_id = format!("{MODEL}-test-voice");
        assert!(r.parameters().is_err());
        r.profile.voice_kind = "cloned".into();
        assert!(r.parameters().is_err());
    }
    #[test]
    fn cantonese_uses_dialect_instruction_and_supported_multilingual_voices() {
        let mut r = request();
        r.profile.locale = "yue-Hant-HK".into();
        r.profile.speaking_style = "亲切自然的香港粤语。".into();
        r.text = "早晨！兩位？".into();
        for (voice, _) in brioche_course_contract::QWEN_MULTILINGUAL_SYSTEM_VOICES {
            r.profile.voice_id = (*voice).into();
            let body = r.parameters().unwrap();
            assert_eq!(body["input"]["text"], r.text);
            assert!(body["input"].get("language_hints").is_none());
            let direction = body["input"]["instruction"].as_str().unwrap();
            assert!(direction.contains("香港粤语"));
            assert!(direction.contains("不要用普通话"));
            assert!(!direction.contains("French"));
            assert!(direction.contains(&r.emotion));
        }
        r.profile.voice_id = "yuxiaoyun_v3.1".into();
        assert!(r.parameters().is_err());
        r.profile.voice_id = "longanhuan_v3.1".into();
        let mut character = brioche_course_contract::Character {
            character_id: "cantonese-fixture".into(),
            revision: 1,
            display_name: "测试角色".into(),
            avatar_id: "fixture-avatar".into(),
            speech_locale: "yue-Hant-HK".into(),
        };
        assert!(crate::character_voices::validate_for_character(&r.profile, &character).is_ok());
        character.speech_locale = "fr-FR".into();
        assert!(crate::character_voices::validate_for_character(&r.profile, &character).is_err());
        for locale in ["zh-CN", "yue", "en-US", ""] {
            r.profile.locale = locale.into();
            assert!(r.parameters().is_err());
        }
    }
    #[test]
    fn receipt_pins_oss_tls_and_retains_only_safe_fields() {
        let mut value = json!({"request_id":"synthesis-test","output":{"finish_reason":"stop","audio":{"url":"http://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/test.wav?Signature=private"}},"usage":{"input_tokens":12,"output_tokens":34},"resource_link":"discard"});
        let result = parse_result(&value).unwrap();
        assert_eq!(result.url.scheme(), "https");
        assert_eq!(result.request_id, "synthesis-test");
        assert_eq!(result.input_tokens, Some(12));
        assert_eq!(result.output_tokens, Some(34));
        for url in [
            "https://bad.example/test.wav",
            "http://127.0.0.1/test.wav",
            "https://dashscope-result-bj.oss-cn-beijing.aliyuncs.com:4433/a",
            "https://user:pass@dashscope-result-bj.oss-cn-beijing.aliyuncs.com/a",
            "https://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/a#secret",
            "https://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/",
        ] {
            value["output"]["audio"]["url"] = json!(url);
            assert!(parse_result(&value).is_err());
        }
        value["output"]["audio"]["url"] =
            json!("https://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/test.wav");
        value["output"]["finish_reason"] = json!("length");
        assert!(parse_result(&value).is_err());
        value["output"]["finish_reason"] = json!("stop");
        value["request_id"] = json!("unsafe/query?secret");
        assert!(parse_result(&value).is_err());
        value["request_id"] = json!("safe");
        value["code"] = json!("provider-error");
        assert!(parse_result(&value).is_err());
    }
    #[test]
    fn wave_repair_preserves_aigc_and_pcm_and_rejects_corruption() {
        let raw = wave(true);
        let fixed = normalize_wave(&raw).unwrap();
        assert_eq!(number(&fixed, 4) as usize, fixed.len() - 8);
        assert_eq!(number(&fixed, 52), 4800);
        assert_eq!(&fixed[8..52], &raw[8..52]);
        assert_eq!(&fixed[56..], &raw[56..]);
        assert_eq!(number(&raw, 4), 2_147_483_583);
        let info = crate::audio::inspect(&fixed, "audio/wav").unwrap();
        assert_eq!(info.duration_ms, 100);
        assert_eq!(info.channels, 1);
        assert_eq!(info.sample_rate, 24000);
        assert_eq!(normalize_wave(&wave(false)).unwrap(), wave(false));
        assert!(normalize_wave(&raw[..raw.len() - 1]).is_err());
        let mut bad = raw.clone();
        bad[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(normalize_wave(&bad).is_err());
        let mut bad = raw.clone();
        bad[20] = 3;
        assert!(normalize_wave(&bad).is_err());
        let mut bad = raw.clone();
        bad[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(normalize_wave(&bad).is_err());
        let mut bad = wave(false);
        bad[52..56].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(normalize_wave(&bad).is_err());
        let mut bad = wave(false);
        bad.extend(b"data");
        bad.extend(2u32.to_le_bytes());
        bad.extend([0, 0]);
        let size = (bad.len() - 8) as u32;
        bad[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(normalize_wave(&bad).is_err());
    }
    #[tokio::test]
    async fn wire_gates_paid_synthesis_and_downloads_without_credentials_or_retries() {
        let state = Arc::new(Mutex::new((
            "DEPLOYING".to_string(),
            MODEL.to_string(),
            200u16,
        )));
        let calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let downloads = Arc::new(Mutex::new(0usize));
        let s = state.clone();
        let c = calls.clone();
        let s2 = state.clone();
        let c2 = calls.clone();
        let d = downloads.clone();
        let result_url = Arc::new(Mutex::new("https://bad.example/secret".to_string()));
        let u = result_url.clone();
        let app=axum::Router::new()
            .route("/customization",axum::routing::post(move |headers:axum::http::HeaderMap,axum::Json(body):axum::Json<Value>| {let s=s.clone();let c=c.clone();async move {
                assert_eq!(headers["authorization"],"Bearer synthetic-key");c.lock().unwrap().push(body);
                let v=s.lock().unwrap().clone();axum::Json(json!({"request_id":"query-test","output":{"target_model":v.1,"status":v.0}}))
            }}))
            .route("/SpeechSynthesizer",axum::routing::post(move |headers:axum::http::HeaderMap,axum::Json(body):axum::Json<Value>|{let s=s2.clone();let c=c2.clone();let u=u.clone();async move{
                assert_eq!(headers["authorization"],"Bearer synthetic-key");c.lock().unwrap().push(body);let code=s.lock().unwrap().2;
                (axum::http::StatusCode::from_u16(code).unwrap(),axum::Json(json!({"request_id":"synthesis-test","output":{"finish_reason":"stop","audio":{"url":u.lock().unwrap().clone()}},"usage":{"input_tokens":20,"output_tokens":30}})))
            }}))
            .route("/audio",axum::routing::get(move |headers:axum::http::HeaderMap|{let d=d.clone();async move{assert!(headers.get("authorization").is_none());assert!(headers.get("cookie").is_none());*d.lock().unwrap()+=1;wave(true)}}))
            .route("/redirect",axum::routing::get(||async {(axum::http::StatusCode::FOUND,[("location","/audio")])}))
            .route("/oversized",axum::routing::get(||async{vec![0u8;MAX_BYTES+1]}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        // Only this private fixture accepts HTTP. Api::new still rejects HTTP production configuration.
        // reqwest builds its TLS backend even for loopback HTTP; do not depend on
        // another concurrently running test having called the production constructor.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let api = Api {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .build()
                .unwrap(),
            endpoint: format!("http://{address}/customization"),
            key: "synthetic-key".into(),
        };
        let mut r = request();
        r.text = "bad\n".into();
        assert!(api.synthesize(&r).await.is_err());
        assert!(calls.lock().unwrap().is_empty());
        r.text = "Bonjour !".into();
        r.profile.voice_kind = "cloned".into();
        r.profile.voice_id = format!("{MODEL}-abc-test");
        r.profile.reference_audio = Some(brioche_course_contract::CharacterVoiceReference {
            asset_id: "reference-test".into(),
            revision: 1,
            transcript: "Bonjour !".into(),
            cloning_permission: "Fixture consent, not a real recording.".into(),
        });
        assert!(api.synthesize(&r).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 1);
        state.lock().unwrap().0 = "UNDEPLOYED".into();
        assert!(api.synthesize(&r).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 2);
        {
            let mut s = state.lock().unwrap();
            s.0 = "OK".into();
            s.1 = "different-model".into();
        }
        assert!(api.synthesize(&r).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 3);
        state.lock().unwrap().1 = MODEL.into();
        assert!(api.synthesize(&r).await.is_err());
        assert_eq!(calls.lock().unwrap().len(), 5);
        assert_eq!(calls.lock().unwrap()[4], r.parameters().unwrap());
        assert_eq!(*downloads.lock().unwrap(), 0);
        r.profile.voice_kind = "system".into();
        r.profile.voice_id = "longanhuan_v3.1".into();
        r.profile.reference_audio = None;
        state.lock().unwrap().2 = 503;
        assert!(matches!(
            api.synthesize(&r).await,
            Err(ProviderError::Unknown)
        ));
        assert_eq!(calls.lock().unwrap().len(), 6);
        state.lock().unwrap().2 = 422;
        assert!(matches!(
            api.synthesize(&r).await,
            Err(ProviderError::Rejected)
        ));
        assert_eq!(calls.lock().unwrap().len(), 7);
        state.lock().unwrap().2 = 200;
        *result_url.lock().unwrap() =
            "http://dashscope-result-bj.oss-cn-beijing.aliyuncs.com/test.wav?Signature=fixture"
                .into();
        r.profile.voice_kind = "cloned".into();
        r.profile.voice_id = format!("{MODEL}-abc-test");
        r.profile.reference_audio = Some(brioche_course_contract::CharacterVoiceReference {
            asset_id: "reference-test".into(),
            revision: 1,
            transcript: "Bonjour !".into(),
            cloning_permission: "Fixture consent.".into(),
        });
        let result = synthesize_with(&api, &r, |url| async move {
            assert_eq!(url.scheme(), "https");
            assert_eq!(
                url.host_str(),
                Some("dashscope-result-bj.oss-cn-beijing.aliyuncs.com")
            );
            Ok(wave(true))
        })
        .await
        .unwrap();
        assert_eq!(calls.lock().unwrap().len(), 9);
        assert_eq!(result.provider_wav, wave(true));
        assert_eq!(result.wav, wave(false));
        assert_eq!(result.info.duration_ms, 100);
        assert_eq!(result.request_id, "synthesis-test");
        assert_eq!(result.input_tokens, Some(20));
        assert_eq!(result.output_tokens, Some(30));
        let verification = result.verification.unwrap();
        assert_eq!(verification.model, MODEL);
        assert_eq!(verification.status, "OK");
        assert_eq!(verification.request_id, "query-test");
        assert!(
            synthesize_with(&api, &r, |_| async { Ok(vec![0; 44]) })
                .await
                .is_err()
        );
        let bytes = fetch_audio(
            &api,
            url::Url::parse(&format!("http://{address}/audio")).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(bytes, wave(true));
        assert!(
            fetch_audio(
                &api,
                url::Url::parse(&format!("http://{address}/redirect")).unwrap()
            )
            .await
            .is_err()
        );
        assert_eq!(*downloads.lock().unwrap(), 1);
        assert!(
            fetch_audio(
                &api,
                url::Url::parse(&format!("http://{address}/oversized")).unwrap()
            )
            .await
            .is_err()
        );
        server.abort();
        let _ = server.await;
    }
}
