use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::agent::{Agent, AgentCommand, AgentSpec, AgentState, LifecycleEvent};
use crate::error::{AgentError, AgentResult};

#[derive(Debug, Clone)]
pub struct AgentHandle {
    pub id: String,
    spec: AgentSpec,
    state: Arc<Mutex<AgentState>>,
    cmd_tx: mpsc::Sender<AgentCommand>,
    lifecycle_tx: mpsc::Sender<LifecycleEvent>,
    last_heartbeat: Arc<AtomicU64>,
    restart_count: Arc<AtomicU64>,
}

impl AgentHandle {
    pub fn new(
        agent: Agent,
        cmd_tx: mpsc::Sender<AgentCommand>,
        lifecycle_tx: mpsc::Sender<LifecycleEvent>,
        last_heartbeat: Arc<AtomicU64>,
        restart_count: Arc<AtomicU64>,
    ) -> Self {
        let id = agent.id().to_string();
        let state = Arc::new(Mutex::new(agent.state().clone()));
        let spec = agent.spec().clone();

        Self {
            id,
            spec,
            state,
            cmd_tx,
            lifecycle_tx,
            last_heartbeat,
            restart_count,
        }
    }

    pub fn spec(&self) -> &AgentSpec {
        &self.spec
    }

    pub async fn state(&self) -> AgentState {
        self.state.lock().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        matches!(self.state().await, AgentState::Running)
    }

    pub fn last_heartbeat(&self) -> u64 {
        self.last_heartbeat.load(Ordering::Relaxed)
    }

    pub fn restart_count(&self) -> u64 {
        self.restart_count.load(Ordering::Relaxed)
    }

    pub async fn send_command(&self, command: AgentCommand) -> AgentResult<()> {
        self.cmd_tx
            .send(command)
            .await
            .map_err(|_| AgentError::ChannelClosed(self.id.clone()))
    }

    pub async fn start(&self) -> AgentResult<()> {
        self.send_command(AgentCommand::Start).await
    }

    pub async fn stop(&self) -> AgentResult<()> {
        self.send_command(AgentCommand::Stop).await
    }

    pub async fn restart(&self) -> AgentResult<()> {
        self.send_command(AgentCommand::Restart).await
    }

    pub async fn shutdown(&self) -> AgentResult<()> {
        self.send_command(AgentCommand::Shutdown).await
    }

    pub fn lifecycle_tx(&self) -> mpsc::Sender<LifecycleEvent> {
        self.lifecycle_tx.clone()
    }

    pub(crate) fn state_arc(&self) -> Arc<Mutex<AgentState>> {
        Arc::clone(&self.state)
    }

    pub(crate) async fn set_state(&self, new_state: AgentState) {
        let mut state = self.state.lock().await;
        *state = new_state;
    }
}
