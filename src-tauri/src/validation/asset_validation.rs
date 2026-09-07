//! Asset validation functions.
//!
//! Validates asset existence, selected versions, and file presence.

use rusqlite::Connection;

use crate::domain::asset::{Asset, AssetType};
use crate::domain::asset_version::AssetVersion;
use crate::domain::ids::{AssetId, SceneId};
use crate::error::AppError;
use crate::repository::{asset_repository, asset_version_repository};

/// Validate that a scene has an asset of the specified type.
///
/// Returns the asset if found, or an error if not.
pub fn validate_scene_has_asset_type(
    conn: &Connection,
    scene_id: SceneId,
    asset_type: AssetType,
) -> Result<Asset, AppError> {
    let assets = asset_repository::list_by_scene(conn, scene_id)?;

    assets
        .into_iter()
        .find(|a| a.asset_type == asset_type)
        .ok_or_else(|| {
            AppError::invalid_input(format!(
                "scene {} missing asset of type {:?}",
                scene_id, asset_type
            ))
        })
}

/// Validate that an asset has a selected version.
///
/// Returns the selected version if found, or an error if not.
pub fn validate_asset_has_selected_version(
    conn: &Connection,
    asset_id: AssetId,
) -> Result<AssetVersion, AppError> {
    let versions = asset_version_repository::list_by_asset(conn, asset_id)?;

    versions.into_iter().find(|v| v.selected).ok_or_else(|| {
        AppError::invalid_input(format!("asset {} has no selected version", asset_id))
    })
}

/// Validate that an asset version's file exists on disk.
///
/// Checks the file_path stored in the AssetVersion.
pub fn validate_asset_file_exists(version: &AssetVersion) -> Result<(), AppError> {
    if !version.file_path.exists() {
        return Err(AppError::io(format!(
            "asset version file does not exist: {}",
            version.file_path.display()
        )));
    }

    Ok(())
}

/// Check which asset types are missing for a scene.
///
/// Returns a list of AssetType that are not present for the scene.
/// Useful for reporting what still needs to be generated.
pub fn check_missing_asset_types(
    conn: &Connection,
    scene_id: SceneId,
) -> Result<Vec<AssetType>, AppError> {
    let assets = asset_repository::list_by_scene(conn, scene_id)?;
    let present_types: Vec<AssetType> = assets.iter().map(|a| a.asset_type).collect();

    let all_types = vec![AssetType::Image, AssetType::Video, AssetType::Audio];
    let missing: Vec<AssetType> = all_types
        .into_iter()
        .filter(|t| !present_types.contains(t))
        .collect();

    Ok(missing)
}

/// Validate that a scene has all required asset types (Image, Video, Audio).
///
/// Returns Ok if all types are present, or an error listing missing types.
pub fn validate_scene_has_all_assets(conn: &Connection, scene_id: SceneId) -> Result<(), AppError> {
    let missing = check_missing_asset_types(conn, scene_id)?;

    if !missing.is_empty() {
        return Err(AppError::invalid_input(format!(
            "scene {} missing asset types: {:?}",
            scene_id, missing
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::repository::{project_repository, scene_repository};

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    fn seed_scene(db: &Database) -> SceneId {
        let project = Project::new("Test", "topic", "style").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert");

        let scene = Scene::new(project.id, 1, "Scene 1").expect("scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert");
        scene.id
    }

    #[test]
    fn validate_scene_has_asset_type_returns_asset_if_found() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let asset = Asset::new(scene_id, AssetType::Image);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        let result = validate_scene_has_asset_type(&db.conn(), scene_id, AssetType::Image);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, asset.id);
    }

    #[test]
    fn validate_scene_has_asset_type_returns_error_if_not_found() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        // Create image but query for video
        let asset = Asset::new(scene_id, AssetType::Image);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        let result = validate_scene_has_asset_type(&db.conn(), scene_id, AssetType::Video);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("missing asset"));
    }

    #[test]
    fn validate_asset_has_selected_version_returns_version_if_selected() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let asset = Asset::new(scene_id, AssetType::Image);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        let version = AssetVersion::new(
            asset.id,
            1,
            std::path::PathBuf::from("/tmp/test.png"),
            "test_provider",
            "test_model",
            "test prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");
        asset_version_repository::select_version(&db.conn(), asset.id, version.id).expect("select");

        let result = validate_asset_has_selected_version(&db.conn(), asset.id);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, version.id);
    }

    #[test]
    fn validate_asset_has_selected_version_returns_error_if_none_selected() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let asset = Asset::new(scene_id, AssetType::Image);
        asset_repository::insert(&db.conn(), &asset).expect("insert");

        // Create version but don't select it
        let version = AssetVersion::new(
            asset.id,
            1,
            std::path::PathBuf::from("/tmp/test.png"),
            "test_provider",
            "test_model",
            "test prompt",
        )
        .expect("version");
        asset_version_repository::insert(&db.conn(), &version).expect("insert");

        let result = validate_asset_has_selected_version(&db.conn(), asset.id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("no selected version"));
    }

    #[test]
    fn validate_asset_file_exists_passes_for_existing_file() {
        let temp_file = tempfile::NamedTempFile::new().expect("temp file");
        let version = AssetVersion::new(
            AssetId::new(),
            1,
            temp_file.path().to_path_buf(),
            "provider",
            "model",
            "prompt",
        )
        .expect("version");

        let result = validate_asset_file_exists(&version);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_asset_file_exists_fails_for_nonexistent_file() {
        let version = AssetVersion::new(
            AssetId::new(),
            1,
            std::path::PathBuf::from("/nonexistent/file.png"),
            "provider",
            "model",
            "prompt",
        )
        .expect("version");

        let result = validate_asset_file_exists(&version);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code(), crate::error::ErrorCode::Io);
    }

    #[test]
    fn check_missing_asset_types_returns_empty_if_all_present() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let image = Asset::new(scene_id, AssetType::Image);
        let video = Asset::new(scene_id, AssetType::Video);
        let audio = Asset::new(scene_id, AssetType::Audio);
        asset_repository::insert(&db.conn(), &image).expect("insert");
        asset_repository::insert(&db.conn(), &video).expect("insert");
        asset_repository::insert(&db.conn(), &audio).expect("insert");

        let missing = check_missing_asset_types(&db.conn(), scene_id).expect("check");
        assert!(missing.is_empty());
    }

    #[test]
    fn check_missing_asset_types_returns_missing_types() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        // Only create image
        let image = Asset::new(scene_id, AssetType::Image);
        asset_repository::insert(&db.conn(), &image).expect("insert");

        let missing = check_missing_asset_types(&db.conn(), scene_id).expect("check");
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&AssetType::Video));
        assert!(missing.contains(&AssetType::Audio));
    }

    #[test]
    fn validate_scene_has_all_assets_passes_if_all_present() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let image = Asset::new(scene_id, AssetType::Image);
        let video = Asset::new(scene_id, AssetType::Video);
        let audio = Asset::new(scene_id, AssetType::Audio);
        asset_repository::insert(&db.conn(), &image).expect("insert");
        asset_repository::insert(&db.conn(), &video).expect("insert");
        asset_repository::insert(&db.conn(), &audio).expect("insert");

        let result = validate_scene_has_all_assets(&db.conn(), scene_id);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_scene_has_all_assets_fails_if_missing() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        // Only create image
        let image = Asset::new(scene_id, AssetType::Image);
        asset_repository::insert(&db.conn(), &image).expect("insert");

        let result = validate_scene_has_all_assets(&db.conn(), scene_id);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("missing asset types"));
    }
}
