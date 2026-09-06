//! Test helper functions for integration tests.
//!
//! Provides utilities for:
//! - Setting up test environments (temp DB, directories)
//! - Creating minimal valid asset files (PNG, MP4, MP3)
//! - Waiting for async job completion
//! - Cleanup

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use combain_studio_lib::db::Database;
use combain_studio_lib::domain::{Job, JobId};
use combain_studio_lib::error::AppError;
use combain_studio_lib::repository::job_repository;

/// Test environment with database and temporary directories.
pub struct TestEnvironment {
    pub db: Database,
    pub data_root: PathBuf,
    _temp_dir: tempfile::TempDir, // Dropped on cleanup
}

impl TestEnvironment {
    /// Create a new test environment with in-memory DB and temp directory.
    pub fn new() -> Result<Self, AppError> {
        let db = Database::open_in_memory()?;
        let temp_dir = tempfile::tempdir()
            .map_err(|e| AppError::io(format!("failed to create temp dir: {}", e)))?;
        let data_root = temp_dir.path().to_path_buf();

        Ok(Self {
            db,
            data_root,
            _temp_dir: temp_dir,
        })
    }

    /// Get database reference.
    pub fn db(&self) -> &Database {
        &self.db
    }
}

/// Wait for a job to reach a terminal state (Completed, Failed, Cancelled).
///
/// Polls the job status every 100ms until it reaches a terminal state or timeout.
pub fn wait_for_job_completion(
    db: &Database,
    job_id: JobId,
    timeout_ms: u64,
) -> Result<Job, AppError> {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        let job = job_repository::get_by_id(&db.conn(), job_id)?
            .ok_or_else(|| AppError::invalid_input(format!("job {} not found", job_id)))?;

        if job.status.is_terminal() {
            return Ok(job);
        }

        if start.elapsed() > timeout {
            return Err(AppError::provider(format!(
                "job {} did not complete within {}ms (status: {:?})",
                job_id, timeout_ms, job.status
            )));
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Create a minimal valid PNG file (1x1 transparent pixel).
///
/// PNG signature + IHDR + IDAT + IEND chunks.
pub fn create_mock_png(path: &Path) -> Result<(), AppError> {
    // Minimal 1x1 transparent PNG (67 bytes)
    #[rustfmt::skip]
    let png_bytes: &[u8] = &[
        // PNG signature
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        // IHDR chunk (1x1, 8-bit RGBA)
        0x00, 0x00, 0x00, 0x0D, // chunk length
        0x49, 0x48, 0x44, 0x52, // "IHDR"
        0x00, 0x00, 0x00, 0x01, // width: 1
        0x00, 0x00, 0x00, 0x01, // height: 1
        0x08, 0x06, 0x00, 0x00, 0x00, // 8-bit RGBA
        0x1F, 0x15, 0xC4, 0x89, // CRC
        // IDAT chunk (image data)
        0x00, 0x00, 0x00, 0x0A, // chunk length
        0x49, 0x44, 0x41, 0x54, // "IDAT"
        0x08, 0xD7, 0x63, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01,
        0xE2, 0x21, 0xBC, 0x33, // CRC
        // IEND chunk
        0x00, 0x00, 0x00, 0x00, // chunk length
        0x49, 0x45, 0x4E, 0x44, // "IEND"
        0xAE, 0x42, 0x60, 0x82, // CRC
    ];

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("failed to create dir: {}", e)))?;
    }

    std::fs::write(path, png_bytes)
        .map_err(|e| AppError::io(format!("failed to write PNG: {}", e)))?;

    Ok(())
}

/// Create a minimal valid MP4 file (1 frame, 1 second, silent).
///
/// This is a minimal MP4 structure that FFmpeg can process.
/// Contains ftyp, moov, and mdat atoms.
pub fn create_mock_mp4(path: &Path) -> Result<(), AppError> {
    // Minimal valid MP4 file structure (~200 bytes)
    // ftyp box + minimal moov box + mdat box
    #[rustfmt::skip]
    let mp4_bytes: &[u8] = &[
        // ftyp box (file type)
        0x00, 0x00, 0x00, 0x20, // box size: 32 bytes
        0x66, 0x74, 0x79, 0x70, // "ftyp"
        0x69, 0x73, 0x6F, 0x6D, // major_brand: "isom"
        0x00, 0x00, 0x02, 0x00, // minor_version: 512
        0x69, 0x73, 0x6F, 0x6D, // compatible_brands: "isom"
        0x69, 0x73, 0x6F, 0x32, // "iso2"
        0x6D, 0x70, 0x34, 0x31, // "mp41"
        0x00, 0x00, 0x00, 0x00, // padding

        // moov box (movie metadata) - minimal structure
        0x00, 0x00, 0x00, 0x6C, // box size: 108 bytes
        0x6D, 0x6F, 0x6F, 0x76, // "moov"

        // mvhd box (movie header) inside moov
        0x00, 0x00, 0x00, 0x6C, // box size
        0x6D, 0x76, 0x68, 0x64, // "mvhd"
        0x00, 0x00, 0x00, 0x00, // version + flags
        0x00, 0x00, 0x00, 0x00, // creation_time
        0x00, 0x00, 0x00, 0x00, // modification_time
        0x00, 0x00, 0x03, 0xE8, // timescale: 1000
        0x00, 0x00, 0x03, 0xE8, // duration: 1000 (1 second)
        0x00, 0x01, 0x00, 0x00, // rate: 1.0
        0x01, 0x00, // volume: 1.0
        0x00, 0x00, // reserved
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // reserved
        // Matrix (unity matrix)
        0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
        // pre_defined
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x02, // next_track_ID: 2

        // mdat box (media data) - empty
        0x00, 0x00, 0x00, 0x08, // box size: 8 bytes (header only)
        0x6D, 0x64, 0x61, 0x74, // "mdat"
    ];

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("failed to create dir: {}", e)))?;
    }

    std::fs::write(path, mp4_bytes)
        .map_err(|e| AppError::io(format!("failed to write MP4: {}", e)))?;

    Ok(())
}

/// Create a minimal valid MP3 file (ID3v2 header + silent frame).
///
/// This is a minimal MP3 structure that audio players can recognize.
pub fn create_mock_mp3(path: &Path) -> Result<(), AppError> {
    // Minimal valid MP3 file with ID3v2 header
    #[rustfmt::skip]
    let mp3_bytes: &[u8] = &[
        // ID3v2 header
        0x49, 0x44, 0x33, // "ID3"
        0x03, 0x00, // version: 2.3.0
        0x00, // flags
        0x00, 0x00, 0x00, 0x00, // size (synchsafe int): 0

        // MPEG frame header (Layer III, 128kbps, 44.1kHz, mono)
        0xFF, 0xFB, // frame sync + version/layer
        0x90, // bitrate/samplerate
        0x00, // padding/private/channel

        // Frame data (minimal silent frame padding)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::io(format!("failed to create dir: {}", e)))?;
    }

    std::fs::write(path, mp3_bytes)
        .map_err(|e| AppError::io(format!("failed to write MP3: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_creates_temp_dir() {
        let env = TestEnvironment::new().expect("create env");
        assert!(env.data_root.exists());
        assert!(env.data_root.is_dir());
    }

    #[test]
    fn creates_valid_png() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let png_path = temp_dir.path().join("test.png");

        create_mock_png(&png_path).expect("create PNG");

        assert!(png_path.exists());
        let bytes = std::fs::read(&png_path).expect("read PNG");
        assert!(bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47])); // PNG signature
    }

    #[test]
    fn creates_valid_mp4() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let mp4_path = temp_dir.path().join("test.mp4");

        create_mock_mp4(&mp4_path).expect("create MP4");

        assert!(mp4_path.exists());
        let bytes = std::fs::read(&mp4_path).expect("read MP4");
        // Check for ftyp box signature at offset 4
        assert_eq!(&bytes[4..8], b"ftyp");
    }

    #[test]
    fn creates_valid_mp3() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let mp3_path = temp_dir.path().join("test.mp3");

        create_mock_mp3(&mp3_path).expect("create MP3");

        assert!(mp3_path.exists());
        let bytes = std::fs::read(&mp3_path).expect("read MP3");
        assert!(bytes.starts_with(b"ID3")); // ID3v2 header
    }
}
