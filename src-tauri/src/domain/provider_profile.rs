//! ProviderProfile entity: a named configuration for an external AI provider
//! (text/image/video/tts), scoped to one purpose. The API key itself is never
//! part of this struct — it lives in the OS credential store, addressed by
//! `credential_alias` (see [`crate::secrets`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::ids::ProviderProfileId;
use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderPurpose {
    Text,
    Image,
    Video,
    Tts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: ProviderProfileId,
    pub purpose: ProviderPurpose,
    pub profile_name: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub voice: Option<String>,
    pub is_default: bool,
    /// Opaque key used to look up the API key in the OS credential store.
    /// Never the secret itself.
    pub credential_alias: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProviderProfile {
    /// Create a new profile. `profile_name`, `provider`, and `model` must not
    /// be empty. `credential_alias` must be unique across all profiles (the
    /// repository enforces this at the storage layer).
    pub fn new(
        purpose: ProviderPurpose,
        profile_name: impl Into<String>,
        provider: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
        voice: Option<String>,
        is_default: bool,
    ) -> Result<Self, AppError> {
        let profile_name = profile_name.into();
        let provider = provider.into();
        let model = model.into();
        validate_required(&profile_name, "profile_name")?;
        validate_required(&provider, "provider")?;
        validate_required(&model, "model")?;

        let now = Utc::now();
        Ok(Self {
            id: ProviderProfileId::new(),
            purpose,
            profile_name,
            provider,
            base_url,
            model,
            voice,
            is_default,
            credential_alias: uuid::Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Replace the editable fields in place and bump `updated_at`. `purpose`
    /// and `credential_alias` are immutable after creation.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        profile_name: impl Into<String>,
        provider: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
        voice: Option<String>,
        is_default: bool,
    ) -> Result<(), AppError> {
        let profile_name = profile_name.into();
        let provider = provider.into();
        let model = model.into();
        validate_required(&profile_name, "profile_name")?;
        validate_required(&provider, "provider")?;
        validate_required(&model, "model")?;

        self.profile_name = profile_name;
        self.provider = provider;
        self.base_url = base_url;
        self.model = model;
        self.voice = voice;
        self.is_default = is_default;
        self.updated_at = Utc::now();
        Ok(())
    }
}

fn validate_required(value: &str, field: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::invalid_input(format!(
            "{field} must not be empty"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(name: &str) -> Result<ProviderProfile, AppError> {
        ProviderProfile::new(
            ProviderPurpose::Text,
            name,
            "openai",
            Some("https://api.openai.com/v1".into()),
            "gpt-4o",
            None,
            false,
        )
    }

    #[test]
    fn creates_profile_with_generated_alias() {
        let profile = make("Primary").expect("valid profile");
        assert_eq!(profile.profile_name, "Primary");
        assert!(!profile.credential_alias.is_empty());
        assert!(!profile.is_default);
    }

    #[test]
    fn rejects_empty_profile_name() {
        let error = make("   ").expect_err("empty name");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn serializes_and_deserializes() {
        let profile = make("Primary").expect("valid profile");
        let json = serde_json::to_string(&profile).expect("serialize");
        let back: ProviderProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(profile, back);
    }

    #[test]
    fn purpose_uses_screaming_snake_case() {
        let json = serde_json::to_string(&ProviderPurpose::Tts).expect("serialize");
        assert_eq!(json, "\"TTS\"");
    }

    #[test]
    fn update_replaces_fields_and_bumps_updated_at() {
        let mut profile = make("Primary").expect("valid profile");
        let alias = profile.credential_alias.clone();
        let created_at = profile.created_at;

        profile
            .update("Renamed", "azure-openai", None, "gpt-4o-mini", None, true)
            .expect("valid update");

        assert_eq!(profile.profile_name, "Renamed");
        assert_eq!(profile.provider, "azure-openai");
        assert!(profile.base_url.is_none());
        assert!(profile.is_default);
        assert_eq!(profile.credential_alias, alias);
        assert_eq!(profile.created_at, created_at);
    }

    #[test]
    fn update_rejects_empty_model() {
        let mut profile = make("Primary").expect("valid profile");
        let error = profile
            .update("Primary", "openai", None, "  ", None, false)
            .expect_err("empty model");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }
}
