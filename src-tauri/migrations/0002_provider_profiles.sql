-- Phase 3.5: provider profiles registry. API keys are never stored here —
-- only `credential_alias`, which addresses a secret in the OS credential
-- store (see `src/secrets`).

CREATE TABLE provider_profiles (
    id TEXT PRIMARY KEY,
    purpose TEXT NOT NULL CHECK (purpose IN ('TEXT', 'IMAGE', 'VIDEO', 'TTS')),
    profile_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT,
    model TEXT NOT NULL,
    voice TEXT,
    is_default INTEGER NOT NULL DEFAULT 0,
    credential_alias TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_provider_profiles_purpose ON provider_profiles(purpose);
