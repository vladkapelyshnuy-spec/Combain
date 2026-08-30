//! Job entity: tracks a long-running or asynchronous operation
//! (AI generation, TTS synthesis, or final render).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::ids::JobId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobType {
    ImageGeneration,
    VideoGeneration,
    Tts,
    Render,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub job_type: JobType,
    pub status: JobStatus,
    /// Progress in the range 0-100.
    pub progress: u8,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    pub fn new(job_type: JobType) -> Self {
        let now = Utc::now();
        Self {
            id: JobId::new(),
            job_type,
            status: JobStatus::Queued,
            progress: 0,
            error: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_job_in_queued_state() {
        let job = Job::new(JobType::ImageGeneration);
        assert_eq!(job.status, JobStatus::Queued);
        assert_eq!(job.progress, 0);
        assert!(job.error.is_none());
    }

    #[test]
    fn serializes_and_deserializes() {
        let job = Job::new(JobType::Render);
        let json = serde_json::to_string(&job).expect("serialize");
        let back: Job = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(job, back);
    }

    #[test]
    fn status_and_type_use_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&JobType::ImageGeneration).expect("serialize"),
            "\"IMAGE_GENERATION\""
        );
        assert_eq!(
            serde_json::to_string(&JobStatus::Completed).expect("serialize"),
            "\"COMPLETED\""
        );
    }
}
