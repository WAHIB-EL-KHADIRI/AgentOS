use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use rusqlite::params;
use serde_json;

use crate::recorder::RecordedThought;

pub type TraceResult<T> = Result<T, TraceStoreError>;

#[derive(Debug, thiserror::Error)]
pub enum TraceStoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("lock poisoned: {0}")]
    Lock(String),
}

impl<T> From<PoisonError<T>> for TraceStoreError {
    fn from(e: PoisonError<T>) -> Self {
        TraceStoreError::Lock(e.to_string())
    }
}

#[async_trait]
pub trait TraceStore: Send + Sync {
    async fn save_thought(&self, thought: &RecordedThought) -> TraceResult<String>;
    async fn load_thoughts(&self, agent_id: &str) -> TraceResult<Vec<RecordedThought>>;
    async fn load_all_thoughts(&self) -> TraceResult<Vec<RecordedThought>>;
    async fn delete_thought(&self, checkpoint_id: &str) -> TraceResult<bool>;
    async fn count(&self) -> TraceResult<usize>;
}

pub struct SqliteTraceStore {
    conn: Mutex<rusqlite::Connection>,
}

impl SqliteTraceStore {
    pub fn new(path: &str) -> TraceResult<Self> {
        let conn = rusqlite::Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize()?;
        Ok(store)
    }

    pub fn in_memory() -> TraceResult<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> TraceResult<()> {
        let conn = self.conn.lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS trace_thoughts (
                checkpoint_id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                parent_checkpoint_id TEXT,
                metadata TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_trace_agent_id ON trace_thoughts(agent_id);
            CREATE INDEX IF NOT EXISTS idx_trace_timestamp ON trace_thoughts(timestamp_ms);",
        )?;
        Ok(())
    }
}

#[async_trait]
impl TraceStore for SqliteTraceStore {
    async fn save_thought(&self, thought: &RecordedThought) -> TraceResult<String> {
        let conn = self.conn.lock()?;
        let metadata_str = serde_json::to_string(&thought.metadata)?;
        conn.execute(
            "INSERT OR REPLACE INTO trace_thoughts
             (checkpoint_id, agent_id, content, timestamp_ms, parent_checkpoint_id, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                thought.checkpoint_id,
                thought.agent_id,
                thought.content,
                thought.timestamp_ms as i64,
                thought.parent_checkpoint_id,
                metadata_str,
            ],
        )?;
        Ok(thought.checkpoint_id.clone())
    }

    async fn load_thoughts(&self, agent_id: &str) -> TraceResult<Vec<RecordedThought>> {
        let conn = self.conn.lock()?;
        let mut stmt = conn.prepare(
            "SELECT checkpoint_id, agent_id, content, timestamp_ms, parent_checkpoint_id, metadata
             FROM trace_thoughts WHERE agent_id = ?1 ORDER BY timestamp_ms ASC",
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            let metadata_str: String = row.get(5)?;
            let metadata: HashMap<String, String> =
                serde_json::from_str(&metadata_str).unwrap_or_default();
            Ok(RecordedThought {
                checkpoint_id: row.get(0)?,
                agent_id: row.get(1)?,
                content: row.get(2)?,
                timestamp_ms: row.get::<_, i64>(3)? as u64,
                parent_checkpoint_id: row.get(4)?,
                metadata,
            })
        })?;
        let mut thoughts = Vec::new();
        for row in rows {
            thoughts.push(row?);
        }
        Ok(thoughts)
    }

    async fn load_all_thoughts(&self) -> TraceResult<Vec<RecordedThought>> {
        let conn = self.conn.lock()?;
        let mut stmt = conn.prepare(
            "SELECT checkpoint_id, agent_id, content, timestamp_ms, parent_checkpoint_id, metadata
             FROM trace_thoughts ORDER BY timestamp_ms ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let metadata_str: String = row.get(5)?;
            let metadata: HashMap<String, String> =
                serde_json::from_str(&metadata_str).unwrap_or_default();
            Ok(RecordedThought {
                checkpoint_id: row.get(0)?,
                agent_id: row.get(1)?,
                content: row.get(2)?,
                timestamp_ms: row.get::<_, i64>(3)? as u64,
                parent_checkpoint_id: row.get(4)?,
                metadata,
            })
        })?;
        let mut thoughts = Vec::new();
        for row in rows {
            thoughts.push(row?);
        }
        Ok(thoughts)
    }

    async fn delete_thought(&self, checkpoint_id: &str) -> TraceResult<bool> {
        let conn = self.conn.lock()?;
        let affected = conn.execute(
            "DELETE FROM trace_thoughts WHERE checkpoint_id = ?1",
            params![checkpoint_id],
        )?;
        Ok(affected > 0)
    }

    async fn count(&self) -> TraceResult<usize> {
        let conn = self.conn.lock()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM trace_thoughts", [], |row| row.get(0))?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_load_thought() {
        let store = SqliteTraceStore::in_memory().unwrap();
        let thought = RecordedThought::new("agent-1", "hello world");
        let id = store.save_thought(&thought).await.unwrap();
        assert_eq!(id, thought.checkpoint_id);

        let loaded = store.load_thoughts("agent-1").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "hello world");
    }

    #[tokio::test]
    async fn test_load_all_thoughts() {
        let store = SqliteTraceStore::in_memory().unwrap();
        store
            .save_thought(&RecordedThought::new("agent-1", "a"))
            .await
            .unwrap();
        store
            .save_thought(&RecordedThought::new("agent-2", "b"))
            .await
            .unwrap();

        let all = store.load_all_thoughts().await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_thought() {
        let store = SqliteTraceStore::in_memory().unwrap();
        let thought = RecordedThought::new("agent-1", "delete me");
        let id = store.save_thought(&thought).await.unwrap();

        let deleted = store.delete_thought(&id).await.unwrap();
        assert!(deleted);

        let thoughts = store.load_thoughts("agent-1").await.unwrap();
        assert!(thoughts.is_empty());
    }

    #[tokio::test]
    async fn test_count() {
        let store = SqliteTraceStore::in_memory().unwrap();
        assert_eq!(store.count().await.unwrap(), 0);

        store
            .save_thought(&RecordedThought::new("agent-1", "x"))
            .await
            .unwrap();
        store
            .save_thought(&RecordedThought::new("agent-2", "y"))
            .await
            .unwrap();

        assert_eq!(store.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_thought_with_metadata() {
        let store = SqliteTraceStore::in_memory().unwrap();
        let thought = RecordedThought::new("agent-1", "thought with meta")
            .with_metadata("tool", "search")
            .with_metadata("model", "gpt-4");
        store.save_thought(&thought).await.unwrap();

        let loaded = store.load_thoughts("agent-1").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].metadata.get("tool").unwrap(), "search");
        assert_eq!(loaded[0].metadata.get("model").unwrap(), "gpt-4");
    }
}
