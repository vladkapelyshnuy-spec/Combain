//! Provider-independent contract for text-to-speech synthesis.
//!
//! Concrete adapters (e.g. [`crate::provider::openai_tts_provider::OpenAiTtsProvider`])
//! implement [`TTSProvider`]; domain/commands code depends only on this trait.

use async_trait::async_trait;

use crate::error::AppError;

/// Input for a TTS synthesis request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TTSRequest {
    /// Text to synthesize into speech.
    pub text: String,
    /// Optional voice override; if None, provider uses its default.
    pub voice: Option<String>,
}

/// Raw generated audio bytes, before they are validated and written to disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedAudio {
    pub bytes: Vec<u8>,
    /// Name of the model that produced this audio (e.g. `"tts-1"`), for
    /// [`crate::domain::asset_version::AssetVersion::model`].
    pub model: String,
    /// Voice used for synthesis (e.g. `"alloy"`).
    pub voice: String,
    /// Audio file format extension (e.g. `"mp3"`, `"wav"`).
    pub format: String,
}

/// A TTS backend capable of turning text into audio bytes.
#[async_trait]
pub trait TTSProvider: Send + Sync {
    /// Synthesize speech from text.
    async fn synthesize(&self, request: &TTSRequest) -> Result<GeneratedAudio, AppError>;
}
