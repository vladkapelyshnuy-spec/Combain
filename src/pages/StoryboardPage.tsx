import { useCallback, useEffect, useState } from "react";
import { Button, ErrorState, LoadingState } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import type { Project } from "../models/project";
import type { Scene } from "../models/scene";
import type { AppPath } from "../routing/routes";
import { getScenes } from "../services/sceneService";
import { getIpcError } from "../services/projectService";

type StoryboardPageProps = {
  onNavigate: (path: AppPath) => void;
  selectedProject: Project | null;
};

function formatStatus(status: Scene["status"]) {
  return status.charAt(0) + status.slice(1).toLowerCase();
}

export function StoryboardPage({ onNavigate, selectedProject }: StoryboardPageProps) {
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadScenes = useCallback(async () => {
    if (!selectedProject) return;
    setIsLoading(true);
    setLoadError(null);
    try {
      setScenes(await getScenes(selectedProject.id));
    } catch (error) {
      setLoadError(getIpcError(error).message);
    } finally {
      setIsLoading(false);
    }
  }, [selectedProject]);

  useEffect(() => {
    setScenes([]);
    if (selectedProject) void loadScenes();
  }, [loadScenes, selectedProject]);

  if (!selectedProject) {
    return (
      <div className="workspace-page">
        <header className="workspace-header">
          <div className="workspace-heading">
            <span className="workspace-heading-icon" aria-hidden="true">
              <SectionIcon name="storyboard" />
            </span>
            <div>
              <p className="eyebrow">Phase 5 workspace</p>
              <h1>Storyboard</h1>
              <p className="lead">Review the visual flow of every scene in sequence.</p>
            </div>
          </div>
        </header>
        <div className="storyboard-empty">
          <span className="empty-icon" aria-hidden="true"><SectionIcon name="projects" /></span>
          <h2>No project selected</h2>
          <p>Choose an existing project before opening its storyboard.</p>
          <Button onClick={() => onNavigate("/projects")}>Go to Projects</Button>
        </div>
      </div>
    );
  }

  return (
    <div className="workspace-page storyboard-page">
      <header className="workspace-header">
        <div className="workspace-heading">
          <span className="workspace-heading-icon" aria-hidden="true">
            <SectionIcon name="storyboard" />
          </span>
          <div>
            <p className="eyebrow">{selectedProject.name}</p>
            <h1>Storyboard</h1>
            <p className="lead">Review scene direction and pacing before visual generation.</p>
          </div>
        </div>
        <span className="project-count" aria-live="polite">
          {isLoading ? "Loading…" : `${scenes.length} ${scenes.length === 1 ? "scene" : "scenes"}`}
        </span>
      </header>

      {isLoading ? (
        <LoadingState
          title="Loading storyboard"
          description="Reading the selected project's scenes from local storage."
        />
      ) : null}

      {!isLoading && loadError ? (
        <ErrorState
          title="Could not load storyboard"
          description={loadError}
          onRetry={() => void loadScenes()}
        />
      ) : null}

      {!isLoading && !loadError && scenes.length === 0 ? (
        <div className="storyboard-empty">
          <span className="empty-icon" aria-hidden="true"><SectionIcon name="scenes" /></span>
          <h2>No scenes to storyboard</h2>
          <p>Create scenes or generate a script first. The storyboard will follow their saved order.</p>
          <Button onClick={() => onNavigate("/scenes")}>Open Scene Editor</Button>
        </div>
      ) : null}

      {!isLoading && !loadError && scenes.length > 0 ? (
        <>
          <div className="storyboard-toolbar">
            <p>Scenes are displayed in their saved production order.</p>
            <div>
              <Button variant="secondary" onClick={() => onNavigate("/scenes")}>Edit scenes</Button>
              <Button variant="secondary" onClick={() => void loadScenes()}>Refresh</Button>
            </div>
          </div>
          <ol className="storyboard-grid" aria-label="Project storyboard scenes">
            {scenes.map((scene, index) => (
              <li className="storyboard-card" key={scene.id}>
                <article aria-labelledby={`storyboard-scene-${scene.id}`}>
                  <header className="storyboard-card-header">
                    <div>
                      <p className="eyebrow">Scene {String(index + 1).padStart(2, "0")}</p>
                      <h2 id={`storyboard-scene-${scene.id}`}>{scene.title}</h2>
                    </div>
                    <span className={`scene-status scene-status-${scene.status.toLowerCase()}`}>
                      {formatStatus(scene.status)}
                    </span>
                  </header>

                  <div className="storyboard-visual-placeholder" aria-label="Image not generated">
                    <span className="storyboard-placeholder-mark" aria-hidden="true">
                      <SectionIcon name="storyboard" />
                    </span>
                    <strong>Image placeholder</strong>
                    <span>Image generation becomes available in Phase 6</span>
                  </div>

                  <div className="storyboard-narration">
                    <span>Narration</span>
                    <p>{scene.narration || "No narration has been added."}</p>
                  </div>

                  <dl className="storyboard-prompts">
                    <div>
                      <dt>Visual prompt</dt>
                      <dd>{scene.visual_prompt || "Not set"}</dd>
                    </div>
                    <div>
                      <dt>Video prompt</dt>
                      <dd>{scene.video_prompt || "Not set"}</dd>
                    </div>
                  </dl>

                  <footer className="storyboard-card-footer">
                    <span>Duration</span>
                    <strong>{scene.duration} sec</strong>
                  </footer>
                </article>
              </li>
            ))}
          </ol>
        </>
      ) : null}
    </div>
  );
}
