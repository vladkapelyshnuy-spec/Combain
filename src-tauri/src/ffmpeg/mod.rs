//! FFmpeg integration for video rendering.
//!
//! Provides binary detection, command building, execution, and output validation
//! for rendering timelines into final MP4 videos.

pub mod command_builder;
pub mod executor;
pub mod validator;

/// Fixed sidecar identifiers declared in `tauri.conf.json`. They are not
/// user-controlled executable paths and never fall back to the system PATH.
pub const FFMPEG_SIDECAR: &str = "ffmpeg";
pub const FFPROBE_SIDECAR: &str = "ffprobe";
