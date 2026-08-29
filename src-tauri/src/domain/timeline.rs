//! Timeline entity: the render source of truth for a project.
//!
//! The timeline itself is a lightweight container; its ordered content
//! lives in [`TimelineClip`](crate::domain::timeline_clip::TimelineClip) rows
//! referencing it by [`TimelineId`].

use serde::{Deserialize, Serialize};

use crate::domain::ids::{ProjectId, TimelineId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    pub id: TimelineId,
    pub project_id: ProjectId,
}

impl Timeline {
    pub fn new(project_id: ProjectId) -> Self {
        Self {
            id: TimelineId::new(),
            project_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_timeline_for_project() {
        let project_id = ProjectId::new();
        let timeline = Timeline::new(project_id);
        assert_eq!(timeline.project_id, project_id);
    }

    #[test]
    fn serializes_and_deserializes() {
        let timeline = Timeline::new(ProjectId::new());
        let json = serde_json::to_string(&timeline).expect("serialize");
        let back: Timeline = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(timeline, back);
    }
}
