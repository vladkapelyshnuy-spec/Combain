export type ProjectId = string;

export type Project = {
  id: ProjectId;
  name: string;
  topic: string;
  style: string;
  created_at: string;
  updated_at: string;
};

export type ProjectFields = Pick<Project, "name" | "topic" | "style">;
