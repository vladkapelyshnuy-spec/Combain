//! Persistence for [`AssetVersion`].

use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::asset_version::AssetVersion;
use crate::domain::ids::{AssetId, AssetVersionId};
use crate::error::AppError;

pub fn insert(conn: &Connection, version: &AssetVersion) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO asset_versions
            (id, asset_id, version, file_path, provider, model, prompt, created_at, selected)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            version.id.to_string(),
            version.asset_id.to_string(),
            version.version,
            path_to_str(&version.file_path),
            version.provider,
            version.model,
            version.prompt,
            version.created_at,
            version.selected,
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

/// Mark exactly one version of an asset as selected, clearing any previous selection.
pub fn select_version(
    conn: &Connection,
    asset_id: AssetId,
    version_id: AssetVersionId,
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction().map_err(map_db_err)?;
    tx.execute(
        "UPDATE asset_versions SET selected = 0 WHERE asset_id = ?1",
        params![asset_id.to_string()],
    )
    .map_err(map_db_err)?;
    let rows = tx
        .execute(
            "UPDATE asset_versions SET selected = 1 WHERE id = ?1 AND asset_id = ?2",
            params![version_id.to_string(), asset_id.to_string()],
        )
        .map_err(map_db_err)?;
    tx.commit().map_err(map_db_err)?;

    if rows == 0 {
        return Err(AppError::invalid_input(format!(
            "asset version {version_id} does not belong to asset {asset_id}"
        )));
    }
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: AssetVersionId) -> Result<Option<AssetVersion>, AppError> {
    conn.query_row(
        "SELECT id, asset_id, version, file_path, provider, model, prompt, created_at, selected
         FROM asset_versions WHERE id = ?1",
        params![id.to_string()],
        row_to_version,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn list_by_asset(conn: &Connection, asset_id: AssetId) -> Result<Vec<AssetVersion>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, asset_id, version, file_path, provider, model, prompt, created_at, selected
             FROM asset_versions WHERE asset_id = ?1 ORDER BY version",
        )
        .map_err(map_db_err)?;
    let rows = stmt
        .query_map(params![asset_id.to_string()], row_to_version)
        .map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetVersion> {
    let id: String = row.get(0)?;
    let asset_id: String = row.get(1)?;
    let file_path: String = row.get(3)?;

    Ok(AssetVersion {
        id: parse_uuid_column(&id).map(AssetVersionId::from_uuid)?,
        asset_id: parse_uuid_column(&asset_id).map(AssetId::from_uuid)?,
        version: row.get(2)?,
        file_path: PathBuf::from(file_path),
        provider: row.get(4)?,
        model: row.get(5)?,
        prompt: row.get(6)?,
        created_at: row.get(7)?,
        selected: row.get(8)?,
    })
}

fn path_to_str(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
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
    use crate::domain::asset::{Asset, AssetType};
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::repository::{asset_repository, project_repository, scene_repository};

    fn seed_asset(db: &Database) -> AssetId {
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        let scene = Scene::new(project.id, 1, "Intro").expect("valid scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");
        let asset = Asset::new(scene.id, AssetType::Image);
        asset_repository::insert(&db.conn(), &asset).expect("insert asset");
        asset.id
    }

    #[test]
    fn round_trips_a_version() {
        let db = Database::open_in_memory().expect("open");
        let asset_id = seed_asset(&db);
        let version =
            AssetVersion::new(asset_id, 1, "out/img_v1.png", "openai", "dall-e-3", "a cat")
                .expect("valid version");

        insert(&db.conn(), &version).expect("insert");
        let loaded = get_by_id(&db.conn(), version.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, version);
    }

    #[test]
    fn lists_versions_by_asset_in_order() {
        let db = Database::open_in_memory().expect("open");
        let asset_id = seed_asset(&db);
        let v1 =
            AssetVersion::new(asset_id, 1, "v1.png", "openai", "dall-e-3", "a cat").expect("v1");
        let v2 =
            AssetVersion::new(asset_id, 2, "v2.png", "openai", "dall-e-3", "a dog").expect("v2");
        insert(&db.conn(), &v2).expect("insert v2");
        insert(&db.conn(), &v1).expect("insert v1");

        let versions = list_by_asset(&db.conn(), asset_id).expect("list");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].id, v1.id);
        assert_eq!(versions[1].id, v2.id);
    }

    #[test]
    fn selecting_a_version_clears_previous_selection() {
        let db = Database::open_in_memory().expect("open");
        let asset_id = seed_asset(&db);
        let v1 =
            AssetVersion::new(asset_id, 1, "v1.png", "openai", "dall-e-3", "a cat").expect("v1");
        let v2 =
            AssetVersion::new(asset_id, 2, "v2.png", "openai", "dall-e-3", "a dog").expect("v2");
        insert(&db.conn(), &v1).expect("insert v1");
        insert(&db.conn(), &v2).expect("insert v2");

        select_version(&db.conn(), asset_id, v1.id).expect("select v1");
        select_version(&db.conn(), asset_id, v2.id).expect("select v2");

        let versions = list_by_asset(&db.conn(), asset_id).expect("list");
        let v1_loaded = versions.iter().find(|v| v.id == v1.id).expect("v1 present");
        let v2_loaded = versions.iter().find(|v| v.id == v2.id).expect("v2 present");
        assert!(!v1_loaded.selected);
        assert!(v2_loaded.selected);
    }

    #[test]
    fn selecting_mismatched_version_errors() {
        let db = Database::open_in_memory().expect("open");
        let asset_id = seed_asset(&db);
        let error = select_version(&db.conn(), asset_id, AssetVersionId::new())
            .expect_err("mismatched version");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }
}
