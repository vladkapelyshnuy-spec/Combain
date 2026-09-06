-- Phase 9: Job System Enhancement
-- The jobs table was created in 0001_initial with minimal schema.
-- This migration adds new columns and constraints for full job tracking.

-- Add new columns for enhanced job tracking
ALTER TABLE jobs ADD COLUMN error_message TEXT;
ALTER TABLE jobs ADD COLUMN provider_operation_id TEXT;
ALTER TABLE jobs ADD COLUMN scene_id TEXT REFERENCES scenes(id) ON DELETE CASCADE;
ALTER TABLE jobs ADD COLUMN asset_id TEXT REFERENCES assets(id) ON DELETE SET NULL;
ALTER TABLE jobs ADD COLUMN completed_at TEXT;

-- Update existing columns (SQLite doesn't support ALTER COLUMN, so we note the expectations)
-- job_type should contain: 'IMAGE_GENERATION', 'VIDEO_GENERATION', 'TTS', 'RENDER'
-- status should contain: 'QUEUED', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED'
-- progress should be 0-100

-- Create indexes for efficient queries
CREATE INDEX idx_jobs_scene_id ON jobs(scene_id);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE INDEX idx_jobs_created_at ON jobs(created_at DESC);

