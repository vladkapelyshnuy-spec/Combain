//! FFmpeg output validation.
//!
//! Uses ffprobe to validate rendered MP4 files have the expected
//! video and audio streams.

use std::path::Path;
use std::process::Command;

use crate::error::AppError;

/// Validation result for rendered video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    pub has_video: bool,
    pub has_audio: bool,
    pub valid_mp4: bool,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.valid_mp4 && self.has_video
    }

    pub fn is_valid_with_audio(&self) -> bool {
        self.is_valid() && self.has_audio
    }
}

/// Validate MP4 output using ffprobe.
///
/// Checks:
/// - File exists and is readable
/// - File is a valid MP4 container
/// - File has at least one video stream
/// - File has at least one audio stream (optional)
pub fn validate_mp4_output(
    ffprobe_path: &Path,
    output_path: &Path,
) -> Result<ValidationResult, AppError> {
    if !output_path.exists() {
        return Err(AppError::io(format!(
            "output file does not exist: {}",
            output_path.display()
        )));
    }

    // Run ffprobe to detect streams
    let output = Command::new(ffprobe_path)
        .arg("-v")
        .arg("error")
        .arg("-show_streams")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg("-show_entries")
        .arg("stream=codec_type")
        .arg(output_path)
        .output()
        .map_err(|error| AppError::io(format!("failed to run ffprobe: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::provider(format!(
            "ffprobe failed to read output file: {}",
            stderr.lines().next().unwrap_or("unknown error")
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let has_video = stdout.lines().any(|line| line.trim() == "video");
    let has_audio = stdout.lines().any(|line| line.trim() == "audio");

    tracing::info!(
        output_path = %output_path.display(),
        has_video = has_video,
        has_audio = has_audio,
        "validated render output"
    );

    Ok(ValidationResult {
        has_video,
        has_audio,
        valid_mp4: true, // If ffprobe could read it, it's a valid container
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_result_is_valid_requires_video() {
        let result = ValidationResult {
            has_video: true,
            has_audio: false,
            valid_mp4: true,
        };
        assert!(result.is_valid());

        let result_no_video = ValidationResult {
            has_video: false,
            has_audio: true,
            valid_mp4: true,
        };
        assert!(!result_no_video.is_valid());
    }

    #[test]
    fn validation_result_is_valid_with_audio_requires_both() {
        let result = ValidationResult {
            has_video: true,
            has_audio: true,
            valid_mp4: true,
        };
        assert!(result.is_valid_with_audio());

        let result_no_audio = ValidationResult {
            has_video: true,
            has_audio: false,
            valid_mp4: true,
        };
        assert!(!result_no_audio.is_valid_with_audio());
    }

    #[test]
    fn validate_mp4_output_returns_error_for_nonexistent_file() {
        let result = validate_mp4_output(
            Path::new("ffprobe"),
            Path::new("/tmp/nonexistent_file_xyz.mp4"),
        );
        assert!(result.is_err());
    }

    // Note: Full integration test with actual ffprobe would require:
    // 1. FFmpeg installed
    // 2. A real MP4 file to validate
    // These are tested in the full render integration test
}
