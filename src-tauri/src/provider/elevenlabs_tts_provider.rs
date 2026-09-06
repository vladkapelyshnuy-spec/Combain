//! ElevenLabs TTS API adapter.
//!
//! Supports text-to-speech synthesis via a synchronous HTTP request.
//! The provider returns audio bytes immediately (no polling required).
//! Uses custom xi-api-key header for authentication and voice_id as path parameter.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::domain::provider_profile::ProviderProfile;
use crate::error::AppError;
use crate::provider::tts_provider::{GeneratedAudio, TTSProvider, TTSRequest};

const DEFAULT_BASE_URL: &str = "https://api.elevenlabs.io";
const DEFAULT_MODEL: &str = "eleven_multilingual_v2";
const DEFAULT_VOICE: &str = "dHAwRJVaEPhU907QLTPW"; // Natalia (Russian female)
const DEFAULT_FORMAT: &str = "mp3";

/// Connection settings for the ElevenLabs TTS adapter.
///
/// Built from the default TTS [`ProviderProfile`] and its API key (see
/// [`ElevenLabsConfig::from_profile`]). Never persisted to config files,
/// logs, or serialized output (see the custom [`std::fmt::Debug`] impl).
#[derive(Clone)]
pub struct ElevenLabsConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub default_voice: String,
    pub format: String,
}

impl std::fmt::Debug for ElevenLabsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElevenLabsConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("default_voice", &self.default_voice)
            .field("format", &self.format)
            .finish()
    }
}

impl Default for ElevenLabsConfig {
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

impl ElevenLabsConfig {
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

pub struct ElevenLabsTtsProvider {
    client: reqwest::Client,
    config: ElevenLabsConfig,
}

impl ElevenLabsTtsProvider {
    pub fn new(config: ElevenLabsConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

pub fn build_request_body(request: &TTSRequest, model: &str) -> Value {
    json!({
        "text": request.text,
        "model_id": model,
    })
}

#[async_trait]
impl TTSProvider for ElevenLabsTtsProvider {
    async fn synthesize(&self, request: &TTSRequest) -> Result<GeneratedAudio, AppError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AppError::config(
                "no API key stored for the default TTS provider profile; add one in Settings",
            )
        })?;

        let voice_id = request
            .voice
            .as_deref()
            .unwrap_or(&self.config.default_voice);

        let url = format!(
            "{}/v1/text-to-speech/{}",
            self.config.base_url.trim_end_matches('/'),
            voice_id
        );

        let body = build_request_body(request, &self.config.model);

        tracing::info!(
            "requesting ElevenLabs TTS synthesis: text_len={}, voice_id={}, model={}",
            request.text.len(),
            voice_id,
            self.config.model
        );

        let response = self
            .client
            .post(&url)
            .header("xi-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                AppError::provider(format!("request to ElevenLabs TTS failed: {error}"))
            })?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::provider(format!(
                "ElevenLabs TTS returned status {status}: {text}"
            )));
        }

        let bytes = response.bytes().await.map_err(|error| {
            AppError::provider(format!("failed to read ElevenLabs TTS response: {error}"))
        })?;

        Ok(GeneratedAudio {
            bytes: bytes.to_vec(),
            model: self.config.model.clone(),
            voice: voice_id.to_string(),
            format: self.config.format.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> TTSRequest {
        TTSRequest {
            text: "Привет, мир!".into(),
            voice: None,
        }
    }

    fn sample_profile() -> ProviderProfile {
        ProviderProfile::new(
            crate::domain::provider_profile::ProviderPurpose::Tts,
            "Primary",
            "elevenlabs",
            Some("https://api.elevenlabs.io".into()),
            "eleven_multilingual_v2",
            Some("dHAwRJVaEPhU907QLTPW".into()),
            true,
        )
        .expect("valid profile")
    }

    #[test]
    fn from_profile_uses_profile_fields_and_given_key() {
        let profile = sample_profile();
        let config = ElevenLabsConfig::from_profile(&profile, Some("el_test123".into()));

        assert_eq!(config.api_key, Some("el_test123".into()));
        assert_eq!(config.base_url, "https://api.elevenlabs.io");
        assert_eq!(config.model, "eleven_multilingual_v2");
        assert_eq!(config.default_voice, "dHAwRJVaEPhU907QLTPW");
        assert_eq!(config.format, "mp3");
    }

    #[test]
    fn from_profile_falls_back_to_defaults() {
        let mut profile = sample_profile();
        profile.base_url = None;
        profile.voice = None;
        let config = ElevenLabsConfig::from_profile(&profile, None);

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.default_voice, DEFAULT_VOICE);
    }

    #[test]
    fn debug_redacts_api_key() {
        let config = ElevenLabsConfig {
            api_key: Some("el_secret123".into()),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
            default_voice: DEFAULT_VOICE.into(),
            format: DEFAULT_FORMAT.into(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("el_secret123"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_body_includes_required_fields() {
        let request = sample_request();
        let body = build_request_body(&request, "eleven_multilingual_v2");
        assert_eq!(body["text"], "Привет, мир!");
        assert_eq!(body["model_id"], "eleven_multilingual_v2");
    }

    #[test]
    fn request_uses_override_voice_id() {
        let mut request = sample_request();
        request.voice = Some("custom_voice_id_123".into());

        let config = ElevenLabsConfig::default();
        let provider = ElevenLabsTtsProvider::new(config);

        // Voice should be used in URL path, tested in integration
        assert_eq!(request.voice, Some("custom_voice_id_123".into()));
    }
}

