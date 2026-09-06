//! IPC commands for Timeline and TimelineClip operations.
//!
//! Provides timeline management and clip manipulation operations for the UI.
//! All operations use the existing repository layer for persistence.

use rusqlite::Connection;
use tauri::State;

use crate::db::Database;
use crate::domain::ids::{AssetVersionId, ProjectId, TimelineClipId, TimelineId};
use crate::domain::timeline::Timeline;
use crate::domain::timeline_clip::{TimelineClip, Track};
use crate::error::AppError;
use crate::repository::{timeline_clip_repository, timeline_repository};

/// Create a new timeline for a project (or return existing).
pub fn create_timeline_impl(
    conn: &Connection,
    project_id: ProjectId,
) -> Result<Timeline, AppError> {
    // Check if timeline already exists for this project
    if let Some(existing) = timeline_repository::get_by_project(conn, project_id)? {
        return Ok(existing);
    }

    let timeline = Timeline::new(project_id);
    timeline_repository::insert(conn, &timeline)?;
    tracing::info!(timeline_id = %timeline.id, project_id = %project_id, "timeline created");
    Ok(timeline)
}

/// Move a clip to a new start time.
pub fn move_clip_impl(
    conn: &Connection,
    clip_id: TimelineClipId,
    new_start_time: u64,
) -> Result<TimelineClip, AppError> {
    // Find clip's timeline by querying the clip directly
    let timeline_id = find_clip_timeline(conn, clip_id)?;
    let clips = timeline_clip_repository::list_by_timeline(conn, timeline_id)?;
    let mut clip = clips
        .into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| AppError::invalid_input(format!("clip {clip_id} does not exist")))?;

    clip.start_time = new_start_time;
    timeline_clip_repository::update(conn, &clip)?;
    tracing::info!(clip_id = %clip_id, new_start_time = new_start_time, "clip moved");
    Ok(clip)
}

/// Trim a clip to a new duration.
pub fn trim_clip_impl(
    conn: &Connection,
    clip_id: TimelineClipId,
    new_duration: u64,
) -> Result<TimelineClip, AppError> {
    if new_duration == 0 {
        return Err(AppError::invalid_input(
            "timeline clip duration must be greater than zero",
        ));
    }

    let timeline_id = find_clip_timeline(conn, clip_id)?;
    let clips = timeline_clip_repository::list_by_timeline(conn, timeline_id)?;
    let mut clip = clips
        .into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| AppError::invalid_input(format!("clip {clip_id} does not exist")))?;

    clip.duration = new_duration;
    timeline_clip_repository::update(conn, &clip)?;
    tracing::info!(clip_id = %clip_id, new_duration = new_duration, "clip trimmed");
    Ok(clip)
}

/// Split a clip at a given time into two clips.
pub fn split_clip_impl(
    conn: &Connection,
    clip_id: TimelineClipId,
    split_time: u64,
) -> Result<(TimelineClip, TimelineClip), AppError> {
    if split_time == 0 {
        return Err(AppError::invalid_input(
            "split_time must be greater than zero",
        ));
    }

    let timeline_id = find_clip_timeline(conn, clip_id)?;
    let clips = timeline_clip_repository::list_by_timeline(conn, timeline_id)?;
    let original_clip = clips
        .into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| AppError::invalid_input(format!("clip {clip_id} does not exist")))?;

    if split_time >= original_clip.duration {
        return Err(AppError::invalid_input(format!(
            "split_time ({split_time}) must be less than clip duration ({})",
            original_clip.duration
        )));
    }

    // First clip: keep same start, reduce duration
    let mut first_clip = original_clip.clone();
    first_clip.duration = split_time;

    // Second clip: new clip starting after first, remaining duration
    let second_clip = TimelineClip::new(
        timeline_id,
        original_clip.asset_version_id,
        original_clip.track,
        original_clip.start_time + split_time,
        original_clip.duration - split_time,
    )?;

    // Update first, insert second
    timeline_clip_repository::update(conn, &first_clip)?;
    timeline_clip_repository::insert(conn, &second_clip)?;

    tracing::info!(
        original_clip_id = %clip_id,
        first_clip_id = %first_clip.id,
        second_clip_id = %second_clip.id,
        split_time = split_time,
        "clip split"
    );

    Ok((first_clip, second_clip))
}

/// Helper: find the timeline_id for a given clip_id.
fn find_clip_timeline(conn: &Connection, clip_id: TimelineClipId) -> Result<TimelineId, AppError> {
    let timeline_id_str: String = conn
        .query_row(
            "SELECT timeline_id FROM timeline_clips WHERE id = ?1",
            [clip_id.to_string()],
            |row| row.get(0),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::invalid_input(format!("clip {clip_id} does not exist"))
            }
            other => crate::db::map_db_err(other),
        })?;

    TimelineId::from_uuid(
        uuid::Uuid::parse_str(&timeline_id_str)
            .map_err(|e| AppError::db(format!("invalid timeline_id: {e}")))?,
    )
    .pipe(Ok)
}

// ────────────────────────────────────────────────────────────────────────────
// Tauri Commands (thin wrappers around impl functions)
// ────────────────────────────────────────────────────────────────────────────

/// Input: `project_id`
/// Output: newly created or existing [`Timeline`]
/// Error: [`AppError::Db`] on storage failure
/// Side effects: creates Timeline row if doesn't exist
#[tauri::command]
pub fn create_timeline(
    db: State<'_, Database>,
    project_id: ProjectId,
) -> Result<Timeline, AppError> {
    create_timeline_impl(&db.conn(), project_id)
}

/// Input: `project_id`
/// Output: [`Timeline`] if exists
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn get_timeline(
    db: State<'_, Database>,
    project_id: ProjectId,
) -> Result<Option<Timeline>, AppError> {
    timeline_repository::get_by_project(&db.conn(), project_id)
}

/// Input: `timeline_id`, `asset_version_id`, `track`, `start_time` (ms), `duration` (ms)
/// Output: newly created [`TimelineClip`]
/// Error: [`AppError::InvalidInput`] if duration is 0 or timeline doesn't exist;
///        [`AppError::Db`] on storage failure
/// Side effects: creates TimelineClip row
#[tauri::command]
pub fn add_clip(
    db: State<'_, Database>,
    timeline_id: TimelineId,
    asset_version_id: AssetVersionId,
    track: Track,
    start_time: u64,
    duration: u64,
) -> Result<TimelineClip, AppError> {
    let clip = TimelineClip::new(timeline_id, asset_version_id, track, start_time, duration)?;
    let conn = db.conn();
    timeline_clip_repository::insert(&conn, &clip)?;
    tracing::info!(
        clip_id = %clip.id,
        timeline_id = %timeline_id,
        asset_version_id = %asset_version_id,
        track = ?track,
        start_time = start_time,
        duration = duration,
        "clip added to timeline"
    );
    Ok(clip)
}

/// Input: `clip_id`, `start_time` (ms), `duration` (ms)
/// Output: updated [`TimelineClip`]
/// Error: [`AppError::InvalidInput`] if clip doesn't exist or duration is 0;
///        [`AppError::Db`] on storage failure
/// Side effects: updates TimelineClip row
#[tauri::command]
pub fn update_clip(
    db: State<'_, Database>,
    clip_id: TimelineClipId,
    start_time: u64,
    duration: u64,
) -> Result<TimelineClip, AppError> {
    if duration == 0 {
        return Err(AppError::invalid_input(
            "timeline clip duration must be greater than zero",
        ));
    }

    let conn = db.conn();
    let timeline_id = find_clip_timeline(&conn, clip_id)?;
    let clips = timeline_clip_repository::list_by_timeline(&conn, timeline_id)?;
    let mut clip = clips
        .into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| AppError::invalid_input(format!("clip {clip_id} does not exist")))?;

    clip.start_time = start_time;
    clip.duration = duration;
    timeline_clip_repository::update(&conn, &clip)?;
    tracing::info!(clip_id = %clip_id, start_time = start_time, duration = duration, "clip updated");
    Ok(clip)
}

/// Input: `clip_id`
/// Output: unit (success indicator)
/// Error: [`AppError::Db`] on storage failure
/// Side effects: deletes TimelineClip row
#[tauri::command]
pub fn delete_clip(db: State<'_, Database>, clip_id: TimelineClipId) -> Result<(), AppError> {
    timeline_clip_repository::delete(&db.conn(), clip_id)?;
    tracing::info!(clip_id = %clip_id, "clip deleted from timeline");
    Ok(())
}

/// Input: `timeline_id`
/// Output: list of [`TimelineClip`] ordered by start_time
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn list_clips(
    db: State<'_, Database>,
    timeline_id: TimelineId,
) -> Result<Vec<TimelineClip>, AppError> {
    timeline_clip_repository::list_by_timeline(&db.conn(), timeline_id)
}

/// Move a clip to a new start time.
#[tauri::command]
pub fn move_clip(
    db: State<'_, Database>,
    clip_id: TimelineClipId,
    new_start_time: u64,
) -> Result<TimelineClip, AppError> {
    move_clip_impl(&db.conn(), clip_id, new_start_time)
}

/// Trim a clip to a new duration.
#[tauri::command]
pub fn trim_clip(
    db: State<'_, Database>,
    clip_id: TimelineClipId,
    new_duration: u64,
) -> Result<TimelineClip, AppError> {
    trim_clip_impl(&db.conn(), clip_id, new_duration)
}

/// Split a clip at a given time into two clips.
#[tauri::command]
pub fn split_clip(
    db: State<'_, Database>,
    clip_id: TimelineClipId,
    split_time: u64,
) -> Result<(TimelineClip, TimelineClip), AppError> {
    split_clip_impl(&db.conn(), clip_id, split_time)
}

// ────────────────────────────────────────────────────────────────────────────
// Helper trait for Result pipelining
// ────────────────────────────────────────────────────────────────────────────

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::asset::{Asset, AssetType};
    use crate::domain::asset_version::AssetVersion;
    use crate::domain::ids::AssetVersionId;
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::repository::{asset_repository, asset_version_repository, project_repository, scene_repository};

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    fn seed_project_and_asset(db: &Database) -> (ProjectId, AssetVersionId) {
        let project = Project::new("Test Project", "topic", "style").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert project");

        let scene = Scene::new(project.id, 1, "Scene 1").expect("scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");

        let asset = Asset::new(scene.id, AssetType::Video);
        asset_repository::insert(&db.conn(), &asset).expect("insert asset");

        // Create asset version
        let version = AssetVersion::new(
            asset.id,
            1,
            "test_video.mp4",
            "test_provider",
            "test_model",
            "test prompt",
        ).expect("valid version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert version");

        (project.id, version.id)
    }

    #[test]
    fn create_timeline_for_project() {
        let db = open_db();
        let (project_id, _) = seed_project_and_asset(&db);

        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create");
        assert_eq!(timeline.project_id, project_id);
    }

    #[test]
    fn create_timeline_returns_existing_if_present() {
        let db = open_db();
        let (project_id, _) = seed_project_and_asset(&db);

        let first = create_timeline_impl(&db.conn(), project_id).expect("create first");
        let second = create_timeline_impl(&db.conn(), project_id).expect("create second");

        assert_eq!(first.id, second.id);
    }

    #[test]
    fn get_timeline_returns_none_if_not_exists() {
        let db = open_db();
        let (project_id, _) = seed_project_and_asset(&db);

        let timeline = timeline_repository::get_by_project(&db.conn(), project_id).expect("query");
        assert!(timeline.is_none());
    }

    #[test]
    fn add_clip_creates_clip() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let clips =
            timeline_clip_repository::list_by_timeline(&db.conn(), timeline.id).expect("list");
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].id, clip.id);
    }

    #[test]
    fn move_clip_updates_start_time() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let moved = move_clip_impl(&db.conn(), clip.id, 1000).expect("move");
        assert_eq!(moved.start_time, 1000);
        assert_eq!(moved.duration, 5000);
    }

    #[test]
    fn trim_clip_updates_duration() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let trimmed = trim_clip_impl(&db.conn(), clip.id, 3000).expect("trim");
        assert_eq!(trimmed.duration, 3000);
        assert_eq!(trimmed.start_time, 0);
    }

    #[test]
    fn trim_clip_rejects_zero_duration() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let error = trim_clip_impl(&db.conn(), clip.id, 0).expect_err("zero duration");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn split_clip_creates_two_clips() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip = TimelineClip::new(timeline.id, asset_id, Track::Video, 1000, 6000)
            .expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let (first, second) = split_clip_impl(&db.conn(), clip.id, 2000).expect("split");

        assert_eq!(first.start_time, 1000);
        assert_eq!(first.duration, 2000);
        assert_eq!(second.start_time, 3000);
        assert_eq!(second.duration, 4000);

        let clips =
            timeline_clip_repository::list_by_timeline(&db.conn(), timeline.id).expect("list");
        assert_eq!(clips.len(), 2);
    }

    #[test]
    fn split_clip_rejects_zero_split_time() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let error = split_clip_impl(&db.conn(), clip.id, 0).expect_err("zero split");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn split_clip_rejects_split_at_or_past_duration() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let error = split_clip_impl(&db.conn(), clip.id, 5000).expect_err("split at duration");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);

        let error2 = split_clip_impl(&db.conn(), clip.id, 6000).expect_err("split past duration");
        assert_eq!(error2.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn delete_clip_removes_clip() {
        let db = open_db();
        let (project_id, asset_id) = seed_project_and_asset(&db);
        let timeline = create_timeline_impl(&db.conn(), project_id).expect("create timeline");

        let clip =
            TimelineClip::new(timeline.id, asset_id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        timeline_clip_repository::delete(&db.conn(), clip.id).expect("delete");

        let clips =
            timeline_clip_repository::list_by_timeline(&db.conn(), timeline.id).expect("list");
        assert!(clips.is_empty());
    }
}
