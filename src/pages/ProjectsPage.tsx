import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Button } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import type { Project, ProjectFields, ProjectId } from "../models/project";
import {
  createProject,
  getIpcError,
  getProject,
  listProjects,
} from "../services/projectService";

type ProjectsPageProps = {
  onSelectProject: (project: Project) => void;
  selectedProject: Project | null;
};

const initialFields: ProjectFields = { name: "", topic: "", style: "" };

function formatUpdatedAt(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "Recently updated"
    : `Updated ${new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
      }).format(date)}`;
}

function ProjectListSkeleton() {
  return (
    <div className="project-list" aria-label="Loading projects" aria-busy="true">
      {[0, 1, 2].map((item) => (
        <div className="project-card project-card-skeleton" key={item}>
          <span className="skeleton-line skeleton-title" />
          <span className="skeleton-line" />
          <span className="skeleton-line skeleton-short" />
        </div>
      ))}
    </div>
  );
}

export function ProjectsPage({ onSelectProject, selectedProject }: ProjectsPageProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [fields, setFields] = useState<ProjectFields>(initialFields);
  const [nameError, setNameError] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [openingProjectId, setOpeningProjectId] = useState<ProjectId | null>(null);
  const submitGuard = useRef(false);

  const loadProjects = useCallback(async () => {
    setIsLoading(true);
    setLoadError(null);
    try {
      setProjects(await listProjects());
    } catch (error) {
      setLoadError(getIpcError(error).message);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadProjects();
  }, [loadProjects]);

  const handleFieldChange = (field: keyof ProjectFields, value: string) => {
    setFields((current) => ({ ...current, [field]: value }));
    if (field === "name" && value.trim()) setNameError(null);
    setSubmitError(null);
  };

  const handleCreate = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (submitGuard.current) return;

    const normalizedFields = {
      name: fields.name.trim(),
      topic: fields.topic.trim(),
      style: fields.style.trim(),
    };

    if (!normalizedFields.name) {
      setNameError("Project name is required.");
      return;
    }

    submitGuard.current = true;
    setIsSubmitting(true);
    setSubmitError(null);
    try {
      const created = await createProject(normalizedFields);
      onSelectProject(created);
      setProjects((current) => [
        ...current.filter((project) => project.id !== created.id),
        created,
      ]);
      setFields(initialFields);
      setNameError(null);

      try {
        setProjects(await listProjects());
      } catch (refreshError) {
        setLoadError(
          `Project created, but the list could not be refreshed: ${getIpcError(refreshError).message}`,
        );
      }
    } catch (error) {
      setSubmitError(getIpcError(error).message);
    } finally {
      submitGuard.current = false;
      setIsSubmitting(false);
    }
  };

  const handleOpen = async (projectId: ProjectId) => {
    if (openingProjectId) return;
    setOpeningProjectId(projectId);
    setLoadError(null);
    try {
      const project = await getProject(projectId);
      if (!project) {
        setLoadError("The selected project no longer exists.");
        await loadProjects();
        return;
      }
      onSelectProject(project);
      setProjects((current) =>
        current.map((item) => (item.id === project.id ? project : item)),
      );
    } catch (error) {
      setLoadError(getIpcError(error).message);
    } finally {
      setOpeningProjectId(null);
    }
  };

  return (
    <div className="workspace-page projects-page">
      <header className="workspace-header">
        <div className="workspace-heading">
          <span className="workspace-heading-icon" aria-hidden="true">
            <SectionIcon name="projects" />
          </span>
          <div>
            <p className="eyebrow">Project workspace</p>
            <h1>Projects</h1>
            <p className="lead">Create a project or continue from your local workspace.</p>
          </div>
        </div>
        <span className="project-count" aria-live="polite">
          {isLoading ? "Loading…" : `${projects.length} ${projects.length === 1 ? "project" : "projects"}`}
        </span>
      </header>

      <div className="projects-layout">
        <section className="projects-panel" aria-labelledby="project-list-title">
          <div className="projects-panel-header">
            <div>
              <p className="eyebrow">Local projects</p>
              <h2 id="project-list-title">Your projects</h2>
            </div>
            <Button variant="secondary" onClick={() => void loadProjects()} disabled={isLoading}>
              Refresh
            </Button>
          </div>

          {loadError ? (
            <div className="inline-error" role="alert">
              <strong>Could not load projects</strong>
              <p>{loadError}</p>
              <Button variant="secondary" onClick={() => void loadProjects()}>Try again</Button>
            </div>
          ) : null}

          {isLoading ? <ProjectListSkeleton /> : null}

          {!isLoading && !loadError && projects.length === 0 ? (
            <div className="projects-empty">
              <span className="empty-icon" aria-hidden="true">
                <SectionIcon name="projects" />
              </span>
              <h2>No projects yet</h2>
              <p>Create your first project using the form. It will be stored locally.</p>
            </div>
          ) : null}

          {!isLoading && projects.length > 0 ? (
            <div className="project-list">
              {projects.map((project) => {
                const isSelected = selectedProject?.id === project.id;
                const isOpening = openingProjectId === project.id;
                return (
                  <article
                    className={`project-card${isSelected ? " project-card-selected" : ""}`}
                    key={project.id}
                  >
                    <div className="project-card-heading">
                      <span className="project-card-avatar" aria-hidden="true">
                        {project.name.slice(0, 2).toUpperCase()}
                      </span>
                      <div>
                        <h3>{project.name}</h3>
                        <p>{formatUpdatedAt(project.updated_at)}</p>
                      </div>
                      {isSelected ? <span className="selected-badge">Selected</span> : null}
                    </div>
                    <dl className="project-meta">
                      <div><dt>Topic</dt><dd>{project.topic || "Not set"}</dd></div>
                      <div><dt>Style</dt><dd>{project.style || "Not set"}</dd></div>
                    </dl>
                    <Button
                      variant={isSelected ? "secondary" : "primary"}
                      disabled={isOpening || isSelected}
                      onClick={() => void handleOpen(project.id)}
                    >
                      {isOpening ? "Opening…" : isSelected ? "Project open" : "Open project"}
                    </Button>
                  </article>
                );
              })}
            </div>
          ) : null}
        </section>

        <section className="projects-panel project-form-panel" aria-labelledby="new-project-title">
          <p className="eyebrow">New project</p>
          <h2 id="new-project-title">Start a video</h2>
          <p className="form-intro">Set the foundation now. Script and scene tools arrive in later phases.</p>

          <form className="project-form" onSubmit={handleCreate} noValidate>
            <label htmlFor="project-name">Name <span aria-hidden="true">*</span></label>
            <input
              id="project-name"
              name="name"
              value={fields.name}
              onChange={(event) => handleFieldChange("name", event.currentTarget.value)}
              aria-invalid={Boolean(nameError)}
              aria-describedby={nameError ? "project-name-error" : undefined}
              autoComplete="off"
              disabled={isSubmitting}
              required
            />
            {nameError ? <p className="field-error" id="project-name-error">{nameError}</p> : null}

            <label htmlFor="project-topic">Topic</label>
            <textarea
              id="project-topic"
              name="topic"
              value={fields.topic}
              onChange={(event) => handleFieldChange("topic", event.currentTarget.value)}
              placeholder="What is this video about?"
              rows={3}
              disabled={isSubmitting}
            />

            <label htmlFor="project-style">Style</label>
            <input
              id="project-style"
              name="style"
              value={fields.style}
              onChange={(event) => handleFieldChange("style", event.currentTarget.value)}
              placeholder="Cinematic, documentary, educational…"
              autoComplete="off"
              disabled={isSubmitting}
            />

            {submitError ? <p className="form-error" role="alert">{submitError}</p> : null}

            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? "Creating project…" : "Create project"}
            </Button>
          </form>
        </section>
      </div>
    </div>
  );
}
