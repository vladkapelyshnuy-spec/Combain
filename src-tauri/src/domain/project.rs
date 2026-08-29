//! Project entity: the top-level container for a Combain Studio video.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::ids::ProjectId;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub topic: String,
    pub style: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    /// Create a new project. `name` must not be empty; `topic` and `style`
    /// may be empty (filled in later by the AI Director phase).
    pub fn new(
        name: impl Into<String>,
        topic: impl Into<String>,
        style: impl Into<String>,
    ) -> Result<Self, AppError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(AppError::invalid_input("project name must not be empty"));
        }

        let now = Utc::now();
        Ok(Self {
            id: ProjectId::new(),
            name,
            topic: topic.into(),
            style: style.into(),
            created_at: now,
            updated_at: now,
        })
    }

    /// Replace `name`/`topic`/`style` in place and bump `updated_at`.
    /// `name` must not be empty.
    pub fn update(
        &mut self,
        name: impl Into<String>,
        topic: impl Into<String>,
        style: impl Into<String>,
    ) -> Result<(), AppError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(AppError::invalid_input("project name must not be empty"));
        }

        self.name = name;
        self.topic = topic.into();
        self.style = style.into();
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_project_with_generated_id() {
        let project = Project::new("My Video", "space", "cinematic").expect("valid project");
        assert_eq!(project.name, "My Video");
        assert_eq!(project.topic, "space");
    }

    #[test]
    fn rejects_empty_name() {
        let error = Project::new("   ", "space", "cinematic").expect_err("empty name");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn serializes_and_deserializes() {
        let project = Project::new("My Video", "space", "cinematic").expect("valid project");
        let json = serde_json::to_string(&project).expect("serialize");
        let back: Project = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(project, back);
    }

    #[test]
    fn update_replaces_fields_and_bumps_updated_at() {
        let mut project = Project::new("My Video", "space", "cinematic").expect("valid project");
        let created_at = project.created_at;

        project
            .update("Renamed", "ocean", "documentary")
            .expect("valid update");

        assert_eq!(project.name, "Renamed");
        assert_eq!(project.topic, "ocean");
        assert_eq!(project.style, "documentary");
        assert_eq!(project.created_at, created_at);
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut project = Project::new("My Video", "space", "cinematic").expect("valid project");
        let error = project
            .update("   ", "ocean", "documentary")
            .expect_err("empty name");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }
}
