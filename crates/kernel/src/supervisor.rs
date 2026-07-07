use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentos_bus::{AgentBusTrait, InMemoryBus};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{info, warn};

use crate::agent::{
    run_agent_loop_with_observers, Agent, AgentCommand, AgentId, AgentSpec, AgentState,
    LifecycleEvent, SUPERVISOR_TICK,
};
use crate::error::{AgentError, AgentResult};
use crate::handle::AgentHandle;

const AGENT_START_TIMEOUT: Duration = Duration::from_secs(5);
const AGENT_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct Supervisor {
    handles: Arc<RwLock<HashMap<AgentId, AgentHandle>>>,
    lifecycle_rx: Arc<Mutex<mpsc::Receiver<LifecycleEvent>>>,
    lifecycle_tx: mpsc::Sender<LifecycleEvent>,
    bus: Option<Arc<InMemoryBus>>,
    max_agents: usize,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        let (lifecycle_tx, lifecycle_rx) = mpsc::channel(256);
        Self {
            handles: Arc::new(RwLock::new(HashMap::new())),
            lifecycle_rx: Arc::new(Mutex::new(lifecycle_rx)),
            lifecycle_tx,
            bus: None,
            max_agents: 100,
        }
    }

    pub fn with_bus(mut self, bus: InMemoryBus) -> Self {
        self.bus = Some(Arc::new(bus));
        self
    }

    pub fn with_shared_bus(mut self, bus: Arc<InMemoryBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    pub fn with_max_agents(mut self, max: usize) -> Self {
        self.max_agents = max;
        self
    }

    pub async fn set_bus(&mut self, bus: InMemoryBus) {
        self.bus = Some(Arc::new(bus));
    }

    pub fn lifecycle_tx(&self) -> mpsc::Sender<LifecycleEvent> {
        self.lifecycle_tx.clone()
    }

    pub async fn publish_on_bus(&self, envelope: agentos_bus::AgentEnvelope) -> Option<String> {
        let bus = self.bus.as_ref()?;
        bus.publish(envelope).await.ok()
    }

    pub async fn drain_bus_for(&self, agent_id: &str) -> Option<Vec<agentos_bus::AgentEnvelope>> {
        let bus = self.bus.as_ref()?;
        Some(bus.drain_for(agent_id).await)
    }

    pub async fn spawn(&self, spec: AgentSpec) -> AgentResult<AgentHandle> {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();

        let last_heartbeat = Arc::new(AtomicU64::new(current_time_secs()));
        let restart_count = Arc::new(AtomicU64::new(0));

        let agent = Agent::new(spec.clone());

        let handle = AgentHandle::new(
            agent.clone(),
            cmd_tx.clone(),
            self.lifecycle_tx(),
            Arc::clone(&last_heartbeat),
            Arc::clone(&restart_count),
        );

        let agent_id = spec.id.clone();
        let agent_id_for_task = agent_id.clone();
        let ltx = self.lifecycle_tx();
        let state_arc = handle.state_arc();

        {
            let mut handles = self.handles.write().await;

            if handles.len() >= self.max_agents {
                return Err(AgentError::Internal(format!(
                    "max agents ({}) reached",
                    self.max_agents
                )));
            }

            if handles.contains_key(&spec.id) {
                return Err(AgentError::AlreadyRunning(spec.id.clone()));
            }

            handles.insert(agent_id.clone(), handle.clone());
        }

        tokio::spawn(async move {
            let mut inner_cmd_rx = cmd_rx;

            tokio::select! {
                biased;

                cmd = inner_cmd_rx.recv() => {
                    match cmd {
                        Some(AgentCommand::Start) => {
                            let mut a = agent.clone();
                            let _ = a.start();
                            let _ = started_tx.send(a.state().clone());
                            run_agent_loop_with_observers(
                                a,
                                ltx,
                                inner_cmd_rx,
                                Some(state_arc),
                                last_heartbeat,
                                restart_count,
                            )
                            .await;
                        }
                        Some(AgentCommand::Shutdown) => {
                            info!(agent_id = %agent_id_for_task, "agent shut down before starting");
                            let _ = started_tx.send(AgentState::Stopped);
                        }
                        Some(other) => {
                            warn!(agent_id = %agent_id_for_task, ?other, "unexpected command before start");
                            let _ = started_tx.send(AgentState::Created);
                        }
                        None => {
                            info!(agent_id = %agent_id_for_task, "agent command channel closed before start");
                            let _ = started_tx.send(AgentState::Failed("channel closed".into()));
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    warn!(agent_id = %agent_id_for_task, "agent never received start command, shutting down");
                    let _ = started_tx.send(AgentState::Failed("timeout".into()));
                }
            }
        });

        if let Err(e) = cmd_tx.send(AgentCommand::Start).await {
            warn!(agent_id = %agent_id, "failed to send start command: {e}");
            self.cleanup_failed_spawn(&agent_id, &handle).await;
            return Err(AgentError::ChannelClosed(agent_id));
        }

        match tokio::time::timeout(AGENT_START_TIMEOUT, started_rx).await {
            Ok(Ok(state)) => {
                if state == AgentState::Running {
                    let store_handle = {
                        let handles = self.handles.read().await;
                        handles
                            .get(&agent_id)
                            .cloned()
                            .ok_or_else(|| AgentError::NotFound(agent_id.clone()))?
                    };
                    store_handle.set_state(state.clone()).await;
                    info!(agent_id = %agent_id, ?state, name = %spec.name, "agent spawned and running");
                    Ok(store_handle)
                } else {
                    warn!(agent_id = %agent_id, ?state, "agent failed to start properly");
                    self.cleanup_failed_spawn(&agent_id, &handle).await;
                    Err(AgentError::CommandFailed(format!(
                        "agent failed to start: {:?}",
                        state
                    )))
                }
            }
            Ok(Err(_)) => {
                warn!(agent_id = %agent_id, "agent start channel closed without response");
                self.cleanup_failed_spawn(&agent_id, &handle).await;
                Err(AgentError::CommandFailed(
                    "agent start channel closed".into(),
                ))
            }
            Err(_) => {
                warn!(agent_id = %agent_id, "agent start timed out after 5 seconds");
                self.cleanup_failed_spawn(&agent_id, &handle).await;
                Err(AgentError::Timeout(agent_id))
            }
        }
    }

    pub async fn get(&self, id: &str) -> Option<AgentHandle> {
        let handles = self.handles.read().await;
        handles.get(id).cloned()
    }

    pub async fn stop(&self, id: &str) -> AgentResult<()> {
        self.stop_with_timeout(id, AGENT_STOP_TIMEOUT).await
    }

    pub async fn restart(&self, id: &str) -> AgentResult<()> {
        let handle = {
            let handles = self.handles.read().await;
            handles
                .get(id)
                .cloned()
                .ok_or_else(|| AgentError::NotFound(id.to_string()))?
        };

        handle.restart().await
    }

    pub async fn list(&self) -> Vec<AgentHandle> {
        let handles = self.handles.read().await;
        handles.values().cloned().collect()
    }

    pub async fn shutdown_all(&self) {
        let ids: Vec<String> = {
            let handles = self.handles.read().await;
            handles.keys().cloned().collect()
        };

        for id in &ids {
            let _ = self.stop(id).await;
        }
    }

    pub async fn try_recv_lifecycle(&self) -> Option<LifecycleEvent> {
        let mut rx = self.lifecycle_rx.lock().await;
        rx.try_recv().ok()
    }

    pub async fn recv_lifecycle(&self) -> Option<LifecycleEvent> {
        let mut rx = self.lifecycle_rx.lock().await;
        rx.recv().await
    }

    pub async fn monitor(&self) {
        let mut interval = tokio::time::interval(SUPERVISOR_TICK);
        interval.tick().await;

        loop {
            interval.tick().await;
            let now = current_time_secs();
            let stale = self.stale_handles_at(now).await;

            for handle in stale {
                warn!(agent_id = %handle.id, "agent heartbeat timeout, restarting");
                let _ = handle.restart().await;
            }
        }
    }

    async fn stale_handles_at(&self, now: u64) -> Vec<AgentHandle> {
        let handles_lock = self.handles.read().await;
        handles_lock
            .iter()
            .filter(|(_, h)| {
                let last = h.last_heartbeat();
                let timeout = h.spec().heartbeat_timeout_secs;
                last > 0 && timeout > 0 && now.saturating_sub(last) > timeout
            })
            .map(|(_, h)| h.clone())
            .collect()
    }

    pub async fn remove(&self, id: &str) -> AgentResult<()> {
        let mut handles = self.handles.write().await;
        let handle = handles
            .remove(id)
            .ok_or_else(|| AgentError::NotFound(id.to_string()))?;
        let _ = handle.shutdown().await;
        info!(agent_id = %id, "agent removed from supervisor");
        Ok(())
    }

    pub(crate) async fn stop_with_timeout(&self, id: &str, timeout: Duration) -> AgentResult<()> {
        let agent_id = id.to_string();

        match tokio::time::timeout(timeout, async {
            let handle = {
                let handles = self.handles.read().await;
                handles
                    .get(id)
                    .cloned()
                    .ok_or_else(|| AgentError::NotFound(id.to_string()))?
            };

            if !handle.is_running().await {
                return Err(AgentError::NotRunning(id.to_string()));
            }

            handle.stop().await?;

            loop {
                if handle.state().await.is_terminal() {
                    return Ok(());
                }

                tokio::time::sleep(STOP_POLL_INTERVAL).await;
            }
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(AgentError::Timeout(agent_id)),
        }
    }

    async fn cleanup_failed_spawn(&self, id: &str, handle: &AgentHandle) {
        let _ = handle.shutdown().await;
        let mut handles = self.handles.write().await;
        handles.remove(id);
    }
}

fn current_time_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Clone for Supervisor {
    fn clone(&self) -> Self {
        Self {
            handles: Arc::clone(&self.handles),
            lifecycle_rx: Arc::clone(&self.lifecycle_rx),
            lifecycle_tx: self.lifecycle_tx.clone(),
            bus: self.bus.clone(),
            max_agents: self.max_agents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentSpec, AgentState};
    use std::time::Duration;

    async fn wait_until_restart_count(handle: &AgentHandle, expected: u64) {
        for _ in 0..50 {
            if handle.restart_count() == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "restart count should become {expected}, got {}",
            handle.restart_count()
        );
    }

    async fn wait_until_state(
        handle: &AgentHandle,
        expected: fn(&AgentState) -> bool,
    ) -> AgentState {
        for _ in 0..50 {
            let state = handle.state().await;
            if expected(&state) {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        handle.state().await
    }

    #[tokio::test]
    async fn test_supervisor_spawn_and_stop() {
        let sup = Supervisor::new();
        let spec = AgentSpec::new("test-spawn", "Test Spawn");
        let handle = sup.spawn(spec).await.unwrap();
        assert!(handle.is_running().await);
        sup.stop("test-spawn").await.unwrap();
        assert!(!handle.is_running().await);
    }

    #[tokio::test]
    async fn test_supervisor_duplicate_spawn_fails() {
        let sup = Supervisor::new();
        let spec = AgentSpec::new("test-dup", "Test Dup");
        sup.spawn(spec).await.unwrap();
        let spec2 = AgentSpec::new("test-dup", "Test Dup");
        assert!(sup.spawn(spec2).await.is_err());
    }

    #[tokio::test]
    async fn test_supervisor_list_agents() {
        let sup = Supervisor::new();
        sup.spawn(AgentSpec::new("list-1", "List 1")).await.unwrap();
        sup.spawn(AgentSpec::new("list-2", "List 2")).await.unwrap();
        let agents = sup.list().await;
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_supervisor_get_agent() {
        let sup = Supervisor::new();
        sup.spawn(AgentSpec::new("get-test", "Get Test"))
            .await
            .unwrap();
        let handle = sup.get("get-test").await;
        assert!(handle.is_some());
        let missing = sup.get("not-exists").await;
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_supervisor_shutdown_all() {
        let sup = Supervisor::new();
        sup.spawn(AgentSpec::new("shut-1", "Shut 1")).await.unwrap();
        sup.spawn(AgentSpec::new("shut-2", "Shut 2")).await.unwrap();
        sup.shutdown_all().await;
        let agents = sup.list().await;
        assert_eq!(agents.len(), 2);
        for handle in &agents {
            assert!(
                !handle.is_running().await,
                "agent {} should be stopped",
                handle.id
            );
        }
    }

    #[tokio::test]
    async fn test_supervisor_remove_agent() {
        let sup = Supervisor::new();
        sup.spawn(AgentSpec::new("remove-test", "Remove Test"))
            .await
            .unwrap();
        sup.remove("remove-test").await.unwrap();
        assert!(sup.get("remove-test").await.is_none());
    }

    #[tokio::test]
    async fn test_supervisor_clone_stop_works() {
        let sup = Supervisor::new();
        sup.spawn(AgentSpec::new("clone-stop", "Clone Stop Test"))
            .await
            .unwrap();

        let cloned = sup.clone();
        let result = tokio::time::timeout(Duration::from_secs(3), cloned.stop("clone-stop")).await;
        assert!(result.is_ok(), "clone.stop should not hang");
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_stop_does_not_consume_lifecycle_event() {
        let sup = Supervisor::new();
        sup.spawn(AgentSpec::new("watch-stop", "Watch Stop Test"))
            .await
            .unwrap();

        let started = tokio::time::timeout(Duration::from_secs(1), sup.recv_lifecycle())
            .await
            .expect("started event should arrive")
            .expect("lifecycle channel should remain open");
        assert_eq!(started, LifecycleEvent::Started("watch-stop".into()));

        sup.stop("watch-stop").await.unwrap();

        let stopped = tokio::time::timeout(Duration::from_secs(1), sup.recv_lifecycle())
            .await
            .expect("stopped event should remain visible to watchers")
            .expect("lifecycle channel should remain open");
        assert_eq!(stopped, LifecycleEvent::Stopped("watch-stop".into()));
    }

    #[tokio::test]
    async fn test_concurrent_stops_do_not_steal_lifecycle_events() {
        let sup = std::sync::Arc::new(Supervisor::new());
        sup.spawn(AgentSpec::new("multi-stop-1", "Multi Stop 1"))
            .await
            .unwrap();
        sup.spawn(AgentSpec::new("multi-stop-2", "Multi Stop 2"))
            .await
            .unwrap();

        let first = sup.clone();
        let second = sup.clone();
        let result = tokio::time::timeout(Duration::from_secs(2), async move {
            tokio::join!(first.stop("multi-stop-1"), second.stop("multi-stop-2"))
        })
        .await;

        assert!(result.is_ok(), "concurrent stops should not hang");
        let (first_result, second_result) = result.unwrap();
        assert!(first_result.is_ok());
        assert!(second_result.is_ok());
    }

    #[tokio::test]
    async fn test_stop_timeout_returns_error() {
        let sup = Supervisor::new();
        let handle = sup
            .spawn(AgentSpec::new("stop-timeout", "Stop Timeout Test"))
            .await
            .unwrap();
        let state_arc = handle.state_arc();
        let guard = state_arc.lock().await;

        let result = sup
            .stop_with_timeout("stop-timeout", Duration::from_millis(20))
            .await;

        assert_eq!(
            result,
            Err(crate::error::AgentError::Timeout("stop-timeout".into()))
        );

        drop(guard);
        sup.stop("stop-timeout").await.unwrap();
    }

    #[tokio::test]
    async fn test_restart_does_not_block_concurrent_list() {
        let sup = std::sync::Arc::new(Supervisor::new());
        sup.spawn(AgentSpec::new("lock-test", "Lock Test Agent"))
            .await
            .unwrap();

        let sup_for_restart = sup.clone();
        let restart_handle =
            tokio::spawn(async move { sup_for_restart.restart("lock-test").await });

        tokio::time::sleep(Duration::from_millis(20)).await;

        let list_result = tokio::time::timeout(Duration::from_secs(2), sup.list()).await;
        assert!(
            list_result.is_ok(),
            "list should not be blocked by restart holding handles lock"
        );
        assert_eq!(list_result.unwrap().len(), 1);

        assert!(restart_handle.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_heartbeat_timeout_uses_agent_spec() {
        let sup = Supervisor::new();
        let mut spec = AgentSpec::new("heartbeat-timeout", "Heartbeat Timeout Test");
        spec.heartbeat_timeout_secs = 1;

        let handle = sup.spawn(spec).await.unwrap();
        let stale_at = handle.last_heartbeat() + 2;

        let stale = sup.stale_handles_at(stale_at).await;
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].id, "heartbeat-timeout");
    }

    #[tokio::test]
    async fn test_restart_count_tracks_successful_soft_restarts() {
        let sup = Supervisor::new();
        let handle = sup
            .spawn(AgentSpec::new("restart-count", "Restart Count Test"))
            .await
            .unwrap();

        sup.restart("restart-count").await.unwrap();
        wait_until_restart_count(&handle, 1).await;

        assert!(handle.is_running().await);
    }

    #[tokio::test]
    async fn test_max_restarts_exhaustion_fails_agent() {
        let sup = Supervisor::new();
        let mut spec = AgentSpec::new("restart-exhaust", "Restart Exhaustion Test");
        spec.max_restarts = 1;
        let handle = sup.spawn(spec).await.unwrap();

        sup.restart("restart-exhaust").await.unwrap();
        wait_until_restart_count(&handle, 1).await;

        sup.restart("restart-exhaust").await.unwrap();
        let state = wait_until_state(&handle, |state| matches!(state, AgentState::Failed(_))).await;
        assert_eq!(
            state,
            AgentState::Failed("max restart count exceeded".into())
        );
        assert_eq!(handle.restart_count(), 1);
    }

    #[tokio::test]
    async fn test_shutdown_command_reaches_terminal_state() {
        let sup = Supervisor::new();
        let handle = sup
            .spawn(AgentSpec::new(
                "shutdown-correct",
                "Shutdown Correctness Test",
            ))
            .await
            .unwrap();

        handle.shutdown().await.unwrap();

        let state = wait_until_state(&handle, |state| matches!(state, AgentState::Stopped)).await;
        assert_eq!(state, AgentState::Stopped);
    }

    /// Documents a real recovery boundary: soft restart works only while the
    /// agent loop task is alive. Once the loop has exited (after stop), the
    /// command channel is closed and restart must fail cleanly with an error
    /// instead of pretending to recover.
    #[tokio::test]
    async fn test_restart_after_agent_loop_exit_fails_cleanly() {
        let sup = Supervisor::new();
        let handle = sup
            .spawn(AgentSpec::new("dead-loop", "Dead Loop Test"))
            .await
            .unwrap();

        sup.stop("dead-loop").await.unwrap();
        let state = wait_until_state(&handle, |state| state.is_terminal()).await;
        assert!(state.is_terminal());

        // The loop has exited; a restart cannot resurrect it.
        let result = sup.restart("dead-loop").await;
        assert!(
            result.is_err(),
            "restart of an exited agent loop must fail, got {result:?}"
        );
        assert!(!handle.is_running().await);
    }

    /// The monitor restarts agents whose heartbeat is stale. This exercises
    /// one full monitor pass against a live agent whose heartbeat is
    /// artificially aged, without waiting for real timeouts.
    #[tokio::test]
    async fn test_stale_agent_detected_and_soft_restarted() {
        let sup = Supervisor::new();
        let mut spec = AgentSpec::new("stale-agent", "Stale Agent Test");
        spec.heartbeat_timeout_secs = 1;
        let handle = sup.spawn(spec).await.unwrap();

        // Simulate a stale heartbeat far in the future relative to the last
        // beat, then apply exactly what one monitor tick does.
        let stale_at = handle.last_heartbeat() + 60;
        let stale = sup.stale_handles_at(stale_at).await;
        assert_eq!(stale.len(), 1);

        for stale_handle in stale {
            stale_handle.restart().await.unwrap();
        }

        wait_until_restart_count(&handle, 1).await;
        assert!(handle.is_running().await);
        assert_eq!(handle.restart_count(), 1);
    }
}
