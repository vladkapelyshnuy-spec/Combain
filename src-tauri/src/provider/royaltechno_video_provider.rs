//! RoyalTechno AI API adapter for video generation.
//!
//! Provides access to multiple video generation models (Veo 3.1, Gemini Omni Flash,
//! Seedance 2.0) through a unified async request/poll API. Supports both Text-to-Video
//! and Image-to-Video generation, with base64 inline image support for local-only workflows.

use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use tokio::time::{sleep, Duration};

use crate::error::AppError;
use crate::provider::video_provider::{
    GeneratedVideo, VideoGenerationJob, VideoProvider, VideoRequest, VideoStatus,
};

const DEFAULT_BASE_URL: &str = "https://api.royaltechno.cc";
const DEFAULT_MODEL: &str = "omni-flash";
const POLL_INTERVAL_SECS: u64 = 5;
const POLL_TIMEOUT_SECS: u64 = 600; // 10 minutes (video generation takes longer)

/// Configuration for RoyalTechno video provider.
///
/// Never persisted to config files or logs (see the custom [`std::fmt::Debug`] impl).
#[derive(Clone)]
pub struct RoyalTechnoConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for RoyalTechnoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoyalTechnoConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl RoyalTechnoConfig {
    pub fn new(api_key: String, base_url: Option<String>, model: Option<String>) -> Self {
        Self {
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }
}

impl Default for RoyalTechnoConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

pub struct RoyalTechnoVideoProvider {
    client: reqwest::Client,
    config: RoyalTechnoConfig,
}

impl RoyalTechnoVideoProvider {
    pub fn new(config: RoyalTechnoConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

/// RoyalTechno API job creation response.
#[derive(Debug, Deserialize)]
struct JobCreateResponse {
    id: String,
    #[allow(dead_code)]
    status: String,
}

/// RoyalTechno API job status response.
#[derive(Debug, Deserialize)]
struct JobStatusResponse {
    id: String,
    status: String,
    output: Option<JobOutput>,
    error: Option<JobError>,
}

#[derive(Debug, Deserialize)]
struct JobOutput {
    #[serde(rename = "type")]
    output_type: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct JobError {
    code: String,
    message: String,
}

/// Build the RoyalTechno API request body.
///
/// CRITICAL: If source_image_path is provided, reads the file from disk and converts
/// to a base64 data URI for inline submission (no external hosting required).
fn build_request_body(request: &VideoRequest, model: &str) -> Result<Value, AppError> {
    let duration_sec = normalize_duration_sec(request.scene_duration_sec);

    let mut input = json!({
        "prompt": request.prompt,
        "aspect_ratio": "landscape",
        "duration_sec": duration_sec,
    });

    // Handle image-to-video: read local file and convert to base64 data URI
    if let Some(ref image_path) = request.source_image_path {
        let data_uri = read_image_as_data_uri(image_path)?;
        input["start_image_url"] = json!(data_uri);
    }

    Ok(json!({
        "model": model,
        "input": input,
    }))
}

/// RoyalTechno's `omni-flash` model accepts only these discrete durations.
/// Choose the nearest one without changing the duration stored on the scene;
/// exact ties round upward so a requested scene is not shortened arbitrarily.
fn normalize_duration_sec(duration_sec: u32) -> u32 {
    const SUPPORTED_DURATIONS: [u32; 4] = [4, 6, 8, 10];

    SUPPORTED_DURATIONS
        .into_iter()
        .min_by_key(|candidate| {
            (
                candidate.abs_diff(duration_sec),
                std::cmp::Reverse(*candidate),
            )
        })
        .expect("supported duration list is non-empty")
}

/// Read an image file from disk and convert to a base64 data URI.
///
/// Format: "data:image/{mime};base64,{encoded_data}"
/// Supported: JPEG, PNG, WebP (max 5MB as per RoyalTechno API spec)
fn read_image_as_data_uri(image_path: &Path) -> Result<String, AppError> {
    // Read file bytes
    let image_bytes = std::fs::read(image_path).map_err(|error| {
        AppError::io(format!(
            "failed to read source image {}: {error}",
            image_path.display()
        ))
    })?;

    // Check size limit (5MB)
    const MAX_SIZE: usize = 5 * 1024 * 1024;
    if image_bytes.len() > MAX_SIZE {
        return Err(AppError::invalid_input(format!(
            "image file too large: {} bytes (max 5MB)",
            image_bytes.len()
        )));
    }

    // Determine MIME type from extension
    let mime_type = match image_path.extension().and_then(|e| e.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some(ext) => {
            return Err(AppError::invalid_input(format!(
                "unsupported image format: .{ext} (supported: jpg, png, webp)"
            )));
        }
        None => {
            return Err(AppError::invalid_input(
                "image file has no extension".to_string(),
            ));
        }
    };

    // Encode to base64
    let encoded = base64::engine::general_purpose::STANDARD.encode(&image_bytes);

    // Build data URI
    Ok(format!("data:{mime_type};base64,{encoded}"))
}

/// Parse job creation response to extract job ID.
fn parse_create_response(body: &str) -> Result<VideoGenerationJob, AppError> {
    let response: JobCreateResponse = serde_json::from_str(body)
        .map_err(|error| AppError::provider(format!("invalid JSON from provider: {error}")))?;

    Ok(VideoGenerationJob {
        operation_id: response.id,
    })
}

/// Parse job status response and map to VideoStatus.
fn parse_status_response(body: &str, model: &str) -> Result<VideoStatus, AppError> {
    let response: JobStatusResponse = serde_json::from_str(body)
        .map_err(|error| AppError::provider(format!("invalid JSON from provider: {error}")))?;

    match response.status.as_str() {
        "queued" => Ok(VideoStatus::Pending),
        "processing" => Ok(VideoStatus::InProgress),
        "succeeded" => {
            let output = response
                .output
                .ok_or_else(|| AppError::provider("job succeeded but output is missing"))?;

            if output.output_type != "video" {
                return Err(AppError::provider(format!(
                    "unexpected output type: {} (expected video)",
                    output.output_type
                )));
            }

            // Download video from URL
            Ok(VideoStatus::Completed(GeneratedVideo {
                bytes: Vec::new(), // Will be downloaded separately
                model: model.to_string(),
                format: "mp4".to_string(),
            }))
        }
        "failed" => {
            let error_msg = response
                .error
                .map(|e| format!("{}: {}", e.code, e.message))
                .unwrap_or_else(|| "unknown error".to_string());
            Ok(VideoStatus::Failed(error_msg))
        }
        "canceled" => Ok(VideoStatus::Failed("job was canceled".to_string())),
        unknown => Err(AppError::provider(format!(
            "unknown status from provider: {unknown}"
        ))),
    }
}

#[async_trait]
impl VideoProvider for RoyalTechnoVideoProvider {
    async fn request_video(&self, request: &VideoRequest) -> Result<VideoGenerationJob, AppError> {
        if self.config.api_key.is_empty() {
            return Err(AppError::config(
                "no API key configured for RoyalTechno video provider; add one in Settings",
            ));
        }

        let url = format!("{}/v1/jobs", self.config.base_url.trim_end_matches('/'));
        let body = build_request_body(request, &self.config.model)?;

        tracing::info!(
            "requesting video generation via RoyalTechno: model={}, has_image={}, duration_ms={}",
            self.config.model,
            request.source_image_path.is_some(),
            request.target_duration_ms
        );

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                AppError::provider(format!("request to RoyalTechno failed: {error}"))
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|error| {
            AppError::provider(format!("failed to read provider response: {error}"))
        })?;

        if !status.is_success() {
            return Err(AppError::provider(format!(
                "RoyalTechno returned status {status}: {text}"
            )));
        }

        parse_create_response(&text)
    }

    async fn poll_status(&self, job: &VideoGenerationJob) -> Result<VideoStatus, AppError> {
        if self.config.api_key.is_empty() {
            return Err(AppError::config(
                "no API key configured for RoyalTechno video provider",
            ));
        }

        let url = format!(
            "{}/v1/jobs/{}",
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
                "polling RoyalTechno video status: job_id={}, poll_count={}",
                job.operation_id,
                poll_count
            );

            let response = self
                .client
                .get(&url)
                .bearer_auth(&self.config.api_key)
                .send()
                .await
                .map_err(|error| {
                    AppError::provider(format!("failed to poll RoyalTechno status: {error}"))
                })?;

            let status_code = response.status();
            let text = response.text().await.map_err(|error| {
                AppError::provider(format!("failed to read poll response: {error}"))
            })?;

            if !status_code.is_success() {
                return Err(AppError::provider(format!(
                    "RoyalTechno poll returned status {status_code}: {text}"
                )));
            }

            let video_status = parse_status_response(&text, &self.config.model)?;

            match video_status {
                VideoStatus::Completed(ref gen_video) => {
                    tracing::info!(
                        "video generation completed: job_id={}, poll_count={}",
                        job.operation_id,
                        poll_count
                    );

                    // Download the actual video bytes from the URL
                    // Parse the response again to get the URL
                    let response: JobStatusResponse = serde_json::from_str(&text)
                        .map_err(|error| AppError::provider(format!("invalid JSON: {error}")))?;

                    let output = response
                        .output
                        .ok_or_else(|| AppError::provider("succeeded job missing output"))?;

                    tracing::debug!("downloading video from: {}", output.url);

                    let video_bytes = self
                        .client
                        .get(&output.url)
                        .send()
                        .await
                        .map_err(|error| {
                            AppError::provider(format!("failed to download video: {error}"))
                        })?
                        .bytes()
                        .await
                        .map_err(|error| {
                            AppError::provider(format!("failed to read video bytes: {error}"))
                        })?
                        .to_vec();

                    return Ok(VideoStatus::Completed(GeneratedVideo {
                        bytes: video_bytes,
                        model: gen_video.model.clone(),
                        format: gen_video.format.clone(),
                    }));
                }
                VideoStatus::Failed(ref error_msg) => {
                    tracing::error!(
                        "video generation failed: job_id={}, error={}",
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

    #[test]
    fn config_redacts_api_key_in_debug() {
        let config = RoyalTechnoConfig {
            api_key: "rt_live_secret123".to_string(),
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("rt_live_secret123"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn build_request_body_text_to_video() {
        let request = VideoRequest {
            prompt: "A serene mountain landscape".to_string(),
            source_image_path: None,
            target_duration_ms: 5000,
            scene_duration_sec: 6,
        };

        let body = build_request_body(&request, "omni-flash").expect("build body");
        assert_eq!(body["model"], "omni-flash");
        assert_eq!(body["input"]["prompt"], "A serene mountain landscape");
        assert_eq!(body["input"]["duration_sec"], 6);
        assert!(body["input"].get("start_image_url").is_none());
    }

    #[test]
    fn normalizes_omni_flash_duration_to_nearest_supported_value() {
        for (requested, expected) in [(4, 4), (5, 6), (6, 6), (7, 8), (8, 8), (10, 10)] {
            assert_eq!(normalize_duration_sec(requested), expected);
        }
    }

    #[test]
    fn parse_create_response_success() {
        let json = r#"{"id":"job_abc123","status":"queued"}"#;
        let job = parse_create_response(json).expect("parse");
        assert_eq!(job.operation_id, "job_abc123");
    }

    #[test]
    fn parse_status_pending() {
        let json = r#"{"id":"job_abc123","status":"queued"}"#;
        let status = parse_status_response(json, "omni-flash").expect("parse");
        assert!(matches!(status, VideoStatus::Pending));
    }

    #[test]
    fn parse_status_in_progress() {
        let json = r#"{"id":"job_abc123","status":"processing"}"#;
        let status = parse_status_response(json, "omni-flash").expect("parse");
        assert!(matches!(status, VideoStatus::InProgress));
    }

    #[test]
    fn parse_status_failed() {
        let json = r#"{"id":"job_abc123","status":"failed","error":{"code":"content_policy","message":"prompt rejected"}}"#;
        let status = parse_status_response(json, "omni-flash").expect("parse");
        match status {
            VideoStatus::Failed(msg) => {
                assert!(msg.contains("content_policy"));
                assert!(msg.contains("prompt rejected"));
            }
            _ => panic!("expected Failed status"),
        }
    }
}
