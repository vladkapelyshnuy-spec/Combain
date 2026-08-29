//! AssetVersion entity: one generated file for an [`Asset`](crate::domain::asset::Asset).
//!
//! Regenerating an asset creates a new version rather than overwriting the
//! previous one, so users can compare and select among generations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::domain::ids::{AssetId, AssetVersionId};
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetVersion {
    pub id: AssetVersionId,
    pub asset_id: AssetId,
    pub version: u32,
    pub file_path: PathBuf,
    pub provider: String,
    pub model: String,
    pub prompt: String,
    pub created_at: DateTime<Utc>,
    pub selected: bool,
}

impl AssetVersion {
    /// `version` must be >= 1: version numbers are 1-based and increase per asset.
    pub fn new(
        asset_id: AssetId,
        version: u32,
        file_path: impl Into<PathBuf>,
        provider: impl Into<String>,
        model: impl Into<String>,
        prompt: impl Into<String>,
    ) -> Result<Self, AppError> {
        if version == 0 {
            return Err(AppError::invalid_input("asset version must start at 1"));
        }

        Ok(Self {
            id: AssetVersionId::new(),
            asset_id,
            version,
            file_path: file_path.into(),
            provider: provider.into(),
            model: model.into(),
            prompt: prompt.into(),
            created_at: Utc::now(),
            selected: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_first_version_unselected() {
        let version = AssetVersion::new(
            AssetId::new(),
            1,
            "out/img_v1.png",
            "openai",
            "dall-e-3",
            "a cat",
        )
        .expect("valid version");
        assert!(!version.selected);
        assert_eq!(version.version, 1);
    }

    #[test]
    fn rejects_zero_version() {
        let error = AssetVersion::new(
            AssetId::new(),
            0,
            "out/img.png",
            "openai",
            "dall-e-3",
            "a cat",
        )
        .expect_err("zero version");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn serializes_and_deserializes() {
        let version = AssetVersion::new(
            AssetId::new(),
            2,
            "out/img_v2.png",
            "openai",
            "dall-e-3",
            "a dog",
        )
        .expect("valid version");
        let json = serde_json::to_string(&version).expect("serialize");
        let back: AssetVersion = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(version, back);
    }
}
