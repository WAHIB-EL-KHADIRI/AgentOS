use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SystemEventType {
    AgentSpawned,
    AgentStopped,
    AgentFailed,
    AgentDegraded,
    MemoryStored,
    SecretSet,
    SecretRead,
    ServiceRegistered,
    ServiceUnregistered,
    AgentDiscovered,
    PermissionGranted,
    PermissionChecked,
    ThoughtRecorded,
    Custom(String),
}

impl std::fmt::Display for SystemEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemEventType::AgentSpawned => write!(f, "agent.spawned"),
            SystemEventType::AgentStopped => write!(f, "agent.stopped"),
            SystemEventType::AgentFailed => write!(f, "agent.failed"),
            SystemEventType::AgentDegraded => write!(f, "agent.degraded"),
            SystemEventType::MemoryStored => write!(f, "memory.stored"),
            SystemEventType::SecretSet => write!(f, "secret.set"),
            SystemEventType::SecretRead => write!(f, "secret.read"),
            SystemEventType::ServiceRegistered => write!(f, "service.registered"),
            SystemEventType::ServiceUnregistered => write!(f, "service.unregistered"),
            SystemEventType::AgentDiscovered => write!(f, "agent.discovered"),
            SystemEventType::PermissionGranted => write!(f, "permission.granted"),
            SystemEventType::PermissionChecked => write!(f, "permission.checked"),
            SystemEventType::ThoughtRecorded => write!(f, "thought.recorded"),
            SystemEventType::Custom(t) => write!(f, "custom.{t}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEvent {
    pub id: String,
    pub event_type: SystemEventType,
    pub agent_id: Option<String>,
    pub payload: String,
    pub timestamp_ms: u64,
    pub sequence: u64,
}

pub trait EventStore: Send + Sync {
    fn append(&mut self, event: SystemEvent);
    fn read_all(&self) -> Vec<SystemEvent>;
    fn read_for_agent(&self, agent_id: &str) -> Vec<SystemEvent>;
    fn read_since(&self, sequence: u64) -> Vec<SystemEvent>;
    fn count(&self) -> usize;
}

const DEFAULT_MAX_EVENTS: usize = 100_000;

#[derive(Debug)]
pub struct InMemoryEventStore {
    events: VecDeque<SystemEvent>,
    max_events: usize,
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventStore {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
            max_events: DEFAULT_MAX_EVENTS,
        }
    }

    pub fn with_max_events(max: usize) -> Self {
        Self {
            events: VecDeque::new(),
            max_events: max,
        }
    }
}

impl EventStore for InMemoryEventStore {
    fn append(&mut self, event: SystemEvent) {
        if self.events.len() >= self.max_events {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    fn read_all(&self) -> Vec<SystemEvent> {
        self.events.iter().cloned().collect()
    }

    fn read_for_agent(&self, agent_id: &str) -> Vec<SystemEvent> {
        self.events
            .iter()
            .filter(|e| e.agent_id.as_deref() == Some(agent_id))
            .cloned()
            .collect()
    }

    fn read_since(&self, sequence: u64) -> Vec<SystemEvent> {
        self.events
            .iter()
            .filter(|e| e.sequence > sequence)
            .cloned()
            .collect()
    }

    fn count(&self) -> usize {
        self.events.len()
    }
}

pub struct EventBus {
    store: Arc<RwLock<InMemoryEventStore>>,
    sequence: Arc<std::sync::atomic::AtomicU64>,
    listeners: Arc<RwLock<Vec<Arc<dyn EventListener + Send + Sync>>>>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("store", &self.store)
            .field("sequence", &self.sequence)
            .field("listeners_count", &self.listeners.blocking_read().len())
            .finish()
    }
}

pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &SystemEvent);
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(InMemoryEventStore::new())),
            sequence: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn emit(
        &self,
        event_type: SystemEventType,
        agent_id: Option<String>,
        payload: String,
    ) -> SystemEvent {
        let seq = self
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let event = SystemEvent {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.clone(),
            agent_id,
            payload,
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            sequence: seq,
        };

        self.store.write().await.append(event.clone());

        info!(event = %event_type, seq = seq, "system event emitted");

        let listeners = {
            let listeners = self.listeners.read().await;
            listeners.clone()
        };
        for listener in listeners {
            listener.on_event(&event);
        }

        event
    }

    pub async fn read_all(&self) -> Vec<SystemEvent> {
        let store = self.store.read().await;
        store.read_all()
    }

    pub async fn read_for_agent(&self, agent_id: &str) -> Vec<SystemEvent> {
        let store = self.store.read().await;
        store.read_for_agent(agent_id)
    }

    pub async fn read_since(&self, sequence: u64) -> Vec<SystemEvent> {
        let store = self.store.read().await;
        store.read_since(sequence)
    }

    pub async fn subscribe(&self, listener: Box<dyn EventListener + Send + Sync>) {
        let mut listeners = self.listeners.write().await;
        listeners.push(Arc::from(listener));
    }
}

pub struct SqliteEventStore {
    conn: std::sync::Mutex<rusqlite::Connection>,
}

impl SqliteEventStore {
    pub fn new(path: &str) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS system_events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                agent_id TEXT,
                payload TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                sequence INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_agent ON system_events(agent_id);
            CREATE INDEX IF NOT EXISTS idx_events_sequence ON system_events(sequence);",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS system_events (
                id TEXT PRIMARY KEY,
                event_type TEXT NOT NULL,
                agent_id TEXT,
                payload TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                sequence INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_agent ON system_events(agent_id);
            CREATE INDEX IF NOT EXISTS idx_events_sequence ON system_events(sequence);",
        )?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }
}

impl EventStore for SqliteEventStore {
    fn append(&mut self, event: SystemEvent) {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => {
                tracing::warn!("SqliteEventStore: lock poisoned, skipping append");
                return;
            }
        };
        if let Err(e) = conn.execute(
            "INSERT INTO system_events (id, event_type, agent_id, payload, timestamp_ms, sequence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                event.id,
                event.event_type.to_string(),
                event.agent_id,
                event.payload,
                event.timestamp_ms as i64,
                event.sequence as i64,
            ],
        ) {
            tracing::warn!("SqliteEventStore: append failed: {e}");
        }
    }

    fn read_all(&self) -> Vec<SystemEvent> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare("SELECT id, event_type, agent_id, payload, timestamp_ms, sequence FROM system_events ORDER BY sequence") {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("SqliteEventStore: prepare read_all failed: {e}");
                return Vec::new();
            }
        };
        let events: Vec<SystemEvent> = match stmt.query_map([], |row| {
            let event_type_str: String = row.get(1)?;
            let ts: i64 = row.get(4)?;
            let seq: i64 = row.get(5)?;
            Ok(SystemEvent {
                id: row.get(0)?,
                event_type: parse_event_type(&event_type_str),
                agent_id: row.get(2)?,
                payload: row.get(3)?,
                timestamp_ms: ts as u64,
                sequence: seq as u64,
            })
        }) {
            Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::warn!("SqliteEventStore: query_map read_all failed: {e}");
                Vec::new()
            }
        };
        events
    }

    fn read_for_agent(&self, agent_id: &str) -> Vec<SystemEvent> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare("SELECT id, event_type, agent_id, payload, timestamp_ms, sequence FROM system_events WHERE agent_id = ?1 ORDER BY sequence") {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("SqliteEventStore: prepare read_for_agent failed: {e}");
                return Vec::new();
            }
        };
        let events: Vec<SystemEvent> = match stmt.query_map(rusqlite::params![agent_id], |row| {
            let event_type_str: String = row.get(1)?;
            let ts: i64 = row.get(4)?;
            let seq: i64 = row.get(5)?;
            Ok(SystemEvent {
                id: row.get(0)?,
                event_type: parse_event_type(&event_type_str),
                agent_id: row.get(2)?,
                payload: row.get(3)?,
                timestamp_ms: ts as u64,
                sequence: seq as u64,
            })
        }) {
            Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::warn!("SqliteEventStore: query_map read_for_agent failed: {e}");
                Vec::new()
            }
        };
        events
    }

    fn read_since(&self, sequence: u64) -> Vec<SystemEvent> {
        if sequence > i64::MAX as u64 {
            return Vec::new();
        }

        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let mut stmt = match conn.prepare("SELECT id, event_type, agent_id, payload, timestamp_ms, sequence FROM system_events WHERE sequence > ?1 ORDER BY sequence") {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("SqliteEventStore: prepare read_since failed: {e}");
                return Vec::new();
            }
        };
        let events: Vec<SystemEvent> =
            match stmt.query_map(rusqlite::params![sequence as i64], |row| {
                let event_type_str: String = row.get(1)?;
                let ts: i64 = row.get(4)?;
                let seq: i64 = row.get(5)?;
                Ok(SystemEvent {
                    id: row.get(0)?,
                    event_type: parse_event_type(&event_type_str),
                    agent_id: row.get(2)?,
                    payload: row.get(3)?,
                    timestamp_ms: ts as u64,
                    sequence: seq as u64,
                })
            }) {
                Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
                Err(e) => {
                    tracing::warn!("SqliteEventStore: query_map read_since failed: {e}");
                    Vec::new()
                }
            };
        events
    }

    fn count(&self) -> usize {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row("SELECT COUNT(*) FROM system_events", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0) as usize
    }
}

fn parse_event_type(s: &str) -> SystemEventType {
    match s {
        "agent.spawned" => SystemEventType::AgentSpawned,
        "agent.stopped" => SystemEventType::AgentStopped,
        "agent.failed" => SystemEventType::AgentFailed,
        "agent.degraded" => SystemEventType::AgentDegraded,
        "memory.stored" => SystemEventType::MemoryStored,
        "secret.set" => SystemEventType::SecretSet,
        "secret.read" => SystemEventType::SecretRead,
        "service.registered" => SystemEventType::ServiceRegistered,
        "service.unregistered" => SystemEventType::ServiceUnregistered,
        "agent.discovered" => SystemEventType::AgentDiscovered,
        "permission.granted" => SystemEventType::PermissionGranted,
        "permission.checked" => SystemEventType::PermissionChecked,
        "thought.recorded" => SystemEventType::ThoughtRecorded,
        custom => SystemEventType::Custom(custom.trim_start_matches("custom.").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn test_event_type_display() {
        assert_eq!(SystemEventType::AgentSpawned.to_string(), "agent.spawned");
        assert_eq!(
            SystemEventType::Custom("test".into()).to_string(),
            "custom.test"
        );
    }

    #[tokio::test]
    async fn test_event_emit_and_read() {
        let bus = EventBus::new();

        bus.emit(
            SystemEventType::AgentSpawned,
            Some("agent-1".into()),
            "hello".into(),
        )
        .await;

        let events = bus.read_all().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, SystemEventType::AgentSpawned);
        assert_eq!(events[0].payload, "hello");
        assert!(!events[0].id.is_empty());
        assert!(events[0].timestamp_ms > 0);
    }

    #[tokio::test]
    async fn test_read_for_agent() {
        let bus = EventBus::new();

        bus.emit(
            SystemEventType::AgentSpawned,
            Some("agent-1".into()),
            "a".into(),
        )
        .await;
        bus.emit(
            SystemEventType::AgentSpawned,
            Some("agent-2".into()),
            "b".into(),
        )
        .await;
        bus.emit(
            SystemEventType::AgentStopped,
            Some("agent-1".into()),
            "c".into(),
        )
        .await;

        let for_a1 = bus.read_for_agent("agent-1").await;
        assert_eq!(for_a1.len(), 2);
        assert_eq!(for_a1[0].payload, "a");
        assert_eq!(for_a1[1].payload, "c");

        let for_a2 = bus.read_for_agent("agent-2").await;
        assert_eq!(for_a2.len(), 1);
    }

    #[tokio::test]
    async fn test_read_since() {
        let bus = EventBus::new();

        bus.emit(SystemEventType::AgentSpawned, None, "a".into())
            .await;
        bus.emit(SystemEventType::AgentStopped, None, "b".into())
            .await;

        let since_0 = bus.read_since(0).await;
        assert_eq!(since_0.len(), 1);
        assert_eq!(since_0[0].payload, "b");

        let since_1 = bus.read_since(1).await;
        assert_eq!(since_1.len(), 0);
    }

    #[tokio::test]
    async fn test_event_sequence_increments() {
        let bus = EventBus::new();

        let e1 = bus
            .emit(SystemEventType::AgentSpawned, None, "x".into())
            .await;
        let e2 = bus
            .emit(SystemEventType::AgentStopped, None, "y".into())
            .await;

        assert_eq!(e1.sequence, 0);
        assert_eq!(e2.sequence, 1);
    }

    #[tokio::test]
    async fn test_event_listener() {
        struct Counter(Arc<AtomicU64>);
        impl EventListener for Counter {
            fn on_event(&self, _event: &SystemEvent) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }

        let bus = EventBus::new();
        let counter = Arc::new(AtomicU64::new(0));
        bus.subscribe(Box::new(Counter(Arc::clone(&counter)))).await;

        bus.emit(SystemEventType::AgentSpawned, None, "x".into())
            .await;
        bus.emit(SystemEventType::AgentStopped, None, "y".into())
            .await;

        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_sqlite_event_store() {
        let mut store = SqliteEventStore::in_memory().unwrap();

        let event = SystemEvent {
            id: "test-id".into(),
            event_type: SystemEventType::AgentSpawned,
            agent_id: Some("agent-1".into()),
            payload: "test payload".into(),
            timestamp_ms: 1000,
            sequence: 0,
        };
        store.append(event);
        assert_eq!(store.count(), 1);

        let all = store.read_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].payload, "test payload");
        assert_eq!(all[0].event_type, SystemEventType::AgentSpawned);

        let for_agent = store.read_for_agent("agent-1");
        assert_eq!(for_agent.len(), 1);

        let for_other = store.read_for_agent("agent-2");
        assert_eq!(for_other.len(), 0);

        let since = store.read_since(0);
        assert_eq!(since.len(), 0);

        let since_all = store.read_since(u64::MAX);
        assert_eq!(since_all.len(), 0);
    }
}
