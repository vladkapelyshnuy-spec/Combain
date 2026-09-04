-- Phase 7.5: Add RoyalTechno as default Video provider
-- This migration:
-- 1. Clears any existing default Video provider
-- 2. Inserts RoyalTechno profile with model "omni-flash" (only if not already exists)
-- Note: API key must be added separately via Settings UI

-- Clear existing default Video provider (if any)
UPDATE provider_profiles
SET is_default = 0
WHERE purpose = 'VIDEO' AND is_default = 1;

-- Insert RoyalTechno Video provider as default (only if not already exists)
-- credential_alias is a UUID that will be used to store the API key in OS keyring
INSERT INTO provider_profiles (
    id,
    purpose,
    profile_name,
    provider,
    base_url,
    model,
    voice,
    is_default,
    credential_alias,
    created_at,
    updated_at
)
SELECT
    lower(hex(randomblob(16))),  -- Generate random UUID-like ID
    'VIDEO',
    'RoyalTechno Video',
    'royaltechno',
    'https://api.royaltechno.cc',
    'omni-flash',
    NULL,
    1,  -- is_default
    lower(hex(randomblob(16))),  -- Generate random credential_alias
    datetime('now'),
    datetime('now')
WHERE NOT EXISTS (
    SELECT 1 FROM provider_profiles
    WHERE provider = 'royaltechno' AND purpose = 'VIDEO'
);
