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
}
