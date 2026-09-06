-- Phase 8.5: Add ElevenLabs as default TTS provider
-- This migration:
-- 1. Clears any existing default TTS provider
-- 2. Inserts ElevenLabs profile with model "eleven_multilingual_v2" and Russian female voice
-- Note: API key must be added separately via Settings UI

-- Clear existing default TTS provider (if any)
UPDATE provider_profiles
SET is_default = 0
WHERE purpose = 'TTS' AND is_default = 1;

-- Insert ElevenLabs TTS provider as default (only if not already exists)
-- credential_alias is a UUID that will be used to store the API key in OS keyring
-- voice: Natalia (Russian female voice, ID: dHAwRJVaEPhU907QLTPW)
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
    'TTS',
    'ElevenLabs TTS',
    'elevenlabs',
    'https://api.elevenlabs.io',
    'eleven_multilingual_v2',
    'dHAwRJVaEPhU907QLTPW',  -- Natalia (Russian female voice)
    1,  -- is_default
    lower(hex(randomblob(16))),  -- Generate random credential_alias
    datetime('now'),
    datetime('now')
WHERE NOT EXISTS (
    SELECT 1 FROM provider_profiles
    WHERE provider = 'elevenlabs' AND purpose = 'TTS'
);
