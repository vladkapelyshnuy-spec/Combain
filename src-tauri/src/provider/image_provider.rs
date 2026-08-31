//! Provider-independent contract for image generation.
//!
//! Concrete adapters (e.g. [`crate::provider::openai_image_provider::OpenAiImageProvider`])
//! implement [`ImageProvider`]; domain/commands code depends only on this trait.

use async_trait::async_trait;

use crate::error::AppError;

/// Input for an image generation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRequest {
    pub prompt: String,
}

/// Raw generated image bytes, before they are validated and written to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    /// Name of the model that produced this image (e.g. `"dall-e-3"`), for
    /// [`crate::domain::asset_version::AssetVersion::model`].
    pub model: String,
}

/// An image-generation backend capable of turning a prompt into image bytes.
#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate_image(&self, request: &ImageRequest) -> Result<GeneratedImage, AppError>;
}
