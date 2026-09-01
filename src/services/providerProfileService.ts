import { invoke } from "@tauri-apps/api/core";
import type {
  CreateProviderProfileInput,
  ProviderProfile,
  ProviderProfileId,
  UpdateProviderProfileInput,
} from "../models/providerProfile";

export function listProviderProfiles(): Promise<ProviderProfile[]> {
  return invoke<ProviderProfile[]>("list_provider_profiles");
}

export function createProviderProfile(
  input: CreateProviderProfileInput,
): Promise<ProviderProfile> {
  return invoke<ProviderProfile>("create_provider_profile", { input });
}

export function updateProviderProfile(
  profileId: ProviderProfileId,
  input: UpdateProviderProfileInput,
): Promise<ProviderProfile> {
  return invoke<ProviderProfile>("update_provider_profile", { profileId, input });
}

export function deleteProviderProfile(profileId: ProviderProfileId): Promise<void> {
  return invoke<void>("delete_provider_profile", { profileId });
}
