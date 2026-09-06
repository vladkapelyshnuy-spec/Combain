import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { SectionIcon } from "../components/SectionIcon";
import { Button, ErrorState, LoadingState } from "../components/ui";
import type { Asset, AssetVersion } from "../models/asset";
import type { Project } from "../models/project";
import type { Scene } from "../models/scene";
import type { Timeline, TimelineClip, TimelineTrack } from "../models/timeline";
import type { AppPath } from "../routing/routes";
import { getAssetFileUrl, getAssets, getAssetVersions } from "../services/assetService";
import { getIpcError } from "../services/projectService";
import { getScenes } from "../services/sceneService";
import { addClip, createTimeline, deleteClip, getTimeline, listClips, moveClip, splitClip, trimClip, updateClip } from "../services/timelineService";

type TimelinePageProps = { selectedProject: Project | null; onNavigate: (path: AppPath) => void };
type TimelineAsset = { asset: Asset; scene: Scene; versions: AssetVersion[]; selectedVersion: AssetVersion | null };
type TimingDraft = { startSeconds: string; durationSeconds: string; splitSeconds: string };
type TimelineCandidate = { item: TimelineAsset; track: TimelineTrack; startTime: number; duration: number };
type TimelineInteraction = {
  kind: "move" | "trim-left" | "trim-right";
  clipId: string;
  originClientX: number;
  originalStart: number;
  originalDuration: number;
  previewStart: number;
  previewDuration: number;
};
type TimelineClipboard = Pick<TimelineClip, "asset_version_id" | "track" | "duration">;
type TimelineContextMenu = { clipId: string; x: number; y: number };
type MediaFilter = "ALL" | "VIDEO" | "AUDIO" | "IMAGE";
type TimelineDropTarget = { track: TimelineTrack; startTime: number; duration: number; valid: boolean };

const DEFAULT_PIXELS_PER_SECOND = 72;
const MIN_TIMELINE_SECONDS = 10;
const MIN_CLIP_DURATION_MS = 100;
const SNAP_THRESHOLD_PX = 9;

function secondsToMs(value: string) {
  return Math.round(Number(value) * 1000);
}

function formatSeconds(milliseconds: number) {
  const seconds = milliseconds / 1000;
  return Number.isInteger(seconds) ? `${seconds}` : seconds.toFixed(1);
}

function formatTimestamp(milliseconds: number) {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function isEditableTarget(target: EventTarget | null) {
  return target instanceof HTMLInputElement
    || target instanceof HTMLTextAreaElement
    || target instanceof HTMLSelectElement
    || (target instanceof HTMLElement && target.isContentEditable);
}

function getClipKey(assetId: string, track: TimelineTrack) {
  return `${assetId}:${track}`;
}

function snapTime(value: number, threshold: number, boundaries: number[]) {
  const wholeSecond = Math.round(value / 1000) * 1000;
  const candidates = [0, wholeSecond, ...boundaries];
  let result = value;
  let closestDistance = threshold + 1;
  candidates.forEach((candidate) => {
    const distance = Math.abs(candidate - value);
    if (distance <= threshold && distance < closestDistance) {
      result = candidate;
      closestDistance = distance;
    }
  });
  return Math.max(0, Math.round(result));
}

function getTimelineCandidates(scenes: Scene[], assets: TimelineAsset[]) {
  const candidates: TimelineCandidate[] = [];
  let accumulatedDuration = 0;

  [...scenes].sort((left, right) => left.order - right.order).forEach((scene) => {
    const duration = Math.round(scene.duration * 1000);
    if (duration > 0) {
      assets
        .filter((item) => item.scene.id === scene.id && item.selectedVersion)
        .forEach((item) => candidates.push({
          item,
          track: item.asset.asset_type as TimelineTrack,
          startTime: accumulatedDuration,
          duration,
        }));
    }
    accumulatedDuration += Math.max(0, duration);
  });

  return candidates;
}

async function addMissingClips(
  timelineId: string,
  currentClips: TimelineClip[],
  candidates: TimelineCandidate[],
  versionAssetIds: Map<string, string>,
) {
  const existingKeys = new Set(currentClips.map((clip) => getClipKey(versionAssetIds.get(clip.asset_version_id) ?? clip.asset_version_id, clip.track)));
  const createdClips: TimelineClip[] = [];

  for (const candidate of candidates) {
    const key = getClipKey(candidate.item.asset.id, candidate.track);
    if (existingKeys.has(key)) continue;
    createdClips.push(await addClip(
      timelineId,
      candidate.item.selectedVersion!.id,
      candidate.track,
      candidate.startTime,
      candidate.duration,
    ));
    existingKeys.add(key);
  }

  return [...currentClips, ...createdClips];
}

export function TimelinePage({ selectedProject, onNavigate }: TimelinePageProps) {
  const [timeline, setTimeline] = useState<Timeline | null>(null);
  const [clips, setClips] = useState<TimelineClip[]>([]);
  const [scenes, setScenes] = useState<Scene[]>([]);
  const [assets, setAssets] = useState<TimelineAsset[]>([]);
  const [libraryAssets, setLibraryAssets] = useState<TimelineAsset[]>([]);
  const [selectedClipId, setSelectedClipId] = useState<string | null>(null);
  const [timingDraft, setTimingDraft] = useState<TimingDraft>({ startSeconds: "0", durationSeconds: "1", splitSeconds: "0.5" });
  const [isLoading, setIsLoading] = useState(false);
  const [activeAction, setActiveAction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playheadTime, setPlayheadTime] = useState(0);
  const [pixelsPerSecond, setPixelsPerSecond] = useState(DEFAULT_PIXELS_PER_SECOND);
  const [interaction, setInteraction] = useState<TimelineInteraction | null>(null);
  const [clipboard, setClipboard] = useState<TimelineClipboard | null>(null);
  const [contextMenu, setContextMenu] = useState<TimelineContextMenu | null>(null);
  const [isPreviewPlaying, setIsPreviewPlaying] = useState(false);
  const [isMediaPanelOpen, setIsMediaPanelOpen] = useState(true);
  const [mediaFilter, setMediaFilter] = useState<MediaFilter>("ALL");
  const [mediaSearch, setMediaSearch] = useState("");
  const [draggedAssetId, setDraggedAssetId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<TimelineDropTarget | null>(null);
  const interactionRef = useRef<TimelineInteraction | null>(null);
  const mediaRef = useRef<HTMLMediaElement | null>(null);
  const contextMenuRef = useRef<HTMLDivElement | null>(null);
  const autoPopulationPromises = useRef(new Map<string, Promise<TimelineClip[]>>());

  const loadTimeline = useCallback(async () => {
    if (!selectedProject) return;
    setIsLoading(true);
    setError(null);
    try {
      const [existingTimeline, scenes] = await Promise.all([getTimeline(selectedProject.id), getScenes(selectedProject.id)]);
      const loadedTimeline = existingTimeline ?? await createTimeline(selectedProject.id);
      const [loadedClips, sceneAssets] = await Promise.all([
        listClips(loadedTimeline.id),
        Promise.all(scenes.map(async (scene) => {
          const sceneMedia = await getAssets(scene.id);
          return Promise.all(sceneMedia.map(async (asset) => {
            const versions = await getAssetVersions(asset.id);
            return { asset, scene, versions, selectedVersion: versions.find((version) => version.selected) ?? null } satisfies TimelineAsset;
          }));
        })),
      ]);
      const loadedLibraryAssets = sceneAssets.flat();
      const loadedAssets = loadedLibraryAssets.filter((item) => item.asset.asset_type === "VIDEO" || item.asset.asset_type === "AUDIO");
      const versionAssetIds = new Map(loadedLibraryAssets.flatMap((item) => item.versions.map((version) => [version.id, item.asset.id] as const)));
      const candidates = getTimelineCandidates(scenes, loadedAssets);
      let effectiveClips = loadedClips;
      if (loadedClips.length === 0) {
        let populationPromise = autoPopulationPromises.current.get(loadedTimeline.id);
        if (!populationPromise) {
          populationPromise = addMissingClips(loadedTimeline.id, loadedClips, candidates, versionAssetIds);
          autoPopulationPromises.current.set(loadedTimeline.id, populationPromise);
        }
        try {
          effectiveClips = await populationPromise;
        } finally {
          if (autoPopulationPromises.current.get(loadedTimeline.id) === populationPromise) {
            autoPopulationPromises.current.delete(loadedTimeline.id);
          }
        }
      }
      setTimeline(loadedTimeline);
      setClips(effectiveClips);
      setScenes(scenes);
      setAssets(loadedAssets);
      setLibraryAssets(loadedLibraryAssets);
      setSelectedClipId((current) => effectiveClips.some((clip) => clip.id === current) ? current : null);
    } catch (loadError) {
      setError(getIpcError(loadError).message);
    } finally {
      setIsLoading(false);
    }
  }, [selectedProject]);

  useEffect(() => {
    setTimeline(null);
    setClips([]);
    setScenes([]);
    setAssets([]);
    setLibraryAssets([]);
    setSelectedClipId(null);
    setPlayheadTime(0);
    setClipboard(null);
    setContextMenu(null);
    setIsPreviewPlaying(false);
    setDraggedAssetId(null);
    setDropTarget(null);
    interactionRef.current = null;
    setInteraction(null);
    if (selectedProject) void loadTimeline();
  }, [loadTimeline, selectedProject]);

  const selectedClip = clips.find((clip) => clip.id === selectedClipId) ?? null;
  const versionMedia = useMemo(() => new Map(libraryAssets.flatMap((item) => item.versions.map((version) => [version.id, { ...item, selectedVersion: version }] as const))), [libraryAssets]);
  const versionAssetIds = useMemo(() => new Map([...versionMedia].map(([versionId, media]) => [versionId, media.asset.id])), [versionMedia]);
  const selectedAsset = selectedClip ? versionMedia.get(selectedClip.asset_version_id) ?? null : null;
  const contextClip = contextMenu ? clips.find((clip) => clip.id === contextMenu.clipId) ?? null : null;

  useEffect(() => {
    setIsPreviewPlaying(false);
    if (!selectedClip) return;
    setTimingDraft({
      startSeconds: formatSeconds(selectedClip.start_time),
      durationSeconds: formatSeconds(selectedClip.duration),
      splitSeconds: formatSeconds(selectedClip.duration / 2),
    });
  }, [selectedClip]);

  const previewClips = useMemo(() => clips.map((clip) => interaction?.clipId === clip.id ? {
    ...clip,
    start_time: interaction.previewStart,
    duration: interaction.previewDuration,
  } : clip), [clips, interaction]);
  const projectDuration = scenes.reduce((total, scene) => total + Math.max(0, scene.duration), 0);
  const timelineContentDuration = Math.max(projectDuration * 1000, Math.max(0, ...previewClips.map((clip) => clip.start_time + clip.duration)));
  const timelineSeconds = Math.max(MIN_TIMELINE_SECONDS, Math.ceil(timelineContentDuration / 1000) + 2);
  const rulerMarks = useMemo(() => Array.from({ length: Math.floor(timelineSeconds / 5) + 1 }, (_, index) => index * 5), [timelineSeconds]);
  const timelineCandidates = useMemo(() => getTimelineCandidates(scenes, assets), [scenes, assets]);
  const existingClipKeys = useMemo(() => new Set(clips.map((clip) => {
    const media = versionMedia.get(clip.asset_version_id);
    return getClipKey(media?.asset.id ?? clip.asset_version_id, clip.track);
  })), [clips, versionMedia]);
  const missingCandidates = timelineCandidates.filter((candidate) => !existingClipKeys.has(getClipKey(candidate.item.asset.id, candidate.track)));
  const filteredLibraryAssets = useMemo(() => libraryAssets.filter((item) => {
    const matchesType = mediaFilter === "ALL" || item.asset.asset_type === mediaFilter;
    const query = mediaSearch.trim().toLowerCase();
    return matchesType && (!query || item.scene.title.toLowerCase().includes(query));
  }), [libraryAssets, mediaFilter, mediaSearch]);
  const playheadInsideSelectedClip = Boolean(selectedClip
    && playheadTime > selectedClip.start_time
    && playheadTime < selectedClip.start_time + selectedClip.duration);

  const refreshClips = async (timelineId: string) => setClips(await listClips(timelineId));
  const runAction = async (name: string, action: () => Promise<void>) => {
    if (activeAction) return;
    setActiveAction(name);
    setError(null);
    try { await action(); } catch (actionError) { setError(getIpcError(actionError).message); } finally { setActiveAction(null); }
  };

  const handleMediaDragOver = (event: React.DragEvent<HTMLDivElement>, track: TimelineTrack) => {
    const item = libraryAssets.find((candidate) => candidate.asset.id === draggedAssetId);
    if (!item || !item.selectedVersion) return;
    const valid = item.asset.asset_type === track;
    event.preventDefault();
    event.dataTransfer.dropEffect = valid ? "copy" : "none";
    const bounds = event.currentTarget.getBoundingClientRect();
    const rawStart = Math.max(0, Math.round((event.clientX - bounds.left) / pixelsPerSecond * 1000));
    const boundaries = clips.filter((clip) => clip.track === track).flatMap((clip) => [clip.start_time, clip.start_time + clip.duration]);
    const duration = Math.max(MIN_CLIP_DURATION_MS, Math.round(item.scene.duration * 1000));
    setDropTarget({
      track,
      startTime: snapTime(rawStart, SNAP_THRESHOLD_PX / pixelsPerSecond * 1000, boundaries),
      duration,
      valid,
    });
  };

  const handleMediaDrop = (event: React.DragEvent<HTMLDivElement>, track: TimelineTrack) => {
    event.preventDefault();
    const item = libraryAssets.find((candidate) => candidate.asset.id === draggedAssetId);
    const placement = dropTarget;
    setDraggedAssetId(null);
    setDropTarget(null);
    if (!timeline || !item?.selectedVersion || !placement?.valid || placement.track !== track || item.asset.asset_type !== track) return;
    const assetVersionId = item.selectedVersion.id;
    void runAction("drop-media", async () => {
      const created = await addClip(timeline.id, assetVersionId, track, placement.startTime, placement.duration);
      await refreshClips(timeline.id);
      setSelectedClipId(created.id);
    });
  };

  const beginInteraction = (
    event: React.PointerEvent,
    clip: TimelineClip,
    kind: TimelineInteraction["kind"],
  ) => {
    if (activeAction || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const nextInteraction: TimelineInteraction = {
      kind,
      clipId: clip.id,
      originClientX: event.clientX,
      originalStart: clip.start_time,
      originalDuration: clip.duration,
      previewStart: clip.start_time,
      previewDuration: clip.duration,
    };
    setSelectedClipId(clip.id);
    interactionRef.current = nextInteraction;
    setInteraction(nextInteraction);
  };

  useEffect(() => {
    if (!interaction) return;

    const handlePointerMove = (event: PointerEvent) => {
      const current = interactionRef.current;
      if (!current) return;
      const clip = clips.find((item) => item.id === current.clipId);
      if (!clip) return;
      const delta = Math.round((event.clientX - current.originClientX) / pixelsPerSecond * 1000);
      const threshold = SNAP_THRESHOLD_PX / pixelsPerSecond * 1000;
      const boundaries = clips
        .filter((item) => item.id !== current.clipId && item.track === clip.track)
        .flatMap((item) => [item.start_time, item.start_time + item.duration]);
      let previewStart = current.originalStart;
      let previewDuration = current.originalDuration;

      if (current.kind === "move") {
        const moveBoundaries = [
          ...boundaries,
          ...boundaries.map((boundary) => boundary - current.originalDuration).filter((boundary) => boundary >= 0),
        ];
        previewStart = snapTime(Math.max(0, current.originalStart + delta), threshold, moveBoundaries);
      } else if (current.kind === "trim-right") {
        const rawEnd = Math.max(current.originalStart + MIN_CLIP_DURATION_MS, current.originalStart + current.originalDuration + delta);
        const snappedEnd = snapTime(rawEnd, threshold, boundaries);
        previewDuration = Math.max(MIN_CLIP_DURATION_MS, snappedEnd - current.originalStart);
      } else {
        const originalEnd = current.originalStart + current.originalDuration;
        const rawStart = Math.min(originalEnd - MIN_CLIP_DURATION_MS, Math.max(0, current.originalStart + delta));
        previewStart = Math.min(originalEnd - MIN_CLIP_DURATION_MS, snapTime(rawStart, threshold, boundaries));
        previewDuration = originalEnd - previewStart;
      }

      const nextInteraction = { ...current, previewStart, previewDuration };
      interactionRef.current = nextInteraction;
      setInteraction(nextInteraction);
    };

    const handlePointerUp = () => {
      const completed = interactionRef.current;
      interactionRef.current = null;
      setInteraction(null);
      if (!completed || !timeline) return;
      if (completed.previewStart === completed.originalStart && completed.previewDuration === completed.originalDuration) return;
      setClips((currentClips) => currentClips.map((clip) => clip.id === completed.clipId ? {
        ...clip,
        start_time: completed.previewStart,
        duration: completed.previewDuration,
      } : clip));
      void runAction(`timeline-${completed.kind}`, async () => {
        try {
          if (completed.kind === "move") {
            await moveClip(completed.clipId, completed.previewStart);
          } else if (completed.kind === "trim-right") {
            await trimClip(completed.clipId, completed.previewDuration);
          } else {
            await updateClip(completed.clipId, completed.previewStart, completed.previewDuration);
          }
        } finally {
          await refreshClips(timeline.id);
        }
      });
    };

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp, { once: true });
    window.addEventListener("pointercancel", handlePointerUp, { once: true });
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerUp);
    };
  }, [clips, interaction, pixelsPerSecond, timeline]);

  const setPlayheadFromPointer = (event: React.PointerEvent<HTMLElement>, labelOffset = 0) => {
    if (event.button !== 0) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const nextTime = Math.max(0, Math.min(timelineContentDuration, Math.round((event.clientX - bounds.left - labelOffset) / pixelsPerSecond * 1000)));
    setPlayheadTime(nextTime);
    if (mediaRef.current && selectedClip && nextTime >= selectedClip.start_time && nextTime <= selectedClip.start_time + selectedClip.duration) {
      mediaRef.current.currentTime = (nextTime - selectedClip.start_time) / 1000;
    }
  };

  const handleAddMissing = () => {
    if (!timeline || missingCandidates.length === 0) return;
    void runAction("add-missing", async () => {
      await addMissingClips(timeline.id, clips, missingCandidates, versionAssetIds);
      await refreshClips(timeline.id);
    });
  };

  const handleSaveTiming = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!timeline || !selectedClip) return;
    const startTime = secondsToMs(timingDraft.startSeconds);
    const duration = secondsToMs(timingDraft.durationSeconds);
    if (!Number.isFinite(startTime) || !Number.isFinite(duration) || startTime < 0 || duration <= 0) {
      setError("Start time must be zero or greater and duration must be greater than zero.");
      return;
    }
    void runAction("update", async () => { await updateClip(selectedClip.id, startTime, duration); await refreshClips(timeline.id); });
  };

  const handleSplit = () => {
    if (!timeline || !selectedClip) return;
    const splitTime = secondsToMs(timingDraft.splitSeconds);
    if (!Number.isFinite(splitTime) || splitTime <= 0 || splitTime >= selectedClip.duration) {
      setError("Split time must be greater than zero and shorter than the clip duration.");
      return;
    }
    void runAction("split", async () => {
      const [, second] = await splitClip(selectedClip.id, splitTime);
      await refreshClips(timeline.id);
      setSelectedClipId(second.id);
    });
  };

  const splitAtPlayhead = (clip: TimelineClip) => {
    if (!timeline || playheadTime <= clip.start_time || playheadTime >= clip.start_time + clip.duration) return;
    const relativeSplitTime = playheadTime - clip.start_time;
    void runAction("split-playhead", async () => {
      const [, second] = await splitClip(clip.id, relativeSplitTime);
      await refreshClips(timeline.id);
      setSelectedClipId(second.id);
    });
  };

  const handleSplitAtPlayhead = () => {
    if (!selectedClip) return;
    splitAtPlayhead(selectedClip);
  };

  const deleteTimelineClip = (clip: TimelineClip) => {
    if (!timeline) return;
    void runAction("delete", async () => { await deleteClip(clip.id); await refreshClips(timeline.id); setSelectedClipId((current) => current === clip.id ? null : current); });
  };

  const handleDelete = () => {
    if (selectedClip) deleteTimelineClip(selectedClip);
  };

  const copyTimelineClip = (clip: TimelineClip) => {
    setClipboard({ asset_version_id: clip.asset_version_id, track: clip.track, duration: clip.duration });
  };

  const duplicateTimelineClip = (clip: TimelineClip) => {
    if (!timeline) return;
    void runAction("duplicate", async () => {
      const created = await addClip(timeline.id, clip.asset_version_id, clip.track, clip.start_time + clip.duration, clip.duration);
      await refreshClips(timeline.id);
      setSelectedClipId(created.id);
    });
  };

  const pasteTimelineClip = () => {
    if (!timeline || !clipboard) return;
    const fallbackStart = selectedClip ? selectedClip.start_time + selectedClip.duration : 0;
    const startTime = playheadTime > 0 ? playheadTime : fallbackStart;
    void runAction("paste", async () => {
      const created = await addClip(timeline.id, clipboard.asset_version_id, clipboard.track, startTime, clipboard.duration);
      await refreshClips(timeline.id);
      setSelectedClipId(created.id);
    });
  };

  const seekPlayhead = (nextTime: number) => {
    const clampedTime = Math.max(0, Math.min(timelineContentDuration, nextTime));
    setPlayheadTime(clampedTime);
    if (mediaRef.current && selectedClip && clampedTime >= selectedClip.start_time && clampedTime <= selectedClip.start_time + selectedClip.duration) {
      mediaRef.current.currentTime = (clampedTime - selectedClip.start_time) / 1000;
    }
  };

  const togglePreviewPlayback = () => {
    const media = mediaRef.current;
    if (!media) return;
    if (media.paused) {
      void media.play().catch((playError: unknown) => setError(getIpcError(playError).message));
    } else {
      media.pause();
    }
  };

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (isEditableTarget(event.target) || activeAction || interaction) return;
      const key = event.key.toLowerCase();
      if ((event.key === "Delete" || event.key === "Backspace") && selectedClip) {
        event.preventDefault();
        deleteTimelineClip(selectedClip);
      } else if (event.ctrlKey && key === "c" && selectedClip) {
        event.preventDefault();
        copyTimelineClip(selectedClip);
      } else if (event.ctrlKey && key === "v" && clipboard) {
        event.preventDefault();
        pasteTimelineClip();
      } else if (event.ctrlKey && key === "d" && selectedClip) {
        event.preventDefault();
        duplicateTimelineClip(selectedClip);
      } else if (event.ctrlKey && key === "b" && selectedClip && playheadInsideSelectedClip) {
        event.preventDefault();
        splitAtPlayhead(selectedClip);
      } else if (event.key === "Escape") {
        setContextMenu(null);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeAction, clipboard, interaction, playheadInsideSelectedClip, playheadTime, selectedClip, timeline]);

  useEffect(() => {
    if (!contextMenu) return;
    const handleOutsidePointer = (event: PointerEvent) => {
      if (!contextMenuRef.current?.contains(event.target as Node)) setContextMenu(null);
    };
    window.addEventListener("pointerdown", handleOutsidePointer);
    const focusFrame = window.requestAnimationFrame(() => contextMenuRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus());
    return () => {
      window.removeEventListener("pointerdown", handleOutsidePointer);
      window.cancelAnimationFrame(focusFrame);
    };
  }, [contextMenu]);

  if (!selectedProject) {
    return <div className="workspace-page"><header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="timeline" /></span><div><p className="eyebrow">Timeline workspace</p><h1>Timeline</h1><p className="lead">Arrange selected video and voice assets into a sequence.</p></div></div></header><div className="storyboard-empty"><h2>No project selected</h2><p>Select a project before editing its timeline.</p><Button onClick={() => onNavigate("/projects")}>Go to Projects</Button></div></div>;
  }
  if (isLoading) return <div className="workspace-page"><LoadingState title="Loading timeline" description="Reading clips and selected media versions." /></div>;
  if (!timeline && error) return <div className="workspace-page"><ErrorState title="Could not load timeline" description={error} onRetry={() => void loadTimeline()} /></div>;

  return (
    <div className="workspace-page timeline-page">
      <header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="timeline" /></span><div><p className="eyebrow">{selectedProject.name}</p><h1>Timeline</h1><p className="lead">Place and edit selected video and audio assets using precise timing.</p></div></div><div className="timeline-header-actions"><span className="project-count">{clips.length} {clips.length === 1 ? "clip" : "clips"}</span><Button variant="secondary" onClick={handleAddMissing} disabled={Boolean(activeAction) || missingCandidates.length === 0}>{activeAction === "add-missing" ? "Adding media…" : missingCandidates.length ? `Add missing media (${missingCandidates.length})` : "All media added"}</Button></div></header>
      {error ? <p className="timeline-error" role="alert">{error}</p> : null}

      <div className={`${isMediaPanelOpen ? "timeline-desktop-layout" : "timeline-desktop-layout media-collapsed"}${selectedClip ? " properties-open" : ""}`}>
      <aside className="timeline-library" aria-label="Project media">
        {isMediaPanelOpen ? <>
          <header><div><p className="eyebrow">Project media</p><h2>Media</h2></div><button type="button" aria-label="Collapse Media panel" onClick={() => setIsMediaPanelOpen(false)}>‹</button></header>
          <label className="timeline-library-search"><span>Search media</span><input type="search" value={mediaSearch} placeholder="Scene title" onChange={(event) => setMediaSearch(event.currentTarget.value)} /></label>
          <div className="timeline-library-filters" aria-label="Filter media">
            {(["ALL", "VIDEO", "AUDIO", "IMAGE"] as const).map((filter) => <button key={filter} type="button" className={mediaFilter === filter ? "active" : ""} aria-pressed={mediaFilter === filter} onClick={() => setMediaFilter(filter)}>{filter === "ALL" ? "All" : filter.charAt(0) + filter.slice(1).toLowerCase()}</button>)}
          </div>
          <div className="timeline-library-list">
            {filteredLibraryAssets.length ? filteredLibraryAssets.map((item) => {
              const canDrag = item.asset.asset_type !== "IMAGE" && Boolean(item.selectedVersion);
              return <article
                key={item.asset.id}
                className={`timeline-library-item${draggedAssetId === item.asset.id ? " dragging" : ""}${canDrag ? " draggable" : ""}`}
                draggable={canDrag}
                onDragStart={(event) => {
                  if (!canDrag) return;
                  setDraggedAssetId(item.asset.id);
                  event.dataTransfer.effectAllowed = "copy";
                  event.dataTransfer.setData("text/plain", item.asset.id);
                }}
                onDragEnd={() => { setDraggedAssetId(null); setDropTarget(null); }}
              >
                <div className={`timeline-library-thumb ${item.asset.asset_type.toLowerCase()}`}>
                  {item.selectedVersion && item.asset.asset_type === "IMAGE" ? <img src={getAssetFileUrl(item.selectedVersion.file_path)} alt="" /> : item.selectedVersion && item.asset.asset_type === "VIDEO" ? <video muted preload="metadata" src={getAssetFileUrl(item.selectedVersion.file_path)} /> : <span aria-hidden="true">{item.asset.asset_type === "AUDIO" ? "♫" : "◇"}</span>}
                </div>
                <div><strong>{item.scene.title}</strong><span>{item.asset.asset_type} · {item.selectedVersion ? `v${item.selectedVersion.version}` : "no selected version"}</span></div>
              </article>;
            }) : <div className="timeline-library-empty"><strong>No media found</strong><span>Generate media in Storyboard or change the filter.</span></div>}
          </div>
          <p className="timeline-library-hint">Drag video or audio directly onto its matching track.</p>
        </> : <button className="timeline-library-expand" type="button" aria-label="Expand Media panel" onClick={() => setIsMediaPanelOpen(true)}><span>›</span><strong>Media</strong></button>}
      </aside>

      <main className="timeline-center-workspace">
      <section className="timeline-preview-panel" aria-label="Selected clip preview">
        <div className="timeline-preview-stage">
          {selectedAsset?.selectedVersion && selectedClip ? selectedClip.track === "VIDEO" ? (
            <video
              key={selectedAsset.selectedVersion.id}
              ref={(element) => { mediaRef.current = element; }}
              preload="metadata"
              src={getAssetFileUrl(selectedAsset.selectedVersion.file_path)}
              onPlay={() => setIsPreviewPlaying(true)}
              onPause={() => setIsPreviewPlaying(false)}
              onEnded={() => setIsPreviewPlaying(false)}
              onTimeUpdate={(event) => setPlayheadTime(Math.min(selectedClip.start_time + selectedClip.duration, selectedClip.start_time + event.currentTarget.currentTime * 1000))}
            >Video preview unavailable.</video>
          ) : (
            <div className="timeline-audio-stage">
              <span aria-hidden="true">♫</span>
              <strong>{selectedAsset.scene.title}</strong>
              <small>Selected audio · v{selectedAsset.selectedVersion.version}</small>
              <audio
                key={selectedAsset.selectedVersion.id}
                ref={(element) => { mediaRef.current = element; }}
                preload="metadata"
                src={getAssetFileUrl(selectedAsset.selectedVersion.file_path)}
                onPlay={() => setIsPreviewPlaying(true)}
                onPause={() => setIsPreviewPlaying(false)}
                onEnded={() => setIsPreviewPlaying(false)}
                onTimeUpdate={(event) => setPlayheadTime(Math.min(selectedClip.start_time + selectedClip.duration, selectedClip.start_time + event.currentTarget.currentTime * 1000))}
              >Audio preview unavailable.</audio>
            </div>
          ) : <div className="timeline-preview-empty"><span aria-hidden="true">▶</span><strong>{selectedClip ? "No selected asset version" : "Select a clip to preview"}</strong><small>Clip Preview only — combined Timeline playback is not available.</small></div>}
        </div>
        <div className="timeline-transport" aria-label="Preview transport controls">
          <div className="timeline-transport-buttons">
            <Button variant="secondary" aria-label="Go to Timeline start" onClick={() => seekPlayhead(0)}>│◀</Button>
            <Button variant="secondary" aria-label="Move playhead back one second" onClick={() => seekPlayhead(playheadTime - 1000)}>−1s</Button>
            <Button aria-label={isPreviewPlaying ? "Pause selected media" : "Play selected media"} onClick={togglePreviewPlayback} disabled={!selectedAsset?.selectedVersion}>{isPreviewPlaying ? "Pause" : "Play"}</Button>
            <Button variant="secondary" aria-label="Move playhead forward one second" onClick={() => seekPlayhead(playheadTime + 1000)}>+1s</Button>
            <Button variant="secondary" aria-label="Go to Timeline end" onClick={() => seekPlayhead(timelineContentDuration)}>▶│</Button>
          </div>
          <div className="timeline-timecode"><strong>{formatTimestamp(playheadTime)}</strong><span>/</span><span>{formatTimestamp(timelineContentDuration)}</span></div>
        </div>
      </section>

      <section className="timeline-editor-panel" aria-label="Timeline tracks">
        <div className="timeline-scroll">
          <div className="timeline-canvas" style={{ width: 66 + timelineSeconds * pixelsPerSecond }}>
            <div
              className="timeline-ruler"
              role="slider"
              tabIndex={0}
              aria-label="Timeline playhead"
              aria-valuemin={0}
              aria-valuemax={Math.round(timelineContentDuration)}
              aria-valuenow={playheadTime}
              onPointerDown={(event) => setPlayheadFromPointer(event, 66)}
              onKeyDown={(event) => {
                if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
                event.preventDefault();
                setPlayheadTime((value) => Math.max(0, Math.min(timelineContentDuration, value + (event.key === "ArrowRight" ? 1000 : -1000))));
              }}
            >
              {rulerMarks.map((second) => <span key={second} style={{ left: 66 + second * pixelsPerSecond }}>{formatTimestamp(second * 1000)}</span>)}
            </div>
            <div className="timeline-playhead" style={{ left: 66 + playheadTime / 1000 * pixelsPerSecond }} aria-hidden="true"><span /></div>
            {(["VIDEO", "AUDIO"] as const).map((track) => (
              <div className="timeline-track" key={track}>
                <div className="timeline-track-label">{track}</div>
                <div
                  className={`timeline-track-lane${dropTarget?.track === track ? dropTarget.valid ? " drop-valid" : " drop-invalid" : ""}`}
                  style={{ backgroundSize: `${pixelsPerSecond}px 100%` }}
                  onDragOver={(event) => handleMediaDragOver(event, track)}
                  onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropTarget(null); }}
                  onDrop={(event) => handleMediaDrop(event, track)}
                  onPointerDown={(event) => {
                    if (event.target !== event.currentTarget) return;
                    setSelectedClipId(null);
                    setPlayheadFromPointer(event);
                  }}
                >
                  {dropTarget?.track === track && dropTarget.valid ? <span className="timeline-drop-indicator" style={{ left: dropTarget.startTime / 1000 * pixelsPerSecond, width: Math.max(24, dropTarget.duration / 1000 * pixelsPerSecond) }}><span>{formatSeconds(dropTarget.startTime)}s</span></span> : null}
                  {previewClips
                    .filter((clip) => clip.track === track)
                    .sort((left, right) => left.start_time - right.start_time)
                    .map((clip) => {
                      const metadata = versionMedia.get(clip.asset_version_id);
                      const isSelected = clip.id === selectedClipId;
                      const isInteracting = interaction?.clipId === clip.id;
                      return (
                        <div
                          key={clip.id}
                          role="button"
                          tabIndex={0}
                          className={`${isSelected ? `timeline-clip timeline-clip-${track.toLowerCase()} selected` : `timeline-clip timeline-clip-${track.toLowerCase()}`}${isInteracting ? " interacting" : ""}`}
                          style={{ left: clip.start_time / 1000 * pixelsPerSecond, width: Math.max(24, clip.duration / 1000 * pixelsPerSecond) }}
                          onPointerDown={(event) => beginInteraction(event, clip, "move")}
                          onContextMenu={(event) => {
                            event.preventDefault();
                            setSelectedClipId(clip.id);
                            setContextMenu({
                              clipId: clip.id,
                              x: Math.max(8, Math.min(event.clientX, window.innerWidth - 190)),
                              y: Math.max(8, Math.min(event.clientY, window.innerHeight - 230)),
                            });
                          }}
                          onKeyDown={(event) => {
                            if (event.key !== "Enter" && event.key !== " ") return;
                            event.preventDefault();
                            setSelectedClipId(clip.id);
                          }}
                          aria-pressed={isSelected}
                          aria-label={`${track} clip, ${metadata?.scene.title ?? "Unknown asset"}, starts at ${formatSeconds(clip.start_time)} seconds, duration ${formatSeconds(clip.duration)} seconds`}
                        >
                          {isSelected ? <span className="timeline-resize-handle left" aria-hidden="true" onPointerDown={(event) => beginInteraction(event, clip, "trim-left")} /> : null}
                          <span className="timeline-clip-decoration" aria-hidden="true" />
                          <strong>{track === "AUDIO" ? "♫ " : ""}{metadata?.scene.title ?? "Unknown asset"}</strong>
                          <span>{formatSeconds(clip.start_time)}s · {formatSeconds(clip.duration)}s · {metadata?.selectedVersion ? `v${metadata.selectedVersion.version}` : "no version"}</span>
                          {isSelected ? <span className="timeline-resize-handle right" aria-hidden="true" onPointerDown={(event) => beginInteraction(event, clip, "trim-right")} /> : null}
                          {isInteracting ? <span className="timeline-interaction-value">{formatSeconds(clip.start_time)}s / {formatSeconds(clip.duration)}s</span> : null}
                        </div>
                      );
                    })}
                </div>
              </div>
            ))}
          </div>
        </div>
        <div className="timeline-toolbar">
          <div><strong>Timeline</strong><span>{clipboard ? `${clipboard.track} clip copied` : "Right-click a clip for actions"}</span></div>
          <div className="timeline-zoom" aria-label="Timeline zoom controls">
            <Button variant="secondary" aria-label="Zoom out" onClick={() => setPixelsPerSecond((value) => Math.max(36, value - 18))} disabled={pixelsPerSecond <= 36}>−</Button>
            <span>{Math.round(pixelsPerSecond / DEFAULT_PIXELS_PER_SECOND * 100)}%</span>
            <Button variant="secondary" aria-label="Zoom in" onClick={() => setPixelsPerSecond((value) => Math.min(144, value + 18))} disabled={pixelsPerSecond >= 144}>+</Button>
            <Button variant="secondary" onClick={() => setPixelsPerSecond(DEFAULT_PIXELS_PER_SECOND)}>Fit</Button>
          </div>
        </div>
      </section>
      </main>

      {selectedClip ? <aside className="timeline-properties" aria-labelledby="clip-editor-heading">
          <header><div><p className="eyebrow">Selected clip</p><h2 id="clip-editor-heading">Properties</h2></div><button type="button" aria-label="Close clip properties" onClick={() => setSelectedClipId(null)}>×</button></header>
            <div className="timeline-clip-summary">
              <span className={`timeline-track-pill ${selectedClip.track.toLowerCase()}`}>{selectedClip.track}</span>
              <strong>{selectedAsset?.scene.title ?? "Unknown asset"}</strong>
              <dl>
                <div><dt>Start</dt><dd>{formatSeconds(selectedClip.start_time)} sec</dd></div>
                <div><dt>Duration</dt><dd>{formatSeconds(selectedClip.duration)} sec</dd></div>
                <div><dt>Version</dt><dd>{selectedAsset?.selectedVersion ? `v${selectedAsset.selectedVersion.version}` : "None"}</dd></div>
              </dl>
              <small>This clip is locked to this exact asset version.</small>
            </div>
            {!selectedAsset?.selectedVersion ? <p className="timeline-panel-warning">This asset has no selected version to preview.</p> : null}
            <form className="timeline-form" onSubmit={handleSaveTiming}>
              <div className="timeline-form-row"><label>Start (sec)<input type="number" min="0" step="0.1" value={timingDraft.startSeconds} onChange={(event) => { const inputValue = event.currentTarget.value; setTimingDraft((current) => ({ ...current, startSeconds: inputValue })); }} required /></label><label>Duration (sec)<input type="number" min="0.1" step="0.1" value={timingDraft.durationSeconds} onChange={(event) => { const inputValue = event.currentTarget.value; setTimingDraft((current) => ({ ...current, durationSeconds: inputValue })); }} required /></label></div>
              <Button type="submit" disabled={Boolean(activeAction)}>Save timing</Button>
            </form>
            <div className="timeline-playhead-split"><div><span>Playhead</span><strong>{formatSeconds(playheadTime)} sec</strong></div><Button variant="secondary" onClick={handleSplitAtPlayhead} disabled={Boolean(activeAction) || !playheadInsideSelectedClip}>Split at playhead</Button></div>
            <div className="timeline-split-controls"><label>Precise split after (sec)<input type="number" min="0.1" step="0.1" value={timingDraft.splitSeconds} onChange={(event) => { const inputValue = event.currentTarget.value; setTimingDraft((current) => ({ ...current, splitSeconds: inputValue })); }} /></label><Button variant="secondary" onClick={handleSplit} disabled={Boolean(activeAction)}>Split clip</Button></div>
            <Button variant="secondary" onClick={handleDelete} disabled={Boolean(activeAction)}>Delete clip</Button>
      </aside> : null}
      </div>
      {contextMenu && contextClip ? <div
        ref={contextMenuRef}
        className="timeline-context-menu"
        role="menu"
        aria-label="Clip actions"
        style={{ left: contextMenu.x, top: contextMenu.y }}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
          event.preventDefault();
          const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
          const currentIndex = buttons.indexOf(document.activeElement as HTMLButtonElement);
          const direction = event.key === "ArrowDown" ? 1 : -1;
          buttons[(currentIndex + direction + buttons.length) % buttons.length]?.focus();
        }}
      >
        <button role="menuitem" type="button" onClick={() => { setContextMenu(null); duplicateTimelineClip(contextClip); }}>Duplicate <kbd>Ctrl+D</kbd></button>
        <button role="menuitem" type="button" onClick={() => { copyTimelineClip(contextClip); setContextMenu(null); }}>Copy <kbd>Ctrl+C</kbd></button>
        <button role="menuitem" type="button" disabled={!clipboard} onClick={() => { setContextMenu(null); pasteTimelineClip(); }}>Paste <kbd>Ctrl+V</kbd></button>
        <span role="separator" />
        <button role="menuitem" type="button" disabled={playheadTime <= contextClip.start_time || playheadTime >= contextClip.start_time + contextClip.duration} onClick={() => { setContextMenu(null); splitAtPlayhead(contextClip); }}>Split at playhead <kbd>Ctrl+B</kbd></button>
        <button className="danger" role="menuitem" type="button" onClick={() => { setContextMenu(null); deleteTimelineClip(contextClip); }}>Delete <kbd>Del</kbd></button>
      </div> : null}
      <p className="timeline-limitation-note">Clips are locked to exact asset versions. Combined synchronized preview and Render/Export UI are not enabled yet.</p>
    </div>
  );
}
