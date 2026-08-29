//! Persistence for [`Project`].

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::ids::ProjectId;
use crate::domain::project::Project;
use crate::error::AppError;

pub fn insert(conn: &Connection, project: &Project) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO projects (id, name, topic, style, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            project.id.to_string(),
            project.name,
            project.topic,
            project.style,
            project.created_at,
            project.updated_at,
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn update(conn: &Connection, project: &Project) -> Result<(), AppError> {
    let rows = conn
        .execute(
            "UPDATE projects SET name = ?2, topic = ?3, style = ?4, updated_at = ?5 WHERE id = ?1",
            params![
                project.id.to_string(),
                project.name,
                project.topic,
                project.style,
                project.updated_at,
            ],
        )
        .map_err(map_db_err)?;

    if rows == 0 {
        return Err(AppError::invalid_input(format!(
            "project {} does not exist",
            project.id
        )));
    }
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: ProjectId) -> Result<Option<Project>, AppError> {
    conn.query_row(
        "SELECT id, name, topic, style, created_at, updated_at FROM projects WHERE id = ?1",
        params![id.to_string()],
        row_to_project,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn list_all(conn: &Connection) -> Result<Vec<Project>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, name, topic, style, created_at, updated_at FROM projects ORDER BY created_at")
        .map_err(map_db_err)?;
    let rows = stmt.query_map([], row_to_project).map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

fn row_to_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let id: String = row.get(0)?;
    Ok(Project {
        id: parse_id(&id)?,
        name: row.get(1)?,
        topic: row.get(2)?,
        style: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn parse_id(value: &str) -> rusqlite::Result<ProjectId> {
    uuid::Uuid::parse_str(value)
        .map(ProjectId::from_uuid)
        .map_err(|error| {
            rusqlite::Error::InvalidColumnType(0, error.to_string(), rusqlite::types::Type::Text)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn round_trips_a_project() {
        let db = Database::open_in_memory().expect("open");
        let project = Project::new("My Video", "space", "cinematic").expect("valid project");

        insert(&db.conn(), &project).expect("insert");
        let loaded = get_by_id(&db.conn(), project.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, project);
    }

    #[test]
    fn returns_none_for_missing_project() {
        let db = Database::open_in_memory().expect("open");
        let loaded = get_by_id(&db.conn(), ProjectId::new()).expect("query");
        assert!(loaded.is_none());
    }

    #[test]
    fn updates_existing_project() {
        let db = Database::open_in_memory().expect("open");
        let mut project = Project::new("My Video", "space", "cinematic").expect("valid project");
        insert(&db.conn(), &project).expect("insert");

        project.name = "Renamed".into();
        update(&db.conn(), &project).expect("update");

        let loaded = get_by_id(&db.conn(), project.id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded.name, "Renamed");
    }

    #[test]
    fn update_missing_project_errors() {
        let db = Database::open_in_memory().expect("open");
        let project = Project::new("My Video", "space", "cinematic").expect("valid project");
        let error = update(&db.conn(), &project).expect_err("missing project");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn lists_projects_in_creation_order() {
        let db = Database::open_in_memory().expect("open");
        let first = Project::new("First", "a", "b").expect("valid project");
        let second = Project::new("Second", "a", "b").expect("valid project");
        insert(&db.conn(), &first).expect("insert first");
        insert(&db.conn(), &second).expect("insert second");

        let all = list_all(&db.conn()).expect("list");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, first.id);
        assert_eq!(all[1].id, second.id);
    }
}
