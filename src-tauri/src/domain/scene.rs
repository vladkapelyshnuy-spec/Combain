//! Scene entity: a single narrative beat within a project's script.

use serde::{Deserialize, Serialize};

use crate::domain::ids::{ProjectId, SceneId};
use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SceneStatus {
    Draft,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub project_id: ProjectId,
    pub order: u32,
    pub title: String,
    pub narration: String,
    pub visual_prompt: String,
    pub video_prompt: String,
    /// Duration in seconds.
    pub duration: u32,
    pub status: SceneStatus,
}

impl Scene {
    pub fn new(
        project_id: ProjectId,
        order: u32,
        title: impl Into<String>,
    ) -> Result<Self, AppError> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(AppError::invalid_input("scene title must not be empty"));
        }

        Ok(Self {
            id: SceneId::new(),
            project_id,
            order,
            title,
            narration: String::new(),
            visual_prompt: String::new(),
            video_prompt: String::new(),
            duration: 0,
            status: SceneStatus::Draft,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_scene_with_draft_status() {
        let scene = Scene::new(ProjectId::new(), 1, "Intro").expect("valid scene");
        assert_eq!(scene.status, SceneStatus::Draft);
        assert_eq!(scene.order, 1);
    }

    #[test]
    fn rejects_empty_title() {
        let error = Scene::new(ProjectId::new(), 1, "  ").expect_err("empty title");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn serializes_and_deserializes() {
        let scene = Scene::new(ProjectId::new(), 1, "Intro").expect("valid scene");
        let json = serde_json::to_string(&scene).expect("serialize");
        let back: Scene = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(scene, back);
    }

    #[test]
    fn status_uses_screaming_snake_case() {
        let json = serde_json::to_string(&SceneStatus::Ready).expect("serialize");
        assert_eq!(json, "\"READY\"");
    }
}
