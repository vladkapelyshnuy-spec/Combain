import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, ErrorState, LoadingState } from "../components/ui";
import { SectionIcon } from "../components/SectionIcon";
import type { Asset, AssetType, AssetVersion, AssetVersionId } from "../models/asset";
import type { Project } from "../models/project";
import type { Scene, SceneId } from "../models/scene";
import type { AppPath } from "../routing/routes";
import {
  generateImage,
  generateTts,
  generateVideo,
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

type SceneMediaState = {
  asset: Asset | null;
  versions: AssetVersion[];
  previewVersionId: AssetVersionId | null;
  error: string | null;
};

type VideoGenerationMode = "image" | "text";
type StoryboardMediaType = Extract<AssetType, "IMAGE" | "VIDEO" | "AUDIO">;

const emptyMediaState = (): SceneMediaState => ({
  asset: null,
  versions: [],
  previewVersionId: null,
  error: null,
});

function formatStatus(status: Scene["status"]) {
  return status.charAt(0) + status.slice(1).toLowerCase();
}

async function loadSceneMedia(
  sceneId: SceneId,
  assetType: StoryboardMediaType,
): Promise<SceneMediaState> {
  try {
    const asset = (await getAssets(sceneId)).find((item) => item.asset_type === assetType) ?? null;
    if (!asset) return emptyMediaState();
    const versions = await getAssetVersions(asset.id);
    const selected = versions.find((version) => version.selected) ?? versions[versions.length - 1] ?? null;
    return { asset, versions, previewVersionId: selected?.id ?? null, error: null };
  } catch (error) {
    return { ...emptyMediaState(), error: getIpcError(error).message };
  }
}

export function StoryboardPage({ onNavigate, selectedProject }: StoryboardPageProps) {
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [imageStates, setImageStates] = useState<Record<SceneId, SceneMediaState>>({});
  const [videoStates, setVideoStates] = useState<Record<SceneId, SceneMediaState>>({});
  const [audioStates, setAudioStates] = useState<Record<SceneId, SceneMediaState>>({});
  const [activeImageScenes, setActiveImageScenes] = useState<Set<SceneId>>(new Set());
  const [activeVideoScenes, setActiveVideoScenes] = useState<Set<SceneId>>(new Set());
  const [activeAudioScenes, setActiveAudioScenes] = useState<Set<SceneId>>(new Set());
  const imageRequests = useRef<Set<SceneId>>(new Set());
  const videoRequests = useRef<Set<SceneId>>(new Set());
  const audioRequests = useRef<Set<SceneId>>(new Set());
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
        loadedScenes.map(async (scene) => {
          const [image, video, audio] = await Promise.all([
            loadSceneMedia(scene.id, "IMAGE"),
            loadSceneMedia(scene.id, "VIDEO"),
            loadSceneMedia(scene.id, "AUDIO"),
          ]);
          return [scene.id, image, video, audio] as const;
        }),
      );
      setImageStates(Object.fromEntries(entries.map(([sceneId, image]) => [sceneId, image])));
      setVideoStates(Object.fromEntries(entries.map(([sceneId, , video]) => [sceneId, video])));
      setAudioStates(Object.fromEntries(entries.map(([sceneId, , , audio]) => [sceneId, audio])));
    } catch (error) {
      setLoadError(getIpcError(error).message);
    } finally {
      setIsLoading(false);
    }
  }, [selectedProject]);

  useEffect(() => {
    setScenes([]);
    setImageStates({});
    setVideoStates({});
    setAudioStates({});
    setActiveImageScenes(new Set());
    setActiveVideoScenes(new Set());
    setActiveAudioScenes(new Set());
    if (selectedProject) void loadStoryboard();
  }, [loadStoryboard, selectedProject]);

  const setSceneActive = (
    setter: React.Dispatch<React.SetStateAction<Set<SceneId>>>,
    sceneId: SceneId,
    active: boolean,
  ) => {
    setter((current) => {
      const next = new Set(current);
      if (active) next.add(sceneId);
      else next.delete(sceneId);
      return next;
    });
  };

  const updateMediaState = (
    setter: React.Dispatch<React.SetStateAction<Record<SceneId, SceneMediaState>>>,
    sceneId: SceneId,
    update: (current: SceneMediaState) => SceneMediaState,
  ) => {
    setter((current) => ({
      ...current,
      [sceneId]: update(current[sceneId] ?? emptyMediaState()),
    }));
  };

  const handleGenerateImage = async (scene: Scene) => {
    if (imageRequests.current.has(scene.id) || !scene.visual_prompt.trim()) return;
    imageRequests.current.add(scene.id);
    setSceneActive(setActiveImageScenes, scene.id, true);
    updateMediaState(setImageStates, scene.id, (current) => ({ ...current, error: null }));
    try {
      const generated = await generateImage(scene.id, scene.visual_prompt);
      updateMediaState(setImageStates, scene.id, (current) => ({
        asset: current.asset ?? {
          id: generated.asset_id,
          scene_id: scene.id,
          asset_type: "IMAGE",
        },
        versions: [...current.versions.filter((version) => version.id !== generated.id), generated]
          .sort((left, right) => left.version - right.version),
        previewVersionId: generated.id,
        error: null,
      }));
      const persisted = await loadSceneMedia(scene.id, "IMAGE");
      updateMediaState(setImageStates, scene.id, () => ({
        ...persisted,
        previewVersionId: persisted.versions.some((version) => version.id === generated.id)
          ? generated.id
          : persisted.previewVersionId,
      }));
    } catch (error) {
      updateMediaState(setImageStates, scene.id, (current) => ({
        ...current,
        error: getIpcError(error).message,
      }));
    } finally {
      imageRequests.current.delete(scene.id);
      setSceneActive(setActiveImageScenes, scene.id, false);
    }
  };

  const handleGenerateVideo = async (scene: Scene, mode: VideoGenerationMode) => {
    if (videoRequests.current.has(scene.id) || !scene.video_prompt.trim()) return;
    videoRequests.current.add(scene.id);
    setSceneActive(setActiveVideoScenes, scene.id, true);
    updateMediaState(setVideoStates, scene.id, (current) => ({ ...current, error: null }));
    try {
      const generated = await generateVideo(scene.id, scene.video_prompt, mode === "image");
      updateMediaState(setVideoStates, scene.id, (current) => ({
        asset: current.asset ?? {
          id: generated.asset_id,
          scene_id: scene.id,
          asset_type: "VIDEO",
        },
        versions: [...current.versions.filter((version) => version.id !== generated.id), generated]
          .sort((left, right) => left.version - right.version),
        previewVersionId: generated.id,
        error: null,
      }));
      const persisted = await loadSceneMedia(scene.id, "VIDEO");
      updateMediaState(setVideoStates, scene.id, () => ({
        ...persisted,
        previewVersionId: persisted.versions.some((version) => version.id === generated.id)
          ? generated.id
          : persisted.previewVersionId,
      }));
    } catch (error) {
      updateMediaState(setVideoStates, scene.id, (current) => ({
        ...current,
        error: getIpcError(error).message,
      }));
    } finally {
      videoRequests.current.delete(scene.id);
      setSceneActive(setActiveVideoScenes, scene.id, false);
    }
  };

  const handleGenerateAudio = async (scene: Scene) => {
    if (audioRequests.current.has(scene.id) || !scene.narration.trim()) return;
    audioRequests.current.add(scene.id);
    setSceneActive(setActiveAudioScenes, scene.id, true);
    updateMediaState(setAudioStates, scene.id, (current) => ({ ...current, error: null }));
    try {
      const generated = await generateTts(scene.id);
      updateMediaState(setAudioStates, scene.id, (current) => ({
        asset: current.asset ?? {
          id: generated.asset_id,
          scene_id: scene.id,
          asset_type: "AUDIO",
        },
        versions: [...current.versions.filter((version) => version.id !== generated.id), generated]
          .sort((left, right) => left.version - right.version),
        previewVersionId: generated.id,
        error: null,
      }));
      const persisted = await loadSceneMedia(scene.id, "AUDIO");
      updateMediaState(setAudioStates, scene.id, () => ({
        ...persisted,
        previewVersionId: persisted.versions.some((version) => version.id === generated.id)
          ? generated.id
          : persisted.previewVersionId,
      }));
    } catch (error) {
      updateMediaState(setAudioStates, scene.id, (current) => ({
        ...current,
        error: getIpcError(error).message,
      }));
    } finally {
      audioRequests.current.delete(scene.id);
      setSceneActive(setActiveAudioScenes, scene.id, false);
    }
  };

  const handleSelectVersion = async (
    sceneId: SceneId,
    version: AssetVersion,
    mediaType: StoryboardMediaType,
  ) => {
    const isImage = mediaType === "IMAGE";
    const isVideo = mediaType === "VIDEO";
    const states = isImage ? imageStates : isVideo ? videoStates : audioStates;
    const activeScenes = isImage ? activeImageScenes : isVideo ? activeVideoScenes : activeAudioScenes;
    const stateSetter = isImage ? setImageStates : isVideo ? setVideoStates : setAudioStates;
    const activeSetter = isImage ? setActiveImageScenes : isVideo ? setActiveVideoScenes : setActiveAudioScenes;
    const mediaState = states[sceneId];
    if (!mediaState?.asset || activeScenes.has(sceneId) || version.selected) return;

    setSceneActive(activeSetter, sceneId, true);
    updateMediaState(stateSetter, sceneId, (current) => ({ ...current, error: null }));
    try {
      await selectAssetVersion(mediaState.asset.id, version.id);
      const persisted = await loadSceneMedia(sceneId, mediaType);
      updateMediaState(stateSetter, sceneId, () => persisted);
    } catch (error) {
      updateMediaState(stateSetter, sceneId, (current) => ({
        ...current,
        error: getIpcError(error).message,
      }));
    } finally {
      setSceneActive(activeSetter, sceneId, false);
    }
  };

  if (!selectedProject) {
    return (
      <div className="workspace-page">
        <header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="storyboard" /></span><div><p className="eyebrow">Phase 7 workspace</p><h1>Storyboard</h1><p className="lead">Review and generate visuals and video for every scene.</p></div></div></header>
        <div className="storyboard-empty"><span className="empty-icon" aria-hidden="true"><SectionIcon name="projects" /></span><h2>No project selected</h2><p>Choose an existing project before opening its storyboard.</p><Button onClick={() => onNavigate("/projects")}>Go to Projects</Button></div>
      </div>
    );
  }

  const hasActiveGeneration = activeImageScenes.size > 0 || activeVideoScenes.size > 0 || activeAudioScenes.size > 0;

  return (
    <div className="workspace-page storyboard-page">
      <header className="workspace-header">
        <div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="storyboard" /></span><div><p className="eyebrow">{selectedProject.name}</p><h1>Storyboard</h1><p className="lead">Generate scene images and videos while preserving every saved version.</p></div></div>
        <span className="project-count" aria-live="polite">{isLoading ? "Loading…" : `${scenes.length} ${scenes.length === 1 ? "scene" : "scenes"}`}</span>
      </header>

      {isLoading ? <LoadingState title="Loading storyboard" description="Reading scenes and existing image and video versions from local storage." /> : null}
      {!isLoading && loadError ? <ErrorState title="Could not load storyboard" description={loadError} onRetry={() => void loadStoryboard()} /> : null}
      {!isLoading && !loadError && scenes.length === 0 ? <div className="storyboard-empty"><span className="empty-icon" aria-hidden="true"><SectionIcon name="scenes" /></span><h2>No scenes to storyboard</h2><p>Create scenes or generate a script first. The storyboard will follow their saved order.</p><Button onClick={() => onNavigate("/scenes")}>Open Scene Editor</Button></div> : null}

      {!isLoading && !loadError && scenes.length > 0 ? <>
        <div className="storyboard-toolbar"><p>Scenes and stored media versions are displayed in production order.</p><div><Button variant="secondary" onClick={() => onNavigate("/scenes")}>Edit scenes</Button><Button variant="secondary" onClick={() => void loadStoryboard()} disabled={hasActiveGeneration}>Refresh</Button></div></div>
        <ol className="storyboard-grid" aria-label="Project storyboard scenes">
          {scenes.map((scene, index) => <StoryboardCard key={scene.id} scene={scene} index={index} imageState={imageStates[scene.id] ?? emptyMediaState()} videoState={videoStates[scene.id] ?? emptyMediaState()} audioState={audioStates[scene.id] ?? emptyMediaState()} isImageActive={activeImageScenes.has(scene.id)} isVideoActive={activeVideoScenes.has(scene.id)} isAudioActive={activeAudioScenes.has(scene.id)} onGenerateImage={handleGenerateImage} onGenerateVideo={handleGenerateVideo} onGenerateAudio={handleGenerateAudio} onSelectVersion={handleSelectVersion} />)}
        </ol>
      </> : null}
    </div>
  );
}

type StoryboardCardProps = {
  scene: Scene;
  index: number;
  imageState: SceneMediaState;
  videoState: SceneMediaState;
  audioState: SceneMediaState;
  isImageActive: boolean;
  isVideoActive: boolean;
  isAudioActive: boolean;
  onGenerateImage: (scene: Scene) => Promise<void>;
  onGenerateVideo: (scene: Scene, mode: VideoGenerationMode) => Promise<void>;
  onGenerateAudio: (scene: Scene) => Promise<void>;
  onSelectVersion: (
    sceneId: SceneId,
    version: AssetVersion,
    mediaType: StoryboardMediaType,
  ) => Promise<void>;
};

function findPreviewVersion(state: SceneMediaState) {
  return state.versions.find((version) => version.id === state.previewVersionId)
    ?? state.versions.find((version) => version.selected)
    ?? state.versions[state.versions.length - 1]
    ?? null;
}

function StoryboardCard({
  scene,
  index,
  imageState,
  videoState,
  audioState,
  isImageActive,
  isVideoActive,
  isAudioActive,
  onGenerateImage,
  onGenerateVideo,
  onGenerateAudio,
  onSelectVersion,
}: StoryboardCardProps) {
  const imagePreview = useMemo(() => findPreviewVersion(imageState), [imageState]);
  const videoPreview = useMemo(() => findPreviewVersion(videoState), [videoState]);
  const audioPreview = useMemo(() => findPreviewVersion(audioState), [audioState]);
  const [videoPreviewFailed, setVideoPreviewFailed] = useState(false);
  const [audioPreviewFailed, setAudioPreviewFailed] = useState(false);
  const hasImagePrompt = Boolean(scene.visual_prompt.trim());
  const hasVideoPrompt = Boolean(scene.video_prompt.trim());
  const hasNarration = Boolean(scene.narration.trim());
  const hasSelectedImage = imageState.versions.some((version) => version.selected);
  const [videoGenerationMode, setVideoGenerationMode] = useState<VideoGenerationMode>(
    hasSelectedImage ? "image" : "text",
  );
  const videoStatus = isVideoActive
    ? "RUNNING"
    : videoState.error
      ? "FAILED"
      : videoState.versions.length
        ? "COMPLETED"
        : null;

  useEffect(() => {
    setVideoPreviewFailed(false);
  }, [videoPreview?.id]);

  useEffect(() => {
    setAudioPreviewFailed(false);
  }, [audioPreview?.id]);

  useEffect(() => {
    if (!hasSelectedImage && videoGenerationMode === "image") {
      setVideoGenerationMode("text");
    }
  }, [hasSelectedImage, videoGenerationMode]);

  return (
    <li className="storyboard-card">
      <article aria-labelledby={`storyboard-scene-${scene.id}`}>
        <header className="storyboard-card-header"><div><p className="eyebrow">Scene {String(index + 1).padStart(2, "0")}</p><h2 id={`storyboard-scene-${scene.id}`}>{scene.title}</h2></div><span className={`scene-status scene-status-${scene.status.toLowerCase()}`}>{formatStatus(scene.status)}</span></header>

        <section className="storyboard-media-section" aria-labelledby={`image-heading-${scene.id}`}>
          <h3 id={`image-heading-${scene.id}`} className="storyboard-media-heading">Image</h3>
          <div className={imagePreview ? "storyboard-image-frame" : "storyboard-visual-placeholder"} aria-busy={isImageActive}>
            {imagePreview ? <img src={getAssetFileUrl(imagePreview.file_path)} alt={`Generated visual for ${scene.title}, version ${imagePreview.version}`} /> : <><span className="storyboard-placeholder-mark" aria-hidden="true"><SectionIcon name="storyboard" /></span><strong>No generated image</strong><span>{hasImagePrompt ? "Generate the first version from this scene's visual prompt" : "Add a Visual Prompt in the Scene Editor first"}</span></>}
            {isImageActive ? <div className="storyboard-generation-overlay" role="status"><span className="spinner" aria-hidden="true" /><strong>Processing image…</strong><span>Waiting for the configured backend provider</span></div> : null}
          </div>
          <div className="storyboard-image-controls">
            <div><span className="storyboard-control-label">Image</span><strong>{imagePreview ? `Version ${imagePreview.version}` : "Not generated"}</strong></div>
            <Button onClick={() => void onGenerateImage(scene)} disabled={isImageActive || !hasImagePrompt}>{isImageActive ? "Generating…" : imageState.versions.length ? "Generate new version" : "Generate image"}</Button>
          </div>
          {!hasImagePrompt ? <p className="storyboard-image-warning">A Visual Prompt is required before image generation.</p> : null}
          {imageState.error ? <p className="storyboard-image-error" role="alert">{imageState.error}</p> : null}
          <VersionList scene={scene} mediaType="IMAGE" state={imageState} isActive={isImageActive} onSelectVersion={onSelectVersion} />
        </section>

        <section className="storyboard-media-section storyboard-video-section" aria-labelledby={`video-heading-${scene.id}`}>
          <div className="storyboard-media-heading-row"><h3 id={`video-heading-${scene.id}`} className="storyboard-media-heading">Video</h3>{videoStatus ? <span className={`video-status-badge video-status-${videoStatus.toLowerCase()}`} aria-live="polite">{videoStatus === "RUNNING" ? "Generating video…" : videoStatus.charAt(0) + videoStatus.slice(1).toLowerCase()}</span> : null}</div>
          <div className={videoPreview && !videoPreviewFailed ? "storyboard-video-frame" : "storyboard-video-placeholder"} aria-busy={isVideoActive}>
            {videoPreview && !videoPreviewFailed ? <video key={videoPreview.id} src={getAssetFileUrl(videoPreview.file_path)} controls preload="metadata" onError={() => setVideoPreviewFailed(true)}>Generated video for {scene.title}</video> : <><span className="storyboard-placeholder-mark" aria-hidden="true"><SectionIcon name="storyboard" /></span><strong>{videoPreviewFailed ? "Video preview unavailable" : "No generated video"}</strong><span>{videoPreviewFailed ? "The stored local video could not be loaded." : hasVideoPrompt ? "Generate the first video from this scene's video prompt" : "Add a Video Prompt in the Scene Editor first"}</span></>}
            {isVideoActive ? <div className="storyboard-generation-overlay" role="status"><span className="spinner" aria-hidden="true" /><strong>Generating video…</strong><span>This may take several minutes. Other scenes remain available.</span></div> : null}
          </div>
          <div className="storyboard-image-controls">
            <div><span className="storyboard-control-label">Video</span><strong>{videoPreview ? `Version ${videoPreview.version}` : "Not generated"}</strong><small>{videoGenerationMode === "image" ? "Using selected image" : "Using video prompt only"}</small></div>
            <div className="video-generation-actions">
              <fieldset className="video-mode-selector" disabled={isVideoActive}>
                <legend>Generation mode</legend>
                <label><input type="radio" name={`video-mode-${scene.id}`} value="image" checked={videoGenerationMode === "image"} onChange={() => setVideoGenerationMode("image")} disabled={!hasSelectedImage} />Image to video</label>
                <label><input type="radio" name={`video-mode-${scene.id}`} value="text" checked={videoGenerationMode === "text"} onChange={() => setVideoGenerationMode("text")} />Text to video</label>
              </fieldset>
              <Button onClick={() => void onGenerateVideo(scene, videoGenerationMode)} disabled={isVideoActive || !hasVideoPrompt}>{isVideoActive ? "Generating…" : videoState.versions.length ? "Regenerate video" : "Generate video"}</Button>
            </div>
          </div>
          {!hasVideoPrompt ? <p className="storyboard-image-warning">A Video Prompt is required before video generation.</p> : null}
          {videoPreviewFailed ? <p className="storyboard-image-error" role="alert">The selected video file could not be previewed. Try another stored version.</p> : null}
          {videoState.error ? <p className="storyboard-image-error" role="alert">{videoState.error}</p> : null}
          <VersionList scene={scene} mediaType="VIDEO" state={videoState} isActive={isVideoActive} onSelectVersion={onSelectVersion} />
        </section>

        <section className="storyboard-media-section storyboard-audio-section" aria-labelledby={`audio-heading-${scene.id}`}>
          <div className="storyboard-media-heading-row"><h3 id={`audio-heading-${scene.id}`} className="storyboard-media-heading">Audio / Voice</h3>{isAudioActive ? <span className="audio-generation-status" role="status">Generating audio…</span> : null}</div>
          <div className="storyboard-audio-preview" aria-busy={isAudioActive}>
            {audioPreview && !audioPreviewFailed ? <audio key={audioPreview.id} controls preload="metadata" onError={() => setAudioPreviewFailed(true)}><source src={getAssetFileUrl(audioPreview.file_path)} type="audio/mpeg" />Your browser does not support audio playback.</audio> : <div className="storyboard-audio-placeholder"><span className="storyboard-placeholder-mark" aria-hidden="true"><SectionIcon name="media" /></span><div><strong>{audioPreviewFailed ? "Audio preview unavailable" : "No generated voice"}</strong><span>{audioPreviewFailed ? "The stored local audio could not be loaded." : hasNarration ? "Generate speech directly from this scene's narration" : "Add narration in the Scene Editor first"}</span></div></div>}
          </div>
          <div className="storyboard-image-controls">
            <div><span className="storyboard-control-label">Voice</span><strong>{audioPreview ? `Version ${audioPreview.version}` : "Not generated"}</strong><small>{audioPreview ? audioPreview.model : "Uses the default TTS profile"}</small></div>
            <Button onClick={() => void onGenerateAudio(scene)} disabled={isAudioActive || !hasNarration}>{isAudioActive ? "Generating…" : audioState.versions.length ? "Generate new voice" : "Generate voice"}</Button>
          </div>
          {!hasNarration ? <p className="storyboard-image-warning">Narration is required before voice generation.</p> : null}
          {audioPreviewFailed ? <p className="storyboard-image-error" role="alert">The selected audio file could not be previewed. Try another stored version.</p> : null}
          {audioState.error ? <p className="storyboard-image-error" role="alert">{audioState.error}</p> : null}
          <VersionList scene={scene} mediaType="AUDIO" state={audioState} isActive={isAudioActive} onSelectVersion={onSelectVersion} />
        </section>

        <div className="storyboard-narration"><span>Narration</span><p>{scene.narration || "No narration has been added."}</p></div>
        <dl className="storyboard-prompts"><div><dt>Visual prompt</dt><dd>{scene.visual_prompt || "Not set"}</dd></div><div><dt>Video prompt</dt><dd>{scene.video_prompt || "Not set"}</dd></div></dl>
        <footer className="storyboard-card-footer"><span>Duration</span><strong>{scene.duration} sec</strong></footer>
      </article>
    </li>
  );
}

type VersionListProps = {
  scene: Scene;
  mediaType: StoryboardMediaType;
  state: SceneMediaState;
  isActive: boolean;
  onSelectVersion: StoryboardCardProps["onSelectVersion"];
};

function VersionList({ scene, mediaType, state, isActive, onSelectVersion }: VersionListProps) {
  if (!state.versions.length) return null;
  const label = mediaType === "IMAGE" ? "Image" : mediaType === "VIDEO" ? "Video" : "Audio";
  return (
    <div className="asset-version-list" aria-label={`${label} versions for ${scene.title}`}>
      {state.versions.map((version) => <button key={version.id} type="button" className={version.selected ? "asset-version-button selected" : "asset-version-button"} onClick={() => void onSelectVersion(scene.id, version, mediaType)} disabled={isActive || version.selected} aria-label={`${version.selected ? "Selected" : "Select"} ${label.toLowerCase()} version ${version.version}`}><span>v{version.version}</span>{version.selected ? <small>Selected</small> : <small>{version.id === state.previewVersionId ? "Preview" : "Select"}</small>}</button>)}
    </div>
  );
}
