import type { AssetVersionId } from "./asset";
import type { ProjectId } from "./project";

export type TimelineId = string;
export type TimelineClipId = string;
export type TimelineTrack = "VIDEO" | "AUDIO";

export type Timeline = {
  id: TimelineId;
  project_id: ProjectId;
};

export type TimelineClip = {
  id: TimelineClipId;
  timeline_id: TimelineId;
  asset_version_id: AssetVersionId;
  track: TimelineTrack;
  start_time: number;
  duration: number;
  source_offset: number;
};

export type TimelineClipTiming = Pick<TimelineClip, "start_time" | "duration">;
