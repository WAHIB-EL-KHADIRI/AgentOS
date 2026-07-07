use std::path::{Path, PathBuf};

use agentos_trace::RecordedThought;
use rusqlite::Connection;

use crate::state::{
    clean_backend_state, doctor_sqlite_state_at, export_backend_state, import_into_backend,
    inspect_loaded_state, migrate_json_to_sqlite_paths, migrations, CliState, StateBackend,
    StateBackendKind, StateCleanOptions, StateCleanReport, StateDoctorReport, StateExportReport,
    StateImportMode, StateImportReport, StateInspectReport, StateMigrationReport, StoredAgentEntry,
    StoredLogEntry,
};

pub struct SqliteStateBackend {
    path: PathBuf,
}

impl SqliteStateBackend {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub(crate) fn connect(&self) -> anyhow::Result<Connection> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                anyhow::anyhow!("cannot create SQLite state dir '{}': {e}", parent.display())
            })?;
        }

        let conn = Connection::open(&self.path).map_err(|e| {
            anyhow::anyhow!("cannot open SQLite state '{}': {e}", self.path.display())
        })?;
        migrations::apply_migrations(&conn)?;
        Ok(conn)
    }
}

impl StateBackend for SqliteStateBackend {
    fn kind(&self) -> StateBackendKind {
        StateBackendKind::Sqlite
    }

    fn path(&self) -> PathBuf {
        self.path.clone()
    }

    fn load_state(&self) -> anyhow::Result<CliState> {
        load_state_from_sqlite_path(&self.path)
    }

    fn save_state(&self, state: &CliState) -> anyhow::Result<()> {
        save_state_to_sqlite_path(state, &self.path)
    }

    fn doctor(&self) -> anyhow::Result<StateDoctorReport> {
        doctor_sqlite_state_at(&self.path)
    }

    fn inspect(&self) -> StateInspectReport {
        inspect_loaded_state(self)
    }

    fn export_state(&self, output: &Path, pretty: bool) -> anyhow::Result<StateExportReport> {
        export_backend_state(self, output, pretty)
    }

    fn import_state(
        &self,
        input: &Path,
        mode: StateImportMode,
        dry_run: bool,
    ) -> anyhow::Result<StateImportReport> {
        import_into_backend(self, input, mode, dry_run)
    }

    fn clean(&self, options: &StateCleanOptions) -> anyhow::Result<StateCleanReport> {
        clean_backend_state(self, options)
    }

    fn migrate_from_json(
        &self,
        source: &Path,
        dry_run: bool,
    ) -> anyhow::Result<StateMigrationReport> {
        migrate_json_to_sqlite_paths(source, &self.path, dry_run)
    }
}

pub fn load_state_from_sqlite_path(path: &Path) -> anyhow::Result<CliState> {
    let backend = SqliteStateBackend::new(path);
    let conn = backend.connect()?;

    let mut agents_stmt = conn
        .prepare(
            "SELECT agent_id, name, config_path, state, started_at_ms, updated_at_ms FROM agents ORDER BY updated_at_ms",
        )
        .map_err(|e| anyhow::anyhow!("cannot read SQLite agents: {e}"))?;
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
        })
        .map_err(|e| anyhow::anyhow!("cannot query SQLite agents: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("cannot decode SQLite agents: {e}"))?;

    let mut logs_stmt = conn
        .prepare(
            "SELECT agent_id, event_type, message, timestamp_ms FROM logs ORDER BY timestamp_ms, id",
        )
        .map_err(|e| anyhow::anyhow!("cannot read SQLite logs: {e}"))?;
    let logs = logs_stmt
        .query_map([], |row| {
            Ok(StoredLogEntry {
                agent_id: row.get(0)?,
                event_type: row.get(1)?,
                message: row.get(2)?,
                timestamp_ms: row.get::<_, i64>(3)? as u64,
            })
        })
        .map_err(|e| anyhow::anyhow!("cannot query SQLite logs: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("cannot decode SQLite logs: {e}"))?;

    let mut checkpoints_stmt = conn
        .prepare(
            "SELECT checkpoint_id, agent_id, content, timestamp_ms, parent_checkpoint_id, metadata_json FROM checkpoints ORDER BY timestamp_ms, checkpoint_id",
        )
        .map_err(|e| anyhow::anyhow!("cannot read SQLite checkpoints: {e}"))?;
    let thoughts = checkpoints_stmt
        .query_map([], |row| {
            let metadata_json: String = row.get(5)?;
            let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();
            Ok(RecordedThought {
                checkpoint_id: row.get(0)?,
                agent_id: row.get(1)?,
                content: row.get(2)?,
                timestamp_ms: row.get::<_, i64>(3)? as u64,
                parent_checkpoint_id: row.get(4)?,
                metadata,
            })
        })
        .map_err(|e| anyhow::anyhow!("cannot query SQLite checkpoints: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("cannot decode SQLite checkpoints: {e}"))?;

    Ok(CliState {
        agents,
        logs,
        thoughts,
    })
}

pub fn sqlite_schema_version_at(path: &Path) -> anyhow::Result<i64> {
    let backend = SqliteStateBackend::new(path);
    let conn = backend.connect()?;
    migrations::get_schema_version(&conn)
}

pub fn save_state_to_sqlite_path(state: &CliState, path: &Path) -> anyhow::Result<()> {
    let backend = SqliteStateBackend::new(path);
    let mut conn = backend.connect()?;
    let tx = conn
        .transaction()
        .map_err(|e| anyhow::anyhow!("cannot start SQLite transaction: {e}"))?;

    tx.execute("DELETE FROM logs", [])
        .map_err(|e| anyhow::anyhow!("cannot clear SQLite logs: {e}"))?;
    tx.execute("DELETE FROM checkpoints", [])
        .map_err(|e| anyhow::anyhow!("cannot clear SQLite checkpoints: {e}"))?;
    tx.execute("DELETE FROM agents", [])
        .map_err(|e| anyhow::anyhow!("cannot clear SQLite agents: {e}"))?;

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
        )
        .map_err(|e| anyhow::anyhow!("cannot write SQLite agent '{}': {e}", agent.agent_id))?;
    }

    for log in &state.logs {
        tx.execute(
            "INSERT INTO logs (agent_id, event_type, message, timestamp_ms) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                log.agent_id,
                log.event_type,
                log.message,
                log.timestamp_ms as i64,
            ],
        )
        .map_err(|e| anyhow::anyhow!("cannot write SQLite log for '{}': {e}", log.agent_id))?;
    }

    for thought in &state.thoughts {
        let metadata_json = serde_json::to_string(&thought.metadata)
            .map_err(|e| anyhow::anyhow!("cannot encode checkpoint metadata: {e}"))?;
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
        )
        .map_err(|e| anyhow::anyhow!("cannot write SQLite checkpoint '{}': {e}", thought.checkpoint_id))?;
    }

    tx.commit()
        .map_err(|e| anyhow::anyhow!("cannot commit SQLite state: {e}"))?;
    Ok(())
}
