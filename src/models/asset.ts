import type { SceneId } from "./scene";

export type AssetId = string;
export type AssetVersionId = string;
export type AssetType = "IMAGE" | "VIDEO" | "AUDIO";

export type Asset = {
  id: AssetId;
  scene_id: SceneId;
  asset_type: AssetType;
};

export type AssetVersion = {
  id: AssetVersionId;
  asset_id: AssetId;
  version: number;
  file_path: string;
  provider: string;
  model: string;
  prompt: string;
  created_at: string;
  selected: boolean;
};
