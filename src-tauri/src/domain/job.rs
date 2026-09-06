//! Job tracking for long-running and asynchronous operations.
//!
//! Jobs represent operations like image generation, video generation, TTS synthesis,
//! and timeline rendering. Each job tracks its status, progress, and any errors.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::domain::{AssetId, JobId, SceneId};
use crate::error::AppError;

/// Type of job operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobType {
    ImageGeneration,
    VideoGeneration,
    Tts,
    Render,
}

impl fmt::Display for JobType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobType::ImageGeneration => write!(f, "IMAGE_GENERATION"),
            JobType::VideoGeneration => write!(f, "VIDEO_GENERATION"),
            JobType::Tts => write!(f, "TTS"),
            JobType::Render => write!(f, "RENDER"),
        }
    }
}

/// Current status of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStatus::Queued => write!(f, "QUEUED"),
            JobStatus::Running => write!(f, "RUNNING"),
            JobStatus::Completed => write!(f, "COMPLETED"),
            JobStatus::Failed => write!(f, "FAILED"),
            JobStatus::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

impl JobStatus {
    /// Check if this is a terminal state (job cannot transition further).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }

    /// Validate state transition.
    pub fn can_transition_to(self, next: JobStatus) -> bool {
        use JobStatus::*;

        match (self, next) {
            // Terminal states cannot transition
            (Completed, _) | (Failed, _) | (Cancelled, _) => false,
            // Queued can go to Running or Cancelled
            (Queued, Running) | (Queued, Cancelled) => true,
            // Running can go to Completed, Failed, or Cancelled
            (Running, Completed) | (Running, Failed) | (Running, Cancelled) => true,
            // All other transitions are invalid
            _ => false,
        }
    }
}

/// Job represents a long-running or asynchronous operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub job_type: JobType,
    pub status: JobStatus,
    pub progress: u8,
    pub error_message: Option<String>,
    pub provider_operation_id: Option<String>,
    pub scene_id: Option<SceneId>,
    pub asset_id: Option<AssetId>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl Job {
    /// Create a new job in Running state.
    pub fn new(job_type: JobType, scene_id: Option<SceneId>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: JobId::new(),
            job_type,
            status: JobStatus::Running,
            progress: 0,
            error_message: None,
            provider_operation_id: None,
            scene_id,
            asset_id: None,
            created_at: now.clone(),
            updated_at: now,
            completed_at: None,
        }
    }

    /// Update job status with validation.
    pub fn update_status(
        &mut self,
        new_status: JobStatus,
        error_message: Option<String>,
    ) -> Result<(), AppError> {
        if !self.status.can_transition_to(new_status) {
            return Err(AppError::invalid_input(format!(
                "invalid job state transition: {} -> {}",
                self.status, new_status
            )));
        }

        self.status = new_status;
        self.error_message = error_message;
        self.updated_at = chrono::Utc::now().to_rfc3339();

        if new_status.is_terminal() {
            self.completed_at = Some(self.updated_at.clone());
        }

        Ok(())
    }

    /// Update job progress (0-100).
    pub fn update_progress(&mut self, progress: u8) -> Result<(), AppError> {
        if progress > 100 {
            return Err(AppError::invalid_input(format!(
                "job progress must be 0-100, got {progress}"
            )));
        }

        self.progress = progress;
        self.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(())
    }

    /// Mark job as cancelled.
    pub fn cancel(&mut self) -> Result<(), AppError> {
        self.update_status(JobStatus::Cancelled, None)
    }

    /// Set provider operation ID for async tracking.
    pub fn set_provider_operation_id(&mut self, operation_id: String) {
        self.provider_operation_id = Some(operation_id);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Set asset ID when asset is created.
    pub fn set_asset_id(&mut self, asset_id: AssetId) {
        self.asset_id = Some(asset_id);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_starts_in_running_state() {
        let job = Job::new(JobType::ImageGeneration, None);
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.progress, 0);
        assert!(job.error_message.is_none());
        assert!(job.completed_at.is_none());
    }

    #[test]
    fn running_can_complete() {
        let mut job = Job::new(JobType::Tts, None);
        job.update_status(JobStatus::Completed, None)
            .expect("transition");
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn running_can_fail_with_error() {
        let mut job = Job::new(JobType::VideoGeneration, None);
        job.update_status(JobStatus::Failed, Some("provider timeout".into()))
            .expect("transition");
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error_message.as_deref(), Some("provider timeout"));
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn completed_cannot_transition() {
        let mut job = Job::new(JobType::Render, None);
        job.update_status(JobStatus::Completed, None)
            .expect("complete");

        let result = job.update_status(JobStatus::Running, None);
        assert!(result.is_err());
    }

    #[test]
    fn failed_cannot_transition() {
        let mut job = Job::new(JobType::ImageGeneration, None);
        job.update_status(JobStatus::Failed, Some("error".into()))
            .expect("fail");

        let result = job.update_status(JobStatus::Running, None);
        assert!(result.is_err());
    }

    #[test]
    fn progress_must_be_0_to_100() {
        let mut job = Job::new(JobType::Tts, None);

        job.update_progress(0).expect("0 valid");
        job.update_progress(50).expect("50 valid");
        job.update_progress(100).expect("100 valid");

        let result = job.update_progress(101);
        assert!(result.is_err());
    }

    #[test]
    fn cancel_marks_job_as_cancelled() {
        let mut job = Job::new(JobType::VideoGeneration, None);
        job.cancel().expect("cancel");
        assert_eq!(job.status, JobStatus::Cancelled);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn provider_operation_id_can_be_set() {
        let mut job = Job::new(JobType::VideoGeneration, None);
        job.set_provider_operation_id("op_12345".into());
        assert_eq!(job.provider_operation_id.as_deref(), Some("op_12345"));
    }
}
