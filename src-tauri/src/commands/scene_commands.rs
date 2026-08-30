//! IPC commands for [`Scene`](crate::domain::scene::Scene).
//!
//! Each `#[tauri::command]` wraps a plain function over `&Connection` so the
//! logic can be unit tested without a running Tauri app.

use rusqlite::Connection;
use tauri::State;

use crate::db::Database;
use crate::domain::ids::{ProjectId, SceneId};
use crate::domain::scene::{Scene, SceneStatus};
use crate::error::AppError;
use crate::repository::scene_repository;

fn next_order(conn: &Connection, project_id: ProjectId) -> Result<u32, AppError> {
    let existing = scene_repository::list_by_project(conn, project_id)?;
    Ok(existing.len() as u32)
}

fn create_scene_impl(
    conn: &Connection,
    project_id: ProjectId,
    title: String,
) -> Result<Scene, AppError> {
    let order = next_order(conn, project_id)?;
    let scene = Scene::new(project_id, order, title)?;
    scene_repository::insert(conn, &scene)?;
    tracing::info!(scene_id = %scene.id, project_id = %project_id, "scene created");
    Ok(scene)
}

#[allow(clippy::too_many_arguments)]
fn update_scene_impl(
    conn: &Connection,
    scene_id: SceneId,
    title: String,
    narration: String,
    visual_prompt: String,
    video_prompt: String,
    duration: u32,
    status: SceneStatus,
) -> Result<Scene, AppError> {
    let mut scene = scene_repository::get_by_id(conn, scene_id)?
        .ok_or_else(|| AppError::invalid_input(format!("scene {scene_id} does not exist")))?;

    scene.update(
        title,
        narration,
        visual_prompt,
        video_prompt,
        duration,
        status,
    )?;
    scene_repository::update(conn, &scene)?;
    tracing::info!(scene_id = %scene.id, "scene updated");
    Ok(scene)
}

/// Input: `project_id`
/// Output: [`Scene`]s belonging to the project, ordered by `order`
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn get_scenes(db: State<'_, Database>, project_id: ProjectId) -> Result<Vec<Scene>, AppError> {
    scene_repository::list_by_project(&db.conn(), project_id)
}

/// Input: `project_id`, `title`
/// Output: the created [`Scene`], appended at the end of the project's scene order
/// Error: [`AppError::InvalidInput`] if `title` is empty; [`AppError::Db`] on storage failure
/// Side effects: inserts a new row into `scenes`
#[tauri::command]
pub fn create_scene(
    db: State<'_, Database>,
    project_id: ProjectId,
    title: String,
) -> Result<Scene, AppError> {
    create_scene_impl(&db.conn(), project_id, title)
}

/// Input: `scene_id`, `title`, `narration`, `visual_prompt`, `video_prompt`, `duration`, `status`
/// Output: the updated [`Scene`]
/// Error: [`AppError::InvalidInput`] if `title` is empty or `scene_id` is unknown; [`AppError::Db`] on storage failure
/// Side effects: updates the matching row in `scenes`
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_scene(
    db: State<'_, Database>,
    scene_id: SceneId,
    title: String,
    narration: String,
    visual_prompt: String,
    video_prompt: String,
    duration: u32,
    status: SceneStatus,
) -> Result<Scene, AppError> {
    update_scene_impl(
        &db.conn(),
        scene_id,
        title,
        narration,
        visual_prompt,
        video_prompt,
        duration,
        status,
    )
}

/// Input: `scene_id`
/// Output: none
/// Error: [`AppError::Db`] on storage failure
/// Side effects: deletes the matching row from `scenes` (cascades to its assets)
#[tauri::command]
pub fn delete_scene(db: State<'_, Database>, scene_id: SceneId) -> Result<(), AppError> {
    scene_repository::delete(&db.conn(), scene_id)?;
    tracing::info!(scene_id = %scene_id, "scene deleted");
    Ok(())
}

/// Input: `project_id`, `scene_ids` (full ordered sequence of the project's scene IDs)
/// Output: none
/// Error: [`AppError::InvalidInput`] if any ID is missing or belongs to another project; [`AppError::Db`] on storage failure
/// Side effects: updates `order_index` for every scene in `scene_ids`
#[tauri::command]
pub fn reorder_scenes(
    db: State<'_, Database>,
    project_id: ProjectId,
    scene_ids: Vec<SceneId>,
) -> Result<(), AppError> {
    scene_repository::reorder(&db.conn(), project_id, &scene_ids)?;
    tracing::info!(project_id = %project_id, "scenes reordered");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::repository::project_repository;

    fn open_db() -> Database {
        Database::open_in_memory().expect("open in-memory db")
    }

    fn seed_project(db: &Database) -> ProjectId {
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        project.id
    }

    #[test]
    fn creates_scenes_with_increasing_order() {
        let db = open_db();
        let project_id = seed_project(&db);

        let first = create_scene_impl(&db.conn(), project_id, "First".into()).expect("create");
        let second = create_scene_impl(&db.conn(), project_id, "Second".into()).expect("create");

        assert_eq!(first.order, 0);
        assert_eq!(second.order, 1);
    }

    #[test]
    fn create_rejects_empty_title() {
        let db = open_db();
        let project_id = seed_project(&db);
        let error =
            create_scene_impl(&db.conn(), project_id, "  ".into()).expect_err("empty title");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn lists_scenes_for_project() {
        let db = open_db();
        let project_id = seed_project(&db);
        create_scene_impl(&db.conn(), project_id, "First".into()).expect("create");
        create_scene_impl(&db.conn(), project_id, "Second".into()).expect("create");

        let scenes = scene_repository::list_by_project(&db.conn(), project_id).expect("list");
        assert_eq!(scenes.len(), 2);
    }

    #[test]
    fn updates_a_scene() {
        let db = open_db();
        let project_id = seed_project(&db);
        let scene = create_scene_impl(&db.conn(), project_id, "First".into()).expect("create");

        let updated = update_scene_impl(
            &db.conn(),
            scene.id,
            "Renamed".into(),
            "Hello".into(),
            "a cat".into(),
            "a cat walking".into(),
            5,
            SceneStatus::Ready,
        )
        .expect("update");

        assert_eq!(updated.title, "Renamed");
        assert_eq!(updated.status, SceneStatus::Ready);
    }

    #[test]
    fn update_missing_scene_errors() {
        let db = open_db();
        let error = update_scene_impl(
            &db.conn(),
            SceneId::new(),
            "Renamed".into(),
            "".into(),
            "".into(),
            "".into(),
            0,
            SceneStatus::Draft,
        )
        .expect_err("missing scene");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn deletes_a_scene() {
        let db = open_db();
        let project_id = seed_project(&db);
        let scene = create_scene_impl(&db.conn(), project_id, "First".into()).expect("create");

        scene_repository::delete(&db.conn(), scene.id).expect("delete");
        let loaded = scene_repository::get_by_id(&db.conn(), scene.id).expect("query");
        assert!(loaded.is_none());
    }

    #[test]
    fn reorders_scenes_and_persists_new_order() {
        let db = open_db();
        let project_id = seed_project(&db);
        let first = create_scene_impl(&db.conn(), project_id, "First".into()).expect("create");
        let second = create_scene_impl(&db.conn(), project_id, "Second".into()).expect("create");

        scene_repository::reorder(&db.conn(), project_id, &[second.id, first.id]).expect("reorder");

        let scenes = scene_repository::list_by_project(&db.conn(), project_id).expect("list");
        assert_eq!(scenes[0].id, second.id);
        assert_eq!(scenes[1].id, first.id);
    }
}
