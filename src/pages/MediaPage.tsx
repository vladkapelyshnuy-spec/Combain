import { useCallback, useEffect, useState } from "react";
import { SectionIcon } from "../components/SectionIcon";
import { Button, ErrorState, LoadingState } from "../components/ui";
import type { Asset, AssetType, AssetVersion } from "../models/asset";
import type { Project } from "../models/project";
import type { Scene } from "../models/scene";
import type { AppPath } from "../routing/routes";
import { getAssetFileUrl, getAssets, getAssetVersions } from "../services/assetService";
import { getIpcError } from "../services/projectService";
import { getScenes } from "../services/sceneService";

type MediaPageProps = { selectedProject: Project | null; onNavigate: (path: AppPath) => void };
type MediaItem = { asset: Asset; scene: Scene; sceneNumber: number; selectedVersion: AssetVersion | null };

const mediaGroups: Array<{ type: AssetType; title: string; empty: string }> = [
  { type: "IMAGE", title: "Images", empty: "No generated images yet." },
  { type: "VIDEO", title: "Videos", empty: "No generated videos yet." },
  { type: "AUDIO", title: "Audio", empty: "No generated voice tracks yet." },
];

export function MediaPage({ selectedProject, onNavigate }: MediaPageProps) {
  const [items, setItems] = useState<MediaItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadMedia = useCallback(async () => {
    if (!selectedProject) return;
    setIsLoading(true);
    setError(null);
    try {
      const scenes = await getScenes(selectedProject.id);
      const sceneItems = await Promise.all(
        [...scenes].sort((left, right) => left.order - right.order).map(async (scene, sceneIndex) => {
          const assets = await getAssets(scene.id);
          return Promise.all(assets.map(async (asset) => {
            const versions = await getAssetVersions(asset.id);
            return {
              asset,
              scene,
              sceneNumber: sceneIndex + 1,
              selectedVersion: versions.find((version) => version.selected) ?? null,
            } satisfies MediaItem;
          }));
        }),
      );
      setItems(sceneItems.flat());
    } catch (loadError) {
      setError(getIpcError(loadError).message);
    } finally {
      setIsLoading(false);
    }
  }, [selectedProject]);

  useEffect(() => {
    setItems([]);
    if (selectedProject) void loadMedia();
  }, [loadMedia, selectedProject]);

  if (!selectedProject) {
    return <div className="workspace-page"><header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="media" /></span><div><p className="eyebrow">Media library</p><h1>Media</h1><p className="lead">Browse generated media from every project scene.</p></div></div></header><div className="storyboard-empty"><h2>No project selected</h2><p>Select a project to open its generated media library.</p><Button onClick={() => onNavigate("/projects")}>Go to Projects</Button></div></div>;
  }

  return (
    <div className="workspace-page media-page">
      <header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="media" /></span><div><p className="eyebrow">{selectedProject.name}</p><h1>Media</h1><p className="lead">Review the selected image, video, and voice version for every scene.</p></div></div><div className="media-header-actions"><span className="project-count">{items.length} {items.length === 1 ? "asset" : "assets"}</span><Button variant="secondary" onClick={() => void loadMedia()} disabled={isLoading}>Refresh</Button></div></header>
      {isLoading ? <LoadingState title="Loading media" description="Reading scenes and generated asset versions." /> : null}
      {!isLoading && error ? <ErrorState title="Could not load media" description={error} onRetry={() => void loadMedia()} /> : null}
      {!isLoading && !error ? <div className="media-groups">{mediaGroups.map((group) => {
        const groupItems = items.filter((item) => item.asset.asset_type === group.type);
        return <section className="media-group" key={group.type} aria-labelledby={`media-${group.type.toLowerCase()}`}><header><div><p className="eyebrow">{group.type}</p><h2 id={`media-${group.type.toLowerCase()}`}>{group.title}</h2></div><span>{groupItems.length}</span></header>{groupItems.length ? <div className="media-grid">{groupItems.map((item) => <MediaCard key={item.asset.id} item={item} />)}</div> : <div className="media-empty"><span className="placeholder-mark" aria-hidden="true"><SectionIcon name="media" /></span><p>{group.empty}</p><Button variant="secondary" onClick={() => onNavigate("/storyboard")}>Open Storyboard</Button></div>}</section>;
      })}</div> : null}
    </div>
  );
}

function MediaCard({ item }: { item: MediaItem }) {
  const { asset, scene, sceneNumber, selectedVersion } = item;
  return (
    <article className="media-card">
      <div className={`media-preview media-preview-${asset.asset_type.toLowerCase()}`}>
        {selectedVersion ? asset.asset_type === "IMAGE" ? <img src={getAssetFileUrl(selectedVersion.file_path)} alt={`Selected image for ${scene.title}`} /> : asset.asset_type === "VIDEO" ? <video controls preload="metadata" src={getAssetFileUrl(selectedVersion.file_path)}>Video preview unavailable.</video> : <audio controls preload="metadata" src={getAssetFileUrl(selectedVersion.file_path)}>Audio preview unavailable.</audio> : <div className="media-preview-missing"><SectionIcon name="media" /><span>No selected version</span></div>}
      </div>
      <div className="media-card-copy"><div><span className="timeline-track-pill">{asset.asset_type}</span><span>Scene {String(sceneNumber).padStart(2, "0")}</span></div><h3>{scene.title}</h3><dl><div><dt>Selected version</dt><dd>{selectedVersion ? `v${selectedVersion.version}` : "None"}</dd></div><div><dt>Model</dt><dd>{selectedVersion?.model || "Not available"}</dd></div></dl></div>
    </article>
  );
}
