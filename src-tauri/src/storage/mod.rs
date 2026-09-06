//! Provider-independent file storage for generated assets.
//!
//! Knows how to place a version's file under the portable data root and
//! enforce the "never overwrite a previous version" rule. Knows nothing
//! about HTTP, providers, or the database.

use std::path::{Path, PathBuf};

use crate::domain::ids::{AssetId, ProjectId};
use crate::error::AppError;

/// Deterministic on-disk path for one asset version's file.
///
/// `<data_root>/projects/<project_id>/assets/<asset_id>/v<version>.<extension>`
pub fn asset_version_path(
    data_root: &Path,
    project_id: ProjectId,
    asset_id: AssetId,
    version: u32,
    extension: &str,
) -> PathBuf {
    data_root
        .join("projects")
        .join(project_id.to_string())
        .join("assets")
        .join(asset_id.to_string())
        .join(format!("v{version}.{extension}"))
}

/// Write `bytes` to `path`. Fails if `path` already exists: asset versions
/// are immutable once written, matching the domain's versioning rule.
pub fn save_new_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if path.exists() {
        return Err(AppError::conflict(format!(
            "file already exists at {}",
            path.display()
        )));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            AppError::io(format!("failed to create {}: {error}", parent.display()))
        })?;
    }

    std::fs::write(path, bytes)
        .map_err(|error| AppError::io(format!("failed to write {}: {error}", path.display())))?;
    Ok(())
}

/// Detect supported generated-image formats by their file signatures. Some
/// OpenAI-compatible providers return JPEG or WebP even when the response is
/// delivered through `b64_json`.
pub fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.starts_with(&PNG_SIGNATURE) {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

/// Check the MP4 magic bytes. Video providers should return MP4.
/// MP4 files begin with `ftyp` box signature at byte offset 4.
pub fn looks_like_mp4(bytes: &[u8]) -> bool {
    if bytes.len() < 12 {
        return false;
    }
    // MP4 starts with: [size (4 bytes)][ftyp (4 bytes)][brand (4+ bytes)]
    // We check for "ftyp" at offset 4
    &bytes[4..8] == b"ftyp"
}

/// Check for MP3 magic bytes. TTS providers typically return MP3.
/// MP3 files can start with:
/// - MPEG frame sync (0xFF 0xFB or 0xFF 0xF3 or other 0xFF 0xFx)
/// - ID3v2 tag (0x49 0x44 0x33 = "ID3")
pub fn looks_like_mp3(bytes: &[u8]) -> bool {
    if bytes.len() < 3 {
        return false;
    }
    // Check for ID3v2 tag
    if bytes.starts_with(b"ID3") {
        return true;
    }
    // Check for MPEG frame sync (0xFF followed by 0xFx or 0xEx)
    if bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return true;
    }
    false
}

/// Deterministic on-disk path for renders directory for a project.
///
/// `<data_root>/projects/<project_id>/renders/`
pub fn renders_dir(data_root: &Path, project_id: ProjectId) -> PathBuf {
    data_root
        .join("projects")
        .join(project_id.to_string())
        .join("renders")
}

/// Construct full path for a render output file.
///
/// `<data_root>/projects/<project_id>/renders/<output_name>.mp4`
pub fn render_output_path(data_root: &Path, project_id: ProjectId, output_name: &str) -> PathBuf {
    renders_dir(data_root, project_id).join(format!("{output_name}.mp4"))
}

/// Check if render output file already exists.
pub fn render_output_exists(data_root: &Path, project_id: ProjectId, output_name: &str) -> bool {
    render_output_path(data_root, project_id, output_name).exists()
}

/// Create renders directory if it doesn't exist.
pub fn ensure_renders_dir(data_root: &Path, project_id: ProjectId) -> Result<PathBuf, AppError> {
    let dir = renders_dir(data_root, project_id);
    std::fs::create_dir_all(&dir)
        .map_err(|error| AppError::io(format!("failed to create {}: {error}", dir.display())))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_image_formats() {
        assert_eq!(
            image_extension(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
            Some("png")
        );
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"RIFF\x04\0\0\0WEBPdata"), Some("webp"));
        assert_eq!(image_extension(b"not an image"), None);
    }

    #[test]
    fn recognizes_valid_mp4() {
        let mp4: &[u8] = &[
            0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm',
        ];
        assert!(looks_like_mp4(mp4));
    }

    #[test]
    fn rejects_invalid_mp4() {
        assert!(!looks_like_mp4(b"not a video"));
    }

    #[test]
    fn rejects_short_buffer() {
        assert!(!looks_like_mp4(&[0x00, 0x00]));
    }

    #[test]
    fn recognizes_valid_mp3_with_id3_tag() {
        let mp3_id3: &[u8] = b"ID3\x03\x00\x00\x00\x00\x00\x00";
        assert!(looks_like_mp3(mp3_id3));
    }

    #[test]
    fn recognizes_valid_mp3_with_frame_sync() {
        let mp3_frame: &[u8] = &[0xFF, 0xFB, 0x90, 0x44];
        assert!(looks_like_mp3(mp3_frame));
        let mp3_frame2: &[u8] = &[0xFF, 0xF3, 0x44, 0xC0];
        assert!(looks_like_mp3(mp3_frame2));
    }

    #[test]
    fn rejects_invalid_mp3() {
        assert!(!looks_like_mp3(b"not an audio file"));
        assert!(!looks_like_mp3(&[0xFF, 0x00])); // invalid frame sync
        assert!(!looks_like_mp3(&[0x00, 0xFB])); // missing 0xFF
    }

    #[test]
    fn rejects_short_buffer_mp3() {
        assert!(!looks_like_mp3(&[0xFF]));
        assert!(!looks_like_mp3(&[]));
    }

    #[test]
    fn renders_dir_path_is_correct() {
        let data_root = Path::new("/data");
        let project_id = ProjectId::new();
        let path = renders_dir(data_root, project_id);
        assert_eq!(
            path,
            data_root
                .join("projects")
                .join(project_id.to_string())
                .join("renders")
        );
    }

    #[test]
    fn render_output_path_includes_mp4_extension() {
        let data_root = Path::new("/data");
        let project_id = ProjectId::new();
        let path = render_output_path(data_root, project_id, "final_video");
        assert!(path.to_string_lossy().ends_with("final_video.mp4"));
    }

    #[test]
    fn render_output_exists_returns_false_for_nonexistent() {
        let temp_dir = std::env::temp_dir();
        let project_id = ProjectId::new();
        assert!(!render_output_exists(&temp_dir, project_id, "nonexistent"));
    }

    #[test]
    fn ensure_renders_dir_creates_directory() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let project_id = ProjectId::new();

        let dir = ensure_renders_dir(temp_dir.path(), project_id).expect("create dir");
        assert!(dir.exists());
        assert!(dir.is_dir());
    }
}
