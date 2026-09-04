//! Provider-independent contract for video generation.
//!
//! Concrete adapters (e.g. [`crate::provider::openai_video_provider::OpenAiVideoProvider`])
//! implement [`VideoProvider`]; domain/commands code depends only on this trait.
//!
//! Video generation is asynchronous: the provider returns a job ID immediately,
//! and the caller must poll for completion.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::error::AppError;

/// Input for a video generation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoRequest {
    /// Text prompt describing the desired video content.
    pub prompt: String,
    /// Optional source image path for Image-to-Video generation.
    /// If provided, the video will be generated from this image.
    pub source_image_path: Option<PathBuf>,
    /// Target duration in milliseconds (estimated from audio narration).
    /// Phase 8 TTS will provide the real audio duration; this is a mock estimate.
    pub target_duration_ms: u64,
    /// The duration configured on the Combain scene, in seconds.
    /// Provider adapters may use this for provider-specific duration constraints.
    pub scene_duration_sec: u32,
}

/// Handle for an asynchronous video generation job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoGenerationJob {
    /// Provider-specific operation/job identifier for polling.
    pub operation_id: String,
}

/// Status of a video generation job being polled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoStatus {
    /// Job is queued or initializing.
    Pending,
    /// Job is actively generating video.
    InProgress,
    /// Job completed successfully with the generated video.
    Completed(GeneratedVideo),
    /// Job failed with an error message.
    Failed(String),
}

/// Raw generated video bytes, before they are validated and written to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedVideo {
    pub bytes: Vec<u8>,
    /// Name of the model that produced this video (e.g. `"video-turbo"`), for
    /// [`crate::domain::asset_version::AssetVersion::model`].
    pub model: String,
    /// Video file format extension (e.g. `"mp4"`).
    pub format: String,
}

/// A video-generation backend capable of turning a prompt (and optional image)
/// into video bytes via an asynchronous request/poll pattern.
#[async_trait]
pub trait VideoProvider: Send + Sync {
    /// Initiate an asynchronous video generation request.
    /// Returns immediately with a job handle for polling.
    async fn request_video(&self, request: &VideoRequest) -> Result<VideoGenerationJob, AppError>;

    /// Poll the status of a previously-requested video generation job.
    /// Returns `VideoStatus::Completed` when the video is ready.
    async fn poll_status(&self, job: &VideoGenerationJob) -> Result<VideoStatus, AppError>;
}
