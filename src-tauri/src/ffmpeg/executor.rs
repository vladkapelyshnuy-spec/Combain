//! FFmpeg subprocess execution.
//!
//! Handles running FFmpeg commands as child processes, capturing output,
//! and handling errors.

use std::process::Command;

use crate::error::AppError;

/// Result of FFmpeg execution.
#[derive(Debug)]
pub struct FFmpegOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Execute FFmpeg command and capture output.
///
/// Runs the command synchronously, capturing stdout and stderr.
/// Returns error if FFmpeg exits with non-zero status.
pub fn execute_ffmpeg(mut cmd: Command) -> Result<FFmpegOutput, AppError> {
    tracing::info!(command = ?cmd, "executing FFmpeg");

    let output = cmd
        .output()
        .map_err(|error| AppError::io(format!("failed to execute FFmpeg: {error}")))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Log FFmpeg output
    if !stderr.is_empty() {
        for line in stderr.lines().take(20) {
            tracing::debug!(ffmpeg_stderr = %line);
        }
    }

    if !output.status.success() {
        // Extract error summary from stderr
        let error_summary = extract_error_summary(&stderr);
        return Err(AppError::provider(format!(
            "FFmpeg failed with exit code {}: {}",
            exit_code, error_summary
        )));
    }

    tracing::info!(exit_code = exit_code, "FFmpeg completed successfully");

    Ok(FFmpegOutput {
        exit_code,
        stdout,
        stderr,
    })
}

/// Extract a concise error message from FFmpeg stderr.
///
/// FFmpeg stderr can be verbose; this extracts the most relevant error line.
fn extract_error_summary(stderr: &str) -> String {
    // Look for common error patterns (search from end, but check multiple patterns)
    for line in stderr.lines().rev() {
        let line = line.trim();
        // Prefer detailed error messages over generic ones
        if line.contains("Error") && !line.starts_with("Conversion failed") {
            return line.to_string();
        }
        if line.contains("error") && !line.starts_with("Conversion failed") {
            return line.to_string();
        }
    }

    // Fallback to "Conversion failed" or last non-empty line
    for line in stderr.lines().rev() {
        let line = line.trim();
        if line.starts_with("Conversion failed") {
            return line.to_string();
        }
        if !line.is_empty() {
            return line.to_string();
        }
    }

    "unknown FFmpeg error".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_error_summary_finds_error_line() {
        let stderr = "FFmpeg version...\nConfiguration...\nError while opening encoder for output stream #0:0\nConversion failed!";
        let summary = extract_error_summary(stderr);
        assert!(summary.contains("Error while opening encoder"));
    }

    #[test]
    fn extract_error_summary_falls_back_to_last_line() {
        let stderr = "line 1\nline 2\nlast line";
        let summary = extract_error_summary(stderr);
        assert_eq!(summary, "last line");
    }

    #[test]
    fn execute_ffmpeg_captures_output() {
        // Test with a simple command that should succeed
        let mut cmd = Command::new("echo");
        cmd.arg("test output");

        let output = execute_ffmpeg(cmd).expect("execute");
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("test output"));
    }

    #[test]
    fn execute_ffmpeg_returns_error_on_failure() {
        // Test with a command that will fail
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("exit 1");

        let error = execute_ffmpeg(cmd).expect_err("should fail");
        assert_eq!(error.code(), crate::error::ErrorCode::Provider);
    }

    #[test]
    fn execute_ffmpeg_handles_missing_binary() {
        let mut cmd = Command::new("nonexistent_binary_xyz");
        let error = execute_ffmpeg(cmd).expect_err("should fail");
        assert_eq!(error.code(), crate::error::ErrorCode::Io);
    }
}
