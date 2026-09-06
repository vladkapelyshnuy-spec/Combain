//! Timeline validation functions.
//!
//! Validates timeline existence, clips, and asset references.

use rusqlite::Connection;

use crate::domain::ids::{ProjectId, TimelineId};
use crate::domain::timeline::Timeline;
use crate::domain::timeline_clip::TimelineClip;
use crate::error::AppError;
use crate::repository::{asset_version_repository, timeline_clip_repository, timeline_repository};

/// Validate that a timeline exists for a project.
///
/// Returns the timeline if found, or an error if not.
pub fn validate_timeline_exists(conn: &Connection, project_id: ProjectId) -> Result<Timeline, AppError> {
    timeline_repository::get_by_project(conn, project_id)?
        .ok_or_else(|| AppError::invalid_input(format!("project {} has no timeline", project_id)))
}

/// Validate that a timeline has at least one clip.
///
/// Returns all clips for the timeline, or an error if none exist.
pub fn validate_timeline_has_clips(conn: &Connection, timeline_id: TimelineId) -> Result<Vec<TimelineClip>, AppError> {
    let clips = timeline_clip_repository::list_by_timeline(conn, timeline_id)?;

    if clips.is_empty() {
        return Err(AppError::invalid_input(format!(
            "timeline {} has no clips",
            timeline_id
        )));
    }

    Ok(clips)
}

/// Validate that all clips in a timeline reference valid asset versions.
///
/// Checks that each clip's asset_version_id exists in the database.
pub fn validate_all_clips_have_asset_versions(conn: &Connection, timeline_id: TimelineId) -> Result<(), AppError> {
    let clips = timeline_clip_repository::list_by_timeline(conn, timeline_id)?;

    for clip in &clips {
        // Try to get the asset version
        let version = asset_version_repository::get_by_id(conn, clip.asset_version_id)?;
        if version.is_none() {
            return Err(AppError::invalid_input(format!(
                "clip {} references non-existent asset version {}",
                clip.id, clip.asset_version_id
            )));
        }
    }

    Ok(())
}

/// Validate that all clips' asset version files exist on disk.
///
/// Ensures each clip can be rendered by checking its asset version file exists.
pub fn validate_all_clip_asset_files_exist(
    conn: &Connection,
    timeline_id: TimelineId,
) -> Result<(), AppError> {
    let clips = timeline_clip_repository::list_by_timeline(conn, timeline_id)?;

    for clip in &clips {
        let version = asset_version_repository::get_by_id(conn, clip.asset_version_id)?
            .ok_or_else(|| AppError::invalid_input(format!(
                "clip {} references non-existent asset version {}",
                clip.id, clip.asset_version_id
            )))?;

        if !version.file_path.exists() {
            return Err(AppError::invalid_input(format!(
                "clip {} references asset version {} whose file does not exist: {}",
                clip.id, clip.asset_version_id, version.file_path.display()
            )));
        }
    }

    Ok(())
}

/// Comprehensive timeline validation.
///
/// Runs all timeline validation checks:
/// - Timeline exists
/// - Timeline has clips
/// - All clips reference valid asset versions
/// - All clip asset version files exist
pub fn validate_timeline_ready_for_render(
    conn: &Connection,
    project_id: ProjectId,
) -> Result<Timeline, AppError> {
    let timeline = validate_timeline_exists(conn, project_id)?;
    validate_timeline_has_clips(conn, timeline.id)?;
    validate_all_clips_have_asset_versions(conn, timeline.id)?;
    validate_all_clip_asset_files_exist(conn, timeline.id)?;

    Ok(timeline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::asset::{Asset, AssetType};
    use crate::domain::asset_version::AssetVersion;
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::domain::timeline_clip::Track;
    use crate::repository::{asset_repository, asset_version_repository, project_repository, scene_repository};

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    fn seed_project_and_scene(db: &Database) -> (ProjectId, crate::domain::ids::SceneId) {
        let project = Project::new("Test", "topic", "style").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");

        let scene = Scene::new(project.id, 1, "Scene 1").expect("scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert");

        (project.id, scene.id)
    }

    #[test]
    fn validate_timeline_exists_returns_timeline_if_found() {
        let db = open_db();
        let (project_id, _) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let result = validate_timeline_exists(&db.conn(), project_id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, timeline.id);
    }

    #[test]
    fn validate_timeline_exists_returns_error_if_not_found() {
        let db = open_db();
        let (project_id, _) = seed_project_and_scene(&db);

        let result = validate_timeline_exists(&db.conn(), project_id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("no timeline"));
    }

    #[test]
    fn validate_timeline_has_clips_returns_clips_if_present() {
        let db = open_db();
        let (project_id, scene_id) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let asset = Asset::new(scene_id, AssetType::Video);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        let version = AssetVersion::new(
            asset.id,
            1,
            std::path::PathBuf::from("/tmp/test.mp4"),
            "provider",
            "model",
            "prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");

        let clip = TimelineClip::new(timeline.id, version.id, Track::Video, 0, 5000).expect("clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let result = validate_timeline_has_clips(&db.conn(), timeline.id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn validate_timeline_has_clips_returns_error_if_no_clips() {
        let db = open_db();
        let (project_id, _) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let result = validate_timeline_has_clips(&db.conn(), timeline.id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("no clips"));
    }

    #[test]
    fn validate_all_clips_have_asset_versions_passes_for_valid_clips() {
        let db = open_db();
        let (project_id, scene_id) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let asset = Asset::new(scene_id, AssetType::Video);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        let version = AssetVersion::new(
            asset.id,
            1,
            std::path::PathBuf::from("/tmp/test.mp4"),
            "provider",
            "model",
            "prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");

        let clip = TimelineClip::new(timeline.id, version.id, Track::Video, 0, 5000).expect("clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let result = validate_all_clips_have_asset_versions(&db.conn(), timeline.id);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_all_clip_asset_files_exist_passes_for_valid_files() {
        let db = open_db();
        let (project_id, scene_id) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let asset = Asset::new(scene_id, AssetType::Video);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        // Create a temporary file
        let temp_file = tempfile::NamedTempFile::new().expect("temp file");
        let file_path = temp_file.path().to_path_buf();

        let version = AssetVersion::new(
            asset.id,
            1,
            file_path,
            "provider",
            "model",
            "prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");

        let clip = TimelineClip::new(timeline.id, version.id, Track::Video, 0, 5000).expect("clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let result = validate_all_clip_asset_files_exist(&db.conn(), timeline.id);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_all_clip_asset_files_exist_fails_for_missing_file() {
        let db = open_db();
        let (project_id, scene_id) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let asset = Asset::new(scene_id, AssetType::Video);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        let version = AssetVersion::new(
            asset.id,
            1,
            std::path::PathBuf::from("/nonexistent/file.mp4"),
            "provider",
            "model",
            "prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");

        let clip = TimelineClip::new(timeline.id, version.id, Track::Video, 0, 5000).expect("clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let result = validate_all_clip_asset_files_exist(&db.conn(), timeline.id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn validate_timeline_ready_for_render_passes_for_complete_timeline() {
        let db = open_db();
        let (project_id, scene_id) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let asset = Asset::new(scene_id, AssetType::Video);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        // Create a temporary file
        let temp_file = tempfile::NamedTempFile::new().expect("temp file");
        let file_path = temp_file.path().to_path_buf();

        let version = AssetVersion::new(
            asset.id,
            1,
            file_path,
            "provider",
            "model",
            "prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");

        let clip = TimelineClip::new(timeline.id, version.id, Track::Video, 0, 5000).expect("clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert");

        let result = validate_timeline_ready_for_render(&db.conn(), project_id);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_timeline_ready_for_render_fails_if_no_timeline() {
        let db = open_db();
        let (project_id, _) = seed_project_and_scene(&db);

        let result = validate_timeline_ready_for_render(&db.conn(), project_id);
        assert!(result.is_err());
    }

    #[test]
    fn validate_timeline_ready_for_render_fails_if_no_clips() {
        let db = open_db();
        let (project_id, _) = seed_project_and_scene(&db);

        let timeline = Timeline::new(project_id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let result = validate_timeline_ready_for_render(&db.conn(), project_id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("no clips"));
    }
}
