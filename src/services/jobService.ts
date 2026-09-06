import { invoke } from "@tauri-apps/api/core";
import type { SceneId } from "../models/scene";
import type { Job, JobId } from "../models/job";
import type { TimelineId } from "../models/timeline";

export function getJobStatus(jobId: JobId): Promise<Job> {
  return invoke<Job>("get_job_status", { jobId });
}

export function listJobs(sceneId: SceneId): Promise<Job[]> {
  return invoke<Job[]>("list_jobs", { sceneId });
}

export function cancelJob(jobId: JobId): Promise<void> {
  return invoke<void>("cancel_job", { jobId });
}

export function renderVideo(timelineId: TimelineId, outputName: string): Promise<Job> {
  return invoke<Job>("render_video", { timelineId, outputName });
}

export function getLatestRenderJob(timelineId: TimelineId): Promise<Job | null> {
  return invoke<Job | null>("get_latest_render_job", { timelineId });
}
