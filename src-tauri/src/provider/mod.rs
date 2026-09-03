//! AI provider integrations: provider-independent traits (see [`text_provider`])
//! plus concrete adapters. Domain and command code must depend only on the
//! traits, never on a specific adapter, so providers can be swapped without
//! touching business logic.
#![allow(unused_imports)]

pub mod image_provider;
pub mod openai_image_provider;
pub mod openai_text_provider;
pub mod openai_video_provider;
pub mod text_provider;
pub mod video_provider;

#[cfg(test)]
pub mod mock;

pub use image_provider::{GeneratedImage, ImageProvider, ImageRequest};
pub use openai_image_provider::{OpenAiImageConfig, OpenAiImageProvider};
pub use openai_text_provider::{OpenAiConfig, OpenAiTextProvider};
pub use openai_video_provider::{OpenAiVideoConfig, OpenAiVideoProvider};
pub use text_provider::{
    validate_script_response, GeneratedScene, ScriptRequest, ScriptResponse, TextProvider,
};
pub use video_provider::{
    GeneratedVideo, VideoGenerationJob, VideoProvider, VideoRequest, VideoStatus,
};
