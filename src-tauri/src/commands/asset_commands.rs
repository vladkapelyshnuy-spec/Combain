//! IPC commands for [`Asset`]/[`AssetVersion`]: image generation and version
//! selection. Mirrors the style of [`crate::commands::script_commands`]: plain,
//! unit-testable functions over `&Connection` wrapped by thin `#[tauri::command]`s.
//! The provider call happens between two connection scopes so the SQLite mutex
//! is never held across an `.await`.

use std::path::Path;

use rusqlite::Connection;
use tauri::State;

use crate::db::Database;
use crate::domain::asset::{Asset, AssetType};
use crate::domain::asset_version::AssetVersion;
use crate::domain::ids::{AssetId, AssetVersionId, ProviderProfileId, SceneId};
use crate::domain::provider_profile::ProviderPurpose;
use crate::error::AppError;
use crate::paths;
use crate::provider::image_provider::ImageRequest;
use crate::provider::{ImageProvider, OpenAiImageConfig, OpenAiImageProvider};
use crate::repository::{
    asset_repository, asset_version_repository, provider_profile_repository, scene_repository,
};
use crate::{secrets, storage};

const IMAGE_PROVIDER_NAME: &str = "openai";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::domain::provider_profile::ProviderProfile;
    use crate::domain::scene::Scene;
    use crate::provider::image_provider::GeneratedImage;
    use crate::provider::mock::MockImageProvider;
    use crate::repository::project_repository;

    const PNG_BYTES: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xAA];

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
                "narration",
                visual_prompt,
                "video prompt",
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
}
