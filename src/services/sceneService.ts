import { invoke } from "@tauri-apps/api/core";
import type { ProjectId } from "../models/project";
import type {
  GenerateScriptResult,
  Scene,
  SceneFields,
  SceneId,
  ScriptGenerationInput,
} from "../models/scene";

export function getScenes(projectId: ProjectId): Promise<Scene[]> {
  return invoke<Scene[]>("get_scenes", { projectId });
}

export function createScene(projectId: ProjectId, title: string): Promise<Scene> {
  return invoke<Scene>("create_scene", { projectId, title });
}

export function updateScene(sceneId: SceneId, fields: SceneFields): Promise<Scene> {
  return invoke<Scene>("update_scene", {
    sceneId,
    title: fields.title,
    narration: fields.narration,
    visualPrompt: fields.visual_prompt,
    videoPrompt: fields.video_prompt,
    duration: fields.duration,
    status: fields.status,
  });
}

export function deleteScene(sceneId: SceneId): Promise<void> {
  return invoke<void>("delete_scene", { sceneId });
}

export function reorderScenes(projectId: ProjectId, sceneIds: SceneId[]): Promise<void> {
  return invoke<void>("reorder_scenes", { projectId, sceneIds });
}

export function generateScript(input: ScriptGenerationInput): Promise<GenerateScriptResult> {
  return invoke<GenerateScriptResult>("generate_script", input);
}
