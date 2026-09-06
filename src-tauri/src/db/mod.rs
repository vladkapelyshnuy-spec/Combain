//! SQLite connection management and schema migrations.
//!
//! Repositories (see [`crate::repository`]) build on top of [`Database`].
//! This module knows nothing about domain types beyond passing IDs and
//! primitives back and forth; row <-> struct mapping lives in `repository`.

mod migrations;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::AppError;

/// Owns the SQLite connection for the application's data file.
///
/// Wrapped in a [`Mutex`] so it can be shared across Tauri's async command
/// threads (`rusqlite::Connection` is `Send` but not `Sync`).
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the database file at `path`, enable foreign keys,
    /// and apply any pending migrations.
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::io(format!("failed to create data directory: {error}"))
            })?;
        }

        let conn = Connection::open(path)
            .map_err(|error| AppError::db(format!("failed to open database: {error}")))?;
        Self::from_connection(conn)
    }

    /// Open an in-memory database. Used by tests and integration tests.
    pub fn open_in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory()
            .map_err(|error| AppError::db(format!("failed to open in-memory database: {error}")))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self, AppError> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|error| AppError::db(format!("failed to enable foreign keys: {error}")))?;

        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn migrate(&self) -> Result<(), AppError> {
        migrations::run(&self.conn())
    }
}

pub(crate) fn map_db_err(error: rusqlite::Error) -> AppError {
    AppError::db(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::Project;
    use crate::repository::project_repository;

    /// Matches the Phase 3 acceptance scenario: create project, close app,
    /// reopen app, project remains.
    #[test]
    fn project_survives_close_and_reopen() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("database.sqlite");

        let project = {
            let db = Database::open(&db_path).expect("open");
            let project = Project::new("My Video", "space", "cinematic").expect("valid project");
            project_repository::insert(&db.conn(), &project).expect("insert");
            project
        }; // `db` dropped here, simulating app close.

        let reopened = Database::open(&db_path).expect("reopen");
        let loaded = project_repository::get_by_id(&reopened.conn(), project.id)
            .expect("query")
            .expect("project present after reopen");

        assert_eq!(loaded, project);
    }

    #[test]
    fn migrations_are_idempotent_across_reopen() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("database.sqlite");

        Database::open(&db_path).expect("first open");
        Database::open(&db_path).expect("second open should not fail");
    }
}
