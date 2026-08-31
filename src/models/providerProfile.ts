export const providerPurposes = ["text", "image", "video", "tts"] as const;

export type ProviderPurpose = (typeof providerPurposes)[number];
export type ProviderProfileId = string;

export type ProviderProfile = {
  id: ProviderProfileId;
  purpose: ProviderPurpose;
  profileName: string;
  provider: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  isDefault: boolean;
  /** Reserved for the future TTS provider contract. */
  voice?: string;
};

export type ProviderProfileDraft = Omit<ProviderProfile, "id" | "purpose">;
export type ProviderProfilesByPurpose = Record<ProviderPurpose, ProviderProfile[]>;

export const emptyProviderProfiles = (): ProviderProfilesByPurpose => ({
  text: [], image: [], video: [], tts: [],
});
