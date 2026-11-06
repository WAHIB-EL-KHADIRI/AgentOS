use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::migrations;
use crate::{
    StateBackend, StateBackendKind, StateError, StateResult, StateSnapshot, StoredAgentEntry,
    StoredLogEntry, StoredThoughtEntry, CURRENT_STATE_VERSION,
};

/// SQLite storage for operational state snapshots.
pub struct SqliteStateBackend {
    path: PathBuf,
}

impl SqliteStateBackend {
    /// Create a backend for the given database file.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Return the configured database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connect(&self) -> StateResult<Connection> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| StateError::io(parent, error))?;
        }

        let conn = Connection::open(&self.path).map_err(|error| {
            StateError::backend(
                format!("cannot open SQLite state '{}'", self.path.display()),
                error,
            )
        })?;
        migrations::apply_migrations(&conn)?;
        Ok(conn)
    }
}

impl StateBackend for SqliteStateBackend {
    fn kind(&self) -> StateBackendKind {
        StateBackendKind::SQLITE
    }

    fn load_state(&self) -> StateResult<StateSnapshot> {
        load_state_from_sqlite_path(&self.path)
    }

    fn save_state(&self, state: &StateSnapshot) -> StateResult<()> {
        save_state_to_sqlite_path(state, &self.path)
    }
}

/// Load and decode a snapshot from a SQLite database.
pub fn load_state_from_sqlite_path(path: &Path) -> StateResult<StateSnapshot> {
    let backend = SqliteStateBackend::new(path);
    let conn = backend.connect()?;

    let mut agents_stmt = conn.prepare(
        "SELECT agent_id, name, config_path, state, started_at_ms, updated_at_ms FROM agents ORDER BY updated_at_ms",
    )?;
    let agents = agents_stmt
        .query_map([], |row| {
            Ok(StoredAgentEntry {
                agent_id: row.get(0)?,
                name: row.get(1)?,
                config_path: row.get(2)?,
                state: row.get(3)?,
                started_at_ms: row.get::<_, i64>(4)? as u64,
                updated_at_ms: row.get::<_, i64>(5)? as u64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut logs_stmt = conn.prepare(
        "SELECT agent_id, event_type, message, timestamp_ms FROM logs ORDER BY timestamp_ms, id",
    )?;
    let logs = logs_stmt
        .query_map([], |row| {
            Ok(StoredLogEntry {
                agent_id: row.get(0)?,
                event_type: row.get(1)?,
                message: row.get(2)?,
                timestamp_ms: row.get::<_, i64>(3)? as u64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut checkpoints_stmt = conn.prepare(
        "SELECT checkpoint_id, agent_id, content, timestamp_ms, parent_checkpoint_id, metadata_json FROM checkpoints ORDER BY timestamp_ms, checkpoint_id",
    )?;
    let thoughts = checkpoints_stmt
        .query_map([], |row| {
            let metadata_json: String = row.get(5)?;
            let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();
            Ok(StoredThoughtEntry {
                checkpoint_id: row.get(0)?,
                agent_id: row.get(1)?,
                content: row.get(2)?,
                timestamp_ms: row.get::<_, i64>(3)? as u64,
                parent_checkpoint_id: row.get(4)?,
                metadata,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(StateSnapshot {
        version: CURRENT_STATE_VERSION,
        agents,
        logs,
        thoughts,
    })
}

/// Return the current SQLite schema migration version.
pub fn sqlite_schema_version_at(path: &Path) -> StateResult<i64> {
    let backend = SqliteStateBackend::new(path);
    let conn = backend.connect()?;
    migrations::get_schema_version(&conn)
}

/// Replace all state rows in a SQLite transaction.
pub fn save_state_to_sqlite_path(state: &StateSnapshot, path: &Path) -> StateResult<()> {
    let backend = SqliteStateBackend::new(path);
    let mut conn = backend.connect()?;
    let tx = conn.transaction()?;

    tx.execute("DELETE FROM logs", [])?;
    tx.execute("DELETE FROM checkpoints", [])?;
    tx.execute("DELETE FROM agents", [])?;

    for agent in &state.agents {
        tx.execute(
            "INSERT INTO agents (agent_id, name, config_path, state, started_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                agent.agent_id,
                agent.name,
                agent.config_path,
                agent.state,
                agent.started_at_ms as i64,
                agent.updated_at_ms as i64,
            ],
        )?;
    }

    for log in &state.logs {
        tx.execute(
            "INSERT INTO logs (agent_id, event_type, message, timestamp_ms) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![log.agent_id, log.event_type, log.message, log.timestamp_ms as i64],
        )?;
    }

    for thought in &state.thoughts {
        let metadata_json = serde_json::to_string(&thought.metadata)?;
        tx.execute(
            "INSERT INTO checkpoints (checkpoint_id, agent_id, content, timestamp_ms, parent_checkpoint_id, metadata_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                thought.checkpoint_id,
                thought.agent_id,
                thought.content,
                thought.timestamp_ms as i64,
                thought.parent_checkpoint_id,
                metadata_json,
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}
