//! In-memory [`TextProvider`]/[`ImageProvider`] used by command tests so they
//! never hit a network or a paid API.

use async_trait::async_trait;
use std::sync::Mutex;

use crate::error::AppError;
use crate::provider::image_provider::{GeneratedImage, ImageProvider, ImageRequest};
use crate::provider::text_provider::{ScriptRequest, ScriptResponse, TextProvider};

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
