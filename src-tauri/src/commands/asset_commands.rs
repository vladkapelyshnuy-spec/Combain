//! IPC commands for [`Asset`]/[`AssetVersion`]: image, video, and TTS generation
//! plus version selection. Mirrors the style of [`crate::commands::script_commands`]:
//! plain, unit-testable functions over `&Connection` wrapped by thin `#[tauri::command]`s.
//! The provider call happens between two connection scopes so the SQLite mutex
//! is never held across an `.await`.

use std::path::Path;

use rusqlite::Connection;
use tauri::State;

use crate::db::Database;
use crate::domain::asset::{Asset, AssetType};
use crate::domain::asset_version::AssetVersion;
use crate::domain::audio_utils;
use crate::domain::ids::{AssetId, AssetVersionId, JobId, ProviderProfileId, SceneId};
use crate::domain::provider_profile::ProviderPurpose;
use crate::domain::{Job, JobStatus, JobType};
use crate::error::AppError;
use crate::paths;
use crate::provider::image_provider::ImageRequest;
use crate::provider::tts_provider::TTSRequest;
use crate::provider::video_provider::VideoRequest;
use crate::provider::{
    ElevenLabsConfig, ElevenLabsTtsProvider, ImageProvider, OpenAiImageConfig, OpenAiImageProvider,
    OpenAiTtsConfig, OpenAiTtsProvider, OpenAiVideoConfig, OpenAiVideoProvider, RoyalTechnoConfig,
    RoyalTechnoVideoProvider, TTSProvider, VideoProvider, VideoStatus,
};
use crate::repository::{
    asset_repository, asset_version_repository, job_repository, provider_profile_repository,
    scene_repository,
};
use crate::{secrets, storage};

const IMAGE_PROVIDER_NAME: &str = "openai";
const IMAGE_EXTENSION: &str = "png";
const VIDEO_EXTENSION: &str = "mp4";
const VIDEO_PROVIDER_NAME: &str = "openai";
const TTS_PROVIDER_NAME: &str = "openai";
const TTS_EXTENSION: &str = "mp3";

#[derive(Debug)]
struct ResolvedImageConfig {
    profile_id: ProviderProfileId,
    config: OpenAiImageConfig,
}

/// Resolve the active image provider at request time. Provider profiles and
/// credentials can change while the application is running, so this must not
/// be cached in Tauri managed state.
fn resolve_image_config(db: &Database) -> Result<ResolvedImageConfig, AppError> {
    let profile =
        provider_profile_repository::get_default_by_purpose(&db.conn(), ProviderPurpose::Image)?
            .ok_or_else(|| {
                AppError::config(
                    "no default image provider profile configured; add one in Settings",
                )
            })?;

    let api_key = secrets::get_secret(&profile.credential_alias)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::config(
                "no API key stored for the default image provider profile; add one in Settings",
            )
        })?;

    Ok(ResolvedImageConfig {
        profile_id: profile.id,
        config: OpenAiImageConfig::from_profile(&profile, Some(api_key)),
    })
}

#[derive(Debug)]
enum ResolvedVideoProvider {
    OpenAI {
        profile_id: ProviderProfileId,
        config: OpenAiVideoConfig,
    },
    RoyalTechno {
        profile_id: ProviderProfileId,
        config: RoyalTechnoConfig,
    },
}

/// Resolve the active video provider at request time. Provider profiles and
/// credentials can change while the application is running, so this must not
/// be cached in Tauri managed state.
fn resolve_video_provider(db: &Database) -> Result<ResolvedVideoProvider, AppError> {
    let profile =
        provider_profile_repository::get_default_by_purpose(&db.conn(), ProviderPurpose::Video)?
            .ok_or_else(|| {
                AppError::config(
                    "no default video provider profile configured; add one in Settings",
                )
            })?;

    let api_key = secrets::get_secret(&profile.credential_alias)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::config(
                "no API key stored for the default video provider profile; add one in Settings",
            )
        })?;

    // Route based on provider
    match profile.provider.as_str() {
        "royaltechno" => Ok(ResolvedVideoProvider::RoyalTechno {
            profile_id: profile.id,
            config: RoyalTechnoConfig::new(
                api_key,
                profile.base_url.clone(),
                Some(profile.model.clone()),
            ),
        }),
        "openai" => Ok(ResolvedVideoProvider::OpenAI {
            profile_id: profile.id,
            config: OpenAiVideoConfig::from_profile(&profile, Some(api_key)),
        }),
        unknown => Err(AppError::config(format!(
            "unknown video provider: {unknown} (supported: openai, royaltechno)"
        ))),
    }
}

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
    // Validate scene exists first (before creating job)
    let (project_id, request, existing_asset) = {
        let conn = db.conn();
        let scene = scene_repository::get_by_id(&conn, scene_id)?
            .ok_or_else(|| AppError::invalid_input(format!("scene {scene_id} does not exist")))?;
        let prompt = resolve_prompt(&scene, prompt)?;
        let existing_asset = find_image_asset(&conn, scene_id)?;
        (scene.project_id, ImageRequest { prompt }, existing_asset)
    };

    // Create job after validation
    let job = Job::new(JobType::ImageGeneration, Some(scene_id));
    let job_id = job.id;
    {
        let conn = db.conn();
        job_repository::insert(&conn, &job)?;
    }

    // Execute generation (wrapped in match for job status tracking)
    let result: Result<AssetVersion, AppError> = async {

        let generated = provider.generate_image(&request).await?;
        let image_extension = storage::image_extension(&generated.bytes).ok_or_else(|| {
            AppError::provider("image provider returned data that is not a supported image format")
        })?;

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
            image_extension,
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

        // Update job with asset_id before committing
        job_repository::update_asset_id(&tx, job_id, asset.id)?;

        tx.commit().map_err(crate::db::map_db_err)?;

        tracing::info!(scene_id = %scene_id, asset_id = %asset.id, version = version_number, "image generated");
        Ok(version)
    }
    .await;

    // Update job status based on result
    let conn = db.conn();
    match &result {
        Ok(_) => {
            job_repository::update_status(&conn, job_id, JobStatus::Completed, None)?;
        }
        Err(e) => {
            job_repository::update_status(&conn, job_id, JobStatus::Failed, Some(e.to_string()))?;
        }
    }

    result
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
    scene_id: SceneId,
    prompt: Option<String>,
) -> Result<AssetVersion, AppError> {
    let resolved = resolve_image_config(&db)?;
    tracing::debug!(
        profile_id = %resolved.profile_id,
        base_url = %resolved.config.base_url,
        model = %resolved.config.model,
        "image provider resolved for request"
    );
    let provider = OpenAiImageProvider::new(resolved.config);
    let data_root = paths::portable_data_dir()?;
    generate_image_impl(&db, &provider, &data_root, scene_id, prompt).await
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
fn get_selected_image_path(
    conn: &Connection,
    scene_id: SceneId,
) -> Result<Option<std::path::PathBuf>, AppError> {
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
    // Validate scene exists first (before creating job)
    let (project_id, request, existing_asset) = {
        let conn = db.conn();
        let scene = scene_repository::get_by_id(&conn, scene_id)?
            .ok_or_else(|| AppError::invalid_input(format!("scene {scene_id} does not exist")))?;

        let prompt = resolve_video_prompt(&scene, prompt)?;
        let target_duration_ms = audio_utils::estimate_audio_duration_ms(&scene.narration);
        tracing::info!(
            "estimated audio duration: {}ms from narration \"{}\"",
            target_duration_ms,
            &scene.narration[..scene.narration.len().min(50)]
        );

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
            scene_duration_sec: scene.duration,
        };

        (scene.project_id, request, existing_asset)
    };

    // Create job after validation
    let job_entity = Job::new(JobType::VideoGeneration, Some(scene_id));
    let job_id = job_entity.id;
    {
        let conn = db.conn();
        job_repository::insert(&conn, &job_entity)?;
    }

    // Execute generation (wrapped for job status tracking)
    let result: Result<AssetVersion, AppError> = async {
        tracing::info!(
            "requesting video generation: scene_id={}, use_image_to_video={}",
            scene_id,
            use_image_to_video
        );
        let provider_job = provider.request_video(&request).await?;

        // Store provider operation ID
        {
            let conn = db.conn();
            job_repository::update_provider_operation_id(
                &conn,
                job_id,
                provider_job.operation_id.clone(),
            )?;
        }

        // Poll until completion
        let generated = loop {
            let status = provider.poll_status(&provider_job).await?;
            match status {
                VideoStatus::Completed(video) => break video,
                VideoStatus::Failed(error_msg) => {
                    return Err(AppError::provider(format!(
                        "video generation failed: {error_msg}"
                    )));
                }
                VideoStatus::Pending | VideoStatus::InProgress => {
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

        // Update job with asset_id before committing
        job_repository::update_asset_id(&tx, job_id, asset.id)?;

        tx.commit().map_err(crate::db::map_db_err)?;

        tracing::info!(
            scene_id = %scene_id,
            asset_id = %asset.id,
            version = version_number,
            "video generated"
        );
        Ok(version)
    }
    .await;

    // Update job status based on result
    let conn = db.conn();
    match &result {
        Ok(_) => {
            job_repository::update_status(&conn, job_id, JobStatus::Completed, None)?;
        }
        Err(e) => {
            job_repository::update_status(&conn, job_id, JobStatus::Failed, Some(e.to_string()))?;
        }
    }

    result
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
    scene_id: SceneId,
    prompt: Option<String>,
    use_image_to_video: bool,
) -> Result<AssetVersion, AppError> {
    let data_root = paths::portable_data_dir()?;
    let resolved = resolve_video_provider(&db)?;

    match resolved {
        ResolvedVideoProvider::RoyalTechno { profile_id, config } => {
            tracing::debug!(
                profile_id = %profile_id,
                base_url = %config.base_url,
                model = %config.model,
                "RoyalTechno video provider resolved for request"
            );
            let provider = RoyalTechnoVideoProvider::new(config);
            generate_video_impl(
                &db,
                &provider,
                &data_root,
                scene_id,
                prompt,
                use_image_to_video,
            )
            .await
        }
        ResolvedVideoProvider::OpenAI { profile_id, config } => {
            tracing::debug!(
                profile_id = %profile_id,
                base_url = %config.base_url,
                model = %config.model,
                "OpenAI video provider resolved for request"
            );
            let provider = OpenAiVideoProvider::new(config);
            generate_video_impl(
                &db,
                &provider,
                &data_root,
                scene_id,
                prompt,
                use_image_to_video,
            )
            .await
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// TTS Generation
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum ResolvedTtsProvider {
    OpenAI {
        profile_id: ProviderProfileId,
        config: OpenAiTtsConfig,
    },
    ElevenLabs {
        profile_id: ProviderProfileId,
        config: ElevenLabsConfig,
    },
}

/// Resolve the active TTS provider at request time. Provider profiles and
/// credentials can change while the application is running, so this must not
/// be cached in Tauri managed state.
fn resolve_tts_provider(db: &Database) -> Result<ResolvedTtsProvider, AppError> {
    let profile =
        provider_profile_repository::get_default_by_purpose(&db.conn(), ProviderPurpose::Tts)?
            .ok_or_else(|| {
                AppError::config("no default TTS provider profile configured; add one in Settings")
            })?;

    let api_key = secrets::get_secret(&profile.credential_alias)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::config(
                "no API key stored for the default TTS provider profile; add one in Settings",
            )
        })?;

    // Route based on provider
    match profile.provider.as_str() {
        "elevenlabs" => Ok(ResolvedTtsProvider::ElevenLabs {
            profile_id: profile.id,
            config: ElevenLabsConfig::from_profile(&profile, Some(api_key)),
        }),
        "openai" => Ok(ResolvedTtsProvider::OpenAI {
            profile_id: profile.id,
            config: OpenAiTtsConfig::from_profile(&profile, Some(api_key)),
        }),
        unknown => Err(AppError::config(format!(
            "unknown TTS provider: {unknown} (supported: openai, elevenlabs)"
        ))),
    }
}

/// Find this scene's existing audio asset, if any. A scene has at most one
/// audio asset; regeneration adds versions to it rather than creating another.
fn find_audio_asset(conn: &Connection, scene_id: SceneId) -> Result<Option<Asset>, AppError> {
    let assets = asset_repository::list_by_scene(conn, scene_id)?;
    Ok(assets
        .into_iter()
        .find(|asset| asset.asset_type == AssetType::Audio))
}

/// Core TTS generation logic, independent of Tauri `State`, so it can run
/// under a plain `Connection`/`TTSProvider` (including a mock) in tests.
pub async fn generate_tts_impl(
    db: &Database,
    provider: &(dyn TTSProvider + Send + Sync),
    data_root: &Path,
    scene_id: SceneId,
    voice: Option<String>,
) -> Result<AssetVersion, AppError> {
    // Validate scene exists first (before creating job)
    let (project_id, request, existing_asset, narration) = {
        let conn = db.conn();
        let scene = scene_repository::get_by_id(&conn, scene_id)?
            .ok_or_else(|| AppError::invalid_input(format!("scene {scene_id} does not exist")))?;

        if scene.narration.trim().is_empty() {
            return Err(AppError::invalid_input(
                "scene narration must not be empty for TTS generation",
            ));
        }

        let existing_asset = find_audio_asset(&conn, scene_id)?;
        let request = TTSRequest {
            text: scene.narration.clone(),
            voice,
        };

        (
            scene.project_id,
            request,
            existing_asset,
            scene.narration.clone(),
        )
    };

    // Create job after validation
    let job = Job::new(JobType::Tts, Some(scene_id));
    let job_id = job.id;
    {
        let conn = db.conn();
        job_repository::insert(&conn, &job)?;
    }

    // Execute generation (wrapped for job status tracking)
    let result: Result<AssetVersion, AppError> = async {
        let generated = provider.synthesize(&request).await?;

        if !storage::looks_like_mp3(&generated.bytes) {
            return Err(AppError::provider(
                "TTS provider returned data that is not a valid MP3",
            ));
        }

        let (asset, is_new_asset) = match existing_asset {
            Some(asset) => (asset, false),
            None => (Asset::new(scene_id, AssetType::Audio), true),
        };

        let version_number = if is_new_asset {
            1
        } else {
            let conn = db.conn();
            next_version_number(&conn, asset.id)?
        };

        let file_path = storage::asset_version_path(
            data_root,
            project_id,
            asset.id,
            version_number,
            TTS_EXTENSION,
        );

        storage::save_new_file(&file_path, &generated.bytes)?;
        let duration_ms = audio_utils::extract_audio_duration_ms(&file_path, &narration).await?;
        tracing::info!(
            "TTS audio duration: {}ms (extracted from file)",
            duration_ms
        );

        let conn = db.conn();
        let tx = conn
            .unchecked_transaction()
            .map_err(crate::db::map_db_err)?;

        if is_new_asset {
            asset_repository::insert(&tx, &asset)?;
        }

        let mut version = AssetVersion::new(
            asset.id,
            version_number,
            file_path,
            TTS_PROVIDER_NAME,
            generated.model.clone(),
            format!("voice: {}", generated.voice),
        )?;
        asset_version_repository::insert(&tx, &version)?;

        if version_number == 1 {
            asset_version_repository::select_version_in(&tx, asset.id, version.id)?;
            version.selected = true;
        }

        // Update job with asset_id before committing
        job_repository::update_asset_id(&tx, job_id, asset.id)?;

        tx.commit().map_err(crate::db::map_db_err)?;

        tracing::info!(
            scene_id = %scene_id,
            asset_id = %asset.id,
            version = version_number,
            "TTS audio generated"
        );
        Ok(version)
    }
    .await;

    // Update job status based on result
    let conn = db.conn();
    match &result {
        Ok(_) => {
            job_repository::update_status(&conn, job_id, JobStatus::Completed, None)?;
        }
        Err(e) => {
            job_repository::update_status(&conn, job_id, JobStatus::Failed, Some(e.to_string()))?;
        }
    }

    result
}

/// Input: `scene_id`, `voice` (optional; falls back to profile's default voice)
/// Output: the newly created [`AssetVersion`]
/// Error: [`AppError::InvalidInput`] if the scene is unknown or narration is empty;
///        [`AppError::Config`] if no provider API key is configured;
///        [`AppError::Provider`] on provider/network/format failure;
///        [`AppError::Db`]/[`AppError::Io`] on storage failure
/// Side effects: creates an `Asset` (first generation only), inserts a new
/// `AssetVersion` row, and writes the audio file to disk. Never overwrites or
/// deletes a previous version.
#[tauri::command]
pub async fn generate_tts(
    db: State<'_, Database>,
    scene_id: SceneId,
    voice: Option<String>,
) -> Result<AssetVersion, AppError> {
    let data_root = paths::portable_data_dir()?;
    let resolved = resolve_tts_provider(&db)?;

    match resolved {
        ResolvedTtsProvider::ElevenLabs { profile_id, config } => {
            tracing::debug!(
                profile_id = %profile_id,
                base_url = %config.base_url,
                model = %config.model,
                default_voice = %config.default_voice,
                "ElevenLabs TTS provider resolved for request"
            );
            let provider = ElevenLabsTtsProvider::new(config);
            generate_tts_impl(&db, &provider, &data_root, scene_id, voice).await
        }
        ResolvedTtsProvider::OpenAI { profile_id, config } => {
            tracing::debug!(
                profile_id = %profile_id,
                base_url = %config.base_url,
                model = %config.model,
                default_voice = %config.default_voice,
                "OpenAI TTS provider resolved for request"
            );
            let provider = OpenAiTtsProvider::new(config);
            generate_tts_impl(&db, &provider, &data_root, scene_id, voice).await
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Job Management Commands
// ────────────────────────────────────────────────────────────────────────────

/// Input: `job_id`
/// Output: [`Job`] with current status
/// Error: [`AppError::InvalidInput`] if job not found;
///        [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn get_job_status(db: State<'_, Database>, job_id: JobId) -> Result<Job, AppError> {
    job_repository::get_by_id(&db.conn(), job_id)?
        .ok_or_else(|| AppError::invalid_input(format!("job {job_id} does not exist")))
}

/// Input: `scene_id`
/// Output: list of [`Job`]s for the scene
/// Error: [`AppError::Db`] on storage failure
/// Side effects: none
#[tauri::command]
pub fn list_jobs(db: State<'_, Database>, scene_id: SceneId) -> Result<Vec<Job>, AppError> {
    job_repository::list_by_scene(&db.conn(), scene_id)
}

/// Input: `job_id`
/// Output: unit (success indicator)
/// Error: [`AppError::InvalidInput`] if job not found or invalid state transition;
///        [`AppError::Db`] on storage failure
/// Side effects: marks job status as `CANCELLED`
/// Note: does not make any API call to provider; provider may continue processing
#[tauri::command]
pub fn cancel_job(db: State<'_, Database>, job_id: JobId) -> Result<(), AppError> {
    let conn = db.conn();

    // Verify job exists and can be cancelled
    let job = job_repository::get_by_id(&conn, job_id)?
        .ok_or_else(|| AppError::invalid_input(format!("job {job_id} does not exist")))?;

    if job.status.is_terminal() {
        return Err(AppError::invalid_input(format!(
            "job {} is already in terminal state: {}",
            job_id, job.status
        )));
    }

    job_repository::update_status(&conn, job_id, JobStatus::Cancelled, None)?;
    tracing::info!(job_id = %job_id, "job cancelled");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::domain::provider_profile::ProviderProfile;
    use crate::domain::scene::Scene;
    use crate::provider::image_provider::GeneratedImage;
    use crate::provider::mock::{MockImageProvider, MockTtsProvider, MockVideoProvider};
    use crate::provider::tts_provider::GeneratedAudio;
    use crate::provider::video_provider::GeneratedVideo;
    use crate::repository::project_repository;

    const PNG_BYTES: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xAA];
    const MP4_BYTES: &[u8] = &[
        0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm',
    ];
    const MP3_BYTES: &[u8] = &[0xFF, 0xFB, 0x90, 0x44, 0x00, 0x00, 0x00, 0x00];

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

    fn tts_provider(bytes: &[u8]) -> MockTtsProvider {
        MockTtsProvider::with_response(GeneratedAudio {
            bytes: bytes.to_vec(),
            model: "tts-1".into(),
            voice: "alloy".into(),
            format: "mp3".into(),
        })
    }

    fn image_profile(name: &str, model: &str, is_default: bool) -> ProviderProfile {
        ProviderProfile::new(
            ProviderPurpose::Image,
            name,
            "openai",
            Some("https://api.example.com/v1".into()),
            model,
            None,
            is_default,
        )
        .expect("valid image profile")
    }

    // ──────────────────────────────────────────────────────────────────────
    // Image Generation Tests
    // ──────────────────────────────────────────────────────────────────────

    #[test]
    fn image_config_tracks_secret_rotation_and_default_switches() {
        let db = open_db();
        let first = image_profile("First", "image-model-one", true);
        provider_profile_repository::insert(&db.conn(), &first).expect("insert first profile");

        crate::secrets::install_mock_backend();
        secrets::set_secret(&first.credential_alias, "image-key-one").expect("store first key");

        let initial = resolve_image_config(&db).expect("resolve first profile");
        assert_eq!(initial.profile_id, first.id);
        assert_eq!(initial.config.api_key.as_deref(), Some("image-key-one"));
        assert_eq!(initial.config.model, "image-model-one");

        secrets::set_secret(&first.credential_alias, "image-key-rotated")
            .expect("rotate first key");
        let rotated = resolve_image_config(&db).expect("resolve rotated key");
        assert_eq!(rotated.profile_id, first.id);
        assert_eq!(rotated.config.api_key.as_deref(), Some("image-key-rotated"));

        let second = image_profile("Second", "image-model-two", true);
        provider_profile_repository::clear_default_for_purpose(
            &db.conn(),
            ProviderPurpose::Image,
            second.id,
        )
        .expect("clear old default");
        provider_profile_repository::insert(&db.conn(), &second).expect("insert second profile");
        secrets::set_secret(&second.credential_alias, "image-key-two").expect("store second key");

        let switched = resolve_image_config(&db).expect("resolve second profile");
        assert_eq!(switched.profile_id, second.id);
        assert_eq!(switched.config.api_key.as_deref(), Some("image-key-two"));
        assert_eq!(switched.config.model, "image-model-two");
    }

    #[test]
    fn image_config_reports_missing_secure_secret() {
        let db = open_db();
        let profile = image_profile("Missing secret", "image-model", true);
        provider_profile_repository::insert(&db.conn(), &profile).expect("insert profile");

        crate::secrets::install_mock_backend();
        let error = resolve_image_config(&db).expect_err("missing secret must fail");

        assert_eq!(error.code(), crate::error::ErrorCode::Config);
        assert!(error
            .to_string()
            .contains("no API key stored for the default image provider profile"));
    }

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

    #[tokio::test]
    async fn jpeg_generation_uses_matching_file_extension() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let jpeg_bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00];

        let version = generate_image_impl(
            &db,
            &image_provider(&jpeg_bytes),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("generate JPEG");

        assert_eq!(
            version
                .file_path
                .extension()
                .and_then(|value| value.to_str()),
            Some("jpg")
        );
        assert_eq!(
            std::fs::read(&version.file_path).expect("read image"),
            jpeg_bytes
        );
    }

    // ──────────────────────────────────────────────────────────────────────
    // TTS Generation Tests
    // ──────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn first_tts_generation_creates_asset_and_selects_v1() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = tts_provider(MP3_BYTES);

        let version = generate_tts_impl(&db, &provider, data_root.path(), scene_id, None)
            .await
            .expect("generate");

        assert_eq!(version.version, 1);
        assert!(version.selected);
        assert!(version.file_path.exists());
        assert_eq!(version.model, "tts-1");
        assert!(version.file_path.to_str().unwrap().ends_with(".mp3"));
        assert!(version.prompt.contains("voice:"));

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.asset_type == AssetType::Audio)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn second_tts_generation_adds_version_without_changing_selection() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");

        let first = generate_tts_impl(
            &db,
            &tts_provider(MP3_BYTES),
            data_root.path(),
            scene_id,
            None,
        )
        .await
        .expect("first generate");

        let second = generate_tts_impl(
            &db,
            &tts_provider(MP3_BYTES),
            data_root.path(),
            scene_id,
            Some("nova".into()),
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
    async fn rejects_unknown_scene_for_tts() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");

        let error = generate_tts_impl(
            &db,
            &tts_provider(MP3_BYTES),
            data_root.path(),
            SceneId::new(),
            None,
        )
        .await
        .expect_err("unknown scene");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_when_narration_is_empty() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        let mut scene = Scene::new(project.id, 0, "Intro").expect("valid scene");
        scene
            .update(
                "Intro",
                "", // empty narration
                "visual prompt",
                "video prompt",
                5,
                crate::domain::scene::SceneStatus::Ready,
            )
            .expect("update scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");

        let error = generate_tts_impl(
            &db,
            &tts_provider(MP3_BYTES),
            data_root.path(),
            scene.id,
            None,
        )
        .await
        .expect_err("empty narration");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn tts_provider_failure_does_not_persist_asset() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = MockTtsProvider::with_error("provider unavailable");

        let error = generate_tts_impl(&db, &provider, data_root.path(), scene_id, None)
            .await
            .expect_err("provider failure");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.asset_type == AssetType::Audio)
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn non_mp3_bytes_do_not_persist_audio_asset() {
        let db = open_db();
        let data_root = tempfile::tempdir().expect("temp dir");
        let scene_id = seed_scene(&db, "a nebula");
        let provider = tts_provider(b"not an audio file");

        let error = generate_tts_impl(&db, &provider, data_root.path(), scene_id, None)
            .await
            .expect_err("invalid mp3");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let assets = asset_repository::list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(
            assets
                .iter()
                .filter(|a| a.asset_type == AssetType::Audio)
                .count(),
            0
        );
    }
}
