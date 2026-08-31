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
use crate::domain::ids::{AssetId, AssetVersionId, SceneId};
use crate::error::AppError;
use crate::paths;
use crate::provider::image_provider::ImageRequest;
use crate::provider::ImageProvider;
use crate::repository::{asset_repository, asset_version_repository, scene_repository};
use crate::storage;

const IMAGE_EXTENSION: &str = "png";
const IMAGE_PROVIDER_NAME: &str = "openai";

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
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
}
