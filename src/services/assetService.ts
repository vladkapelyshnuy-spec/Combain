import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Asset, AssetId, AssetVersion, AssetVersionId } from "../models/asset";
import type { SceneId } from "../models/scene";

export function generateImage(sceneId: SceneId, prompt: string): Promise<AssetVersion> {
  return invoke<AssetVersion>("generate_image", { sceneId, prompt });
}

export function getAssets(sceneId: SceneId): Promise<Asset[]> {
  return invoke<Asset[]>("get_assets", { sceneId });
}

export function getAssetVersions(assetId: AssetId): Promise<AssetVersion[]> {
  return invoke<AssetVersion[]>("get_asset_versions", { assetId });
}

export function selectAssetVersion(
  assetId: AssetId,
  versionId: AssetVersionId,
): Promise<void> {
  return invoke<void>("select_asset_version", { assetId, versionId });
}

export function getAssetFileUrl(filePath: string): string {
  return convertFileSrc(filePath);
}
