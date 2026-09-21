use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use agentos_bus::{AgentBusTrait, InMemoryBus};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{error, info, warn};

use crate::agent::{
    run_agent_loop_with_observers, Agent, AgentId, AgentReadiness, AgentSpec, LifecycleEvent,
    SUPERVISOR_TICK,
};
use crate::error::{AgentError, AgentResult};
use crate::handle::AgentHandle;

const AGENT_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// RAII-owned reservation for a readiness-pending spawn.
///
/// The id is inserted into the shared pending set at admission time and removed
/// exactly once: either explicitly on the success path (followed by `defuse`,
/// so `Drop` becomes a no-op) or by `Drop` on every other exit — probe failure,
/// timeout, `Agent::start` failure, early return, or future cancellation.
/// `Drop` only locks a short synchronous mutex, so no async cleanup is needed.
#[derive(Debug)]
struct PendingReservation {
    pending: Arc<std::sync::Mutex<HashSet<AgentId>>>,
    id: AgentId,
    committed: bool,
}

impl PendingReservation {
    fn new(pending: Arc<std::sync::Mutex<HashSet<AgentId>>>, id: AgentId) -> Self {
        Self {
            pending,
            id,
            committed: false,
        }
    }

    fn defuse(&mut self) {
        self.committed = true;
    }
}

impl Drop for PendingReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&self.id);
        }
    }
}

#[derive(Debug)]
pub struct Supervisor {
    handles: Arc<RwLock<HashMap<AgentId, AgentHandle>>>,
    /// Ids reserved by in-flight `spawn_with_readiness` calls. Pending agents
    /// are not visible via `get`/`list`/health; they only count toward
    /// duplicate and `max_agents` checks until they convert or release.
    pending: Arc<std::sync::Mutex<HashSet<AgentId>>>,
    lifecycle_rx: Arc<Mutex<mpsc::Receiver<LifecycleEvent>>>,
    lifecycle_tx: mpsc::Sender<LifecycleEvent>,
    bus: Option<Arc<InMemoryBus>>,
    max_agents: usize,
    /// Consecutive restart-dispatch failures, keyed by agent id. An entry
    /// exists only while an agent is stale *and* the supervisor cannot hand it
    /// a restart command, which keeps a single transient failure
    /// distinguishable from an agent that is permanently unreachable.
    restart_failures: Arc<RwLock<HashMap<AgentId, u32>>>,
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
            pending: Arc::new(std::sync::Mutex::new(HashSet::new())),
            lifecycle_rx: Arc::new(Mutex::new(lifecycle_rx)),
            lifecycle_tx,
            bus: None,
            max_agents: 100,
            restart_failures: Arc::new(RwLock::new(HashMap::new())),
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

        let last_heartbeat = Arc::new(AtomicU64::new(current_time_secs()));
        let restart_count = Arc::new(AtomicU64::new(0));

        let mut agent = Agent::new(spec.clone());
        let agent_id = spec.id.clone();
        let ltx = self.lifecycle_tx();

        let (handle, state_arc) = {
            let mut handles = self.handles.write().await;

            // Registered + pending agents jointly count for duplicate and
            // capacity checks, so a readiness-pending id cannot be shadowed
            // by a normal spawn racing it.
            let pending_count = self
                .pending
                .lock()
                .map(|pending| {
                    if pending.contains(&spec.id) {
                        usize::MAX
                    } else {
                        pending.len()
                    }
                })
                .map_err(|_| AgentError::Internal("pending lock poisoned".into()))?;
            if pending_count == usize::MAX {
                return Err(AgentError::AlreadyRunning(spec.id.clone()));
            }

            if handles.len() + pending_count >= self.max_agents {
                return Err(AgentError::Internal(format!(
                    "max agents ({}) reached",
                    self.max_agents
                )));
            }

            if handles.contains_key(&spec.id) {
                return Err(AgentError::AlreadyRunning(spec.id.clone()));
            }

            agent.start()?;

            let handle = AgentHandle::new(
                agent.clone(),
                cmd_tx,
                self.lifecycle_tx(),
                Arc::clone(&last_heartbeat),
                Arc::clone(&restart_count),
            );
            let state_arc = handle.state_arc();
            handles.insert(agent_id.clone(), handle.clone());

            (handle, state_arc)
        };

        let state = agent.state().clone();
        tokio::spawn(run_agent_loop_with_observers(
            agent,
            ltx,
            cmd_rx,
            Some(state_arc),
            last_heartbeat,
            restart_count,
        ));

        info!(agent_id = %agent_id, ?state, name = %spec.name, "agent spawned and running");
        Ok(handle)
    }

    /// Opt-in async readiness spawn.
    ///
    /// The id and one `max_agents` slot are reserved before awaiting readiness,
    /// so a pending spawn is invisible to `get`/`list`/health yet still blocks
    /// duplicates and consumes capacity. The reservation is RAII-owned: probe
    /// failure, timeout, `Agent::start` failure, early return, or future
    /// cancellation releases it exactly once with no async cleanup.
    ///
    /// On success the local agent is started, the reservation is atomically
    /// converted into the registered handle, and the normal agent loop is
    /// launched. Soft [`AgentCommand::Restart`](crate::AgentCommand) never
    /// reruns readiness; initial failure consumes zero restart budget and does
    /// not retry.
    ///
    /// A rejected spawn never emits a lifecycle event: the return value is the
    /// spawn contract, and lifecycle events only describe agents that became
    /// running handles (#108).
    pub async fn spawn_with_readiness<R>(
        &self,
        spec: AgentSpec,
        readiness: R,
    ) -> AgentResult<AgentHandle>
    where
        R: AgentReadiness,
    {
        let agent_id = spec.id.clone();
        let timeout_secs = spec.readiness_timeout_secs;

        // Admission: privately reserve id + capacity before awaiting readiness.
        {
            let handles = self.handles.write().await;
            let mut pending = self
                .pending
                .lock()
                .map_err(|_| AgentError::Internal("pending lock poisoned".into()))?;

            if handles.contains_key(&spec.id) || pending.contains(&spec.id) {
                return Err(AgentError::AlreadyRunning(spec.id.clone()));
            }
            if handles.len() + pending.len() >= self.max_agents {
                return Err(AgentError::Internal(format!(
                    "max agents ({}) reached",
                    self.max_agents
                )));
            }
            pending.insert(spec.id.clone());
        }

        let mut reservation = PendingReservation::new(Arc::clone(&self.pending), agent_id.clone());
        let mut agent = Agent::new(spec.clone());

        // Exactly one timeout owns "did not become ready in time".
        let readiness_result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            readiness.wait_until_ready(&spec),
        )
        .await;

        match readiness_result {
            Ok(Ok(())) => {}
            Ok(Err(reason)) => {
                // The local agent never became a handle; record the failure
                // locally only. No lifecycle event is emitted.
                agent.fail(&reason);
                return Err(AgentError::CommandFailed(reason));
            }
            Err(_) => {
                agent.fail(format!("readiness timed out after {timeout_secs}s"));
                return Err(AgentError::Timeout(agent_id));
            }
        }

        // Readiness succeeded: synchronous start of the local agent. Any
        // failure here drops `reservation` and frees id/capacity.
        agent.start()?;

        // Atomically convert the reservation into the registered handle. This
        // section holds the handles write lock and only touches synchronous
        // state, so there is no cancellation point between removing the
        // reservation and inserting the handle.
        let (handle, state_arc, cmd_rx, last_heartbeat, restart_count, ltx) = {
            let mut handles = self.handles.write().await;
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&agent_id);
            }
            if handles.contains_key(&agent_id) {
                // Unreachable: the reservation blocked duplicates. If it ever
                // fired, fail closed without emitting a lifecycle event.
                reservation.defuse();
                return Err(AgentError::AlreadyRunning(agent_id));
            }

            let (cmd_tx, cmd_rx) = mpsc::channel(32);
            let last_heartbeat = Arc::new(AtomicU64::new(current_time_secs()));
            let restart_count = Arc::new(AtomicU64::new(0));
            let ltx = self.lifecycle_tx();
            let handle = AgentHandle::new(
                agent.clone(),
                cmd_tx,
                self.lifecycle_tx(),
                Arc::clone(&last_heartbeat),
                Arc::clone(&restart_count),
            );
            let state_arc = handle.state_arc();
            handles.insert(agent_id.clone(), handle.clone());
            reservation.defuse();
            (
                handle,
                state_arc,
                cmd_rx,
                last_heartbeat,
                restart_count,
                ltx,
            )
        };

        let state = agent.state().clone();
        tokio::spawn(run_agent_loop_with_observers(
            agent,
            ltx,
            cmd_rx,
            Some(state_arc),
            last_heartbeat,
            restart_count,
        ));

        info!(agent_id = %agent_id, ?state, name = %spec.name, "agent spawned and running");
        Ok(handle)
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
        self.monitor_with_tick(SUPERVISOR_TICK).await;
    }

    pub(crate) async fn monitor_with_tick(&self, tick: Duration) {
        let mut interval = tokio::time::interval(tick);
        interval.tick().await;

        loop {
            interval.tick().await;
            self.supervise_once_at(current_time_secs()).await;
        }
    }

    /// One supervision pass: restart every agent whose heartbeat is stale.
    ///
    /// Dispatch is non-blocking by design. Restart commands go into each
    /// agent's bounded channel, and an agent that has stopped draining that
    /// channel would otherwise block this pass — and therefore every
    /// subsequent tick — for the remaining life of the process. Such an agent
    /// is logged and recorded instead, and the pass moves on.
    pub(crate) async fn supervise_once_at(&self, now: u64) {
        let stale = self.stale_handles_at(now).await;
        let mut failures = self.restart_failures.write().await;

        for handle in &stale {
            warn!(agent_id = %handle.id, "agent heartbeat timeout, restarting");

            match handle.try_restart() {
                Ok(()) => {
                    if failures.remove(&handle.id).is_some() {
                        info!(
                            agent_id = %handle.id,
                            "restart command accepted again after earlier failures"
                        );
                    }
                }
                Err(error) => {
                    let streak = failures.entry(handle.id.clone()).or_insert(0);
                    *streak += 1;
                    error!(
                        agent_id = %handle.id,
                        consecutive_failures = *streak,
                        %error,
                        "could not dispatch restart; this agent is unsupervised, other agents continue"
                    );

                    // Make the first failure of a streak visible to lifecycle
                    // watchers too, not only to log readers. try_send because
                    // the lifecycle channel is bounded as well.
                    if *streak == 1 {
                        let _ = handle.lifecycle_tx().try_send(LifecycleEvent::Degraded(
                            handle.id.clone(),
                            format!("restart could not be dispatched: {error}"),
                        ));
                    }
                }
            }
        }

        // Drop streaks for agents that recovered or are no longer stale (a
        // removed agent never reappears in `stale`), so the map stays bounded
        // by the number of currently unreachable agents.
        let stale_ids: HashSet<&AgentId> = stale.iter().map(|handle| &handle.id).collect();
        failures.retain(|id, _| stale_ids.contains(id));
    }

    /// Number of consecutive supervision passes that could not hand agent
    /// `id` a restart command. Zero means the last attempt was accepted, or
    /// that no restart has been attempted.
    pub async fn restart_failure_streak(&self, id: &str) -> u32 {
        self.restart_failures
            .read()
            .await
            .get(id)
            .copied()
            .unwrap_or(0)
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
            pending: Arc::clone(&self.pending),
            lifecycle_rx: Arc::clone(&self.lifecycle_rx),
            lifecycle_tx: self.lifecycle_tx.clone(),
            bus: self.bus.clone(),
            max_agents: self.max_agents,
            restart_failures: Arc::clone(&self.restart_failures),
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

    /// An agent that is stale almost immediately and may be restarted many
    /// times, so a test can drive repeated supervision passes.
    fn stale_spec(id: &str) -> AgentSpec {
        let mut spec = AgentSpec::new(id, id);
        spec.heartbeat_timeout_secs = 1;
        spec.max_restarts = u32::MAX;
        spec
    }

    /// Stops an agent loop from draining its command channel and then fills
    /// that channel. The loop blocks on the state mutex as soon as it touches
    /// agent state, so holding the returned guard keeps the agent wedged for
    /// as long as the test needs it.
    async fn wedge_agent(handle: &AgentHandle) -> tokio::sync::OwnedMutexGuard<AgentState> {
        let guard = handle.state_arc().lock_owned().await;

        for _ in 0..512 {
            match tokio::time::timeout(Duration::from_millis(50), handle.restart()).await {
                Ok(Ok(())) => continue,
                Ok(Err(error)) => panic!("wedged agent channel closed early: {error}"),
                Err(_) => return guard,
            }
        }

        panic!("command channel never filled up");
    }

    /// A single supervision pass must not be stalled by one agent that has
    /// stopped draining its bounded command channel: the other stale agent
    /// still gets restarted, and the unreachable one is recorded.
    #[tokio::test]
    async fn test_wedged_agent_does_not_stall_a_supervision_pass() {
        let sup = Supervisor::new();
        let wedged = sup.spawn(stale_spec("wedged-pass")).await.unwrap();
        let healthy = sup.spawn(stale_spec("healthy-pass")).await.unwrap();

        let _guard = wedge_agent(&wedged).await;

        let now = healthy.last_heartbeat() + 60;
        let pass = tokio::time::timeout(Duration::from_secs(2), sup.supervise_once_at(now)).await;
        assert!(
            pass.is_ok(),
            "one wedged agent must not stall a supervision pass"
        );

        wait_until_restart_count(&healthy, 1).await;
        assert!(
            sup.restart_failure_streak("wedged-pass").await >= 1,
            "an agent that cannot be restarted must be marked, not silently skipped"
        );
        assert_eq!(sup.restart_failure_streak("healthy-pass").await, 0);
    }

    /// The same failure driven through the real supervision loop: a wedged
    /// agent must not stop the loop from ticking, so a second stale agent
    /// keeps being health-checked and restarted.
    #[tokio::test]
    async fn test_wedged_agent_does_not_stop_the_supervision_loop() {
        let sup = Supervisor::new();
        let wedged = sup.spawn(stale_spec("wedged-loop")).await.unwrap();
        let healthy = sup.spawn(stale_spec("healthy-loop")).await.unwrap();

        let _guard = wedge_agent(&wedged).await;

        // heartbeat_timeout_secs is 1 and neither agent beats before 5s, so
        // both are stale once two wall-clock seconds have passed.
        tokio::time::sleep(Duration::from_millis(2_100)).await;

        let monitored = sup.clone();
        let monitor_task =
            tokio::spawn(
                async move { monitored.monitor_with_tick(Duration::from_millis(50)).await },
            );

        // A loop that ticks exactly once could still restart the healthy agent
        // once, depending on the order the stale set happens to iterate in.
        // Repeated restarts are only possible if the loop keeps ticking.
        let mut restarts = 0;
        for _ in 0..60 {
            restarts = healthy.restart_count();
            if restarts >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        monitor_task.abort();

        assert!(
            restarts >= 3,
            "supervision loop stopped ticking: healthy agent was restarted {restarts} time(s)"
        );
        assert!(
            sup.restart_failure_streak("wedged-loop").await >= 2,
            "repeated dispatch failures must accumulate so they are distinguishable from a transient one"
        );
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
