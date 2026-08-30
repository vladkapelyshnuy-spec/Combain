-- Phase 3: initial schema mirroring the Phase 2 domain entities.
-- IDs are stored as TEXT (UUID string form); timestamps as TEXT (RFC3339).

CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    topic TEXT NOT NULL,
    style TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE scenes (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    order_index INTEGER NOT NULL,
    title TEXT NOT NULL,
    narration TEXT NOT NULL,
    visual_prompt TEXT NOT NULL,
    video_prompt TEXT NOT NULL,
    duration INTEGER NOT NULL,
    status TEXT NOT NULL
);

CREATE INDEX idx_scenes_project_id ON scenes(project_id);

CREATE TABLE assets (
    id TEXT PRIMARY KEY,
    scene_id TEXT NOT NULL REFERENCES scenes(id) ON DELETE CASCADE,
    asset_type TEXT NOT NULL
);

CREATE INDEX idx_assets_scene_id ON assets(scene_id);

CREATE TABLE asset_versions (
    id TEXT PRIMARY KEY,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    file_path TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt TEXT NOT NULL,
    created_at TEXT NOT NULL,
    selected INTEGER NOT NULL
);

CREATE INDEX idx_asset_versions_asset_id ON asset_versions(asset_id);

CREATE TABLE timelines (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE
);

CREATE INDEX idx_timelines_project_id ON timelines(project_id);

CREATE TABLE timeline_clips (
    id TEXT PRIMARY KEY,
    timeline_id TEXT NOT NULL REFERENCES timelines(id) ON DELETE CASCADE,
    asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    track TEXT NOT NULL,
    start_time INTEGER NOT NULL,
    duration INTEGER NOT NULL
);

CREATE INDEX idx_timeline_clips_timeline_id ON timeline_clips(timeline_id);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL,
    progress INTEGER NOT NULL,
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
