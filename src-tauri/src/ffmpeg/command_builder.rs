//! FFmpeg command builder for video rendering.
//!
//! Constructs complex FFmpeg commands with filter graphs for concatenating
//! video and audio streams from timeline clips, respecting clip timing (start_time, duration).

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Clip timing information for FFmpeg rendering.
#[derive(Debug, Clone)]
pub struct ClipTiming {
    pub file_path: PathBuf,
    /// Start time in milliseconds (clip position on timeline).
    pub start_ms: u64,
    /// Duration in milliseconds (how long the clip plays).
    pub duration_ms: u64,
}

/// Configuration for FFmpeg render output.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub output_path: PathBuf,
    pub video_clips: Vec<ClipTiming>,
    pub audio_clips: Vec<ClipTiming>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl RenderConfig {
    pub fn new(
        output_path: PathBuf,
        video_clips: Vec<ClipTiming>,
        audio_clips: Vec<ClipTiming>,
    ) -> Self {
        Self {
            output_path,
            video_clips,
            audio_clips,
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
/// - Complex filter graph with trim/setpts for each clip
/// - Concatenation of trimmed streams
/// - H.264 video codec
/// - AAC audio codec
/// - Fixed resolution and frame rate
pub fn build_render_command(
    ffmpeg_path: &Path,
    config: &RenderConfig,
) -> Result<std::process::Command, AppError> {
    if config.video_clips.is_empty() {
        return Err(AppError::invalid_input(
            "cannot render timeline with no video clips",
        ));
    }

    let mut cmd = std::process::Command::new(ffmpeg_path);
    cmd.arg("-y"); // Overwrite output (we already checked it doesn't exist)

    // Add all input files
    for video_clip in &config.video_clips {
        cmd.arg("-i").arg(&video_clip.file_path);
    }
    for audio_clip in &config.audio_clips {
        cmd.arg("-i").arg(&audio_clip.file_path);
    }

    // Build filter complex with timing
    let filter_complex = build_filter_complex(&config.video_clips, &config.audio_clips)?;
    cmd.arg("-filter_complex").arg(filter_complex);

    // Map outputs from filter
    cmd.arg("-map").arg("[vout]");

    if !config.audio_clips.is_empty() {
        cmd.arg("-map").arg("[aout]");
    }

    // Video codec settings
    cmd.arg("-c:v").arg("libx264");
    cmd.arg("-preset").arg("medium");
    cmd.arg("-crf").arg("23");

    // Audio codec settings (if we have audio)
    if !config.audio_clips.is_empty() {
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

/// Build filter_complex string for concatenating streams with timing.
///
/// For each video input, apply trim (to extract the desired duration) and setpts (to reset timestamps).
/// Then concat all trimmed streams.
///
/// Example for 2 video clips:
/// [0:v]trim=duration=5.5,setpts=PTS-STARTPTS[v0];
/// [1:v]trim=duration=3.2,setpts=PTS-STARTPTS[v1];
/// [v0][v1]concat=n=2:v=1:a=0[vout]
///
/// Same for audio with atrim and asetpts.
fn build_filter_complex(
    video_clips: &[ClipTiming],
    audio_clips: &[ClipTiming],
) -> Result<String, AppError> {
    if video_clips.is_empty() {
        return Err(AppError::invalid_input("no video clips for filter"));
    }

    let mut filter = String::new();

    // Video trim + concat filter
    for (i, clip) in video_clips.iter().enumerate() {
        let duration_sec = clip.duration_ms as f64 / 1000.0;
        filter.push_str(&format!(
            "[{}:v]trim=duration={:.3},setpts=PTS-STARTPTS[v{}]",
            i, duration_sec, i
        ));
        if i < video_clips.len() - 1 || !audio_clips.is_empty() {
            filter.push(';');
        }
    }

    // Concat video streams
    for i in 0..video_clips.len() {
        filter.push_str(&format!("[v{}]", i));
    }
    filter.push_str(&format!("concat=n={}:v=1:a=0[vout]", video_clips.len()));

    // Audio trim + concat filter (if we have audio)
    if !audio_clips.is_empty() {
        filter.push(';');

        for (i, clip) in audio_clips.iter().enumerate() {
            let duration_sec = clip.duration_ms as f64 / 1000.0;
            let input_idx = video_clips.len() + i;
            filter.push_str(&format!(
                "[{}:a]atrim=duration={:.3},asetpts=PTS-STARTPTS[a{}]",
                input_idx, duration_sec, i
            ));
            if i < audio_clips.len() - 1 {
                filter.push(';');
            }
        }

        filter.push(';');
        for i in 0..audio_clips.len() {
            filter.push_str(&format!("[a{}]", i));
        }
        filter.push_str(&format!("concat=n={}:v=0:a=1[aout]", audio_clips.len()));
    }

    Ok(filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_filter_for_single_video() {
        let clips = vec![ClipTiming {
            file_path: PathBuf::from("/tmp/video1.mp4"),
            start_ms: 0,
            duration_ms: 5500,
        }];
        let filter = build_filter_complex(&clips, &[]).expect("single video");
        assert!(filter.contains("[0:v]trim=duration=5.500"));
        assert!(filter.contains("setpts=PTS-STARTPTS[v0]"));
        assert!(filter.contains("[v0]concat=n=1:v=1:a=0[vout]"));
    }

    #[test]
    fn builds_filter_for_multiple_videos() {
        let clips = vec![
            ClipTiming {
                file_path: PathBuf::from("/tmp/video1.mp4"),
                start_ms: 0,
                duration_ms: 5000,
            },
            ClipTiming {
                file_path: PathBuf::from("/tmp/video2.mp4"),
                start_ms: 5000,
                duration_ms: 3000,
            },
        ];
        let filter = build_filter_complex(&clips, &[]).expect("multiple videos");
        assert!(filter.contains("[0:v]trim=duration=5.000"));
        assert!(filter.contains("[1:v]trim=duration=3.000"));
        assert!(filter.contains("[v0][v1]concat=n=2:v=1:a=0[vout]"));
    }

    #[test]
    fn builds_filter_for_videos_and_audios() {
        let video_clips = vec![
            ClipTiming {
                file_path: PathBuf::from("/tmp/video1.mp4"),
                start_ms: 0,
                duration_ms: 5000,
            },
            ClipTiming {
                file_path: PathBuf::from("/tmp/video2.mp4"),
                start_ms: 5000,
                duration_ms: 3000,
            },
        ];
        let audio_clips = vec![
            ClipTiming {
                file_path: PathBuf::from("/tmp/audio1.mp3"),
                start_ms: 0,
                duration_ms: 5000,
            },
            ClipTiming {
                file_path: PathBuf::from("/tmp/audio2.mp3"),
                start_ms: 5000,
                duration_ms: 3000,
            },
        ];
        let filter = build_filter_complex(&video_clips, &audio_clips).expect("video and audio");
        assert!(filter.contains("[0:v]trim=duration=5.000"));
        assert!(filter.contains("[1:v]trim=duration=3.000"));
        assert!(filter.contains("[2:a]atrim=duration=5.000"));
        assert!(filter.contains("[3:a]atrim=duration=3.000"));
        assert!(filter.contains("[v0][v1]concat=n=2:v=1:a=0[vout]"));
        assert!(filter.contains("[a0][a1]concat=n=2:v=0:a=1[aout]"));
    }

    #[test]
    fn rejects_zero_video_inputs() {
        let error = build_filter_complex(&[], &[]).expect_err("zero videos");
        assert_eq!(error.code(), crate::error::ErrorCode::InvalidInput);
    }

    #[test]
    fn builds_command_with_correct_args() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/output.mp4"),
            vec![
                ClipTiming {
                    file_path: PathBuf::from("/tmp/video1.mp4"),
                    start_ms: 0,
                    duration_ms: 5000,
                },
                ClipTiming {
                    file_path: PathBuf::from("/tmp/video2.mp4"),
                    start_ms: 5000,
                    duration_ms: 3000,
                },
            ],
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/audio1.mp3"),
                start_ms: 0,
                duration_ms: 8000,
            }],
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
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/video1.mp4"),
                start_ms: 0,
                duration_ms: 5000,
            }],
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
