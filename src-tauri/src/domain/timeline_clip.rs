//! TimelineClip entity: a single placed asset reference on a timeline track.

use serde::{Deserialize, Serialize};

use crate::domain::ids::{AssetVersionId, TimelineClipId, TimelineId};
use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Track {
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineClip {
    pub id: TimelineClipId,
    pub timeline_id: TimelineId,
    pub asset_version_id: AssetVersionId,
    pub track: Track,
    /// Start offset in milliseconds, relative to the timeline origin.
    pub start_time: u64,
    /// Duration in milliseconds.
    pub duration: u64,
    /// Start offset in milliseconds inside the source media.
    pub source_offset: u64,
}

impl TimelineClip {
    pub fn new(
        timeline_id: TimelineId,
        asset_version_id: AssetVersionId,
        track: Track,
        start_time: u64,
        duration: u64,
    ) -> Result<Self, AppError> {
        if duration == 0 {
            return Err(AppError::invalid_input(
                "timeline clip duration must be greater than zero",
            ));
        }

        Ok(Self {
            id: TimelineClipId::new(),
            timeline_id,
            asset_version_id,
            track,
            start_time,
            duration,
            source_offset: 0,
        })
    }

    pub fn with_source_offset(mut self, source_offset: u64) -> Self {
        self.source_offset = source_offset;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_clip_on_video_track() {
        let clip = TimelineClip::new(
            TimelineId::new(),
            AssetVersionId::new(),
            Track::Video,
            0,
            5000,
        )
        .expect("valid clip");
        assert_eq!(clip.track, Track::Video);
        assert_eq!(clip.start_time, 0);
        assert_eq!(clip.source_offset, 0);
    }

    #[test]
    fn rejects_zero_duration() {
        let error = TimelineClip::new(TimelineId::new(), AssetVersionId::new(), Track::Audio, 0, 0)
            .expect_err("zero duration");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn serializes_and_deserializes() {
        let clip = TimelineClip::new(
            TimelineId::new(),
            AssetVersionId::new(),
            Track::Audio,
            1000,
            4000,
        )
        .expect("valid clip");
        let json = serde_json::to_string(&clip).expect("serialize");
        let back: TimelineClip = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(clip, back);
    }

    #[test]
    fn track_uses_screaming_snake_case() {
        let json = serde_json::to_string(&Track::Video).expect("serialize");
        assert_eq!(json, "\"VIDEO\"");
    }
}
