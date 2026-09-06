//! Job repository: persistence for long-running operations.

use rusqlite::{params, Connection, Row};

use crate::domain::{AssetId, Job, JobId, JobStatus, JobType, SceneId};
use crate::error::AppError;

/// Parse job_type string from database.
fn parse_job_type(s: &str) -> Result<JobType, AppError> {
    match s {
        "IMAGE_GENERATION" => Ok(JobType::ImageGeneration),
        "VIDEO_GENERATION" => Ok(JobType::VideoGeneration),
        "TTS" => Ok(JobType::Tts),
        "RENDER" => Ok(JobType::Render),
        _ => Err(AppError::db(format!("unknown job_type: {s}"))),
    }
}

/// Parse job_status string from database.
fn parse_job_status(s: &str) -> Result<JobStatus, AppError> {
    match s {
        "QUEUED" => Ok(JobStatus::Queued),
        "RUNNING" => Ok(JobStatus::Running),
        "COMPLETED" => Ok(JobStatus::Completed),
        "FAILED" => Ok(JobStatus::Failed),
        "CANCELLED" => Ok(JobStatus::Cancelled),
        _ => Err(AppError::db(format!("unknown job_status: {s}"))),
    }
}

/// Map database row to Job.
fn row_to_job(row: &Row) -> rusqlite::Result<Job> {
    let job_type_str: String = row.get("job_type")?;
    let status_str: String = row.get("status")?;

    let scene_id_str: Option<String> = row.get("scene_id").ok().flatten();
    let asset_id_str: Option<String> = row.get("asset_id").ok().flatten();

    // Handle both old 'error' column and new 'error_message' column
    let error_message: Option<String> = row
        .get("error_message")
        .ok()
        .flatten()
        .or_else(|| row.get("error").ok().flatten());

    let provider_operation_id: Option<String> = row.get("provider_operation_id").ok().flatten();
    let completed_at: Option<String> = row.get("completed_at").ok().flatten();

    Ok(Job {
        id: JobId::from_uuid(row.get::<_, String>("id")?.parse().unwrap()),
        job_type: parse_job_type(&job_type_str).unwrap(),
        status: parse_job_status(&status_str).unwrap(),
        progress: row.get("progress")?,
        error_message,
        provider_operation_id,
        scene_id: scene_id_str
            .and_then(|s| s.parse().ok())
            .map(SceneId::from_uuid),
        asset_id: asset_id_str
            .and_then(|s| s.parse().ok())
            .map(AssetId::from_uuid),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        completed_at,
    })
}

/// Insert a new job.
pub fn insert(conn: &Connection, job: &Job) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO jobs (
            id, job_type, status, progress, error_message,
            provider_operation_id, scene_id, asset_id,
            created_at, updated_at, completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            job.id.to_string(),
            job.job_type.to_string(),
            job.status.to_string(),
            job.progress,
            job.error_message,
            job.provider_operation_id,
            job.scene_id.as_ref().map(|id| id.to_string()),
            job.asset_id.as_ref().map(|id| id.to_string()),
            job.created_at,
            job.updated_at,
            job.completed_at,
        ],
    )
    .map_err(crate::db::map_db_err)?;
    Ok(())
}

/// Get job by ID.
pub fn get_by_id(conn: &Connection, id: JobId) -> Result<Option<Job>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_type, status, progress, error_message,
             provider_operation_id, scene_id, asset_id,
             created_at, updated_at, completed_at
             FROM jobs WHERE id = ?1",
        )
        .map_err(crate::db::map_db_err)?;

    let mut rows = stmt
        .query(params![id.to_string()])
        .map_err(crate::db::map_db_err)?;

    match rows.next().map_err(crate::db::map_db_err)? {
        Some(row) => Ok(Some(row_to_job(row).map_err(crate::db::map_db_err)?)),
        None => Ok(None),
    }
}

/// List all jobs for a scene.
pub fn list_by_scene(conn: &Connection, scene_id: SceneId) -> Result<Vec<Job>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_type, status, progress, error_message,
             provider_operation_id, scene_id, asset_id,
             created_at, updated_at, completed_at
             FROM jobs WHERE scene_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(crate::db::map_db_err)?;

    let rows = stmt
        .query_map(params![scene_id.to_string()], row_to_job)
        .map_err(crate::db::map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(crate::db::map_db_err)
}

/// List jobs by status.
pub fn list_by_status(conn: &Connection, status: JobStatus) -> Result<Vec<Job>, AppError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, job_type, status, progress, error_message,
             provider_operation_id, scene_id, asset_id,
             created_at, updated_at, completed_at
             FROM jobs WHERE status = ?1
             ORDER BY created_at DESC",
        )
        .map_err(crate::db::map_db_err)?;

    let rows = stmt
        .query_map(params![status.to_string()], row_to_job)
        .map_err(crate::db::map_db_err)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(crate::db::map_db_err)
}

/// Update job status and error message.
pub fn update_status(
    conn: &Connection,
    id: JobId,
    status: JobStatus,
    error_message: Option<String>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let completed_at = if status.is_terminal() {
        Some(now.clone())
    } else {
        None
    };

    conn.execute(
        "UPDATE jobs SET status = ?1, error_message = ?2, updated_at = ?3, completed_at = ?4
         WHERE id = ?5",
        params![
            status.to_string(),
            error_message,
            now,
            completed_at,
            id.to_string()
        ],
    )
    .map_err(crate::db::map_db_err)?;
    Ok(())
}

/// Update job progress.
pub fn update_progress(conn: &Connection, id: JobId, progress: u8) -> Result<(), AppError> {
    if progress > 100 {
        return Err(AppError::invalid_input(format!(
            "progress must be 0-100, got {progress}"
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE jobs SET progress = ?1, updated_at = ?2 WHERE id = ?3",
        params![progress, now, id.to_string()],
    )
    .map_err(crate::db::map_db_err)?;
    Ok(())
}

/// Update provider operation ID.
pub fn update_provider_operation_id(
    conn: &Connection,
    id: JobId,
    operation_id: String,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE jobs SET provider_operation_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![operation_id, now, id.to_string()],
    )
    .map_err(crate::db::map_db_err)?;
    Ok(())
}

/// Update asset ID.
pub fn update_asset_id(conn: &Connection, id: JobId, asset_id: AssetId) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE jobs SET asset_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![asset_id.to_string(), now, id.to_string()],
    )
    .map_err(crate::db::map_db_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::domain::Project;
    use crate::domain::Scene;
    use crate::repository::{project_repository, scene_repository};

    fn open_db() -> Database {
        Database::open_in_memory().expect("open db")
    }

    fn seed_scene(db: &Database) -> SceneId {
        let project = Project::new("Test", "topic", "style").expect("project");
        project_repository::insert(&db.conn(), &project).expect("insert project");

        let mut scene = Scene::new(project.id, 0, "Scene 1").expect("scene");
        scene
            .update(
                "Scene 1",
                "narration",
                "visual",
                "video",
                5,
                crate::domain::scene::SceneStatus::Ready,
            )
            .expect("update");
        scene_repository::insert(&db.conn(), &scene).expect("insert scene");
        scene.id
    }

    #[test]
    fn insert_and_get_job() {
        let db = open_db();
        let scene_id = seed_scene(&db);
        let job = Job::new(JobType::ImageGeneration, Some(scene_id));

        insert(&db.conn(), &job).expect("insert");

        let loaded = get_by_id(&db.conn(), job.id).expect("get").expect("found");
        assert_eq!(loaded.id, job.id);
        assert_eq!(loaded.job_type, JobType::ImageGeneration);
        assert_eq!(loaded.status, JobStatus::Running);
    }

    #[test]
    fn list_by_scene_returns_scene_jobs() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let job1 = Job::new(JobType::ImageGeneration, Some(scene_id));
        let job2 = Job::new(JobType::Tts, Some(scene_id));

        insert(&db.conn(), &job1).expect("insert 1");
        insert(&db.conn(), &job2).expect("insert 2");

        let jobs = list_by_scene(&db.conn(), scene_id).expect("list");
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn update_status_changes_job_state() {
        let db = open_db();
        let scene_id = seed_scene(&db);
        let job = Job::new(JobType::VideoGeneration, Some(scene_id));

        insert(&db.conn(), &job).expect("insert");

        update_status(&db.conn(), job.id, JobStatus::Completed, None).expect("update");

        let loaded = get_by_id(&db.conn(), job.id).expect("get").expect("found");
        assert_eq!(loaded.status, JobStatus::Completed);
        assert!(loaded.completed_at.is_some());
    }

    #[test]
    fn update_status_stores_error_on_failure() {
        let db = open_db();
        let scene_id = seed_scene(&db);
        let job = Job::new(JobType::Tts, Some(scene_id));

        insert(&db.conn(), &job).expect("insert");

        update_status(
            &db.conn(),
            job.id,
            JobStatus::Failed,
            Some("provider timeout".into()),
        )
        .expect("update");

        let loaded = get_by_id(&db.conn(), job.id).expect("get").expect("found");
        assert_eq!(loaded.status, JobStatus::Failed);
        assert_eq!(loaded.error_message.as_deref(), Some("provider timeout"));
    }

    #[test]
    fn update_progress_modifies_job() {
        let db = open_db();
        let scene_id = seed_scene(&db);
        let job = Job::new(JobType::ImageGeneration, Some(scene_id));

        insert(&db.conn(), &job).expect("insert");
        update_progress(&db.conn(), job.id, 50).expect("update");

        let loaded = get_by_id(&db.conn(), job.id).expect("get").expect("found");
        assert_eq!(loaded.progress, 50);
    }

    #[test]
    fn list_by_status_filters_correctly() {
        let db = open_db();
        let scene_id = seed_scene(&db);

        let job1 = Job::new(JobType::ImageGeneration, Some(scene_id));
        let job2 = Job::new(JobType::Tts, Some(scene_id));

        insert(&db.conn(), &job1).expect("insert 1");
        insert(&db.conn(), &job2).expect("insert 2");

        update_status(&db.conn(), job1.id, JobStatus::Completed, None).expect("complete");

        let running = list_by_status(&db.conn(), JobStatus::Running).expect("list running");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, job2.id);

        let completed = list_by_status(&db.conn(), JobStatus::Completed).expect("list completed");
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, job1.id);
    }
}
