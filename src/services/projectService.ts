import { invoke } from "@tauri-apps/api/core";
import type { Project, ProjectFields, ProjectId } from "../models/project";

export type IpcError = {
  code?: string;
  message: string;
};

export function getIpcError(error: unknown): IpcError {
  if (typeof error === "string") {
    return { message: error };
  }

  if (error && typeof error === "object") {
    const candidate = error as { code?: unknown; message?: unknown };
    if (typeof candidate.message === "string") {
      return {
        code: typeof candidate.code === "string" ? candidate.code : undefined,
        message: candidate.message,
      };
    }
  }

  return { message: "An unexpected application error occurred." };
}

export function listProjects(): Promise<Project[]> {
  return invoke<Project[]>("list_projects");
}

export function createProject(fields: ProjectFields): Promise<Project> {
  return invoke<Project>("create_project", fields);
}

export function getProject(projectId: ProjectId): Promise<Project | null> {
  return invoke<Project | null>("get_project", { projectId });
}

export function updateProject(
  projectId: ProjectId,
  fields: ProjectFields,
): Promise<Project> {
  return invoke<Project>("update_project", { projectId, ...fields });
}
