use std::sync::Arc;

use agentos_kernel::agent::AgentState;
use tokio::sync::Mutex;

use crate::bus::BusClient;
use crate::error::{SdkError, SdkResult};

/// Runtime context provided to every agent.
pub struct AgentContext {
    pub agent_id: String,
    pub agent_name: String,
    pub bus: Option<BusClient>,
    pub state: Arc<Mutex<AgentState>>,
}

impl AgentContext {
    pub fn new(agent_id: impl Into<String>, agent_name: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_name: agent_name.into(),
            bus: None,
            state: Arc::new(Mutex::new(AgentState::Created)),
        }
    }

    pub fn with_bus(mut self, bus: BusClient) -> Self {
        self.bus = Some(bus);
        self
    }

    pub async fn state(&self) -> AgentState {
        self.state.lock().await.clone()
    }

    pub async fn publish(&self, topic: impl Into<String>, payload: Vec<u8>) -> SdkResult<()> {
        match &self.bus {
            Some(bus) => {
                bus.publish(topic, payload).await?;
                Ok(())
            }
            None => Err(SdkError::BusNotConnected),
        }
    }
}

impl std::fmt::Debug for AgentContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentContext")
            .field("agent_id", &self.agent_id)
            .field("agent_name", &self.agent_name)
            .field("has_bus", &self.bus.is_some())
            .finish()
    }
}
