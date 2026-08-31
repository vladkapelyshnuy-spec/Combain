//! OpenAI-compatible Images API adapter.
//!
//! Requests `response_format: "b64_json"` so the generated image bytes come
//! back directly in the response body, with no second HTTP round trip to
//! fetch a URL. The Images API always returns PNG.

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::provider::image_provider::{GeneratedImage, ImageProvider, ImageRequest};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "dall-e-3";

/// Connection settings for the OpenAI-compatible image adapter.
///
/// Reuses the same `OPENAI_API_KEY`/`OPENAI_BASE_URL` env vars as the text
/// provider (Phase 5) since both talk to the same OpenAI-compatible host;
/// the model is configured separately via `OPENAI_IMAGE_MODEL`.
#[derive(Clone)]
pub struct OpenAiImageConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for OpenAiImageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiImageConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl OpenAiImageConfig {
    pub fn from_env() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let base_url = std::env::var("OPENAI_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = std::env::var("OPENAI_IMAGE_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        Self {
            api_key,
            base_url,
            model,
        }
    }
}

pub struct OpenAiImageProvider {
    client: reqwest::Client,
    config: OpenAiImageConfig,
}

impl OpenAiImageProvider {
    pub fn new(config: OpenAiImageConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

/// Build the Images API request body. Pure/testable without network access.
pub fn build_request_body(request: &ImageRequest, model: &str) -> Value {
    json!({
        "model": model,
        "prompt": request.prompt,
        "n": 1,
        "response_format": "b64_json",
    })
}

/// Extract and decode the generated image bytes from an Images API response body.
/// Pure/testable without network access.
pub fn parse_response(body: &str, model: &str) -> Result<GeneratedImage, AppError> {
    let envelope: Value = serde_json::from_str(body)
        .map_err(|error| AppError::provider(format!("invalid JSON from provider: {error}")))?;

    let b64 = envelope
        .get("data")
        .and_then(|data| data.get(0))
        .and_then(|item| item.get("b64_json"))
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::provider("provider response is missing data[0].b64_json"))?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|error| {
            AppError::provider(format!("provider image is not valid base64: {error}"))
        })?;

    Ok(GeneratedImage {
        bytes,
        model: model.to_string(),
    })
}

#[async_trait]
impl ImageProvider for OpenAiImageProvider {
    async fn generate_image(&self, request: &ImageRequest) -> Result<GeneratedImage, AppError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AppError::config("OPENAI_API_KEY is not set; cannot call the image provider")
        })?;

        let url = format!(
            "{}/images/generations",
            self.config.base_url.trim_end_matches('/')
        );
        let body = build_request_body(request, &self.config.model);

        let response = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                AppError::provider(format!("request to image provider failed: {error}"))
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|error| {
            AppError::provider(format!("failed to read provider response: {error}"))
        })?;

        if !status.is_success() {
            return Err(AppError::provider(format!(
                "image provider returned status {status}: {text}"
            )));
        }

        parse_response(&text, &self.config.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ImageRequest {
        ImageRequest {
            prompt: "a cinematic nebula".into(),
        }
    }

    #[test]
    fn debug_redacts_api_key() {
        let config = OpenAiImageConfig {
            api_key: Some("sk-secret".into()),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_body_includes_model_and_prompt() {
        let body = build_request_body(&sample_request(), "dall-e-3");
        assert_eq!(body["model"], "dall-e-3");
        assert_eq!(body["response_format"], "b64_json");
        assert_eq!(body["prompt"], "a cinematic nebula");
    }

    #[test]
    fn parses_valid_images_envelope() {
        let png_bytes: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xAA];
        let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
        let envelope = json!({ "data": [{ "b64_json": b64 }] });

        let parsed = parse_response(&envelope.to_string(), "dall-e-3").expect("parse");
        assert_eq!(parsed.bytes, png_bytes);
        assert_eq!(parsed.model, "dall-e-3");
    }

    #[test]
    fn rejects_envelope_missing_b64_json() {
        let envelope = json!({ "data": [{}] });
        let error =
            parse_response(&envelope.to_string(), "dall-e-3").expect_err("missing b64_json");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn rejects_invalid_base64() {
        let envelope = json!({ "data": [{ "b64_json": "not-base64!!" }] });
        let error = parse_response(&envelope.to_string(), "dall-e-3").expect_err("bad base64");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }
}
