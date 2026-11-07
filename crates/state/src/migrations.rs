use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::{StateError, StateResult};

/// The newest SQLite schema version understood by this crate.
pub const CURRENT_SCHEMA_VERSION: i64 = 1;

/// Apply all required SQLite schema migrations.
pub fn apply_migrations(conn: &Connection) -> StateResult<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at_ms INTEGER NOT NULL
        );
        "#,
    )?;

    let current = get_schema_version(conn)?;
    validate_supported_version(current)?;

    if current < 1 {
        apply_v1(conn)?;
    }

    validate_required_tables(conn)?;
    Ok(())
}

/// Read the newest recorded SQLite schema version.
pub fn get_schema_version(conn: &Connection) -> StateResult<i64> {
    let version = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(version)
}

/// Reject a schema created by a newer AgentOS release.
pub fn validate_supported_version(version: i64) -> StateResult<()> {
    if version > CURRENT_SCHEMA_VERSION {
        return Err(StateError::message(format!(
            "SQLite state schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn apply_v1(conn: &Connection) -> StateResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            agent_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            config_path TEXT NOT NULL,
            state TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            agent_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            message TEXT NOT NULL,
            timestamp_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS checkpoints (
            checkpoint_id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            parent_checkpoint_id TEXT,
            metadata_json TEXT NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_logs_agent_id ON logs(agent_id);
        CREATE INDEX IF NOT EXISTS idx_checkpoints_agent_id ON checkpoints(agent_id);
        "#,
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (?1, ?2)",
        rusqlite::params![CURRENT_SCHEMA_VERSION, current_time_millis() as i64],
    )?;
    Ok(())
}

fn validate_required_tables(conn: &Connection) -> StateResult<()> {
    for table in ["schema_migrations", "agents", "logs", "checkpoints"] {
        let exists = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            rusqlite::params![table],
            |row| row.get::<_, i64>(0),
        )?;
        if exists == 0 {
            return Err(StateError::message(format!(
                "SQLite state schema is missing required table '{table}'"
            )));
        }
    }
    Ok(())
}

fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
