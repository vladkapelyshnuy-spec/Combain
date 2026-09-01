//! Persistence for [`ProviderProfile`]. Only `credential_alias` is stored
//! here — the secret itself lives in the OS credential store.

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::ids::ProviderProfileId;
use crate::domain::provider_profile::{ProviderProfile, ProviderPurpose};
use crate::error::AppError;

pub fn insert(conn: &Connection, profile: &ProviderProfile) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO provider_profiles
            (id, purpose, profile_name, provider, base_url, model, voice, is_default,
             credential_alias, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            profile.id.to_string(),
            purpose_to_str(profile.purpose),
            profile.profile_name,
            profile.provider,
            profile.base_url,
            profile.model,
            profile.voice,
            profile.is_default,
            profile.credential_alias,
            profile.created_at,
            profile.updated_at,
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn update(conn: &Connection, profile: &ProviderProfile) -> Result<(), AppError> {
    let rows = conn
        .execute(
            "UPDATE provider_profiles SET profile_name = ?2, provider = ?3, base_url = ?4,
                model = ?5, voice = ?6, is_default = ?7, updated_at = ?8
             WHERE id = ?1",
            params![
                profile.id.to_string(),
                profile.profile_name,
                profile.provider,
                profile.base_url,
                profile.model,
                profile.voice,
                profile.is_default,
                profile.updated_at,
            ],
        )
        .map_err(map_db_err)?;

    if rows == 0 {
        return Err(AppError::invalid_input(format!(
            "provider profile {} does not exist",
            profile.id
        )));
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: ProviderProfileId) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM provider_profiles WHERE id = ?1",
        params![id.to_string()],
    )
    .map_err(map_db_err)?;
    Ok(())
}

/// Unset `is_default` on every other profile sharing `purpose`. Callers use
/// this before inserting/updating a profile with `is_default = true`.
pub fn clear_default_for_purpose(
    conn: &Connection,
    purpose: ProviderPurpose,
    except_id: ProviderProfileId,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE provider_profiles SET is_default = 0 WHERE purpose = ?1 AND id != ?2",
        params![purpose_to_str(purpose), except_id.to_string()],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn get_by_id(
    conn: &Connection,
    id: ProviderProfileId,
) -> Result<Option<ProviderProfile>, AppError> {
    conn.query_row(
        "SELECT id, purpose, profile_name, provider, base_url, model, voice, is_default,
                credential_alias, created_at, updated_at
         FROM provider_profiles WHERE id = ?1",
        params![id.to_string()],
        row_to_profile,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn list_all(conn: &Connection) -> Result<Vec<ProviderProfile>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, purpose, profile_name, provider, base_url, model, voice, is_default,
                    credential_alias, created_at, updated_at
             FROM provider_profiles ORDER BY created_at",
        )
        .map_err(map_db_err)?;
    let rows = stmt.query_map([], row_to_profile).map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

/// The profile marked `is_default` for `purpose`, if any. Callers (e.g. the
/// OpenAI text/image adapters) use this to build their runtime config
/// without depending on env vars.
pub fn get_default_by_purpose(
    conn: &Connection,
    purpose: ProviderPurpose,
) -> Result<Option<ProviderProfile>, AppError> {
    conn.query_row(
        "SELECT id, purpose, profile_name, provider, base_url, model, voice, is_default,
                credential_alias, created_at, updated_at
         FROM provider_profiles WHERE purpose = ?1 AND is_default = 1",
        params![purpose_to_str(purpose)],
        row_to_profile,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

pub fn list_by_purpose(
    conn: &Connection,
    purpose: ProviderPurpose,
) -> Result<Vec<ProviderProfile>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, purpose, profile_name, provider, base_url, model, voice, is_default,
                    credential_alias, created_at, updated_at
             FROM provider_profiles WHERE purpose = ?1 ORDER BY created_at",
        )
        .map_err(map_db_err)?;
    let rows = stmt
        .query_map(params![purpose_to_str(purpose)], row_to_profile)
        .map_err(map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(map_db_err)
}

fn row_to_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderProfile> {
    let id: String = row.get(0)?;
    let purpose: String = row.get(1)?;

    Ok(ProviderProfile {
        id: parse_uuid_column(&id).map(ProviderProfileId::from_uuid)?,
        purpose: str_to_purpose(&purpose)?,
        profile_name: row.get(2)?,
        provider: row.get(3)?,
        base_url: row.get(4)?,
        model: row.get(5)?,
        voice: row.get(6)?,
        is_default: row.get(7)?,
        credential_alias: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn purpose_to_str(purpose: ProviderPurpose) -> &'static str {
    match purpose {
        ProviderPurpose::Text => "TEXT",
        ProviderPurpose::Image => "IMAGE",
        ProviderPurpose::Video => "VIDEO",
        ProviderPurpose::Tts => "TTS",
    }
}

fn str_to_purpose(value: &str) -> rusqlite::Result<ProviderPurpose> {
    match value {
        "TEXT" => Ok(ProviderPurpose::Text),
        "IMAGE" => Ok(ProviderPurpose::Image),
        "VIDEO" => Ok(ProviderPurpose::Video),
        "TTS" => Ok(ProviderPurpose::Tts),
        other => Err(rusqlite::Error::InvalidColumnType(
            1,
            format!("unknown provider purpose '{other}'"),
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

    fn make(purpose: ProviderPurpose, name: &str, is_default: bool) -> ProviderProfile {
        ProviderProfile::new(purpose, name, "openai", None, "gpt-4o", None, is_default)
            .expect("valid profile")
    }

    #[test]
    fn round_trips_a_profile() {
        let db = Database::open_in_memory().expect("open");
        let profile = make(ProviderPurpose::Text, "Primary", false);

        insert(&db.conn(), &profile).expect("insert");
        let loaded = get_by_id(&db.conn(), profile.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, profile);
    }

    #[test]
    fn updates_existing_profile() {
        let db = Database::open_in_memory().expect("open");
        let mut profile = make(ProviderPurpose::Image, "Primary", false);
        insert(&db.conn(), &profile).expect("insert");

        profile
            .update("Renamed", "stability", None, "sdxl", None, true)
            .expect("valid update");
        update(&db.conn(), &profile).expect("update");

        let loaded = get_by_id(&db.conn(), profile.id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded.profile_name, "Renamed");
        assert_eq!(loaded.provider, "stability");
        assert!(loaded.is_default);
    }

    #[test]
    fn update_missing_profile_errors() {
        let db = Database::open_in_memory().expect("open");
        let profile = make(ProviderPurpose::Text, "Primary", false);
        let error = update(&db.conn(), &profile).expect_err("missing profile");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn deletes_a_profile() {
        let db = Database::open_in_memory().expect("open");
        let profile = make(ProviderPurpose::Tts, "Primary", false);
        insert(&db.conn(), &profile).expect("insert");

        delete(&db.conn(), profile.id).expect("delete");
        let loaded = get_by_id(&db.conn(), profile.id).expect("query");
        assert!(loaded.is_none());
    }

    #[test]
    fn lists_profiles_by_purpose() {
        let db = Database::open_in_memory().expect("open");
        let text_profile = make(ProviderPurpose::Text, "Text profile", false);
        let image_profile = make(ProviderPurpose::Image, "Image profile", false);
        insert(&db.conn(), &text_profile).expect("insert text");
        insert(&db.conn(), &image_profile).expect("insert image");

        let text_profiles = list_by_purpose(&db.conn(), ProviderPurpose::Text).expect("list");
        assert_eq!(text_profiles.len(), 1);
        assert_eq!(text_profiles[0].id, text_profile.id);

        let all = list_all(&db.conn()).expect("list all");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn clear_default_for_purpose_unsets_other_defaults() {
        let db = Database::open_in_memory().expect("open");
        let first = make(ProviderPurpose::Text, "First", true);
        let second = make(ProviderPurpose::Text, "Second", false);
        insert(&db.conn(), &first).expect("insert first");
        insert(&db.conn(), &second).expect("insert second");

        clear_default_for_purpose(&db.conn(), ProviderPurpose::Text, second.id)
            .expect("clear defaults");

        let first_loaded = get_by_id(&db.conn(), first.id)
            .expect("query")
            .expect("present");
        assert!(!first_loaded.is_default);
    }

    #[test]
    fn get_default_by_purpose_returns_the_default_profile() {
        let db = Database::open_in_memory().expect("open");
        let first = make(ProviderPurpose::Text, "First", false);
        let second = make(ProviderPurpose::Text, "Second", true);
        insert(&db.conn(), &first).expect("insert first");
        insert(&db.conn(), &second).expect("insert second");

        let default = get_default_by_purpose(&db.conn(), ProviderPurpose::Text)
            .expect("query")
            .expect("present");
        assert_eq!(default.id, second.id);
    }

    #[test]
    fn get_default_by_purpose_returns_none_when_no_default() {
        let db = Database::open_in_memory().expect("open");
        let profile = make(ProviderPurpose::Text, "First", false);
        insert(&db.conn(), &profile).expect("insert");

        let default = get_default_by_purpose(&db.conn(), ProviderPurpose::Text).expect("query");
        assert!(default.is_none());
    }

    #[test]
    fn duplicate_credential_alias_is_rejected() {
        let db = Database::open_in_memory().expect("open");
        let first = make(ProviderPurpose::Text, "First", false);
        insert(&db.conn(), &first).expect("insert first");

        let mut second = make(ProviderPurpose::Text, "Second", false);
        second.credential_alias = first.credential_alias.clone();
        let error = insert(&db.conn(), &second).expect_err("duplicate alias");
        assert_eq!(error.code(), crate::error::ErrorCode::Db);
    }
}
