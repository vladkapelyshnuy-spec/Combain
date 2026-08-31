//! Portable filesystem layout: everything the app writes (database, generated
//! assets) lives under a `data/` directory next to the executable, so the
//! whole install can be copied/moved as one folder.

use std::path::PathBuf;

use crate::error::AppError;

/// Directory for all mutable application data: `<exe_dir>/data`.
///
/// Uses `current_exe()` rather than the process's current working directory,
/// so the location is stable regardless of how the app was launched.
pub fn portable_data_dir() -> Result<PathBuf, AppError> {
    let exe_path = std::env::current_exe()
        .map_err(|error| AppError::io(format!("failed to resolve executable path: {error}")))?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| AppError::io("executable path has no parent directory".to_string()))?;
    Ok(exe_dir.join("data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_data_dir_next_to_executable() {
        let dir = portable_data_dir().expect("portable data dir");
        let exe_dir = std::env::current_exe()
            .expect("current exe")
            .parent()
            .expect("parent")
            .to_path_buf();

        assert_eq!(dir, exe_dir.join("data"));
    }
}
