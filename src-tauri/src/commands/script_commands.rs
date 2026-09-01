//! IPC command for the AI Director: turns a topic into a persisted script.
//!
//! Mirrors the style of [`crate::commands::project_commands`]: plain,
//! unit-testable functions over `&Connection` wrapped by a thin
//! `#[tauri::command]`. The provider call happens between two separate
//! connection scopes so the SQLite mutex is never held across an `.await`.

use rusqlite::Connection;
use serde::Serialize;
use tauri::State;

use crate::db::Database;
use crate::domain::ids::{ProjectId, ProviderProfileId};
use crate::domain::provider_profile::ProviderPurpose;
use crate::domain::scene::Scene;
use crate::error::AppError;
use crate::provider::text_provider::{validate_script_response, ScriptRequest, ScriptResponse};
use crate::provider::TextProvider;
use crate::provider::{OpenAiConfig, OpenAiTextProvider};
use crate::repository::{project_repository, provider_profile_repository, scene_repository};
use crate::secrets;

/// Result of a successful `generate_script` call.
#[derive(Debug, Clone, Serialize)]
pub struct GenerateScriptResult {
    pub title: String,
    pub scenes: Vec<Scene>,
}

#[derive(Debug)]
struct ResolvedTextConfig {
    profile_id: ProviderProfileId,
    config: OpenAiConfig,
}

fn resolve_text_config(db: &Database) -> Result<ResolvedTextConfig, AppError> {
    let profile =
        provider_profile_repository::get_default_by_purpose(&db.conn(), ProviderPurpose::Text)?
            .ok_or_else(|| {
                AppError::config("no default text provider profile configured; add one in Settings")
            })?;
    let api_key = secrets::get_secret(&profile.credential_alias)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            AppError::config(
                "no API key stored for the default text provider profile; add one in Settings",
            )
        })?;

    Ok(ResolvedTextConfig {
        profile_id: profile.id,
        config: OpenAiConfig::from_profile(&profile, Some(api_key)),
    })
}

fn validate_request(request: &ScriptRequest) -> Result<(), AppError> {
    if request.topic.trim().is_empty() {
        return Err(AppError::invalid_input("topic must not be empty"));
    }
    if request.language.trim().is_empty() {
        return Err(AppError::invalid_input("language must not be empty"));
    }
    if request.duration_seconds == 0 {
        return Err(AppError::invalid_input(
            "duration_seconds must be greater than zero",
        ));
    }
    Ok(())
}

/// Ensure `project_id` exists and has no scenes yet.
fn ensure_project_ready_for_script(
    conn: &Connection,
    project_id: ProjectId,
) -> Result<(), AppError> {
    project_repository::get_by_id(conn, project_id)?
        .ok_or_else(|| AppError::invalid_input(format!("project {project_id} does not exist")))?;

    let existing = scene_repository::list_by_project(conn, project_id)?;
    if !existing.is_empty() {
        return Err(AppError::conflict(format!(
            "project {project_id} already has scenes"
        )));
    }
    Ok(())
}

/// Convert a validated [`ScriptResponse`] into persisted [`Scene`]s, in one transaction.
fn persist_script(
    conn: &Connection,
    project_id: ProjectId,
    response: &ScriptResponse,
) -> Result<Vec<Scene>, AppError> {
    let tx = conn
        .unchecked_transaction()
        .map_err(crate::db::map_db_err)?;
    let mut scenes = Vec::with_capacity(response.scenes.len());

    for (order, generated) in response.scenes.iter().enumerate() {
        let mut scene = Scene::new(project_id, order as u32, generated.title.clone())?;
        scene.update(
            generated.title.clone(),
            generated.narration.clone(),
            generated.visual_prompt.clone(),
            generated.video_prompt.clone(),
            generated.duration,
            crate::domain::scene::SceneStatus::Ready,
        )?;
        scene_repository::insert(&tx, &scene)?;
        scenes.push(scene);
    }

    tx.commit().map_err(crate::db::map_db_err)?;
    Ok(scenes)
}

/// Core logic, independent of Tauri `State`, so it can run under a plain
/// `Connection` and any `TextProvider` implementation (including a mock) in tests.
pub async fn generate_script_impl(
    db: &Database,
    provider: &(dyn TextProvider + Send + Sync),
    project_id: ProjectId,
    topic: String,
    style: String,
    duration_seconds: u32,
    language: String,
) -> Result<GenerateScriptResult, AppError> {
    let request = ScriptRequest {
        topic,
        style,
        duration_seconds,
        language,
    };
    validate_request(&request)?;

    {
        let conn = db.conn();
        ensure_project_ready_for_script(&conn, project_id)?;
    } // guard dropped before the `.await` below

    let response = provider.generate_script(&request).await?;
    validate_script_response(&response)?;

    let conn = db.conn();
    // Re-check for a concurrent generation that landed between the two connection scopes.
    ensure_project_ready_for_script(&conn, project_id)?;
    let scenes = persist_script(&conn, project_id, &response)?;

    tracing::info!(project_id = %project_id, scene_count = scenes.len(), "script generated");
    Ok(GenerateScriptResult {
        title: response.title.clone(),
        scenes,
    })
}

/// Input: `project_id`, `topic`, `style`, `duration_seconds`, `language`
/// Output: [`GenerateScriptResult`] (title + persisted scenes)
/// Error: [`AppError::InvalidInput`] on bad request or unknown project;
///        [`AppError::Conflict`] if the project already has scenes;
///        [`AppError::Config`] if no provider API key is configured;
///        [`AppError::Provider`] on provider/network/schema failure;
///        [`AppError::Db`] on storage failure
/// Side effects: inserts one row per generated scene into `scenes`
#[tauri::command]
pub async fn generate_script(
    db: State<'_, Database>,
    project_id: ProjectId,
    topic: String,
    style: String,
    duration_seconds: u32,
    language: String,
) -> Result<GenerateScriptResult, AppError> {
    let resolved = resolve_text_config(&db)?;
    tracing::info!(provider_profile_id = %resolved.profile_id, "text provider resolved");
    let provider = OpenAiTextProvider::new(resolved.config);
    generate_script_impl(
        &db,
        &provider,
        project_id,
        topic,
        style,
        duration_seconds,
        language,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::domain::provider_profile::ProviderProfile;
    use crate::provider::mock::MockTextProvider;
    use crate::provider::text_provider::GeneratedScene;

    fn open_db() -> Database {
        Database::open_in_memory().expect("open in-memory db")
    }

    fn seed_project(db: &Database) -> ProjectId {
        let project = Project::new("P", "space", "cinematic").expect("valid project");
        project_repository::insert(&db.conn(), &project).expect("insert project");
        project.id
    }

    fn sample_response() -> ScriptResponse {
        ScriptResponse {
            title: "5 facts about space".into(),
            scenes: vec![
                GeneratedScene {
                    number: 1,
                    title: "Intro".into(),
                    narration: "Hello there".into(),
                    visual_prompt: "a nebula".into(),
                    video_prompt: "slow pan".into(),
                    duration: 5,
                },
                GeneratedScene {
                    number: 2,
                    title: "Fact one".into(),
                    narration: "Space is big".into(),
                    visual_prompt: "a planet".into(),
                    video_prompt: "zoom out".into(),
                    duration: 5,
                },
            ],
        }
    }

    #[test]
    fn resolves_current_default_profile_id_and_secure_key() {
        crate::secrets::install_mock_backend();
        let db = open_db();
        let profile = ProviderProfile::new(
            ProviderPurpose::Text,
            "Primary",
            "openai-compatible",
            Some("https://api.example.com/v1".into()),
            "deepseek-v4-flash",
            None,
            true,
        )
        .expect("profile");
        provider_profile_repository::insert(&db.conn(), &profile).expect("insert profile");
        crate::secrets::set_secret(&profile.credential_alias, "secret-one").expect("store key");

        let first = resolve_text_config(&db).expect("resolve first key");
        assert_eq!(first.profile_id, profile.id);
        assert_eq!(first.config.api_key.as_deref(), Some("secret-one"));
        assert_eq!(first.config.model, "deepseek-v4-flash");

        crate::secrets::set_secret(&profile.credential_alias, "secret-two").expect("rotate key");
        let rotated = resolve_text_config(&db).expect("resolve rotated key");
        assert_eq!(rotated.profile_id, profile.id);
        assert_eq!(rotated.config.api_key.as_deref(), Some("secret-two"));
    }

    #[test]
    fn resolving_default_profile_without_secure_key_errors() {
        crate::secrets::install_mock_backend();
        let db = open_db();
        let profile = ProviderProfile::new(
            ProviderPurpose::Text,
            "Missing key",
            "openai",
            None,
            "gpt-4o-mini",
            None,
            true,
        )
        .expect("profile");
        provider_profile_repository::insert(&db.conn(), &profile).expect("insert profile");

        let error = resolve_text_config(&db).expect_err("missing key");
        assert_eq!(error.code(), crate::error::ErrorCode::Config);
        assert!(error.to_string().contains("no API key stored"));
    }

    #[tokio::test]
    async fn generates_and_persists_scenes() {
        let db = open_db();
        let project_id = seed_project(&db);
        let provider = MockTextProvider::with_response(sample_response());

        let result = generate_script_impl(
            &db,
            &provider,
            project_id,
            "space facts".into(),
            "cinematic".into(),
            30,
            "en".into(),
        )
        .await
        .expect("generate script");

        assert_eq!(result.title, "5 facts about space");
        assert_eq!(result.scenes.len(), 2);
        assert_eq!(result.scenes[0].order, 0);
        assert_eq!(result.scenes[1].order, 1);

        let persisted = scene_repository::list_by_project(&db.conn(), project_id).expect("list");
        assert_eq!(persisted.len(), 2);
    }

    #[tokio::test]
    async fn rejects_empty_topic() {
        let db = open_db();
        let project_id = seed_project(&db);
        let provider = MockTextProvider::with_response(sample_response());

        let error = generate_script_impl(
            &db,
            &provider,
            project_id,
            "  ".into(),
            "cinematic".into(),
            30,
            "en".into(),
        )
        .await
        .expect_err("empty topic");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_zero_duration() {
        let db = open_db();
        let project_id = seed_project(&db);
        let provider = MockTextProvider::with_response(sample_response());

        let error = generate_script_impl(
            &db,
            &provider,
            project_id,
            "space facts".into(),
            "cinematic".into(),
            0,
            "en".into(),
        )
        .await
        .expect_err("zero duration");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_unknown_project() {
        let db = open_db();
        let provider = MockTextProvider::with_response(sample_response());

        let error = generate_script_impl(
            &db,
            &provider,
            ProjectId::new(),
            "space facts".into(),
            "cinematic".into(),
            30,
            "en".into(),
        )
        .await
        .expect_err("unknown project");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_project_that_already_has_scenes() {
        let db = open_db();
        let project_id = seed_project(&db);
        let existing = Scene::new(project_id, 0, "Existing").expect("valid scene");
        scene_repository::insert(&db.conn(), &existing).expect("seed scene");
        let provider = MockTextProvider::with_response(sample_response());

        let error = generate_script_impl(
            &db,
            &provider,
            project_id,
            "space facts".into(),
            "cinematic".into(),
            30,
            "en".into(),
        )
        .await
        .expect_err("project already has scenes");
        assert_eq!(error.code(), crate::error::ErrorCode::Conflict);
    }

    #[tokio::test]
    async fn provider_failure_does_not_persist_scenes() {
        let db = open_db();
        let project_id = seed_project(&db);
        let provider = MockTextProvider::with_error("provider timeout");

        let error = generate_script_impl(
            &db,
            &provider,
            project_id,
            "space facts".into(),
            "cinematic".into(),
            30,
            "en".into(),
        )
        .await
        .expect_err("provider failure");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let persisted = scene_repository::list_by_project(&db.conn(), project_id).expect("list");
        assert!(persisted.is_empty());
    }

    #[tokio::test]
    async fn invalid_structured_response_does_not_persist_scenes() {
        let db = open_db();
        let project_id = seed_project(&db);
        let mut bad_response = sample_response();
        bad_response.scenes[0].narration = "".into();
        let provider = MockTextProvider::with_response(bad_response);

        let error = generate_script_impl(
            &db,
            &provider,
            project_id,
            "space facts".into(),
            "cinematic".into(),
            30,
            "en".into(),
        )
        .await
        .expect_err("invalid response");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);

        let persisted = scene_repository::list_by_project(&db.conn(), project_id).expect("list");
        assert!(persisted.is_empty());
    }
}
