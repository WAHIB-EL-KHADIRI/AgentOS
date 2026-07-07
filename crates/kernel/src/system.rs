use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

use agentos_bus::InMemoryBus;
use agentos_memory::{Embedder, HashingEmbedder, InMemoryStore, MemoryRecord, MemoryStore};
use agentos_registry::{Registry, ServiceDescriptor};
use agentos_trace::TraceRecorder;
use agentos_vault::{PermissionSet, Vault};
use tokio::sync::RwLock;

use crate::agent::AgentSpec;
use crate::error::AgentError;
use crate::events::{EventBus, SystemEventType};
use crate::handle::AgentHandle;
use crate::plugins::AgentHooks;
use crate::runtime_config::RuntimeConfig;
use crate::supervisor::Supervisor;
use crate::AgentResult;

pub type SystemResult<T> = Result<T, SystemError>;

#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("system not initialized: {0}")]
    NotInitialized(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct AgentOSSystem {
    pub supervisor: Supervisor,
    pub bus: Arc<InMemoryBus>,
    pub trace_recorder: Arc<RwLock<TraceRecorder>>,
    pub memory_store: Arc<dyn MemoryStore>,
    pub vault: Arc<RwLock<Vault>>,
    pub registry: Arc<RwLock<Registry>>,
    pub embedder: Arc<dyn Embedder>,
    pub event_bus: Arc<EventBus>,
    pub agent_hooks: Arc<AgentHooks>,
    pub config: RuntimeConfig,
    permission_set: Arc<RwLock<PermissionSet>>,
    agent_logs: Arc<RwLock<AgentLogStore>>,
}

impl fmt::Debug for AgentOSSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentOSSystem")
            .field("supervisor", &self.supervisor)
            .field("bus", &self.bus)
            .field("trace_recorder", &self.trace_recorder)
            .field("memory_store", &"Arc<dyn MemoryStore>")
            .field("vault", &self.vault)
            .field("registry", &self.registry)
            .field("embedder", &"Arc<dyn Embedder>")
            .field("permission_set", &self.permission_set)
            .field("agent_logs", &self.agent_logs)
            .finish()
    }
}

impl Default for AgentOSSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentOSSystem {
    pub fn new() -> Self {
        Self::with_config(RuntimeConfig::default())
    }

    pub fn with_config(config: RuntimeConfig) -> Self {
        let bus = Arc::new(InMemoryBus::new());
        let supervisor = Supervisor::new()
            .with_shared_bus(Arc::clone(&bus))
            .with_max_agents(config.max_agents);
        let event_bus = Arc::new(EventBus::new());
        let agent_hooks = Arc::new(AgentHooks::new());

        Self {
            supervisor,
            bus,
            trace_recorder: Arc::new(RwLock::new(TraceRecorder::new())),
            memory_store: Arc::new(InMemoryStore::new()),
            vault: Arc::new(RwLock::new(Vault::new())),
            registry: Arc::new(RwLock::new(Registry::new())),
            embedder: Arc::new(HashingEmbedder::new(128)),
            event_bus,
            agent_hooks,
            config,
            permission_set: Arc::new(RwLock::new(PermissionSet::new())),
            agent_logs: Arc::new(RwLock::new(AgentLogStore::new())),
        }
    }

    pub async fn spawn_agent(&self, spec: AgentSpec) -> AgentResult<AgentHandle> {
        let agent_id = spec.id.clone();
        let agent_name = spec.name.clone();
        let capabilities = spec.capabilities.clone();

        let handle = self.supervisor.spawn(spec).await?;

        // Register in registry
        {
            let mut reg = self.registry.write().await;
            let desc = ServiceDescriptor::new(&agent_id, "local").with_capabilities(capabilities);
            reg.register_or_update(desc);
        }

        // Record trace checkpoint for spawn
        {
            let mut trace = self.trace_recorder.write().await;
            trace.record_checkpoint(&agent_id, format!("Agent '{}' spawned", agent_name));
        }

        // Log the event
        self.log_event(
            &agent_id,
            "spawned",
            &format!("Agent '{}' started", agent_name),
        )
        .await;

        // Emit system event
        self.event_bus
            .emit(
                SystemEventType::AgentSpawned,
                Some(agent_id.to_string()),
                format!("Agent '{}' spawned", agent_name),
            )
            .await;

        // Notify plugins
        self.agent_hooks.on_spawned(self, &agent_id).await;

        Ok(handle)
    }

    pub async fn record_thought(&self, agent_id: &str, content: &str) -> String {
        let mut trace = self.trace_recorder.write().await;
        let id = trace.record_checkpoint(agent_id, content);
        self.log_event(agent_id, "thought", content).await;

        self.event_bus
            .emit(
                SystemEventType::ThoughtRecorded,
                Some(agent_id.to_string()),
                content.to_string(),
            )
            .await;

        self.agent_hooks.on_thought(self, agent_id, content).await;

        id
    }

    pub async fn store_memory(&self, agent_id: &str, content: &str) -> AgentResult<String> {
        let mut record = MemoryRecord::new(agent_id, content);
        record.embedding = self.embedder.embed(content);
        let id = self
            .memory_store
            .insert(record)
            .map_err(|e| AgentError::Internal(e.to_string()))?;

        self.event_bus
            .emit(
                SystemEventType::MemoryStored,
                Some(agent_id.to_string()),
                content.to_string(),
            )
            .await;

        self.agent_hooks.on_memory(self, agent_id, content).await;

        Ok(id)
    }

    pub async fn search_memory(
        &self,
        agent_id: &str,
        query: &str,
        top_k: usize,
    ) -> AgentResult<Vec<MemoryRecord>> {
        let query_embedding = self.embedder.embed(query);
        self.memory_store
            .search(agent_id, &query_embedding, top_k)
            .map_err(|e| AgentError::Internal(e.to_string()))
    }

    pub async fn set_secret(&self, agent_id: &str, key: &str, value: &str) {
        let mut vault = self.vault.write().await;
        vault.put(agent_id, key, value);
    }

    pub async fn get_secret(&self, agent_id: &str, key: &str) -> Option<String> {
        let mut vault = self.vault.write().await;
        let secret = vault.get(agent_id, key).ok()?;
        Some(secret.expose().to_string())
    }

    pub async fn register_service(&self, descriptor: ServiceDescriptor) {
        let mut reg = self.registry.write().await;
        reg.register_or_update(descriptor);
    }

    pub async fn discover_agents(&self, capability: &str) -> Vec<ServiceDescriptor> {
        let reg = self.registry.read().await;
        reg.discover_by_capability(capability)
            .into_iter()
            .cloned()
            .collect()
    }

    pub async fn log_event(&self, agent_id: &str, event_type: &str, message: &str) {
        let mut logs = self.agent_logs.write().await;
        logs.push(AgentLogEntry {
            agent_id: agent_id.to_string(),
            event_type: event_type.to_string(),
            message: message.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        });
    }

    pub async fn get_logs(&self, agent_id: &str, limit: usize) -> Vec<AgentLogEntry> {
        let logs = self.agent_logs.read().await;
        logs.get_for_agent(agent_id, limit)
    }

    pub async fn get_all_logs(&self) -> Vec<AgentLogEntry> {
        let logs = self.agent_logs.read().await;
        logs.all_entries()
    }

    pub async fn shutdown_all(&self) {
        let agents = self.supervisor.list().await;
        for handle in &agents {
            let _ = self.record_thought(&handle.id, "Agent shutting down").await;
        }
        self.supervisor.shutdown_all().await;
    }

    pub async fn grant_permission(&self, permission: agentos_vault::Permission) {
        let mut perms = self.permission_set.write().await;
        perms.grant(permission);
    }

    pub async fn check_permission(&self, permission: &agentos_vault::Permission) -> bool {
        let perms = self.permission_set.read().await;
        perms.contains(permission)
    }
}

#[derive(Debug, Clone)]
pub struct AgentLogEntry {
    pub agent_id: String,
    pub event_type: String,
    pub message: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Default)]
pub struct AgentLogStore {
    entries: VecDeque<AgentLogEntry>,
    max_entries: usize,
}

impl AgentLogStore {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries: 10_000,
        }
    }

    pub fn push(&mut self, entry: AgentLogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn get_for_agent(&self, agent_id: &str, limit: usize) -> Vec<AgentLogEntry> {
        self.entries
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn all_entries(&self) -> Vec<AgentLogEntry> {
        self.entries.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentSpec;

    #[tokio::test]
    async fn test_system_spawn_and_trace() {
        let system = AgentOSSystem::new();
        let spec = AgentSpec::new("sys-test-1", "System Test");
        let handle = system.spawn_agent(spec).await.unwrap();
        assert!(handle.is_running().await);

        let thought_id = system.record_thought("sys-test-1", "test thought").await;
        assert!(!thought_id.is_empty());

        system.shutdown_all().await;
        assert!(!handle.is_running().await);
    }

    #[tokio::test]
    async fn test_system_memory_integration() {
        let system = AgentOSSystem::new();
        let id = system
            .store_memory("agent-1", "Important memory")
            .await
            .unwrap();
        assert!(!id.is_empty());

        let results = system.search_memory("agent-1", "memory", 5).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_system_vault_integration() {
        let system = AgentOSSystem::new();
        system.set_secret("agent-1", "API_KEY", "sk-123").await;
        let value = system.get_secret("agent-1", "API_KEY").await;
        assert_eq!(value, Some("sk-123".into()));
    }

    #[tokio::test]
    async fn test_system_registry_integration() {
        let system = AgentOSSystem::new();
        let desc = ServiceDescriptor::new("search-agent", "localhost:9001")
            .with_capabilities(vec!["search".into()]);
        system.register_service(desc).await;

        let results = system.discover_agents("search").await;
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_system_logging() {
        let system = AgentOSSystem::new();
        system.log_event("agent-1", "test", "hello world").await;
        let logs = system.get_logs("agent-1", 10).await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "hello world");
    }

    #[tokio::test]
    async fn test_system_with_config() {
        let cfg = RuntimeConfig {
            http_port: 9090,
            max_agents: 5,
            ..Default::default()
        };
        let system = AgentOSSystem::with_config(cfg);
        assert_eq!(system.config.http_port, 9090);
        assert_eq!(system.config.max_agents, 5);
    }

    #[tokio::test]
    async fn test_system_spawn_records_registry() {
        let system = AgentOSSystem::new();
        let mut spec = AgentSpec::new("reg-test", "Registry Test");
        spec.capabilities = vec!["search".into(), "memory".into()];
        let _handle = system.spawn_agent(spec).await.unwrap();

        let agents = system.discover_agents("search").await;
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "reg-test");
    }

    #[tokio::test]
    async fn test_system_spawn_failure_has_no_success_side_effects() {
        let cfg = RuntimeConfig {
            max_agents: 0,
            ..Default::default()
        };
        let system = AgentOSSystem::with_config(cfg);
        let mut spec = AgentSpec::new("spawn-fail", "Spawn Fail");
        spec.capabilities = vec!["search".into()];

        assert!(system.spawn_agent(spec).await.is_err());
        assert!(system
            .event_bus
            .read_for_agent("spawn-fail")
            .await
            .is_empty());
        assert!(system.get_logs("spawn-fail", 10).await.is_empty());
        assert!(system.discover_agents("search").await.is_empty());
    }

    #[tokio::test]
    async fn test_system_duplicate_spawn_emits_one_spawn_event() {
        let system = AgentOSSystem::new();
        system
            .spawn_agent(AgentSpec::new("dup-event", "Duplicate Event"))
            .await
            .unwrap();

        assert!(system
            .spawn_agent(AgentSpec::new("dup-event", "Duplicate Event"))
            .await
            .is_err());

        let events = system.event_bus.read_for_agent("dup-event").await;
        let spawned_events = events
            .iter()
            .filter(|event| event.event_type == SystemEventType::AgentSpawned)
            .count();
        assert_eq!(spawned_events, 1);

        let logs = system.get_logs("dup-event", 10).await;
        let spawned_logs = logs
            .iter()
            .filter(|entry| entry.event_type == "spawned")
            .count();
        assert_eq!(spawned_logs, 1);

        system.shutdown_all().await;
    }

    #[tokio::test]
    async fn test_system_and_supervisor_share_same_bus() {
        use agentos_bus::{AgentBusTrait, AgentEnvelope};
        let system = AgentOSSystem::new();

        let env = AgentEnvelope::new("src", "target-agent", "test", b"hello".to_vec());
        system.bus.publish(env).await.unwrap();

        let drained = system.supervisor.drain_bus_for("target-agent").await;
        assert!(
            drained.is_some(),
            "supervisor should see messages published on system.bus"
        );
        assert_eq!(drained.unwrap().len(), 1);
    }
}
