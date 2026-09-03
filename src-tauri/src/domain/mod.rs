//! Domain layer: entities and value types for Combain Studio.
//!
//! This module has no dependency on Tauri, the frontend, or any AI provider.
//! Persistence (Phase 3) and IPC (Phase 4) build on top of these types, so
//! most of this public API is unused until then.
#![allow(dead_code, unused_imports)]

pub mod asset;
pub mod asset_version;
pub mod audio_utils;
pub mod ids;
pub mod job;
pub mod project;
pub mod provider_profile;
pub mod scene;
pub mod timeline;
pub mod timeline_clip;

pub use asset::{Asset, AssetType};
pub use asset_version::AssetVersion;
pub use ids::{
    AssetId, AssetVersionId, JobId, ProjectId, ProviderProfileId, SceneId, TimelineClipId,
    TimelineId,
};
pub use job::{Job, JobStatus, JobType};
pub use project::Project;
pub use provider_profile::{ProviderProfile, ProviderPurpose};
pub use scene::{Scene, SceneStatus};
pub use timeline::Timeline;
pub use timeline_clip::{TimelineClip, Track};
