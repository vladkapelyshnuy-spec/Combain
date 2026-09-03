//! OpenAI-compatible TTS API adapter.
//!
//! Supports text-to-speech synthesis via a synchronous HTTP request.
//! The provider returns audio bytes immediately (no polling required).

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::domain::provider_profile::ProviderProfile;
use crate::error::AppError;
use crate::provider::tts_provider::{GeneratedAudio, TTSProvider, TTSRequest};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "tts-1";
const DEFAULT_VOICE: &str = "alloy";
const DEFAULT_FORMAT: &str = "mp3";

/// Connection settings for the OpenAI-compatible TTS adapter.
///
/// Built from the default TTS [`ProviderProfile`] and its API key (see
/// [`OpenAiTtsConfig::from_profile`]). Never persisted to config files,
/// logs, or serialized output (see the custom [`std::fmt::Debug`] impl).
#[derive(Clone)]
pub struct OpenAiTtsConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub default_voice: String,
    pub format: String,
}

impl std::fmt::Debug for OpenAiTtsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiTtsConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("default_voice", &self.default_voice)
            .field("format", &self.format)
            .finish()
    }
}

impl Default for OpenAiTtsConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            default_voice: DEFAULT_VOICE.to_string(),
            format: DEFAULT_FORMAT.to_string(),
        }
    }
}

impl OpenAiTtsConfig {
    pub fn from_profile(profile: &ProviderProfile, api_key: Option<String>) -> Self {
        let api_key = api_key.filter(|value| !value.trim().is_empty());
        let base_url = profile
            .base_url
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = if profile.model.trim().is_empty() {
            DEFAULT_MODEL.to_string()
        } else {
            profile.model.clone()
        };
        let default_voice = profile
            .voice
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_VOICE.to_string());

        Self {
            api_key,
            base_url,
            model,
            default_voice,
            format: DEFAULT_FORMAT.to_string(),
        }
    }
}

pub struct OpenAiTtsProvider {
    client: reqwest::Client,
    config: OpenAiTtsConfig,
}

impl OpenAiTtsProvider {
    pub fn new(config: OpenAiTtsConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

pub fn build_request_body(
    request: &TTSRequest,
    model: &str,
    default_voice: &str,
    format: &str,
) -> Value {
    let voice = request.voice.as_deref().unwrap_or(default_voice);
    json!({
        "model": model,
        "input": request.text,
        "voice": voice,
        "response_format": format,
    })
}

#[async_trait]
impl TTSProvider for OpenAiTtsProvider {
    async fn synthesize(&self, request: &TTSRequest) -> Result<GeneratedAudio, AppError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AppError::config(
                "no API key stored for the default TTS provider profile; add one in Settings",
            )
        })?;

        let url = format!(
            "{}/audio/speech",
            self.config.base_url.trim_end_matches('/')
        );
        let body = build_request_body(
            request,
            &self.config.model,
            &self.config.default_voice,
            &self.config.format,
        );

        let voice = request
            .voice
            .as_deref()
            .unwrap_or(&self.config.default_voice);

        tracing::info!(
            "requesting TTS synthesis: text_len={}, voice={}, model={}",
            request.text.len(),
            voice,
            self.config.model
        );

        let response = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                AppError::provider(format!("request to TTS provider failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::provider(format!(
                "TTS provider returned status {status}: {text}"
            )));
        }

        let bytes = response.bytes().await.map_err(|error| {
            AppError::provider(format!("failed to read TTS provider response: {error}"))
        })?;

        Ok(GeneratedAudio {
            bytes: bytes.to_vec(),
            model: self.config.model.clone(),
            voice: voice.to_string(),
            format: self.config.format.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> TTSRequest {
        TTSRequest {
            text: "Hello, world!".into(),
            voice: None,
        }
    }

    fn sample_profile() -> ProviderProfile {
        ProviderProfile::new(
            crate::domain::provider_profile::ProviderPurpose::Tts,
            "Primary",
            "openai",
            Some("https://api.example.com/v1".into()),
            "tts-1",
            Some("nova".into()),
            true,
        )
        .expect("valid profile")
    }

    #[test]
    fn from_profile_uses_profile_fields_and_given_key() {
        let profile = sample_profile();
        let config = OpenAiTtsConfig::from_profile(&profile, Some("sk-test".into()));

        assert_eq!(config.api_key, Some("sk-test".into()));
        assert_eq!(config.base_url, "https://api.example.com/v1");
        assert_eq!(config.model, "tts-1");
        assert_eq!(config.default_voice, "nova");
        assert_eq!(config.format, "mp3");
    }

    #[test]
    fn from_profile_falls_back_to_defaults() {
        let mut profile = sample_profile();
        profile.base_url = None;
        profile.voice = None;
        let config = OpenAiTtsConfig::from_profile(&profile, None);

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.default_voice, DEFAULT_VOICE);
    }

    #[test]
    fn debug_redacts_api_key() {
        let config = OpenAiTtsConfig {
            api_key: Some("sk-secret".into()),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
            default_voice: DEFAULT_VOICE.into(),
            format: DEFAULT_FORMAT.into(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_body_includes_all_fields() {
        let request = sample_request();
        let body = build_request_body(&request, "tts-1", "alloy", "mp3");
        assert_eq!(body["model"], "tts-1");
        assert_eq!(body["input"], "Hello, world!");
        assert_eq!(body["voice"], "alloy");
        assert_eq!(body["response_format"], "mp3");
    }

    #[test]
    fn request_body_uses_override_voice() {
        let mut request = sample_request();
        request.voice = Some("nova".into());
        let body = build_request_body(&request, "tts-1", "alloy", "mp3");
        assert_eq!(body["voice"], "nova");
    }
}
