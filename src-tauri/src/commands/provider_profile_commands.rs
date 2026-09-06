//! IPC commands for [`ProviderProfile`](crate::domain::provider_profile::ProviderProfile).
//!
//! API keys never appear in [`ProviderProfileDto`] or in SQLite; they are
//! written to and read from the OS credential store via [`crate::secrets`],
//! addressed by the profile's `credential_alias`.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::Database;
use crate::domain::ids::ProviderProfileId;
use crate::domain::provider_profile::{ProviderProfile, ProviderPurpose};
use crate::error::AppError;
use crate::repository::provider_profile_repository;
use crate::secrets;

/// Frontend-facing view of a [`ProviderProfile`]. Carries `has_api_key`
/// instead of the key itself.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderProfileDto {
    pub id: ProviderProfileId,
    pub purpose: ProviderPurpose,
    pub profile_name: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub voice: Option<String>,
    pub is_default: bool,
    pub has_api_key: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProviderProfileInput {
    pub purpose: ProviderPurpose,
    pub profile_name: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub voice: Option<String>,
    pub is_default: bool,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderProfileInput {
    pub profile_name: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub model: String,
    pub voice: Option<String>,
    pub is_default: bool,
    /// `None` leaves the stored API key untouched; `Some` rotates it.
    pub api_key: Option<String>,
}

fn to_dto(profile: ProviderProfile) -> Result<ProviderProfileDto, AppError> {
    let has_api_key = secrets::get_secret(&profile.credential_alias)?.is_some();
    Ok(ProviderProfileDto {
        id: profile.id,
        purpose: profile.purpose,
        profile_name: profile.profile_name,
        provider: profile.provider,
        base_url: profile.base_url,
        model: profile.model,
        voice: profile.voice,
        is_default: profile.is_default,
        has_api_key,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    })
}

fn create_provider_profile_impl(
    conn: &Connection,
    input: CreateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    if input.api_key.trim().is_empty() {
        return Err(AppError::invalid_input("api_key must not be empty"));
    }

    let profile = ProviderProfile::new(
        input.purpose,
        input.profile_name,
        input.provider,
        input.base_url,
        input.model,
        input.voice,
        input.is_default,
    )?;

    let tx = conn
        .unchecked_transaction()
        .map_err(crate::db::map_db_err)?;
    if profile.is_default {
        provider_profile_repository::clear_default_for_purpose(&tx, profile.purpose, profile.id)?;
    }
    provider_profile_repository::insert(&tx, &profile)?;
    tx.commit().map_err(crate::db::map_db_err)?;

    secrets::set_secret(&profile.credential_alias, &input.api_key)?;

    tracing::info!(profile_id = %profile.id, purpose = ?profile.purpose, "provider profile created");
    to_dto(profile)
}

fn update_provider_profile_impl(
    conn: &Connection,
    profile_id: ProviderProfileId,
    input: UpdateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    let mut profile =
        provider_profile_repository::get_by_id(conn, profile_id)?.ok_or_else(|| {
            AppError::invalid_input(format!("provider profile {profile_id} does not exist"))
        })?;

    if let Some(api_key) = &input.api_key {
        if api_key.trim().is_empty() {
            return Err(AppError::invalid_input("api_key must not be empty"));
        }
    }

    profile.update(
        input.profile_name,
        input.provider,
        input.base_url,
        input.model,
        input.voice,
        input.is_default,
    )?;

    let tx = conn
        .unchecked_transaction()
        .map_err(crate::db::map_db_err)?;
    if profile.is_default {
        provider_profile_repository::clear_default_for_purpose(&tx, profile.purpose, profile.id)?;
    }
    provider_profile_repository::update(&tx, &profile)?;
    tx.commit().map_err(crate::db::map_db_err)?;

    if let Some(api_key) = input.api_key {
        secrets::set_secret(&profile.credential_alias, &api_key)?;
    }

    tracing::info!(profile_id = %profile.id, "provider profile updated");
    to_dto(profile)
}

fn delete_provider_profile_impl(
    conn: &Connection,
    profile_id: ProviderProfileId,
) -> Result<(), AppError> {
    let profile = provider_profile_repository::get_by_id(conn, profile_id)?.ok_or_else(|| {
        AppError::invalid_input(format!("provider profile {profile_id} does not exist"))
    })?;

    provider_profile_repository::delete(conn, profile_id)?;
    secrets::delete_secret(&profile.credential_alias)?;

    tracing::info!(profile_id = %profile_id, "provider profile deleted");
    Ok(())
}

/// Input: none
/// Output: all [`ProviderProfileDto`]s ordered by creation time
/// Error: [`AppError::Db`]/[`AppError::Internal`] on storage or credential-store failure
/// Side effects: none
#[tauri::command]
pub fn list_provider_profiles(
    db: State<'_, Database>,
) -> Result<Vec<ProviderProfileDto>, AppError> {
    let profiles = provider_profile_repository::list_all(&db.conn())?;
    profiles.into_iter().map(to_dto).collect()
}

/// Input: [`CreateProviderProfileInput`] (includes the raw `api_key`)
/// Output: the created [`ProviderProfileDto`]
/// Error: [`AppError::InvalidInput`] if required fields are empty;
///        [`AppError::Db`] on storage failure; [`AppError::Internal`] if the
///        credential store is unavailable
/// Side effects: inserts a new row into `provider_profiles` and writes the
/// API key to the OS credential store. If `is_default` is true, clears the
/// previous default for the same `purpose`.
#[tauri::command]
pub fn create_provider_profile(
    db: State<'_, Database>,
    input: CreateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    create_provider_profile_impl(&db.conn(), input)
}

/// Input: `profile_id`, [`UpdateProviderProfileInput`]
/// Output: the updated [`ProviderProfileDto`]
/// Error: [`AppError::InvalidInput`] if required fields are empty or
///        `profile_id` is unknown; [`AppError::Db`] on storage failure;
///        [`AppError::Internal`] if the credential store is unavailable
/// Side effects: updates the matching row in `provider_profiles`; rotates the
/// stored API key only if `api_key` is `Some`
#[tauri::command]
pub fn update_provider_profile(
    db: State<'_, Database>,
    profile_id: ProviderProfileId,
    input: UpdateProviderProfileInput,
) -> Result<ProviderProfileDto, AppError> {
    update_provider_profile_impl(&db.conn(), profile_id, input)
}

/// Input: `profile_id`
/// Output: none
/// Error: [`AppError::InvalidInput`] if `profile_id` is unknown;
///        [`AppError::Db`] on storage failure
/// Side effects: deletes the matching row from `provider_profiles` and
/// removes the API key from the OS credential store
#[tauri::command]
pub fn delete_provider_profile(
    db: State<'_, Database>,
    profile_id: ProviderProfileId,
) -> Result<(), AppError> {
    delete_provider_profile_impl(&db.conn(), profile_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_db() -> Database {
        crate::secrets::install_mock_backend();
        Database::open_in_memory().expect("open in-memory db")
    }

    fn create_input(is_default: bool) -> CreateProviderProfileInput {
        CreateProviderProfileInput {
            purpose: ProviderPurpose::Text,
            profile_name: "Primary".into(),
            provider: "openai".into(),
            base_url: Some("https://api.openai.com/v1".into()),
            model: "gpt-4o".into(),
            voice: None,
            is_default,
            api_key: "sk-test-123".into(),
        }
    }

    #[test]
    fn creates_a_profile_and_stores_the_key_in_the_credential_store() {
        let db = open_db();
        let dto = create_provider_profile_impl(&db.conn(), create_input(false)).expect("create");

        assert_eq!(dto.profile_name, "Primary");
        assert!(dto.has_api_key);

        let stored = provider_profile_repository::get_by_id(&db.conn(), dto.id)
            .expect("query")
            .expect("present");
        let secret = crate::secrets::get_secret(&stored.credential_alias).expect("get secret");
        assert_eq!(secret, Some("sk-test-123".to_string()));
    }

    #[test]
    fn create_rejects_empty_api_key() {
        let db = open_db();
        let mut input = create_input(false);
        input.api_key = "  ".into();

        let error = create_provider_profile_impl(&db.conn(), input).expect_err("empty api key");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn creating_a_second_default_clears_the_first() {
        let db = open_db();
        let first = create_provider_profile_impl(&db.conn(), create_input(true)).expect("first");
        let second = create_provider_profile_impl(&db.conn(), create_input(true)).expect("second");

        let first_loaded = provider_profile_repository::get_by_id(&db.conn(), first.id)
            .expect("query")
            .expect("present");
        assert!(!first_loaded.is_default);
        assert!(second.is_default);
    }

    #[test]
    fn lists_profiles_without_exposing_the_key() {
        let db = open_db();
        create_provider_profile_impl(&db.conn(), create_input(false)).expect("create");

        let profiles = provider_profile_repository::list_all(&db.conn())
            .expect("list")
            .into_iter()
            .map(to_dto)
            .collect::<Result<Vec<_>, _>>()
            .expect("to_dto");

        // Migrations add: 0003 RoyalTechno Video + 0004 ElevenLabs TTS = 3 total
        assert_eq!(profiles.len(), 3);
        // Find the profile we created (not the migrated ones)
        let created_profile = profiles
            .iter()
            .find(|p| p.profile_name != "RoyalTechno Video" && p.profile_name != "ElevenLabs TTS")
            .expect("find created profile");
        assert!(created_profile.has_api_key);
    }

    #[test]
    fn update_rotates_the_key_only_when_provided() {
        let db = open_db();
        let created =
            create_provider_profile_impl(&db.conn(), create_input(false)).expect("create");

        let unchanged = update_provider_profile_impl(
            &db.conn(),
            created.id,
            UpdateProviderProfileInput {
                profile_name: "Renamed".into(),
                provider: "openai".into(),
                base_url: None,
                model: "gpt-4o-mini".into(),
                voice: None,
                is_default: false,
                api_key: None,
            },
        )
        .expect("update without key");
        assert_eq!(unchanged.profile_name, "Renamed");
        assert!(unchanged.has_api_key);

        let stored = provider_profile_repository::get_by_id(&db.conn(), created.id)
            .expect("query")
            .expect("present");
        let secret = crate::secrets::get_secret(&stored.credential_alias).expect("get secret");
        assert_eq!(secret, Some("sk-test-123".to_string()));

        update_provider_profile_impl(
            &db.conn(),
            created.id,
            UpdateProviderProfileInput {
                profile_name: "Renamed".into(),
                provider: "openai".into(),
                base_url: None,
                model: "gpt-4o-mini".into(),
                voice: None,
                is_default: false,
                api_key: Some("sk-rotated".into()),
            },
        )
        .expect("update with rotated key");

        let secret = crate::secrets::get_secret(&stored.credential_alias).expect("get secret");
        assert_eq!(secret, Some("sk-rotated".to_string()));
    }

    #[test]
    fn update_missing_profile_errors() {
        let db = open_db();
        let error = update_provider_profile_impl(
            &db.conn(),
            ProviderProfileId::new(),
            UpdateProviderProfileInput {
                profile_name: "Renamed".into(),
                provider: "openai".into(),
                base_url: None,
                model: "gpt-4o".into(),
                voice: None,
                is_default: false,
                api_key: None,
            },
        )
        .expect_err("missing profile");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn delete_removes_the_profile_and_its_secret() {
        let db = open_db();
        let created =
            create_provider_profile_impl(&db.conn(), create_input(false)).expect("create");
        let stored = provider_profile_repository::get_by_id(&db.conn(), created.id)
            .expect("query")
            .expect("present");
        let alias = stored.credential_alias.clone();

        delete_provider_profile_impl(&db.conn(), created.id).expect("delete");

        let loaded = provider_profile_repository::get_by_id(&db.conn(), created.id).expect("query");
        assert!(loaded.is_none());
        let secret = crate::secrets::get_secret(&alias).expect("get secret");
        assert_eq!(secret, None);
    }

    #[test]
    fn delete_missing_profile_errors() {
        let db = open_db();
        let error = delete_provider_profile_impl(&db.conn(), ProviderProfileId::new())
            .expect_err("missing");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }
}
