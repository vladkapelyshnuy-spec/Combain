//! Persistence for [`Job`].

use rusqlite::{params, Connection};

use crate::db::map_db_err;
use crate::domain::ids::JobId;
use crate::domain::job::{Job, JobStatus, JobType};
use crate::error::AppError;

pub fn insert(conn: &Connection, job: &Job) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO jobs (id, job_type, status, progress, error, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            job.id.to_string(),
            job_type_to_str(job.job_type),
            status_to_str(job.status),
            job.progress,
            job.error,
            job.created_at,
            job.updated_at,
        ],
    )
    .map_err(map_db_err)?;
    Ok(())
}

pub fn update(conn: &Connection, job: &Job) -> Result<(), AppError> {
    let rows = conn
        .execute(
            "UPDATE jobs SET status = ?2, progress = ?3, error = ?4, updated_at = ?5 WHERE id = ?1",
            params![
                job.id.to_string(),
                status_to_str(job.status),
                job.progress,
                job.error,
                job.updated_at,
            ],
        )
        .map_err(map_db_err)?;

    if rows == 0 {
        return Err(AppError::invalid_input(format!(
            "job {} does not exist",
            job.id
        )));
    }
    Ok(())
}

pub fn get_by_id(conn: &Connection, id: JobId) -> Result<Option<Job>, AppError> {
    conn.query_row(
        "SELECT id, job_type, status, progress, error, created_at, updated_at FROM jobs WHERE id = ?1",
        params![id.to_string()],
        row_to_job,
    )
    .map(Some)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_db_err(other)),
    })
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let id: String = row.get(0)?;
    let job_type: String = row.get(1)?;
    let status: String = row.get(2)?;

    Ok(Job {
        id: parse_uuid_column(&id).map(JobId::from_uuid)?,
        job_type: str_to_job_type(&job_type)?,
        status: str_to_status(&status)?,
        progress: row.get(3)?,
        error: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn job_type_to_str(job_type: JobType) -> &'static str {
    match job_type {
        JobType::ImageGeneration => "IMAGE_GENERATION",
        JobType::VideoGeneration => "VIDEO_GENERATION",
        JobType::Tts => "TTS",
        JobType::Render => "RENDER",
    }
}

fn str_to_job_type(value: &str) -> rusqlite::Result<JobType> {
    match value {
        "IMAGE_GENERATION" => Ok(JobType::ImageGeneration),
        "VIDEO_GENERATION" => Ok(JobType::VideoGeneration),
        "TTS" => Ok(JobType::Tts),
        "RENDER" => Ok(JobType::Render),
        other => Err(rusqlite::Error::InvalidColumnType(
            1,
            format!("unknown job type '{other}'"),
            rusqlite::types::Type::Text,
        )),
    }
}

fn status_to_str(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Queued => "QUEUED",
        JobStatus::Running => "RUNNING",
        JobStatus::Completed => "COMPLETED",
        JobStatus::Failed => "FAILED",
        JobStatus::Cancelled => "CANCELLED",
    }
}

fn str_to_status(value: &str) -> rusqlite::Result<JobStatus> {
    match value {
        "QUEUED" => Ok(JobStatus::Queued),
        "RUNNING" => Ok(JobStatus::Running),
        "COMPLETED" => Ok(JobStatus::Completed),
        "FAILED" => Ok(JobStatus::Failed),
        "CANCELLED" => Ok(JobStatus::Cancelled),
        other => Err(rusqlite::Error::InvalidColumnType(
            2,
            format!("unknown job status '{other}'"),
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

    #[test]
    fn round_trips_a_job() {
        let db = Database::open_in_memory().expect("open");
        let job = Job::new(JobType::ImageGeneration);

        insert(&db.conn(), &job).expect("insert");
        let loaded = get_by_id(&db.conn(), job.id)
            .expect("query")
            .expect("present");

        assert_eq!(loaded, job);
    }

    #[test]
    fn updates_job_progress_and_status() {
        let db = Database::open_in_memory().expect("open");
        let mut job = Job::new(JobType::Render);
        insert(&db.conn(), &job).expect("insert");

        job.status = JobStatus::Running;
        job.progress = 42;
        update(&db.conn(), &job).expect("update");

        let loaded = get_by_id(&db.conn(), job.id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded.status, JobStatus::Running);
        assert_eq!(loaded.progress, 42);
    }

    #[test]
    fn stores_failure_error_message() {
        let db = Database::open_in_memory().expect("open");
        let mut job = Job::new(JobType::Tts);
        insert(&db.conn(), &job).expect("insert");

        job.status = JobStatus::Failed;
        job.error = Some("provider timeout".into());
        update(&db.conn(), &job).expect("update");

        let loaded = get_by_id(&db.conn(), job.id)
            .expect("query")
            .expect("present");
        assert_eq!(loaded.error, Some("provider timeout".into()));
    }

    #[test]
    fn update_missing_job_errors() {
        let db = Database::open_in_memory().expect("open");
        let job = Job::new(JobType::VideoGeneration);
        let error = update(&db.conn(), &job).expect_err("missing job");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }
}
