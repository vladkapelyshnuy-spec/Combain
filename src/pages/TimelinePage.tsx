import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { SectionIcon } from "../components/SectionIcon";
import { TimelinePreview } from "../components/TimelinePreview";
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
type TimelineCandidate = { item: TimelineAsset; track: TimelineTrack; startTime: number; duration: number };
type TimelineInteraction = {
  kind: "move" | "trim-left" | "trim-right";
  clipId: string;
  originClientX: number;
  originalStart: number;
  originalDuration: number;
  previewStart: number;
  previewDuration: number;
  groupStarts: Record<string, number>;
  delta: number;
  additiveSelection: boolean;
};
type TimelineClipboardItem = Pick<TimelineClip, "asset_version_id" | "track" | "start_time" | "duration" | "source_offset">;
type TimelineHistoryEntry = { label: string; undo: () => Promise<void> };
type TimelineContextMenu = { clipId: string; x: number; y: number };
type MediaFilter = "ALL" | "VIDEO" | "AUDIO" | "IMAGE";
type TimelineDropTarget = { track: TimelineTrack; startTime: number; duration: number; valid: boolean };

const DEFAULT_PIXELS_PER_SECOND = 72;
const MIN_TIMELINE_SECONDS = 10;
const MIN_CLIP_DURATION_MS = 100;
const SNAP_THRESHOLD_PX = 9;

function formatSeconds(milliseconds: number) {
  const seconds = milliseconds / 1000;
  return Number.isInteger(seconds) ? `${seconds}` : seconds.toFixed(1);
}

function formatTimestamp(milliseconds: number) {
  const totalSeconds = Math.max(0, milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = (totalSeconds % 60).toFixed(1).padStart(4, "0");
  return `${String(minutes).padStart(2, "0")}:${seconds}`;
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
  const [selectedClipIds, setSelectedClipIds] = useState<Set<string>>(() => new Set());
  const [isLoading, setIsLoading] = useState(false);
  const [activeAction, setActiveAction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playheadTime, setPlayheadTime] = useState(0);
  const [pixelsPerSecond, setPixelsPerSecond] = useState(DEFAULT_PIXELS_PER_SECOND);
  const [interaction, setInteraction] = useState<TimelineInteraction | null>(null);
  const [clipboard, setClipboard] = useState<TimelineClipboardItem[]>([]);
  const [history, setHistory] = useState<TimelineHistoryEntry[]>([]);
  const [contextMenu, setContextMenu] = useState<TimelineContextMenu | null>(null);
  const [isPreviewPlaying, setIsPreviewPlaying] = useState(false);
  const [isMediaPanelOpen, setIsMediaPanelOpen] = useState(true);
  const [mediaFilter, setMediaFilter] = useState<MediaFilter>("ALL");
  const [mediaSearch, setMediaSearch] = useState("");
  const [draggedAssetId, setDraggedAssetId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<TimelineDropTarget | null>(null);
  const interactionRef = useRef<TimelineInteraction | null>(null);
  const editorRef = useRef<HTMLDivElement | null>(null);
  const isPlayheadDraggingRef = useRef(false);
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
      setSelectedClipIds((current) => new Set([...current].filter((id) => effectiveClips.some((clip) => clip.id === id))));
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
    setSelectedClipIds(new Set());
    setPlayheadTime(0);
    setClipboard([]);
    setHistory([]);
    setContextMenu(null);
    setIsPreviewPlaying(false);
    setDraggedAssetId(null);
    setDropTarget(null);
    interactionRef.current = null;
    setInteraction(null);
    if (selectedProject) void loadTimeline();
  }, [loadTimeline, selectedProject]);

  const selectedClips = clips.filter((clip) => selectedClipIds.has(clip.id));
  const selectedClip = selectedClips.length === 1 ? selectedClips[0] : null;
  const versionMedia = useMemo(() => new Map(libraryAssets.flatMap((item) => item.versions.map((version) => [version.id, { ...item, selectedVersion: version }] as const))), [libraryAssets]);
  const versionAssetIds = useMemo(() => new Map([...versionMedia].map(([versionId, media]) => [versionId, media.asset.id])), [versionMedia]);
  const previewMediaByVersion = useMemo(() => new Map([...versionMedia].map(([id, media]) => [id, { version: media.selectedVersion!, sceneTitle: media.scene.title }])), [versionMedia]);
  const contextClip = contextMenu ? clips.find((clip) => clip.id === contextMenu.clipId) ?? null : null;

  const previewClips = useMemo(() => clips.map((clip) => {
    if (!interaction) return clip;
    if (interaction.kind === "move" && clip.id in interaction.groupStarts) {
      return { ...clip, start_time: interaction.groupStarts[clip.id] + interaction.delta };
    }
    return interaction.clipId === clip.id ? { ...clip, start_time: interaction.previewStart, duration: interaction.previewDuration } : clip;
  }), [clips, interaction]);
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
      setSelectedClipIds(new Set([created.id]));
      pushHistory({ label: "Add media", undo: async () => { await deleteClip(created.id); } });
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
    const additive = event.ctrlKey || event.metaKey || event.shiftKey;
    const movingSelection = selectedClipIds.has(clip.id) ? selectedClips : [clip];
    if (!selectedClipIds.has(clip.id) || additive) selectClip(clip.id, additive, !event.shiftKey);
    const nextInteraction: TimelineInteraction = {
      kind,
      clipId: clip.id,
      originClientX: event.clientX,
      originalStart: clip.start_time,
      originalDuration: clip.duration,
      previewStart: clip.start_time,
      previewDuration: clip.duration,
      groupStarts: Object.fromEntries(movingSelection.map((item) => [item.id, item.start_time])),
      delta: 0,
      additiveSelection: additive,
    };
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
      const boundaries = [playheadTime, ...clips
        .filter((item) => item.id !== current.clipId && item.track === clip.track)
        .flatMap((item) => [item.start_time, item.start_time + item.duration])];
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

      const groupMinimumStart = Math.min(...Object.values(current.groupStarts));
      const appliedDelta = current.kind === "move"
        ? Math.max(-groupMinimumStart, previewStart - current.originalStart)
        : 0;
      const nextInteraction = { ...current, previewStart, previewDuration, delta: appliedDelta };
      interactionRef.current = nextInteraction;
      setInteraction(nextInteraction);
    };

    const handlePointerUp = () => {
      const completed = interactionRef.current;
      interactionRef.current = null;
      setInteraction(null);
      if (!completed || !timeline) return;
      if (completed.delta === 0 && completed.previewStart === completed.originalStart && completed.previewDuration === completed.originalDuration) {
        if (!completed.additiveSelection) setSelectedClipIds(new Set([completed.clipId]));
        return;
      }
      setClips((currentClips) => currentClips.map((clip) => {
        if (completed.kind === "move" && clip.id in completed.groupStarts) {
          return { ...clip, start_time: completed.groupStarts[clip.id] + completed.delta };
        }
        return clip.id === completed.clipId
          ? { ...clip, start_time: completed.previewStart, duration: completed.previewDuration }
          : clip;
      }));
      void runAction(`timeline-${completed.kind}`, async () => {
        try {
          if (completed.kind === "move") {
            const entries = Object.entries(completed.groupStarts).sort((left, right) => completed.delta >= 0
              ? right[1] - left[1]
              : left[1] - right[1]);
            const movedIds: string[] = [];
            try {
              for (const [clipId, originalStart] of entries) {
                await moveClip(clipId, originalStart + completed.delta);
                movedIds.push(clipId);
              }
            } catch (moveError) {
              for (const clipId of movedIds.reverse()) {
                await moveClip(clipId, completed.groupStarts[clipId]);
              }
              throw moveError;
            }
            pushHistory({
              label: entries.length > 1 ? "Move clips" : "Move clip",
              undo: async () => {
                for (const [clipId, originalStart] of [...entries].reverse()) await moveClip(clipId, originalStart);
              },
            });
          } else if (completed.kind === "trim-right") {
            await trimClip(completed.clipId, completed.previewDuration);
            pushHistory({ label: "Trim clip", undo: async () => { await trimClip(completed.clipId, completed.originalDuration); } });
          } else {
            await updateClip(completed.clipId, completed.previewStart, completed.previewDuration);
            pushHistory({ label: "Trim clip", undo: async () => { await updateClip(completed.clipId, completed.originalStart, completed.originalDuration); } });
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
  }, [clips, interaction, pixelsPerSecond, playheadTime, timeline]);

  const setPlayheadFromPointer = (event: React.PointerEvent<HTMLElement>, labelOffset = 0) => {
    if (event.button !== 0 && !isPlayheadDraggingRef.current) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const nextTime = Math.max(0, Math.min(timelineContentDuration, Math.round((event.clientX - bounds.left - labelOffset) / pixelsPerSecond * 1000)));
    setPlayheadTime(nextTime);
    isPlayheadDraggingRef.current = true;
  };

  const pushHistory = (entry: TimelineHistoryEntry) => {
    setHistory((current) => [...current.slice(-29), entry]);
  };

  const selectClip = (clipId: string, additive: boolean, toggle = true) => {
    setSelectedClipIds((current) => {
      if (!additive) return new Set([clipId]);
      const next = new Set(current);
      if (toggle && next.has(clipId)) next.delete(clipId);
      else next.add(clipId);
      return next;
    });
  };

  useEffect(() => {
    const stopDragging = () => { isPlayheadDraggingRef.current = false; };
    window.addEventListener("pointerup", stopDragging);
    window.addEventListener("pointercancel", stopDragging);
    return () => {
      window.removeEventListener("pointerup", stopDragging);
      window.removeEventListener("pointercancel", stopDragging);
    };
  }, []);

  const handleAddMissing = () => {
    if (!timeline || missingCandidates.length === 0) return;
    void runAction("add-missing", async () => {
      const updated = await addMissingClips(timeline.id, clips, missingCandidates, versionAssetIds);
      const created = updated.filter((clip) => !clips.some((existing) => existing.id === clip.id));
      await refreshClips(timeline.id);
      if (created.length) pushHistory({ label: "Add missing media", undo: async () => { for (const clip of created) await deleteClip(clip.id); } });
    });
  };

  const splitAtPlayhead = (clip: TimelineClip) => {
    if (!timeline || playheadTime <= clip.start_time || playheadTime >= clip.start_time + clip.duration) return;
    const relativeSplitTime = playheadTime - clip.start_time;
    void runAction("split-playhead", async () => {
      const [first, second] = await splitClip(clip.id, relativeSplitTime);
      await refreshClips(timeline.id);
      setSelectedClipIds(new Set([second.id]));
      pushHistory({
        label: "Split clip",
        undo: async () => {
          await deleteClip(second.id);
          await updateClip(first.id, clip.start_time, clip.duration);
        },
      });
    });
  };

  const copySelectedClips = () => {
    setClipboard(selectedClips.map(({ asset_version_id, track, start_time, duration, source_offset }) => ({ asset_version_id, track, start_time, duration, source_offset })));
  };

  const addClipGroup = (items: TimelineClipboardItem[], baseStart: number, label: string) => {
    if (!timeline || items.length === 0) return;
    const minimumStart = Math.min(...items.map((clip) => clip.start_time));
    void runAction(label.toLowerCase(), async () => {
      const created: TimelineClip[] = [];
      try {
        for (const clip of items) created.push(await addClip(timeline.id, clip.asset_version_id, clip.track, Math.max(0, baseStart + clip.start_time - minimumStart), clip.duration, clip.source_offset));
      } catch (createError) {
        for (const clip of created.reverse()) await deleteClip(clip.id);
        throw createError;
      }
      await refreshClips(timeline.id);
      setSelectedClipIds(new Set(created.map((clip) => clip.id)));
      pushHistory({ label, undo: async () => { for (const clip of created) await deleteClip(clip.id); } });
    });
  };

  const duplicateSelectedClips = () => {
    const end = Math.max(0, ...clips.map((clip) => clip.start_time + clip.duration));
    const items = selectedClips.map(({ asset_version_id, track, start_time, duration, source_offset }) => ({ asset_version_id, track, start_time, duration, source_offset }));
    addClipGroup(items, end + 250, "Duplicate clips");
  };

  const deleteSelectedClips = () => {
    if (!timeline || selectedClips.length === 0) return;
    const deleted = [...selectedClips];
    void runAction("delete", async () => {
      const removed: TimelineClip[] = [];
      try {
        for (const clip of deleted) {
          await deleteClip(clip.id);
          removed.push(clip);
        }
      } catch (deleteError) {
        for (const clip of removed) await addClip(timeline.id, clip.asset_version_id, clip.track, clip.start_time, clip.duration, clip.source_offset);
        throw deleteError;
      }
      await refreshClips(timeline.id);
      setSelectedClipIds(new Set());
      pushHistory({ label: "Delete clips", undo: async () => { for (const clip of deleted) await addClip(timeline.id, clip.asset_version_id, clip.track, clip.start_time, clip.duration, clip.source_offset); } });
    });
  };

  const undoLastAction = () => {
    if (!timeline || history.length === 0) return;
    const entry = history[history.length - 1];
    void runAction("undo", async () => {
      await entry.undo();
      await refreshClips(timeline.id);
      setSelectedClipIds(new Set());
      setHistory((current) => current.slice(0, -1));
    });
  };

  const seekPlayhead = (nextTime: number) => {
    const clampedTime = Math.max(0, Math.min(timelineContentDuration, nextTime));
    setPlayheadTime(clampedTime);
  };

  const togglePreviewPlayback = () => {
    if (timelineContentDuration <= 0) return;
    if (!isPreviewPlaying && playheadTime >= timelineContentDuration) setPlayheadTime(0);
    setIsPreviewPlaying((value) => !value);
  };

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!editorRef.current?.contains(document.activeElement) || isEditableTarget(event.target) || activeAction || interaction) return;
      const key = event.key.toLowerCase();
      const modifier = event.ctrlKey || event.metaKey;
      if ((event.key === "Delete" || event.key === "Backspace") && selectedClips.length) {
        event.preventDefault();
        deleteSelectedClips();
      } else if (modifier && key === "c" && selectedClips.length) {
        event.preventDefault();
        copySelectedClips();
      } else if (modifier && key === "v" && clipboard.length) {
        event.preventDefault();
        addClipGroup(clipboard, playheadTime, "Paste clips");
      } else if (modifier && key === "d" && selectedClips.length) {
        event.preventDefault();
        duplicateSelectedClips();
      } else if (modifier && key === "z" && history.length) {
        event.preventDefault();
        undoLastAction();
      } else if (modifier && key === "b" && selectedClip && playheadInsideSelectedClip) {
        event.preventDefault();
        splitAtPlayhead(selectedClip);
      } else if (event.key === "Escape") {
        setContextMenu(null);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeAction, clipboard, history, interaction, playheadInsideSelectedClip, playheadTime, selectedClip, selectedClips, timeline]);

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
    <div
      className="workspace-page timeline-page"
      ref={editorRef}
      tabIndex={-1}
      onPointerDown={(event) => {
        if (!isEditableTarget(event.target) && !(event.target instanceof HTMLButtonElement)) {
          editorRef.current?.focus({ preventScroll: true });
        }
      }}
    >
      <header className="workspace-header"><div className="workspace-heading"><span className="workspace-heading-icon" aria-hidden="true"><SectionIcon name="timeline" /></span><div><p className="eyebrow">{selectedProject.name}</p><h1>Timeline</h1><p className="lead">Place and edit selected video and audio assets using precise timing.</p></div></div><div className="timeline-header-actions"><span className="project-count">{clips.length} {clips.length === 1 ? "clip" : "clips"}</span><Button variant="secondary" onClick={handleAddMissing} disabled={Boolean(activeAction) || missingCandidates.length === 0}>{activeAction === "add-missing" ? "Adding media…" : missingCandidates.length ? `Add missing media (${missingCandidates.length})` : "All media added"}</Button></div></header>
      {error ? <p className="timeline-error" role="alert">{error}</p> : null}

      <div className={isMediaPanelOpen ? "timeline-desktop-layout" : "timeline-desktop-layout media-collapsed"}>
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
                <div><strong>Scene {item.scene.order} · {item.scene.title}</strong><span>{item.asset.asset_type} · {item.selectedVersion ? `v${item.selectedVersion.version} · ${item.selectedVersion.model} · ${formatSeconds(item.scene.duration * 1000)}s` : "no selected version"}</span></div>
              </article>;
            }) : <div className="timeline-library-empty"><strong>No media found</strong><span>Generate media in Storyboard or change the filter.</span></div>}
          </div>
          <p className="timeline-library-hint">Drag video or audio directly onto its matching track.</p>
        </> : <button className="timeline-library-expand" type="button" aria-label="Expand Media panel" onClick={() => setIsMediaPanelOpen(true)}><span>›</span><strong>Media</strong></button>}
      </aside>

      <main className="timeline-center-workspace">
      <section className="timeline-preview-panel" aria-label="Full timeline preview">
        <TimelinePreview
          clips={clips}
          mediaByVersion={previewMediaByVersion}
          currentTime={playheadTime}
          duration={timelineContentDuration}
          playing={isPreviewPlaying}
          onTimeChange={setPlayheadTime}
          onPlayingChange={setIsPreviewPlaying}
          onError={setError}
        />
        <div className="timeline-transport" aria-label="Preview transport controls">
          <div className="timeline-transport-buttons">
            <Button variant="secondary" aria-label="Go to Timeline start" onClick={() => seekPlayhead(0)}>│◀</Button>
            <Button variant="secondary" aria-label="Move playhead back one second" onClick={() => seekPlayhead(playheadTime - 1000)}>−1s</Button>
            <Button aria-label={isPreviewPlaying ? "Pause timeline" : "Play timeline"} onClick={togglePreviewPlayback} disabled={timelineContentDuration <= 0}>{isPreviewPlaying ? "Pause" : "Play"}</Button>
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
              onPointerDown={(event) => {
                event.currentTarget.setPointerCapture(event.pointerId);
                setPlayheadFromPointer(event, 66);
              }}
              onPointerMove={(event) => { if (isPlayheadDraggingRef.current) setPlayheadFromPointer(event, 66); }}
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
                    setSelectedClipIds(new Set());
                    setPlayheadFromPointer(event);
                  }}
                >
                  {dropTarget?.track === track && dropTarget.valid ? <span className="timeline-drop-indicator" style={{ left: dropTarget.startTime / 1000 * pixelsPerSecond, width: Math.max(24, dropTarget.duration / 1000 * pixelsPerSecond) }}><span>{formatSeconds(dropTarget.startTime)}s</span></span> : null}
                  {previewClips
                    .filter((clip) => clip.track === track)
                    .sort((left, right) => left.start_time - right.start_time)
                    .map((clip) => {
                      const metadata = versionMedia.get(clip.asset_version_id);
                      const isSelected = selectedClipIds.has(clip.id);
                      const isInteracting = interaction?.kind === "move" ? clip.id in interaction.groupStarts : interaction?.clipId === clip.id;
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
                            if (!selectedClipIds.has(clip.id)) setSelectedClipIds(new Set([clip.id]));
                            setContextMenu({
                              clipId: clip.id,
                              x: Math.max(8, Math.min(event.clientX, window.innerWidth - 190)),
                              y: Math.max(8, Math.min(event.clientY, window.innerHeight - 230)),
                            });
                          }}
                          onKeyDown={(event) => {
                            if (event.key !== "Enter" && event.key !== " ") return;
                            event.preventDefault();
                            selectClip(clip.id, event.ctrlKey || event.metaKey || event.shiftKey, !event.shiftKey);
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
          <div><Button variant="secondary" onClick={undoLastAction} disabled={Boolean(activeAction) || history.length === 0} aria-label={history.length ? `Undo ${history[history.length - 1].label}` : "Nothing to undo"}>↶ Undo</Button><strong>{selectedClips.length ? `${selectedClips.length} selected` : "Timeline"}</strong><span>{clipboard.length ? `${clipboard.length} clip${clipboard.length === 1 ? "" : "s"} copied` : "Right-click a clip for actions"}</span></div>
          <div className="timeline-zoom" aria-label="Timeline zoom controls">
            <Button variant="secondary" aria-label="Zoom out" onClick={() => setPixelsPerSecond((value) => Math.max(36, value - 18))} disabled={pixelsPerSecond <= 36}>−</Button>
            <span>{Math.round(pixelsPerSecond / DEFAULT_PIXELS_PER_SECOND * 100)}%</span>
            <Button variant="secondary" aria-label="Zoom in" onClick={() => setPixelsPerSecond((value) => Math.min(144, value + 18))} disabled={pixelsPerSecond >= 144}>+</Button>
            <Button variant="secondary" onClick={() => setPixelsPerSecond(DEFAULT_PIXELS_PER_SECOND)}>Fit</Button>
          </div>
        </div>
      </section>
      </main>

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
        <button role="menuitem" type="button" onClick={() => { setContextMenu(null); duplicateSelectedClips(); }}>Duplicate selection <kbd>Ctrl+D</kbd></button>
        <button role="menuitem" type="button" onClick={() => { copySelectedClips(); setContextMenu(null); }}>Copy selection <kbd>Ctrl+C</kbd></button>
        <button role="menuitem" type="button" disabled={!clipboard.length} onClick={() => { setContextMenu(null); addClipGroup(clipboard, playheadTime, "Paste clips"); }}>Paste at playhead <kbd>Ctrl+V</kbd></button>
        <span role="separator" />
        <button role="menuitem" type="button" disabled={selectedClips.length !== 1 || playheadTime <= contextClip.start_time || playheadTime >= contextClip.start_time + contextClip.duration} onClick={() => { setContextMenu(null); splitAtPlayhead(contextClip); }}>Split at playhead <kbd>Ctrl+B</kbd></button>
        <button className="danger" role="menuitem" type="button" onClick={() => { setContextMenu(null); deleteSelectedClips(); }}>Delete selection <kbd>Del</kbd></button>
      </div> : null}
      <p className="timeline-limitation-note">Clips are locked to exact asset versions. Timeline preview follows render timing; Render/Export UI is not enabled yet.</p>
    </div>
  );
}
