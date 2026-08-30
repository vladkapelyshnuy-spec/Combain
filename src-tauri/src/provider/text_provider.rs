//! Provider-independent contract for text generation (script writing).
//!
//! Concrete adapters (e.g. [`crate::provider::openai_text_provider::OpenAiTextProvider`])
//! implement [`TextProvider`]; domain/commands code depends only on this trait.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Input for a script generation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptRequest {
    pub topic: String,
    pub style: String,
    pub duration_seconds: u32,
    pub language: String,
}

/// One scene as proposed by the AI Director, before it becomes a [`crate::domain::scene::Scene`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedScene {
    pub number: u32,
    pub title: String,
    pub narration: String,
    pub visual_prompt: String,
    pub video_prompt: String,
    pub duration: u32,
}

/// Structured script output: a title plus an ordered list of scenes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptResponse {
    pub title: String,
    pub scenes: Vec<GeneratedScene>,
}

/// Validate a provider's structured response before it is allowed to touch storage.
///
/// A response failing this check must not cause any DB writes.
pub fn validate_script_response(response: &ScriptResponse) -> Result<(), AppError> {
    if response.title.trim().is_empty() {
        return Err(AppError::provider("script response has an empty title"));
    }
    if response.scenes.is_empty() {
        return Err(AppError::provider("script response has no scenes"));
    }
    for scene in &response.scenes {
        if scene.title.trim().is_empty() {
            return Err(AppError::provider(format!(
                "scene {} has an empty title",
                scene.number
            )));
        }
        if scene.narration.trim().is_empty() {
            return Err(AppError::provider(format!(
                "scene {} has empty narration",
                scene.number
            )));
        }
        if scene.duration == 0 {
            return Err(AppError::provider(format!(
                "scene {} has zero duration",
                scene.number
            )));
        }
    }
    Ok(())
}

/// A text-generation backend capable of turning a topic into a structured script.
#[async_trait]
pub trait TextProvider: Send + Sync {
    async fn generate_script(&self, request: &ScriptRequest) -> Result<ScriptResponse, AppError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_response() -> ScriptResponse {
        ScriptResponse {
            title: "5 facts about space".into(),
            scenes: vec![GeneratedScene {
                number: 1,
                title: "Intro".into(),
                narration: "Hello".into(),
                visual_prompt: "cinematic space scene".into(),
                video_prompt: "slow camera movement".into(),
                duration: 5,
            }],
        }
    }

    #[test]
    fn accepts_a_valid_response() {
        assert!(validate_script_response(&valid_response()).is_ok());
    }

    #[test]
    fn rejects_empty_title() {
        let mut response = valid_response();
        response.title = "  ".into();
        let error = validate_script_response(&response).expect_err("empty title");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn rejects_no_scenes() {
        let mut response = valid_response();
        response.scenes.clear();
        let error = validate_script_response(&response).expect_err("no scenes");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn rejects_scene_with_empty_narration() {
        let mut response = valid_response();
        response.scenes[0].narration = "".into();
        let error = validate_script_response(&response).expect_err("empty narration");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn rejects_scene_with_zero_duration() {
        let mut response = valid_response();
        response.scenes[0].duration = 0;
        let error = validate_script_response(&response).expect_err("zero duration");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }
}
