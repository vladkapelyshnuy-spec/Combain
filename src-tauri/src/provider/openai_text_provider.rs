//! OpenAI-compatible [`TextProvider`] adapter.
//!
//! Talks to any server implementing the OpenAI Chat Completions API shape
//! (official OpenAI, or a compatible proxy). The base URL and model are
//! configurable so this adapter is not tied to `api.openai.com`.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::provider::text_provider::{ScriptRequest, ScriptResponse, TextProvider};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// Connection settings for the OpenAI-compatible adapter.
///
/// `api_key` is read from `OPENAI_API_KEY` and never persisted to config
/// files, logs, or serialized output (see the custom [`std::fmt::Debug`] impl).
#[derive(Clone)]
pub struct OpenAiConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for OpenAiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiConfig")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl OpenAiConfig {
    /// Load settings from the environment. Missing `OPENAI_BASE_URL`/`OPENAI_MODEL`
    /// fall back to sane defaults; a missing `OPENAI_API_KEY` is not an error here,
    /// it only surfaces once a request is actually attempted.
    pub fn from_env() -> Self {
        let api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let base_url = std::env::var("OPENAI_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = std::env::var("OPENAI_MODEL")
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

pub struct OpenAiTextProvider {
    client: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiTextProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config,
        }
    }
}

fn system_prompt() -> &'static str {
    "You are an AI Video Director. Given a topic, produce a short-video script as \
     strict JSON matching this shape: {\"title\": string, \"scenes\": [{\"number\": \
     integer, \"title\": string, \"narration\": string, \"visual_prompt\": string, \
     \"video_prompt\": string, \"duration\": integer}]}. `duration` is per-scene \
     seconds. Respond with JSON only, no prose, no markdown fences."
}

/// Build the Chat Completions request body. Pure/testable without network access.
pub fn build_request_body(request: &ScriptRequest, model: &str) -> Value {
    let user_prompt = format!(
        "Topic: {}\nStyle: {}\nTotal duration: {} seconds\nLanguage: {}\n\
         Split the script into scenes whose durations sum to approximately the total duration.",
        request.topic, request.style, request.duration_seconds, request.language
    );

    json!({
        "model": model,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system_prompt() },
            { "role": "user", "content": user_prompt }
        ]
    })
}

/// Extract and parse the structured script from a Chat Completions response body.
/// Pure/testable without network access.
pub fn parse_response(body: &str) -> Result<ScriptResponse, AppError> {
    let envelope: Value = serde_json::from_str(body)
        .map_err(|error| AppError::provider(format!("invalid JSON from provider: {error}")))?;

    let content = envelope
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::provider("provider response is missing choices[0].message.content")
        })?;

    serde_json::from_str::<ScriptResponse>(content).map_err(|error| {
        AppError::provider(format!(
            "provider content is not valid script JSON: {error}"
        ))
    })
}

#[async_trait]
impl TextProvider for OpenAiTextProvider {
    async fn generate_script(&self, request: &ScriptRequest) -> Result<ScriptResponse, AppError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AppError::config("OPENAI_API_KEY is not set; cannot call the text provider")
        })?;

        let url = format!(
            "{}/chat/completions",
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
                AppError::provider(format!("request to text provider failed: {error}"))
            })?;

        let status = response.status();
        let text = response.text().await.map_err(|error| {
            AppError::provider(format!("failed to read provider response: {error}"))
        })?;

        if !status.is_success() {
            return Err(AppError::provider(format!(
                "text provider returned status {status}: {text}"
            )));
        }

        parse_response(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ScriptRequest {
        ScriptRequest {
            topic: "space facts".into(),
            style: "cinematic".into(),
            duration_seconds: 30,
            language: "en".into(),
        }
    }

    #[test]
    fn debug_redacts_api_key() {
        let config = OpenAiConfig {
            api_key: Some("sk-secret".into()),
            base_url: DEFAULT_BASE_URL.into(),
            model: DEFAULT_MODEL.into(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("sk-secret"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_body_includes_model_and_topic() {
        let body = build_request_body(&sample_request(), "gpt-4o-mini");
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["response_format"]["type"], "json_object");
        let user_message = body["messages"][1]["content"].as_str().unwrap();
        assert!(user_message.contains("space facts"));
        assert!(user_message.contains("30 seconds"));
    }

    #[test]
    fn parses_valid_chat_completions_envelope() {
        let script = json!({
            "title": "5 facts about space",
            "scenes": [{
                "number": 1,
                "title": "Intro",
                "narration": "Hello",
                "visual_prompt": "a nebula",
                "video_prompt": "slow pan",
                "duration": 5
            }]
        });
        let envelope = json!({
            "choices": [{ "message": { "content": script.to_string() } }]
        });

        let parsed = parse_response(&envelope.to_string()).expect("parse");
        assert_eq!(parsed.title, "5 facts about space");
        assert_eq!(parsed.scenes.len(), 1);
    }

    #[test]
    fn rejects_envelope_missing_content() {
        let envelope = json!({ "choices": [] });
        let error = parse_response(&envelope.to_string()).expect_err("missing content");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn rejects_content_that_is_not_script_json() {
        let envelope = json!({
            "choices": [{ "message": { "content": "not json" } }]
        });
        let error = parse_response(&envelope.to_string()).expect_err("bad content");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }
}
