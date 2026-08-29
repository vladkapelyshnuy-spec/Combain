//! Persistence for [`Scene`].

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::ids::{ProjectId, SceneId};
use crate::domain::scene::{Scene, SceneStatus};
use crate::error::AppError;

pub fn insert(conn: &Connection, scene: &Scene) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO scenes
            (id, project_id, order_index, title, narration, visual_prompt, video_prompt, duration, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            scene.id.to_string(),
            scene.project_id.to_string(),
            scene.order,
            scene.title,
            scene.narration,
            scene.visual_prompt,
            scene.video_prompt,
            scene.duration,
            status_to_str(scene.status),
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn update(conn: &Connection, scene: &Scene) -> Result<(), AppError> {
    let rows = conn
        .execute(
            "UPDATE scenes SET order_index = ?2, title = ?3, narration = ?4, visual_prompt = ?5,
                video_prompt = ?6, duration = ?7, status = ?8
             WHERE id = ?1",
            params![
                scene.id.to_string(),
                scene.order,
                scene.title,
                scene.narration,
                scene.visual_prompt,
                scene.video_prompt,
                scene.duration,
                status_to_str(scene.status),
            ],
        )
        .map_err(map_db_err)?;

    if rows == 0 {
        return Err(AppError::invalid_input(format!(
            "scene {} does not exist",
            scene.id
        )));
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: SceneId) -> Result<(), AppError> {
    conn.execute("DELETE FROM scenes WHERE id = ?1", params![id.to_string()])
        .map_err(map_db_err)?;
    Ok(())
}

/// Set `order_index` for each scene to its position in `ordered_ids`.
/// Every ID must already belong to `project_id`; the whole call fails atomically
/// if any ID is missing or belongs to a different project.
pub fn reorder(
    conn: &Connection,
    project_id: ProjectId,
    ordered_ids: &[SceneId],
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction().map_err(map_db_err)?;
    for (index, scene_id) in ordered_ids.iter().enumerate() {
        let rows = tx
            .execute(
                "UPDATE scenes SET order_index = ?3 WHERE id = ?1 AND project_id = ?2",
                params![scene_id.to_string(), project_id.to_string(), index as i64],
            )
            .map_err(map_db_err)?;

        if rows == 0 {
            return Err(AppError::invalid_input(format!(
                "scene {scene_id} does not belong to project {project_id}"
            )));
        }
    }
    tx.commit().map_err(map_db_err)?;
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: SceneId) -> Result<Option<Scene>, AppError> {
    conn.query_row(
        "SELECT id, project_id, order_index, title, narration, visual_prompt, video_prompt, duration, status
         FROM scenes WHERE id = ?1",
        params![id.to_string()],
        row_to_scene,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn list_by_project(conn: &Connection, project_id: ProjectId) -> Result<Vec<Scene>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, order_index, title, narration, visual_prompt, video_prompt, duration, status
             FROM scenes WHERE project_id = ?1 ORDER BY order_index",
        )
        .map_err(map_db_err)?;
    let rows = stmt
        .query_map(params![project_id.to_string()], row_to_scene)
        .map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

fn row_to_scene(row: &rusqlite::Row<'_>) -> rusqlite::Result<Scene> {
    let id: String = row.get(0)?;
    let project_id: String = row.get(1)?;
    let status: String = row.get(8)?;

    Ok(Scene {
        id: parse_uuid_column(&id).map(SceneId::from_uuid)?,
        project_id: parse_uuid_column(&project_id).map(ProjectId::from_uuid)?,
        order: row.get(2)?,
        title: row.get(3)?,
        narration: row.get(4)?,
        visual_prompt: row.get(5)?,
        video_prompt: row.get(6)?,
        duration: row.get(7)?,
        status: str_to_status(&status)?,
    })
}

fn status_to_str(status: SceneStatus) -> &'static str {
    match status {
        SceneStatus::Draft => "DRAFT",
        SceneStatus::Ready => "READY",
        SceneStatus::Failed => "FAILED",
    }
}

fn str_to_status(value: &str) -> rusqlite::Result<SceneStatus> {
    match value {
        "DRAFT" => Ok(SceneStatus::Draft),
        "READY" => Ok(SceneStatus::Ready),
        "FAILED" => Ok(SceneStatus::Failed),
        other => Err(rusqlite::Error::InvalidColumnType(
            8,
            format!("unknown scene status '{other}'"),
            rusqlite::types::Type::Text,
        )),
    }
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
    use crate::repository::project_repository;

    fn seed_project(db: &Database) -> ProjectId {
        let project = crate::domain::project::Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        project.id
    }

    #[test]
    fn round_trips_a_scene() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let scene = Scene::new(project_id, 1, "Intro").expect("valid scene");

        insert(&db.conn(), &scene).expect("insert");
        let loaded = get_by_id(&db.conn(), scene.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, scene);
    }

    #[test]
    fn updates_existing_scene() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let mut scene = Scene::new(project_id, 1, "Intro").expect("valid scene");
        insert(&db.conn(), &scene).expect("insert");

        scene.status = SceneStatus::Ready;
        scene.narration = "Hello world".into();
        update(&db.conn(), &scene).expect("update");

        let loaded = get_by_id(&db.conn(), scene.id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded.status, SceneStatus::Ready);
        assert_eq!(loaded.narration, "Hello world");
    }

    #[test]
    fn deletes_a_scene() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let scene = Scene::new(project_id, 1, "Intro").expect("valid scene");
        insert(&db.conn(), &scene).expect("insert");

        delete(&db.conn(), scene.id).expect("delete");
        let loaded = get_by_id(&db.conn(), scene.id).expect("query");
        assert!(loaded.is_none());
    }

    #[test]
    fn lists_scenes_by_project_in_order() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let second = Scene::new(project_id, 2, "Second").expect("valid scene");
        let first = Scene::new(project_id, 1, "First").expect("valid scene");
        insert(&db.conn(), &second).expect("insert second");
        insert(&db.conn(), &first).expect("insert first");

        let scenes = list_by_project(&db.conn(), project_id).expect("list");
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].id, first.id);
        assert_eq!(scenes[1].id, second.id);
    }

    #[test]
    fn reorders_scenes_by_given_id_sequence() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let first = Scene::new(project_id, 1, "First").expect("valid scene");
        let second = Scene::new(project_id, 2, "Second").expect("valid scene");
        insert(&db.conn(), &first).expect("insert first");
        insert(&db.conn(), &second).expect("insert second");

        reorder(&db.conn(), project_id, &[second.id, first.id]).expect("reorder");

        let scenes = list_by_project(&db.conn(), project_id).expect("list");
        assert_eq!(scenes[0].id, second.id);
        assert_eq!(scenes[0].order, 0);
        assert_eq!(scenes[1].id, first.id);
        assert_eq!(scenes[1].order, 1);
    }

    #[test]
    fn reorder_rejects_scene_from_other_project() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let other_project_id = seed_project(&db);
        let scene = Scene::new(other_project_id, 1, "Intro").expect("valid scene");
        insert(&db.conn(), &scene).expect("insert");

        let error = reorder(&db.conn(), project_id, &[scene.id]).expect_err("mismatched project");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn cascade_deletes_scenes_when_project_is_removed() {
        let db = Database::open_in_memory().expect("open");
        let project_id = seed_project(&db);
        let scene = Scene::new(project_id, 1, "Intro").expect("valid scene");
        insert(&db.conn(), &scene).expect("insert");

        db.conn()
            .execute(
                "DELETE FROM projects WHERE id = ?1",
                params![project_id.to_string()],
            )
            .expect("delete project");

        let loaded = get_by_id(&db.conn(), scene.id).expect("query");
        assert!(loaded.is_none());
    }
}
