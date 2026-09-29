use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::error::{SendTimeoutError, TrySendError};
use tokio::sync::{mpsc, Mutex};

use crate::agent::{Agent, AgentCommand, AgentSpec, AgentState, LifecycleEvent};
use crate::error::{AgentError, AgentResult};

/// Upper bound on how long a command send waits for room in an agent's
/// bounded command channel. An agent that has stopped draining its channel
/// degrades itself; it must never block the caller indefinitely.
const COMMAND_SEND_TIMEOUT: Duration = Duration::from_secs(5);

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

    /// Sends a command to the agent loop, waiting at most
    /// [`COMMAND_SEND_TIMEOUT`] for room in the bounded channel.
    ///
    /// The wait is bounded on purpose: an agent that stops draining its
    /// channel used to block every caller of this method forever.
    pub async fn send_command(&self, command: AgentCommand) -> AgentResult<()> {
        match self
            .cmd_tx
            .send_timeout(command, COMMAND_SEND_TIMEOUT)
            .await
        {
            Ok(()) => Ok(()),
            Err(SendTimeoutError::Timeout(_)) => Err(AgentError::Timeout(self.id.clone())),
            Err(SendTimeoutError::Closed(_)) => Err(AgentError::ChannelClosed(self.id.clone())),
        }
    }

    /// Non-blocking counterpart to [`AgentHandle::send_command`], for callers
    /// that must not be delayed at all by one unresponsive agent.
    ///
    /// Returns [`AgentError::CommandFailed`] when the channel is full (the
    /// agent loop is not draining commands) and [`AgentError::ChannelClosed`]
    /// when the loop has exited.
    pub fn try_send_command(&self, command: AgentCommand) -> AgentResult<()> {
        self.cmd_tx.try_send(command).map_err(|error| match error {
            TrySendError::Full(_) => AgentError::CommandFailed(format!(
                "agent '{}' is not draining its command channel (capacity {})",
                self.id,
                self.cmd_tx.max_capacity()
            )),
            TrySendError::Closed(_) => AgentError::ChannelClosed(self.id.clone()),
        })
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

    /// Requests a restart without ever waiting for channel capacity.
    ///
    /// This is what the supervision loop uses: a full or closed channel must
    /// degrade the affected agent only, never stall supervision of the others.
    pub fn try_restart(&self) -> AgentResult<()> {
        self.try_send_command(AgentCommand::Restart)
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
}
