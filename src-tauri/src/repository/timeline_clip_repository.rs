//! Persistence for [`TimelineClip`].

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::ids::{AssetVersionId, TimelineClipId, TimelineId};
use crate::domain::timeline_clip::{TimelineClip, Track};
use crate::error::AppError;

pub fn insert(conn: &Connection, clip: &TimelineClip) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO timeline_clips (id, timeline_id, asset_version_id, track, start_time, duration)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            clip.id.to_string(),
            clip.timeline_id.to_string(),
            clip.asset_version_id.to_string(),
            track_to_str(clip.track),
            clip.start_time as i64,
            clip.duration as i64,
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn update(conn: &Connection, clip: &TimelineClip) -> Result<(), AppError> {
    let rows = conn
        .execute(
            "UPDATE timeline_clips SET asset_version_id = ?2, track = ?3, start_time = ?4, duration = ?5 WHERE id = ?1",
            params![
                clip.id.to_string(),
                clip.asset_version_id.to_string(),
                track_to_str(clip.track),
                clip.start_time as i64,
                clip.duration as i64,
            ],
        )
        .map_err(map_db_err)?;

    if rows == 0 {
        return Err(AppError::invalid_input(format!(
            "timeline clip {} does not exist",
            clip.id
        )));
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: TimelineClipId) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM timeline_clips WHERE id = ?1",
        params![id.to_string()],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn list_by_timeline(
    conn: &Connection,
    timeline_id: TimelineId,
) -> Result<Vec<TimelineClip>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, timeline_id, asset_version_id, track, start_time, duration
             FROM timeline_clips WHERE timeline_id = ?1 ORDER BY start_time",
        )
        .map_err(map_db_err)?;
    let rows = stmt
        .query_map(params![timeline_id.to_string()], row_to_clip)
        .map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

fn row_to_clip(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimelineClip> {
    let id: String = row.get(0)?;
    let timeline_id: String = row.get(1)?;
    let asset_version_id: String = row.get(2)?;
    let track: String = row.get(3)?;

    Ok(TimelineClip {
        id: parse_uuid_column(&id).map(TimelineClipId::from_uuid)?,
        timeline_id: parse_uuid_column(&timeline_id).map(TimelineId::from_uuid)?,
        asset_version_id: parse_uuid_column(&asset_version_id).map(AssetVersionId::from_uuid)?,
        track: str_to_track(&track)?,
        start_time: row.get::<_, i64>(4)? as u64,
        duration: row.get::<_, i64>(5)? as u64,
    })
}

fn track_to_str(track: Track) -> &'static str {
    match track {
        Track::Video => "VIDEO",
        Track::Audio => "AUDIO",
    }
}

fn str_to_track(value: &str) -> rusqlite::Result<Track> {
    match value {
        "VIDEO" => Ok(Track::Video),
        "AUDIO" => Ok(Track::Audio),
        other => Err(rusqlite::Error::InvalidColumnType(
            3,
            format!("unknown track '{other}'"),
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
    use crate::domain::asset::{Asset, AssetType};
    use crate::domain::project::Project;
    use crate::domain::scene::Scene;
    use crate::domain::timeline::Timeline;
    use crate::repository::{
        asset_repository, project_repository, scene_repository, timeline_repository,
    };

    use crate::domain::asset_version::AssetVersion;

    fn seed_timeline_and_asset(db: &Database) -> (TimelineId, AssetVersionId) {
        let project = Project::new("P", "t", "s").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        let scene = Scene::new(project.id, 1, "Intro").expect("valid scene");
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
        crate::repository::asset_version_repository::insert(&db.conn(), &version)
            .expect("insert version");

        let timeline = Timeline::new(project.id);
        timeline_repository::insert(&db.conn(), &timeline).expect("insert timeline");
        (timeline.id, version.id)
    }

    #[test]
    fn round_trips_a_clip() {
        let db = Database::open_in_memory().expect("open");
        let (timeline_id, asset_version_id) = seed_timeline_and_asset(&db);
        let clip =
            TimelineClip::new(timeline_id, asset_version_id, Track::Video, 0, 5000).expect("valid clip");

        insert(&db.conn(), &clip).expect("insert");
        let clips = list_by_timeline(&db.conn(), timeline_id).expect("list");

        assert_eq!(clips, vec![clip]);
    }

    #[test]
    fn updates_a_clip() {
        let db = Database::open_in_memory().expect("open");
        let (timeline_id, asset_version_id) = seed_timeline_and_asset(&db);
        let mut clip =
            TimelineClip::new(timeline_id, asset_version_id, Track::Video, 0, 5000).expect("valid clip");
        insert(&db.conn(), &clip).expect("insert");

        clip.start_time = 1000;
        clip.duration = 3000;
        update(&db.conn(), &clip).expect("update");

        let clips = list_by_timeline(&db.conn(), timeline_id).expect("list");
        assert_eq!(clips[0].start_time, 1000);
        assert_eq!(clips[0].duration, 3000);
    }

    #[test]
    fn deletes_a_clip() {
        let db = Database::open_in_memory().expect("open");
        let (timeline_id, asset_version_id) = seed_timeline_and_asset(&db);
        let clip =
            TimelineClip::new(timeline_id, asset_version_id, Track::Audio, 0, 4000).expect("valid clip");
        insert(&db.conn(), &clip).expect("insert");

        delete(&db.conn(), clip.id).expect("delete");
        let clips = list_by_timeline(&db.conn(), timeline_id).expect("list");
        assert!(clips.is_empty());
    }

    #[test]
    fn lists_clips_ordered_by_start_time() {
        let db = Database::open_in_memory().expect("open");
        let (timeline_id, asset_version_id) = seed_timeline_and_asset(&db);
        let later =
            TimelineClip::new(timeline_id, asset_version_id, Track::Video, 5000, 2000).expect("later");
        let earlier =
            TimelineClip::new(timeline_id, asset_version_id, Track::Video, 0, 5000).expect("earlier");
        insert(&db.conn(), &later).expect("insert later");
        insert(&db.conn(), &earlier).expect("insert earlier");

        let clips = list_by_timeline(&db.conn(), timeline_id).expect("list");
        assert_eq!(clips[0].id, earlier.id);
        assert_eq!(clips[1].id, later.id);
    }
}
