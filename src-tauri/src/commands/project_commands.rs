//! IPC commands for [`Project`](crate::domain::project::Project).
//!
//! Each `#[tauri::command]` is a thin wrapper around a plain function that
//! takes `&Connection` directly, so the logic can be unit tested without a
//! running Tauri app (`State` cannot be constructed outside one).

use rusqlite::Connection;
use tauri::State;

use crate::db::Database;
use crate::domain::ids::ProjectId;
use crate::domain::project::Project;
use crate::error::AppError;
use crate::repository::project_repository;

fn create_project_impl(
    conn: &Connection,
    name: String,
    topic: String,
    style: String,
) -> Result<Project, AppError> {
    let project = Project::new(name, topic, style)?;
    project_repository::insert(conn, &project)?;
    tracing::info!(project_id = %project.id, "project created");
    Ok(project)
}

fn update_project_impl(
    conn: &Connection,
    project_id: ProjectId,
    name: String,
    topic: String,
    style: String,
) -> Result<Project, AppError> {
    let mut project = project_repository::get_by_id(conn, project_id)?
        .ok_or_else(|| AppError::invalid_input(format!("project {project_id} does not exist")))?;

    project.update(name, topic, style)?;
    project_repository::update(conn, &project)?;
    tracing::info!(project_id = %project.id, "project updated");
    Ok(project)
}

/// Input: `name`, `topic`, `style`
/// Output: the created [`Project`]
/// Error: [`AppError::InvalidInput`] if `name` is empty; [`AppError::Db`] on storage failure
/// Side effects: inserts a new row into `projects`
#[tauri::command]
pub fn create_project(
    db: State<'_, Database>,
    name: String,
    topic: String,
    style: String,
) -> Result<Project, AppError> {
    create_project_impl(&db.conn(), name, topic, style)
}

/// Input: `project_id`
/// Output: the [`Project`] if it exists, `None` otherwise
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn get_project(
    db: State<'_, Database>,
    project_id: ProjectId,
) -> Result<Option<Project>, AppError> {
    project_repository::get_by_id(&db.conn(), project_id)
}

/// Input: none
/// Output: all [`Project`]s ordered by creation time
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn list_projects(db: State<'_, Database>) -> Result<Vec<Project>, AppError> {
    project_repository::list_all(&db.conn())
}

/// Input: `project_id`, `name`, `topic`, `style`
/// Output: the updated [`Project`]
/// Error: [`AppError::InvalidInput`] if `name` is empty or `project_id` is unknown; [`AppError::Db`] on storage failure
/// Side effects: updates the matching row in `projects`
#[tauri::command]
pub fn update_project(
    db: State<'_, Database>,
    project_id: ProjectId,
    name: String,
    topic: String,
    style: String,
) -> Result<Project, AppError> {
    update_project_impl(&db.conn(), project_id, name, topic, style)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> Database {
        Database::open_in_memory().expect("open in-memory db")
    }

    #[test]
    fn creates_a_project() {
        let db = open_db();
        let project = create_project_impl(
            &db.conn(),
            "My Video".into(),
            "space".into(),
            "cinematic".into(),
        )
        .expect("create");

        let loaded = project_repository::get_by_id(&db.conn(), project.id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded, project);
    }

    #[test]
    fn create_rejects_empty_name() {
        let db = open_db();
        let error =
            create_project_impl(&db.conn(), "  ".into(), "space".into(), "cinematic".into())
                .expect_err("empty name");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn lists_created_projects() {
        let db = open_db();
        create_project_impl(&db.conn(), "First".into(), "a".into(), "b".into()).expect("create");
        create_project_impl(&db.conn(), "Second".into(), "a".into(), "b".into()).expect("create");

        let all = project_repository::list_all(&db.conn()).expect("list");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn get_returns_none_for_missing_project() {
        let db = open_db();
        let loaded = project_repository::get_by_id(&db.conn(), ProjectId::new()).expect("query");
        assert!(loaded.is_none());
    }

    #[test]
    fn updates_an_existing_project() {
        let db = open_db();
        let project = create_project_impl(
            &db.conn(),
            "My Video".into(),
            "space".into(),
            "cinematic".into(),
        )
        .expect("create");

        let updated = update_project_impl(
            &db.conn(),
            project.id,
            "Renamed".into(),
            "ocean".into(),
            "documentary".into(),
        )
        .expect("update");

        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.topic, "ocean");
        assert_eq!(updated.style, "documentary");
    }

    #[test]
    fn update_missing_project_errors() {
        let db = open_db();
        let error = update_project_impl(
            &db.conn(),
            ProjectId::new(),
            "Renamed".into(),
            "ocean".into(),
            "documentary".into(),
        )
        .expect_err("missing project");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }
}
