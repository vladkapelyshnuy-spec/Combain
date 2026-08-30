//! Persistence for [`Asset`].

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::asset::{Asset, AssetType};
use crate::domain::ids::{AssetId, SceneId};
use crate::error::AppError;

pub fn insert(conn: &Connection, asset: &Asset) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO assets (id, scene_id, asset_type) VALUES (?1, ?2, ?3)",
        params![
            asset.id.to_string(),
            asset.scene_id.to_string(),
            asset_type_to_str(asset.asset_type),
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: AssetId) -> Result<Option<Asset>, AppError> {
    conn.query_row(
        "SELECT id, scene_id, asset_type FROM assets WHERE id = ?1",
        params![id.to_string()],
        row_to_asset,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn list_by_scene(conn: &Connection, scene_id: SceneId) -> Result<Vec<Asset>, AppError> {
    let mut stmt = conn
        .prepare("SELECT id, scene_id, asset_type FROM assets WHERE scene_id = ?1")
        .map_err(map_db_err)?;
    let rows = stmt
        .query_map(params![scene_id.to_string()], row_to_asset)
        .map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

fn row_to_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<Asset> {
    let id: String = row.get(0)?;
    let scene_id: String = row.get(1)?;
    let asset_type: String = row.get(2)?;

    Ok(Asset {
        id: parse_uuid_column(&id).map(AssetId::from_uuid)?,
        scene_id: parse_uuid_column(&scene_id).map(SceneId::from_uuid)?,
        asset_type: str_to_asset_type(&asset_type)?,
    })
}

fn asset_type_to_str(asset_type: AssetType) -> &'static str {
    match asset_type {
        AssetType::Image => "IMAGE",
        AssetType::Video => "VIDEO",
        AssetType::Audio => "AUDIO",
    }
}

fn str_to_asset_type(value: &str) -> rusqlite::Result<AssetType> {
    match value {
        "IMAGE" => Ok(AssetType::Image),
        "VIDEO" => Ok(AssetType::Video),
        "AUDIO" => Ok(AssetType::Audio),
        other => Err(rusqlite::Error::InvalidColumnType(
            2,
            format!("unknown asset type '{other}'"),
            rusqlite::types::Type::Text,
        )),
    }
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
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::repository::{project_repository, scene_repository};

    fn seed_scene(db: &Database) -> SceneId {
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        let scene = Scene::new(project.id, 1, "Intro").expect("valid scene");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");
        scene.id
    }

    #[test]
    fn round_trips_an_asset() {
        let db = Database::open_in_memory().expect("open");
        let scene_id = seed_scene(&db);
        let asset = Asset::new(scene_id, AssetType::Image);

        insert(&db.conn(), &asset).expect("insert");
        let loaded = get_by_id(&db.conn(), asset.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, asset);
    }

    #[test]
    fn lists_assets_by_scene() {
        let db = Database::open_in_memory().expect("open");
        let scene_id = seed_scene(&db);
        let image = Asset::new(scene_id, AssetType::Image);
        let video = Asset::new(scene_id, AssetType::Video);
        insert(&db.conn(), &image).expect("insert image");
        insert(&db.conn(), &video).expect("insert video");

        let assets = list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(assets.len(), 2);
    }

    #[test]
    fn returns_none_for_missing_asset() {
        let db = Database::open_in_memory().expect("open");
        let loaded = get_by_id(&db.conn(), AssetId::new()).expect("query");
        assert!(loaded.is_none());
    }
}
