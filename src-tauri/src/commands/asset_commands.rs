//! IPC commands for [`Asset`]/[`AssetVersion`]: image generation and version
//! selection. Mirrors the style of [`crate::commands::script_commands`]: plain,
//! unit-testable functions over `&Connection` wrapped by thin `#[tauri::command]`s.
//! The provider call happens between two connection scopes so the SQLite mutex
//! is never held across an `.await`.

use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use tauri::State;

use crate::db::Database;
use crate::domain::asset::{Asset, AssetType};
use crate::domain::asset_version::AssetVersion;
use crate::domain::audio_utils;
use crate::domain::ids::{AssetId, AssetVersionId, SceneId};
use crate::error::AppError;
use crate::paths;
use crate::provider::image_provider::ImageRequest;
use crate::provider::video_provider::VideoRequest;
use crate::provider::{ImageProvider, VideoProvider, VideoStatus};
use crate::repository::{asset_repository, asset_version_repository, scene_repository};
use crate::storage;

const IMAGE_EXTENSION: &str = "png";
const IMAGE_PROVIDER_NAME: &str = "openai";
const VIDEO_EXTENSION: &str = "mp4";
const VIDEO_PROVIDER_NAME: &str = "openai";

fn resolve_prompt(
    scene: &crate::domain::scene::Scene,
    prompt: Option<String>,
) -> Result<String, AppError> {
    let prompt = prompt.unwrap_or_else(|| scene.visual_prompt.clone());
    if prompt.trim().is_empty() {
        return Err(AppError::invalid_input(
            "prompt must not be empty and scene has no visual_prompt to fall back to",
        ));
    }
    Ok(prompt)
}

/// Find this scene's existing image asset, if any. A scene has at most one
/// image asset; regeneration adds versions to it rather than creating another.
fn find_image_asset(conn: &Connection, scene_id: SceneId) -> Result<Option<Asset>, AppError> {
    let assets = asset_repository::list_by_scene(conn, scene_id)?;
    Ok(assets
        .into_iter()
        .find(|asset| asset.asset_type == AssetType::Image))
}

fn next_version_number(conn: &Connection, asset_id: AssetId) -> Result<u32, AppError> {
    let existing = asset_version_repository::list_by_asset(conn, asset_id)?;
    Ok(existing.len() as u32 + 1)
}

/// Core logic, independent of Tauri `State`, so it can run under a plain
/// `Connection`/`ImageProvider` (including a mock) in tests.
pub async fn generate_image_impl(
    db: &Database,
    provider: &(dyn ImageProvider + Send + Sync),
    data_root: &Path,
    scene_id: SceneId,
    prompt: Option<String>,
) -> Result<AssetVersion, AppError> {
    let (project_id, request, existing_asset) = {
        let conn = db.conn();
        let scene = scene_repository::get_by_id(&conn, scene_id)?
            .ok_or_else(|| AppError::invalid_input(format!("scene {scene_id} does not exist")))?;
        let prompt = resolve_prompt(&scene, prompt)?;
        let existing_asset = find_image_asset(&conn, scene_id)?;
        (scene.project_id, ImageRequest { prompt }, existing_asset)
    }; // guard dropped before the `.await` below

    let generated = provider.generate_image(&request).await?;
    if !storage::looks_like_png(&generated.bytes) {
        return Err(AppError::provider(
            "image provider returned data that is not a valid PNG",
        ));
    }

    let conn = db.conn();
    let tx = conn
        .unchecked_transaction()
        .map_err(crate::db::map_db_err)?;

    let asset = match existing_asset {
        Some(asset) => asset,
        None => {
            let asset = Asset::new(scene_id, AssetType::Image);
            asset_repository::insert(&tx, &asset)?;
            asset
        }
    };
    let version_number = next_version_number(&tx, asset.id)?;

    let file_path = storage::asset_version_path(
        data_root,
        project_id,
        asset.id,
        version_number,
        IMAGE_EXTENSION,
    );
    storage::save_new_file(&file_path, &generated.bytes)?;

    let mut version = AssetVersion::new(
        asset.id,
        version_number,
        file_path,
        IMAGE_PROVIDER_NAME,
        generated.model.clone(),
        request.prompt.clone(),
    )?;
    asset_version_repository::insert(&tx, &version)?;

    if version_number == 1 {
        asset_version_repository::select_version_in(&tx, asset.id, version.id)?;
        version.selected = true;
    }

    tx.commit().map_err(crate::db::map_db_err)?;

    tracing::info!(scene_id = %scene_id, asset_id = %asset.id, version = version_number, "image generated");
    Ok(version)
}

/// Input: `scene_id`, `prompt` (optional; falls back to the scene's `visual_prompt`)
/// Output: the newly created [`AssetVersion`]
/// Error: [`AppError::InvalidInput`] if the scene is unknown or no prompt is available;
///        [`AppError::Config`] if no provider API key is configured;
///        [`AppError::Provider`] on provider/network/format failure;
///        [`AppError::Db`]/[`AppError::Io`] on storage failure
/// Side effects: creates an `Asset` (first generation only), inserts a new
/// `AssetVersion` row, and writes the image file to disk. Never overwrites or
/// deletes a previous version.
#[tauri::command]
pub async fn generate_image(
    db: State<'_, Database>,
    provider: State<'_, Arc<dyn ImageProvider + Send + Sync>>,
    scene_id: SceneId,
    prompt: Option<String>,
) -> Result<AssetVersion, AppError> {
    let data_root = paths::portable_data_dir()?;
    generate_image_impl(&db, provider.as_ref(), &data_root, scene_id, prompt).await
}

/// Input: `scene_id`
/// Output: [`Asset`]s belonging to the scene
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn get_assets(db: State<'_, Database>, scene_id: SceneId) -> Result<Vec<Asset>, AppError> {
    asset_repository::list_by_scene(&db.conn(), scene_id)
}

/// Input: `asset_id`
/// Output: [`AssetVersion`]s belonging to the asset, ordered by version
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn get_asset_versions(
    db: State<'_, Database>,
    asset_id: AssetId,
) -> Result<Vec<AssetVersion>, AppError> {
    asset_version_repository::list_by_asset(&db.conn(), asset_id)
}

/// Input: `asset_id`, `version_id`
/// Output: none
/// Error: [`AppError::InvalidInput`] if `version_id` does not belong to `asset_id`;
///        [`AppError::Db`] on storage failure
/// Side effects: marks `version_id` selected and clears the previous selection
#[tauri::command]
pub fn select_asset_version(
    db: State<'_, Database>,
    asset_id: AssetId,
    version_id: AssetVersionId,
) -> Result<(), AppError> {
    asset_version_repository::select_version(&db.conn(), asset_id, version_id)?;
    tracing::info!(asset_id = %asset_id, version_id = %version_id, "asset version selected");
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────────
// Video Generation
// ────────────────────────────────────────────────────────────────────────────

fn resolve_video_prompt(
    scene: &crate::domain::scene::Scene,
    prompt: Option<String>,
) -> Result<String, AppError> {
    let prompt = prompt.unwrap_or_else(|| scene.video_prompt.clone());
    if prompt.trim().is_empty() {
        return Err(AppError::invalid_input(
            "prompt must not be empty and scene has no video_prompt to fall back to",
        ));
    }
    Ok(prompt)
}

/// Find this scene's existing video asset, if any. A scene has at most one
/// video asset; regeneration adds versions to it rather than creating another.
fn find_video_asset(conn: &Connection, scene_id: SceneId) -> Result<Option<Asset>, AppError> {
    let assets = asset_repository::list_by_scene(conn, scene_id)?;
    Ok(assets
        .into_iter()
        .find(|asset| asset.asset_type == AssetType::Video))
}

/// Get the selected image version's file path for Image-to-Video generation.
fn get_selected_image_path(conn: &Connection, scene_id: SceneId) -> Result<Option<std::path::PathBuf>, AppError> {
    let image_asset = find_image_asset(conn, scene_id)?;
    if let Some(asset) = image_asset {
        let versions = asset_version_repository::list_by_asset(conn, asset.id)?;
        if let Some(selected) = versions.into_iter().find(|v| v.selected) {
            return Ok(Some(selected.file_path));
        }
    }
    Ok(None)
}

/// Core video generation logic, independent of Tauri `State`, so it can run
/// under a plain `Connection`/`VideoProvider` (including a mock) in tests.
pub async fn generate_video_impl(
    db: &Database,
    provider: &(dyn VideoProvider + Send + Sync),
    data_root: &Path,
    scene_id: SceneId,
    prompt: Option<String>,
    use_image_to_video: bool,
) -> Result<AssetVersion, AppError> {
    let (project_id, request, existing_asset) = {
        let conn = db.conn();
        let scene = scene_repository::get_by_id(&conn, scene_id)?
            .ok_or_else(|| AppError::invalid_input(format!("scene {scene_id} does not exist")))?;

        let prompt = resolve_video_prompt(&scene, prompt)?;

        // Estimate audio duration from narration (mock until Phase 8 TTS)
        let target_duration_ms = audio_utils::estimate_audio_duration_ms(&scene.narration);
        tracing::info!(
            "estimated audio duration: {}ms from narration \"{}\"",
            target_duration_ms,
            &scene.narration[..scene.narration.len().min(50)]
        );

        // Image-to-Video: use selected image if available
        let source_image_path = if use_image_to_video {
            get_selected_image_path(&conn, scene_id)?
        } else {
            None
        };

        let existing_asset = find_video_asset(&conn, scene_id)?;

        let request = VideoRequest {
            prompt,
            source_image_path,
            target_duration_ms,
        };

        (scene.project_id, request, existing_asset)
    }; // guard dropped before the `.await` below

    // Request video generation
    tracing::info!(
        "requesting video generation: scene_id={}, use_image_to_video={}",
        scene_id,
        use_image_to_video
    );
    let job = provider.request_video(&request).await?;

    // Poll until completion
    let generated = loop {
        let status = provider.poll_status(&job).await?;
        match status {
            VideoStatus::Completed(video) => break video,
            VideoStatus::Failed(error_msg) => {
                return Err(AppError::provider(format!(
                    "video generation failed: {error_msg}"
                )));
            }
            VideoStatus::Pending | VideoStatus::InProgress => {
                // Provider handles polling interval
                continue;
            }
        }
    };

    if !storage::looks_like_mp4(&generated.bytes) {
        return Err(AppError::provider(
            "video provider returned data that is not a valid MP4",
        ));
    }

    let conn = db.conn();
    let tx = conn
        .unchecked_transaction()
        .map_err(crate::db::map_db_err)?;

    let asset = match existing_asset {
        Some(asset) => asset,
        None => {
            let asset = Asset::new(scene_id, AssetType::Video);
            asset_repository::insert(&tx, &asset)?;
            asset
        }
    };
    let version_number = next_version_number(&tx, asset.id)?;

    let file_path = storage::asset_version_path(
        data_root,
        project_id,
        asset.id,
        version_number,
        VIDEO_EXTENSION,
    );
    storage::save_new_file(&file_path, &generated.bytes)?;

    let mut version = AssetVersion::new(
        asset.id,
        version_number,
        file_path,
        VIDEO_PROVIDER_NAME,
        generated.model.clone(),
        request.prompt.clone(),
    )?;
    asset_version_repository::insert(&tx, &version)?;

    if version_number == 1 {
        asset_version_repository::select_version_in(&tx, asset.id, version.id)?;
        version.selected = true;
    }

    tx.commit().map_err(crate::db::map_db_err)?;

    tracing::info!(
        scene_id = %scene_id,
        asset_id = %asset.id,
        version = version_number,
        "video generated"
    );
    Ok(version)
}

/// Input: `scene_id`, `prompt` (optional; falls back to the scene's `video_prompt`),
///        `use_image_to_video` (true = use selected image, false = text-to-video)
/// Output: the newly created [`AssetVersion`]
/// Error: [`AppError::InvalidInput`] if the scene is unknown or no prompt is available;
///        [`AppError::Config`] if no provider API key is configured;
///        [`AppError::Provider`] on provider/network/format failure;
///        [`AppError::Db`]/[`AppError::Io`] on storage failure
/// Side effects: creates an `Asset` (first generation only), inserts a new
/// `AssetVersion` row, and writes the video file to disk. Never overwrites or
/// deletes a previous version.
#[tauri::command]
pub async fn generate_video(
    db: State<'_, Database>,
    provider: State<'_, Arc<dyn VideoProvider + Send + Sync>>,
    scene_id: SceneId,
    prompt: Option<String>,
    use_image_to_video: bool,
) -> Result<AssetVersion, AppError> {
    let data_root = paths::portable_data_dir()?;
    generate_video_impl(&db, provider.as_ref(), &data_root, scene_id, prompt, use_image_to_video).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::provider::image_provider::GeneratedImage;
    use crate::provider::video_provider::GeneratedVideo;
    use crate::provider::mock::{MockImageProvider, MockVideoProvider};
    use crate::repository::project_repository;

    const PNG_BYTES: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xAA];
    const MP4_BYTES: &[u8] = &[0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm'];

    fn open_db() -> Database {
        Database::open_in_memory().expect("open in-memory db")
    }

    fn seed_scene(db: &Database, visual_prompt: &str) -> SceneId {
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        let mut scene = Scene::new(project.id, 0, "Intro").expect("valid scene");
        scene
            .update(
                "Intro",
                "This is a test narration with ten words exactly here.",
                visual_prompt,
                "cinematic video prompt",
                5,
                crate::domain::scene::SceneStatus::Ready,
            )
            .expect("update scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");
        scene.id
    }

    fn image_provider(bytes: &[u8]) -> MockImageProvider {
        MockImageProvider::with_response(GeneratedImage {
            bytes: bytes.to_vec(),
            model: "dall-e-3".into(),
        })
    }

    fn video_provider(bytes: &[u8]) -> MockVideoProvider {
        MockVideoProvider::with_response(GeneratedVideo {
            bytes: bytes.to_vec(),
            model: "video-turbo".into(),
            format: "mp4".into(),
        })
    }

    // ──────────────────────────────────────────────────────────────────────
    // Image Generation Tests
    // ──────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn first_generation_creates_asset_and_selects_v1() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = image_provider(PNG_BYTES);

        let version = generate_image_impl(&db, &provider, data_root.path(), scene_id, None)
            .await
            .expect("generate");

        assert_eq!(version.version, 1);
        assert!(version.selected);
        assert!(version.file_path.exists());
        assert_eq!(version.model, "dall-e-3");

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(assets.len(), 1);
    }

    #[tokio::test]
    async fn second_generation_adds_version_without_changing_selection() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        let first = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("first generate");

        let second = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("second generate");

        assert_eq!(second.version, 2);
        assert!(!second.selected);
        assert_ne!(first.file_path, second.file_path);
        assert!(first.file_path.exists());
        assert!(second.file_path.exists());

        let versions = asset_version_repository::list_by_asset(&db.conn(), second.asset_id)
            .expect("list versions");
        assert_eq!(versions.len(), 2);
    }

    #[tokio::test]
    async fn select_asset_version_switches_selection() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        let first = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("first generate");
        let second = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("second generate");

        asset_version_repository::select_version(&db.conn(), first.asset_id, second.id)
            .expect("select");

        let versions =
            asset_version_repository::list_by_asset(&db.conn(), first.asset_id).expect("list");
        let first_loaded = versions.iter().find(|v| v.id == first.id).expect("first");
        let second_loaded = versions.iter().find(|v| v.id == second.id).expect("second");
        assert!(!first_loaded.selected);
        assert!(second_loaded.selected);
    }

    #[tokio::test]
    async fn rejects_unknown_scene() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");

        let error = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            SceneId::new(),
            None,
        )
        .await
        .expect_err("unknown scene");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_when_no_prompt_available() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "");

        let error = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect_err("no prompt");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn provider_failure_does_not_persist_asset() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = MockImageProvider::with_error("provider unavailable");

        let error = generate_image_impl(&db, &provider, data_root.path(), scene_id, None)
            .await
            .expect_err("provider failure");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert!(assets.is_empty());
    }

    #[tokio::test]
    async fn non_png_bytes_do_not_persist_asset() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = image_provider(b"not a png");

        let error = generate_image_impl(&db, &provider, data_root.path(), scene_id, None)
            .await
            .expect_err("invalid png");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert!(assets.is_empty());
    }

    // ──────────────────────────────────────────────────────────────────────
    // Video Generation Tests
    // ──────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn first_video_generation_creates_asset_and_selects_v1() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = video_provider(MP4_BYTES);

        let version = generate_video_impl(
            &db,
            &provider,
            data_root.path(),
            scene_id,
            None,
            false, // text-to-video
        )
        .await
        .expect("generate");

        assert_eq!(version.version, 1);
        assert!(version.selected);
        assert!(version.file_path.exists());
        assert_eq!(version.model, "video-turbo");
        assert!(version.file_path.to_str().unwrap().ends_with(".mp4"));

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.asset_type == AssetType::Video)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn second_video_generation_adds_version_without_changing_selection() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        let first = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            scene_id,
            None,
            false,
        )
        .await
        .expect("first generate");

        let second = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            scene_id,
            None,
            false,
        )
        .await
        .expect("second generate");

        assert_eq!(second.version, 2);
        assert!(!second.selected);
        assert_ne!(first.file_path, second.file_path);
        assert!(first.file_path.exists());
        assert!(second.file_path.exists());

        let versions = asset_version_repository::list_by_asset(&db.conn(), second.asset_id)
            .expect("list versions");
        assert_eq!(versions.len(), 2);
    }

    #[tokio::test]
    async fn image_to_video_uses_selected_image_path() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        // First generate an image
        let _image_version = generate_image_impl(
            &db,
            &image_provider(PNG_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("generate image");

        // Then generate video using that image
        let video_version = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            scene_id,
            None,
            true, // use_image_to_video
        )
        .await
        .expect("generate video");

        assert_eq!(video_version.version, 1);
        assert!(video_version.file_path.exists());
    }

    #[tokio::test]
    async fn text_to_video_works_when_no_image_exists() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        let version = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            scene_id,
            None,
            false, // text-to-video
        )
        .await
        .expect("generate");

        assert_eq!(version.version, 1);
        assert!(version.selected);
    }

    #[tokio::test]
    async fn estimated_audio_duration_is_computed() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        // Scene has "This is a test narration with ten words exactly here." = 10 words
        // Expected duration: 10 / 2.5 = 4 seconds = 4000ms
        let version = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            scene_id,
            None,
            false,
        )
        .await
        .expect("generate");

        assert!(version.file_path.exists());
        // The mock provider doesn't validate duration, but the impl computes it
    }

    #[tokio::test]
    async fn rejects_unknown_scene_for_video() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");

        let error = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            SceneId::new(),
            None,
            false,
        )
        .await
        .expect_err("unknown scene");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_when_no_video_prompt_available() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        let mut scene = Scene::new(project.id, 0, "Intro").expect("valid scene");
        scene
            .update(
                "Intro",
                "narration text",
                "visual prompt",
                "", // empty video_prompt
                5,
                crate::domain::scene::SceneStatus::Ready,
            )
            .expect("update scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");

        let error = generate_video_impl(
            &db,
            &video_provider(MP4_BYTES),
            data_root.path(),
            scene.id,
            None,
            false,
        )
        .await
        .expect_err("no prompt");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn video_provider_failure_does_not_persist_asset() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = MockVideoProvider::with_error("provider unavailable");

        let error = generate_video_impl(&db, &provider, data_root.path(), scene_id, None, false)
            .await
            .expect_err("provider failure");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.asset_type == AssetType::Video)
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn non_mp4_bytes_do_not_persist_video_asset() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = video_provider(b"not a video");

        let error = generate_video_impl(&db, &provider, data_root.path(), scene_id, None, false)
            .await
            .expect_err("invalid mp4");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.asset_type == AssetType::Video)
                .count(),
            0
        );
    }
}

