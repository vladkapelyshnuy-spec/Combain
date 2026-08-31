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

/// Check the PNG magic bytes. OpenAI's Images API always returns PNG.
pub fn looks_like_png(bytes: &[u8]) -> bool {
    const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.len() >= PNG_SIGNATURE.len() && bytes[..8] == PNG_SIGNATURE
}
