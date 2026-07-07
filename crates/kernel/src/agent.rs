use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::error::{AgentError, AgentResult};

pub type AgentId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Created,
    Running,
    Stopped,
    Degraded(String),
    Failed(String),
}

impl AgentState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Stopped | AgentState::Failed(_))
    }

    pub fn is_running(&self) -> bool {
        matches!(self, AgentState::Running)
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentState::Created => write!(f, "created"),
            AgentState::Running => write!(f, "running"),
            AgentState::Stopped => write!(f, "stopped"),
            AgentState::Degraded(_) => write!(f, "degraded"),
            AgentState::Failed(_) => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    pub id: AgentId,
    pub name: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub max_restarts: u32,
    #[serde(default = "default_heartbeat_timeout")]
    pub heartbeat_timeout_secs: u64,
}

const fn default_heartbeat_timeout() -> u64 {
    30
}

impl AgentSpec {
    pub fn new(id: impl Into<AgentId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            prompt: String::new(),
            capabilities: Vec::new(),
            max_restarts: 5,
            heartbeat_timeout_secs: 30,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCommand {
    Start,
    Stop,
    /// Soft in-process restart: reset runtime state and uptime without
    /// replacing the running task or emitting a Stopped lifecycle event.
    Restart,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleEvent {
    Started(AgentId),
    Stopped(AgentId),
    Degraded(AgentId, String),
    Failed(AgentId, String),
    Heartbeat(AgentId),
}

#[derive(Debug, Clone)]
pub struct Agent {
    spec: AgentSpec,
    state: AgentState,
    pub(crate) uptime_secs: u64,
    pub(crate) restart_count: u32,
}

impl Agent {
    pub fn new(spec: AgentSpec) -> Self {
        Self {
            spec,
            state: AgentState::Created,
            uptime_secs: 0,
            restart_count: 0,
        }
    }

    pub fn id(&self) -> &str {
        &self.spec.id
    }

    pub fn spec(&self) -> &AgentSpec {
        &self.spec
    }

    pub fn state(&self) -> &AgentState {
        &self.state
    }

    pub fn start(&mut self) -> AgentResult<()> {
        match &self.state {
            AgentState::Running => Err(AgentError::AlreadyRunning(self.spec.id.clone())),
            _ => {
                self.state = AgentState::Running;
                self.uptime_secs = 0;
                Ok(())
            }
        }
    }

    pub fn stop(&mut self) -> AgentResult<()> {
        match &self.state {
            AgentState::Running | AgentState::Degraded(_) => {
                self.state = AgentState::Stopped;
                Ok(())
            }
            _ => Err(AgentError::NotRunning(self.spec.id.clone())),
        }
    }

    pub fn degrade(&mut self, reason: impl Into<String>) {
        self.state = AgentState::Degraded(reason.into());
    }

    pub fn fail(&mut self, reason: impl Into<String>) {
        self.state = AgentState::Failed(reason.into());
    }

    pub fn restart_count(&self) -> u32 {
        self.restart_count
    }

    pub fn uptime_secs(&self) -> u64 {
        self.uptime_secs
    }
}

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
pub(crate) const SUPERVISOR_TICK: Duration = Duration::from_secs(10);

pub async fn run_agent_loop(
    agent: Agent,
    lifecycle_tx: mpsc::Sender<LifecycleEvent>,
    cmd_rx: mpsc::Receiver<AgentCommand>,
    state_arc: Option<Arc<Mutex<AgentState>>>,
) {
    run_agent_loop_inner(agent, lifecycle_tx, cmd_rx, state_arc, None, None).await;
}

pub(crate) async fn run_agent_loop_with_observers(
    agent: Agent,
    lifecycle_tx: mpsc::Sender<LifecycleEvent>,
    cmd_rx: mpsc::Receiver<AgentCommand>,
    state_arc: Option<Arc<Mutex<AgentState>>>,
    last_heartbeat: Arc<AtomicU64>,
    restart_count: Arc<AtomicU64>,
) {
    run_agent_loop_inner(
        agent,
        lifecycle_tx,
        cmd_rx,
        state_arc,
        Some(last_heartbeat),
        Some(restart_count),
    )
    .await;
}

async fn run_agent_loop_inner(
    mut agent: Agent,
    lifecycle_tx: mpsc::Sender<LifecycleEvent>,
    mut cmd_rx: mpsc::Receiver<AgentCommand>,
    state_arc: Option<Arc<Mutex<AgentState>>>,
    last_heartbeat: Option<Arc<AtomicU64>>,
    restart_count: Option<Arc<AtomicU64>>,
) {
    tracing::info!(agent_id = %agent.id(), "agent loop started");

    let agent_id = agent.id().to_string();

    if let Some(ref s) = state_arc {
        *s.lock().await = AgentState::Running;
    }
    if let Some(ref last) = last_heartbeat {
        last.store(current_time_secs(), Ordering::Relaxed);
    }

    let _ = lifecycle_tx
        .send(LifecycleEvent::Started(agent_id.clone()))
        .await;

    let mut heartbeat_interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat_interval.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                tracing::trace!(agent_id = %agent_id, "heartbeat");
                agent.uptime_secs += HEARTBEAT_INTERVAL.as_secs();
                if let Some(ref last) = last_heartbeat {
                    last.store(current_time_secs(), Ordering::Relaxed);
                }
                let _ = lifecycle_tx.send(LifecycleEvent::Heartbeat(agent_id.clone())).await;
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(AgentCommand::Stop) => {
                        tracing::info!(agent_id = %agent_id, "stopping agent");
                        let _ = agent.stop();
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Stopped;
                        }
                        let _ = lifecycle_tx.send(LifecycleEvent::Stopped(agent_id.clone())).await;
                        break;
                    }
                    Some(AgentCommand::Restart) => {
                        tracing::info!(agent_id = %agent_id, "restarting agent");
                        if agent.restart_count >= agent.spec().max_restarts {
                            let msg = "max restart count exceeded".to_string();
                            agent.fail(&msg);
                            if let Some(ref s) = state_arc {
                                *s.lock().await = AgentState::Failed(msg.clone());
                            }
                            let _ = lifecycle_tx.send(LifecycleEvent::Failed(agent_id.clone(), msg)).await;
                            break;
                        }
                        agent.restart_count += 1;
                        if let Some(ref restarts) = restart_count {
                            restarts.store(agent.restart_count as u64, Ordering::Relaxed);
                        }
                        // This is a soft in-process restart: the agent loop stays alive,
                        // but runtime state and uptime are reset before continuing.
                        let _ = agent.stop();
                        if let Err(error) = agent.start() {
                            let msg = format!("restart failed: {error}");
                            agent.fail(&msg);
                            if let Some(ref s) = state_arc {
                                *s.lock().await = AgentState::Failed(msg.clone());
                            }
                            let _ = lifecycle_tx.send(LifecycleEvent::Failed(agent_id.clone(), msg)).await;
                            break;
                        }
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Running;
                        }
                        let _ = lifecycle_tx.send(LifecycleEvent::Started(agent_id.clone())).await;
                    }
                    Some(AgentCommand::Shutdown) => {
                        tracing::info!(agent_id = %agent_id, "shutting down agent");
                        let _ = agent.stop();
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Stopped;
                        }
                        let _ = lifecycle_tx.send(LifecycleEvent::Stopped(agent_id.clone())).await;
                        break;
                    }
                    None => {
                        tracing::warn!(agent_id = %agent_id, "command channel closed");
                        agent.fail("command channel closed unexpectedly");
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Failed("command channel closed".into());
                        }
                        let _ = lifecycle_tx.send(LifecycleEvent::Failed(agent_id.clone(), "command channel closed".into())).await;
                        break;
                    }
                    Some(AgentCommand::Start) => {
                        let _ = agent.start();
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Running;
                        }
                    }
                }
            }
        }
    }

    tracing::info!(agent_id = %agent_id, "agent loop ended");
}

fn current_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let spec = AgentSpec::new("test-1", "Test Agent");
        let agent = Agent::new(spec);
        assert_eq!(agent.state(), &AgentState::Created);
        assert_eq!(agent.uptime_secs(), 0);
        assert_eq!(agent.restart_count(), 0);
    }

    #[test]
    fn test_agent_start_stop() {
        let spec = AgentSpec::new("test-2", "Test Agent");
        let mut agent = Agent::new(spec);
        assert!(agent.start().is_ok());
        assert_eq!(agent.state(), &AgentState::Running);
        assert!(agent.stop().is_ok());
        assert_eq!(agent.state(), &AgentState::Stopped);
    }

    #[test]
    fn test_agent_double_start_fails() {
        let spec = AgentSpec::new("test-3", "Test Agent");
        let mut agent = Agent::new(spec);
        assert!(agent.start().is_ok());
        assert!(agent.start().is_err());
    }

    #[test]
    fn test_agent_stop_not_running_fails() {
        let spec = AgentSpec::new("test-4", "Test Agent");
        let mut agent = Agent::new(spec);
        assert!(agent.stop().is_err());
    }

    #[test]
    fn test_agent_degade_and_fail() {
        let spec = AgentSpec::new("test-5", "Test Agent");
        let mut agent = Agent::new(spec);
        agent.degrade("slow response");
        assert_eq!(agent.state(), &AgentState::Degraded("slow response".into()));
        agent.fail("crashed");
        assert_eq!(agent.state(), &AgentState::Failed("crashed".into()));
    }

    #[test]
    fn test_agent_state_is_terminal() {
        assert!(!AgentState::Created.is_terminal());
        assert!(!AgentState::Running.is_terminal());
        assert!(AgentState::Stopped.is_terminal());
        assert!(AgentState::Failed("".into()).is_terminal());
    }

    #[test]
    fn test_agent_spec_builder() {
        let mut spec = AgentSpec::new("test-6", "Test Agent");
        spec.prompt = "You are helpful".into();
        spec.capabilities = vec!["memory".into()];
        assert_eq!(spec.id, "test-6");
        assert_eq!(spec.name, "Test Agent");
        assert_eq!(spec.prompt, "You are helpful");
        assert!(spec.capabilities.contains(&"memory".into()));
    }
}
