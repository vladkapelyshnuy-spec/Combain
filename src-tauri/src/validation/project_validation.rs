//! Project validation functions.
//!
//! Validates project existence, completeness, and required fields.

use rusqlite::Connection;

use crate::domain::ids::ProjectId;
use crate::domain::project::Project;
use crate::error::AppError;
use crate::repository::project_repository;

/// Validate that a project exists in the database.
///
/// Returns the project if it exists, or an error if not found.
pub fn validate_project_exists(
    conn: &Connection,
    project_id: ProjectId,
) -> Result<Project, AppError> {
    project_repository::get_by_id(conn, project_id)?
        .ok_or_else(|| AppError::invalid_input(format!("project {} does not exist", project_id)))
}

/// Validate that a project has all required fields populated.
///
/// Checks:
/// - name is non-empty
/// - topic is non-empty
/// - style is non-empty
pub fn validate_project_complete(conn: &Connection, project_id: ProjectId) -> Result<(), AppError> {
    let project = validate_project_exists(conn, project_id)?;

    if project.name.trim().is_empty() {
        return Err(AppError::invalid_input(format!(
            "project {} has empty name",
            project_id
        )));
    }

    if project.topic.trim().is_empty() {
        return Err(AppError::invalid_input(format!(
            "project {} has empty topic",
            project_id
        )));
    }

    if project.style.trim().is_empty() {
        return Err(AppError::invalid_input(format!(
            "project {} has empty style",
            project_id
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    #[test]
    fn validate_project_exists_returns_project_if_found() {
        let db = open_db();
        let project = Project::new("Test", "topic", "style").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");

        let result = validate_project_exists(&db.conn(), project.id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, project.id);
    }

    #[test]
    fn validate_project_exists_returns_error_if_not_found() {
        let db = open_db();
        let project_id = ProjectId::new();

        let result = validate_project_exists(&db.conn(), project_id);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            crate::error::ErrorCode::InvalidInput
        );
    }

    #[test]
    fn validate_project_complete_passes_for_valid_project() {
        let db = open_db();
        let project = Project::new("Test Project", "AI topic", "cinematic").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");

        let result = validate_project_complete(&db.conn(), project.id);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_project_complete_fails_for_empty_name() {
        let db = open_db();
        let mut project = Project::new("Test", "topic", "style").expect("project");
        project.name = "".to_string();
        project_repository::insert(&db.conn(), &project).expect("insert");

        let result = validate_project_complete(&db.conn(), project.id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty name"));
    }

    #[test]
    fn validate_project_complete_fails_for_empty_topic() {
        let db = open_db();
        let mut project = Project::new("Test", "topic", "style").expect("project");
        project.topic = "".to_string();
        project_repository::insert(&db.conn(), &project).expect("insert");

        let result = validate_project_complete(&db.conn(), project.id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty topic"));
    }

    #[test]
    fn validate_project_complete_fails_for_empty_style() {
        let db = open_db();
        let mut project = Project::new("Test", "topic", "style").expect("project");
        project.style = "".to_string();
        project_repository::insert(&db.conn(), &project).expect("insert");

        let result = validate_project_complete(&db.conn(), project.id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("empty style"));
    }
}
