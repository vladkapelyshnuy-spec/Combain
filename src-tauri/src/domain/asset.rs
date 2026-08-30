//! Asset entity: a generated media slot attached to a scene.
//!
//! An `Asset` itself holds no file data; its [`AssetVersion`](crate::domain::asset_version::AssetVersion)
//! children hold the actual generated files and provider metadata.

use serde::{Deserialize, Serialize};

use crate::domain::ids::{AssetId, SceneId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetType {
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub scene_id: SceneId,
    pub asset_type: AssetType,
}

impl Asset {
    pub fn new(scene_id: SceneId, asset_type: AssetType) -> Self {
        Self {
            id: AssetId::new(),
            scene_id,
            asset_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_asset_with_generated_id() {
        let asset = Asset::new(SceneId::new(), AssetType::Image);
        assert_eq!(asset.asset_type, AssetType::Image);
    }

    #[test]
    fn serializes_and_deserializes() {
        let asset = Asset::new(SceneId::new(), AssetType::Video);
        let json = serde_json::to_string(&asset).expect("serialize");
        let back: Asset = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(asset, back);
    }

    #[test]
    fn asset_type_uses_screaming_snake_case() {
        let json = serde_json::to_string(&AssetType::Audio).expect("serialize");
        assert_eq!(json, "\"AUDIO\"");
    }
}
