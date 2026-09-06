//! FFmpeg subprocess execution.
//!
//! Handles running FFmpeg commands as child processes, capturing output,
//! and handling errors.

use std::ffi::OsString;
use std::process::Command;

use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

use crate::error::AppError;

/// Result of FFmpeg execution.
#[derive(Debug)]
pub struct FFmpegOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Execute one of the two application-bundled media sidecars. The sidecar
/// name is fixed by backend code and is never supplied by the frontend.
pub fn execute_bundled_sidecar(
    app: &AppHandle,
    sidecar: &'static str,
    args: Vec<OsString>,
) -> Result<FFmpegOutput, AppError> {
    let command = app
        .shell()
        .sidecar(sidecar)
        .map_err(|error| {
            AppError::config(format!("Bundled {sidecar} runtime is unavailable: {error}"))
        })?
        .args(args);
    tracing::info!(sidecar, "executing bundled media runtime");
    let mut command: Command = command.into();
    let output = command.output().map_err(|error| {
        AppError::io(format!(
            "Bundled {sidecar} runtime could not be started: {error}"
        ))
    })?;
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(AppError::provider(format!(
            "Bundled {sidecar} failed with exit code {exit_code}: {}",
            extract_error_summary(&stderr)
        )));
    }
    Ok(FFmpegOutput {
        exit_code,
        stdout,
        stderr,
    })
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
        let cmd = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg("echo test output");
            command
        } else {
            let mut command = Command::new("echo");
            command.arg("test output");
            command
        };

        let output = execute_ffmpeg(cmd).expect("execute");
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("test output"));
    }

    #[test]
    fn execute_ffmpeg_returns_error_on_failure() {
        // Test with a command that will fail
        let cmd = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg("exit 1");
            command
        } else {
            let mut command = Command::new("sh");
            command.arg("-c").arg("exit 1");
            command
        };

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
