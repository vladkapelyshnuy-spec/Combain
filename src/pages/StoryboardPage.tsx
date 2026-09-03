import { useCallback, useEffect, useMemo, useState } from "react";
import { Button, ErrorState, LoadingState } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import type { Asset, AssetVersion, AssetVersionId } from "../models/asset";
import type { Project } from "../models/project";
import type { Scene, SceneId } from "../models/scene";
import type { AppPath } from "../routing/routes";
import {
  generateImage,
  getAssetFileUrl,
  getAssets,
  getAssetVersions,
  selectAssetVersion,
} from "../services/assetService";
import { getIpcError } from "../services/projectService";
import { getScenes } from "../services/sceneService";

type StoryboardPageProps = {
  onNavigate: (path: AppPath) => void;
  selectedProject: Project | null;
};

type SceneImageState = {
  asset: Asset | null;
  versions: AssetVersion[];
  previewVersionId: AssetVersionId | null;
  error: string | null;
};

const emptyImageState = (): SceneImageState => ({
  asset: null,
  versions: [],
  previewVersionId: null,
  error: null,
});

function formatStatus(status: Scene["status"]) {
  return status.charAt(0) + status.slice(1).toLowerCase();
}

async function loadSceneImage(sceneId: SceneId): Promise<SceneImageState> {
  try {
    const imageAsset = (await getAssets(sceneId)).find((asset) => asset.asset_type === "IMAGE") ?? null;
    if (!imageAsset) return emptyImageState();
    const versions = await getAssetVersions(imageAsset.id);
    const selected = versions.find((version) => version.selected) ?? versions[versions.length - 1] ?? null;
    return { asset: imageAsset, versions, previewVersionId: selected?.id ?? null, error: null };
  } catch (error) {
    return { ...emptyImageState(), error: getIpcError(error).message };
  }
}

export function StoryboardPage({ onNavigate, selectedProject }: StoryboardPageProps) {
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [imageStates, setImageStates] = useState<Record<SceneId, SceneImageState>>({});
  const [activeScenes, setActiveScenes] = useState<Set<SceneId>>(new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadStoryboard = useCallback(async () => {
    if (!selectedProject) return;
    setIsLoading(true);
    setLoadError(null);
    try {
      const loadedScenes = await getScenes(selectedProject.id);
      setScenes(loadedScenes);
      const entries = await Promise.all(
        loadedScenes.map(async (scene) => [scene.id, await loadSceneImage(scene.id)] as const),
      );
      setImageStates(Object.fromEntries(entries));
    } catch (error) {
      setLoadError(getIpcError(error).message);
    } finally {
      setIsLoading(false);
    }
  }, [selectedProject]);

  useEffect(() => {
    setScenes([]);
    setImageStates({});
    setActiveScenes(new Set());
    if (selectedProject) void loadStoryboard();
  }, [loadStoryboard, selectedProject]);

  const setSceneActive = (sceneId: SceneId, active: boolean) => {
    setActiveScenes((current) => {
      const next = new Set(current);
      if (active) next.add(sceneId);
      else next.delete(sceneId);
      return next;
    });
  };

  const updateImageState = (sceneId: SceneId, update: (current: SceneImageState) => SceneImageState) => {
    setImageStates((current) => ({
      ...current,
      [sceneId]: update(current[sceneId] ?? emptyImageState()),
    }));
  };

  const handleGenerate = async (scene: Scene) => {
    if (activeScenes.has(scene.id) || !scene.visual_prompt.trim()) return;
    setSceneActive(scene.id, true);
    updateImageState(scene.id, (current) => ({ ...current, error: null }));
    try {
      const generated = await generateImage(scene.id, scene.visual_prompt);
      updateImageState(scene.id, (current) => {
        const asset = current.asset ?? {
          id: generated.asset_id,
          scene_id: scene.id,
          asset_type: "IMAGE" as const,
        };
        return {
          asset,
          versions: [...current.versions.filter((version) => version.id !== generated.id), generated]
            .sort((left, right) => left.version - right.version),
          previewVersionId: generated.id,
          error: null,
        };
      });
      const persisted = await loadSceneImage(scene.id);
      updateImageState(scene.id, () => ({
        ...persisted,
        previewVersionId: persisted.versions.some((version) => version.id === generated.id)
          ? generated.id
          : persisted.previewVersionId,
      }));
    } catch (error) {
      updateImageState(scene.id, (current) => ({ ...current, error: getIpcError(error).message }));
    } finally {
      setSceneActive(scene.id, false);
    }
  };

  const handleSelectVersion = async (sceneId: SceneId, version: AssetVersion) => {
    const imageState = imageStates[sceneId];
    if (!imageState?.asset || activeScenes.has(sceneId) || version.selected) return;
    setSceneActive(sceneId, true);
    updateImageState(sceneId, (current) => ({ ...current, error: null }));
    try {
      await selectAssetVersion(imageState.asset.id, version.id);
      updateImageState(sceneId, (current) => ({
        ...current,
        versions: current.versions.map((item) => ({ ...item, selected: item.id === version.id })),
        previewVersionId: version.id,
      }));
    } catch (error) {
      updateImageState(sceneId, (current) => ({ ...current, error: getIpcError(error).message }));
    } finally {
      setSceneActive(sceneId, false);
    }
  };

  if (!selectedProject) {
    return (
      <div className="workspace-page">
        <header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="storyboard" /></span><div><p className="eyebrow">Phase 6 workspace</p><h1>Storyboard</h1><p className="lead">Review and generate visuals for every scene.</p></div></div></header>
        <div className="storyboard-empty"><span className="empty-icon" aria-hidden="true"><SectionIcon name="projects" /></span><h2>No project selected</h2><p>Choose an existing project before opening its storyboard.</p><Button onClick={() => onNavigate("/projects")}>Go to Projects</Button></div>
      </div>
    );
  }

  return (
    <div className="workspace-page storyboard-page">
      <header className="workspace-header">
        <div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="storyboard" /></span><div><p className="eyebrow">{selectedProject.name}</p><h1>Storyboard</h1><p className="lead">Generate scene images while preserving every saved version.</p></div></div>
        <span className="project-count" aria-live="polite">{isLoading ? "Loading…" : `${scenes.length} ${scenes.length === 1 ? "scene" : "scenes"}`}</span>
      </header>

      {isLoading ? <LoadingState title="Loading storyboard" description="Reading scenes and existing image versions from local storage." /> : null}
      {!isLoading && loadError ? <ErrorState title="Could not load storyboard" description={loadError} onRetry={() => void loadStoryboard()} /> : null}
      {!isLoading && !loadError && scenes.length === 0 ? <div className="storyboard-empty"><span className="empty-icon" aria-hidden="true"><SectionIcon name="scenes" /></span><h2>No scenes to storyboard</h2><p>Create scenes or generate a script first. The storyboard will follow their saved order.</p><Button onClick={() => onNavigate("/scenes")}>Open Scene Editor</Button></div> : null}

      {!isLoading && !loadError && scenes.length > 0 ? <>
        <div className="storyboard-toolbar"><p>Scenes and stored image versions are displayed in production order.</p><div><Button variant="secondary" onClick={() => onNavigate("/scenes")}>Edit scenes</Button><Button variant="secondary" onClick={() => void loadStoryboard()} disabled={activeScenes.size > 0}>Refresh</Button></div></div>
        <ol className="storyboard-grid" aria-label="Project storyboard scenes">
          {scenes.map((scene, index) => <StoryboardCard key={scene.id} scene={scene} index={index} imageState={imageStates[scene.id] ?? emptyImageState()} isActive={activeScenes.has(scene.id)} onGenerate={handleGenerate} onSelectVersion={handleSelectVersion} />)}
        </ol>
      </> : null}
    </div>
  );
}

type StoryboardCardProps = {
  scene: Scene;
  index: number;
  imageState: SceneImageState;
  isActive: boolean;
  onGenerate: (scene: Scene) => Promise<void>;
  onSelectVersion: (sceneId: SceneId, version: AssetVersion) => Promise<void>;
};

function StoryboardCard({ scene, index, imageState, isActive, onGenerate, onSelectVersion }: StoryboardCardProps) {
  const previewVersion = useMemo(
    () => imageState.versions.find((version) => version.id === imageState.previewVersionId)
      ?? imageState.versions.find((version) => version.selected)
      ?? imageState.versions[imageState.versions.length - 1]
      ?? null,
    [imageState],
  );
  const hasPrompt = Boolean(scene.visual_prompt.trim());

  return (
    <li className="storyboard-card">
      <article aria-labelledby={`storyboard-scene-${scene.id}`}>
        <header className="storyboard-card-header"><div><p className="eyebrow">Scene {String(index + 1).padStart(2, "0")}</p><h2 id={`storyboard-scene-${scene.id}`}>{scene.title}</h2></div><span className={`scene-status scene-status-${scene.status.toLowerCase()}`}>{formatStatus(scene.status)}</span></header>

        <div className={previewVersion ? "storyboard-image-frame" : "storyboard-visual-placeholder"} aria-busy={isActive}>
          {previewVersion ? <img src={getAssetFileUrl(previewVersion.file_path)} alt={`Generated visual for ${scene.title}, version ${previewVersion.version}`} /> : <><span className="storyboard-placeholder-mark" aria-hidden="true"><SectionIcon name="storyboard" /></span><strong>No generated image</strong><span>{hasPrompt ? "Generate the first version from this scene's visual prompt" : "Add a Visual Prompt in the Scene Editor first"}</span></>}
          {isActive ? <div className="storyboard-generation-overlay" role="status"><span className="spinner" aria-hidden="true" /><strong>Processing image…</strong><span>Waiting for the configured backend provider</span></div> : null}
        </div>

        <div className="storyboard-image-controls">
          <div><span className="storyboard-control-label">Image</span><strong>{previewVersion ? `Version ${previewVersion.version}` : "Not generated"}</strong></div>
          <Button onClick={() => void onGenerate(scene)} disabled={isActive || !hasPrompt}>{isActive ? "Generating…" : imageState.versions.length ? "Generate new version" : "Generate image"}</Button>
        </div>
        {!hasPrompt ? <p className="storyboard-image-warning">A Visual Prompt is required before image generation.</p> : null}
        {imageState.error ? <p className="storyboard-image-error" role="alert">{imageState.error}</p> : null}

        {imageState.versions.length > 0 ? <div className="asset-version-list" aria-label={`Image versions for ${scene.title}`}>
          {imageState.versions.map((version) => <button key={version.id} type="button" className={version.selected ? "asset-version-button selected" : "asset-version-button"} onClick={() => void onSelectVersion(scene.id, version)} disabled={isActive || version.selected} aria-label={`${version.selected ? "Selected" : "Select"} image version ${version.version}`}><span>v{version.version}</span>{version.selected ? <small>Selected</small> : <small>{version.id === imageState.previewVersionId ? "Preview" : "Select"}</small>}</button>)}
        </div> : null}

        <div className="storyboard-narration"><span>Narration</span><p>{scene.narration || "No narration has been added."}</p></div>
        <dl className="storyboard-prompts"><div><dt>Visual prompt</dt><dd>{scene.visual_prompt || "Not set"}</dd></div><div><dt>Video prompt</dt><dd>{scene.video_prompt || "Not set"}</dd></div></dl>
        <footer className="storyboard-card-footer"><span>Duration</span><strong>{scene.duration} sec</strong></footer>
      </article>
    </li>
  );
}
