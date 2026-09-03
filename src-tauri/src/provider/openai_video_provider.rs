//! OpenAI-compatible Video API adapter.
//!
//! Supports both Image-to-Video and Text-to-Video generation via an async
//! request/poll pattern. The provider returns an operation ID immediately,
//! and the caller must poll `/video/generations/{id}` until completion.

use async_trait::async_trait;
use base64::Engine;
use serde_json::{json, Value};
use std::path::Path;
use tokio::time::{sleep, Duration};

use crate::domain::provider_profile::ProviderProfile;
use crate::error::AppError;
use crate::provider::video_provider::{
    GeneratedVideo, VideoGenerationJob, VideoProvider, VideoRequest, VideoStatus,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "video-turbo";
const POLL_INTERVAL_SECS: u64 = 3;
const POLL_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Connection settings for the OpenAI-compatible video adapter.
///
/// Built from the default Video [`ProviderProfile`] and its API key (see
/// [`OpenAiVideoConfig::from_profile`]). Never persisted to config files,
/// logs, or serialized output (see the custom [`std::fmt::Debug`] impl).
#[derive(Clone)]
pub struct OpenAiVideoConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for OpenAiVideoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiVideoConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl Default for OpenAiVideoConfig {
    /// Used when no default Video provider profile is configured yet; any
    /// request attempt fails with a clear "add a profile in Settings" error.
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

impl OpenAiVideoConfig {
    /// Build settings from a Video [`ProviderProfile`] and its API key
    /// (looked up separately from the OS credential store via
    /// [`crate::secrets`]). A missing/empty `api_key` is not an error here,
    /// it only surfaces once a request is actually attempted.
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

        Self {
            api_key,
            base_url,
            model,
        }
    }
}

pub struct OpenAiVideoProvider {
    client: reqwest::Client,
    config: OpenAiVideoConfig,
}

impl OpenAiVideoProvider {
    pub fn new(config: OpenAiVideoConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

/// Build the Video API request body. Pure/testable without network access.
pub fn build_request_body(
    request: &VideoRequest,
    model: &str,
) -> Result<Value, AppError> {
    let mut body = json!({
        "model": model,
        "prompt": request.prompt,
        "duration_ms": request.target_duration_ms,
    });

    // Image-to-Video: read and encode the source image
    if let Some(ref image_path) = request.source_image_path {
        let image_bytes = std::fs::read(image_path).map_err(|error| {
            AppError::io(format!(
                "failed to read source image {}: {error}",
                image_path.display()
            ))
        })?;
        let image_b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
        body["image"] = json!(image_b64);
    }

    Ok(body)
}

/// Parse the response from POST /video/generations to extract the operation ID.
pub fn parse_request_response(body: &str) -> Result<VideoGenerationJob, AppError> {
    let envelope: Value = serde_json::from_str(body)
        .map_err(|error| AppError::provider(format!("invalid JSON from provider: {error}")))?;

    let operation_id = envelope
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::provider("provider response is missing 'id'"))?;

    Ok(VideoGenerationJob {
        operation_id: operation_id.to_string(),
    })
}

/// Parse the response from GET /video/generations/{id} to extract status.
pub fn parse_poll_response(body: &str, model: &str) -> Result<VideoStatus, AppError> {
    let envelope: Value = serde_json::from_str(body)
        .map_err(|error| AppError::provider(format!("invalid JSON from provider: {error}")))?;

    let status = envelope
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::provider("provider response is missing 'status'"))?;

    match status {
        "pending" | "queued" => Ok(VideoStatus::Pending),
        "in_progress" | "processing" => Ok(VideoStatus::InProgress),
        "completed" | "succeeded" => {
            let video_b64 = envelope
                .get("video")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::provider("completed job is missing 'video' field")
                })?;

            let bytes = base64::engine::general_purpose::STANDARD
                .decode(video_b64)
                .map_err(|error| {
                    AppError::provider(format!("provider video is not valid base64: {error}"))
                })?;

            Ok(VideoStatus::Completed(GeneratedVideo {
                bytes,
                model: model.to_string(),
                format: "mp4".to_string(),
            }))
        }
        "failed" | "error" => {
            let error_msg = envelope
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            Ok(VideoStatus::Failed(error_msg.to_string()))
        }
        unknown => Err(AppError::provider(format!(
            "unknown status from provider: {unknown}"
        ))),
    }
}

#[async_trait]
impl VideoProvider for OpenAiVideoProvider {
    async fn request_video(
        &self,
        request: &VideoRequest,
    ) -> Result<VideoGenerationJob, AppError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AppError::config(
                "no API key stored for the default video provider profile; add one in Settings",
            )
        })?;

        let url = format!(
            "{}/video/generations",
            self.config.base_url.trim_end_matches('/')
        );
        let body = build_request_body(request, &self.config.model)?;

        tracing::info!(
            "requesting video generation: prompt={}, has_image={}, duration_ms={}",
            request.prompt,
            request.source_image_path.is_some(),
            request.target_duration_ms
        );

        let response = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                AppError::provider(format!("request to video provider failed: {error}"))
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|error| {
            AppError::provider(format!("failed to read provider response: {error}"))
        })?;

        if !status.is_success() {
            return Err(AppError::provider(format!(
                "video provider returned status {status}: {text}"
            )));
        }

        parse_request_response(&text)
    }

    async fn poll_status(&self, job: &VideoGenerationJob) -> Result<VideoStatus, AppError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AppError::config("no API key available for video provider")
        })?;

        let url = format!(
            "{}/video/generations/{}",
            self.config.base_url.trim_end_matches('/'),
            job.operation_id
        );

        let start = std::time::Instant::now();
        let mut poll_count = 0;

        loop {
            if start.elapsed().as_secs() > POLL_TIMEOUT_SECS {
                return Err(AppError::provider(format!(
                    "video generation timed out after {} seconds",
                    POLL_TIMEOUT_SECS
                )));
            }

            tracing::debug!(
                "polling video status: operation_id={}, poll_count={}",
                job.operation_id,
                poll_count
            );

            let response = self
                .client
                .get(&url)
                .bearer_auth(api_key)
                .send()
                .await
                .map_err(|error| {
                    AppError::provider(format!("failed to poll video status: {error}"))
                })?;

            let status = response.status();
            let text = response.text().await.map_err(|error| {
                AppError::provider(format!("failed to read poll response: {error}"))
            })?;

            if !status.is_success() {
                return Err(AppError::provider(format!(
                    "video provider poll returned status {status}: {text}"
                )));
            }

            let video_status = parse_poll_response(&text, &self.config.model)?;

            match video_status {
                VideoStatus::Completed(_) => {
                    tracing::info!(
                        "video generation completed: operation_id={}, poll_count={}",
                        job.operation_id,
                        poll_count
                    );
                    return Ok(video_status);
                }
                VideoStatus::Failed(ref error_msg) => {
                    tracing::error!(
                        "video generation failed: operation_id={}, error={}",
                        job.operation_id,
                        error_msg
                    );
                    return Ok(video_status);
                }
                VideoStatus::Pending | VideoStatus::InProgress => {
                    poll_count += 1;
                    sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> VideoRequest {
        VideoRequest {
            prompt: "a cinematic space scene".into(),
            source_image_path: None,
            target_duration_ms: 5000,
        }
    }

    fn sample_profile() -> ProviderProfile {
        ProviderProfile::new(
            crate::domain::provider_profile::ProviderPurpose::Video,
            "Primary",
            "openai",
            Some("https://api.example.com/v1".into()),
            "video-turbo",
            None,
            true,
        )
        .expect("valid profile")
    }

    #[test]
    fn from_profile_uses_profile_fields_and_given_key() {
        let profile = sample_profile();
        let config = OpenAiVideoConfig::from_profile(&profile, Some("sk-test".into()));

        assert_eq!(config.api_key, Some("sk-test".into()));
        assert_eq!(config.base_url, "https://api.example.com/v1");
        assert_eq!(config.model, "video-turbo");
    }

    #[test]
    fn from_profile_falls_back_to_defaults_and_none_key() {
        let mut profile = sample_profile();
        profile.base_url = None;
        let config = OpenAiVideoConfig::from_profile(&profile, None);

        assert_eq!(config.api_key, None);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn debug_redacts_api_key() {
        let config = OpenAiVideoConfig {
            api_key: Some("sk-secret".into()),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_body_includes_model_prompt_and_duration() {
        let request = sample_request();
        let body = build_request_body(&request, "video-turbo").expect("build body");
        assert_eq!(body["model"], "video-turbo");
        assert_eq!(body["prompt"], "a cinematic space scene");
        assert_eq!(body["duration_ms"], 5000);
        assert!(body.get("image").is_none());
    }

    #[test]
    fn parses_valid_request_response() {
        let envelope = json!({ "id": "op_12345" });
        let job = parse_request_response(&envelope.to_string()).expect("parse");
        assert_eq!(job.operation_id, "op_12345");
    }

    #[test]
    fn rejects_request_response_missing_id() {
        let envelope = json!({ "status": "queued" });
        let error = parse_request_response(&envelope.to_string()).expect_err("missing id");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn parses_pending_status() {
        let envelope = json!({ "status": "pending" });
        let status = parse_poll_response(&envelope.to_string(), "video-turbo").expect("parse");
        assert!(matches!(status, VideoStatus::Pending));
    }

    #[test]
    fn parses_in_progress_status() {
        let envelope = json!({ "status": "in_progress" });
        let status = parse_poll_response(&envelope.to_string(), "video-turbo").expect("parse");
        assert!(matches!(status, VideoStatus::InProgress));
    }

    #[test]
    fn parses_completed_status_with_video() {
        let video_bytes: &[u8] = &[0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70];
        let video_b64 = base64::engine::general_purpose::STANDARD.encode(video_bytes);
        let envelope = json!({ "status": "completed", "video": video_b64 });

        let status = parse_poll_response(&envelope.to_string(), "video-turbo").expect("parse");
        match status {
            VideoStatus::Completed(video) => {
                assert_eq!(video.bytes, video_bytes);
                assert_eq!(video.model, "video-turbo");
                assert_eq!(video.format, "mp4");
            }
            _ => panic!("expected Completed status"),
        }
    }

    #[test]
    fn parses_failed_status() {
        let envelope = json!({ "status": "failed", "error": "invalid prompt" });
        let status = parse_poll_response(&envelope.to_string(), "video-turbo").expect("parse");
        match status {
            VideoStatus::Failed(msg) => assert_eq!(msg, "invalid prompt"),
            _ => panic!("expected Failed status"),
        }
    }

    #[test]
    fn rejects_unknown_status() {
        let envelope = json!({ "status": "unknown_state" });
        let error =
            parse_poll_response(&envelope.to_string(), "video-turbo").expect_err("unknown status");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }
}




