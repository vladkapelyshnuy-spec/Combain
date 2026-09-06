import { invoke } from "@tauri-apps/api/core";
import type { AssetVersionId } from "../models/asset";
import type { ProjectId } from "../models/project";
import type { Timeline, TimelineClip, TimelineClipId, TimelineId, TimelineTrack } from "../models/timeline";

export function createTimeline(projectId: ProjectId): Promise<Timeline> {
  return invoke<Timeline>("create_timeline", { projectId });
}

export function getTimeline(projectId: ProjectId): Promise<Timeline | null> {
  return invoke<Timeline | null>("get_timeline", { projectId });
}

export function listClips(timelineId: TimelineId): Promise<TimelineClip[]> {
  return invoke<TimelineClip[]>("list_clips", { timelineId });
}

export function addClip(timelineId: TimelineId, assetVersionId: AssetVersionId, track: TimelineTrack, startTime: number, duration: number, sourceOffset = 0): Promise<TimelineClip> {
  return invoke<TimelineClip>("add_clip", { timelineId, assetVersionId, track, startTime, duration, sourceOffset });
}

export function updateClip(clipId: TimelineClipId, startTime: number, duration: number): Promise<TimelineClip> {
  return invoke<TimelineClip>("update_clip", { clipId, startTime, duration });
}

export function moveClip(clipId: TimelineClipId, newStartTime: number): Promise<TimelineClip> {
  return invoke<TimelineClip>("move_clip", { clipId, newStartTime });
}

export function trimClip(clipId: TimelineClipId, newDuration: number): Promise<TimelineClip> {
  return invoke<TimelineClip>("trim_clip", { clipId, newDuration });
}

export function splitClip(clipId: TimelineClipId, splitTime: number): Promise<[TimelineClip, TimelineClip]> {
  return invoke<[TimelineClip, TimelineClip]>("split_clip", { clipId, splitTime });
}

export function deleteClip(clipId: TimelineClipId): Promise<void> {
  return invoke<void>("delete_clip", { clipId });
}
