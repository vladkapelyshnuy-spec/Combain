//! Persistence for [`Timeline`].

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::ids::{ProjectId, TimelineId};
use crate::domain::timeline::Timeline;
use crate::error::AppError;

pub fn insert(conn: &Connection, timeline: &Timeline) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO timelines (id, project_id) VALUES (?1, ?2)",
        params![timeline.id.to_string(), timeline.project_id.to_string()],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: TimelineId) -> Result<Option<Timeline>, AppError> {
    conn.query_row(
        "SELECT id, project_id FROM timelines WHERE id = ?1",
        params![id.to_string()],
        row_to_timeline,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn get_by_project(
    conn: &Connection,
    project_id: ProjectId,
) -> Result<Option<Timeline>, AppError> {
    conn.query_row(
        "SELECT id, project_id FROM timelines WHERE project_id = ?1",
        params![project_id.to_string()],
        row_to_timeline,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

fn row_to_timeline(row: &rusqlite::Row<'_>) -> rusqlite::Result<Timeline> {
    let id: String = row.get(0)?;
    let project_id: String = row.get(1)?;

    Ok(Timeline {
        id: parse_uuid_column(&id).map(TimelineId::from_uuid)?,
        project_id: parse_uuid_column(&project_id).map(ProjectId::from_uuid)?,
    })
}

fn parse_uuid_column(value: &str) -> rusqlite::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(value).map_err(|error| {
        rusqlite::Error::InvalidColumnType(0, error.to_string(), rusqlite::types::Type::Text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::project::Project;
    use crate::repository::project_repository;

    fn seed_project(db: &Database) -> ProjectId {
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        project.id
    }

    #[test]
    fn round_trips_a_timeline() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let timeline = Timeline::new(project_id);

        insert(&db.conn(), &timeline).expect("insert");
        let loaded = get_by_id(&db.conn(), timeline.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, timeline);
    }

    #[test]
    fn finds_timeline_by_project() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let timeline = Timeline::new(project_id);
        insert(&db.conn(), &timeline).expect("insert");

        let loaded = get_by_project(&db.conn(), project_id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded.id, timeline.id);
    }
}
