use std::sync::Arc;

use agentos_kernel::{
    agent::{AgentId, AgentSpec, AgentState},
    AgentOSSystem, Supervisor,
};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::SdkResult;
use crate::tool::Tool;

/// Configuration for an agent built with the SDK.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub prompt: String,
    pub capabilities: Vec<String>,
    pub max_restarts: u32,
    pub heartbeat_timeout_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "untitled-agent".into(),
            prompt: "You are a helpful AI agent.".into(),
            capabilities: Vec::new(),
            max_restarts: 5,
            heartbeat_timeout_secs: 30,
        }
    }
}

/// A handle to a running agent.
#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub id: AgentId,
    pub name: String,
    state: Arc<Mutex<AgentState>>,
    kernel_handle: Option<agentos_kernel::AgentHandle>,
}

impl AgentHandle {
    pub fn new(id: AgentId, name: String, state: Arc<Mutex<AgentState>>) -> Self {
        Self {
            id,
            name,
            state,
            kernel_handle: None,
        }
    }

    pub fn from_kernel(handle: agentos_kernel::AgentHandle) -> Self {
        let spec = handle.spec().clone();
        Self {
            id: handle.id.clone(),
            name: spec.name,
            state: Arc::new(Mutex::new(AgentState::Created)),
            kernel_handle: Some(handle),
        }
    }

    pub async fn state(&self) -> AgentState {
        if let Some(handle) = &self.kernel_handle {
            return handle.state().await;
        }
        self.state.lock().await.clone()
    }

    pub fn spec(&self) -> AgentSpec {
        if let Some(handle) = &self.kernel_handle {
            return handle.spec().clone();
        }
        AgentSpec::new(&self.id, &self.name)
    }

    pub async fn stop(&self) -> SdkResult<()> {
        if let Some(handle) = &self.kernel_handle {
            handle.stop().await?;
            let mut state = self.state.lock().await;
            *state = AgentState::Stopped;
        }
        Ok(())
    }
}

/// Builder for creating and spawning agents programmatically.
pub struct AgentBuilder {
    id_hint: String,
    config: AgentConfig,
    tools: Vec<Box<dyn Tool>>,
}

impl AgentBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            config: AgentConfig {
                name: id.clone(),
                ..Default::default()
            },
            id_hint: id,
            tools: Vec::new(),
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.prompt = prompt.into();
        self
    }

    pub fn capability(mut self, cap: impl Into<String>) -> Self {
        self.config.capabilities.push(cap.into());
        self
    }

    pub fn capabilities(mut self, caps: Vec<impl Into<String>>) -> Self {
        for cap in caps {
            self.config.capabilities.push(cap.into());
        }
        self
    }

    pub fn max_restarts(mut self, n: u32) -> Self {
        self.config.max_restarts = n;
        self
    }

    pub fn tool(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.push(tool);
        self
    }

    pub async fn spawn_on_supervisor(self, supervisor: &Supervisor) -> SdkResult<AgentHandle> {
        let tool_count = self.tools.len();
        let id = self.id_hint.clone();
        let spec = self.into_spec(id);
        let handle = supervisor.spawn(spec).await?;
        tracing::info!(
            agent_id = %handle.id,
            tools = tool_count,
            "agent spawned via SDK supervisor"
        );
        Ok(AgentHandle::from_kernel(handle))
    }

    pub async fn spawn_on_system(self, system: &AgentOSSystem) -> SdkResult<AgentHandle> {
        let tool_count = self.tools.len();
        let id = self.id_hint.clone();
        let spec = self.into_spec(id);
        let handle = system.spawn_agent(spec).await?;
        tracing::info!(
            agent_id = %handle.id,
            tools = tool_count,
            "agent spawned via SDK system"
        );
        Ok(AgentHandle::from_kernel(handle))
    }

    pub async fn spawn(self) -> SdkResult<AgentHandle> {
        let id = Uuid::new_v4().to_string();
        let state = Arc::new(Mutex::new(AgentState::Created));

        let handle = AgentHandle::new(id, self.config.name.clone(), state);

        tracing::info!(
            agent_id = %handle.id,
            name = %handle.name,
            capabilities = ?self.config.capabilities,
            "agent spawned via SDK"
        );

        Ok(handle)
    }

    fn into_spec(self, id: impl Into<String>) -> AgentSpec {
        let mut capabilities = self.config.capabilities;
        for tool in &self.tools {
            let capability = format!("tool:{}", tool.name());
            if !capabilities.contains(&capability) {
                capabilities.push(capability);
            }
        }

        AgentSpec {
            id: id.into(),
            name: self.config.name,
            prompt: self.config.prompt,
            capabilities,
            max_restarts: self.config.max_restarts,
            heartbeat_timeout_secs: self.config.heartbeat_timeout_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builder_defaults() {
        let handle = AgentBuilder::new("test-agent")
            .spawn()
            .await
            .expect("spawn should succeed");

        assert_eq!(handle.name, "test-agent");
        assert!(!handle.id.is_empty());
    }

    #[tokio::test]
    async fn test_builder_with_capabilities() {
        let handle = AgentBuilder::new("cap-test")
            .name("Capability Test")
            .capability("search_web")
            .capability("memory")
            .spawn()
            .await
            .expect("spawn should succeed");

        assert_eq!(handle.name, "Capability Test");
    }

    #[tokio::test]
    async fn test_builder_with_config() {
        let handle = AgentBuilder::new("config-test")
            .name("Config Test Agent")
            .prompt("You are a test agent.")
            .max_restarts(3)
            .capabilities(vec!["tool_a", "tool_b"])
            .spawn()
            .await
            .expect("spawn should succeed");

        assert_eq!(handle.name, "Config Test Agent");
    }

    #[tokio::test]
    async fn test_builder_spawns_on_supervisor() {
        let supervisor = Supervisor::new();
        let handle = AgentBuilder::new("supervised-sdk-agent")
            .prompt("You are supervised.")
            .spawn_on_supervisor(&supervisor)
            .await
            .expect("supervisor spawn should succeed");

        assert_eq!(handle.id, "supervised-sdk-agent");
        assert_eq!(handle.state().await, AgentState::Running);
        assert!(supervisor.get("supervised-sdk-agent").await.is_some());

        supervisor.stop("supervised-sdk-agent").await.unwrap();
    }

    #[tokio::test]
    async fn test_builder_spawns_on_system_and_records_trace() {
        let system = AgentOSSystem::new();
        let handle = AgentBuilder::new("system-sdk-agent")
            .name("System SDK Agent")
            .spawn_on_system(&system)
            .await
            .expect("system spawn should succeed");

        assert_eq!(handle.id, "system-sdk-agent");
        assert_eq!(handle.state().await, AgentState::Running);
        assert!(system.discover_agents("missing").await.is_empty());
        assert_eq!(system.get_logs("system-sdk-agent", 10).await.len(), 1);

        system.shutdown_all().await;
    }
}
