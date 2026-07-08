#![forbid(unsafe_code)]

pub mod agent;
pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod events;
pub mod handle;
pub mod health;
pub mod journal;
pub mod manifest;
pub mod metrics;
pub mod persistence;
pub mod plugins;
pub mod runtime_config;
pub mod supervisor;
pub mod system;
pub mod tools;

pub use agent::{Agent, AgentCommand, AgentId, AgentSpec, AgentState, LifecycleEvent};
pub use agentos_bus::{AgentBusTrait, AgentEnvelope, InMemoryBus};
pub use circuit_breaker::{CallError, CircuitBreaker, CircuitBreakerConfig, CircuitState};
pub use config::AgentConfig;
pub use error::{AgentError, AgentResult};
pub use events::{
    EventBus, EventListener, EventStore, InMemoryEventStore, SqliteEventStore, SystemEvent,
    SystemEventType,
};
pub use handle::AgentHandle;
pub use health::HealthServer;
pub use journal::{
    compare_replay, DriftKind, RecordedExchange, RecordedSession, RecordedToolInvocation,
    ReplayDrift,
};
pub use manifest::{AgentManifest, ManifestRuntime, RestartPolicy};
pub use persistence::Persistence;
pub use plugins::{AgentHooks, PluginRegistry};
pub use runtime_config::RuntimeConfig;
pub use supervisor::Supervisor;
pub use system::{
    AgentExecutionStep, AgentLogEntry, AgentOSSystem, SystemError, SystemResult,
    ToolInvocationRecord,
};
pub use tools::{RuntimeTool, ToolRegistry};
