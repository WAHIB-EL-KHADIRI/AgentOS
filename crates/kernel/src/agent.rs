use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};

use crate::error::{AgentError, AgentResult};

/// An agent's identity, and deliberately **not** a filesystem path.
///
/// Ids reach persistence, so the distinction has teeth: `journal_path` and
/// `trace_path` flatten anything outside `[A-Za-z0-9_-]` into `_`, which is a
/// correct guard precisely because an id is an opaque label -- nothing is lost
/// by collapsing separators that were never meaningful to begin with.
///
/// If a future requirement ever wants ids to carry real structure (nested
/// namespaces, say), flattening stops being right and the persistence layer
/// has to move to canonicalise-then-contain: resolve the candidate path and
/// verify it is still inside the data dir. Do not widen the character set on
/// its own and leave the flattening in place -- that combination silently
/// merges distinct ids onto one file.
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

/// Emit a lifecycle event without ever parking the agent loop.
///
/// The lifecycle channel is bounded and production code is free to leave it
/// undrained -- `cli/dev.rs` and `cli/repl.rs` both spawn agents and never call
/// `recv_lifecycle`. A blocking `send` inside the `select!` below would then park
/// the loop mid-arm, so `cmd_rx` stops being polled and `Stop`, `Shutdown` and
/// `Restart` become undeliverable for the remaining life of the process. At one
/// heartbeat per `HEARTBEAT_INTERVAL` a single agent fills the channel in minutes,
/// which makes that a routine outcome rather than a failure case.
///
/// Emission is therefore lossy by design. Nothing load-bearing depends on it:
/// `Supervisor::stale_handles_at` reads the `last_heartbeat` atomic, which is
/// stored before this is ever called. Only observers can miss an event, and a
/// dropped one is logged.
fn emit_lifecycle(tx: &mpsc::Sender<LifecycleEvent>, agent_id: &str, event: LifecycleEvent) {
    // Heartbeats are periodic, so a full channel would log on every tick. The
    // terminal events are one-shot and worth a louder line.
    let is_heartbeat = matches!(event, LifecycleEvent::Heartbeat(_));

    match tx.try_send(event) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(dropped)) => {
            if is_heartbeat {
                tracing::debug!(
                    agent_id = %agent_id,
                    "lifecycle channel full, heartbeat dropped"
                );
            } else {
                tracing::warn!(
                    agent_id = %agent_id,
                    event = ?dropped,
                    "lifecycle channel full, event dropped; no consumer is draining it"
                );
            }
        }
        Err(mpsc::error::TrySendError::Closed(dropped)) => {
            tracing::debug!(
                agent_id = %agent_id,
                event = ?dropped,
                "lifecycle channel closed, event dropped"
            );
        }
    }
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

    emit_lifecycle(
        &lifecycle_tx,
        &agent_id,
        LifecycleEvent::Started(agent_id.clone()),
    );

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
                emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Heartbeat(agent_id.clone()));
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(AgentCommand::Stop) => {
                        tracing::info!(agent_id = %agent_id, "stopping agent");
                        let _ = agent.stop();
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Stopped;
                        }
                        emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Stopped(agent_id.clone()));
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
                            emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Failed(agent_id.clone(), msg));
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
                            emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Failed(agent_id.clone(), msg));
                            break;
                        }
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Running;
                        }
                        emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Started(agent_id.clone()));
                    }
                    Some(AgentCommand::Shutdown) => {
                        tracing::info!(agent_id = %agent_id, "shutting down agent");
                        let _ = agent.stop();
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Stopped;
                        }
                        emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Stopped(agent_id.clone()));
                        break;
                    }
                    None => {
                        tracing::warn!(agent_id = %agent_id, "command channel closed");
                        agent.fail("command channel closed unexpectedly");
                        if let Some(ref s) = state_arc {
                            *s.lock().await = AgentState::Failed("command channel closed".into());
                        }
                        emit_lifecycle(&lifecycle_tx, &agent_id, LifecycleEvent::Failed(agent_id.clone(), "command channel closed".into()));
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

    /// A full lifecycle channel must not cost the agent its ability to be
    /// stopped.
    ///
    /// This is reachable without any failure at all: `cli/dev.rs` and
    /// `cli/repl.rs` spawn agents and never drain the channel, so the heartbeat
    /// alone fills it after `256 * HEARTBEAT_INTERVAL` -- about twenty minutes of
    /// entirely normal operation. Before the fix the heartbeat arm of the
    /// `select!` parked on a blocking `send`, `cmd_rx` stopped being polled, and
    /// `Stop` could never be delivered again.
    ///
    /// Capacity 1 reproduces the same state as capacity 256 without the wait:
    /// the initial `Started` fills the channel outright, so the very next
    /// emission -- here the `Stopped` on the way out -- meets a full channel.
    /// That is the same blocking send the heartbeat arm performs, reached
    /// without depending on `tokio`'s `test-util` time control.
    #[tokio::test]
    async fn test_full_lifecycle_channel_does_not_make_an_agent_unstoppable() {
        let (lifecycle_tx, _lifecycle_rx) = mpsc::channel(1);
        let (cmd_tx, cmd_rx) = mpsc::channel(8);

        let agent = Agent::new(AgentSpec::new("unstoppable", "Unstoppable"));
        let loop_handle = tokio::spawn(run_agent_loop_inner(
            agent,
            lifecycle_tx,
            cmd_rx,
            None,
            None,
            None,
        ));

        // Let `Started` land, which leaves the channel full. `_lifecycle_rx` is
        // deliberately never drained: that is exactly what cli/dev.rs does.
        tokio::task::yield_now().await;

        cmd_tx
            .send(AgentCommand::Stop)
            .await
            .expect("agent loop must still be receiving commands");

        let stopped = tokio::time::timeout(Duration::from_secs(5), loop_handle).await;
        assert!(
            stopped.is_ok(),
            "agent ignored Stop: the loop is parked emitting a lifecycle event \
             into a full channel and no longer polls cmd_rx"
        );
    }

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
