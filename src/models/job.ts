import type { AssetId } from "./asset";
import type { SceneId } from "./scene";
import type { TimelineId } from "./timeline";

export type JobId = string;
export type JobType = "IMAGE_GENERATION" | "VIDEO_GENERATION" | "TTS" | "RENDER";
export type JobStatus = "QUEUED" | "RUNNING" | "COMPLETED" | "FAILED" | "CANCELLED";

export type Job = {
  id: JobId;
  job_type: JobType;
  status: JobStatus;
  progress: number;
  error_message: string | null;
  provider_operation_id: string | null;
  scene_id: SceneId | null;
  asset_id: AssetId | null;
  timeline_id: TimelineId | null;
  output_path: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
};
