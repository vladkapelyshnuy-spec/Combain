import type { ProjectId } from "./project";

export type SceneId = string;

export type SceneStatus = "DRAFT" | "READY" | "FAILED";

export type Scene = {
  id: SceneId;
  project_id: ProjectId;
  order: number;
  title: string;
  narration: string;
  visual_prompt: string;
  video_prompt: string;
  duration: number;
  status: SceneStatus;
};

export type SceneFields = Pick<
  Scene,
  "title" | "narration" | "visual_prompt" | "video_prompt" | "duration" | "status"
>;

export type ScriptGenerationInput = {
  projectId: ProjectId;
  topic: string;
  style: string;
  durationSeconds: number;
  language: string;
};

export type GenerateScriptResult = {
  title: string;
  scenes: Scene[];
};
