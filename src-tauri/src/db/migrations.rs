//! Migration runner. Each migration is a `(id, sql)` pair; applied migrations
//! are tracked in `schema_migrations` so re-running `migrate()` is a no-op.

use rusqlite::Connection;

use super::map_db_err;
use crate::error::AppError;

struct Migration {
    id: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        id: "0001_initial",
        sql: include_str!("../../migrations/0001_initial.sql"),
    },
    Migration {
        id: "0002_provider_profiles",
        sql: include_str!("../../migrations/0002_provider_profiles.sql"),
    },
    Migration {
        id: "0003_royaltechno_video_provider",
        sql: include_str!("../../migrations/0003_royaltechno_video_provider.sql"),
    },
    Migration {
        id: "0004_elevenlabs_tts_provider",
        sql: include_str!("../../migrations/0004_elevenlabs_tts_provider.sql"),
    },
    Migration {
        id: "0005_jobs",
        sql: include_str!("../../migrations/0005_jobs.sql"),
    },
    Migration {
        id: "0006_timeline_render_fixes",
        sql: include_str!("../../migrations/0006_timeline_render_fixes.sql"),
    },
    Migration {
        id: "0007_timeline_source_offset",
        sql: include_str!("../../migrations/0007_timeline_source_offset.sql"),
    },
];

pub fn run(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )
    .map_err(map_db_err)?;

    for migration in MIGRATIONS {
        let already_applied: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE id = ?1)",
                [migration.id],
                |row| row.get(0),
            )
            .map_err(map_db_err)?;

        if already_applied {
            continue;
        }

        let tx = conn.unchecked_transaction().map_err(map_db_err)?;
        tx.execute_batch(migration.sql).map_err(map_db_err)?;
        tx.execute(
            "INSERT INTO schema_migrations (id) VALUES (?1)",
            [migration.id],
        )
        .map_err(map_db_err)?;
        tx.commit().map_err(map_db_err)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_migrations_and_creates_tables() {
        let conn = Connection::open_in_memory().expect("open");
        run(&conn).expect("migrate");

        let table_exists = |name: &str| -> bool {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [name],
                |row| row.get::<_, bool>(0),
            )
            .expect("query")
        };

        for table in [
            "projects",
            "scenes",
            "assets",
            "asset_versions",
            "timelines",
            "timeline_clips",
            "jobs",
            "provider_profiles",
        ] {
            assert!(table_exists(table), "missing table {table}");
        }

        let source_offset_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('timeline_clips') WHERE name = 'source_offset')",
                [],
                |row| row.get(0),
            )
            .expect("query source_offset column");
        assert!(source_offset_exists);
    }

    #[test]
    fn is_idempotent_on_repeated_calls() {
        let conn = Connection::open_in_memory().expect("open");
        run(&conn).expect("first migrate");
        run(&conn).expect("second migrate should be a no-op");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("query");
        assert_eq!(count, MIGRATIONS.len() as i64);
    }
}
