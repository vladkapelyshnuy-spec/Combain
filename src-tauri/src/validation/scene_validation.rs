//! Scene validation functions.
//!
//! Validates scene existence, ordering, and completeness.

use rusqlite::Connection;

use crate::domain::ids::ProjectId;
use crate::domain::scene::Scene;
use crate::error::AppError;
use crate::repository::scene_repository;

/// Validate that a project has at least one scene.
///
/// Returns all scenes for the project, or an error if none exist.
pub fn validate_scenes_exist(conn: &Connection, project_id: ProjectId) -> Result<Vec<Scene>, AppError> {
    let scenes = scene_repository::list_by_project(conn, project_id)?;

    if scenes.is_empty() {
        return Err(AppError::invalid_input(format!(
            "project {} has no scenes",
            project_id
        )));
    }

    Ok(scenes)
}

/// Validate that scenes have sequential ordering starting from 1.
///
/// Checks:
/// - First scene has order = 1
/// - Each subsequent scene increments by 1
/// - No gaps in ordering
pub fn validate_scene_order(scenes: &[Scene]) -> Result<(), AppError> {
    if scenes.is_empty() {
        return Err(AppError::invalid_input("cannot validate order of empty scene list"));
    }

    // Scenes should already be ordered by the repository, but verify
    let mut expected_order = 1;
    for scene in scenes {
        if scene.order != expected_order {
            return Err(AppError::invalid_input(format!(
                "scene ordering is invalid: expected order {}, found {}",
                expected_order, scene.order
            )));
        }
        expected_order += 1;
    }

    Ok(())
}

/// Validate that the scene count matches the expected value.
///
/// Useful for verifying script generation produced the expected number of scenes.
pub fn validate_scene_count(scenes: &[Scene], expected: usize) -> Result<(), AppError> {
    if scenes.len() != expected {
        return Err(AppError::invalid_input(format!(
            "expected {} scenes, found {}",
            expected,
            scenes.len()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::project::Project;
    use crate::repository::project_repository;

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    fn seed_project(db: &Database) -> ProjectId {
        let project = Project::new("Test", "topic", "style").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");
        project.id
    }

    #[test]
    fn validate_scenes_exist_returns_scenes_if_present() {
        let db = open_db();
        let project_id = seed_project(&db);

        let scene1 = Scene::new(project_id, 1, "Scene 1").expect("scene");
        let scene2 = Scene::new(project_id, 2, "Scene 2").expect("scene");
        scene_repository::insert(&db.conn(), &scene1).expect("insert");
        scene_repository::insert(&db.conn(), &scene2).expect("insert");

        let result = validate_scenes_exist(&db.conn(), project_id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn validate_scenes_exist_returns_error_if_no_scenes() {
        let db = open_db();
        let project_id = seed_project(&db);

        let result = validate_scenes_exist(&db.conn(), project_id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("no scenes"));
    }

    #[test]
    fn validate_scene_order_passes_for_sequential_scenes() {
        let db = open_db();
        let project_id = seed_project(&db);

        let scene1 = Scene::new(project_id, 1, "Scene 1").expect("scene");
        let scene2 = Scene::new(project_id, 2, "Scene 2").expect("scene");
        let scene3 = Scene::new(project_id, 3, "Scene 3").expect("scene");
        scene_repository::insert(&db.conn(), &scene1).expect("insert");
        scene_repository::insert(&db.conn(), &scene2).expect("insert");
        scene_repository::insert(&db.conn(), &scene3).expect("insert");

        let scenes = scene_repository::list_by_project(&db.conn(), project_id).expect("list");
        let result = validate_scene_order(&scenes);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_scene_order_fails_for_gap_in_ordering() {
        let project_id = ProjectId::new();

        // Manually construct scenes with a gap (order 1, 3 - missing 2)
        let scene1 = Scene::new(project_id, 1, "Scene 1").expect("scene");
        let scene3 = Scene::new(project_id, 3, "Scene 3").expect("scene");

        let scenes = vec![scene1, scene3];
        let result = validate_scene_order(&scenes);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("expected order 2"));
    }

    #[test]
    fn validate_scene_order_fails_for_scenes_not_starting_at_1() {
        let project_id = ProjectId::new();

        let scene2 = Scene::new(project_id, 2, "Scene 2").expect("scene");
        let scene3 = Scene::new(project_id, 3, "Scene 3").expect("scene");

        let scenes = vec![scene2, scene3];
        let result = validate_scene_order(&scenes);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("expected order 1"));
    }

    #[test]
    fn validate_scene_order_fails_for_empty_list() {
        let scenes: Vec<Scene> = vec![];
        let result = validate_scene_order(&scenes);
        assert!(result.is_err());
    }

    #[test]
    fn validate_scene_count_passes_for_matching_count() {
        let project_id = ProjectId::new();
        let scenes = vec![
            Scene::new(project_id, 1, "S1").expect("scene"),
            Scene::new(project_id, 2, "S2").expect("scene"),
            Scene::new(project_id, 3, "S3").expect("scene"),
        ];

        let result = validate_scene_count(&scenes, 3);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_scene_count_fails_for_mismatched_count() {
        let project_id = ProjectId::new();
        let scenes = vec![
            Scene::new(project_id, 1, "S1").expect("scene"),
            Scene::new(project_id, 2, "S2").expect("scene"),
        ];

        let result = validate_scene_count(&scenes, 3);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("expected 3 scenes, found 2"));
    }
}
