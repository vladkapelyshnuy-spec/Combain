export const providerPurposes = ["TEXT", "IMAGE", "VIDEO", "TTS"] as const;

export type ProviderPurpose = (typeof providerPurposes)[number];
export type ProviderProfileId = string;

/** Backend view. The API key itself is never included — only `has_api_key`. */
export type ProviderProfile = {
  id: ProviderProfileId;
  purpose: ProviderPurpose;
  profile_name: string;
  provider: string;
  base_url: string | null;
  model: string;
  voice: string | null;
  is_default: boolean;
  has_api_key: boolean;
  created_at: string;
  updated_at: string;
};

export type CreateProviderProfileInput = {
  purpose: ProviderPurpose;
  profile_name: string;
  provider: string;
  base_url: string | null;
  model: string;
  voice: string | null;
  is_default: boolean;
  api_key: string;
};

export type UpdateProviderProfileInput = {
  profile_name: string;
  provider: string;
  base_url: string | null;
  model: string;
  voice: string | null;
  is_default: boolean;
  /** `null` leaves the stored API key untouched; a string rotates it. */
  api_key: string | null;
};
