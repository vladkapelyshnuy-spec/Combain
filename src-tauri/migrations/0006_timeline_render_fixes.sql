-- Phase 10.5: Timeline/Render Critical Fixes
-- Bug 1: Lock timeline clips to specific asset_version_id (not asset_id)
-- Bug 2: Add job fields for timeline_id and output_path tracking
-- Enhancement: Enable UI to retrieve render outputs and re-attach to jobs

PRAGMA foreign_keys = OFF;

-- Part 1: Add new columns to jobs table for render tracking
ALTER TABLE jobs ADD COLUMN timeline_id TEXT REFERENCES timelines(id) ON DELETE CASCADE;
ALTER TABLE jobs ADD COLUMN output_path TEXT;

-- Create index for fast lookup of latest render job by timeline
CREATE INDEX idx_jobs_timeline_created ON jobs(timeline_id, created_at DESC);

-- Part 2: Migrate timeline_clips from asset_id to asset_version_id
-- Step 2a: Create new table with correct schema
CREATE TABLE timeline_clips_new (
    id TEXT PRIMARY KEY,
    timeline_id TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE,
    asset_version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,
    track TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    duration INTEGER NOT NULL
);

-- Step 2b: Migrate existing clips to their currently selected asset version
-- This locks each clip to the version that is currently selected
INSERT INTO timeline_clips_new (id, timeline_id, asset_version_id, track, start_time, duration)
SELECT
    tc.id,
    tc.timeline_id,
    av.id as asset_version_id,
    tc.track,
    tc.start_time,
    tc.duration
FROM timeline_clips tc
INNER JOIN asset_versions av ON av.asset_id = tc.asset_id
WHERE av.selected = 1;

-- Step 2c: Drop old table and rename new one
DROP TABLE timeline_clips;
ALTER TABLE timeline_clips_new RENAME TO timeline_clips;

-- Step 2d: Recreate index
CREATE INDEX idx_timeline_clips_timeline_id ON timeline_clips(timeline_id);

PRAGMA foreign_keys = ON;
