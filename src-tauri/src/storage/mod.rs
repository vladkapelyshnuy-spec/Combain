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
}
