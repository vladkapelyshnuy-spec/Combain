//! In-memory providers used by command tests so they never hit a network
//! or a paid API: [`TextProvider`], [`ImageProvider`], [`VideoProvider`],
//! [`TTSProvider`].

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

use crate::error::AppError;
use crate::provider::image_provider::{GeneratedImage, ImageProvider, ImageRequest};
use crate::provider::text_provider::{ScriptRequest, ScriptResponse, TextProvider};
use crate::provider::tts_provider::{GeneratedAudio, TTSProvider, TTSRequest};
use crate::provider::video_provider::{
    GeneratedVideo, VideoGenerationJob, VideoProvider, VideoRequest, VideoStatus,
};

pub struct MockTextProvider {
    response: Mutex<Option<Result<ScriptResponse, String>>>,
}

impl MockTextProvider {
    pub fn with_response(response: ScriptResponse) -> Self {
        Self {
            response: Mutex::new(Some(Ok(response))),
        }
    }

    pub fn with_error(message: impl Into<String>) -> Self {
        Self {
            response: Mutex::new(Some(Err(message.into()))),
        }
    }
}

#[async_trait]
impl TextProvider for MockTextProvider {
    async fn generate_script(&self, _request: &ScriptRequest) -> Result<ScriptResponse, AppError> {
        let mut guard = self.response.lock().unwrap_or_else(|p| p.into_inner());
        match guard.take() {
            Some(Ok(response)) => Ok(response),
            Some(Err(message)) => Err(AppError::provider(message)),
            None => Err(AppError::provider(
                "mock provider has no response configured",
            )),
        }
    }
}

pub struct MockImageProvider {
    response: Mutex<Option<Result<GeneratedImage, String>>>,
}

impl MockImageProvider {
    pub fn with_response(response: GeneratedImage) -> Self {
        Self {
            response: Mutex::new(Some(Ok(response))),
        }
    }

    pub fn with_error(message: impl Into<String>) -> Self {
        Self {
            response: Mutex::new(Some(Err(message.into()))),
        }
    }
}

#[async_trait]
impl ImageProvider for MockImageProvider {
    async fn generate_image(&self, _request: &ImageRequest) -> Result<GeneratedImage, AppError> {
        let mut guard = self.response.lock().unwrap_or_else(|p| p.into_inner());
        match guard.take() {
            Some(Ok(response)) => Ok(response),
            Some(Err(message)) => Err(AppError::provider(message)),
            None => Err(AppError::provider(
                "mock provider has no response configured",
            )),
        }
    }
}

pub struct MockVideoProvider {
    response: Mutex<Option<Result<GeneratedVideo, String>>>,
    /// Simulates async delay: poll returns Pending until this counter exceeds threshold.
    poll_count: Arc<Mutex<u32>>,
    polls_until_complete: u32,
}

impl MockVideoProvider {
    pub fn with_response(response: GeneratedVideo) -> Self {
        Self {
            response: Mutex::new(Some(Ok(response))),
            poll_count: Arc::new(Mutex::new(0)),
            polls_until_complete: 2,
        }
    }

    pub fn with_error(message: impl Into<String>) -> Self {
        Self {
            response: Mutex::new(Some(Err(message.into()))),
            poll_count: Arc::new(Mutex::new(0)),
            polls_until_complete: 0,
        }
    }
}

#[async_trait]
impl VideoProvider for MockVideoProvider {
    async fn request_video(&self, _request: &VideoRequest) -> Result<VideoGenerationJob, AppError> {
        Ok(VideoGenerationJob {
            operation_id: "mock_op_123".to_string(),
        })
    }

    async fn poll_status(&self, _job: &VideoGenerationJob) -> Result<VideoStatus, AppError> {
        let mut count = self.poll_count.lock().unwrap_or_else(|p| p.into_inner());
        *count += 1;

        if *count < self.polls_until_complete {
            return Ok(VideoStatus::InProgress);
        }

        let mut guard = self.response.lock().unwrap_or_else(|p| p.into_inner());
        match guard.take() {
            Some(Ok(video)) => Ok(VideoStatus::Completed(video)),
            Some(Err(message)) => Ok(VideoStatus::Failed(message)),
            None => Err(AppError::provider(
                "mock video provider has no response configured",
            )),
        }
    }
}

pub struct MockTtsProvider {
    response: Mutex<Option<Result<GeneratedAudio, String>>>,
}

impl MockTtsProvider {
    pub fn with_response(audio: GeneratedAudio) -> Self {
        Self {
            response: Mutex::new(Some(Ok(audio))),
        }
    }

    pub fn with_error(message: impl Into<String>) -> Self {
        Self {
            response: Mutex::new(Some(Err(message.into()))),
        }
    }
}

#[async_trait]
impl TTSProvider for MockTtsProvider {
    async fn synthesize(&self, _request: &TTSRequest) -> Result<GeneratedAudio, AppError> {
        let mut guard = self.response.lock().unwrap_or_else(|p| p.into_inner());
        match guard.take() {
            Some(Ok(audio)) => Ok(audio),
            Some(Err(message)) => Err(AppError::provider(message)),
            None => Err(AppError::provider(
                "mock TTS provider has no response configured",
            )),
        }
    }
}
