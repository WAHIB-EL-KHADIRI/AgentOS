use std::path::Path;

use parking_lot::Mutex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_CONTENT_LEN: usize = 1_000_000;

pub type MemoryResult<T> = Result<T, MemoryError>;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("storage error: {0}")]
    Storage(String),

    #[error("record not found: {0}")]
    NotFound(String),

    #[error("embedding error: {0}")]
    Embedding(String),
}

impl From<rusqlite::Error> for MemoryError {
    fn from(e: rusqlite::Error) -> Self {
        MemoryError::Storage(e.to_string())
    }
}

impl From<serde_json::Error> for MemoryError {
    fn from(e: serde_json::Error) -> Self {
        MemoryError::Storage(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub agent_id: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub created_at_ms: u64,
}

impl MemoryRecord {
    pub fn new(agent_id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut content: String = content.into();
        if content.len() > MAX_CONTENT_LEN {
            content.truncate(MAX_CONTENT_LEN);
        }
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.into(),
            content,
            embedding: Vec::new(),
            created_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

pub trait MemoryStore: Send + Sync {
    fn insert(&self, record: MemoryRecord) -> MemoryResult<String>;
    fn list_for_agent(&self, agent_id: &str) -> MemoryResult<Vec<MemoryRecord>>;
    fn search(
        &self,
        agent_id: &str,
        query: &[f32],
        top_k: usize,
    ) -> MemoryResult<Vec<MemoryRecord>>;
    fn full_text_search(
        &self,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> MemoryResult<Vec<MemoryRecord>>;
    fn delete(&self, id: &str) -> MemoryResult<()>;
    fn count(&self, agent_id: &str) -> MemoryResult<usize>;
}

#[derive(Debug, Default)]
pub struct MemoryStoreConfig {
    pub max_records_per_agent: usize,
}

impl Default for Box<dyn MemoryStore> {
    fn default() -> Self {
        Box::new(InMemoryStore::new())
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    records: Mutex<Vec<MemoryRecord>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MemoryStore for InMemoryStore {
    fn insert(&self, record: MemoryRecord) -> MemoryResult<String> {
        let id = record.id.clone();
        let mut records = self.records.lock();
        records.push(record);
        Ok(id)
    }

    fn list_for_agent(&self, agent_id: &str) -> MemoryResult<Vec<MemoryRecord>> {
        let records = self.records.lock();
        Ok(records
            .iter()
            .filter(|r| r.agent_id == agent_id)
            .cloned()
            .collect())
    }

    fn search(
        &self,
        agent_id: &str,
        query: &[f32],
        top_k: usize,
    ) -> MemoryResult<Vec<MemoryRecord>> {
        let records = self.records.lock();
        let mut scored: Vec<(f32, MemoryRecord)> = records
            .iter()
            .filter(|r| r.agent_id == agent_id && !r.embedding.is_empty())
            .map(|r| {
                let score = cosine_similarity(query, &r.embedding);
                (score, r.clone())
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(top_k).map(|(_, r)| r).collect())
    }

    fn delete(&self, id: &str) -> MemoryResult<()> {
        let mut records = self.records.lock();
        let len_before = records.len();
        records.retain(|r| r.id != id);
        if records.len() == len_before {
            return Err(MemoryError::NotFound(id.to_string()));
        }
        Ok(())
    }

    fn count(&self, agent_id: &str) -> MemoryResult<usize> {
        let records = self.records.lock();
        Ok(records.iter().filter(|r| r.agent_id == agent_id).count())
    }

    fn full_text_search(
        &self,
        agent_id: &str,
        query: &str,
        _limit: usize,
    ) -> MemoryResult<Vec<MemoryRecord>> {
        let records = self.records.lock();
        let lower = query.to_lowercase();
        let terms: Vec<&str> = lower.split_whitespace().collect();
        let mut results: Vec<MemoryRecord> = records
            .iter()
            .filter(|r| {
                r.agent_id == agent_id && {
                    let content = r.content.to_lowercase();
                    terms.iter().all(|t| content.contains(t))
                }
            })
            .cloned()
            .collect();
        results.truncate(_limit);
        Ok(results)
    }
}

pub struct SqliteMemoryStore {
    conn: Mutex<Connection>,
}

impl SqliteMemoryStore {
    pub fn new(path: impl AsRef<Path>) -> MemoryResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                agent_id UNINDEXED,
                content,
                tokenize='unicode61'
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl MemoryStore for SqliteMemoryStore {
    fn insert(&self, record: MemoryRecord) -> MemoryResult<String> {
        let conn = self.conn.lock();
        let embedding_blob = if record.embedding.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&record.embedding)?)
        };

        conn.execute(
            "INSERT INTO memories (id, agent_id, content, embedding, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                record.id,
                record.agent_id,
                record.content,
                embedding_blob,
                record.created_at_ms as i64,
            ],
        )?;
        let _ = conn.execute(
            "INSERT INTO memories_fts (id, agent_id, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![record.id, record.agent_id, record.content],
        );
        Ok(record.id)
    }

    fn list_for_agent(&self, agent_id: &str) -> MemoryResult<Vec<MemoryRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, agent_id, content, embedding, created_at_ms FROM memories WHERE agent_id = ?1 ORDER BY created_at_ms DESC",
        )?;
        let records = stmt
            .query_map(rusqlite::params![agent_id], |row| {
                let embedding_str: Option<String> = row.get(3)?;
                let embedding: Vec<f32> = embedding_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();

                Ok(MemoryRecord {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    content: row.get(2)?,
                    embedding,
                    created_at_ms: row.get::<_, i64>(4)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    fn search(
        &self,
        agent_id: &str,
        query: &[f32],
        top_k: usize,
    ) -> MemoryResult<Vec<MemoryRecord>> {
        let records = self.list_for_agent(agent_id)?;
        let mut scored: Vec<(f32, MemoryRecord)> = records
            .into_iter()
            .filter(|r| !r.embedding.is_empty())
            .map(|r| {
                let score = cosine_similarity(query, &r.embedding);
                (score, r)
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(top_k).map(|(_, r)| r).collect())
    }

    fn full_text_search(
        &self,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> MemoryResult<Vec<MemoryRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT m.id, m.agent_id, m.content, m.embedding, m.created_at_ms
             FROM memories_fts fts
             JOIN memories m ON m.id = fts.id
             WHERE fts.agent_id = ?1 AND memories_fts MATCH ?2
             ORDER BY rank
             LIMIT ?3",
        )?;
        let records = stmt
            .query_map(rusqlite::params![agent_id, query, limit as i64], |row| {
                let embedding_str: Option<String> = row.get(3)?;
                let embedding: Vec<f32> = embedding_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();

                Ok(MemoryRecord {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    content: row.get(2)?,
                    embedding,
                    created_at_ms: row.get::<_, i64>(4)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    fn delete(&self, id: &str) -> MemoryResult<()> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM memories WHERE id = ?1", rusqlite::params![id])?;
        if affected == 0 {
            return Err(MemoryError::NotFound(id.to_string()));
        }
        let _ = conn.execute(
            "DELETE FROM memories_fts WHERE id = ?1",
            rusqlite::params![id],
        );
        Ok(())
    }

    fn count(&self, agent_id: &str) -> MemoryResult<usize> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM memories WHERE agent_id = ?1",
            rusqlite::params![agent_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_insert_and_list() {
        let store = InMemoryStore::new();
        let record = MemoryRecord::new("agent-1", "test memory");
        let id = store.insert(record).unwrap();
        assert!(!id.is_empty());

        let records = store.list_for_agent("agent-1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].content, "test memory");
    }

    #[test]
    fn test_in_memory_delete() {
        let store = InMemoryStore::new();
        let record = MemoryRecord::new("agent-1", "test");
        let id = store.insert(record).unwrap();
        store.delete(&id).unwrap();
        assert!(store.delete(&id).is_err());
    }

    #[test]
    fn test_in_memory_count() {
        let store = InMemoryStore::new();
        store.insert(MemoryRecord::new("agent-1", "a")).unwrap();
        store.insert(MemoryRecord::new("agent-1", "b")).unwrap();
        store.insert(MemoryRecord::new("agent-2", "c")).unwrap();
        assert_eq!(store.count("agent-1").unwrap(), 2);
        assert_eq!(store.count("agent-2").unwrap(), 1);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 1e-6);

        let empty: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&empty, &empty), 0.0);
    }

    #[test]
    fn test_search_by_embedding() {
        let store = InMemoryStore::new();
        let mut r1 = MemoryRecord::new("agent-1", "dog breed information");
        r1.embedding = vec![1.0, 0.0, 0.0];
        let mut r2 = MemoryRecord::new("agent-1", "cat food recipes");
        r2.embedding = vec![0.0, 1.0, 0.0];

        store.insert(r1).unwrap();
        store.insert(r2).unwrap();

        let results = store.search("agent-1", &[0.9, 0.1, 0.0], 5).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].content.contains("dog"));
    }

    #[test]
    fn test_sqlite_store() {
        let path = std::env::temp_dir().join(format!("test_memory_{}.db", uuid::Uuid::new_v4()));
        let store = SqliteMemoryStore::new(&path).unwrap();

        let record = MemoryRecord::new("agent-1", "persistent memory");
        let id = store.insert(record).unwrap();

        let records = store.list_for_agent("agent-1").unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].content, "persistent memory");

        store.delete(&id).unwrap();
        assert_eq!(store.count("agent-1").unwrap(), 0);

        let _ = std::fs::remove_file(&path);
    }
}
