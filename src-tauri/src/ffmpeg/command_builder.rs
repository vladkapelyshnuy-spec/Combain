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
    /// Start offset in milliseconds inside the source file.
    pub source_offset_ms: u64,
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
    let filter_complex = build_filter_complex(config)?;
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

/// Build a global-timeline filter graph.
///
/// Video clips are ordered by timeline position and concatenated with generated
/// black segments for gaps. VIDEO overlap is rejected because MVP compositing
/// semantics are undefined. Audio clips are source-trimmed, delayed to their
/// global start time, and mixed, so AUDIO overlap is supported.
fn build_filter_complex(config: &RenderConfig) -> Result<String, AppError> {
    if config.video_clips.is_empty() {
        return Err(AppError::invalid_input("no video clips for filter"));
    }

    let mut video_clips: Vec<_> = config.video_clips.iter().enumerate().collect();
    video_clips.sort_by_key(|(_, clip)| clip.start_ms);
    for pair in video_clips.windows(2) {
        let (_, left) = pair[0];
        let (_, right) = pair[1];
        if right.start_ms < left.start_ms.saturating_add(left.duration_ms) {
            return Err(AppError::invalid_input(
                "overlapping VIDEO clips are not supported by the MVP renderer",
            ));
        }
    }

    let video_end = video_clips
        .iter()
        .map(|(_, clip)| clip.start_ms.saturating_add(clip.duration_ms))
        .max()
        .unwrap_or(0);
    let audio_end = config
        .audio_clips
        .iter()
        .map(|clip| clip.start_ms.saturating_add(clip.duration_ms))
        .max()
        .unwrap_or(0);
    let timeline_end = video_end.max(audio_end);
    let mut filters = Vec::new();
    let mut video_segments = Vec::new();
    let mut cursor = 0;
    let mut gap_index = 0;

    for (output_index, (input_index, clip)) in video_clips.iter().enumerate() {
        if clip.start_ms > cursor {
            let gap_duration = (clip.start_ms - cursor) as f64 / 1000.0;
            let label = format!("vgap{gap_index}");
            filters.push(format!(
                "color=c=black:s={}x{}:r={}:d={gap_duration:.3}[{label}]",
                config.width, config.height, config.fps
            ));
            video_segments.push(label);
            gap_index += 1;
        }

        let source_start = clip.source_offset_ms as f64 / 1000.0;
        let duration = clip.duration_ms as f64 / 1000.0;
        let label = format!("v{output_index}");
        filters.push(format!(
            "[{input_index}:v]trim=start={source_start:.3}:duration={duration:.3},setpts=PTS-STARTPTS,scale={}:{},fps={},setsar=1[{label}]",
            config.width, config.height, config.fps
        ));
        video_segments.push(label);
        cursor = clip.start_ms.saturating_add(clip.duration_ms);
    }

    if timeline_end > cursor {
        let gap_duration = (timeline_end - cursor) as f64 / 1000.0;
        let label = format!("vgap{gap_index}");
        filters.push(format!(
            "color=c=black:s={}x{}:r={}:d={gap_duration:.3}[{label}]",
            config.width, config.height, config.fps
        ));
        video_segments.push(label);
    }

    let video_inputs = video_segments
        .iter()
        .map(|label| format!("[{label}]"))
        .collect::<String>();
    filters.push(format!(
        "{video_inputs}concat=n={}:v=1:a=0[vout]",
        video_segments.len()
    ));

    if !config.audio_clips.is_empty() {
        for (audio_index, clip) in config.audio_clips.iter().enumerate() {
            let input_index = config.video_clips.len() + audio_index;
            let source_start = clip.source_offset_ms as f64 / 1000.0;
            let duration = clip.duration_ms as f64 / 1000.0;
            filters.push(format!(
                "[{input_index}:a]atrim=start={source_start:.3}:duration={duration:.3},asetpts=PTS-STARTPTS,adelay={}:all=1[a{audio_index}]",
                clip.start_ms
            ));
        }
        let audio_inputs = (0..config.audio_clips.len())
            .map(|index| format!("[a{index}]"))
            .collect::<String>();
        filters.push(format!(
            "{audio_inputs}amix=inputs={}:duration=longest:dropout_transition=0,apad=whole_dur={:.3}[aout]",
            config.audio_clips.len(),
            timeline_end as f64 / 1000.0
        ));
    }

    Ok(filters.join(";"))
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
            source_offset_ms: 0,
        }];
        let filter = build_filter_complex(&RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            clips,
            vec![],
        ))
        .expect("single video");
        assert!(filter.contains("[0:v]trim=start=0.000:duration=5.500"));
        assert!(filter.contains("setpts=PTS-STARTPTS,scale="));
        assert!(filter.contains("[v0]concat=n=1:v=1:a=0[vout]"));
    }

    #[test]
    fn builds_filter_for_multiple_videos() {
        let clips = vec![
            ClipTiming {
                file_path: PathBuf::from("/tmp/video1.mp4"),
                start_ms: 0,
                duration_ms: 5000,
                source_offset_ms: 0,
            },
            ClipTiming {
                file_path: PathBuf::from("/tmp/video2.mp4"),
                start_ms: 5000,
                duration_ms: 3000,
                source_offset_ms: 0,
            },
        ];
        let filter = build_filter_complex(&RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            clips,
            vec![],
        ))
        .expect("multiple videos");
        assert!(filter.contains("[0:v]trim=start=0.000:duration=5.000"));
        assert!(filter.contains("[1:v]trim=start=0.000:duration=3.000"));
        assert!(filter.contains("[v0][v1]concat=n=2:v=1:a=0[vout]"));
    }

    #[test]
    fn builds_filter_for_videos_and_audios() {
        let video_clips = vec![
            ClipTiming {
                file_path: PathBuf::from("/tmp/video1.mp4"),
                start_ms: 0,
                duration_ms: 5000,
                source_offset_ms: 0,
            },
            ClipTiming {
                file_path: PathBuf::from("/tmp/video2.mp4"),
                start_ms: 5000,
                duration_ms: 3000,
                source_offset_ms: 0,
            },
        ];
        let audio_clips = vec![
            ClipTiming {
                file_path: PathBuf::from("/tmp/audio1.mp3"),
                start_ms: 0,
                duration_ms: 5000,
                source_offset_ms: 0,
            },
            ClipTiming {
                file_path: PathBuf::from("/tmp/audio2.mp3"),
                start_ms: 5000,
                duration_ms: 3000,
                source_offset_ms: 0,
            },
        ];
        let filter = build_filter_complex(&RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            video_clips,
            audio_clips,
        ))
        .expect("video and audio");
        assert!(filter.contains("[0:v]trim=start=0.000:duration=5.000"));
        assert!(filter.contains("[1:v]trim=start=0.000:duration=3.000"));
        assert!(filter.contains("[2:a]atrim=start=0.000:duration=5.000"));
        assert!(filter.contains("[3:a]atrim=start=0.000:duration=3.000"));
        assert!(filter.contains("[v0][v1]concat=n=2:v=1:a=0[vout]"));
        assert!(filter.contains("[a0][a1]amix=inputs=2"));
    }

    #[test]
    fn preserves_video_gap_and_global_duration() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            vec![
                ClipTiming {
                    file_path: PathBuf::from("/tmp/a.mp4"),
                    start_ms: 0,
                    duration_ms: 5000,
                    source_offset_ms: 0,
                },
                ClipTiming {
                    file_path: PathBuf::from("/tmp/b.mp4"),
                    start_ms: 10000,
                    duration_ms: 5000,
                    source_offset_ms: 0,
                },
            ],
            vec![],
        );
        let filter = build_filter_complex(&config).expect("gap timeline");
        assert!(filter.contains("color=c=black:s=1920x1080:r=30:d=5.000[vgap0]"));
        assert!(filter.contains("[v0][vgap0][v1]concat=n=3:v=1:a=0[vout]"));
    }

    #[test]
    fn delays_audio_to_global_start_time() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/video.mp4"),
                start_ms: 0,
                duration_ms: 10000,
                source_offset_ms: 0,
            }],
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/audio.mp3"),
                start_ms: 7000,
                duration_ms: 3000,
                source_offset_ms: 0,
            }],
        );
        let filter = build_filter_complex(&config).expect("delayed audio");
        assert!(filter.contains("adelay=7000:all=1[a0]"));
        assert!(filter.contains("amix=inputs=1:duration=longest"));
    }

    #[test]
    fn trims_from_source_offset() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/video.mp4"),
                start_ms: 0,
                duration_ms: 3000,
                source_offset_ms: 4000,
            }],
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/audio.mp3"),
                start_ms: 0,
                duration_ms: 3000,
                source_offset_ms: 4000,
            }],
        );
        let filter = build_filter_complex(&config).expect("source offset");
        assert!(filter.contains("[0:v]trim=start=4.000:duration=3.000"));
        assert!(filter.contains("[1:a]atrim=start=4.000:duration=3.000"));
    }

    #[test]
    fn rejects_overlapping_video_clips() {
        let config = RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            vec![
                ClipTiming {
                    file_path: PathBuf::from("/tmp/a.mp4"),
                    start_ms: 0,
                    duration_ms: 5000,
                    source_offset_ms: 0,
                },
                ClipTiming {
                    file_path: PathBuf::from("/tmp/b.mp4"),
                    start_ms: 4000,
                    duration_ms: 5000,
                    source_offset_ms: 0,
                },
            ],
            vec![],
        );
        let error = build_filter_complex(&config).expect_err("video overlap");
        assert!(error.to_string().contains("overlapping VIDEO clips"));
    }

    #[test]
    fn rejects_zero_video_inputs() {
        let error = build_filter_complex(&RenderConfig::new(
            PathBuf::from("/tmp/out.mp4"),
            vec![],
            vec![],
        ))
        .expect_err("zero videos");
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
                    source_offset_ms: 0,
                },
                ClipTiming {
                    file_path: PathBuf::from("/tmp/video2.mp4"),
                    start_ms: 5000,
                    duration_ms: 3000,
                    source_offset_ms: 0,
                },
            ],
            vec![ClipTiming {
                file_path: PathBuf::from("/tmp/audio1.mp3"),
                start_ms: 0,
                duration_ms: 8000,
                source_offset_ms: 0,
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
                source_offset_ms: 0,
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
