-- Phase 10.6: preserve the source-media position for split and trimmed clips.
-- Existing clips historically started at the beginning of their source.
ALTER TABLE timeline_clips
ADD COLUMN source_offset INTEGER NOT NULL DEFAULT 0;
