import { useCallback, useEffect, useMemo, useRef } from "react";
import type { AssetVersion } from "../models/asset";
import type { TimelineClip } from "../models/timeline";
import { getAssetFileUrl } from "../services/assetService";

type PreviewMedia = {
  version: AssetVersion;
  sceneTitle: string;
};

type TimelinePreviewProps = {
  clips: TimelineClip[];
  mediaByVersion: Map<string, PreviewMedia>;
  currentTime: number;
  duration: number;
  playing: boolean;
  onTimeChange: (time: number) => void;
  onPlayingChange: (playing: boolean) => void;
  onError: (message: string) => void;
};

const SYNC_TOLERANCE_SECONDS = 0.18;

function isActive(clip: TimelineClip, time: number) {
  return time >= clip.start_time && time < clip.start_time + clip.duration;
}

function sourceTimeSeconds(clip: TimelineClip, timelineTime: number) {
  return Math.max(0, clip.source_offset + timelineTime - clip.start_time) / 1000;
}

function describeLoadError(element: HTMLMediaElement, sceneTitle: string) {
  switch (element.error?.code) {
    case MediaError.MEDIA_ERR_NETWORK:
      return `The local media file for ${sceneTitle} is missing or could not be read through the Tauri asset protocol.`;
    case MediaError.MEDIA_ERR_DECODE:
      return `The local media file for ${sceneTitle} could not be decoded.`;
    case MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED:
      return `The local media format or converted asset URL for ${sceneTitle} is not supported.`;
    default:
      return `Could not load local media for ${sceneTitle}.`;
  }
}

export function TimelinePreview({
  clips,
  mediaByVersion,
  currentTime,
  duration,
  playing,
  onTimeChange,
  onPlayingChange,
  onError,
}: TimelinePreviewProps) {
  const elementsRef = useRef(new Map<string, HTMLMediaElement>());
  const currentTimeRef = useRef(currentTime);
  const durationRef = useRef(duration);
  const onTimeChangeRef = useRef(onTimeChange);
  const onPlayingChangeRef = useRef(onPlayingChange);
  const animationFrameRef = useRef<number | null>(null);
  const playPendingRef = useRef(new Set<string>());
  const playingRef = useRef(playing);

  currentTimeRef.current = currentTime;
  durationRef.current = duration;
  onTimeChangeRef.current = onTimeChange;
  onPlayingChangeRef.current = onPlayingChange;
  playingRef.current = playing;

  const videoClips = useMemo(() => clips.filter((clip) => clip.track === "VIDEO"), [clips]);
  const audioClips = useMemo(() => clips.filter((clip) => clip.track === "AUDIO"), [clips]);
  const activeVideo = videoClips.find((clip) => isActive(clip, currentTime)) ?? null;
  const sources = useMemo(() => new Map<string, { url: string | null; error: string | null }>([...mediaByVersion].map(([versionId, media]) => {
    try {
      return [versionId, { url: getAssetFileUrl(media.version.file_path), error: null }] as const;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return [versionId, { url: null, error: `Could not convert the local asset path for ${media.sceneTitle}: ${message}` }] as const;
    }
  })), [mediaByVersion]);

  useEffect(() => {
    const conversionError = [...sources.values()].find((source) => source.error)?.error;
    if (conversionError) onError(conversionError);
  }, [onError, sources]);

  const registerElement = useCallback((clipId: string, element: HTMLMediaElement | null) => {
    if (element) elementsRef.current.set(clipId, element);
    else elementsRef.current.delete(clipId);
  }, []);

  const synchronizeMedia = useCallback((timelineTime: number, shouldPlay: boolean) => {
    clips.forEach((clip) => {
      const element = elementsRef.current.get(clip.id);
      if (!element) return;
      if (!isActive(clip, timelineTime)) {
        if (!element.paused) element.pause();
        return;
      }

      const expectedTime = sourceTimeSeconds(clip, timelineTime);
      if (Number.isFinite(element.duration)) {
        const safeTime = Math.min(expectedTime, Math.max(0, element.duration - 0.01));
        if (Math.abs(element.currentTime - safeTime) > SYNC_TOLERANCE_SECONDS) {
          element.currentTime = safeTime;
        }
      } else if (Math.abs(element.currentTime - expectedTime) > SYNC_TOLERANCE_SECONDS) {
        element.currentTime = expectedTime;
      }

      if (shouldPlay && element.paused && element.readyState > 0 && !playPendingRef.current.has(clip.id)) {
        playPendingRef.current.add(clip.id);
        void element.play().catch((error: unknown) => {
          const mediaError = error instanceof DOMException ? error : null;
          if (mediaError?.name === "AbortError") return;
          onPlayingChangeRef.current(false);
          if (mediaError?.name === "NotAllowedError") {
            onError("Timeline playback was blocked by the WebView. Press Play again to allow audio playback.");
          } else if (mediaError?.name === "NotSupportedError") {
            onError("The local media file uses an unsupported or undecodable format.");
          } else {
            onError(`Timeline media playback failed${mediaError?.message ? `: ${mediaError.message}` : "."}`);
          }
        }).finally(() => {
          playPendingRef.current.delete(clip.id);
        });
      } else if (!shouldPlay && !element.paused) {
        element.pause();
      }
    });
  }, [clips, onError]);

  useEffect(() => {
    synchronizeMedia(currentTime, playing);
  }, [currentTime, playing, synchronizeMedia]);

  useEffect(() => {
    if (!playing) return;
    const startedAt = performance.now();
    const timelineStartedAt = currentTimeRef.current;

    const tick = (now: number) => {
      const nextTime = Math.min(durationRef.current, timelineStartedAt + now - startedAt);
      onTimeChangeRef.current(nextTime);
      synchronizeMedia(nextTime, true);
      if (nextTime >= durationRef.current) {
        onPlayingChangeRef.current(false);
        return;
      }
      animationFrameRef.current = requestAnimationFrame(tick);
    };

    animationFrameRef.current = requestAnimationFrame(tick);
    return () => {
      if (animationFrameRef.current !== null) cancelAnimationFrame(animationFrameRef.current);
      animationFrameRef.current = null;
    };
  }, [playing, synchronizeMedia]);

  useEffect(() => () => {
    elementsRef.current.forEach((element) => element.pause());
  }, []);

  return (
    <div className="timeline-preview-stage">
      <div className={`timeline-preview-gap${activeVideo ? " has-video" : ""}`} aria-hidden={Boolean(activeVideo)}>
        <span>NO VIDEO</span>
        <strong>Timeline gap</strong>
      </div>
      {videoClips.map((clip) => {
        const media = mediaByVersion.get(clip.asset_version_id);
        const source = sources.get(clip.asset_version_id);
        if (!media || !source?.url) return null;
        const active = clip.id === activeVideo?.id;
        return (
          <video
            key={clip.id}
            ref={(element) => registerElement(clip.id, element)}
            className={active ? "active" : ""}
            preload="auto"
            muted
            playsInline
            src={source.url}
            aria-label={`Timeline video: ${media.sceneTitle}`}
            onError={(event) => onError(describeLoadError(event.currentTarget, media.sceneTitle))}
            onCanPlay={() => synchronizeMedia(currentTimeRef.current, playingRef.current)}
          />
        );
      })}
      <div className="timeline-preview-audio-elements" aria-hidden="true">
        {audioClips.map((clip) => {
          const media = mediaByVersion.get(clip.asset_version_id);
          const source = sources.get(clip.asset_version_id);
          if (!media || !source?.url) return null;
          return (
            <audio
              key={clip.id}
              ref={(element) => registerElement(clip.id, element)}
              preload="auto"
              src={source.url}
              onError={(event) => onError(describeLoadError(event.currentTarget, media.sceneTitle))}
              onCanPlay={() => synchronizeMedia(currentTimeRef.current, playingRef.current)}
            />
          );
        })}
      </div>
      {playing ? <span className="timeline-preview-live">Timeline preview</span> : null}
    </div>
  );
}
