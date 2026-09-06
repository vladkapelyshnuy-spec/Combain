//! Audio duration utilities for TTS and video generation.
//!
//! Phase 7 introduced `estimate_audio_duration_ms()` as a temporary mock.
//! Phase 8 adds `extract_audio_duration_ms()` to read real duration from
//! generated audio files using FFprobe.

use std::{ffi::OsString, path::Path};

use tauri::AppHandle;

use crate::error::AppError;
use crate::ffmpeg::{executor::execute_bundled_sidecar, FFPROBE_SIDECAR};

/// Average speaking rate in words per second for typical narration.
const WORDS_PER_SECOND: f64 = 2.5;

/// Estimate audio duration in milliseconds from narration text.
///
/// Formula: `(word_count / 2.5 words_per_second) * 1000ms`
///
/// This is a fallback approximation used when real audio files are not
/// available yet or FFprobe is unavailable. Prefer `extract_audio_duration_ms()`
/// when working with actual audio files.
pub fn estimate_audio_duration_ms(narration: &str) -> u64 {
    let word_count = narration.split_whitespace().count() as f64;
    if word_count == 0.0 {
        return 0;
    }
    ((word_count / WORDS_PER_SECOND) * 1000.0) as u64
}

/// Extract real audio duration in milliseconds from an audio file using FFprobe.
///
/// Uses `tokio::process::Command` to avoid blocking the async executor.
/// Falls back to text-based estimation if FFprobe is unavailable or fails.
///
/// FFprobe output parsing is robust: only numeric lines are parsed, ignoring
/// warnings or other non-numeric stdout content.
pub async fn extract_audio_duration_ms(
    app: Option<&AppHandle>,
    file_path: &Path,
    fallback_text: &str,
) -> Result<u64, AppError> {
    let Some(app) = app else {
        return Ok(estimate_audio_duration_ms(fallback_text));
    };
    match try_ffprobe_duration(app, file_path) {
        Ok(duration) => {
            tracing::debug!(
                "extracted audio duration: {}ms from {}",
                duration,
                file_path.display()
            );
            Ok(duration)
        }
        Err(error) => {
            tracing::warn!(
                "FFprobe unavailable or failed ({}), falling back to text estimation",
                error
            );
            Ok(estimate_audio_duration_ms(fallback_text))
        }
    }
}

fn try_ffprobe_duration(app: &AppHandle, file_path: &Path) -> Result<u64, AppError> {
    let output = execute_bundled_sidecar(
        app,
        FFPROBE_SIDECAR,
        vec![
            OsString::from("-v"),
            OsString::from("error"),
            OsString::from("-show_entries"),
            OsString::from("format=duration"),
            OsString::from("-of"),
            OsString::from("default=noprint_wrappers=1:nokey=1"),
            file_path.as_os_str().to_owned(),
        ],
    )?;
    let stdout = output.stdout;

    // Robust parsing: only parse the first line that looks like a number
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Ok(duration_secs) = trimmed.parse::<f64>() {
            if duration_secs >= 0.0 {
                return Ok((duration_secs * 1000.0) as u64);
            }
        }
    }

    Err(AppError::io(
        "ffprobe output did not contain a valid duration",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_duration_for_typical_narration() {
        // 10 words at 2.5 words/sec = 4 seconds = 4000ms
        let text = "This is a typical narration with exactly ten words here.";
        let duration = estimate_audio_duration_ms(text);
        assert_eq!(duration, 4000);
    }

    #[test]
    fn handles_empty_string() {
        let duration = estimate_audio_duration_ms("");
        assert_eq!(duration, 0);
    }

    #[test]
    fn handles_single_word() {
        // 1 word at 2.5 words/sec = 0.4 sec = 400ms
        let duration = estimate_audio_duration_ms("Hello");
        assert_eq!(duration, 400);
    }

    #[test]
    fn handles_long_text() {
        // 25 words at 2.5 words/sec = 10 seconds = 10000ms
        let text = "a ".repeat(25);
        let duration = estimate_audio_duration_ms(&text);
        assert_eq!(duration, 10000);
    }

    #[tokio::test]
    async fn extract_falls_back_when_ffprobe_unavailable() {
        let nonexistent = Path::new("/nonexistent/audio.mp3");
        let fallback_text = "one two three four five";
        let duration = extract_audio_duration_ms(None, nonexistent, fallback_text)
            .await
            .expect("fallback should work");
        // 5 words / 2.5 = 2 sec = 2000ms
        assert_eq!(duration, 2000);
    }
}
