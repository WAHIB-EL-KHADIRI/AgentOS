use rusqlite::Connection;

pub const CURRENT_SCHEMA_VERSION: i64 = 1;

pub fn apply_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at_ms INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|e| anyhow::anyhow!("cannot initialize SQLite schema migration table: {e}"))?;

    let current = get_schema_version(conn)?;
    validate_supported_version(current)?;

    if current < 1 {
        apply_v1(conn)?;
    }

    validate_required_tables(conn)?;
    Ok(())
}

pub fn get_schema_version(conn: &Connection) -> anyhow::Result<i64> {
    let version = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| anyhow::anyhow!("cannot read SQLite schema version: {e}"))?;
    Ok(version)
}

pub fn validate_supported_version(version: i64) -> anyhow::Result<()> {
    if version > CURRENT_SCHEMA_VERSION {
        anyhow::bail!(
            "SQLite state schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
        );
    }
    Ok(())
}

fn apply_v1(conn: &Connection) -> anyhow::Result<()> {
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
    )
    .map_err(|e| anyhow::anyhow!("cannot apply SQLite state schema v1: {e}"))?;

    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations (version, applied_at_ms) VALUES (?1, ?2)",
        rusqlite::params![
            CURRENT_SCHEMA_VERSION,
            crate::state::current_time_millis() as i64
        ],
    )
    .map_err(|e| anyhow::anyhow!("cannot record SQLite schema migration v1: {e}"))?;
    Ok(())
}

fn validate_required_tables(conn: &Connection) -> anyhow::Result<()> {
    for table in ["schema_migrations", "agents", "logs", "checkpoints"] {
        let exists = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                rusqlite::params![table],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|e| anyhow::anyhow!("cannot validate SQLite table '{table}': {e}"))?;
        if exists == 0 {
            anyhow::bail!("SQLite state schema is missing required table '{table}'");
        }
    }
    Ok(())
}
