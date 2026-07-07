use std::sync::Arc;

use agentos_kernel::agent::{AgentId, AgentSpec, AgentState};
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
}

impl AgentHandle {
    pub fn new(id: AgentId, name: String, state: Arc<Mutex<AgentState>>) -> Self {
        Self { id, name, state }
    }

    pub async fn state(&self) -> AgentState {
        self.state.lock().await.clone()
    }

    pub fn spec(&self) -> AgentSpec {
        AgentSpec::new(&self.id, &self.name)
    }
}

/// Builder for creating and spawning agents programmatically.
pub struct AgentBuilder {
    config: AgentConfig,
    tools: Vec<Box<dyn Tool>>,
}

impl AgentBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            config: AgentConfig {
                name: id.into(),
                ..Default::default()
            },
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
}
