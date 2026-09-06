//! FFmpeg integration for video rendering.
//!
//! Provides binary detection, command building, execution, and output validation
//! for rendering timelines into final MP4 videos.

pub mod command_builder;
pub mod executor;
pub mod validator;

use std::path::PathBuf;
use std::process::Command;

use crate::error::AppError;

/// FFmpeg binary paths detected at runtime.
#[derive(Debug, Clone)]
pub struct FFmpegBinaries {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

/// Detect FFmpeg and ffprobe binaries in PATH.
/// Returns error if either binary is not found.
pub fn detect_binaries() -> Result<FFmpegBinaries, AppError> {
    let ffmpeg = find_binary("ffmpeg")?;
    let ffprobe = find_binary("ffprobe")?;

    tracing::info!(
        ffmpeg = %ffmpeg.display(),
        ffprobe = %ffprobe.display(),
        "FFmpeg binaries detected"
    );

    Ok(FFmpegBinaries { ffmpeg, ffprobe })
}

/// Find a binary in PATH by name.
/// On Windows, tries both with and without .exe extension.
fn find_binary(name: &str) -> Result<PathBuf, AppError> {
    // Try direct lookup first
    if let Ok(output) = Command::new(name).arg("-version").output() {
        if output.status.success() {
            tracing::debug!(binary = name, "found binary in PATH");
            return Ok(PathBuf::from(name));
        }
    }

    // On Windows, try with .exe extension
    #[cfg(target_os = "windows")]
    {
        let name_exe = format!("{}.exe", name);
        if let Ok(output) = Command::new(&name_exe).arg("-version").output() {
            if output.status.success() {
                tracing::debug!(binary = %name_exe, "found binary in PATH");
                return Ok(PathBuf::from(name_exe));
            }
        }
    }

    Err(AppError::config(format!(
        "{} not found in PATH. Please install FFmpeg: https://ffmpeg.org/download.html",
        name
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_binaries_finds_ffmpeg_if_installed() {
        // This test will pass if FFmpeg is installed, skip otherwise
        match detect_binaries() {
            Ok(binaries) => {
                assert!(binaries.ffmpeg.to_string_lossy().contains("ffmpeg"));
                assert!(binaries.ffprobe.to_string_lossy().contains("ffprobe"));
            }
            Err(e) => {
                // Skip test if FFmpeg not installed
                eprintln!("FFmpeg not installed (expected in dev): {}", e);
            }
        }
    }

    #[test]
    fn find_binary_returns_error_for_nonexistent() {
        let result = find_binary("nonexistent_binary_xyz");
        assert!(result.is_err());
    }
}
