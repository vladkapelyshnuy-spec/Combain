//! IPC command for rendering timelines to video.
//!
//! Coordinates timeline loading, asset resolution, FFmpeg execution,
//! and job tracking for the full render pipeline.

use std::path::PathBuf;

use rusqlite::Connection;
use tauri::{AppHandle, State};

use crate::db::Database;
use crate::domain::ids::{AssetVersionId, TimelineId};
use crate::domain::job::{Job, JobType};
use crate::domain::timeline_clip::Track;
use crate::error::AppError;
use crate::ffmpeg;
use crate::repository::{
    asset_version_repository, job_repository, timeline_clip_repository, timeline_repository,
};
use crate::storage;

/// Resolve asset version file path by ID.
fn resolve_asset_version_path(
    conn: &Connection,
    asset_version_id: AssetVersionId,
) -> Result<PathBuf, AppError> {
    let version =
        asset_version_repository::get_by_id(conn, asset_version_id)?.ok_or_else(|| {
            AppError::invalid_input(format!("asset version {} does not exist", asset_version_id))
        })?;

    // Verify file exists at the stored path
    if !version.file_path.exists() {
        return Err(AppError::io(format!(
            "asset version file does not exist: {}",
            version.file_path.display()
        )));
    }

    Ok(version.file_path)
}

/// Core render implementation.
pub fn render_video_impl(
    db: &Database,
    app: Option<&AppHandle>,
    data_root: &std::path::Path,
    timeline_id: TimelineId,
    output_name: &str,
) -> Result<Job, AppError> {
    let conn = db.conn();

    // Load timeline
    let timeline = timeline_repository::get_by_id(&conn, timeline_id)?.ok_or_else(|| {
        AppError::invalid_input(format!("timeline {} does not exist", timeline_id))
    })?;

    // Check output doesn't already exist
    if storage::render_output_exists(data_root, timeline.project_id, output_name) {
        return Err(AppError::conflict(format!(
            "render output '{}' already exists. Choose a different name or delete the existing file.",
            output_name
        )));
    }

    // Load clips ordered by start_time
    let clips = timeline_clip_repository::list_by_timeline(&conn, timeline_id)?;
    if clips.is_empty() {
        return Err(AppError::invalid_input(
            "cannot render empty timeline (no clips)",
        ));
    }

    // Separate video and audio clips
    let video_clips: Vec<_> = clips.iter().filter(|c| c.track == Track::Video).collect();
    let audio_clips: Vec<_> = clips.iter().filter(|c| c.track == Track::Audio).collect();

    if video_clips.is_empty() {
        return Err(AppError::invalid_input(
            "cannot render timeline with no video clips",
        ));
    }

    // Resolve asset version file paths and build clip timing info
    let mut video_timings = Vec::new();
    for clip in &video_clips {
        let path = resolve_asset_version_path(&conn, clip.asset_version_id)?;
        video_timings.push(ffmpeg::command_builder::ClipTiming {
            file_path: path,
            start_ms: clip.start_time,
            duration_ms: clip.duration,
            source_offset_ms: clip.source_offset,
        });
    }

    let mut audio_timings = Vec::new();
    for clip in &audio_clips {
        let path = resolve_asset_version_path(&conn, clip.asset_version_id)?;
        audio_timings.push(ffmpeg::command_builder::ClipTiming {
            file_path: path,
            start_ms: clip.start_time,
            duration_ms: clip.duration,
            source_offset_ms: clip.source_offset,
        });
    }

    // Ensure renders directory exists
    storage::ensure_renders_dir(data_root, timeline.project_id)?;

    // Construct output path
    let output_path = storage::render_output_path(data_root, timeline.project_id, output_name);

    // Create render job with timeline_id and output_path
    let mut job = Job::new(JobType::Render, None); // No scene_id for renders
    job.set_timeline_id(timeline_id);
    job.set_output_path(output_path.to_string_lossy().to_string());
    let job_id = job.id;
    job_repository::insert(&conn, &job)?;
    tracing::info!(job_id = %job_id, timeline_id = %timeline_id, output_name = %output_name, "render job created");

    let app = app.ok_or_else(|| {
        AppError::config("Bundled FFmpeg runtime requires a Tauri application handle")
    })?;

    // Build FFmpeg command with clip timing
    let config = ffmpeg::command_builder::RenderConfig::new(
        output_path.clone(),
        video_timings,
        audio_timings.clone(),
    );

    let ffmpeg_cmd =
        ffmpeg::command_builder::build_render_command(std::path::Path::new("ffmpeg"), &config)
            .map_err(|e| {
                job_repository::update_status(
                    &conn,
                    job_id,
                    crate::domain::job::JobStatus::Failed,
                    Some(e.to_string()),
                )
                .ok();
                e
            })?;

    // Execute FFmpeg
    tracing::info!(
        job_id = %job_id,
        video_count = config.video_clips.len(),
        audio_count = config.audio_clips.len(),
        "executing FFmpeg render"
    );

    let ffmpeg_args = ffmpeg_cmd
        .get_args()
        .map(|argument| argument.to_os_string())
        .collect();
    let _ffmpeg_output =
        ffmpeg::executor::execute_bundled_sidecar(app, ffmpeg::FFMPEG_SIDECAR, ffmpeg_args)
            .map_err(|e| {
                job_repository::update_status(
                    &conn,
                    job_id,
                    crate::domain::job::JobStatus::Failed,
                    Some(e.to_string()),
                )
                .ok();
                e
            })?;

    // Validate output
    let validation = ffmpeg::validator::validate_mp4_output(app, &output_path).map_err(|e| {
        job_repository::update_status(
            &conn,
            job_id,
            crate::domain::job::JobStatus::Failed,
            Some(e.to_string()),
        )
        .ok();
        e
    })?;

    if !validation.is_valid() {
        let error_msg = format!(
            "render output validation failed: has_video={}, has_audio={}",
            validation.has_video, validation.has_audio
        );
        job_repository::update_status(
            &conn,
            job_id,
            crate::domain::job::JobStatus::Failed,
            Some(error_msg.clone()),
        )
        .ok();
        return Err(AppError::provider(error_msg));
    }

    // If we expected audio but didn't get it, warn but don't fail
    if !audio_clips.is_empty() && !validation.has_audio {
        tracing::warn!(
            job_id = %job_id,
            "render completed but output has no audio stream (expected audio)"
        );
    }

    // Update job to completed
    job_repository::update_status(
        &conn,
        job_id,
        crate::domain::job::JobStatus::Completed,
        None,
    )?;
    tracing::info!(job_id = %job_id, output_path = %output_path.display(), "render completed successfully");

    // Reload job to return with updated status
    job_repository::get_by_id(&conn, job_id)?
        .ok_or_else(|| AppError::db("job disappeared after completion".to_string()))
}

/// Input: `timeline_id`, `output_name` (without .mp4 extension)
/// Output: created [`Job`] with status Running → Completed/Failed
/// Error: [`AppError::InvalidInput`] if timeline doesn't exist, has no clips, or output already exists;
///        [`AppError::Config`] if FFmpeg not found;
///        [`AppError::Provider`] if FFmpeg execution fails;
///        [`AppError::Db`]/[`AppError::Io`] on storage failure
/// Side effects: creates Job row, executes FFmpeg subprocess, writes MP4 file
#[tauri::command]
pub fn render_video(
    app: AppHandle,
    db: State<'_, Database>,
    config: State<'_, crate::config::AppConfig>,
    timeline_id: TimelineId,
    output_name: String,
) -> Result<Job, AppError> {
    let data_root = config
        .app_data_dir
        .as_ref()
        .ok_or_else(|| AppError::config("app_data_dir not configured".to_string()))?;

    render_video_impl(&db, Some(&app), data_root, timeline_id, &output_name)
}

/// Input: `timeline_id`
/// Output: latest [`Job`] for this timeline (most recent created_at), or None if no jobs exist
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
///
/// Allows UI to re-attach to render progress after app restart.
#[tauri::command]
pub fn get_latest_render_job(
    db: State<'_, Database>,
    timeline_id: TimelineId,
) -> Result<Option<Job>, AppError> {
    job_repository::get_latest_by_timeline(&db.conn(), timeline_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::asset::{Asset, AssetType};
    use crate::domain::asset_version::AssetVersion;
    use crate::domain::ids::ProjectId;
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::domain::timeline::Timeline;
    use crate::domain::timeline_clip::TimelineClip;
    use crate::repository::{
        asset_repository, asset_version_repository, project_repository, scene_repository,
        timeline_clip_repository, timeline_repository,
    };

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    fn seed_timeline_with_clips(db: &Database) -> (ProjectId, TimelineId, AssetVersionId) {
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
        )
        .expect("valid version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert version");

        let timeline = Timeline::new(project.id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert timeline");

        let clip =
            TimelineClip::new(timeline.id, version.id, Track::Video, 0, 5000).expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &clip).expect("insert clip");

        (project.id, timeline.id, version.id)
    }

    #[test]
    fn resolve_asset_version_path_returns_error_if_not_exists() {
        let db = open_db();
        let (_, _, asset_version_id) = seed_timeline_with_clips(&db);

        let error = resolve_asset_version_path(&db.conn(), asset_version_id)
            .expect_err("file does not exist");
        assert_eq!(error.code(), crate::error::ErrorCode::Io);
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn render_video_impl_rejects_nonexistent_timeline() {
        let db = open_db();
        let temp_dir = tempfile::tempdir().expect("temp dir");

        let error = render_video_impl(&db, None, temp_dir.path(), TimelineId::new(), "output")
            .expect_err("nonexistent timeline");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn render_video_impl_rejects_empty_timeline() {
        let db = open_db();
        let temp_dir = tempfile::tempdir().expect("temp dir");

        let project = Project::new("Test", "t", "s").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");

        let timeline = Timeline::new(project.id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        let error = render_video_impl(&db, None, temp_dir.path(), timeline.id, "output")
            .expect_err("empty timeline");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
        assert!(error.to_string().contains("no clips"));
    }

    #[test]
    fn render_video_impl_rejects_existing_output() {
        let db = open_db();
        let temp_dir = tempfile::tempdir().expect("temp dir");

        let (project_id, timeline_id, _) = seed_timeline_with_clips(&db);

        // Create existing output file
        let output_path = storage::render_output_path(temp_dir.path(), project_id, "existing");
        storage::ensure_renders_dir(temp_dir.path(), project_id).expect("create dir");
        std::fs::write(&output_path, b"existing file").expect("write file");

        let error = render_video_impl(&db, None, temp_dir.path(), timeline_id, "existing")
            .expect_err("existing output");
        assert_eq!(error.code(), crate::error::ErrorCode::Conflict);
        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn render_video_impl_requires_video_clips() {
        let db = open_db();
        let temp_dir = tempfile::tempdir().expect("temp dir");

        let project = Project::new("Test", "t", "s").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");

        let scene = Scene::new(project.id, 1, "Scene").expect("scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert");

        let audio_asset = Asset::new(scene.id, AssetType::Audio);
        asset_repository::insert(&db.conn(), &audio_asset).expect("insert");

        // Create audio asset version
        let audio_version = AssetVersion::new(
            audio_asset.id,
            1,
            "test_audio.mp3",
            "test_provider",
            "test_model",
            "test prompt",
        )
        .expect("valid version");
        asset_version_repository::insert(&db.conn(), &audio_version).expect("insert version");

        let timeline = Timeline::new(project.id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert");

        // Add only audio clip, no video
        let audio_clip = TimelineClip::new(timeline.id, audio_version.id, Track::Audio, 0, 3000)
            .expect("create clip");
        timeline_clip_repository::insert(&db.conn(), &audio_clip).expect("insert");

        let error = render_video_impl(&db, None, temp_dir.path(), timeline.id, "output")
            .expect_err("no video clips");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
        assert!(error.to_string().contains("no video clips"));
    }
}
