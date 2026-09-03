import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Button, ErrorState, LoadingState } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import type { Project } from "../models/project";
import type { Scene, SceneFields, SceneId } from "../models/scene";
import type { AppPath } from "../routing/routes";
import {
  createScene,
  deleteScene,
  generateScript,
  getScenes,
  reorderScenes,
  updateScene,
} from "../services/sceneService";
import { getIpcError } from "../services/projectService";

type ScriptScenePageProps = {
  currentPath: "/script" | "/scenes";
  onNavigate: (path: AppPath) => void;
  selectedProject: Project | null;
};

type GenerationFields = {
  topic: string;
  style: string;
  durationSeconds: string;
  language: string;
};

const toSceneFields = (scene: Scene): SceneFields => ({
  title: scene.title,
  narration: scene.narration,
  visual_prompt: scene.visual_prompt,
  video_prompt: scene.video_prompt,
  duration: scene.duration,
  status: scene.status,
});

export function ScriptScenePage({ currentPath, onNavigate, selectedProject }: ScriptScenePageProps) {
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [selectedSceneId, setSelectedSceneId] = useState<SceneId | null>(null);
  const [draft, setDraft] = useState<SceneFields | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [activeAction, setActiveAction] = useState<string | null>(null);
  const [scriptTitle, setScriptTitle] = useState<string | null>(null);
  const [newSceneTitle, setNewSceneTitle] = useState("");
  const [generation, setGeneration] = useState<GenerationFields>({
    topic: selectedProject?.topic ?? "",
    style: selectedProject?.style ?? "",
    durationSeconds: "30",
    language: "en",
  });
  const actionGuard = useRef(false);

  const selectedScene = useMemo(
    () => scenes.find((scene) => scene.id === selectedSceneId) ?? null,
    [scenes, selectedSceneId],
  );

  const selectScene = useCallback((scene: Scene) => {
    setSelectedSceneId(scene.id);
    setDraft(toSceneFields(scene));
    setActionError(null);
  }, []);

  const loadScenes = useCallback(async () => {
    if (!selectedProject) return;
    setIsLoading(true);
    setLoadError(null);
    try {
      const loaded = await getScenes(selectedProject.id);
      setScenes(loaded);
      setSelectedSceneId((currentId) => {
        const next = loaded.find((scene) => scene.id === currentId) ?? loaded[0] ?? null;
        setDraft(next ? toSceneFields(next) : null);
        return next?.id ?? null;
      });
    } catch (error) {
      setLoadError(getIpcError(error).message);
    } finally {
      setIsLoading(false);
    }
  }, [selectedProject]);

  useEffect(() => {
    setScenes([]);
    setSelectedSceneId(null);
    setDraft(null);
    setScriptTitle(null);
    setGeneration({
      topic: selectedProject?.topic ?? "",
      style: selectedProject?.style ?? "",
      durationSeconds: "30",
      language: "en",
    });
    if (selectedProject) void loadScenes();
  }, [loadScenes, selectedProject]);

  const runAction = async (name: string, action: () => Promise<void>) => {
    if (actionGuard.current) return;
    actionGuard.current = true;
    setActiveAction(name);
    setActionError(null);
    try {
      await action();
    } catch (error) {
      setActionError(getIpcError(error).message);
    } finally {
      actionGuard.current = false;
      setActiveAction(null);
    }
  };

  const handleGenerate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedProject || scenes.length > 0) return;
    const topic = generation.topic.trim();
    const language = generation.language.trim();
    const durationSeconds = Number(generation.durationSeconds);
    if (!topic || !language || !Number.isInteger(durationSeconds) || durationSeconds <= 0) {
      setActionError("Topic, language, and a positive whole-number duration are required.");
      return;
    }
    void runAction("generate", async () => {
      const result = await generateScript({
        projectId: selectedProject.id,
        topic,
        style: generation.style.trim(),
        durationSeconds,
        language,
      });
      setScriptTitle(result.title);
      setScenes(result.scenes);
      if (result.scenes[0]) selectScene(result.scenes[0]);
    });
  };

  const handleCreate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedProject || !newSceneTitle.trim()) {
      setActionError("Scene title is required.");
      return;
    }
    void runAction("create", async () => {
      const created = await createScene(selectedProject.id, newSceneTitle.trim());
      setScenes((current) => [...current, created]);
      setNewSceneTitle("");
      selectScene(created);
    });
  };

  const handleSave = () => {
    if (!selectedScene || !draft) return;
    const duration = Number(draft.duration);
    if (!draft.title.trim() || !Number.isInteger(duration) || duration < 0) {
      setActionError("Scene title and a non-negative whole-number duration are required.");
      return;
    }
    void runAction("save", async () => {
      const updated = await updateScene(selectedScene.id, {
        ...draft,
        title: draft.title.trim(),
        duration,
      });
      setScenes((current) => current.map((scene) => (scene.id === updated.id ? updated : scene)));
      setDraft(toSceneFields(updated));
    });
  };

  const handleDelete = () => {
    if (!selectedScene || !window.confirm(`Delete “${selectedScene.title}”? This cannot be undone.`)) return;
    void runAction("delete", async () => {
      await deleteScene(selectedScene.id);
      const remaining = scenes.filter((scene) => scene.id !== selectedScene.id);
      setScenes(remaining);
      const next = remaining[Math.min(selectedScene.order, remaining.length - 1)] ?? null;
      if (next) selectScene(next);
      else {
        setSelectedSceneId(null);
        setDraft(null);
      }
    });
  };

  const handleMove = (direction: -1 | 1) => {
    if (!selectedProject || !selectedScene) return;
    const index = scenes.findIndex((scene) => scene.id === selectedScene.id);
    const target = index + direction;
    if (target < 0 || target >= scenes.length) return;
    const reordered = [...scenes];
    [reordered[index], reordered[target]] = [reordered[target], reordered[index]];
    void runAction("reorder", async () => {
      await reorderScenes(selectedProject.id, reordered.map((scene) => scene.id));
      setScenes(reordered.map((scene, order) => ({ ...scene, order })));
    });
  };

  if (!selectedProject) {
    return (
      <div className="workspace-page">
        <header className="workspace-header">
          <div className="workspace-heading">
            <span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name={currentPath === "/script" ? "script" : "scenes"} /></span>
            <div><p className="eyebrow">Phase 4 workspace</p><h1>{currentPath === "/script" ? "Script" : "Scenes"}</h1><p className="lead">Select a project before editing its script and scenes.</p></div>
          </div>
        </header>
        <div className="scene-empty"><h2>No project selected</h2><p>Open an existing project or create a new one to continue.</p><Button onClick={() => onNavigate("/projects")}>Choose project</Button></div>
      </div>
    );
  }

  return (
    <div className="workspace-page script-scene-page">
      <header className="workspace-header">
        <div className="workspace-heading">
          <span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name={currentPath === "/script" ? "script" : "scenes"} /></span>
          <div><p className="eyebrow">{selectedProject.name}</p><h1>{currentPath === "/script" ? "Script Editor" : "Scene Editor"}</h1><p className="lead">Generate a structured script, then refine each scene for production.</p></div>
        </div>
        <span className="project-count" aria-live="polite">{scenes.length} {scenes.length === 1 ? "scene" : "scenes"}</span>
      </header>

      <section className="script-generator" aria-labelledby="script-generator-title">
        <div className="script-generator-heading"><div><p className="eyebrow">AI Director</p><h2 id="script-generator-title">Generate structured script</h2></div>{scriptTitle ? <span className="selected-badge">{scriptTitle}</span> : null}</div>
        <form className="generation-form" onSubmit={handleGenerate}>
          <label>Topic<input value={generation.topic} onChange={(event) => { const inputValue = event.currentTarget.value; setGeneration((value) => ({ ...value, topic: inputValue })); }} disabled={Boolean(activeAction) || scenes.length > 0} required /></label>
          <label>Style<input value={generation.style} onChange={(event) => { const inputValue = event.currentTarget.value; setGeneration((value) => ({ ...value, style: inputValue })); }} disabled={Boolean(activeAction) || scenes.length > 0} /></label>
          <label>Duration (seconds)<input type="number" min="1" step="1" value={generation.durationSeconds} onChange={(event) => { const inputValue = event.currentTarget.value; setGeneration((value) => ({ ...value, durationSeconds: inputValue })); }} disabled={Boolean(activeAction) || scenes.length > 0} required /></label>
          <label>Language<input value={generation.language} onChange={(event) => { const inputValue = event.currentTarget.value; setGeneration((value) => ({ ...value, language: inputValue })); }} disabled={Boolean(activeAction) || scenes.length > 0} required /></label>
          <Button type="submit" disabled={Boolean(activeAction) || scenes.length > 0}>{activeAction === "generate" ? "Generating script…" : "Generate script"}</Button>
        </form>
        {scenes.length > 0 ? <p className="form-note">Script generation is available only before scenes are created. Existing scenes are never overwritten.</p> : null}
      </section>

      {actionError ? <div className="form-error page-action-error" role="alert">{actionError}</div> : null}
      {isLoading ? <LoadingState title="Loading scenes" description="Reading the selected project's script from local storage." /> : null}
      {!isLoading && loadError ? <ErrorState title="Could not load scenes" description={loadError} onRetry={() => void loadScenes()} /> : null}

      {!isLoading && !loadError ? (
        <div className="scene-editor-layout">
          <aside className="scene-list-panel" aria-labelledby="scene-list-title">
            <div className="scene-panel-heading"><div><p className="eyebrow">Sequence</p><h2 id="scene-list-title">Scenes</h2></div><Button variant="secondary" onClick={() => void loadScenes()} disabled={Boolean(activeAction)}>Refresh</Button></div>
            <form className="new-scene-form" onSubmit={handleCreate}>
              <label htmlFor="new-scene-title">New scene title</label>
              <div><input id="new-scene-title" value={newSceneTitle} onChange={(event) => setNewSceneTitle(event.currentTarget.value)} disabled={Boolean(activeAction)} placeholder="Scene title" /><Button type="submit" disabled={Boolean(activeAction) || !newSceneTitle.trim()}>{activeAction === "create" ? "Adding…" : "Add"}</Button></div>
            </form>
            {scenes.length === 0 ? <div className="scene-list-empty"><p>No scenes yet. Generate a script or add the first scene manually.</p></div> : (
              <ol className="scene-list">
                {scenes.map((scene, index) => <li key={scene.id}><button className={scene.id === selectedSceneId ? "scene-list-item selected" : "scene-list-item"} onClick={() => selectScene(scene)} aria-current={scene.id === selectedSceneId ? "true" : undefined}><span>{String(index + 1).padStart(2, "0")}</span><span><strong>{scene.title}</strong><small>{scene.duration}s · {scene.status.toLowerCase()}</small></span></button></li>)}
              </ol>
            )}
          </aside>

          <section className="scene-form-panel" aria-labelledby="scene-editor-title">
            {selectedScene && draft ? <>
              <div className="scene-panel-heading"><div><p className="eyebrow">Scene {selectedScene.order + 1}</p><h2 id="scene-editor-title">Edit scene</h2></div><div className="scene-order-actions"><Button variant="secondary" onClick={() => handleMove(-1)} disabled={Boolean(activeAction) || selectedScene.order === 0} aria-label="Move scene up">↑</Button><Button variant="secondary" onClick={() => handleMove(1)} disabled={Boolean(activeAction) || selectedScene.order === scenes.length - 1} aria-label="Move scene down">↓</Button></div></div>
              <form className="scene-form" onSubmit={(event) => { event.preventDefault(); handleSave(); }}>
                <label>Title<input value={draft.title} onChange={(event) => setDraft({ ...draft, title: event.currentTarget.value })} disabled={Boolean(activeAction)} required /></label>
                <label>Narration<textarea rows={5} value={draft.narration} onChange={(event) => setDraft({ ...draft, narration: event.currentTarget.value })} disabled={Boolean(activeAction)} /></label>
                <label>Visual Prompt<textarea rows={3} value={draft.visual_prompt} onChange={(event) => setDraft({ ...draft, visual_prompt: event.currentTarget.value })} disabled={Boolean(activeAction)} /></label>
                <label>Video Prompt<textarea rows={3} value={draft.video_prompt} onChange={(event) => setDraft({ ...draft, video_prompt: event.currentTarget.value })} disabled={Boolean(activeAction)} /></label>
                <label>Duration (seconds)<input type="number" min="0" step="1" value={draft.duration} onChange={(event) => setDraft({ ...draft, duration: Number(event.currentTarget.value) })} disabled={Boolean(activeAction)} /></label>
                <div className="scene-form-actions"><Button type="submit" disabled={Boolean(activeAction) || !draft.title.trim()}>{activeAction === "save" ? "Saving…" : "Save scene"}</Button><Button variant="secondary" onClick={handleDelete} disabled={Boolean(activeAction)}>{activeAction === "delete" ? "Deleting…" : "Delete scene"}</Button></div>
              </form>
            </> : <div className="scene-editor-empty"><h2 id="scene-editor-title">Select a scene</h2><p>Choose a scene from the sequence or add a new one to start editing.</p></div>}
          </section>
        </div>
      ) : null}
    </div>
  );
}
