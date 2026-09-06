//! FFmpeg command builder for video rendering.
//!
//! Constructs complex FFmpeg commands with filter graphs for concatenating
//! video and audio streams from timeline clips.

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Configuration for FFmpeg render output.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub output_path: PathBuf,
    pub video_inputs: Vec<PathBuf>,
    pub audio_inputs: Vec<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl RenderConfig {
    pub fn new(
        output_path: PathBuf,
        video_inputs: Vec<PathBuf>,
        audio_inputs: Vec<PathBuf>,
    ) -> Self {
        Self {
            output_path,
            video_inputs,
            audio_inputs,
            width: 1920,
            height: 1080,
            fps: 30,
        }
    }
}

/// Build FFmpeg command for rendering.
///
/// Constructs a command with:
/// - Multiple input files (videos and audios)
/// - Complex filter graph for concatenation
/// - H.264 video codec
/// - AAC audio codec
/// - Fixed resolution and frame rate
pub fn build_render_command(
    ffmpeg_path: &Path,
    config: &RenderConfig,
) -> Result<std::process::Command, AppError> {
    if config.video_inputs.is_empty() {
        return Err(AppError::invalid_input(
            "cannot render timeline with no video clips",
        ));
    }

    let mut cmd = std::process::Command::new(ffmpeg_path);
    cmd.arg("-y"); // Overwrite output (we already checked it doesn't exist)

    // Add all input files
    for video_input in &config.video_inputs {
        cmd.arg("-i").arg(video_input);
    }
    for audio_input in &config.audio_inputs {
        cmd.arg("-i").arg(audio_input);
    }

    // Build filter complex
    let filter_complex =
        build_filter_complex(config.video_inputs.len(), config.audio_inputs.len())?;
    cmd.arg("-filter_complex").arg(filter_complex);

    // Map outputs from filter
    cmd.arg("-map").arg("[vout]");

    if !config.audio_inputs.is_empty() {
        cmd.arg("-map").arg("[aout]");
    }

    // Video codec settings
    cmd.arg("-c:v").arg("libx264");
    cmd.arg("-preset").arg("medium");
    cmd.arg("-crf").arg("23");

    // Audio codec settings (if we have audio)
    if !config.audio_inputs.is_empty() {
        cmd.arg("-c:a").arg("aac");
        cmd.arg("-b:a").arg("192k");
    }

    // Resolution and frame rate
    cmd.arg("-r").arg(config.fps.to_string());
    cmd.arg("-s")
        .arg(format!("{}x{}", config.width, config.height));

    // Output file
    cmd.arg(&config.output_path);

    Ok(cmd)
}

/// Build filter_complex string for concatenating streams.
///
/// Video concat: [0:v][1:v]...[N:v]concat=n=N:v=1:a=0[vout]
/// Audio concat: [N+1:a][N+2:a]...concat=n=M:v=0:a=1[aout]
fn build_filter_complex(video_count: usize, audio_count: usize) -> Result<String, AppError> {
    if video_count == 0 {
        return Err(AppError::invalid_input("no video inputs for filter"));
    }

    let mut filter = String::new();

    // Video concat filter
    for i in 0..video_count {
        filter.push_str(&format!("[{}:v]", i));
    }
    filter.push_str(&format!("concat=n={}:v=1:a=0[vout]", video_count));

    // Audio concat filter (if we have audio)
    if audio_count > 0 {
        filter.push(';');
        for i in 0..audio_count {
            let input_idx = video_count + i;
            filter.push_str(&format!("[{}:a]", input_idx));
        }
        filter.push_str(&format!("concat=n={}:v=0:a=1[aout]", audio_count));
    }

    Ok(filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_filter_for_single_video() {
        let filter = build_filter_complex(1, 0).expect("single video");
        assert_eq!(filter, "[0:v]concat=n=1:v=1:a=0[vout]");
    }

    #[test]
    fn builds_filter_for_multiple_videos() {
        let filter = build_filter_complex(3, 0).expect("multiple videos");
        assert_eq!(filter, "[0:v][1:v][2:v]concat=n=3:v=1:a=0[vout]");
    }

    #[test]
    fn builds_filter_for_videos_and_audios() {
        let filter = build_filter_complex(2, 2).expect("video and audio");
        assert_eq!(
            filter,
            "[0:v][1:v]concat=n=2:v=1:a=0[vout];[2:a][3:a]concat=n=2:v=0:a=1[aout]"
        );
    }

    #[test]
    fn rejects_zero_video_inputs() {
        let error = build_filter_complex(0, 1).expect_err("zero videos");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn builds_command_with_correct_args() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/output.mp4"),
            vec![
                PathBuf::from("/tmp/video1.mp4"),
                PathBuf::from("/tmp/video2.mp4"),
            ],
            vec![PathBuf::from("/tmp/audio1.mp3")],
        );

        let cmd = build_render_command(Path::new("ffmpeg"), &config).expect("build command");
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/tmp/video1.mp4".to_string()));
        assert!(args.contains(&"/tmp/video2.mp4".to_string()));
        assert!(args.contains(&"/tmp/audio1.mp3".to_string()));
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"-map".to_string()));
        assert!(args.contains(&"[vout]".to_string()));
        assert!(args.contains(&"[aout]".to_string()));
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"-c:a".to_string()));
        assert!(args.contains(&"aac".to_string()));
        assert!(args.contains(&"/tmp/output.mp4".to_string()));
    }

    #[test]
    fn command_omits_audio_map_when_no_audio() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/output.mp4"),
            vec![PathBuf::from("/tmp/video1.mp4")],
            vec![], // No audio
        );

        let cmd = build_render_command(Path::new("ffmpeg"), &config).expect("build command");
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"[vout]".to_string()));
        assert!(!args.contains(&"[aout]".to_string()));
        assert!(!args.contains(&"aac".to_string()));
    }
}
